defmodule ExAutoresearch.Research.Brief do
  @moduledoc """
  A Brief is a recurring research topic that can be run on demand or on a
  schedule.  Each run produces a versioned Report (`run_version` increments
  per brief so the history is diffable).

  Multi-tenant isolation is via the :attribute strategy with `global? true`
  so the scheduler can read across tenants in a later phase.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "briefs"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :create do
      primary? true

      accept [
        :name,
        :question,
        :category,
        :cadence,
        :enabled,
        :organization_id,
        :model,
        :max_depth,
        :max_sources,
        :allow_domains,
        :block_domains,
        :notify_channels
      ]
    end

    update :update do
      accept [
        :name,
        :question,
        :category,
        :model,
        :max_depth,
        :max_sources,
        :allow_domains,
        :block_domains,
        :notify_channels
      ]
    end

    update :toggle do
      accept [:enabled]
    end

    update :mark_ran do
      accept [:last_run_at, :next_run_at]
    end
  end

  multitenancy do
    strategy :attribute
    attribute :organization_id
    global? true
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :name, :string, allow_nil?: false, public?: true
    attribute :question, :string, allow_nil?: false, public?: true

    attribute :category, :atom,
      constraints: [one_of: [:competitor, :market, :policy, :trend, :custom]],
      default: :custom,
      public?: true

    attribute :cadence, :atom,
      constraints: [one_of: [:manual, :hourly, :daily, :weekly]],
      default: :manual,
      public?: true

    attribute :enabled, :boolean, default: false, public?: true
    attribute :organization_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :model, :string, default: "claude-sonnet-4", public?: true
    attribute :max_depth, :integer, default: 3, public?: true
    attribute :max_sources, :integer, default: 25, public?: true
    attribute :allow_domains, {:array, :string}, default: [], public?: true
    attribute :block_domains, {:array, :string}, default: [], public?: true
    attribute :notify_channels, {:array, :string}, default: [], public?: true
    attribute :last_run_at, :utc_datetime_usec, public?: true
    attribute :next_run_at, :utc_datetime_usec, public?: true

    timestamps()
  end

  relationships do
    has_many :reports, ExAutoresearch.Research.Report
    has_many :deltas, ExAutoresearch.Research.Delta
    belongs_to :organization, ExAutoresearch.Accounts.Organization
  end
end
