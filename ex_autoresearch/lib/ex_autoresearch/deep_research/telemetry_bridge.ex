defmodule ExAutoresearch.DeepResearch.TelemetryBridge do
  @moduledoc """
  Bridges scraper telemetry stop events to PubSub for LiveView consumption.
  Call `attach/0` once at application startup (idempotent).
  """

  @handler_id "ex-autoresearch-scraper-bridge"
  @events [
    [:ex_autoresearch, :scraper, :fetch, :stop],
    [:ex_autoresearch, :relevance, :fallback]
  ]

  @doc """
  Attach the telemetry handler. Idempotent: detaches first if already registered.
  Returns `:ok`.
  """
  def attach do
    :telemetry.detach(@handler_id)
    :telemetry.attach_many(@handler_id, @events, &__MODULE__.handle_event/4, nil)
    :ok
  end

  def handle_event(
        [:ex_autoresearch, :scraper, :fetch, :stop],
        measurements,
        metadata,
        _config
      ) do
    try do
      report_id = Map.get(metadata, :report_id)

      if is_nil(report_id) do
        :ok
      else
        duration_ms = System.convert_time_unit(measurements.duration, :native, :millisecond)

        payload = %{
          report_id: report_id,
          url: metadata.url,
          outcome: metadata.outcome,
          primary_impl: metadata.primary_impl,
          primary_error: Map.get(metadata, :primary_error),
          native_error: Map.get(metadata, :native_error),
          duration_ms: duration_ms,
          fallback_used?: metadata.outcome == :fallback_success,
          message: human_message(Map.put(metadata, :duration_ms, duration_ms))
        }

        Phoenix.PubSub.broadcast(
          ExAutoresearch.PubSub,
          "research:events",
          {:scraper_progress, payload}
        )
      end
    rescue
      _ -> :ok
    end
  end

  def handle_event(
        [:ex_autoresearch, :relevance, :fallback],
        _measurements,
        metadata,
        _config
      ) do
    try do
      report_id = Map.get(metadata, :report_id)

      if is_nil(report_id) do
        :ok
      else
        Phoenix.PubSub.broadcast(
          ExAutoresearch.PubSub,
          "research:events",
          {:research_progress, report_id,
           %{
             stage: :analyze,
             status: :degraded,
             detail: "Relevance judge unavailable — using byte-count heuristic for source quality"
           }}
        )
      end
    rescue
      _ -> :ok
    end
  end

  defp human_message(%{
         outcome: :primary_success,
         primary_impl: impl,
         duration_ms: duration_ms
       }) do
    "Fetched via #{short_name(impl)} (#{duration_ms}ms)"
  end

  defp human_message(%{
         outcome: :fallback_success,
         primary_impl: impl,
         primary_error: primary_error
       }) do
    "#{short_name(impl)} failed (#{format_error(primary_error)}), fell back to native scraper"
  end

  defp human_message(%{outcome: :both_failed, primary_impl: impl}) do
    "Both #{short_name(impl)} and native scraper failed"
  end

  defp human_message(%{outcome: :native_failed, primary_error: primary_error}) do
    "Native scraper failed: #{format_error(primary_error)}"
  end

  defp human_message(%{outcome: outcome}) do
    "Scraper outcome: #{outcome}"
  end

  defp short_name(module) do
    module_str = to_string(module)
    prefix = "Elixir.ExAutoresearch.DeepResearch.Scraper."

    if String.starts_with?(module_str, prefix) do
      segment = String.replace_prefix(module_str, prefix, "")

      case segment do
        "Crawl4ai" -> "Crawl4AI"
        other -> other
      end
    else
      module_str
      |> String.split(".")
      |> List.last()
    end
  end

  defp format_error({:http_error, n}), do: "HTTP #{n}"
  defp format_error({:crawl4ai_failed, inner}), do: "crawl4ai failed (#{format_error(inner)})"
  defp format_error(atom) when is_atom(atom), do: to_string(atom)

  defp format_error(other) do
    other |> inspect() |> String.slice(0, 80)
  end
end
