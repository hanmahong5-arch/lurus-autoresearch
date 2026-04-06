defmodule ExAutoresearch.Workers.TemplateScheduler do
  @moduledoc """
  GenServer that watches enabled templates and writes scheduled
  research Oban jobs. Runs periodically on app startup.
  """

  use GenServer

  require Logger

  alias ExAutoresearch.Research.Template
  alias ExAutoresearch.Workers.ResearchWorker

  @check_interval :timer.minutes(5)

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @impl true
  def init(_opts) do
    schedule_next_check()
    {:ok, %{scheduled_template_ids: MapSet.new()}}
  end

  @impl true
  def handle_info(:check_templates, state) do
    state = schedule_enabled_templates(state)
    schedule_next_check()
    {:noreply, state}
  end

  defp schedule_next_check do
    Process.send_after(self(), :check_templates, @check_interval)
  end

  defp schedule_enabled_templates(state) do
    case Ash.read(Template) do
      {:ok, templates} ->
        enabled =
          templates
          |> Enum.filter(fn t -> t.enabled && t.schedule_cron && t.schedule_cron != "" end)

        for template <- enabled,
            not MapSet.member?(state.scheduled_template_ids, template.id),
            reduce: state do
          acc ->
            Logger.info(
              "[TemplateScheduler] Scheduling #{template.name} with cron: #{template.schedule_cron}"
            )

            Oban.insert(
              ResearchWorker.new(
                %{
                  "template_id" => template.id,
                  "organization_id" => template.organization_id
                },
                cron: template.schedule_cron,
                queue: :research
              )
            )

            %{acc | scheduled_template_ids: MapSet.put(acc.scheduled_template_ids, template.id)}
        end

      {:error, reason} ->
        Logger.error("[TemplateScheduler] Failed to read templates: #{inspect(reason)}")
        state
    end
  end
end
