defmodule ExAutoresearch.Research.Template do
  @moduledoc """
  A research template defines a reusable query pattern for recurring
  competitive intelligence tasks. Templates belong to organizations.

  Multi-tenant isolation is enforced at the application level
  — always filter by organization_id when reading/writing templates.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  multitenancy do
    strategy :attribute
    attribute :organization_id
  end

  sqlite do
    table "templates"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :create do
      accept [
        :name, :description, :organization_id, :query_template, :category,
        :schedule_cron, :enabled, :max_depth, :max_sources, :model, :tags
      ]
      primary? true
    end

    update :update do
      accept [
        :name, :description, :query_template, :category, :schedule_cron,
        :enabled, :max_depth, :max_sources, :model, :tags
      ]
    end

    update :toggle do
      accept [:enabled]
    end
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :name, :string, allow_nil?: false
    attribute :description, :string
    attribute :organization_id, :uuid_v7, allow_nil?: false
    attribute :query_template, :string, allow_nil?: false

    attribute :category, :atom,
      constraints: [one_of: [:competitor, :market, :policy, :trend, :custom]],
      default: :custom

    attribute :schedule_cron, :string
    attribute :enabled, :boolean, default: false
    attribute :max_depth, :integer, default: 3
    attribute :max_sources, :integer, default: 25
    attribute :model, :string, default: "claude-sonnet-4"
    attribute :tags, {:array, :string}, default: []

    timestamps()
  end

  relationships do
    belongs_to :organization, ExAutoresearch.Accounts.Organization
  end
end
