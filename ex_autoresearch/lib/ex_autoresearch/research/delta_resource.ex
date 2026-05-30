defmodule ExAutoresearch.Research.Delta do
  @moduledoc """
  A Delta represents the diff between two consecutive Report runs for the same Brief.
  Tracks what claims were added, removed, changed, or contradicted between runs.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "deltas"
    repo ExAutoresearch.Repo

    references do
      reference :brief, ignore?: true
    end
  end

  actions do
    defaults [:read]

    create :create do
      primary? true

      accept [
        :brief_id,
        :organization_id,
        :from_report_id,
        :to_report_id,
        :markdown_digest,
        :added_count,
        :changed_count,
        :removed_count,
        :contradicted_count,
        :generated_at
      ]
    end

    update :mark_read do
      accept [:read_at]
    end
  end

  multitenancy do
    strategy :attribute
    attribute :organization_id
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :brief_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :organization_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :from_report_id, :uuid_v7, public?: true
    attribute :to_report_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :markdown_digest, :string, public?: true
    attribute :added_count, :integer, default: 0, public?: true
    attribute :changed_count, :integer, default: 0, public?: true
    attribute :removed_count, :integer, default: 0, public?: true
    attribute :contradicted_count, :integer, default: 0, public?: true
    attribute :read_at, :utc_datetime_usec, public?: true
    attribute :generated_at, :utc_datetime_usec, public?: true

    timestamps()
  end

  relationships do
    belongs_to :brief, ExAutoresearch.Research.Brief do
      attribute_writable? true
    end
  end
end
