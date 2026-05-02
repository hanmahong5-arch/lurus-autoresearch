defmodule ExAutoresearch.DeepResearch.ResearchOrchestrator do
  @moduledoc """
  Async state machine that drives one deep-research run at a time.

  ## State diagram

      idle ──cast(:start_research)──▶ planning ──┐
                                                 │ {:ok, queries}
                              planning failure ◀─┤   ↓
                              (retry ≤ 3, then   │ searching
                               fail)             │   ↓
                                                 │ investigations
                                                 │   ↓
                                                 │ analyzing
                                                 │   ↓
                              ┌──no deeper──────┐│
                              │                 ↓│
                              │           ┌──deeper queries──┐
                              │           │                  ↓
                              │           │             deepening
                              │           │                  ↓
                              ▼           ▼                  ↓
                                     writing ◀────────────────
                                          ↓
                                     completed → idle

  Each phase runs in a single wrapping `Task.async/1`. The orchestrator's
  `pending_task` field holds at most one `%Task{}` ref at a time; `handle_info`
  matches on that ref and dispatches `on_phase_done/3` to compute the next
  transition. State transitions go through `transition/3`, which logs, emits
  PubSub progress events, updates the Ash `Report.progress_pct`, and runs
  the entry action (`enter/2`) of the new state.

  All persistent state lives in SQLite via Ash. Crashes mark the report
  `:failed` and reset the orchestrator to `:idle`; subsequent runs are
  unaffected.
  """

  use GenServer

  require Logger
  require Ash.Query

  alias ExAutoresearch.Research
  alias ExAutoresearch.DeepResearch.{Tools.ResearchRunner, SearchQualityMonitor}
  alias ExAutoresearch.Agent.LLMClient

  @max_plan_retries 3
  @search_timeout 60_000
  @plan_timeout :timer.minutes(2)
  @analyze_timeout :timer.minutes(2)
  @synth_timeout :timer.minutes(5)

  defstruct status: :idle,
            report: nil,
            opts: nil,
            queries: [],
            deeper_queries: [],
            investigations: [],
            findings_summary: nil,
            retry_count: 0,
            pending_task: nil,
            monitor_pid: nil,
            failure_reason: nil

  @type status ::
          :idle | :planning | :searching | :analyzing | :deepening | :writing

  @type t :: %__MODULE__{
          status: status(),
          report: Ash.Resource.record() | nil,
          opts: keyword() | nil,
          queries: [String.t()],
          deeper_queries: [String.t()],
          investigations: [map()],
          findings_summary: String.t() | nil,
          retry_count: non_neg_integer(),
          pending_task: Task.t() | nil,
          monitor_pid: pid() | nil,
          failure_reason: term() | nil
        }

  @start_schema NimbleOptions.new!(
                  query: [type: :string, required: true],
                  title: [type: :string, required: true],
                  model: [type: :string, default: "claude-sonnet-4"],
                  max_depth: [type: :pos_integer, default: 3],
                  max_sources: [type: :pos_integer, default: 25],
                  organization_id: [type: {:or, [:string, nil]}, required: false]
                )

  # ── Client API ─────────────────────────────────────────────────────────────

  def start_link(opts \\ []), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @spec start_research(keyword()) :: :ok
  def start_research(opts) do
    opts = NimbleOptions.validate!(opts, @start_schema)
    GenServer.cast(__MODULE__, {:start_research, opts})
  end

  @spec stop_research() :: :ok
  def stop_research, do: GenServer.cast(__MODULE__, :stop_research)

  @spec set_model(String.t()) :: :ok
  def set_model(model) when is_binary(model) and model != "",
    do: GenServer.cast(__MODULE__, {:set_model, model})

  def status, do: GenServer.call(__MODULE__, :status)
  def reports, do: GenServer.call(__MODULE__, :reports)
  def report_detail(id), do: GenServer.call(__MODULE__, {:report_detail, id})

  # ── GenServer callbacks ────────────────────────────────────────────────────

  @impl true
  def init(_opts), do: {:ok, %__MODULE__{}}

  @impl true
  def handle_cast({:start_research, opts}, %{status: :idle} = state) do
    report = create_report(opts)
    {:ok, monitor_pid} = SearchQualityMonitor.start_link(report_id: report.id)

    broadcast({:status_changed, %{status: :researching, report_id: report.id}})

    state =
      %{state | report: report, opts: opts, monitor_pid: monitor_pid, retry_count: 0}
      |> transition(:planning)

    {:noreply, state}
  end

  def handle_cast({:start_research, _opts}, state) do
    Logger.warning("[Research] start_research ignored — current status #{state.status}")
    {:noreply, state}
  end

  def handle_cast(:stop_research, state) do
    {:noreply, abort(state, :stopped)}
  end

  def handle_cast({:set_model, model}, state) do
    if state.report do
      try do
        Ash.update!(state.report, %{model: model}, action: :update_status)
      rescue
        _ -> :ok
      end
    end

    broadcast({:status_changed, %{status: state.status, model: model}})
    {:noreply, state}
  end

  @impl true
  def handle_call(:status, _from, state),
    do: {:reply, %{status: state.status, report_id: state.report && state.report.id}, state}

  def handle_call(:reports, _from, state) do
    {:reply, Research.Report |> Ash.Query.sort(inserted_at: :desc) |> Ash.read!(), state}
  end

  def handle_call({:report_detail, id}, _from, state) do
    detail =
      with {:ok, report} when not is_nil(report) <- Ash.get(Research.Report, id) do
        invs =
          Research.Investigation
          |> Ash.Query.filter(report_id == ^report.id)
          |> Ash.read!()
          |> Enum.sort_by(& &1.inserted_at, :asc)

        {:ok, Map.put(report, :investigations, invs)}
      else
        _ -> :error
      end

    {:reply, detail, state}
  end

  @impl true
  def handle_info({ref, {:phase_done, phase, result}}, %{pending_task: %Task{ref: ref}} = state) do
    Process.demonitor(ref, [:flush])
    state = %{state | pending_task: nil}
    {:noreply, on_phase_done(phase, result, state)}
  end

  def handle_info({:DOWN, ref, :process, _pid, reason}, %{pending_task: %Task{ref: ref}} = state) do
    Logger.error("[Research] phase task crashed: #{inspect(reason, limit: 5)}")
    {:noreply, abort(state, {:phase_crashed, reason})}
  end

  def handle_info(:retry_planning, %{status: :planning} = state) do
    {:noreply, enter(:planning, state)}
  end

  def handle_info(_, state), do: {:noreply, state}

  # ── Transitions ────────────────────────────────────────────────────────────

  @spec transition(t(), status(), map()) :: t()
  defp transition(state, new_status, updates \\ %{}) do
    state = struct(state, Map.put(updates, :status, new_status))

    Logger.info("[Research] → #{new_status}")

    broadcast(
      {:research_step,
       %{step: Atom.to_string(new_status), report_id: state.report && state.report.id}}
    )

    update_progress(state.report, progress_for(new_status))

    enter(new_status, state)
  end

  defp progress_for(:planning), do: 5
  defp progress_for(:searching), do: 10
  defp progress_for(:analyzing), do: 50
  defp progress_for(:deepening), do: 70
  defp progress_for(:writing), do: 90
  defp progress_for(_), do: nil

  # ── State entry actions ────────────────────────────────────────────────────

  defp enter(:planning, %{report: report, opts: opts} = state) do
    spawn_phase(state, :planning, fn -> generate_plan(report.query, opts[:max_depth]) end)
  end

  defp enter(:searching, %{report: report, queries: queries} = state) do
    spawn_phase(state, :searching, fn -> run_investigations(report, queries) end)
  end

  defp enter(:analyzing, %{report: report, investigations: invs, opts: opts} = state) do
    spawn_phase(state, :analyzing, fn ->
      analyze(report.query, invs, opts[:max_depth])
    end)
  end

  defp enter(:deepening, %{report: report, deeper_queries: queries} = state) do
    spawn_phase(state, :deepening, fn -> run_investigations(report, queries) end)
  end

  defp enter(:writing, %{report: report, investigations: invs} = state) do
    spawn_phase(state, :writing, fn -> synthesize(report, invs) end)
  end

  defp enter(_other, state), do: state

  # ── Phase completion handlers ──────────────────────────────────────────────

  defp on_phase_done(:planning, {:ok, queries}, state) do
    transition(state, :searching, %{queries: queries})
  end

  defp on_phase_done(:planning, :error, %{retry_count: rc} = state) when rc < @max_plan_retries do
    Logger.warning("[Research] Planning failed (attempt #{rc + 1}), retrying in 2s")
    Process.send_after(self(), :retry_planning, 2_000)
    %{state | retry_count: rc + 1}
  end

  defp on_phase_done(:planning, :error, state) do
    Logger.error("[Research] Planning failed after #{@max_plan_retries} retries")
    finish(state, :failed, "Failed to generate research plan")
  end

  defp on_phase_done(:searching, investigations, state) do
    transition(state, :analyzing, %{investigations: investigations})
  end

  defp on_phase_done(:analyzing, {:no_findings}, state) do
    finish(state, :failed, "No successful investigations to analyze")
  end

  defp on_phase_done(:analyzing, {:ok, [], summary}, state) do
    transition(state, :writing, %{findings_summary: summary})
  end

  defp on_phase_done(:analyzing, {:ok, deeper, summary}, %{report: report} = state) do
    if length(state.investigations) < report.max_sources do
      transition(state, :deepening, %{deeper_queries: deeper, findings_summary: summary})
    else
      transition(state, :writing, %{findings_summary: summary})
    end
  end

  defp on_phase_done(:analyzing, :error, state) do
    transition(state, :writing, %{findings_summary: "Limited findings available"})
  end

  defp on_phase_done(:deepening, deep_invs, state) do
    transition(state, :writing, %{investigations: state.investigations ++ deep_invs})
  end

  defp on_phase_done(:writing, {:ok, body}, %{report: report, investigations: invs} = state) do
    update_report_complete(report, body, length(filter_completed(invs)))
    finish(state, :completed)
  end

  defp on_phase_done(:writing, {:fallback, body}, %{report: report, investigations: invs} = state) do
    update_report_complete(report, body, length(filter_completed(invs)), 0.95)
    finish(state, :completed)
  end

  defp on_phase_done(_, _, state), do: state

  # ── Termination ────────────────────────────────────────────────────────────

  defp finish(state, :completed) do
    broadcast({:research_step, %{step: "completed", report_id: state.report.id}})
    broadcast({:status_changed, %{status: :completed, report_id: state.report.id}})
    cleanup(state)
  end

  defp finish(state, :failed, reason) do
    fail_report(state.report, reason)
    broadcast({:status_changed, %{status: :failed, report_id: state.report && state.report.id}})
    cleanup(%{state | failure_reason: reason})
  end

  defp abort(state, reason) do
    if state.pending_task, do: Task.shutdown(state.pending_task, :brutal_kill)

    case reason do
      :stopped ->
        if state.report do
          try do
            Ash.update!(state.report, %{status: :completed}, action: :update_status)
          rescue
            _ -> :ok
          end
        end

        broadcast({:status_changed, %{status: :idle}})

      {:phase_crashed, why} ->
        fail_report(state.report, "Crashed: #{inspect(why, limit: 3)}")
        broadcast({:status_changed, %{status: :failed}})
    end

    cleanup(state)
  end

  defp cleanup(state) do
    if state.monitor_pid, do: GenServer.stop(state.monitor_pid, :normal)

    %__MODULE__{
      status: :idle,
      retry_count: 0
    }
  end

  # ── Phase implementations ──────────────────────────────────────────────────

  defp spawn_phase(state, phase, work_fn) do
    task = Task.async(fn -> {:phase_done, phase, work_fn.()} end)
    %{state | pending_task: task}
  end

  defp generate_plan(query, max_depth) do
    n = min(max_depth * 3, 9)

    prompt = """
    You are a deep research assistant. Given this research question:

    "#{query}"

    Generate a list of #{n} search queries that will help answer this question comprehensively.

    Format your response as a JSON array of strings ONLY (no other output):
    ["query 1", "query 2", "query 3"]
    """

    with {:ok, body} <- LLMClient.complete(prompt, tier: :main, timeout: @plan_timeout),
         {:ok, queries} <- parse_json_array(body) do
      {:ok, Enum.take(queries, n)}
    else
      _ -> :error
    end
  end

  defp parse_json_array(body) when is_binary(body) do
    with [json] <- Regex.run(~r/\[.*\]/s, body),
         {:ok, list} when is_list(list) <- Jason.decode(json) do
      {:ok, Enum.filter(list, &is_binary/1)}
    else
      _ -> :error
    end
  end

  defp parse_json_array(_), do: :error

  defp run_investigations(report, queries) do
    threads = Application.get_env(:ex_autoresearch, :research, [])[:max_threads] || 5

    queries
    |> Enum.chunk_every(threads)
    |> Enum.flat_map(fn batch ->
      batch
      |> Enum.map(&Task.async(fn -> run_one_investigation(report, &1) end))
      |> Task.await_many(@search_timeout)
    end)
  end

  defp run_one_investigation(report, query) do
    inv =
      Ash.create!(
        Research.Investigation,
        %{
          report_id: report.id,
          depth: 0,
          query: query,
          tool: :search,
          reasoning: "Initial search for: #{query}"
        },
        action: :start
      )

    broadcast({:investigation_started, %{investigation_id: inv.id, query: query, tool: :search}})

    case ResearchRunner.run(query, :search, report_id: report.id, investigation_id: inv.id) do
      {:ok, findings} ->
        complete_investigation(inv, report, findings, query)

      {:error, reason} ->
        Ash.update!(inv, %{status: :failed, error: inspect(reason)}, action: :fail)

        %{
          id: inv.id,
          query: query,
          findings: nil,
          quality_score: 0.0,
          status: :failed,
          error: reason
        }
    end
  rescue
    e ->
      %{
        id: nil,
        query: query,
        findings: nil,
        quality_score: 0.0,
        status: :failed,
        error: Exception.message(e)
      }
  end

  defp complete_investigation(inv, report, findings, query) do
    Ash.update!(
      inv,
      %{
        status: :completed,
        findings: findings.content,
        quality_score: findings.quality_score,
        sources_count: length(findings.sources),
        url: List.first(findings.sources, [])["url"]
      },
      action: :complete
    )

    Ash.update!(
      report,
      %{
        total_sources: report.total_sources + length(findings.sources),
        total_investigations: report.total_investigations + 1
      },
      action: :update_result
    )

    %{
      id: inv.id,
      query: query,
      findings: findings.content,
      quality_score: findings.quality_score,
      status: :completed
    }
  end

  defp analyze(question, investigations, max_depth) do
    case filter_completed(investigations) do
      [] ->
        {:no_findings}

      successful ->
        summary = format_findings(successful, 1000)

        prompt = """
        Based on these research findings for the question "#{question}",
        determine if we need to go deeper.

        #{summary}

        If we need more information, generate up to #{max_depth} more specific search queries.
        Otherwise respond with an empty JSON array.

        Format:
        {"need_deeper": true/false, "queries": ["q1", "q2", ...], "summary": "brief summary of what we know"}
        """

        with {:ok, body} <-
               LLMClient.complete(prompt, tier: :main, timeout: @analyze_timeout),
             {:ok, decision} <- parse_analysis(body, max_depth, summary) do
          decision
        else
          _ -> {:ok, [], summary}
        end
    end
  end

  defp parse_analysis(body, max_depth, fallback_summary) do
    case Jason.decode(body) do
      {:ok, %{"need_deeper" => true, "queries" => qs, "summary" => s}} when is_list(qs) ->
        {:ok, {:ok, Enum.take(qs, max_depth), s}}

      {:ok, %{"queries" => [], "summary" => s}} ->
        {:ok, {:ok, [], s}}

      _ ->
        {:ok, {:ok, [], fallback_summary}}
    end
  end

  defp synthesize(report, investigations) do
    successful = filter_completed(investigations)
    findings_text = format_synthesis(successful)

    prompt = """
    Write a comprehensive research report answering this question:

    "#{report.query}"

    Here are the research findings:

    #{findings_text}

    Format your response as a well-structured markdown report with:
    1. Executive Summary
    2. Detailed Analysis (organized by topic)
    3. Key Findings
    4. Sources (list sources at the end)
    """

    case LLMClient.complete(prompt, tier: :main, timeout: @synth_timeout) do
      {:ok, body} -> {:ok, body}
      _ -> {:fallback, "# #{report.title}\n\n#{findings_text}"}
    end
  end

  # ── Persistence helpers ────────────────────────────────────────────────────

  defp create_report(opts) do
    base = %{
      title: opts[:title] || String.slice(opts[:query], 0, 50),
      query: opts[:query],
      model: opts[:model],
      max_depth: opts[:max_depth],
      max_sources: opts[:max_sources]
    }

    {params, tenant} =
      case Keyword.get(opts, :organization_id) do
        nil -> {base, nil}
        org_id -> {Map.put(base, :organization_id, org_id), org_id}
      end

    Ash.create!(Research.Report, params, action: :start, tenant: tenant)
  end

  defp update_progress(nil, _), do: :ok
  defp update_progress(_report, nil), do: :ok

  defp update_progress(report, pct) do
    Ash.update!(report, %{progress_pct: pct / 100.0}, action: :update_status)
  rescue
    _ -> :ok
  end

  defp update_report_complete(report, body, source_count, progress \\ 1.0) do
    Ash.update!(
      report,
      %{
        status: :completed,
        markdown_body: body,
        progress_pct: progress,
        summary: "#{source_count} sources analyzed"
      },
      action: :complete
    )
  rescue
    _ -> :ok
  end

  defp fail_report(nil, _), do: :ok

  defp fail_report(report, reason) do
    Ash.update!(
      report,
      %{status: :failed, summary: to_string(reason), progress_pct: 0.0},
      action: :update_status
    )
  rescue
    _ -> :ok
  end

  # ── Pure helpers ───────────────────────────────────────────────────────────

  defp filter_completed(investigations),
    do: Enum.filter(investigations, &(&1.status == :completed))

  defp format_findings(invs, max_chars) do
    Enum.map_join(invs, "\n\n---\n\n", fn inv ->
      content = String.slice(inv.findings || "", 0, max_chars)
      "Query: #{inv.query}\nContent: #{content}"
    end)
  end

  defp format_synthesis(invs) do
    Enum.map_join(invs, "\n\n---\n\n", fn inv ->
      "## #{inv.query}\n#{inv.findings || "No content found"}"
    end)
  end

  defp broadcast(msg) do
    Phoenix.PubSub.broadcast(ExAutoresearch.PubSub, "research:events", msg)
  rescue
    _ -> :ok
  end
end
