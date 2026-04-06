defmodule ExAutoresearchWeb.SessionController do
  use ExAutoresearchWeb, :controller

  alias ExAutoresearch.Accounts.Auth

  def login(conn, _params) do
    render(conn, "login.html")
  end

  def create(conn, %{"email" => email, "password" => password}) do
    case Auth.login(String.trim(email), password) do
      {:ok, user} ->
        conn
        |> put_session(:user_id, user.id)
        |> put_flash(:info, "Welcome back!")
        |> redirect(to: "/")

      {:error, _} ->
        conn
        |> put_flash(:error, "Invalid email or password.")
        |> redirect(to: ~p"/login")
    end
  end

  def register(conn, _params) do
    render(conn, "register.html")
  end

  def create_registration(conn, %{"email" => email, "password" => password}) do
    case Auth.register(String.trim(email), password, nil) do
      {:ok, user, _org} ->
        conn
        |> put_session(:user_id, user.id)
        |> put_flash(:info, "Account created!")
        |> redirect(to: "/")

      {:error, :email_already_exists} ->
        conn
        |> put_flash(:error, "An account with that email already exists.")
        |> redirect(to: ~p"/register")

      {:error, _} ->
        conn
        |> put_flash(:error, "Registration failed.")
        |> redirect(to: ~p"/register")
    end
  end

  def delete(conn, _params) do
    conn
    |> delete_session(:user_id)
    |> put_flash(:info, "Logged out.")
    |> redirect(to: "/login")
  end
end
