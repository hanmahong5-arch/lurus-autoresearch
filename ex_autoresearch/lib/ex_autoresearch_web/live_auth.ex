defmodule ExAutoresearchWeb.LiveAuth do
  @moduledoc """
  LiveView `on_mount` callback that hydrates `current_user`, `user_organizations`,
  and `current_org_id` into `socket.assigns` from the session.

  Without this, `socket.assigns[:current_user]` is `nil` in every LiveView even
  when the user is logged in — Phoenix does not automatically copy `conn.assigns`
  into LiveView's socket.

  Registered via `live_session :authenticated, on_mount: [...]` in the router.
  """

  import Phoenix.Component, only: [assign: 3]

  alias ExAutoresearch.Accounts

  def on_mount(:default, _params, session, socket) do
    {:cont, hydrate(socket, session)}
  end

  defp hydrate(socket, %{"user_id" => user_id}) when not is_nil(user_id) do
    case Accounts.get_user(user_id) do
      {:ok, user} ->
        orgs = Accounts.list_user_organizations(user.id)

        org_id =
          case orgs do
            [%{id: id} | _] -> id
            _ -> nil
          end

        socket
        |> assign(:current_user, user)
        |> assign(:user_organizations, orgs)
        |> assign(:current_org_id, org_id)

      _ ->
        assign_anon(socket)
    end
  end

  defp hydrate(socket, _session), do: assign_anon(socket)

  defp assign_anon(socket) do
    socket
    |> assign(:current_user, nil)
    |> assign(:user_organizations, [])
    |> assign(:current_org_id, nil)
  end
end
