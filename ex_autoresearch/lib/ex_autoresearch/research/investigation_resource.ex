defmodule ExAutoresearch.Research.Investigation do
  @moduledoc """
  A single research investigation step within a report.
  Investigations belong to a Report which belongs to an Organization.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "investigations"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :start do
      accept [:report_id, :depth, :query, :tool, :reasoning]
      primary? true
    end

    update :complete do
      accept [:status, :findings, :quality_score, :sources_count, :content_length, :url]
    end

    update :fail do
      accept [:status, :error]
    end

    update :abandon do
      accept [:status]
    end
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :report_id, :uuid_v7, allow_nil?: false, public?: true

    attribute :depth, :integer, default: 0, public?: true
    attribute :query, :string, public?: true
    attribute :tool, :string, allow_nil?: false, default: "search", public?: true

    attribute :reasoning, :string, public?: true

    attribute :status, :atom,
      constraints: [one_of: [:pending, :running, :completed, :failed, :abandoned]],
      default: :pending,
      public?: true

    attribute :findings, :string, public?: true
    attribute :quality_score, :float, public?: true
    attribute :sources_count, :integer, default: 0, public?: true
    attribute :content_length, :integer, default: 0, public?: true
    attribute :url, :string, public?: true
    attribute :error, :string, public?: true

    timestamps()
  end

  relationships do
    belongs_to :report, ExAutoresearch.Research.Report, public?: true
  end
end
