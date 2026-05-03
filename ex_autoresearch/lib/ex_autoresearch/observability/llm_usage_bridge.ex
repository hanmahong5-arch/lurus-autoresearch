defmodule ExAutoresearch.Observability.LLMUsageBridge do
  @moduledoc """
  Telemetry handler that accumulates LLM token usage onto the owning Report.

  Listens for `[:ex_autoresearch, :llm, :complete, :stop]` events emitted by
  `LLMClient.complete/2` and atomically updates the Report's
  `total_input_tokens`, `total_output_tokens`, and `llm_calls_count` fields.

  Requires `:report_id` and `:organization_id` in the event metadata (option A
  for multi-tenancy: callers thread these through LLM opts).
  """

  require Logger

  alias ExAutoresearch.Research

  @handler_id "ex-autoresearch-llm-usage-bridge"
  @events [[:ex_autoresearch, :llm, :complete, :stop]]

  @doc """
  Attach the telemetry handler. Idempotent — detaches first if already registered.
  """
  def attach do
    :telemetry.detach(@handler_id)
    :telemetry.attach_many(@handler_id, @events, &__MODULE__.handle_event/4, nil)
    :ok
  end

  def handle_event([:ex_autoresearch, :llm, :complete, :stop], _measurements, metadata, _config) do
    try do
      report_id = Map.get(metadata, :report_id)
      outcome = Map.get(metadata, :outcome)

      if report_id && outcome == :ok do
        input = Map.get(metadata, :input_tokens, 0)
        output = Map.get(metadata, :output_tokens, 0)
        organization_id = Map.get(metadata, :organization_id)
        accumulate(report_id, organization_id, input, output)
      else
        :ok
      end
    rescue
      e ->
        Logger.warning("[LLMUsageBridge] failed: #{Exception.message(e)}")
        :ok
    end
  end

  defp accumulate(report_id, organization_id, input, output) do
    opts =
      if organization_id do
        [tenant: organization_id, authorize?: false]
      else
        [authorize?: false]
      end

    case Ash.get(Research.Report, report_id, opts) do
      {:ok, report} when not is_nil(report) ->
        new_input = (report.total_input_tokens || 0) + input
        new_output = (report.total_output_tokens || 0) + output
        new_calls = (report.llm_calls_count || 0) + 1

        Ash.update!(
          report,
          %{
            total_input_tokens: new_input,
            total_output_tokens: new_output,
            llm_calls_count: new_calls
          },
          action: :track_llm_usage,
          authorize?: false
        )

      _ ->
        :ok
    end
  rescue
    e ->
      Logger.warning(
        "[LLMUsageBridge] accumulate failed for report #{report_id}: #{Exception.message(e)}"
      )

      :ok
  end
end
