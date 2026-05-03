defmodule ExAutoresearchWeb.DashboardLive do
  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.{Research, DeepResearch, Audit}
  require Ash.Query

  @impl true
  def mount(_params, _session, socket) do
    if connected?(socket),
      do: Phoenix.PubSub.subscribe(ExAutoresearch.PubSub, "research:events")

    # If user is authenticated (via plug), filter by org; else show all
    org_id = socket.assigns[:current_org_id]

    reports_query =
      Research.Report
      |> Ash.Query.sort(inserted_at: :desc)

    reports =
      case org_id do
        nil -> []
        id -> Ash.read!(reports_query, tenant: id) |> Enum.map(&format_report_summary/1)
      end

    socket =
      socket
      |> assign(:reports, reports)
      |> assign(:query, "")
      |> assign(:title, "")
      |> assign(:max_depth, 3)
      |> assign(:max_sources, 25)
      |> assign(:model, "")
      |> assign(:active_report, nil)
      |> assign(:research_step, nil)
      |> assign(:progress, 0)
      |> assign(:status, :idle)
      |> assign(:selected_template_id, nil)
      |> assign(:scraper_status, nil)

    {:ok, socket}
  end

  @impl true
  def handle_params(params, _uri, socket) do
    action = Map.get(params, "action")

    case action do
      "index" -> handle_index(params, socket)
      "logout" -> handle_logout(params, socket)
      _ -> {:noreply, socket}
    end
  end

  defp handle_logout(_params, socket) do
    {:noreply, push_navigate(socket, to: "/logout")}
  end

  defp handle_index(_params, socket) do
    {:noreply, socket}
  end

  @impl true
  def handle_event("set_query", %{"query" => q}, socket),
    do: {:noreply, assign(socket, :query, q)}

  def handle_event("set_title", %{"title" => t}, socket),
    do: {:noreply, assign(socket, :title, t)}

  def handle_event("set_model", %{"model" => m}, socket),
    do: {:noreply, assign(socket, :model, m)}

  def handle_event("set_depth", %{"depth" => d}, socket),
    do: {:noreply, assign(socket, :max_depth, String.to_integer(d))}

  def handle_event("set_sources", %{"sources" => s}, socket),
    do: {:noreply, assign(socket, :max_sources, String.to_integer(s))}

  def handle_event("start_research", _params, socket) do
    query = String.trim(socket.assigns.query)

    if query != "" do
      title =
        if socket.assigns.title != "",
          do: socket.assigns.title,
          else: String.slice(query, 0, 80)

      org_id = socket.assigns[:current_org_id]

      research_params = [
        query: query,
        title: title,
        model: socket.assigns.model,
        max_depth: socket.assigns.max_depth,
        max_sources: socket.assigns.max_sources
      ]

      research_params =
        if org_id,
          do: Keyword.put(research_params, :organization_id, org_id),
          else: research_params

      DeepResearch.ResearchOrchestrator.start_research(research_params)

      if org_id do
        Audit.record!(:research_started, %{
          organization_id: org_id,
          user_id: socket.assigns.current_user && socket.assigns.current_user.id,
          target_type: :report,
          summary: String.slice(query, 0, 200),
          metadata: %{model: socket.assigns.model, max_depth: socket.assigns.max_depth}
        })
      end

      {:noreply,
       socket
       |> assign(:query, "")
       |> assign(:status, :researching)}
    else
      {:noreply, put_flash(socket, :error, "Please enter a research query")}
    end
  end

  def handle_event("view_report", %{"id" => id}, socket) do
    case DeepResearch.ResearchOrchestrator.report_detail(id) do
      {:ok, report} ->
        {:noreply, assign(socket, :active_report, report)}

      _ ->
        {:noreply, put_flash(socket, :error, "Report not found")}
    end
  end

  def handle_event("back_to_list", _params, socket),
    do: {:noreply, assign(socket, :active_report, nil)}

  def handle_event("export_report", %{"id" => id}, socket) do
    result = export_report(id, socket)

    case result do
      {:noreply, %{assigns: %{flash: %{"info" => _}}}} ->
        if org_id = socket.assigns[:current_org_id] do
          Audit.record!(:report_exported, %{
            organization_id: org_id,
            user_id: socket.assigns.current_user && socket.assigns.current_user.id,
            target_type: :report,
            target_id: id
          })
        end

        result

      _ ->
        result
    end
  end

  @impl true
  def handle_info({:status_changed, %{status: status}}, socket),
    do: {:noreply, assign(socket, :status, status)}

  def handle_info({:research_step, step_info}, socket) do
    step =
      case step_info[:step] do
        "planning" -> "Generating search plan..."
        "searching" -> "Searching: #{step_info[:count] || "?"} queries..."
        "analyzing" -> "Analyzing findings..."
        "deep_dive" -> "Deep diving: #{step_info[:count] || "?"} more queries..."
        "writing" -> "Writing report..."
        "completed" -> "Research complete!"
        _ -> "Processing..."
      end

    {:noreply,
     socket
     |> assign(:research_step, step)
     |> assign(:progress, (step_info[:progress] || 0) * 100)}
  end

  def handle_info({:investigation_started, info}, socket) do
    {:noreply, assign(socket, :research_step, "Searching: #{info[:query]}")}
  end

  def handle_info({:quality_alert, alert}, socket) do
    msg =
      case alert[:type] do
        :low_quality -> "Low quality results, pivoting strategy..."
        :diminishing_returns -> "Diminishing returns detected..."
        _ -> "Quality alert"
      end

    {:noreply,
     socket
     |> assign(:research_step, msg)
     |> put_flash(:info, msg)}
  end

  def handle_info({:report_completed, %{report_id: report_id}}, socket) do
    # Refresh reports list
    {:noreply,
     socket
     |> put_flash(:info, "A research report has completed!")
     |> push_navigate(to: ~p"/?report_id=#{report_id}")}
  end

  def handle_info({:scraper_progress, payload}, socket) do
    {:noreply, assign(socket, :scraper_status, payload)}
  end

  def handle_info(_, socket), do: {:noreply, socket}

  # --- Rendering ---

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app
      flash={@flash}
      current_user={assigns[:current_user]}
      active_nav={:dashboard}
      container="max-w-5xl"
    >
      <div class="space-y-6">
        <%= if @active_report do %>
          <.report_detail report={@active_report} />
        <% else %>
          <div>
            <h1 class="text-2xl font-bold tracking-tight">Deep Research</h1>

            <p class="text-sm text-base-content/60 mt-1">
              Ask a question. We'll plan, search, scrape, analyze and write a sourced report.
            </p>
          </div>

          <.search_form
            query={@query}
            title={@title}
            model={@model}
            max_depth={@max_depth}
            max_sources={@max_sources}
          />
          <%= if @status != :idle do %>
            <.progress_bar
              step={@research_step}
              progress={@progress}
              status={@status}
              scraper_status={@scraper_status}
            />
          <% end %>
          <.report_list reports={@reports} />
        <% end %>
      </div>
    </Layouts.app>
    """
  end

  defp search_form(assigns) do
    ~H"""
    <section class="card bg-base-100 border border-base-300 shadow-sm">
      <form phx-submit="start_research" class="card-body space-y-5">
        <div class="form-control">
          <label class="label" for="dash-query">
            <span class="label-text font-medium">Research Question</span>
          </label>
          <input
            id="dash-query"
            type="text"
            name="query"
            value={@query}
            phx-change="set_query"
            phx-debounce="300"
            placeholder="e.g., What are the latest competitor moves in our market?"
            class="input input-bordered w-full"
          />
        </div>

        <div class="form-control">
          <label class="label" for="dash-title">
            <span class="label-text font-medium">Report Title</span>
            <span class="label-text-alt text-base-content/50">optional</span>
          </label>
          <input
            id="dash-title"
            type="text"
            name="title"
            value={@title}
            phx-change="set_title"
            phx-debounce="300"
            placeholder="Auto-generated from query"
            class="input input-bordered w-full"
          />
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
          <div class="form-control">
            <label class="label" for="dash-model">
              <span class="label-text font-medium">LLM Model</span>
            </label>
            <select
              id="dash-model"
              name="model"
              phx-change="set_model"
              class="select select-bordered w-full"
            >
              <option value="" selected={@model in [nil, ""]}>OpenAI Gateway · Auto tier</option>

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

          <div class="form-control">
            <label class="label" for="dash-depth">
              <span class="label-text font-medium">Max Depth</span>
              <span class="label-text-alt text-base-content/50">1–10</span>
            </label>
            <input
              id="dash-depth"
              type="number"
              name="depth"
              value={@max_depth}
              phx-change="set_depth"
              min="1"
              max="10"
              class="input input-bordered w-full"
            />
          </div>

          <div class="form-control">
            <label class="label" for="dash-sources">
              <span class="label-text font-medium">Max Sources</span>
              <span class="label-text-alt text-base-content/50">5–100</span>
            </label>
            <input
              id="dash-sources"
              type="number"
              name="sources"
              value={@max_sources}
              phx-change="set_sources"
              min="5"
              max="100"
              class="input input-bordered w-full"
            />
          </div>
        </div>

        <div class="flex justify-end">
          <button type="submit" class="btn btn-primary">Start Deep Research</button>
        </div>
      </form>
    </section>
    """
  end

  defp progress_bar(assigns) do
    ~H"""
    <section
      id="research-progress"
      class="card bg-base-100 border border-base-300 shadow-sm"
      role="status"
      aria-live="polite"
    >
      <div class="card-body py-4 px-5 space-y-3">
        <div class="flex items-center gap-3">
          <span class="loading loading-spinner loading-sm text-primary" aria-hidden="true"></span>
          <span class="text-sm font-medium">{@step || "Researching..."}</span>
          <span class="ml-auto text-xs font-mono text-base-content/60 tabular-nums">
            {Float.round(@progress / 1, 1)}%
          </span>
        </div>
        <progress class="progress progress-primary w-full" value={@progress} max="100"></progress>
        <%= if @scraper_status do %>
          <.scraper_status_row status={@scraper_status} />
        <% end %>
      </div>
    </section>
    """
  end

  defp scraper_status_row(assigns) do
    ~H"""
    <div
      id={"scraper-status-#{@status.report_id}"}
      class={["alert text-xs py-2 px-3", scraper_alert_class(@status.outcome)]}
    >
      <span class="font-mono shrink-0">{scraper_outcome_icon(@status.outcome)}</span>
      <div class="min-w-0 flex-1">
        <div class="font-medium truncate">{@status.url}</div>

        <div class="opacity-70 mt-0.5">{@status.message}</div>
      </div>
      <span class="opacity-60 shrink-0 tabular-nums font-mono">{@status.duration_ms}ms</span>
    </div>
    """
  end

  defp report_list(assigns) do
    ~H"""
    <section>
      <h2 class="text-sm font-semibold uppercase tracking-wider text-base-content/60 mb-3">
        Recent Reports
      </h2>

      <%= if @reports == [] do %>
        <div class="card bg-base-100 border border-dashed border-base-300">
          <div class="card-body items-center text-center py-12">
            <p class="text-sm text-base-content/60">No reports yet.</p>

            <p class="text-xs text-base-content/50 mt-1">
              Start a research query above to generate one.
            </p>
          </div>
        </div>
      <% else %>
        <ul class="space-y-2">
          <%= for report <- @reports do %>
            <li>
              <button
                phx-click="view_report"
                phx-value-id={report.id}
                class="w-full text-left card bg-base-100 border border-base-300 shadow-sm hover:shadow-md hover:border-primary/40 transition-all"
              >
                <div class="card-body py-4 px-5 flex flex-row items-start justify-between gap-4">
                  <div class="min-w-0 flex-1">
                    <h3 class="font-medium truncate">{report.title}</h3>

                    <p class="text-sm text-base-content/70 mt-1 line-clamp-1">
                      {String.slice(report.query, 0, 200)}
                    </p>
                  </div>

                  <div class="text-right shrink-0 space-y-1">
                    <span class={["badge badge-sm", status_badge_class(report.status)]}>
                      {status_label(report.status)}
                    </span>
                    <p class="text-xs text-base-content/50">{report.source_count || 0} sources</p>
                  </div>
                </div>
              </button>
            </li>
          <% end %>
        </ul>
      <% end %>
    </section>
    """
  end

  defp report_detail(assigns) do
    ~H"""
    <article class="card bg-base-100 border border-base-300 shadow-sm overflow-hidden">
      <header class="border-b border-base-300 px-6 py-4 flex items-center justify-between gap-4">
        <div class="min-w-0">
          <h2 class="text-lg font-semibold truncate">{@report.title}</h2>

          <p class="text-sm text-base-content/70 mt-1 line-clamp-1">{@report.query}</p>
        </div>

        <div class="flex items-center gap-2 shrink-0">
          <%= if @report.status == :completed do %>
            <button
              phx-click="export_report"
              phx-value-id={@report.id}
              class="btn btn-soft btn-sm"
            >
              Export
            </button>
          <% end %>

          <button phx-click="back_to_list" class="btn btn-ghost btn-sm gap-1">
            <span aria-hidden="true">&larr;</span> Back
          </button>
        </div>
      </header>

      <div class="px-6 py-6">
        <%= if @report.status == :completed and @report.markdown_body do %>
          <div class="text-sm leading-relaxed max-w-none [&_h1]:text-2xl [&_h1]:font-bold [&_h1]:mt-6 [&_h1]:mb-3 [&_h2]:text-xl [&_h2]:font-semibold [&_h2]:mt-5 [&_h2]:mb-2 [&_h3]:text-base [&_h3]:font-semibold [&_h3]:mt-4 [&_h3]:mb-1 [&_p]:my-2 [&_ul]:list-disc [&_ul]:pl-6 [&_ul]:my-2 [&_ol]:list-decimal [&_ol]:pl-6 [&_ol]:my-2 [&_li]:my-1 [&_a]:link [&_a]:link-primary [&_code]:text-xs [&_code]:bg-base-200 [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_pre]:bg-base-200 [&_pre]:p-3 [&_pre]:rounded-md [&_pre]:overflow-x-auto [&_pre]:text-xs [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_blockquote]:border-l-4 [&_blockquote]:border-base-300 [&_blockquote]:pl-4 [&_blockquote]:italic [&_blockquote]:text-base-content/70 [&_strong]:font-semibold">
            {format_markdown(@report.markdown_body)}
          </div>
        <% else %>
          <div class="text-center py-12 text-base-content/60">
            <span class="loading loading-spinner loading-md text-primary mb-3"></span>
            <p class="text-sm">
              {case @report.status do
                :pending ->
                  "Waiting to start..."

                :researching ->
                  "Researching... (#{Float.round((@report.progress_pct || 0) * 100 / 1, 1)}%)"

                :analyzing ->
                  "Analyzing findings..."

                :writing ->
                  "Writing report..."

                :failed ->
                  "Research failed"

                _ ->
                  "In progress..."
              end}
            </p>
          </div>
        <% end %>
      </div>
    </article>
    """
  end

  # --- Helpers ---

  defp export_report(id, socket) do
    report_dir = "priv/reports"
    File.mkdir_p(report_dir)
    report_path = Path.join(report_dir, "report_#{id}.md")

    case Ash.get(Research.Report, id) do
      {:ok, %{markdown_body: body}} when is_binary(body) ->
        File.write(report_path, body)
        {:noreply, put_flash(socket, :info, "Report exported to #{report_path}")}

      _ ->
        {:noreply, put_flash(socket, :error, "No markdown body available for export")}
    end
  end

  defp format_report_summary(report) do
    report
    |> Map.put(:source_count, report.total_sources)
    |> Map.take([:id, :title, :query, :status, :source_count, :inserted_at])
  end

  defp status_badge_class(:completed), do: "badge-success"
  defp status_badge_class(:researching), do: "badge-info"
  defp status_badge_class(:failed), do: "badge-error"
  defp status_badge_class(:analyzing), do: "badge-warning"
  defp status_badge_class(:writing), do: "badge-secondary"
  defp status_badge_class(_), do: "badge-ghost"

  defp status_label(:completed), do: "Completed"
  defp status_label(:researching), do: "Researching"
  defp status_label(:failed), do: "Failed"
  defp status_label(:analyzing), do: "Analyzing"
  defp status_label(:writing), do: "Writing"
  defp status_label(_), do: "Pending"

  defp format_markdown(text) do
    {:safe,
     MDEx.to_html!(text,
       extension: [strikethrough: true, tagfilter: false],
       render: [hardbreaks: true, unsafe_: true]
     )}
  end

  defp scraper_alert_class(:primary_success), do: "alert-success"
  defp scraper_alert_class(:fallback_success), do: "alert-warning"
  defp scraper_alert_class(:both_failed), do: "alert-error"
  defp scraper_alert_class(:native_failed), do: "alert-error"
  defp scraper_alert_class(_), do: "alert-info"

  defp scraper_outcome_icon(:primary_success), do: "✓"
  defp scraper_outcome_icon(:fallback_success), do: "⚠"
  defp scraper_outcome_icon(:both_failed), do: "✗"
  defp scraper_outcome_icon(:native_failed), do: "✗"
  defp scraper_outcome_icon(_), do: "•"
end
