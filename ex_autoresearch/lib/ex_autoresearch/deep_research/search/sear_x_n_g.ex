defmodule ExAutoresearch.DeepResearch.Search.SearXNG do
  @moduledoc """
  Web search backed by a self-hosted [SearXNG](https://docs.searxng.org/)
  instance. SearXNG is an open-source metasearch engine that aggregates
  Google / Bing / DuckDuckGo / Wikipedia / etc and returns a clean JSON
  response, sidestepping per-source anti-bot challenges.

  ## Setup

      docker run -d --name searxng -p 8888:8080 \\
          -v searxng-config:/etc/searxng \\
          searxng/searxng:latest

  Then point this backend at it via either:

    * `SEARXNG_BASE_URL=http://localhost:8888`
    * `EX_AUTORESEARCH_SEARCH=searxng`
    * Settings → Integrations → Web Search Backend = SearXNG, Base URL = ...

  ## Notes

  SearXNG must be configured with `formats: [html, json]` in its `settings.yml`
  for the `format=json` query to work. Newer SearXNG images ship JSON disabled
  by default; either run with the `BASE_URL=... INSTANCE_NAME=...` env override
  or mount a custom `settings.yml`.
  """

  @behaviour ExAutoresearch.DeepResearch.Search

  require Logger

  @impl true
  def search(query, opts \\ []) do
    num_results = Keyword.get(opts, :num_results, 10)

    case base_url() do
      nil ->
        {:error, :missing_base_url}

      base ->
        do_search(base, query, num_results)
    end
  end

  defp do_search(base, query, num_results) do
    url = String.trim_trailing(base, "/") <> "/search"

    case Req.get(url,
           params: [q: query, format: "json", safesearch: "0", language: "en"],
           headers: %{"accept" => "application/json"},
           retry: false,
           receive_timeout: 20_000
         ) do
      {:ok, %Req.Response{status: 200, body: %{"results" => results}}} when is_list(results) ->
        {:ok, results |> Enum.map(&normalize/1) |> Enum.take(num_results)}

      {:ok, %Req.Response{status: 200, body: body}} ->
        Logger.error("SearXNG 200 but no :results key: #{inspect(body, limit: 200)}")
        {:error, :unexpected_body}

      {:ok, %Req.Response{status: status} = resp} ->
        Logger.error("SearXNG returned #{status}: #{inspect(resp.body, limit: 200)}")
        {:error, {:api_error, status}}

      {:error, reason} ->
        {:error, {:request_failed, reason}}
    end
  end

  defp normalize(item) do
    %{
      title: Map.get(item, "title", ""),
      url: Map.get(item, "url", ""),
      snippet: Map.get(item, "content", "") || Map.get(item, "snippet", "")
    }
  end

  defp base_url do
    System.get_env("SEARXNG_BASE_URL") ||
      case Application.get_env(:ex_autoresearch, :search) do
        kw when is_list(kw) -> Keyword.get(kw, :searxng_base_url)
        _ -> nil
      end
  end
end
