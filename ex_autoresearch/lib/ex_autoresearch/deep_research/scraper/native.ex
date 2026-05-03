defmodule ExAutoresearch.DeepResearch.Scraper.Native do
  @moduledoc """
  Native Req-based HTML scraper.

  Fetches a URL with `Req.get/2`, detects content-type, and strips HTML tags
  to produce a plain-text representation. Used as the fallback scraper when
  Crawl4AI is unavailable.

  ## Test support

  Pass `plug: {Req.Test, StubName}` in opts to route requests through a
  `Req.Test` stub instead of the network.
  """

  @behaviour ExAutoresearch.DeepResearch.Scraper

  require Logger

  @default_timeout 10_000
  @html_max_chars 10_000
  @plain_max_chars 5_000

  @impl ExAutoresearch.DeepResearch.Scraper
  def fetch(url, opts \\ []) do
    timeout = Keyword.get(opts, :timeout, @default_timeout)
    req_opts = build_req_opts(opts, timeout)
    do_fetch(url, req_opts)
  end

  defp build_req_opts(opts, timeout) do
    base = [
      redirect: true,
      max_redirects: 5,
      receive_timeout: timeout,
      headers: %{"User-Agent" => "Mozilla/5.0 (compatible; ResearchBot/1.0)"}
    ]

    base
    |> maybe_put(opts, :plug)
    |> maybe_put(opts, :retry)
  end

  defp maybe_put(req_opts, opts, key) do
    case Keyword.fetch(opts, key) do
      {:ok, value} -> Keyword.put(req_opts, key, value)
      :error -> req_opts
    end
  end

  defp do_fetch(url, req_opts) do
    case Req.get(url, req_opts) do
      {:ok, %Req.Response{status: 200, body: html} = resp} ->
        content_type =
          resp.headers
          |> Map.get("content-type", [])
          |> List.first("")

        text =
          if String.contains?(content_type, "text/html") or
               String.contains?(content_type, "html") do
            extract_text_from_html(html, url)
          else
            truncate_with_warning(to_string(html), @plain_max_chars, url)
          end

        {:ok, %{markdown: text, metadata: %{source: :native}}}

      {:ok, %Req.Response{status: status}} ->
        {:error, {:http_error, status}}

      {:error, reason} ->
        {:error, {:fetch_failed, reason}}
    end
  rescue
    e -> {:error, {:fetch_error, Exception.message(e)}}
  end

  @doc false
  def extract_text_from_html(html, url \\ nil) do
    html
    |> String.replace(~r/<script[^>]*>.*?<\/script>[\s.]*/si, " ")
    |> String.replace(~r/<style[^>]*>.*?<\/style>[\s.]*/si, " ")
    |> String.replace(~r/<[^>]+>/, " ")
    |> String.replace(~r/\s+/, " ")
    |> String.trim()
    |> truncate_with_warning(@html_max_chars, url)
  end

  defp truncate_with_warning(text, max, url) do
    size = byte_size(text)

    if size > max do
      Logger.warning(
        "Native scraper truncated content from #{size} to #{max} bytes" <>
          if(url, do: " for #{url}", else: "")
      )
    end

    String.slice(text, 0, max)
  end
end
