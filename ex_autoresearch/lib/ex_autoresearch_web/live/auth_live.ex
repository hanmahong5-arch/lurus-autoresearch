defmodule ExAutoresearchWeb.AuthLive do
  @moduledoc """
  Authentication LiveView — login and register forms that POST to SessionController.
  """

  use ExAutoresearchWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    {:ok,
     socket
     |> assign(:mode, :login)
     |> assign(:email, "")
     |> assign(:error, nil)
     |> assign(:password, "")}
  end

  @impl true
  def handle_params(%{"action" => "register"}, _uri, socket) do
    {:noreply, assign(socket, :mode, :register)}
  end

  def handle_params(_params, _uri, socket) do
    {:noreply, assign(socket, :mode, :login)}
  end

  @impl true
  def handle_event("set_email", %{"value" => v}, socket),
    do: {:noreply, assign(socket, :email, v)}

  def handle_event("set_password", %{"value" => v}, socket),
    do: {:noreply, assign(socket, :password, v)}

  @impl true
  def render(assigns) do
    ~H"""
    <div class="min-h-screen bg-base-200 text-base-content flex items-center justify-center px-4 py-12 relative">
      <div class="absolute top-4 right-4">
        <Layouts.theme_toggle />
      </div>

      <div class="w-full max-w-md space-y-6">
        <div class="text-center space-y-1">
          <h1 class="text-2xl font-bold tracking-tight">ExAutoresearch</h1>
          <p class="text-xs text-base-content/60 font-mono">
            v{ExAutoresearch.Changelog.current_version()}
          </p>
        </div>

        <section class="card bg-base-100 border border-base-300 shadow-lg">
          <div class="card-body space-y-5">
            <header>
              <h2 class="text-xl font-semibold">
                {if @mode == :login, do: "Welcome back", else: "Create your account"}
              </h2>
              <p class="text-xs text-base-content/60 mt-1">
                <%= if @mode == :login do %>
                  Sign in to continue your research.
                <% else %>
                  Set up an account to launch and save reports.
                <% end %>
              </p>
            </header>

            <form
              method="post"
              action={if @mode == :login, do: "/session", else: "/register"}
              class="space-y-4"
            >
              <input type="hidden" name="_csrf_token" value={Plug.CSRFProtection.get_csrf_token()} />

              <div class="form-control">
                <label class="label" for="auth-email">
                  <span class="label-text font-medium">Email</span>
                </label>
                <input
                  id="auth-email"
                  type="email"
                  name="email"
                  value={@email}
                  class="input input-bordered w-full"
                  placeholder="you@example.com"
                  required
                  autocomplete="email"
                />
              </div>

              <div class="form-control">
                <label class="label" for="auth-password">
                  <span class="label-text font-medium">Password</span>
                </label>
                <input
                  id="auth-password"
                  type="password"
                  name="password"
                  class="input input-bordered w-full"
                  placeholder="••••••••"
                  required
                  autocomplete={if @mode == :login, do: "current-password", else: "new-password"}
                />
              </div>

              <button type="submit" class="btn btn-primary w-full">
                {if @mode == :login, do: "Sign in", else: "Create account"}
              </button>
            </form>

            <div class="divider my-1 text-xs text-base-content/50">or</div>

            <p class="text-center text-sm text-base-content/70">
              <%= if @mode == :login do %>
                Don't have an account?
                <.link navigate={~p"/login?action=register"} class="link link-primary">
                  Create one
                </.link>
              <% else %>
                Already have an account?
                <.link navigate={~p"/login"} class="link link-primary">
                  Sign in
                </.link>
              <% end %>
            </p>
          </div>
        </section>

        <p class="text-center text-xs text-base-content/50">
          Self-hosted · on-prem · zero data leaves your server.
        </p>
      </div>
    </div>
    """
  end
end
