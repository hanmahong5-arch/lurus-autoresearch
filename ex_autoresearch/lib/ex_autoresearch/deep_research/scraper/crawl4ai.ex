defmodule ExAutoresearch.DeepResearch.Scraper.Crawl4ai do
  @moduledoc """
  Crawl4AI HTTP scraper.

  Sends pages to a self-hosted Crawl4AI instance and returns clean markdown.

  Configuration (via `config/runtime.exs` or env vars):

      config :ex_autoresearch, :crawl4ai, base_url: "http://localhost:11235"

  Or set `CRAWL4AI_BASE_URL` env var.

  Sync endpoint (`POST /crawl_sync`) is tried first. If the server returns 404
  (older Crawl4AI versions), falls back to async (`POST /crawl` + poll
  `GET /task/{id}`).

  This module does NOT fall back to `Scraper.Native` on errors — that is the
  caller's responsibility.

  ## Test support

  Pass `plug: {Req.Test, StubName}` in opts to route all Req calls through a
  `Req.Test` stub instead of the network.
  """

  @behaviour ExAutoresearch.DeepResearch.Scraper

  @default_timeout 30_000
  @poll_interval 1_000
  @max_polls 30

  @impl ExAutoresearch.DeepResearch.Scraper
  def fetch(url, opts \\ []) do
    base_url = base_url()
    timeout = Keyword.get(opts, :timeout, @default_timeout)
    plug_opt = Keyword.take(opts, [:plug])
    do_fetch(url, base_url, timeout, plug_opt)
  end

  defp do_fetch(url, base_url, timeout, plug_opt) do
    body = %{"urls" => [url], "crawler_params" => %{}}
    req_opts = [json: body, receive_timeout: timeout] ++ plug_opt

    case Req.post("#{base_url}/crawl_sync", req_opts) do
      {:ok, %Req.Response{status: 200, body: resp_body}} ->
        parse_sync_response(resp_body, url)

      {:ok, %Req.Response{status: 404}} ->
        fetch_async(url, base_url, body, timeout, plug_opt)

      {:ok, %Req.Response{status: status}} when status >= 500 ->
        {:error, {:crawl4ai_failed, {:http_error, status}}}

      {:ok, %Req.Response{status: status}} ->
        {:error, {:crawl4ai_failed, {:unexpected_status, status}}}

      {:error, reason} ->
        {:error, {:crawl4ai_failed, reason}}
    end
  end

  defp parse_sync_response(%{"results" => [first | _]}, url) do
    case first do
      %{"success" => true, "markdown" => markdown} ->
        {:ok,
         %{
           markdown: markdown,
           metadata: %{source: :crawl4ai, url: url, fetched_at: DateTime.utc_now()}
         }}

      %{"success" => false} ->
        {:error, {:crawl4ai_failed, :page_fetch_failed}}

      _ ->
        {:error, {:crawl4ai_failed, :unexpected_response_shape}}
    end
  end

  defp parse_sync_response(_body, _url) do
    {:error, {:crawl4ai_failed, :unexpected_response_shape}}
  end

  defp fetch_async(url, base_url, body, timeout, plug_opt) do
    req_opts = [json: body, receive_timeout: timeout] ++ plug_opt

    case Req.post("#{base_url}/crawl", req_opts) do
      {:ok, %Req.Response{status: 200, body: %{"task_id" => task_id}}} ->
        poll_task(task_id, base_url, url, @max_polls, plug_opt)

      {:ok, %Req.Response{status: status}} when status >= 500 ->
        {:error, {:crawl4ai_failed, {:http_error, status}}}

      {:ok, %Req.Response{status: status}} ->
        {:error, {:crawl4ai_failed, {:unexpected_status, status}}}

      {:error, reason} ->
        {:error, {:crawl4ai_failed, reason}}
    end
  end

  defp poll_task(_task_id, _base_url, _url, 0, _plug_opt) do
    {:error, {:crawl4ai_failed, :poll_timeout}}
  end

  defp poll_task(task_id, base_url, url, remaining, plug_opt) do
    Process.sleep(@poll_interval)

    req_opts = plug_opt

    case Req.get("#{base_url}/task/#{task_id}", req_opts) do
      {:ok, %Req.Response{status: 200, body: %{"status" => "completed", "result" => result}}} ->
        case result do
          %{"markdown" => markdown} ->
            {:ok,
             %{
               markdown: markdown,
               metadata: %{source: :crawl4ai, url: url, fetched_at: DateTime.utc_now()}
             }}

          _ ->
            {:error, {:crawl4ai_failed, :unexpected_result_shape}}
        end

      {:ok, %Req.Response{status: 200, body: %{"status" => "failed"}}} ->
        {:error, {:crawl4ai_failed, :task_failed}}

      {:ok, %Req.Response{status: 200}} ->
        poll_task(task_id, base_url, url, remaining - 1, plug_opt)

      {:ok, %Req.Response{status: status}} ->
        {:error, {:crawl4ai_failed, {:poll_http_error, status}}}

      {:error, reason} ->
        {:error, {:crawl4ai_failed, reason}}
    end
  end

  defp base_url do
    Application.get_env(:ex_autoresearch, :crawl4ai, [])
    |> Keyword.get(:base_url, "http://localhost:11235")
  end
end
