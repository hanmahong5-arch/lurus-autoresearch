defmodule ExAutoresearch.Research.Source do
  @moduledoc """
  A source URL fetched during a research report.
  Deduplicated by (report_id, url) via upsert identity.
  Isolated via report_id — no multitenancy block needed.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "sources"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :create do
      primary? true
      upsert? true
      upsert_identity :unique_report_url

      accept [
        :report_id,
        :investigation_id,
        :url,
        :domain,
        :title,
        :fetched_at,
        :content_hash,
        :relevance_score,
        :scraper_source
      ]
    end
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :report_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :investigation_id, :uuid_v7, public?: true

    attribute :url, :string, allow_nil?: false, public?: true
    attribute :domain, :string, public?: true
    attribute :title, :string, public?: true
    attribute :fetched_at, :utc_datetime_usec, public?: true
    attribute :content_hash, :string, public?: true
    attribute :relevance_score, :float, public?: true

    attribute :scraper_source, :atom,
      constraints: [one_of: [:crawl4ai, :native, :unknown]],
      default: :unknown,
      public?: true

    timestamps()
  end

  relationships do
    belongs_to :report, ExAutoresearch.Research.Report, attribute_writable?: true, public?: true

    belongs_to :investigation, ExAutoresearch.Research.Investigation do
      allow_nil? true
      attribute_writable? true
      public? true
    end
  end

  identities do
    identity :unique_report_url, [:report_id, :url]
  end
end
