defmodule ExAutoresearchWeb.Plugs.Auth do
  @moduledoc """
  Plugs for authentication:
  - `fetch_current_user` — loads user from session and assigns `current_user`
  - `require_authenticated_user` — redirects to login if not authenticated
  - `redirect_if_user_is_authenticated` — redirects away from login page if already logged in
  """

  alias ExAutoresearch.Accounts

  import Plug.Conn
  import Phoenix.Controller
  use Phoenix.VerifiedRoutes,
    endpoint: ExAutoresearchWeb.Endpoint,
    router: ExAutoresearchWeb.Router,
    statics: ExAutoresearchWeb.static_paths()

  def init(opts), do: opts

  def call(conn, _opts) do
    fetch_current_user(conn, [])
  end

  @doc """
  Fetches the current user from the session and assigns it to the connection.
  """
  def fetch_current_user(conn, _opts) do
    user_id = get_session(conn, :user_id)

    conn =
      cond do
        user_id ->
          case Accounts.get_user(user_id) do
            {:ok, user} -> assign(conn, :current_user, user)
            _ -> assign(conn, :current_user, nil)
          end

        true ->
          assign(conn, :current_user, nil)
      end

    if conn.assigns[:current_user] do
      put_session(conn, :user_id, user_id)
    else
      conn
    end
  end

  @doc """
  Requires an authenticated user. Redirects to login if not authenticated.
  """
  def require_authenticated_user(conn, _opts) do
    if conn.assigns[:current_user] do
      conn
    else
      conn
      |> put_flash(:error, "You must be logged in to access that page.")
      |> redirect(to: ~p"/login")
      |> halt()
    end
  end

  @doc """
  Redirects to dashboard if user is already authenticated.
  Prevents logged-in users from accessing login/register pages.
  """
  def redirect_if_user_is_authenticated(conn, _opts) do
    if conn.assigns[:current_user] do
      conn
      |> redirect(to: ~p"/")
      |> halt()
    else
      conn
    end
  end
end
