defmodule ExAutoresearch.DeepResearch.Search.DuckDuckGo do
  @moduledoc """
  Zero-config web search via DuckDuckGo's HTML endpoint.

  Hits `https://html.duckduckgo.com/html/?q=<query>` and parses the result list
  with Floki. No API key, no account, no rate-limit token. Suitable as the
  default backend for self-hosted deployments where no third-party search key
  is available.
  """

  @behaviour ExAutoresearch.DeepResearch.Search

  require Logger

  @endpoint "https://html.duckduckgo.com/html/"
  @user_agent "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36"

  @impl true
  def search(query, opts \\ []) do
    num_results = Keyword.get(opts, :num_results, 10)

    case Req.get(@endpoint,
           params: [q: query],
           headers: browser_headers(),
           retry: false,
           receive_timeout: 20_000
         ) do
      {:ok, %Req.Response{status: 200, body: html}} when is_binary(html) ->
        case parse(html, num_results) do
          [] ->
            if anti_bot?(html) do
              {:error, :anti_bot_block}
            else
              {:ok, []}
            end

          results ->
            {:ok, results}
        end

      {:ok, %Req.Response{status: status} = resp} ->
        Logger.error("DuckDuckGo returned #{status}: #{inspect(resp.body, limit: 200)}")
        {:error, {:api_error, status}}

      {:error, reason} ->
        {:error, {:request_failed, reason}}
    end
  end

  defp browser_headers do
    %{
      "user-agent" => @user_agent,
      "accept" =>
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
      "accept-language" => "en-US,en;q=0.9",
      "accept-encoding" => "gzip, deflate, br",
      "referer" => "https://duckduckgo.com/",
      "upgrade-insecure-requests" => "1",
      "sec-fetch-dest" => "document",
      "sec-fetch-mode" => "navigate",
      "sec-fetch-site" => "same-origin",
      "sec-fetch-user" => "?1"
    }
  end

  # When DDG returns its anti-bot challenge page the body contains the
  # "anomaly-modal" class. Surfacing this distinctly lets the dispatcher
  # fall back to another backend (e.g. SearXNG) instead of treating zero
  # results as a legitimate empty answer.
  defp anti_bot?(html) when is_binary(html) do
    String.contains?(html, "anomaly-modal") or
      String.contains?(html, "captcha")
  end

  defp anti_bot?(_), do: false

  defp parse(html, num_results) do
    case Floki.parse_document(html) do
      {:ok, doc} ->
        doc
        |> Floki.find("div.result")
        |> Enum.map(&extract_result/1)
        |> Enum.reject(&is_nil/1)
        |> Enum.take(num_results)

      _ ->
        []
    end
  end

  defp extract_result(node) do
    title =
      node
      |> Floki.find("a.result__a")
      |> Floki.text()
      |> String.trim()

    raw_url =
      case Floki.attribute(node, "a.result__a", "href") do
        [href | _] -> href
        _ -> nil
      end

    snippet =
      node
      |> Floki.find("a.result__snippet, .result__snippet")
      |> Floki.text()
      |> String.trim()

    case clean_url(raw_url) do
      nil ->
        nil

      "" ->
        nil

      url ->
        if title == "", do: nil, else: %{title: title, url: url, snippet: snippet}
    end
  end

  # DDG wraps target URLs in a redirect: //duckduckgo.com/l/?uddg=<encoded>&...
  defp clean_url(nil), do: nil

  defp clean_url(href) do
    cond do
      String.starts_with?(href, "//duckduckgo.com/l/") ->
        extract_uddg(href) || normalize_protocol(href)

      String.starts_with?(href, "/l/") ->
        extract_uddg(href) || normalize_protocol(href)

      true ->
        normalize_protocol(href)
    end
  end

  defp extract_uddg(href) do
    with [_, query] <- String.split(href, "?", parts: 2),
         %{"uddg" => encoded} <- URI.decode_query(query),
         decoded when is_binary(decoded) <- URI.decode(encoded) do
      decoded
    else
      _ -> nil
    end
  end

  defp normalize_protocol("//" <> rest), do: "https://" <> rest
  defp normalize_protocol(url), do: url
end
