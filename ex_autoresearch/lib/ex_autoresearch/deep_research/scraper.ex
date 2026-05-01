defmodule ExAutoresearch.DeepResearch.Scraper do
  @moduledoc """
  Behaviour for web page scrapers.

  Implementations:
  - `ExAutoresearch.DeepResearch.Scraper.Native` — inline Req-based HTML extraction (fallback)
  - `ExAutoresearch.DeepResearch.Scraper.Crawl4ai` — self-hosted Crawl4AI HTTP service (default)

  The active implementation is resolved at runtime via:

      config :ex_autoresearch, :scraper, ExAutoresearch.DeepResearch.Scraper.Native

  `fetch/2` emits telemetry via `:telemetry.span/3` on `[:ex_autoresearch, :scraper, :fetch]`.
  When the configured primary fails and is not `Native`, it automatically falls back to
  `Scraper.Native`, logging a warning. Both-failed errors are tagged `{:both_failed, ...}`.
  """

  require Logger

  @doc """
  Fetch a URL and return its text content as markdown plus metadata.

  Returns `{:ok, %{markdown: String.t(), metadata: map()}}` on success,
  or `{:error, term()}` on failure.
  """
  @callback fetch(url :: String.t(), opts :: keyword()) ::
              {:ok, %{markdown: String.t(), metadata: map()}} | {:error, term()}

  @doc """
  Delegates to the configured scraper implementation.

  The implementation is read from `Application.fetch_env!(:ex_autoresearch, :scraper)`.
  Emits telemetry events on `[:ex_autoresearch, :scraper, :fetch]` and falls back to
  `Scraper.Native` when the primary fails (unless primary IS Native).
  """
  @spec fetch(String.t(), keyword()) ::
          {:ok, %{markdown: String.t(), metadata: map()}} | {:error, term()}
  def fetch(url, opts \\ []) do
    primary = Application.fetch_env!(:ex_autoresearch, :scraper)

    metadata = %{
      url: url,
      primary_impl: primary,
      report_id: Keyword.get(opts, :report_id),
      investigation_id: Keyword.get(opts, :investigation_id)
    }

    :telemetry.span([:ex_autoresearch, :scraper, :fetch], metadata, fn ->
      case primary.fetch(url, opts) do
        {:ok, result} ->
          {{:ok, mark_metadata(result, false, nil)},
           Map.put(metadata, :outcome, :primary_success)}

        {:error, reason} when primary != ExAutoresearch.DeepResearch.Scraper.Native ->
          Logger.warning(
            "Scraper #{inspect(primary)} failed for #{url}: #{inspect(reason)}; falling back to Native"
          )

          case ExAutoresearch.DeepResearch.Scraper.Native.fetch(url, opts) do
            {:ok, result} ->
              {{:ok, mark_metadata(result, true, reason)},
               Map.merge(metadata, %{outcome: :fallback_success, primary_error: reason})}

            {:error, native_reason} ->
              Logger.warning(
                "Both primary (#{inspect(primary)}) and Native scraper failed for #{url}: #{inspect(reason)} | #{inspect(native_reason)}"
              )

              {{:error, {:both_failed, primary: reason, native: native_reason}},
               Map.merge(metadata, %{
                 outcome: :both_failed,
                 primary_error: reason,
                 native_error: native_reason
               })}
          end

        {:error, reason} ->
          Logger.warning("Native scraper failed for #{url}: #{inspect(reason)}")
          {{:error, reason}, Map.put(metadata, :outcome, :native_failed)}
      end
    end)
  end

  defp mark_metadata(result, fallback_used?, primary_error) do
    update_in(result, [:metadata], fn meta ->
      meta
      |> Map.put(:fallback_used, fallback_used?)
      |> Map.put(:primary_error, primary_error)
    end)
  end
end
