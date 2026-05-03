import Config

# --- Deep Research Configuration ---

# Search API key (Serper recommended)
if System.get_env("SERPER_API_KEY") do
  config :ex_autoresearch, :search, serper_api_key: System.get_env("SERPER_API_KEY")
end

# Optional: Brave Search API (alternative to Serper)
if System.get_env("BRAVE_API_KEY") do
  config :ex_autoresearch, :search, brave_api_key: System.get_env("BRAVE_API_KEY")
end

# LLM API keys
if System.get_env("ANTHROPIC_API_KEY") do
  config :ex_autoresearch, :llm, anthropic_api_key: System.get_env("ANTHROPIC_API_KEY")
end

if System.get_env("OPENROUTER_API_KEY") do
  config :ex_autoresearch, :llm, openrouter_api_key: System.get_env("OPENROUTER_API_KEY")
end

# OpenAI-compatible gateway (中转站, vLLM, LM Studio, ...).
# Two model tiers: main (pro/planning/synthesis) and fast (flash/sub-queries).
openai_compat =
  [
    base_url: System.get_env("OPENAI_COMPAT_BASE_URL"),
    api_key: System.get_env("OPENAI_COMPAT_API_KEY"),
    model_main:
      System.get_env("OPENAI_COMPAT_MODEL_MAIN") || System.get_env("OPENAI_COMPAT_MODEL"),
    model_fast: System.get_env("OPENAI_COMPAT_MODEL_FAST")
  ]
  |> Enum.reject(fn {_, v} -> v in [nil, ""] end)

if openai_compat != [] do
  config :ex_autoresearch, :llm, openai_compat: openai_compat
end

# Maximum concurrent search threads
config :ex_autoresearch, :research,
  max_threads: String.to_integer(System.get_env("RESEARCH_MAX_THREADS", "5"))

case System.get_env("EX_AUTORESEARCH_SCRAPER") do
  "crawl4ai" -> config :ex_autoresearch, :scraper, ExAutoresearch.DeepResearch.Scraper.Crawl4ai
  "native" -> config :ex_autoresearch, :scraper, ExAutoresearch.DeepResearch.Scraper.Native
  _ -> :ok
end

case System.get_env("EX_AUTORESEARCH_SEARCH") do
  "duckduckgo" ->
    config :ex_autoresearch, :search_backend, ExAutoresearch.DeepResearch.Search.DuckDuckGo

  "serper" ->
    config :ex_autoresearch, :search_backend, ExAutoresearch.DeepResearch.Search.Serper

  "searxng" ->
    config :ex_autoresearch, :search_backend, ExAutoresearch.DeepResearch.Search.SearXNG

  _ ->
    :ok
end

if base_url = System.get_env("SEARXNG_BASE_URL") do
  cfg = Application.get_env(:ex_autoresearch, :search, [])
  cfg = if is_list(cfg), do: cfg, else: []
  config :ex_autoresearch, :search, Keyword.put(cfg, :searxng_base_url, base_url)
end

if base_url = System.get_env("CRAWL4AI_BASE_URL") do
  config :ex_autoresearch, :crawl4ai, base_url: base_url
end

if System.get_env("PHX_SERVER") do
  config :ex_autoresearch, ExAutoresearchWeb.Endpoint, server: true
end

config :ex_autoresearch, ExAutoresearchWeb.Endpoint,
  http: [port: String.to_integer(System.get_env("PORT", "4000"))]

if config_env() == :prod do
  # Production can use either SQLite (default) or PostgreSQL
  # SQLite: set DATABASE_PATH
  # PostgreSQL: set DATABASE_URL
  database_url = System.get_env("DATABASE_URL")
  database_path = System.get_env("DATABASE_PATH")

  cond do
    database_url ->
      config :ex_autoresearch, ExAutoresearch.Repo,
        url: database_url,
        pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10"),
        ssl: System.get_env("DATABASE_SSL", "true") == "true"

    database_path ->
      config :ex_autoresearch, ExAutoresearch.Repo,
        database: database_path,
        pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10")

    true ->
      raise """
      One of DATABASE_URL or DATABASE_PATH must be set for production.

      SQLite: DATABASE_PATH=/data/ex_autoresearch.db
      PostgreSQL: DATABASE_URL=postgres://user:pass@host:5432/db
      """
  end

  secret_key_base =
    System.get_env("SECRET_KEY_BASE") ||
      raise """
      environment variable SECRET_KEY_BASE is missing.
      You can generate one by calling: mix phx.gen.secret
      """

  host = System.get_env("PHX_HOST") || "example.com"

  config :ex_autoresearch, :dns_cluster_query, System.get_env("DNS_CLUSTER_QUERY")

  config :ex_autoresearch, ExAutoresearchWeb.Endpoint,
    url: [host: host, port: 443, scheme: "https"],
    http: [
      ip: {0, 0, 0, 0, 0, 0, 0, 0}
    ],
    secret_key_base: secret_key_base

  # Webhook URL for notifications
  if System.get_env("WEBHOOK_URL") do
    IO.puts("Webhook URL configured")
  end
end
