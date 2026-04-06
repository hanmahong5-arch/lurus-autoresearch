defmodule ExAutoresearch.Agent.LLMClient do
  @moduledoc """
  Simple HTTP-based LLM client supporting multiple providers.

  Providers:
    - anthropic: Direct Anthropic API (Claude)
    - openrouter: OpenRouter API (supports multiple models)

  Configure via environment variables:
    - ANTHROPIC_API_KEY
    - OPENROUTER_API_KEY
  """

  use GenServer

  require Logger

  defstruct [:provider, :model, :api_key, status: :idle]

  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts)
  end

  @impl true
  def init(opts) do
    provider = Keyword.get(opts, :provider, default_provider())
    model = Keyword.get(opts, :model, default_model(provider))
    api_key = get_api_key(provider)

    if api_key do
      Logger.info("LLM client ready: provider=#{provider}, model=#{model}")
      {:ok, %__MODULE__{provider: provider, model: model, api_key: api_key}}
    else
      Logger.warning("No API key found for provider #{provider}")
      {:ok, %__MODULE__{provider: provider, model: model}}
    end
  end

  @impl true
  def handle_call({:prompt, text, requested_model}, _from, state) do
    model = requested_model || state.model
    result = do_completion(text, model, state.api_key, state.provider)
    {:reply, result, state}
  end

  # --- Provider logic ---

  defp do_completion(_prompt, _model, nil, _provider) do
    {:error, :no_api_key}
  end

  defp do_completion(prompt, model, api_key, :anthropic) do
    body = Jason.encode!(%{
      model: model,
      max_tokens: 8192,
      system: "You are a deep research assistant. Provide thorough, well-reasoned responses.",
      messages: [%{role: "user", content: prompt}]
    })

    case Req.post("https://api.anthropic.com/v1/messages",
           headers: %{
             "x-api-key" => api_key,
             "anthropic-version" => "2023-06-01",
             "content-type" => "application/json"
           },
           body: body,
           retry: :temporary,
           retry_max: 2
         ) do
      {:ok, %Req.Response{status: 200, body: data}} ->
        text =
          data
          |> Map.get("content", [])
          |> Enum.filter(&(&1["type"] == "text"))
          |> Enum.map_join("", & &1["text"])

        {:ok, text}

      {:ok, %Req.Response{status: status} = resp} ->
        Logger.error("Anthropic API error (#{status}): #{inspect(resp.body, limit: 200)}")
        {:error, {:api_error, status, resp.body}}

      {:error, reason} ->
        {:error, {:request_failed, reason}}
    end
  rescue
    e -> {:error, {:exception, Exception.message(e)}}
  end

  defp do_completion(prompt, model, api_key, :openrouter) do
    body = Jason.encode!(%{
      model: model,
      max_tokens: 8192,
      messages: [
        %{role: "system", content: "You are a deep research assistant."},
        %{role: "user", content: prompt}
      ]
    })

    case Req.post("https://openrouter.ai/api/v1/chat/completions",
           headers: %{
             "authorization" => "Bearer #{api_key}",
             "content-type" => "application/json",
             "http-referer" => "http://localhost:4000"
           },
           body: body,
           retry: :temporary,
           retry_max: 2
         ) do
      {:ok, %Req.Response{status: 200, body: data}} ->
        text = get_in(data, ["choices", Access.at(0), "message", "content"]) || ""
        {:ok, text}

      {:ok, %Req.Response{status: status} = resp} ->
        Logger.error("OpenRouter API error (#{status}): #{inspect(resp.body, limit: 200)}")
        {:error, {:api_error, status, resp.body}}

      {:error, reason} ->
        {:error, {:request_failed, reason}}
    end
  rescue
    e -> {:error, {:exception, Exception.message(e)}}
  end

  defp default_provider do
    cond do
      System.get_env("ANTHROPIC_API_KEY") -> :anthropic
      System.get_env("OPENROUTER_API_KEY") -> :openrouter
      true -> :anthropic
    end
  end

  defp default_model(:anthropic), do: "claude-sonnet-4-20250514"
  defp default_model(:openrouter), do: "anthropic/claude-sonnet-4"

  defp get_api_key(:anthropic), do: System.get_env("ANTHROPIC_API_KEY")
  defp get_api_key(:openrouter), do: System.get_env("OPENROUTER_API_KEY")
end
