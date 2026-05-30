defmodule ExAutoresearch.Workers.ResearchWorker do
  @moduledoc """
  Oban worker that executes a research task synchronously via ResearchEngine.

  Accepts two arg shapes:
  - `%{"report_id" => id}` — run a report that has already been created (e.g. via ResearchEngine.start/1)
  - `%{"template_id" => id, "organization_id" => _}` — create a report from a template and run it
  """

  use Oban.Worker, queue: :default, max_attempts: 3

  require Logger

  alias ExAutoresearch.{Research, DeepResearch}
  alias DeepResearch.ResearchEngine
  alias ExAutoresearch.Workers.DeltaWorker

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"report_id" => report_id}}) do
    Logger.info("[ResearchWorker] Running report #{report_id}")

    case Ash.get(Research.Report, report_id) do
      {:ok, report} ->
        case ResearchEngine.run(report, []) do
          {:ok, completed_report} ->
            try do
              ExAutoresearch.Notifications.Notifier.report_completed(completed_report)
            rescue
              e ->
                Logger.warning(
                  "[ResearchWorker] Notifier failed for report #{report_id}: #{Exception.message(e)}"
                )
            end

            maybe_enqueue_delta(completed_report)
            :ok

          {:error, reason} ->
            Logger.error(
              "[ResearchWorker] Research failed for report #{report_id}: #{inspect(reason)}"
            )

            {:error, reason}
        end

      {:error, reason} ->
        Logger.error("[ResearchWorker] Could not load report #{report_id}: #{inspect(reason)}")
        {:error, reason}
    end
  end

  def perform(%Oban.Job{args: %{"brief_id" => brief_id}}) do
    Logger.info("[ResearchWorker] Starting research for brief #{brief_id}")

    case Ash.get(ExAutoresearch.Research.Brief, brief_id) do
      {:ok, brief} ->
        case ResearchEngine.run_from_brief(brief) do
          {:ok, completed_report} ->
            try do
              ExAutoresearch.Notifications.Notifier.report_completed(completed_report)
            rescue
              e ->
                Logger.warning(
                  "[ResearchWorker] Notifier failed for brief #{brief_id}: #{Exception.message(e)}"
                )
            end

            maybe_enqueue_delta(completed_report)
            :ok

          {:error, reason} ->
            Logger.error("[ResearchWorker] Brief research failed #{brief_id}: #{inspect(reason)}")

            {:error, reason}
        end

      {:error, reason} ->
        Logger.error("[ResearchWorker] Could not load brief #{brief_id}: #{inspect(reason)}")
        {:error, reason}
    end
  end

  def perform(%Oban.Job{args: %{"template_id" => template_id, "organization_id" => _org_id}}) do
    Logger.info("[ResearchWorker] Starting research for template #{template_id}")

    case Ash.get(Research.Template, template_id) do
      {:ok, template} ->
        case ResearchEngine.run_from_template(template) do
          {:ok, completed_report} ->
            try do
              ExAutoresearch.Notifications.Notifier.report_completed(completed_report)
            rescue
              e ->
                Logger.warning(
                  "[ResearchWorker] Notifier failed for template #{template_id}: #{Exception.message(e)}"
                )
            end

            maybe_enqueue_delta(completed_report)
            :ok

          {:error, reason} ->
            Logger.error(
              "[ResearchWorker] Template research failed #{template_id}: #{inspect(reason)}"
            )

            {:error, reason}
        end

      {:error, reason} ->
        Logger.error(
          "[ResearchWorker] Could not load template #{template_id}: #{inspect(reason)}"
        )

        {:error, reason}
    end
  end

  # Enqueue DeltaWorker when the completed report is linked to a brief
  defp maybe_enqueue_delta(%{brief_id: nil}), do: :ok

  defp maybe_enqueue_delta(%{brief_id: brief_id, id: report_id, organization_id: org_id})
       when is_binary(brief_id) do
    Oban.insert(
      DeltaWorker.new(%{
        "to_report_id" => report_id,
        "brief_id" => brief_id,
        "organization_id" => org_id
      })
    )
  end

  defp maybe_enqueue_delta(_), do: :ok
end
