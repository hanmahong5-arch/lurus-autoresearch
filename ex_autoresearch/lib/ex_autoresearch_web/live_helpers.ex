defmodule ExAutoresearchWeb.LiveHelpers do
  @moduledoc """
  LiveView helper functions for user authentication context.
  """

  use Phoenix.VerifiedRoutes,
    endpoint: ExAutoresearchWeb.Endpoint,
    router: ExAutoresearchWeb.Router,
    statics: ExAutoresearchWeb.static_paths()

  @doc """
  Given a LiveView socket with a current_user already assigned via plug,
  ensures the organization context is available.
  """
  def require_authenticated(assigns) when is_map(assigns) do
    cond do
      Map.has_key?(assigns, :current_user) and assigns.current_user != nil ->
        assigns

      true ->
        assigns
        |> Map.put(:flash, Map.get(assigns, :flash, %{}) |> Map.put(:error, "You must be logged in"))
        |> Map.put(:redirect_to, "/login")
    end
  end
end
