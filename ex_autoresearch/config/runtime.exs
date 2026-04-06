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

# Maximum concurrent search threads
config :ex_autoresearch, :research,
  max_threads: String.to_integer(System.get_env("RESEARCH_MAX_THREADS", "5"))

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
