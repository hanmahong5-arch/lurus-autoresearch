defmodule ExAutoresearch.Accounts.User do
  @moduledoc """
  A user account with email/password authentication.
  A user can belong to multiple organizations via memberships.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "users"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :register do
      accept [:email, :name]
      primary? true
    end

    update :update_user do
      accept [:name]
    end
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :email, :string, allow_nil?: false

    attribute :password, :string, allow_nil?: true, sensitive?: true, public?: true, writable?: true
    attribute :name, :string, allow_nil?: true
    attribute :password_hash, :string, allow_nil?: false, sensitive?: true
    attribute :role, :atom, constraints: [one_of: [:admin, :member]], default: :member

    timestamps()
  end

  identities do
    identity :unique_user_email, [:email]
  end

  relationships do
    has_many :memberships, ExAutoresearch.Accounts.Membership
  end
end
