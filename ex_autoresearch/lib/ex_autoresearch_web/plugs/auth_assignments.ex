defmodule ExAutoresearchWeb.Plugs.SetAssignments do
  @moduledoc """
  Plug that sets up user and organization assignments before LiveViews mount.
  """

  import Plug.Conn

  def init(opts), do: opts

  def call(conn, _opts) do
    case conn.assigns[:current_user] do
      nil ->
        conn

      user ->
        orgs = ExAutoresearch.Accounts.list_user_organizations(user.id)
        assign(conn, :user_organizations, orgs)
    end
  end
end
