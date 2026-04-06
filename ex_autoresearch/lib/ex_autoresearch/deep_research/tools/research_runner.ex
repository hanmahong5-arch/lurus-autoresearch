defmodule ExAutoresearch.DeepResearch.Tools.ResearchRunner do
  @moduledoc """
  Executes a single research investigation:
  - :search -> Web search via Serper API
  - :fetch -> Fetch and extract web page content
  - :analyze -> LLM-based content analysis (handled by orchestrator)

  Returns findings with quality score.
  """

  require Logger

  alias ExAutoresearch.DeepResearch.Tools.Search

  @type result :: %{
          content: String.t() | nil,
          sources: [map()],
          quality_score: float()
        }

  @doc """
  Run a research investigation.
  """
  @spec run(String.t(), atom()) :: {:ok, result()} | {:error, term()}
  def run(query, tool), do: do_run(query, tool, [])

  @spec run(String.t(), atom(), keyword()) :: {:ok, result()} | {:error, term()}
  def run(query, tool, opts), do: do_run(query, tool, opts)

  defp do_run(query, :search, opts) do
    num_results = Keyword.get(opts, :num_results, 10)

    with {:ok, results} <- Search.search(query, num_results: num_results) do
      contents =
        results
        |> Enum.take(5)
        |> Enum.map(fn result ->
          case fetch_page_content(result.url) do
            {:ok, content} ->
              %{
                title: result.title,
                url: result.url,
                snippet: result.snippet,
                content: String.slice(content, 0, 2000)
              }

            {:error, _} ->
              %{
                title: result.title,
                url: result.url,
                snippet: result.snippet,
                content: result.snippet
              }
          end
        end)

      quality = compute_quality(contents, query)

      {:ok,
       %{
         content: format_findings(contents),
         sources: Enum.map(contents, &%{title: &1.title, url: &1.url}),
         quality_score: quality
       }}
    end
  end

  defp do_run(url, :fetch, _opts) when is_binary(url) do
    fetch_page_content(url)
  end

  defp do_run(_input, tool, _opts), do: {:error, {:unsupported_tool, tool}}

  # --- Private helpers ---

  defp fetch_page_content(url, timeout \\ 10_000) do
    case Req.get(url,
           follow_redirects: true,
           max_redirects: 5,
           timeout: timeout,
           headers: %{"User-Agent" => "Mozilla/5.0 (compatible; ResearchBot/1.0)"}
         ) do
      {:ok, %Req.Response{status: 200, body: html} = resp} ->
        content_type =
          resp.headers
          |> Map.get("content-type", [])
          |> List.first("")

        if String.contains?(content_type, "text/html") or String.contains?(content_type, "html") do
          {:ok, extract_text_from_html(html)}
        else
          {:ok, String.slice(to_string(html), 0, 5000)}
        end

      {:ok, %Req.Response{status: status}} ->
        {:error, {:http_error, status}}

      {:error, reason} ->
        {:error, {:fetch_failed, reason}}
    end
  rescue
    e -> {:error, {:fetch_error, Exception.message(e)}}
  end

  defp extract_text_from_html(html) do
    html
    |> String.replace(~r/<script[^>]*>.*?<\/script>[\s.]*/si, " ")
    |> String.replace(~r/<style[^>]*>.*?<\/style>[\s.]*/si, " ")
    |> String.replace(~r/<[^>]+>/, " ")
    |> String.replace(~r/\s+/, " ")
    |> String.trim()
    |> String.slice(0, 10_000)
  end

  defp format_findings(contents) do
    contents
    |> Enum.map_join("\n\n---\n\n", fn item ->
      "## #{item.title}\n#{item.url}\n\n#{item.content || item.snippet}"
    end)
  end

  defp compute_quality(contents, _query) do
    case contents do
      [] ->
        0.0

      items ->
        total_bytes = Enum.reduce(items, 0, fn item, acc ->
          acc + byte_size(item.content || item.snippet || "")
        end)

        avg_bytes = total_bytes / length(items)
        min(avg_bytes / 2000.0, 1.0)
    end
  end
end
