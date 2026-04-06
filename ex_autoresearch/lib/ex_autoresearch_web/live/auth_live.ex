defmodule ExAutoresearchWeb.AuthLive do
  @moduledoc """
  Authentication LiveView — login and register forms that POST to SessionController.
  """

  use ExAutoresearchWeb, :live_view

  @impl true
  def mount(_params, session, socket) do
    {:ok,
     socket
     |> assign(:mode, :login)
     |> assign(:email, "")
     |> assign(:error, nil)
     |> assign(:password, "")
     |> assign(:_csrf_token, session["_csrf_token"])}
  end

  @impl true
  def handle_params(%{"action" => "register"}, _uri, socket) do
    {:noreply, assign(socket, :mode, :register)}
  end

  def handle_params(_params, _uri, socket) do
    {:noreply, assign(socket, :mode, :login)}
  end

  @impl true
  def handle_event("set_email", %{"value" => v}, socket), do: {:noreply, assign(socket, :email, v)}
  def handle_event("set_password", %{"value" => v}, socket), do: {:noreply, assign(socket, :password, v)}

  @impl true
  def render(assigns) do
    ~H"""
    <div class="min-h-screen bg-gray-50 flex items-center justify-center">
      <div class="w-full max-w-md">
        <div class="bg-white rounded-lg shadow p-8">
          <h1 class="text-2xl font-bold text-gray-900 text-center mb-6">
            <%= if @mode == :login, do: "Welcome Back", else: "Create Account" %>
          </h1>

          <%= if @mode == :login do %>
            <form method="post" action="/session">
              <input type="hidden" name="_csrf_token" value={@_csrf_token} />
              <div class="mb-4">
                <label class="block text-sm font-medium text-gray-700 mb-1">Email</label>
                <input
                  type="email"
                  name="email"
                  value={@email}
                  class="w-full px-3 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
                  placeholder="you@example.com"
                  required
                />
              </div>
              <div class="mb-4">
                <label class="block text-sm font-medium text-gray-700 mb-1">Password</label>
                <input
                  type="password"
                  name="password"
                  class="w-full px-3 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
                  placeholder="••••••••"
                  required
                />
              </div>
              <button
                type="submit"
                class="w-full bg-blue-600 text-white py-2 rounded-lg hover:bg-blue-700 font-medium"
              >
                Sign In
              </button>
            </form>
          <% else %>
            <form method="post" action="/register">
              <input type="hidden" name="_csrf_token" value={@_csrf_token} />
              <div class="mb-4">
                <label class="block text-sm font-medium text-gray-700 mb-1">Email</label>
                <input
                  type="email"
                  name="email"
                  value={@email}
                  class="w-full px-3 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
                  placeholder="you@example.com"
                  required
                />
              </div>
              <div class="mb-4">
                <label class="block text-sm font-medium text-gray-700 mb-1">Password</label>
                <input
                  type="password"
                  name="password"
                  class="w-full px-3 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
                  placeholder="••••••••"
                  required
                />
              </div>
              <button
                type="submit"
                class="w-full bg-blue-600 text-white py-2 rounded-lg hover:bg-blue-700 font-medium"
              >
                Create Account
              </button>
            </form>
          <% end %>

          <p class="text-center text-sm text-gray-500 mt-6">
            <%= if @mode == :login do %>
              Don't have an account?
              <.link navigate={~p"/login?action=register"} class="text-blue-600 hover:underline">
                Create one
              </.link>
            <% else %>
              Already have an account?
              <.link navigate={~p"/login"} class="text-blue-600 hover:underline">
                Sign in
              </.link>
            <% end %>
          </p>
        </div>
      </div>
    </div>
    """
  end
end
