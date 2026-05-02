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

  alias ExAutoresearch.Tools.ProviderRunner

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
    fallback = ExAutoresearch.DeepResearch.Scraper.Native

    base_metadata = %{
      url: url,
      primary_impl: primary,
      report_id: Keyword.get(opts, :report_id),
      investigation_id: Keyword.get(opts, :investigation_id)
    }

    runner_opts = [
      outcome_names: %{primary_only_failed: :native_failed},
      transform_ok: fn result, outcome, primary_error ->
        mark_metadata(result, outcome == :fallback_success, primary_error)
      end,
      stop_metadata: fn meta ->
        case Map.pop(meta, :fallback_error) do
          {nil, m} -> m
          {err, m} -> Map.put(m, :native_error, err)
        end
      end
    ]

    case ProviderRunner.run(
           primary,
           fallback,
           :fetch,
           [url, opts],
           [:ex_autoresearch, :scraper, :fetch],
           base_metadata,
           runner_opts
         ) do
      {:error, {:both_failed, primary: reason, fallback: native_reason}} ->
        {:error, {:both_failed, primary: reason, native: native_reason}}

      other ->
        other
    end
  end

  defp mark_metadata(result, fallback_used?, primary_error) do
    update_in(result, [:metadata], fn meta ->
      meta
      |> Map.put(:fallback_used, fallback_used?)
      |> Map.put(:primary_error, primary_error)
    end)
  end
end
