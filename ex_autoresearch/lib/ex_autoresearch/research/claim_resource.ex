defmodule ExAutoresearch.Research.Claim do
  @moduledoc """
  A claim extracted from research findings, optionally grounded by a source.
  Isolated via report_id — no multitenancy block needed.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "claims"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :create do
      primary? true

      accept [
        :report_id,
        :source_id,
        :text,
        :grounding,
        :confidence,
        :origin_subquery,
        :claim_hash,
        :order_index,
        :contradicting_source_ids
      ]
    end
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :report_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :source_id, :uuid_v7, public?: true

    attribute :text, :string, allow_nil?: false, public?: true

    attribute :grounding, :atom,
      constraints: [one_of: [:grounded, :contradicted, :unsupported, :complementary]],
      default: :unsupported,
      public?: true

    attribute :confidence, :float, public?: true
    attribute :origin_subquery, :string, public?: true
    attribute :claim_hash, :string, public?: true
    attribute :order_index, :integer, public?: true

    attribute :contradicting_source_ids, {:array, :uuid_v7}, public?: true

    timestamps()
  end

  relationships do
    belongs_to :report, ExAutoresearch.Research.Report, attribute_writable?: true, public?: true

    belongs_to :source, ExAutoresearch.Research.Source do
      allow_nil? true
      attribute_writable? true
      public? true
    end
  end
end
