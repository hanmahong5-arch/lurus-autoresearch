defmodule ExAutoresearch.Research.Report do
  @moduledoc """
  A deep research report — a named research session with AI-generated content.

  Each report belongs to an organization and can be filtered/created
  via authenticated user sessions.

  Note: Multi-tenant isolation is enforced at the application level
  — always filter by organization_id when reading/writing reports.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  multitenancy do
    strategy :attribute
    attribute :organization_id
  end

  sqlite do
    table "reports"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :start do
      accept [:title, :query, :model, :max_depth, :max_sources, :organization_id, :category, :tags]
      primary? true
    end

    update :update_status do
      accept [:status, :current_step, :summary]
    end

    update :complete do
      accept [:markdown_body]
    end

    update :update_result do
      accept [:total_sources, :total_investigations, :final_score]
    end
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :title, :string, allow_nil?: false
    attribute :query, :string, allow_nil?: false

    attribute :status, :atom,
      constraints: [one_of: [:pending, :researching, :analyzing, :writing, :completed, :failed]],
      default: :pending

    attribute :model, :string, default: "claude-sonnet-4"
    attribute :model_reasoning, :string, default: "claude-sonnet-4"

    # Organization ownership
    attribute :organization_id, :uuid_v7, allow_nil?: true

    # Competitive intelligence fields
    attribute :category, :atom,
      constraints: [one_of: [:competitor, :market, :policy, :trend, :custom]],
      default: :custom

    attribute :tags, {:array, :string}, default: []

    # Research depth control
    attribute :max_depth, :integer, default: 3
    attribute :max_sources, :integer, default: 25

    # Progress tracking
    attribute :current_step, :string
    attribute :progress_pct, :float, default: 0.0

    # Results
    attribute :total_sources, :integer, default: 0
    attribute :total_investigations, :integer, default: 0
    attribute :final_score, :float

    # Final report
    attribute :markdown_body, :string
    attribute :summary, :string

    timestamps()
  end

  identities do
    identity :unique_title, [:title]
  end

  relationships do
    has_many :investigations, ExAutoresearch.Research.Investigation
    belongs_to :organization, ExAutoresearch.Accounts.Organization
  end
end
