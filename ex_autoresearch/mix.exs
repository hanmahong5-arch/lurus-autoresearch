defmodule ExAutoresearch.MixProject do
  use Mix.Project

  def project do
    [
      app: :ex_autoresearch,
      version: "0.2.0",
      elixir: "~> 1.15",
      elixirc_paths: elixirc_paths(Mix.env()),
      start_permanent: Mix.env() == :prod,
      aliases: aliases(),
      deps: deps(),
      compilers: [:phoenix_live_view] ++ Mix.compilers(),
      listeners: [Phoenix.CodeReloader],
      consolidate_protocols: Mix.env() != :dev
    ]
  end

  def application do
    [
      mod: {ExAutoresearch.Application, []},
      extra_applications: [:logger, :runtime_tools, :inets, :ssl]
    ]
  end

  def cli do
    [preferred_envs: [precommit: :test]]
  end

  defp elixirc_paths(:test), do: ["lib", "test/support"]
  defp elixirc_paths(_), do: ["lib"]

  defp deps do
    [
      # Core web framework
      {:phoenix, "~> 1.8.5"},
      {:phoenix_ecto, "~> 4.5"},
      {:ecto_sql, "~> 3.13"},
      {:ecto_sqlite3, ">= 0.0.0"},
      {:phoenix_html, "~> 4.1"},
      {:phoenix_live_reload, "~> 1.2", only: :dev},
      {:phoenix_live_view, "~> 1.1.0"},
      {:phoenix_live_dashboard, "~> 0.8.3"},
      {:lazy_html, ">= 0.1.0", only: :test},

      # HTTP client
      {:req, "~> 0.5"},

      # Data
      {:jason, "~> 1.2"},
      {:nimble_options, "~> 1.1"},
      {:table_rex, "~> 4.0"},
      {:mdex, "~> 0.11.6"},

      {:bcrypt_elixir, "~> 3.0"},

      # Task queue
      {:oban, "~> 2.0"},
      {:oban_web, "~> 2.0"},
      {:ash_oban, "~> 0.7"},

      # Ash Framework
      {:ash, "~> 3.0"},
      {:ash_sqlite, "~> 0.2"},
      {:ash_phoenix, "~> 2.0"},

      # Telemetry
      {:telemetry_metrics, "~> 1.0"},
      {:telemetry_poller, "~> 1.0"},

      # Assets
      {:esbuild, "~> 0.10", runtime: Mix.env() == :dev},
      {:tailwind, "~> 0.3", runtime: Mix.env() == :dev},
      {:heroicons,
       github: "tailwindlabs/heroicons",
       tag: "v2.2.0",
       sparse: "optimized",
       app: false,
       compile: false,
       depth: 1},

      # Server
      {:bandit, "~> 1.5"},
      {:dns_cluster, "~> 0.2.0"},
      {:gettext, "~> 0.26"},

      # Dev tools
      {:sourceror, "~> 1.8", only: [:dev, :test]},
      {:usage_rules, "~> 1.0", only: [:dev]},
      {:igniter, "~> 0.6", only: [:dev, :test]},

      # Swoosh
      {:swoosh, "~> 1.16"}
    ]
  end

  defp aliases do
    [
      setup: ["deps.get", "ecto.setup", "assets.setup", "assets.build"],
      "ecto.setup": ["ecto.create", "ecto.migrate", "run priv/repo/seeds.exs"],
      "ecto.reset": ["ecto.drop", "ecto.setup"],
      test: ["ash.setup --quiet", "test"],
      "assets.setup": ["tailwind.install --if-missing", "esbuild.install --if-missing"],
      "assets.build": ["compile", "tailwind ex_autoresearch", "esbuild ex_autoresearch"],
      "assets.deploy": [
        "tailwind ex_autoresearch --minify",
        "esbuild ex_autoresearch --minify",
        "phx.digest"
      ],
      precommit: ["compile --warnings-as-errors", "deps.unlock --unused", "format", "test"],
      "ash.setup": ["ash.setup", "run priv/repo/seeds.exs"]
    ]
  end
end
