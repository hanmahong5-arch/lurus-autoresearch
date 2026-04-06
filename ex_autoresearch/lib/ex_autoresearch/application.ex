defmodule ExAutoresearch.Application do
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      ExAutoresearchWeb.Telemetry,
      ExAutoresearch.Repo,
      {Ecto.Migrator,
       repos: Application.fetch_env!(:ex_autoresearch, :ecto_repos), skip: skip_migrations?()},
      {Oban,
       AshOban.config(
         Application.fetch_env!(:ex_autoresearch, :ash_domains),
         Application.fetch_env!(:ex_autoresearch, Oban)
       )},
      {DNSCluster, query: Application.get_env(:ex_autoresearch, :dns_cluster_query) || :ignore},
      {Phoenix.PubSub, name: ExAutoresearch.PubSub},
      ExAutoresearch.DeepResearch.ResearchOrchestrator,
      ExAutoresearch.Workers.TemplateScheduler,
      ExAutoresearchWeb.Endpoint
    ]

    opts = [strategy: :one_for_one, name: ExAutoresearch.Supervisor]
    Supervisor.start_link(children, opts)
  end

  @impl true
  def config_change(changed, _new, removed) do
    ExAutoresearchWeb.Endpoint.config_change(changed, removed)
    :ok
  end

  defp skip_migrations?(), do: System.get_env("SKIP_MIGRATIONS") == "true"
end
