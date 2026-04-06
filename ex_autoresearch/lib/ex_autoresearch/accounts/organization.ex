defmodule ExAutoresearch.Accounts.Organization do
  @moduledoc """
  An organization (tenant) that owns reports, templates, and notifications.
  Users are associated via memberships.
  """

  use Ash.Resource,
    domain: ExAutoresearch.Research,
    data_layer: AshSqlite.DataLayer

  sqlite do
    table "organizations"
    repo ExAutoresearch.Repo
  end

  actions do
    defaults [:read]

    create :create do
      accept [:name, :plan]
      primary? true
    end

    update :update do
      accept [:name]
    end

    update :update_plan do
      accept [:plan]
    end
  end

  attributes do
    uuid_v7_primary_key :id

    attribute :name, :string, allow_nil?: false, public?: true
    attribute :plan, :atom,
      constraints: [one_of: [:free, :pro, :enterprise]],
      default: :free,
      public?: true

    timestamps()
  end

  relationships do
    has_many :memberships, ExAutoresearch.Accounts.Membership, public?: true
    has_many :reports, ExAutoresearch.Research.Report, public?: true
    has_many :templates, ExAutoresearch.Research.Template, public?: true
    belongs_to :owner, ExAutoresearch.Accounts.User
  end
end
