defmodule ExAutoresearch.Settings do
  @moduledoc """
  Centralised runtime-settings store for env vars + Application config.

  Each field is identified by an atom.  `put/2` writes to both System env
  (when an `env` key is present) and Application config (via `path`).
  `get/1` reads OS env first, then Application config.
  """

  @fields %{
    serper_api_key: %{
      env: "SERPER_API_KEY",
      path: [:search, :serper_api_key],
      secret?: true,
      flash: "Serper API key saved"
    },
    anthropic_api_key: %{
      env: "ANTHROPIC_API_KEY",
      path: [:llm, :anthropic_api_key],
      secret?: true,
      flash: "Anthropic API key saved"
    },
    openai_compat_base_url: %{
      env: "OPENAI_COMPAT_BASE_URL",
      path: [:llm, :openai_compat, :base_url],
      secret?: false,
      flash: nil
    },
    openai_compat_api_key: %{
      env: "OPENAI_COMPAT_API_KEY",
      path: [:llm, :openai_compat, :api_key],
      secret?: true,
      flash: "Gateway API key saved"
    },
    openai_compat_model_main: %{
      env: "OPENAI_COMPAT_MODEL_MAIN",
      path: [:llm, :openai_compat, :model_main],
      secret?: false,
      flash: nil
    },
    openai_compat_model_fast: %{
      env: "OPENAI_COMPAT_MODEL_FAST",
      path: [:llm, :openai_compat, :model_fast],
      secret?: false,
      flash: nil
    },
    searxng_base_url: %{
      env: "SEARXNG_BASE_URL",
      path: [:search, :searxng_base_url],
      secret?: false,
      flash: nil
    }
  }

  @doc "Return the field descriptor map for a given field id."
  @spec field(atom()) :: map() | nil
  def field(id), do: Map.get(@fields, id)

  @doc "Return all registered field ids."
  @spec field_ids() :: [atom()]
  def field_ids, do: Map.keys(@fields)

  @doc """
  Write `value` for `field_id` to System env (if configured) and Application config.
  Returns `:ok`.
  """
  @spec put(atom(), String.t()) :: :ok
  def put(field_id, value) do
    %{env: env_name, path: path} = Map.fetch!(@fields, field_id)

    if env_name, do: System.put_env(env_name, value)

    [top_key | rest] = path
    current_top = Application.get_env(:ex_autoresearch, top_key, [])
    updated_top = put_nested(current_top, rest, value)
    Application.put_env(:ex_autoresearch, top_key, updated_top)
    :ok
  end

  @doc """
  Read the current value for `field_id`.
  Checks System env first (if an env var is configured), then Application config.
  Returns `""` when unset.
  """
  @spec get(atom()) :: String.t()
  def get(field_id) do
    %{env: env_name, path: path} = Map.fetch!(@fields, field_id)

    env_val = if env_name, do: System.get_env(env_name), else: nil

    if env_val && env_val != "" do
      env_val
    else
      [top_key | rest] = path
      current_top = Application.get_env(:ex_autoresearch, top_key, [])
      get_nested(current_top, rest) || ""
    end
  end

  @doc """
  Return the current search backend as a string: "serper" | "searxng" | "duckduckgo".
  """
  @spec search_backend() :: String.t()
  def search_backend do
    case Application.get_env(:ex_autoresearch, :search_backend) do
      ExAutoresearch.DeepResearch.Search.Serper -> "serper"
      ExAutoresearch.DeepResearch.Search.SearXNG -> "searxng"
      ExAutoresearch.DeepResearch.Search.DuckDuckGo -> "duckduckgo"
      _ -> "duckduckgo"
    end
  end

  @doc """
  Write the search backend from a string value ("serper" | "searxng" | "duckduckgo").
  """
  @spec put_search_backend(String.t()) :: :ok
  def put_search_backend(value) do
    module =
      case value do
        "serper" -> ExAutoresearch.DeepResearch.Search.Serper
        "searxng" -> ExAutoresearch.DeepResearch.Search.SearXNG
        _ -> ExAutoresearch.DeepResearch.Search.DuckDuckGo
      end

    Application.put_env(:ex_autoresearch, :search_backend, module)
    :ok
  end

  # ---------------------------------------------------------------------------
  # Private helpers
  # ---------------------------------------------------------------------------

  # Recursively walk into nested keyword lists, creating missing levels as [].
  defp put_nested(_current, [], value), do: value

  defp put_nested(current, [key | rest], value) do
    current = if is_list(current), do: current, else: []
    nested = Keyword.get(current, key, [])
    Keyword.put(current, key, put_nested(nested, rest, value))
  end

  defp get_nested(current, []) do
    case current do
      v when is_binary(v) -> v
      _ -> nil
    end
  end

  defp get_nested(current, [key | rest]) do
    case current do
      kw when is_list(kw) -> get_nested(Keyword.get(kw, key), rest)
      _ -> nil
    end
  end
end
