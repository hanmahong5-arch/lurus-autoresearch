defmodule ExAutoresearch.DeepResearch.Search do
  @moduledoc """
  Web search behaviour + dispatcher.

  Implementations are swappable via `:ex_autoresearch, :search_backend` app env:

    * `Search.DuckDuckGo` — zero-config HTML scrape of DuckDuckGo (default)
    * `Search.Serper`     — Google results via serper.dev (requires API key)

  Public `search/2` always wraps the call in telemetry and auto-falls back to
  DuckDuckGo when the primary backend fails (unless primary IS DuckDuckGo).
  """

  alias ExAutoresearch.Tools.ProviderRunner

  @type result :: %{title: String.t(), url: String.t(), snippet: String.t()}

  @callback search(query :: String.t(), opts :: keyword()) ::
              {:ok, [result()]} | {:error, term()}

  @default_backend ExAutoresearch.DeepResearch.Search.DuckDuckGo
  @fallback_backend ExAutoresearch.DeepResearch.Search.DuckDuckGo

  @spec search(String.t(), keyword()) :: {:ok, [result()]} | {:error, term()}
  def search(query, opts \\ []) do
    primary = active_backend()

    base_metadata = %{query: query, backend: primary}

    ProviderRunner.run(
      primary,
      @fallback_backend,
      :search,
      [query, opts],
      [:ex_autoresearch, :search, :fetch],
      base_metadata
    )
  end

  defp active_backend do
    Application.get_env(:ex_autoresearch, :search_backend, @default_backend)
  end
end
