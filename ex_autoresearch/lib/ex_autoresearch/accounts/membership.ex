defmodule ExAutoresearch.Accounts.Membership do
  @moduledoc """
  Links a user to an organization with a specific role.
  A user can have multiple memberships (multi-org).
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "memberships"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :create do
      accept [:user_id, :organization_id, :role]
      primary? true
    end

    update :update do
      accept [:role]
    end
  end

  attributes do
    uuid_v7_primary_key :id
    attribute :user_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :organization_id, :uuid_v7, allow_nil?: false, public?: true
    attribute :role, :atom,
      constraints: [one_of: [:owner, :admin, :member]],
      default: :member,
      public?: true

    timestamps()
  end

  relationships do
    belongs_to :user, ExAutoresearch.Accounts.User, public?: true
    belongs_to :organization, ExAutoresearch.Accounts.Organization, public?: true
  end
end
