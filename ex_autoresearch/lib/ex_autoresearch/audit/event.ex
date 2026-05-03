defmodule ExAutoresearch.Audit.Event do
  @moduledoc """
  Append-only audit event. No update or destroy actions are declared —
  Ash itself enforces the immutability invariant.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Audit,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "audit_events"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :record do
      accept [
        :event_type,
        :user_id,
        :target_type,
        :target_id,
        :summary,
        :metadata,
        :organization_id
      ]

      primary? true
    end
  end

  multitenancy do
    strategy :attribute
    attribute :organization_id
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :organization_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :user_id, :uuid_v7, public?: true

    attribute :event_type, :atom,
      constraints: [
        one_of: [
          :research_started,
          :report_completed,
          :report_failed,
          :report_exported,
          :template_created,
          :template_updated,
          :template_deleted,
          :user_logged_in
        ]
      ],
      allow_nil?: false,
      public?: true

    attribute :target_type, :atom,
      constraints: [one_of: [:report, :template, :investigation, :user, :system]],
      public?: true

    attribute :target_id, :string, public?: true
    attribute :summary, :string, public?: true
    attribute :metadata, :map, default: %{}, public?: true

    create_timestamp :inserted_at
  end
end
