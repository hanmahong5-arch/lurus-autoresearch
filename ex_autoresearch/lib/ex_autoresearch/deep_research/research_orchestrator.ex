defmodule ExAutoresearch.DeepResearch.ResearchOrchestrator do
  @moduledoc """
  Main orchestrator for deep research.

  Coordinates the full research lifecycle:
  1. User submits a query
  2. LLM generates initial search plan
  3. Spawns parallel investigation threads
  4. Monitors quality via SearchQualityMonitor
  5. Iterates deeper based on findings
  6. Synthesizes final report
  7. Outputs markdown

  All state lives in SQLite via Ash. Stops and resumes are seamless.
  """

  use GenServer

  require Logger

  alias ExAutoresearch.{Research, DeepResearch}
  alias DeepResearch.{Tools.ResearchRunner, SearchQualityMonitor}
  alias ExAutoresearch.Agent.LLMClient

  require Ash.Query

  defstruct [:report_id, :task, status: :idle]

  @start_schema NimbleOptions.new!(
                  query: [type: :string, required: true, doc: "Research question"],
                  title: [type: :string, required: true, doc: "Report title"],
                  model: [type: :string, default: "claude-sonnet-4"],
                  max_depth: [type: :pos_integer, default: 3],
                  max_sources: [type: :pos_integer, default: 25]
                )

  # Client API

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @spec start_research(keyword()) :: :ok
  def start_research(opts \\ []) do
    opts = NimbleOptions.validate!(opts, @start_schema)
    GenServer.cast(__MODULE__, {:start_research, opts})
  end

  @spec stop_research() :: :ok
  def stop_research, do: GenServer.cast(__MODULE__, :stop_research)

  @spec set_model(String.t()) :: :ok
  def set_model(model_id) when is_binary(model_id) and model_id != "",
    do: GenServer.cast(__MODULE__, {:set_model, model_id})

  def status do
    GenServer.call(__MODULE__, :status)
  end

  def reports do
    GenServer.call(__MODULE__, :reports)
  end

  def report_detail(id) do
    GenServer.call(__MODULE__, {:report_detail, id})
  end

  # Server

  @impl true
  def init(_opts) do
    {:ok, %__MODULE__{}}
  end

  @impl true
  def handle_cast({:start_research, opts}, state) do
    report = create_report(opts)
    broadcast(:status_changed, %{status: :researching, report_id: report.id})

    task = Task.async(fn -> research_loop(report, opts) end)
    {:noreply, %{state | report_id: report.id, task: task, status: :researching}}
  end

  def handle_cast(:stop_research, state) do
    if state.report_id do
      case Ash.get(Research.Report, state.report_id) do
        {:ok, report} when not is_nil(report) ->
          Ash.update!(report, %{status: :completed}, action: :update_status)

        _ -> :ok
      end
    end

    broadcast(:status_changed, %{status: :idle})
    {:noreply, %{state | status: :idle}}
  end

  def handle_cast({:set_model, model_id}, state) do
    if state.report_id do
      case Ash.get(Research.Report, state.report_id) do
        {:ok, report} when not is_nil(report) ->
          Ash.update!(report, %{model: model_id}, action: :update_status)
        _ -> :ok
      end
    end

    broadcast(:status_changed, %{status: state.status, model: model_id})
    {:noreply, state}
  end

  @impl true
  def handle_call(:status, _from, state), do: {:reply, %{status: state.status}, state}

  def handle_call(:reports, _from, state) do
    reports =
      Research.Report
      |> Ash.Query.sort(inserted_at: :desc)
      |> Ash.read!()

    {:reply, reports, state}
  end

  def handle_call({:report_detail, id}, _from, state) do
    detail =
      case Ash.get(Research.Report, id) do
        {:ok, rep} when not is_nil(rep) ->
          investigations = Ash.read!(Ash.Query.filter(Research.Investigation, report_id == ^rep.id))
          investigations = Enum.sort_by(investigations, & &1.inserted_at, :asc)
          {:ok, Map.put(rep, :investigations, investigations)}

        _ -> :error
      end

    {:reply, detail, state}
  end

  @impl true
  def handle_info({ref, _result}, %{task: %Task{ref: ref}} = state) do
    Process.demonitor(ref, [:flush])
    broadcast(:status_changed, %{status: :completed, report_id: state.report_id})
    {:noreply, %{state | task: nil, status: :idle}}
  end

  def handle_info({:DOWN, ref, :process, _pid, reason}, %{task: %Task{ref: ref}} = state) do
    Logger.error("Research loop crashed: #{inspect(reason, limit: 3)}")

    if state.report_id do
      case Ash.get(Research.Report, state.report_id) do
        {:ok, report} when not is_nil(report) ->
          Ash.update!(report, %{status: :failed}, action: :update_status)

        _ -> :ok
      end
    end

    broadcast(:status_changed, %{status: :failed})
    {:noreply, %{state | task: nil, status: :idle}}
  end

  def handle_info(_, state), do: {:noreply, state}

  # Research loop coordinates the full lifecycle
  @max_retries 3

  defp research_loop(report, opts, retry_count \\ 0) do
    max_depth = report.max_depth
    llm_pid = start_llm_backend(report.model)

    # Start quality monitor
    {:ok, monitor} = SearchQualityMonitor.start_link(report_id: report.id)

    try do
      # Phase 1: Generate initial search plan
      Logger.info("[Research] Generating search plan for: #{report.query}")
      broadcast(:research_step, %{step: "planning", report_id: report.id})

      case generate_search_plan(report, llm_pid, max_depth) do
        {:ok, search_queries} ->
          # Phase 2: Execute searches in parallel
          broadcast(:research_step, %{step: "searching", count: length(search_queries), report_id: report.id})
          update_progress(report, 10)

          investigations = execute_searches(report, search_queries, llm_pid)

          # Phase 3: Analyze findings
          broadcast(:research_step, %{step: "analyzing", report_id: report.id})
          update_progress(report, 50)

          case analyze_findings(report, investigations, llm_pid, max_depth) do
            {:ok, deeper_queries, findings_summary} ->
              # Phase 4: Go deeper if needed
              if deeper_queries != [] and report.total_investigations < report.max_sources do
                broadcast(:research_step, %{
                  step: "deep_dive",
                  count: length(deeper_queries),
                  report_id: report.id
                })
                update_progress(report, 70)

                deep_investigations = execute_searches(report, deeper_queries, llm_pid)
                synthesize_report(report, investigations ++ deep_investigations, findings_summary, llm_pid)
              else
                synthesize_report(report, investigations, findings_summary, llm_pid)
              end

            :error ->
              synthesize_report(report, investigations, "Limited findings available", llm_pid)
          end

        :error when retry_count < @max_retries ->
          Logger.warning("[Research] Planning failed (attempt #{retry_count + 1}), retrying")
          Process.sleep(2_000)
          research_loop(report, opts, retry_count + 1)

        :error ->
          Logger.error("[Research] Planning failed after #{retry_count} retries")
          handle_failure(report, "Failed to generate research plan")
      end
    after
      GenServer.stop(monitor, :normal)
      if llm_pid && Process.alive?(llm_pid), do: GenServer.stop(llm_pid, :normal)
    end

    broadcast(:research_step, %{step: "completed", report_id: report.id})
  end

  defp create_report(opts) do
    report_params = %{
      title: opts[:title] || String.slice(opts[:query], 0, 50),
      query: opts[:query],
      model: opts[:model],
      max_depth: opts[:max_depth],
      max_sources: opts[:max_sources]
    }

    {report_params, tenant} =
      case Keyword.get(opts, :organization_id) do
        nil -> {report_params, nil}
        org_id -> {Map.put(report_params, :organization_id, org_id), org_id}
      end

    Ash.create!(Research.Report, report_params, action: :start, tenant: tenant)
  end

  defp update_progress(report, pct) do
    Ash.update!(report, %{progress_pct: pct / 100.0}, action: :update_status)
  rescue
    _ -> :ok
  end

  defp handle_failure(report, reason) do
    Ash.update!(report, %{
      status: :failed,
      summary: reason,
      progress_pct: 0.0
    }, action: :update_status)
  rescue
    _ -> :ok
  end

  # --- LLM Integration ---

  defp start_llm_backend(model) do
    case LLMClient.start_link(model: model) do
      {:ok, pid} -> pid
      {:error, _} -> nil
    end
  rescue
    _ -> nil
  end

  # --- Search Plan Generation ---

  defp generate_search_plan(report, llm_pid, max_depth) do
    prompt = """
    You are a deep research assistant. Given this research question:

    "#{report.query}"

    Generate a list of #{min(max_depth * 3, 9)} search queries that will help answer this question comprehensively.

    Format your response as a JSON array of strings ONLY (no other output):
    ["query 1", "query 2", "query 3"]
    """

    case call_llm(llm_pid, prompt) do
      {:ok, response} ->
        case parse_queries(response) do
          {:ok, queries} -> {:ok, Enum.take(queries, max_depth * 3)}
          :error -> :error
        end

      {:error, _} -> :error
    end
  end

  defp parse_queries(response) do
    # Extract JSON array from LLM response
    case Regex.run(~r/\[.*\]/s, response) do
      [json] ->
        case Jason.decode(json) do
          {:ok, queries} when is_list(queries) ->
            {:ok, Enum.filter(queries, &is_binary/1)}

          _ -> :error
        end

      _ -> :error
    end
  end

  # --- Execute Searches ---

  defp execute_searches(report, queries, llm_pid) do
    max_threads = Application.get_env(:ex_autoresearch, :research, [])[:max_threads] || 5

    queries
    |> Enum.chunk_every(max_threads)
    |> Enum.flat_map(fn batch ->
      batch
      |> Enum.map(fn query ->
        Task.async(fn ->
          run_single_investigation(report, query, :search, llm_pid)
        end)
      end)
      |> Task.await_many(60_000)
    end)
  end

  defp run_single_investigation(report, query, tool, _llm_pid) do
    # Start investigation record
    inv =
      Ash.create!(Research.Investigation, %{
        report_id: report.id,
        depth: 0,
        query: query,
        tool: tool,
        reasoning: "Initial search for: #{query}"
      }, action: :start)

    broadcast(:investigation_started, %{
      investigation_id: inv.id,
      query: query,
      tool: tool
    })

    # Execute search
    result = ResearchRunner.run(query, tool)

    case result do
      {:ok, findings} ->
        Ash.update!(inv, %{
          status: :completed,
          findings: findings.content,
          quality_score: findings.quality_score,
          sources_count: length(findings.sources),
          url: List.first(findings.sources, [])["url"]
        }, action: :complete)

        Ash.update!(report, %{
          total_sources: report.total_sources + length(findings.sources),
          total_investigations: report.total_investigations + 1
        }, action: :update_result)

        %{
          id: inv.id,
          query: query,
          findings: findings.content,
          quality_score: findings.quality_score,
          status: :completed
        }

      {:error, reason} ->
        Ash.update!(inv, %{
          status: :failed,
          error: inspect(reason)
        }, action: :fail)

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
      %{id: nil, query: query, findings: nil, quality_score: 0.0, status: :failed, error: Exception.message(e)}
  end

  # --- Analyze Findings ---

  defp analyze_findings(report, investigations, llm_pid, max_depth) do
    successful = Enum.filter(investigations, &(&1.status == :completed))

    if successful == [] do
      :error
    else
      findings_summary =
        successful
        |> Enum.map_join("\n\n---\n\n", fn inv ->
          "Query: #{inv.query}\nContent: #{String.slice(inv.findings || "", 0, 1000)}"
        end)

      # Ask LLM if we need to go deeper
      prompt = """
      Based on these research findings for the question "#{report.query}",
      determine if we need to go deeper.

      #{findings_summary}

      If we need more information, generate up to #{max_depth} more specific search queries.
      Otherwise respond with an empty JSON array.

      Format:
      {"need_deeper": true/false, "queries": ["q1", "q2", ...], "summary": "brief summary of what we know"}
      """

      case call_llm(llm_pid, prompt) do
        {:ok, response} ->
          case Jason.decode(response) do
            {:ok, %{"need_deeper" => true, "queries" => queries, "summary" => summary}} ->
              {:ok, Enum.take(queries, max_depth), summary}

            {:ok, %{"queries" => [], "summary" => summary}} ->
              {:ok, [], summary}

            _ ->
              {:ok, [], findings_summary}
          end

        _ ->
          {:ok, [], findings_summary}
      end
    end
  end

  # --- Synthesize Final Report ---

  defp synthesize_report(report, investigations, _findings_summary, llm_pid) do
    broadcast(:research_step, %{step: "writing", report_id: report.id})
    update_progress(report, 90)

    successful = Enum.filter(investigations, &(&1.status == :completed))

    findings_text =
      successful
      |> Enum.map_join("\n\n---\n\n", fn inv ->
        "## #{inv.query}\n#{inv.findings || "No content found"}"
      end)

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

    case call_llm(llm_pid, prompt) do
      {:ok, report_body} ->
        Ash.update!(report, %{
          status: :completed,
          markdown_body: report_body,
          progress_pct: 1.0,
          summary: "#{length(successful)} sources analyzed"
        }, action: :complete)

      {:error, _} ->
        # Fallback: use raw findings as report
        Ash.update!(report, %{
          status: :completed,
          markdown_body: "# #{report.title}\n\n#{findings_text}",
          progress_pct: 0.95,
          summary: "Report generated from #{length(successful)} sources"
        }, action: :complete)
    end

    broadcast(:research_step, %{step: "completed", report_id: report.id})
  end

  # --- LLM Call Helper ---

  defp call_llm(nil, _prompt), do: {:error, :no_llm}

  defp call_llm(llm_pid, prompt) do
    case GenServer.call(llm_pid, {:prompt, prompt, nil}, :timer.minutes(5)) do
      {:ok, text} -> {:ok, text}
      {:error, reason} -> {:error, reason}
    end
  rescue
    e -> {:error, {:llm_call_failed, Exception.message(e)}}
  end

  # --- PubSub ---

  defp broadcast(event, payload) do
    Phoenix.PubSub.broadcast(ExAutoresearch.PubSub, "research:events", {event, payload})
  rescue
    _ -> :ok
  end
end
