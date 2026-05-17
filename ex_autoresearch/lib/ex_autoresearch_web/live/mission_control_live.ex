defmodule ExAutoresearchWeb.MissionControlLive do
  @moduledoc """
  Mission Control — a Three.js-driven 3D control room for the deep research
  orchestrator. Subscribes to `"research:events"` and projects each event into
  a scene update pushed to the `MissionControl` JS hook.

  Server → client events are pushed via `push_event/3` with topics
  `mc:agent`, `mc:reactor`, `mc:scraper`, `mc:alert`, `mc:reset`. The hook owns
  the canvas state; the server only emits semantic patches.
  """
  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.{Audit, DeepResearch, Research}
  require Ash.Query

  @poll_interval_ms 2_000
  # Default reactor budget so the inner column has something to fill against
  # until the user reports a real budget.
  @default_token_budget 200_000
  # Cap so the in-memory narrative log stays small. Oldest entries fall off.
  @narrative_max 50
  # Inter-line delay when streaming the synthesized report into the narrative.
  @answer_stream_interval_ms 30
  @answer_stream_max_lines 30

  @impl true
  def mount(_params, _session, socket) do
    if connected?(socket) do
      Phoenix.PubSub.subscribe(ExAutoresearch.PubSub, "research:events")
    end

    {status, report_id} =
      try do
        %{status: s, report_id: rid} = DeepResearch.ResearchOrchestrator.status()
        {s, rid}
      rescue
        _ -> {:idle, nil}
      end

    socket =
      socket
      |> assign(:status, status)
      |> assign(:active_report_id, report_id)
      |> assign(:query, "")
      |> assign(:model, "")
      |> assign(:max_depth, 3)
      |> assign(:max_sources, 25)
      |> assign(:step_label, nil)
      |> assign(:focused, nil)
      |> assign(:token_budget, @default_token_budget)
      |> assign(:narrative, [])

    socket =
      if status != :idle and connected?(socket) do
        Process.send_after(self(), :poll_tokens, 0)
        socket
      else
        socket
      end

    {:ok, socket}
  end

  # ── Client → server intents ────────────────────────────────────────────

  @impl true
  def handle_event("set_query", %{"query" => q}, socket),
    do: {:noreply, assign(socket, :query, q)}

  def handle_event("set_model", %{"model" => m}, socket),
    do: {:noreply, assign(socket, :model, m)}

  def handle_event("set_depth", %{"depth" => d}, socket) do
    {:noreply, assign(socket, :max_depth, parse_int(d, socket.assigns.max_depth))}
  end

  def handle_event("set_sources", %{"sources" => s}, socket) do
    {:noreply, assign(socket, :max_sources, parse_int(s, socket.assigns.max_sources))}
  end

  def handle_event("intent:start_research", _params, socket) do
    query = String.trim(socket.assigns.query)
    org_id = socket.assigns[:current_org_id]

    cond do
      query == "" ->
        {:noreply, put_flash(socket, :error, "Enter a research question first.")}

      is_nil(org_id) ->
        {:noreply,
         put_flash(
           socket,
           :error,
           "Sign in first — research runs are scoped to an organization."
         )}

      true ->
        params = [
          query: query,
          title: String.slice(query, 0, 80),
          model: socket.assigns.model,
          max_depth: socket.assigns.max_depth,
          max_sources: socket.assigns.max_sources,
          organization_id: org_id
        ]

        :ok = DeepResearch.ResearchOrchestrator.start_research(params)

        Audit.record!(:research_started, %{
          organization_id: org_id,
          user_id: socket.assigns.current_user && socket.assigns.current_user.id,
          target_type: :report,
          summary: "[mission] " <> String.slice(query, 0, 192),
          metadata: %{from: "mission", model: socket.assigns.model}
        })

        socket =
          socket
          |> assign(:status, :researching)
          |> assign(:query, "")
          |> push_event("mc:reset", %{})

        schedule_poll(socket)
        {:noreply, socket}
    end
  end

  def handle_event("intent:stop_research", _params, socket) do
    :ok = DeepResearch.ResearchOrchestrator.stop_research()
    socket = push_event(socket, "mc:agent", %{status: "idle", step: "Stopped", progress: 0})
    {:noreply, assign(socket, :status, :idle)}
  end

  def handle_event("intent:focus_satellite", %{"url" => url} = payload, socket) do
    focused = build_focus(url, payload, socket.assigns.active_report_id)

    {:noreply,
     socket
     |> assign(:focused, focused)
     |> push_event("mc:focus", %{active: true})}
  end

  def handle_event("close_drawer", _params, socket) do
    {:noreply,
     socket
     |> assign(:focused, nil)
     |> push_event("mc:focus", %{active: false})}
  end

  # ── PubSub → scene patches ─────────────────────────────────────────────

  @impl true
  def handle_info({:status_changed, %{status: status} = payload}, socket) do
    socket =
      socket
      |> assign(:status, status)
      |> assign(:active_report_id, payload[:report_id] || socket.assigns.active_report_id)
      |> push_event("mc:agent", %{
        status: to_string(status),
        report_id: payload[:report_id]
      })

    if status not in [:idle, :completed, :failed], do: schedule_poll(socket)
    {:noreply, socket}
  end

  def handle_info({:research_step, step_info}, socket) do
    step_atom = step_info[:step] || ""
    label = step_label(step_atom, step_info)

    socket =
      socket
      |> assign(:step_label, label)
      |> push_event("mc:agent", %{
        status: to_string(socket.assigns.status),
        step: label,
        progress: progress_pct(step_atom, step_info)
      })

    {:noreply, socket}
  end

  def handle_info({:investigation_started, info}, socket) do
    label = "Searching: #{info[:query] || ""}"

    socket =
      socket
      |> assign(:step_label, label)
      |> push_event("mc:agent", %{step: label, status: to_string(socket.assigns.status)})
      |> push_narrative(%{
        kind: :subquery,
        text: info[:query] || "",
        investigation_id: info[:investigation_id]
      })

    {:noreply, socket}
  end

  def handle_info({:plan_generated, info}, socket) do
    queries = info[:queries] || []
    plan_text = queries |> Enum.map(&("• " <> &1)) |> Enum.join("\n")

    {:noreply,
     push_narrative(socket, %{
       kind: :plan,
       text: plan_text,
       report_id: info[:report_id]
     })}
  end

  def handle_info({:learning_added, info}, socket) do
    text =
      case {info[:findings_excerpt], info[:reasoning]} do
        {excerpt, _} when is_binary(excerpt) and excerpt != "" -> excerpt
        {_, reasoning} when is_binary(reasoning) -> reasoning
        _ -> info[:query] || ""
      end

    {:noreply,
     push_narrative(socket, %{
       kind: :learning,
       text: text,
       query: info[:query],
       quality_score: info[:quality_score],
       url: info[:url]
     })}
  end

  def handle_info({:scraper_progress, payload}, socket) do
    {:noreply,
     push_event(socket, "mc:scraper", %{
       url: payload[:url] || "",
       outcome: to_string(payload[:outcome] || "unknown"),
       duration_ms: payload[:duration_ms] || 0,
       fallback_used: payload[:fallback_used?] == true,
       message: payload[:message] || ""
     })}
  end

  def handle_info({:quality_alert, alert}, socket) do
    msg =
      case alert[:type] do
        :low_quality -> "Low quality results — pivoting strategy."
        :diminishing_returns -> "Diminishing returns detected."
        other -> "Quality alert: #{inspect(other)}"
      end

    {:noreply,
     push_event(socket, "mc:alert", %{
       type: to_string(alert[:type] || "alert"),
       message: msg
     })}
  end

  def handle_info({:report_completed, %{report_id: rid}}, socket) do
    socket =
      socket
      |> assign(:status, :completed)
      |> assign(:active_report_id, rid)
      |> push_event("mc:agent", %{status: "completed", step: "Report complete", report_id: rid})

    # One last token refresh after completion
    send(self(), :poll_tokens)

    # Kick off a fake-streamed answer preview into the narrative.
    case fetch_report_lines(rid, socket.assigns[:current_org_id]) do
      [] ->
        {:noreply, socket}

      [first | rest] ->
        if rest != [],
          do: Process.send_after(self(), {:answer_line, rest}, @answer_stream_interval_ms)

        {:noreply, push_narrative(socket, %{kind: :answer, text: first, report_id: rid})}
    end
  end

  def handle_info({:answer_line, []}, socket), do: {:noreply, socket}

  def handle_info({:answer_line, [line | rest]}, socket) do
    if rest != [],
      do: Process.send_after(self(), {:answer_line, rest}, @answer_stream_interval_ms)

    {:noreply, push_narrative(socket, %{kind: :answer, text: line})}
  end

  def handle_info(:poll_tokens, socket) do
    socket = refresh_reactor(socket)
    schedule_poll(socket)
    {:noreply, socket}
  end

  def handle_info(_, socket), do: {:noreply, socket}

  # ── Helpers ────────────────────────────────────────────────────────────

  defp refresh_reactor(%{assigns: %{active_report_id: rid}} = socket) when is_binary(rid) do
    try do
      case Ash.get(Research.Report, rid) do
        {:ok, report} ->
          push_event(socket, "mc:reactor", %{
            input_tokens: report.total_input_tokens || 0,
            output_tokens: report.total_output_tokens || 0,
            calls: report.llm_calls_count || 0,
            budget: @default_token_budget
          })

        _ ->
          socket
      end
    rescue
      _ -> socket
    end
  end

  defp refresh_reactor(socket), do: socket

  defp build_focus(url, payload, rid) do
    base = %{
      url: url,
      outcome: payload["outcome"] || "unknown",
      investigation: lookup_investigation(url, rid)
    }

    base
  end

  defp lookup_investigation(_url, nil), do: nil

  defp lookup_investigation(url, rid) do
    try do
      Research.Investigation
      |> Ash.Query.filter(url == ^url and report_id == ^rid)
      |> Ash.Query.sort(inserted_at: :desc)
      |> Ash.Query.limit(1)
      |> Ash.read!()
      |> List.first()
    rescue
      _ -> nil
    end
  end

  defp schedule_poll(socket) do
    if socket.assigns.status not in [:idle, :completed, :failed] and connected?(socket) do
      Process.send_after(self(), :poll_tokens, @poll_interval_ms)
    end

    :ok
  end

  defp parse_int(value, default) when is_binary(value) do
    case Integer.parse(value) do
      {n, _} -> n
      :error -> default
    end
  end

  defp parse_int(_, default), do: default

  defp step_label(step, info) do
    case step do
      "planning" -> "Generating search plan..."
      "searching" -> "Searching: #{info[:count] || "?"} queries..."
      "analyzing" -> "Analyzing findings..."
      "deep_dive" -> "Deep diving: #{info[:count] || "?"} more queries..."
      "writing" -> "Writing report..."
      "completed" -> "Research complete!"
      "" -> nil
      other -> String.capitalize(to_string(other)) <> "..."
    end
  end

  defp progress_pct(_, info), do: info[:progress] || 0

  defp outcome_badge_class("primary_success"),
    do: "bg-emerald-500/20 border border-emerald-400/40 text-emerald-200"

  defp outcome_badge_class("fallback_success"),
    do: "bg-amber-500/20 border border-amber-400/40 text-amber-200"

  defp outcome_badge_class("both_failed"),
    do: "bg-red-500/20 border border-red-500/40 text-red-200"

  defp outcome_badge_class("native_failed"),
    do: "bg-red-500/20 border border-red-500/40 text-red-200"

  defp outcome_badge_class(_), do: "bg-white/5 border border-white/15 text-white/70"

  defp scraper_source_chip_class(:crawl4ai),
    do: "bg-cyan-500/15 border border-cyan-400/30 text-cyan-200"

  defp scraper_source_chip_class(:native),
    do: "bg-emerald-500/15 border border-emerald-400/30 text-emerald-200"

  defp scraper_source_chip_class(:unknown),
    do: "bg-white/5 border border-white/15 text-white/70"

  defp scraper_source_chip_class(_),
    do: "bg-white/5 border border-white/15 text-white/70"

  defp push_narrative(socket, entry) do
    entry = Map.put_new(entry, :ts, System.system_time(:second))
    narrative = [entry | socket.assigns.narrative] |> Enum.take(@narrative_max)
    assign(socket, :narrative, narrative)
  end

  defp fetch_report_lines(rid, tenant) when is_binary(rid) do
    opts = if tenant, do: [tenant: tenant], else: []

    try do
      case Ash.get(Research.Report, rid, opts) do
        {:ok, %{markdown_body: body}} when is_binary(body) and body != "" ->
          body
          |> String.split("\n", trim: false)
          |> Enum.map(&String.trim_trailing/1)
          |> Enum.filter(&(String.trim(&1) != ""))
          |> Enum.take(@answer_stream_max_lines)

        _ ->
          []
      end
    rescue
      _ -> []
    end
  end

  defp fetch_report_lines(_, _), do: []

  defp narrative_li_class(:plan),
    do: "rounded-md bg-cyan-500/5 border border-cyan-400/20 px-3 py-2"

  defp narrative_li_class(:subquery),
    do: "rounded-md bg-white/5 border border-white/10 px-3 py-1.5"

  defp narrative_li_class(:learning),
    do: "rounded-md bg-emerald-500/5 border border-emerald-400/20 px-3 py-2"

  defp narrative_li_class(:answer),
    do: "rounded-md bg-amber-500/5 border border-amber-400/20 px-3 py-1.5"

  defp narrative_li_class(_),
    do: "rounded-md bg-white/5 border border-white/10 px-3 py-2"

  defp narrative_kind_label(:plan), do: "Plan"
  defp narrative_kind_label(:subquery), do: "Search"
  defp narrative_kind_label(:learning), do: "Learn"
  defp narrative_kind_label(:answer), do: "Answer"
  defp narrative_kind_label(other), do: to_string(other)

  defp narrative_badge_class(:plan),
    do: "bg-cyan-500/20 border border-cyan-400/40 text-cyan-200"

  defp narrative_badge_class(:subquery),
    do: "bg-white/5 border border-white/15 text-white/60"

  defp narrative_badge_class(:learning),
    do: "bg-emerald-500/20 border border-emerald-400/40 text-emerald-200"

  defp narrative_badge_class(:answer),
    do: "bg-amber-500/20 border border-amber-400/40 text-amber-100"

  defp narrative_badge_class(_),
    do: "bg-white/5 border border-white/15 text-white/70"

  defp format_quality(score) when is_number(score),
    do: score |> :erlang.float() |> Float.round(2) |> :erlang.float_to_binary(decimals: 2)

  defp format_quality(_), do: ""

  # ── Render ─────────────────────────────────────────────────────────────

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app
      flash={@flash}
      current_user={assigns[:current_user]}
      active_nav={:mission}
      container="max-w-none"
    >
      <div
        class="flex flex-col xl:flex-row gap-3"
        style="height: calc(100vh - 9rem); min-height: 480px;"
      >
        <div
          id="mission-shell"
          class="relative flex-1 min-h-[320px] overflow-hidden rounded-2xl border border-base-300 bg-[#05070d] shadow-xl"
        >
          <%!-- 3D canvas — JS hook owns this subtree --%>
          <div
            id="mission-canvas"
            phx-hook="MissionControl"
            phx-update="ignore"
            class="absolute inset-0"
          >
          </div>

          <%!-- Top-left status panel --%>
          <div class="pointer-events-none absolute top-4 left-4 z-10 flex flex-col gap-2">
            <div class="rounded-lg border border-white/10 bg-black/50 backdrop-blur-md px-4 py-3 text-white shadow-lg w-72">
              <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">Agent Core</div>
              <div class="mt-1 flex items-baseline gap-2">
                <span data-mc-status class="text-lg font-semibold tabular-nums tracking-tight">
                  {String.upcase(to_string(@status))}
                </span>
                <span class="h-2 w-2 rounded-full bg-cyan-400 animate-pulse"></span>
              </div>
              <div data-mc-step class="mt-1 text-xs text-white/70 min-h-[1rem]">
                {@step_label || ""}
              </div>
            </div>

            <div class="rounded-lg border border-white/10 bg-black/50 backdrop-blur-md px-4 py-3 text-white shadow-lg w-72">
              <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">Token Reactor</div>
              <div data-mc-tokens class="mt-1 text-xs font-mono text-orange-200/90">
                0 / {Integer.to_string(@token_budget)} tokens · 0 calls
              </div>
            </div>

            <div class="rounded-lg border border-white/10 bg-black/50 backdrop-blur-md px-4 py-3 text-white shadow-lg w-72">
              <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">
                Investigation Swarm
              </div>
              <div class="mt-1 text-xs text-white/70">
                <span data-mc-sat-count class="text-base font-semibold text-white tabular-nums">
                  0
                </span>
                <span class="opacity-60"> active satellites</span>
              </div>
            </div>
          </div>

          <%!-- Top-right alerts --%>
          <div class="pointer-events-none absolute top-4 right-4 z-10 w-80">
            <div data-mc-alerts class="space-y-1"></div>
          </div>

          <%!-- Investigation drawer (right side, slides in when @focused) --%>
          <%= if @focused do %>
            <aside
              id="mc-drawer"
              class="absolute top-0 right-0 bottom-0 z-20 w-[26rem] max-w-[90vw] border-l border-white/10 bg-black/85 backdrop-blur-md text-white shadow-2xl overflow-y-auto"
            >
              <div class="sticky top-0 z-10 flex items-center justify-between border-b border-white/10 bg-black/70 px-4 py-3">
                <div class="min-w-0">
                  <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">
                    Investigation
                  </div>
                  <div class="mt-0.5 text-sm font-medium truncate" title={@focused.url}>
                    {@focused.url}
                  </div>
                </div>
                <button
                  type="button"
                  phx-click="close_drawer"
                  class="ml-3 shrink-0 rounded-md border border-white/15 bg-white/5 px-2 py-1 text-xs text-white/70 hover:bg-white/10"
                  aria-label="Close drawer"
                >
                  ✕
                </button>
              </div>

              <div class="space-y-4 px-4 py-4 text-xs">
                <div class="flex flex-wrap items-center gap-2">
                  <span class={[
                    "rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider",
                    outcome_badge_class(@focused.outcome)
                  ]}>
                    {@focused.outcome}
                  </span>
                  <%= if inv = @focused.investigation do %>
                    <span class={[
                      "rounded-full px-2 py-0.5 text-[10px] uppercase tracking-wider",
                      scraper_source_chip_class(inv.scraper_source)
                    ]}>
                      {inv.scraper_source}
                    </span>
                    <%= if inv.fallback_used do %>
                      <span class="rounded-full bg-amber-500/20 border border-amber-400/40 px-2 py-0.5 text-[10px] uppercase tracking-wider text-amber-200">
                        fallback
                      </span>
                    <% end %>
                  <% end %>
                </div>

                <%= if inv = @focused.investigation do %>
                  <%= if inv.query do %>
                    <section>
                      <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">Query</div>
                      <div class="mt-1 text-white/90">{inv.query}</div>
                    </section>
                  <% end %>

                  <%= if inv.reasoning do %>
                    <section>
                      <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">
                        Why this URL
                      </div>
                      <div class="mt-1 text-white/80 whitespace-pre-wrap">{inv.reasoning}</div>
                    </section>
                  <% end %>

                  <div class="grid grid-cols-2 gap-3">
                    <%= if inv.quality_score do %>
                      <div>
                        <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">
                          Quality
                        </div>
                        <div class="mt-1 font-mono tabular-nums text-white/90">
                          {Float.round(inv.quality_score, 2)}
                        </div>
                      </div>
                    <% end %>
                    <%= if (inv.content_length || 0) > 0 do %>
                      <div>
                        <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">
                          Content
                        </div>
                        <div class="mt-1 font-mono tabular-nums text-white/90">
                          {inv.content_length} chars
                        </div>
                      </div>
                    <% end %>
                    <%= if inv.fetched_at do %>
                      <div class="col-span-2">
                        <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">
                          Fetched
                        </div>
                        <div class="mt-1 font-mono text-[11px] text-white/80">
                          {Calendar.strftime(inv.fetched_at, "%Y-%m-%d %H:%M:%S UTC")}
                        </div>
                      </div>
                    <% end %>
                  </div>

                  <%= if inv.findings do %>
                    <section>
                      <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">
                        Findings excerpt
                      </div>
                      <div class="mt-1 max-h-64 overflow-y-auto rounded-md bg-white/5 border border-white/10 p-2 text-[11px] leading-relaxed text-white/85 whitespace-pre-wrap">
                        {String.slice(inv.findings, 0, 1200)}{if String.length(inv.findings) > 1200,
                          do: "…",
                          else: ""}
                      </div>
                    </section>
                  <% end %>

                  <%= if inv.error do %>
                    <section>
                      <div class="text-[10px] uppercase tracking-[0.2em] text-red-300/80">Error</div>
                      <div class="mt-1 rounded-md bg-red-500/10 border border-red-500/30 p-2 font-mono text-[11px] text-red-200 whitespace-pre-wrap">
                        {inv.error}
                      </div>
                    </section>
                  <% end %>
                <% else %>
                  <div class="rounded-md bg-white/5 border border-white/10 p-2 text-[11px] text-white/60">
                    No persisted investigation record yet — this fetch may still be in flight or was emitted by telemetry before storage.
                  </div>
                <% end %>

                <div class="pt-2">
                  <a
                    href={@focused.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    class="inline-flex items-center gap-1.5 rounded-md bg-cyan-500/20 border border-cyan-400/40 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-cyan-200 hover:bg-cyan-500/30 transition"
                  >
                    Open URL ↗
                  </a>
                </div>
              </div>
            </aside>
          <% end %>

          <%!-- Bottom control bar --%>
          <div class="absolute bottom-4 inset-x-4 z-10">
            <form
              phx-submit="intent:start_research"
              id="mission-control-form"
              class="flex flex-wrap items-end gap-3 rounded-xl border border-white/10 bg-black/60 backdrop-blur-md px-4 py-3 shadow-2xl"
            >
              <div class="flex-1 min-w-[18rem]">
                <label
                  for="mc-query"
                  class="block text-[10px] uppercase tracking-[0.2em] text-white/40 mb-1"
                >
                  Research Question
                </label>
                <input
                  id="mc-query"
                  type="text"
                  name="query"
                  value={@query}
                  phx-change="set_query"
                  phx-debounce="200"
                  placeholder="Ask anything — the agent will plan, search, scrape, and synthesize."
                  class="w-full rounded-lg bg-white/5 border border-white/10 px-3 py-2 text-sm text-white placeholder:text-white/30 focus:outline-none focus:ring-2 focus:ring-cyan-400/40 focus:border-cyan-400/40"
                />
              </div>

              <div class="w-44">
                <label
                  for="mc-model"
                  class="block text-[10px] uppercase tracking-[0.2em] text-white/40 mb-1"
                >
                  LLM
                </label>
                <select
                  id="mc-model"
                  name="model"
                  phx-change="set_model"
                  class="w-full rounded-lg bg-white/5 border border-white/10 px-2 py-2 text-xs text-white focus:outline-none focus:ring-2 focus:ring-cyan-400/40"
                >
                  <option value="" selected={@model in [nil, ""]}>Auto</option>
                  <option value="claude-sonnet-4" selected={@model == "claude-sonnet-4"}>
                    Claude Sonnet 4
                  </option>
                  <option value="claude-opus-4-6" selected={@model == "claude-opus-4-6"}>
                    Claude Opus 4.6
                  </option>
                  <option value="gpt-4.1" selected={@model == "gpt-4.1"}>GPT-4.1</option>
                  <option value="gemini-2.5-pro" selected={@model == "gemini-2.5-pro"}>
                    Gemini 2.5 Pro
                  </option>
                </select>
              </div>

              <div class="w-20">
                <label
                  for="mc-depth"
                  class="block text-[10px] uppercase tracking-[0.2em] text-white/40 mb-1"
                >
                  Depth
                </label>
                <input
                  id="mc-depth"
                  type="number"
                  name="depth"
                  min="1"
                  max="10"
                  value={@max_depth}
                  phx-change="set_depth"
                  class="w-full rounded-lg bg-white/5 border border-white/10 px-2 py-2 text-xs text-white text-center tabular-nums focus:outline-none focus:ring-2 focus:ring-cyan-400/40"
                />
              </div>

              <div class="w-24">
                <label
                  for="mc-sources"
                  class="block text-[10px] uppercase tracking-[0.2em] text-white/40 mb-1"
                >
                  Sources
                </label>
                <input
                  id="mc-sources"
                  type="number"
                  name="sources"
                  min="5"
                  max="100"
                  value={@max_sources}
                  phx-change="set_sources"
                  class="w-full rounded-lg bg-white/5 border border-white/10 px-2 py-2 text-xs text-white text-center tabular-nums focus:outline-none focus:ring-2 focus:ring-cyan-400/40"
                />
              </div>

              <div class="flex items-end gap-2">
                <button
                  type="submit"
                  id="mc-start"
                  disabled={@status not in [:idle, :completed, :failed]}
                  class={[
                    "rounded-lg px-4 py-2 text-xs font-semibold uppercase tracking-wider",
                    "bg-gradient-to-r from-cyan-500 to-blue-600 text-white shadow-lg shadow-cyan-500/30",
                    "hover:from-cyan-400 hover:to-blue-500 transition disabled:opacity-40 disabled:cursor-not-allowed"
                  ]}
                >
                  Initiate
                </button>
                <button
                  type="button"
                  id="mc-stop"
                  phx-click="intent:stop_research"
                  disabled={@status in [:idle, :completed, :failed]}
                  class={[
                    "rounded-lg px-4 py-2 text-xs font-semibold uppercase tracking-wider",
                    "bg-red-500/20 border border-red-500/40 text-red-200 hover:bg-red-500/30 transition",
                    "disabled:opacity-30 disabled:cursor-not-allowed"
                  ]}
                >
                  Abort
                </button>
              </div>
            </form>
          </div>
        </div>

        <%!-- Right-side Research Narrative panel — text-stream of plan/sub-queries/learnings/answer --%>
        <aside
          id="narrative-panel"
          class="w-full xl:w-96 shrink-0 overflow-y-auto rounded-2xl border border-base-300 bg-[#06080f] text-white shadow-xl"
          aria-label="Research Narrative"
        >
          <div class="sticky top-0 z-10 border-b border-white/10 bg-black/70 backdrop-blur-md px-4 py-3">
            <div class="text-[10px] uppercase tracking-[0.2em] text-white/40">Research Narrative</div>
            <div class="mt-0.5 text-[11px] text-white/55">
              Plan · sub-queries · learnings · answer
            </div>
          </div>

          <ol
            id="narrative-list"
            data-mc-narrative
            class="flex flex-col gap-2 px-3 py-3 text-[12px]"
          >
            <%= for entry <- Enum.reverse(@narrative) do %>
              <li
                data-narrative-kind={Atom.to_string(entry.kind)}
                class={narrative_li_class(entry.kind)}
              >
                <div class="flex items-start gap-2">
                  <span class={[
                    "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider",
                    narrative_badge_class(entry.kind)
                  ]}>
                    {narrative_kind_label(entry.kind)}
                  </span>
                  <div class="min-w-0 flex-1">
                    <pre class="whitespace-pre-wrap break-words font-sans text-[11px] leading-relaxed text-white/85 m-0">{entry.text}</pre>
                    <%= if entry.kind == :learning and is_number(entry[:quality_score]) do %>
                      <div class="mt-1 text-[10px] font-mono tabular-nums text-emerald-300/80">
                        quality {format_quality(entry[:quality_score])}
                      </div>
                    <% end %>
                  </div>
                </div>
              </li>
            <% end %>
            <%= if @narrative == [] do %>
              <li class="rounded-md border border-dashed border-white/10 px-3 py-3 text-[11px] text-white/40">
                Waiting for the agent to plan…
              </li>
            <% end %>
          </ol>
        </aside>
      </div>
    </Layouts.app>
    """
  end
end
