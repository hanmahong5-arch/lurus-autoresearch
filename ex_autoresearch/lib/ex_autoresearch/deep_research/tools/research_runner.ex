defmodule ExAutoresearch.DeepResearch.Tools.ResearchRunner do
  @moduledoc """
  Executes a single research investigation:
  - :search -> Web search via Serper API
  - :fetch -> Fetch and extract web page content
  - :analyze -> LLM-based content analysis (handled by orchestrator)

  Returns findings with quality score and per-source relevance scores.
  """

  require Logger

  alias ExAutoresearch.DeepResearch.Scraper
  alias ExAutoresearch.DeepResearch.Tools.Search
  alias ExAutoresearch.Agent.LLMClient

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

  @doc """
  Filters a list of search result maps by allow/block domain policy.

  - `opts[:allow_domains]` — if non-empty, keep only results whose host is in this list.
  - `opts[:block_domains]` — drop results whose host is in this list.
  - A nil host is kept when allow is empty (no restriction); dropped when allow is non-empty.
  - Empty allow + empty block returns the input unchanged.
  """
  @spec filter_domains([map()], keyword()) :: [map()]
  def filter_domains(results, opts) do
    allow = opts[:allow_domains] || []
    block = opts[:block_domains] || []

    if allow == [] and block == [] do
      results
    else
      Enum.filter(results, fn result ->
        host =
          case URI.parse(result[:url] || result["url"] || "") do
            %URI{host: h} when is_binary(h) -> h
            _ -> nil
          end

        not_blocked = host not in block
        allowed = allow == [] or (host != nil and host in allow)

        not_blocked and allowed
      end)
    end
  end

  @doc """
  Batch LLM relevance judge for a list of content maps against a query.

  Makes ONE LLMClient.complete/2 call and asks for a JSON array of
  `{"index": i, "relevance": 0.0-1.0}` objects.

  Returns a list of floats aligned to the `contents` list order, defaulting
  to 0.5 for any missing index. Returns `:error` on any LLM or parse failure.
  """
  @spec relevance_scores(String.t(), [map()], keyword()) :: [float()] | :error
  def relevance_scores(query, contents, opts \\ []) do
    numbered =
      contents
      |> Enum.with_index(1)
      |> Enum.map_join("\n\n", fn {item, idx} ->
        title = item[:title] || "(no title)"
        body = String.slice(item[:content] || item[:snippet] || "", 0, 1500)
        "#{idx}. Title: #{title}\nContent: #{body}"
      end)

    prompt = """
    You are a relevance judge. For each numbered source below, rate its relevance \
    to the research query on a scale of 0.0 (completely irrelevant) to 1.0 (highly relevant).

    Research query: #{query}

    Sources:
    #{numbered}

    Respond with a JSON array ONLY (no other text):
    [{"index": 1, "relevance": 0.85}, {"index": 2, "relevance": 0.40}, ...]
    """

    llm_opts =
      [tier: :main, timeout: 60_000] ++
        Keyword.take(opts, [:report_id, :investigation_id])

    case LLMClient.complete(prompt, llm_opts) do
      {:ok, body} -> parse_relevance_scores(body, length(contents))
      {:error, _reason} -> :error
    end
  end

  @doc """
  Parse a JSON array of `{"index": i, "relevance": f}` objects into a list of
  floats aligned to `count` positions (1-indexed from LLM, 0-indexed in result).
  Missing entries default to 0.5. Returns `:error` if the JSON cannot be parsed.
  """
  @spec parse_relevance_scores(String.t(), non_neg_integer()) :: [float()] | :error
  def parse_relevance_scores(body, count) when is_binary(body) and count >= 0 do
    with [json] <- Regex.run(~r/\[.*\]/s, body),
         {:ok, list} when is_list(list) <- Jason.decode(json) do
      index_map =
        list
        |> Enum.reduce(%{}, fn
          %{"index" => idx, "relevance" => r}, acc
          when is_integer(idx) and is_number(r) ->
            Map.put(acc, idx, min(max(r / 1.0, 0.0), 1.0))

          _, acc ->
            acc
        end)

      Enum.map(1..count//1, fn i -> Map.get(index_map, i, 0.5) end)
    else
      _ -> :error
    end
  end

  def parse_relevance_scores(_, _), do: :error

  defp do_run(query, :search, opts) do
    num_results = Keyword.get(opts, :num_results, 10)

    with {:ok, results} <- Search.search(query, num_results: num_results) do
      max_content_chars =
        Application.get_env(:ex_autoresearch, :research, [])[:max_content_chars] || 8000

      contents =
        results
        |> filter_domains(opts)
        |> Enum.take(5)
        |> Enum.map(fn result ->
          case Scraper.fetch(result.url, Keyword.take(opts, [:report_id, :investigation_id])) do
            {:ok, %{markdown: markdown, metadata: meta}} ->
              %{
                title: result.title,
                url: result.url,
                snippet: result.snippet,
                content: String.slice(markdown, 0, max_content_chars),
                fetched_at: Map.get(meta, :fetched_at) || DateTime.utc_now(),
                content_hash: :crypto.hash(:sha256, markdown) |> Base.encode16(case: :lower),
                scraper_source: Map.get(meta, :source, :unknown),
                fallback_used: Map.get(meta, :fallback_used, false)
              }

            {:error, _} ->
              %{
                title: result.title,
                url: result.url,
                snippet: result.snippet,
                content: result.snippet,
                fetched_at: DateTime.utc_now(),
                content_hash: nil,
                scraper_source: :unknown,
                fallback_used: false
              }
          end
        end)

      {contents_with_relevance, quality} =
        case relevance_scores(query, contents, opts) do
          :error ->
            :telemetry.execute(
              [:ex_autoresearch, :relevance, :fallback],
              %{source_count: length(contents)},
              %{
                report_id: opts[:report_id],
                investigation_id: opts[:investigation_id],
                query: query,
                reason: :relevance_judge_unavailable
              }
            )

            tagged = Enum.map(contents, &Map.put(&1, :relevance_score, nil))
            {tagged, compute_quality(contents, query)}

          scores ->
            tagged =
              contents
              |> Enum.zip(scores)
              |> Enum.map(fn {item, score} -> Map.put(item, :relevance_score, score) end)

            avg_quality =
              if length(scores) > 0,
                do: Enum.sum(scores) / length(scores),
                else: 0.0

            {tagged, avg_quality}
        end

      contents_with_domain =
        Enum.map(contents_with_relevance, fn item ->
          domain =
            case URI.parse(item.url) do
              %URI{host: host} when is_binary(host) -> host
              _ -> nil
            end

          Map.put(item, :domain, domain)
        end)

      {:ok,
       %{
         content: format_findings(contents_with_domain),
         sources:
           Enum.map(
             contents_with_domain,
             &Map.take(&1, [
               :title,
               :url,
               :fetched_at,
               :content_hash,
               :scraper_source,
               :fallback_used,
               :domain,
               :relevance_score
             ])
           ),
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
        total_bytes =
          Enum.reduce(items, 0, fn item, acc ->
            acc + byte_size(item.content || item.snippet || "")
          end)

        avg_bytes = total_bytes / length(items)
        min(avg_bytes / 2000.0, 1.0)
    end
  end
end
