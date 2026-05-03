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

  @parallelism_default 5

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
    {:ok, report} =
      Ash.create(
        Research.Report,
        %{
          title: "#{template.name} - #{Date.utc_today()}",
          query: template.query_template,
          model: template.model,
          max_depth: template.max_depth,
          max_sources: template.max_sources,
          category: template.category
        },
        action: :start,
        tenant: template.organization_id
      )

    broadcast({:status_changed, %{status: :researching, report_id: report.id}})
    Ash.update!(report, %{status: :researching}, action: :update_status)

    case run_research_loop(report, template) do
      {:ok, markdown_body, findings} ->
        Ash.update!(
          report,
          %{
            status: :completed,
            markdown_body: markdown_body,
            progress_pct: 1.0,
            summary: "#{length(findings)} findings collected"
          },
          action: :complete
        )

        broadcast({:research_step, %{step: "writing", report_id: report.id}})
        ExAutoresearch.Notifications.Notifier.report_completed(report)
        {:ok, report}

      {:error, reason} ->
        Ash.update!(report, %{status: :failed, summary: inspect(reason)}, action: :update_status)
        {:error, reason}
    end
  end

  defp run_research_loop(report, template) do
    case generate_queries(report.query, template.max_depth,
           report_id: report.id,
           organization_id: report.organization_id
         ) do
      [] ->
        {:error, :no_queries}

      queries ->
        investigations = run_investigations(report, queries)
        synthesize(report, investigations)
    end
  end

  defp generate_queries(query, max_depth, llm_opts) do
    prompt = """
    Given this research question: "#{query}"
    Generate #{min(max_depth * 3, 9)} specific search queries.
    Respond with ONLY a JSON array: ["q1", "q2", ...]
    """

    with {:ok, response} <-
           LLMClient.complete(
             prompt,
             [tier: :fast, timeout: :timer.minutes(2)] ++ llm_opts
           ),
         [json] <- Regex.run(~r/\[.*\]/s, response),
         {:ok, qs} when is_list(qs) <- Jason.decode(json) do
      Enum.filter(qs, &is_binary/1)
    else
      _ -> [query]
    end
  end

  defp run_investigations(report, queries) do
    threads =
      Application.get_env(:ex_autoresearch, :research, [])[:max_threads] || @parallelism_default

    queries
    |> Enum.chunk_every(threads)
    |> Enum.flat_map(fn batch ->
      batch
      |> Enum.map(&Task.async(fn -> run_investigation(report, &1) end))
      |> Task.await_many(60_000)
    end)
  end

  defp run_investigation(report, query) do
    inv =
      Ash.create!(
        Research.Investigation,
        %{
          report_id: report.id,
          depth: 0,
          query: query,
          tool: :search,
          reasoning: "Scheduled research for: #{query}"
        },
        action: :start
      )

    case ResearchRunner.run(query, :search) do
      {:ok, findings} ->
        Ash.update!(
          inv,
          %{
            status: :completed,
            findings: findings.content,
            quality_score: findings.quality_score,
            sources_count: length(findings.sources),
            url: List.first(findings.sources, %{})["url"]
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

        %{id: inv.id, query: query, findings: findings.content, status: :completed}

      {:error, reason} ->
        Ash.update!(inv, %{status: :failed, error: inspect(reason)}, action: :fail)
        %{id: inv.id, query: query, findings: nil, status: :failed}
    end
  rescue
    _ -> %{id: nil, query: query, findings: nil, status: :failed}
  end

  defp synthesize(report, investigations) do
    case Enum.filter(investigations, &(&1.status == :completed)) do
      [] ->
        {:error, :no_findings}

      successful ->
        findings_text =
          Enum.map_join(successful, "\n\n---\n\n", fn inv ->
            "## #{inv.query}\n#{inv.findings || "No content"}"
          end)

        prompt = """
        Write a comprehensive research report answering: "#{report.query}"

        Research findings:
        #{findings_text}

        Format as markdown with sections for Executive Summary, Detailed Analysis, Key Findings.
        """

        case LLMClient.complete(prompt,
               tier: :main,
               timeout: :timer.minutes(5),
               report_id: report.id,
               organization_id: report.organization_id
             ) do
          {:ok, body} -> {:ok, body, successful}
          _ -> {:ok, "# #{report.title}\n\n#{findings_text}", successful}
        end
    end
  end

  defp broadcast(msg) do
    Phoenix.PubSub.broadcast(ExAutoresearch.PubSub, "research:events", msg)
  rescue
    _ -> :ok
  end
end
