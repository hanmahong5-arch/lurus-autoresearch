defmodule ExAutoresearch.Workers.ResearchWorker do
  @moduledoc """
  Oban worker that executes a research task from a template.

  Used for scheduled/automated research runs triggered by cron expressions
  defined on templates.
  """

  use Oban.Worker, queue: :default, max_attempts: 3

  require Logger

  alias ExAutoresearch.{Research, DeepResearch}
  alias DeepResearch.Tools.ResearchRunner
  alias ExAutoresearch.Agent.LLMClient

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"template_id" => template_id, "organization_id" => org_id}}) do
    Logger.info("[ResearchWorker] Starting research for template #{template_id}, org #{org_id}")

    with {:ok, template} <- Ash.get(Research.Template, template_id),
         {:ok, _report} <- execute_research(template) do
      :ok
    else
      {:error, reason} ->
        Logger.error("[ResearchWorker] Failed: #{inspect(reason)}")
        {:error, reason}
    end
  end

  @doc """
  Executes a full research cycle from a template, stores the result as a Report.
  """
  def execute_research(template) do
    # Create initial report
    {:ok, report} = Ash.create(Research.Report, %{
      title: "#{template.name} - #{Date.utc_today()}",
      query: template.query_template,
      model: template.model,
      max_depth: template.max_depth,
      max_sources: template.max_sources,
      category: template.category
    }, action: :start, tenant: template.organization_id)

    Phoenix.PubSub.broadcast(
      ExAutoresearch.PubSub,
      "research:events",
      {:status_changed, %{status: :researching, report_id: report.id}}
    )

    Ash.update!(report, %{status: :researching}, action: :update_status)

    # Execute research
    result = run_research_loop(report, template)

    case result do
      {:ok, markdown_body, findings} ->
        Ash.update!(report, %{
          status: :completed,
          markdown_body: markdown_body,
          progress_pct: 1.0,
          summary: "#{length(findings)} findings collected"
        }, action: :complete)

        Phoenix.PubSub.broadcast(
          ExAutoresearch.PubSub,
          "research:events",
          {:research_step, %{step: "writing", report_id: report.id}}
        )

        # Send notification
        ExAutoresearch.Notifications.Notifier.report_completed(report)

        {:ok, report}

      {:error, reason} ->
        Ash.update!(report, %{
          status: :failed,
          summary: inspect(reason)
        }, action: :update_status)

        {:error, reason}
    end
  end

  defp run_research_loop(report, template) do
    llm_pid = start_llm(report.model)

    try do
      queries = generate_queries(report.query, template.max_depth, llm_pid)

      if queries == [] do
        {:error, :no_queries}
      else
        investigations = execute_searches(report, queries, llm_pid)
        synthesize(report, investigations, llm_pid)
      end
    after
      if llm_pid && Process.alive?(llm_pid) do
        GenServer.stop(llm_pid, :normal)
      end
    end
  end

  defp start_llm(model), do: start_llm_backend(model)

  defp start_llm_backend(model) do
    case LLMClient.start_link(model: model) do
      {:ok, pid} -> pid
      _ -> nil
    end
  rescue
    _ -> nil
  end

  defp generate_queries(query, _max_depth, nil) do
    [query]
  end

  defp generate_queries(query, max_depth, llm_pid) do
    prompt = """
    Given this research question: "#{query}"
    Generate #{min(max_depth * 3, 9)} specific search queries.
    Respond with ONLY a JSON array: ["q1", "q2", ...]
    """

    case GenServer.call(llm_pid, {:prompt, prompt, nil}, :timer.minutes(2)) do
      {:ok, response} ->
        case Regex.run(~r/\[.*\]/s, response) do
          [json] ->
            case Jason.decode(json) do
              {:ok, qs} when is_list(qs) -> Enum.filter(qs, &is_binary/1)
              _ -> [query]
            end

          _ -> [query]
        end

      _ -> [query]
    end
  end

  defp execute_searches(report, queries, _llm_pid) do
    max_threads =
      Application.get_env(:ex_autoresearch, :research, [])[:max_threads] || 5

    queries
    |> Enum.chunk_every(max_threads)
    |> Enum.flat_map(fn batch ->
      batch
      |> Enum.map(&Task.async(fn -> run_investigation(report, &1) end))
      |> Task.await_many(60_000)
    end)
  end

  defp run_investigation(report, query) do
    inv =
      Ash.create!(Research.Investigation, %{
        report_id: report.id,
        depth: 0,
        query: query,
        tool: :search,
        reasoning: "Scheduled research for: #{query}"
      }, action: :start)

    case ResearchRunner.run(query, :search) do
      {:ok, findings} ->
        Ash.update!(inv, %{
          status: :completed,
          findings: findings.content,
          quality_score: findings.quality_score,
          sources_count: length(findings.sources),
          url: List.first(findings.sources, %{})["url"]
        }, action: :complete)

        Ash.update!(report, %{
          total_sources: report.total_sources + length(findings.sources),
          total_investigations: report.total_investigations + 1
        }, action: :update_result)

        %{id: inv.id, query: query, findings: findings.content, status: :completed}

      {:error, reason} ->
        Ash.update!(inv, %{status: :failed, error: inspect(reason)}, action: :fail)
        %{id: inv.id, query: query, findings: nil, status: :failed}
    end
  rescue
    _e -> %{id: nil, query: query, findings: nil, status: :failed}
  end

  defp synthesize(report, investigations, llm_pid) do
    successful = Enum.filter(investigations, &(&1.status == :completed))

    if successful == [] do
      {:error, :no_findings}
    else
      findings_text =
        successful
        |> Enum.map_join("\n\n---\n\n", fn inv ->
          "## #{inv.query}\n#{inv.findings || "No content"}"
        end)

      prompt = """
      Write a comprehensive research report answering: "#{report.query}"

      Research findings:
      #{findings_text}

      Format as markdown with sections for Executive Summary, Detailed Analysis, Key Findings.
      """

      case GenServer.call(llm_pid, {:prompt, prompt, nil}, :timer.minutes(5)) do
        {:ok, body} -> {:ok, body, successful}
        _ -> {:ok, "# #{report.title}\n\n#{findings_text}", successful}
      end
    end
  end
end
