defmodule ExAutoresearch.Agent.LLMClient do
  @moduledoc """
  Simple HTTP-based LLM client supporting multiple providers.

  Providers:
    - openai_compat: Any OpenAI-compatible gateway (中转站, vLLM, LM Studio, ...)
    - anthropic:    Direct Anthropic API (Claude)
    - openrouter:   OpenRouter API

  Configure via environment variables:
    - OPENAI_COMPAT_BASE_URL  (e.g. https://newapi.lurus.cn/v1)
    - OPENAI_COMPAT_API_KEY
    - OPENAI_COMPAT_MODEL     (e.g. gpt-4o-mini)
    - ANTHROPIC_API_KEY
    - OPENROUTER_API_KEY
  """

  use GenServer

  require Logger

  defstruct [:provider, :model, :api_key, :base_url, status: :idle]

  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts)
  end

  @doc """
  Run a single completion against the configured provider.

  Manages the underlying short-lived GenServer for you — callers pass a prompt
  and (optionally) a tier (`:main` for planning/synthesis, `:fast` for
  per-investigation queries) plus an explicit `model` override or `timeout`.

  Returns `{:ok, text}` or `{:error, reason}`. Never raises; GenServer.call
  exits are caught and surfaced as `{:error, {:llm_call_failed, reason}}`.
  """
  @spec complete(String.t(), keyword()) :: {:ok, String.t()} | {:error, term()}
  def complete(prompt, opts \\ []) when is_binary(prompt) do
    tier = Keyword.get(opts, :tier, :main)
    model = Keyword.get(opts, :model)
    timeout = Keyword.get(opts, :timeout, :timer.minutes(5))
    report_id = Keyword.get(opts, :report_id)
    organization_id = Keyword.get(opts, :organization_id)
    investigation_id = Keyword.get(opts, :investigation_id)

    meta = %{
      provider: nil,
      model: model,
      report_id: report_id,
      organization_id: organization_id,
      investigation_id: investigation_id
    }

    :telemetry.span([:ex_autoresearch, :llm, :complete], meta, fn ->
      inner_result =
        case start_link(model: model) do
          {:ok, pid} ->
            try do
              GenServer.call(pid, {:prompt, prompt, tier}, timeout)
            catch
              :exit, reason -> {:error, {:llm_call_failed, reason}}
            after
              if Process.alive?(pid), do: GenServer.stop(pid, :normal)
            end

          {:error, reason} ->
            {:error, {:llm_start_failed, reason}}
        end

      case inner_result do
        {:ok, text, usage} ->
          stop_meta =
            Map.merge(meta, %{
              outcome: :ok,
              input_tokens: Map.get(usage, :input_tokens, 0),
              output_tokens: Map.get(usage, :output_tokens, 0)
            })

          {{:ok, text}, stop_meta}

        {:error, reason} ->
          stop_meta =
            Map.merge(meta, %{
              outcome: :error,
              reason: reason,
              input_tokens: 0,
              output_tokens: 0
            })

          {{:error, reason}, stop_meta}
      end
    end)
  end

  @impl true
  def init(opts) do
    provider = Keyword.get(opts, :provider, default_provider())

    explicit_model =
      case Keyword.get(opts, :model) do
        nil -> nil
        "" -> nil
        v when is_binary(v) -> v
      end

    api_key = get_api_key(provider)
    base_url = base_url_for(provider)

    state = %__MODULE__{
      provider: provider,
      model: explicit_model,
      api_key: api_key,
      base_url: base_url
    }

    if api_key do
      Logger.info(
        "LLM client ready: provider=#{provider}#{explicit_model && " model=#{explicit_model}"}"
      )
    else
      Logger.warning("No API key found for provider #{provider}")
    end

    {:ok, state}
  end

  @impl true
  def handle_call({:prompt, text, override}, _from, state) do
    model = resolve_model(override, state)
    result = do_completion(text, model, state.api_key, state.provider, state.base_url)

    {:reply, result, state}
  end

  # tier resolution — explicit model from start_link wins; otherwise tier picks
  # main/fast model from provider config (atom :main | :fast | nil treated as :main)
  defp resolve_model(_override, %{model: m}) when is_binary(m), do: m
  defp resolve_model(:fast, state), do: tier_model(:fast, state.provider)
  defp resolve_model(_, state), do: tier_model(:main, state.provider)

  # --- Provider logic ---

  defp do_completion(_prompt, _model, nil, _provider, _base_url) do
    {:error, :no_api_key}
  end

  defp do_completion(prompt, model, api_key, :openai_compat, base_url)
       when is_binary(base_url) do
    body =
      Jason.encode!(%{
        model: model,
        max_tokens: 8192,
        messages: [
          %{role: "system", content: "You are a deep research assistant."},
          %{role: "user", content: prompt}
        ]
      })

    url = String.trim_trailing(base_url, "/") <> "/chat/completions"

    case Req.post(url,
           headers: %{
             "authorization" => "Bearer #{api_key}",
             "content-type" => "application/json"
           },
           body: body,
           retry: false,
           receive_timeout: 120_000
         ) do
      {:ok, %Req.Response{status: 200, body: data}} ->
        text = get_in(data, ["choices", Access.at(0), "message", "content"]) || ""
        Logger.debug("OpenAI-compat OK model=#{model} content_len=#{byte_size(text)}")

        usage = %{
          input_tokens: get_in(data, ["usage", "prompt_tokens"]) || 0,
          output_tokens: get_in(data, ["usage", "completion_tokens"]) || 0
        }

        {:ok, text, usage}

      {:ok, %Req.Response{status: status} = resp} ->
        Logger.error("OpenAI-compat API error (#{status}): #{inspect(resp.body, limit: 500)}")
        {:error, {:api_error, status, resp.body}}

      {:error, reason} ->
        Logger.error("OpenAI-compat request_failed: #{inspect(reason, limit: 500)}")
        {:error, {:request_failed, reason}}
    end
  rescue
    e ->
      Logger.error("OpenAI-compat exception: #{Exception.message(e)} | #{inspect(e, limit: 200)}")
      {:error, {:exception, Exception.message(e)}}
  end

  defp do_completion(_prompt, _model, _api_key, :openai_compat, _base_url) do
    {:error, :missing_base_url}
  end

  defp do_completion(prompt, model, api_key, :anthropic, _base_url) do
    body =
      Jason.encode!(%{
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
           retry: false,
           receive_timeout: 120_000
         ) do
      {:ok, %Req.Response{status: 200, body: data}} ->
        text =
          data
          |> Map.get("content", [])
          |> Enum.filter(&(&1["type"] == "text"))
          |> Enum.map_join("", & &1["text"])

        usage = %{
          input_tokens: get_in(data, ["usage", "input_tokens"]) || 0,
          output_tokens: get_in(data, ["usage", "output_tokens"]) || 0
        }

        {:ok, text, usage}

      {:ok, %Req.Response{status: status} = resp} ->
        Logger.error("Anthropic API error (#{status}): #{inspect(resp.body, limit: 200)}")
        {:error, {:api_error, status, resp.body}}

      {:error, reason} ->
        {:error, {:request_failed, reason}}
    end
  rescue
    e -> {:error, {:exception, Exception.message(e)}}
  end

  defp do_completion(prompt, model, api_key, :openrouter, _base_url) do
    body =
      Jason.encode!(%{
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
           retry: false,
           receive_timeout: 120_000
         ) do
      {:ok, %Req.Response{status: 200, body: data}} ->
        text = get_in(data, ["choices", Access.at(0), "message", "content"]) || ""

        usage = %{
          input_tokens: get_in(data, ["usage", "prompt_tokens"]) || 0,
          output_tokens: get_in(data, ["usage", "completion_tokens"]) || 0
        }

        {:ok, text, usage}

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
      openai_compat_configured?() -> :openai_compat
      System.get_env("ANTHROPIC_API_KEY") -> :anthropic
      System.get_env("OPENROUTER_API_KEY") -> :openrouter
      true -> :anthropic
    end
  end

  defp openai_compat_configured? do
    cfg = openai_compat_config()

    is_binary(cfg[:base_url]) and is_binary(cfg[:api_key]) and
      cfg[:base_url] != "" and cfg[:api_key] != ""
  end

  defp openai_compat_config do
    case Application.get_env(:ex_autoresearch, :llm, []) do
      kw when is_list(kw) -> Keyword.get(kw, :openai_compat, []) |> Enum.into(%{})
      _ -> %{}
    end
  end

  # Tier → concrete model name. main = pro (planning, synthesis, deep eval);
  # fast = flash (per-investigation query generation, scoring).
  defp tier_model(:fast, :openai_compat) do
    cfg = openai_compat_config()
    cfg[:model_fast] || cfg[:model_main] || "gpt-4o-mini"
  end

  defp tier_model(_, :openai_compat) do
    cfg = openai_compat_config()
    cfg[:model_main] || cfg[:model_fast] || "gpt-4o-mini"
  end

  defp tier_model(_, :anthropic), do: "claude-sonnet-4-20250514"
  defp tier_model(_, :openrouter), do: "anthropic/claude-sonnet-4"

  defp get_api_key(:openai_compat), do: openai_compat_config()[:api_key]
  defp get_api_key(:anthropic), do: System.get_env("ANTHROPIC_API_KEY")
  defp get_api_key(:openrouter), do: System.get_env("OPENROUTER_API_KEY")

  defp base_url_for(:openai_compat), do: openai_compat_config()[:base_url]
  defp base_url_for(_), do: nil
end
