defmodule ExAutoresearch.Workers.BriefScheduleWorker do
  @moduledoc """
  Oban cron worker that fires every minute and enqueues research jobs for
  enabled, non-manual Briefs whose `next_run_at` is due.

  Replaces `ExAutoresearch.Workers.TemplateScheduler` for brief-based
  scheduling. Template-cron scheduling is intentionally superseded; the
  TemplateLive one-click launch still works independently.
  """

  use Oban.Worker, queue: :default, max_attempts: 1

  require Logger
  require Ash.Query

  alias ExAutoresearch.Research.Brief
  alias ExAutoresearch.DeepResearch.ResearchEngine

  @impl Oban.Worker
  def perform(%Oban.Job{}) do
    now = DateTime.utc_now()

    due_briefs =
      Brief
      |> Ash.Query.filter(enabled == true and cadence != :manual)
      |> Ash.Query.filter(is_nil(next_run_at) or next_run_at <= ^now)
      |> Ash.read!()

    for brief <- due_briefs do
      try do
        {:ok, _job} = ResearchEngine.enqueue_brief(brief)

        next_run_at = DateTime.add(now, seconds_for(brief.cadence), :second)

        Ash.update!(brief, %{last_run_at: now, next_run_at: next_run_at},
          action: :mark_ran,
          tenant: brief.organization_id
        )

        Logger.info(
          "[BriefScheduleWorker] Enqueued brief #{brief.id} (#{brief.name}); next_run_at=#{next_run_at}"
        )
      rescue
        e ->
          Logger.error(
            "[BriefScheduleWorker] Failed to handle brief #{brief.id}: #{Exception.message(e)}"
          )
      end
    end

    :ok
  end

  defp seconds_for(:hourly), do: 3_600
  defp seconds_for(:daily), do: 86_400
  defp seconds_for(:weekly), do: 604_800
  defp seconds_for(_), do: 86_400
end
