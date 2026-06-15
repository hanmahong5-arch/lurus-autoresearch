defmodule ExAutoresearch.Notifications.Webhook do
  @moduledoc """
  Sends HTTP POST requests to external webhook URLs for integrations
  with enterprise chat tools (企业微信, 飞书/Lark, 钉钉, Slack).

  Test seam: set `config :ex_autoresearch, :webhook_req_options, plug: {Req.Test, StubName}`
  to intercept requests in tests. Production behavior is unchanged when the key is unset.
  """

  require Logger

  @doc """
  POST a JSON payload to the given URL.
  """
  def post(url, payload) do
    body = Jason.encode!(payload)
    extra_opts = Application.get_env(:ex_autoresearch, :webhook_req_options, [])

    case Req.post(
           url,
           [
             headers: %{"content-type" => "application/json"},
             body: body,
             retry: :transient,
             max_retries: 2
           ] ++ extra_opts
         ) do
      {:ok, %Req.Response{status: status}} when status in 200..299 ->
        :ok

      {:ok, %Req.Response{status: status} = resp} ->
        {:error, {:http_error, status, resp.body}}

      {:error, reason} ->
        {:error, reason}
    end
  rescue
    e -> {:error, {:exception, Exception.message(e)}}
  end
end
