defmodule ExAutoresearch.DeepResearch.Scraper do
  @moduledoc """
  Behaviour for web page scrapers.

  Implementations:
  - `ExAutoresearch.DeepResearch.Scraper.Native` — inline Req-based HTML extraction (fallback)
  - `ExAutoresearch.DeepResearch.Scraper.Crawl4ai` — self-hosted Crawl4AI HTTP service (default)

  The active implementation is resolved at runtime via:

      config :ex_autoresearch, :scraper, ExAutoresearch.DeepResearch.Scraper.Native
  """

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
  """
  @spec fetch(String.t(), keyword()) ::
          {:ok, %{markdown: String.t(), metadata: map()}} | {:error, term()}
  def fetch(url, opts \\ []) do
    impl = Application.fetch_env!(:ex_autoresearch, :scraper)
    impl.fetch(url, opts)
  end
end
