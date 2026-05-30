defmodule ExAutoresearch.DeepResearch.ResearchEngine do
  @moduledoc """
  Stateless research engine. Drives one deep-research run synchronously.

  The orchestrator GenServer has been replaced by this module.  All persistent
  state lives in SQLite via Ash; the engine is pure pipeline logic.

  Public API
  ----------
  - `start/1`            — validate opts, create report, enqueue Oban job, return `{:ok, report}`
  - `run/2`              — synchronous linear pipeline (plan→search→analyze→deepen→write→verify)
  - `run_from_template/1`— create a report from a Template struct then call `run/2`
  - `continue?/1`        — check if a report is still active or has been stopped externally
  - `status/0`           — most-recent active report status (or `:idle`)
  - `report_detail/1`    — load a report with its investigations
  - `stop_research/0`    — mark the active report completed and broadcast idle
  - `quality_alert?/1`   — pure function: classify a list of quality scores
  """

  require Logger
  require Ash.Query

  alias ExAutoresearch.Research
  alias ExAutoresearch.DeepResearch.{Tools.ResearchRunner, SourcesBlock, Verifier}
  alias ExAutoresearch.Agent.LLMClient

  @max_plan_retries 3
  @search_timeout 60_000
  @plan_timeout :timer.minutes(2)
  @analyze_timeout :timer.minutes(2)
  @synth_timeout :timer.minutes(5)

  @start_schema NimbleOptions.new!(
                  query: [type: :string, required: true],
                  title: [type: :string, required: true],
                  model: [type: :string, default: "claude-sonnet-4"],
                  max_depth: [type: :pos_integer, default: 3],
                  max_sources: [type: :pos_integer, default: 25],
                  organization_id: [type: {:or, [:string, nil]}, required: false]
                )

  # ── Public API ─────────────────────────────────────────────────────────────

  @doc """
  Validates opts, creates a Report, sets status to :researching, enqueues an
  Oban job, and returns `{:ok, report}`.
  """
  @spec start(keyword()) :: {:ok, Ash.Resource.record()} | {:error, term()}
  def start(opts) do
    opts = NimbleOptions.validate!(opts, @start_schema)
    report = create_report(opts)

    Ash.update!(report, %{status: :researching}, action: :update_status)
    broadcast({:status_changed, %{status: :researching, report_id: report.id}})

    Oban.insert(
      ExAutoresearch.Workers.ResearchWorker.new(
        %{"report_id" => report.id},
        queue: :research
      )
    )

    {:ok, report}
  end

  @doc """
  Synchronous linear pipeline. Runs plan→search→analyze→(optionally deepen)→write→verify.
  Returns `{:ok, completed_report}` or `{:error, reason}`.
  """
  @spec run(Ash.Resource.record(), keyword()) :: {:ok, Ash.Resource.record()} | {:error, term()}
  def run(report, opts) do
    domain_opts = Keyword.take(opts, [:allow_domains, :block_domains])

    with {:ok, report} <- continue?(report),
         {:ok, queries, report} <- do_plan(report),
         {:ok, report} <- continue?(report) do
      broadcast({:plan_generated, %{report_id: report.id, queries: queries}})
      broadcast({:research_step, %{step: "searching", report_id: report.id}})
      update_progress(report, progress_for(:searching))

      investigations = run_investigations(report, queries, domain_opts)

      with {:ok, report} <- continue?(report) do
        broadcast({:research_step, %{step: "analyzing", report_id: report.id}})
        update_progress(report, progress_for(:analyzing))

        case analyze(report.query, investigations, report.max_depth,
               report_id: report.id,
               organization_id: report.organization_id
             ) do
          {:no_findings} ->
            fail_report(report, "No successful investigations to analyze")
            {:error, :no_findings}

          analysis_result ->
            with {:ok, report} <- continue?(report) do
              {deeper_queries, summary} = unwrap_analysis(analysis_result)

              scores =
                investigations
                |> filter_completed()
                |> Enum.map(& &1.quality_score)

              quality_signal = quality_alert?(scores)

              investigations =
                if deepen?(deeper_queries, quality_signal, investigations, report) do
                  broadcast({:research_step, %{step: "deepening", report_id: report.id}})
                  update_progress(report, progress_for(:deepening))

                  deep_invs = run_investigations(report, deeper_queries, domain_opts)
                  investigations ++ deep_invs
                else
                  investigations
                end

              with {:ok, report} <- continue?(report) do
                broadcast({:research_step, %{step: "writing", report_id: report.id}})
                update_progress(report, progress_for(:writing))

                _ = summary

                {draft_body, draft_progress} =
                  case synthesize(report, investigations) do
                    {:ok, body} -> {body, 1.0}
                    {:fallback, body} -> {body, 0.95}
                  end

                broadcast({:research_step, %{step: "verifying", report_id: report.id}})
                update_progress(report, progress_for(:verifying))

                annotated_body =
                  case Verifier.verify(report, draft_body, investigations) do
                    {:ok, annotated} -> annotated
                    _ -> draft_body
                  end

                update_report_complete(
                  report,
                  annotated_body,
                  length(filter_completed(investigations)),
                  draft_progress
                )

                broadcast({:research_step, %{step: "completed", report_id: report.id}})

                broadcast({:status_changed, %{status: :completed, report_id: report.id}})

                case Ash.get(Research.Report, report.id) do
                  {:ok, reloaded} -> {:ok, reloaded}
                  _ -> {:ok, report}
                end
              else
                :halted -> {:ok, report}
              end
            else
              :halted -> {:ok, report}
            end
        end
      else
        :halted -> {:ok, report}
      end
    else
      :halted -> {:ok, report}
      {:error, :planning_failed} = err -> err
    end
  end

  @doc """
  Returns the next run_version integer for a given Brief.
  Queries all Reports with that brief_id and returns max(run_version) + 1,
  or 1 if no reports exist yet.
  """
  @spec next_run_version(Ash.Resource.record()) :: pos_integer()
  def next_run_version(brief) do
    tenant = brief.organization_id

    opts = if tenant, do: [tenant: tenant], else: []

    reports =
      Research.Report
      |> Ash.Query.filter(brief_id == ^brief.id)
      |> Ash.read!(opts)

    case reports do
      [] ->
        1

      list ->
        max_v = list |> Enum.map(& &1.run_version) |> Enum.max()
        max_v + 1
    end
  rescue
    _ -> 1
  end

  @doc """
  Enqueues an Oban job to run research from a Brief. Non-blocking.
  Returns `{:ok, job}` or `{:error, reason}`.
  """
  @spec enqueue_brief(Ash.Resource.record()) :: {:ok, Oban.Job.t()} | {:error, term()}
  def enqueue_brief(brief) do
    Oban.insert(
      ExAutoresearch.Workers.ResearchWorker.new(
        %{"brief_id" => brief.id},
        queue: :research
      )
    )
  end

  @doc """
  Creates a Report from a Brief (with auto-incremented run_version) and runs
  the full pipeline synchronously. Mirrors run_from_template/1.
  """
  @spec run_from_brief(Ash.Resource.record()) ::
          {:ok, Ash.Resource.record()} | {:error, term()}
  def run_from_brief(brief) do
    v = next_run_version(brief)
    tenant = brief.organization_id

    params = %{
      title: "#{brief.name} — v#{v}",
      query: brief.question,
      model: brief.model,
      max_depth: brief.max_depth,
      max_sources: brief.max_sources,
      category: brief.category,
      organization_id: tenant,
      brief_id: brief.id,
      run_version: v
    }

    opts = if tenant, do: [action: :start, tenant: tenant], else: [action: :start]

    report = Ash.create!(Research.Report, params, opts)
    Ash.update!(report, %{status: :researching}, action: :update_status)
    broadcast({:status_changed, %{status: :researching, report_id: report.id}})

    run(report, allow_domains: brief.allow_domains, block_domains: brief.block_domains)
  end

  @doc """
  Creates a Report from a Template and runs the full pipeline synchronously.
  """
  @spec run_from_template(Ash.Resource.record()) ::
          {:ok, Ash.Resource.record()} | {:error, term()}
  def run_from_template(template) do
    title = "#{template.name} - #{Date.utc_today()}"
    tenant = template.organization_id

    base = %{
      title: title,
      query: template.query_template,
      model: template.model,
      max_depth: template.max_depth,
      max_sources: template.max_sources,
      category: template.category
    }

    params =
      case tenant do
        nil -> base
        org_id -> Map.put(base, :organization_id, org_id)
      end

    report = Ash.create!(Research.Report, params, action: :start, tenant: tenant)
    Ash.update!(report, %{status: :researching}, action: :update_status)
    broadcast({:status_changed, %{status: :researching, report_id: report.id}})

    run(report, [])
  end

  @doc """
  Checks whether the report is still active. Returns `{:ok, fresh_report}` if it
  should continue, or `:halted` if it has already been completed/failed externally.
  """
  @spec continue?(Ash.Resource.record()) ::
          {:ok, Ash.Resource.record()} | :halted
  def continue?(report) do
    opts = if report.organization_id, do: [tenant: report.organization_id], else: []

    case Ash.get(Research.Report, report.id, opts) do
      {:ok, fresh} when fresh.status in [:completed, :failed] -> :halted
      {:ok, fresh} -> {:ok, fresh}
      _ -> :halted
    end
  end

  @doc """
  Returns the status of the most-recent active report, or `%{status: :idle, report_id: nil}`.
  """
  @spec status() :: %{status: atom(), report_id: binary() | nil}
  def status do
    active_statuses = [:pending, :researching, :analyzing, :writing]

    result =
      try do
        Research.Report
        |> Ash.Query.filter(status in ^active_statuses)
        |> Ash.Query.sort(inserted_at: :desc)
        |> Ash.Query.limit(1)
        |> Ash.read!()
        |> List.first()
      rescue
        _ -> nil
      end

    case result do
      nil -> %{status: :idle, report_id: nil}
      report -> %{status: report.status, report_id: report.id}
    end
  end

  @doc """
  Loads a report with its investigations sorted by inserted_at ascending.
  Returns `{:ok, report_with_investigations}` or `:error`.
  """
  @spec report_detail(binary()) :: {:ok, Ash.Resource.record()} | :error
  def report_detail(id) do
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
  end

  @doc """
  Finds the current active report and marks it completed, then broadcasts idle.
  """
  @spec stop_research() :: :ok
  def stop_research do
    active_statuses = [:pending, :researching, :analyzing, :writing]

    report =
      try do
        Research.Report
        |> Ash.Query.filter(status in ^active_statuses)
        |> Ash.Query.sort(inserted_at: :desc)
        |> Ash.Query.limit(1)
        |> Ash.read!()
        |> List.first()
      rescue
        _ -> nil
      end

    if report do
      try do
        Ash.update!(report, %{status: :completed}, action: :update_status)
      rescue
        _ -> :ok
      end
    end

    broadcast({:status_changed, %{status: :idle}})
    :ok
  end

  @doc """
  Pure function. Given a list of quality scores from completed investigations,
  returns `:low_quality`, `:diminishing_returns`, or `nil`.

  - `:low_quality`           — ≥3 scores and overall avg < 0.3
  - `:diminishing_returns`   — ≥3 scores, avg ≥ 0.3, but last-5 avg < 0.15
  - `nil`                    — not enough data or quality is fine
  """
  @spec quality_alert?([number()]) :: :low_quality | :diminishing_returns | nil
  def quality_alert?(scores) when length(scores) >= 3 do
    avg = Enum.sum(scores) / length(scores)

    if avg < 0.3 do
      :low_quality
    else
      recent = Enum.take(scores, -5)

      if length(recent) >= 3 do
        recent_avg = Enum.sum(recent) / length(recent)
        if recent_avg < 0.15, do: :diminishing_returns, else: nil
      else
        nil
      end
    end
  end

  def quality_alert?(_scores), do: nil

  # ── Pipeline helpers ───────────────────────────────────────────────────────

  defp do_plan(report) do
    do_plan(report, 0)
  end

  defp do_plan(report, attempt) when attempt >= @max_plan_retries do
    Logger.error("[Research] Planning failed after #{@max_plan_retries} retries")
    fail_report(report, "Failed to generate research plan")
    {:error, :planning_failed}
  end

  defp do_plan(report, attempt) do
    broadcast({:research_step, %{step: "planning", report_id: report.id}})
    update_progress(report, progress_for(:planning))

    case generate_plan(report.query, report.max_depth,
           report_id: report.id,
           organization_id: report.organization_id
         ) do
      {:ok, queries} ->
        {:ok, queries, report}

      :error ->
        Logger.warning("[Research] Planning failed (attempt #{attempt + 1}), retrying in 2s")
        Process.sleep(2_000)
        do_plan(report, attempt + 1)
    end
  end

  defp unwrap_analysis({:ok, deeper, summary}), do: {deeper, summary}
  defp unwrap_analysis(:error), do: {[], "Limited findings available"}
  defp unwrap_analysis(_), do: {[], ""}

  defp deepen?(deeper_queries, quality_signal, investigations, report) do
    has_deeper = deeper_queries != []
    no_quality_block = quality_signal not in [:low_quality, :diminishing_returns]
    under_limit = length(investigations) < report.max_sources

    has_deeper and no_quality_block and under_limit
  end

  # ── Phase implementations (moved verbatim from orchestrator) ──────────────

  defp generate_plan(query, max_depth, llm_opts) do
    n = min(max_depth * 3, 9)

    prompt = """
    You are a deep research assistant. Given this research question:

    "#{query}"

    Generate a list of #{n} search queries that will help answer this question comprehensively.

    Format your response as a JSON array of strings ONLY (no other output):
    ["query 1", "query 2", "query 3"]
    """

    with {:ok, body} <-
           LLMClient.complete(prompt, [tier: :main, timeout: @plan_timeout] ++ llm_opts),
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

  defp run_investigations(report, queries, domain_opts) do
    threads = Application.get_env(:ex_autoresearch, :research, [])[:max_threads] || 5

    queries
    |> Enum.chunk_every(threads)
    |> Enum.flat_map(fn batch ->
      batch
      |> Enum.map(&Task.async(fn -> run_one_investigation(report, &1, domain_opts) end))
      |> Task.await_many(@search_timeout)
    end)
  end

  defp run_one_investigation(report, query, domain_opts) do
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

    runner_opts =
      [report_id: report.id, investigation_id: inv.id] ++ domain_opts

    case ResearchRunner.run(query, :search, runner_opts) do
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
    first_source = List.first(findings.sources) || %{}

    Ash.update!(
      inv,
      %{
        status: :completed,
        findings: findings.content,
        quality_score: findings.quality_score,
        sources_count: length(findings.sources),
        url: Map.get(first_source, :url),
        fetched_at: Map.get(first_source, :fetched_at),
        content_hash: Map.get(first_source, :content_hash),
        scraper_source: Map.get(first_source, :scraper_source, :unknown),
        fallback_used: Map.get(first_source, :fallback_used, false)
      },
      action: :complete
    )

    Enum.each(findings.sources, fn s ->
      try do
        Ash.create!(
          Research.Source,
          %{
            report_id: report.id,
            investigation_id: inv.id,
            url: s.url,
            domain: s[:domain],
            title: s[:title],
            fetched_at: s[:fetched_at],
            content_hash: s[:content_hash],
            relevance_score: s[:relevance_score],
            scraper_source: s[:scraper_source] || :unknown
          },
          action: :create
        )
      rescue
        e ->
          Logger.warning(
            "[Research] Failed to persist source row for #{s.url}: #{Exception.message(e)}"
          )
      end
    end)

    maybe_broadcast_learning(report, inv, findings, query, first_source)

    broadcast(
      {:investigation_completed,
       %{investigation_id: inv.id, quality_score: findings.quality_score}}
    )

    %{
      id: inv.id,
      query: query,
      findings: findings.content,
      quality_score: findings.quality_score,
      status: :completed
    }
  end

  defp maybe_broadcast_learning(report, inv, findings, query, first_source) do
    score = findings.quality_score

    if is_number(score) and score >= 0.3 do
      excerpt =
        findings.content
        |> to_string()
        |> String.slice(0, 200)

      broadcast(
        {:learning_added,
         %{
           report_id: report.id,
           investigation_id: inv.id,
           query: query,
           quality_score: score,
           reasoning: inv.reasoning,
           findings_excerpt: excerpt,
           url: Map.get(first_source, :url)
         }}
      )
    end

    :ok
  end

  defp analyze(question, investigations, max_depth, llm_opts) do
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
               LLMClient.complete(prompt, [tier: :main, timeout: @analyze_timeout] ++ llm_opts),
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
    numbered = SourcesBlock.numbered_sources(report)

    {findings_text, citation_instruction} =
      case numbered do
        [] ->
          successful = filter_completed(investigations)
          {format_synthesis(successful), ""}

        pairs ->
          text =
            pairs
            |> Enum.map_join("\n\n---\n\n", fn {idx, inv} ->
              "[#{idx}] #{inv.query} — #{inv.url}\n#{inv.findings}"
            end)

          instruction = """
          Use inline citations in the format [1], [2], etc. to reference the numbered \
          sources above. Do NOT write your own Sources or References section — a \
          ## Sources block is appended automatically after your report.
          """

          {text, instruction}
      end

    prompt = """
    Write a comprehensive research report answering this question:

    "#{report.query}"

    Here are the research findings:

    #{findings_text}

    Format your response as a well-structured markdown report with:
    1. Executive Summary
    2. Detailed Analysis (organized by topic)
    3. Key Findings
    #{citation_instruction}
    """

    case LLMClient.complete(prompt,
           tier: :main,
           timeout: @synth_timeout,
           report_id: report.id,
           organization_id: report.organization_id
         ) do
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

  defp update_report_complete(report, body, _source_count, progress) do
    total_sources =
      try do
        Research.Source
        |> Ash.Query.filter(report_id == ^report.id)
        |> Ash.read!()
        |> length()
      rescue
        _ -> 0
      end

    total_investigations =
      try do
        Research.Investigation
        |> Ash.Query.filter(report_id == ^report.id and status == :completed)
        |> Ash.read!()
        |> length()
      rescue
        _ -> 0
      end

    report =
      try do
        Ash.update!(
          report,
          %{
            total_sources: total_sources,
            total_investigations: total_investigations
          },
          action: :update_result
        )
      rescue
        _ -> report
      end

    final_body = body <> SourcesBlock.build(report)

    Ash.update!(
      report,
      %{
        status: :completed,
        markdown_body: final_body,
        progress_pct: progress,
        summary: "#{total_sources} sources analyzed"
      },
      action: :complete
    )

    if report.organization_id do
      ExAutoresearch.Audit.record!(:report_completed, %{
        organization_id: report.organization_id,
        target_type: :report,
        target_id: report.id,
        summary: String.slice(report.title || "", 0, 200),
        metadata: %{
          total_sources: total_sources,
          total_investigations: total_investigations,
          final_score: report.final_score
        }
      })
    end
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

    if report.organization_id do
      ExAutoresearch.Audit.record!(:report_failed, %{
        organization_id: report.organization_id,
        target_type: :report,
        target_id: report.id,
        summary: String.slice(report.title || "", 0, 200),
        metadata: %{reason: to_string(reason)}
      })
    end
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

  defp progress_for(:planning), do: 5
  defp progress_for(:searching), do: 10
  defp progress_for(:analyzing), do: 50
  defp progress_for(:deepening), do: 70
  defp progress_for(:writing), do: 90
  defp progress_for(:verifying), do: 95
  defp progress_for(_), do: nil
end
