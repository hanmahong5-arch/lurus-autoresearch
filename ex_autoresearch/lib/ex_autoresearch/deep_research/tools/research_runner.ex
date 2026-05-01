defmodule ExAutoresearch.DeepResearch.Tools.ResearchRunner do
  @moduledoc """
  Executes a single research investigation:
  - :search -> Web search via Serper API
  - :fetch -> Fetch and extract web page content
  - :analyze -> LLM-based content analysis (handled by orchestrator)

  Returns findings with quality score.
  """

  require Logger

  alias ExAutoresearch.DeepResearch.Scraper
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
          case Scraper.fetch(result.url, Keyword.take(opts, [:report_id, :investigation_id])) do
            {:ok, %{markdown: markdown}} ->
              %{
                title: result.title,
                url: result.url,
                snippet: result.snippet,
                content: String.slice(markdown, 0, 2000)
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

  defp do_run(url, :fetch, opts) when is_binary(url) do
    case Scraper.fetch(url, Keyword.take(opts, [:report_id, :investigation_id])) do
      {:ok, %{markdown: markdown}} -> {:ok, markdown}
      {:error, _} = err -> err
    end
  end

  defp do_run(_input, tool, _opts), do: {:error, {:unsupported_tool, tool}}

  # --- Private helpers ---

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
