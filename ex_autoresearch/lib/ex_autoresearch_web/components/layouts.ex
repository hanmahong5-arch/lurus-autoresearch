defmodule ExAutoresearchWeb.Layouts do
  @moduledoc """
  This module holds layouts and related functionality
  used by your application.
  """
  use ExAutoresearchWeb, :html

  # Embed all files in layouts/* within this module.
  # The default root.html.heex file contains the HTML
  # skeleton of your application, namely HTML headers
  # and other static content.
  embed_templates "layouts/*"

  @doc """
  Renders your app layout.

  This function is typically invoked from every template,
  and it often contains your application menu, sidebar,
  or similar.

  ## Examples

      <Layouts.app flash={@flash}>
        <h1>Content</h1>
      </Layouts.app>

  """
  attr :flash, :map, required: true, doc: "the map of flash messages"

  attr :current_scope, :map,
    default: nil,
    doc: "the current [scope](https://hexdocs.pm/phoenix/scopes.html)"

  attr :current_user, :map,
    default: nil,
    doc: "the authenticated user, if any — drives the user menu in the navbar"

  attr :active_nav, :atom,
    default: nil,
    doc:
      "which top-level nav entry to highlight (:dashboard | :templates | :schedules | :settings)"

  attr :container, :string,
    default: "max-w-6xl",
    doc: "max-width Tailwind class applied to the centered main column"

  slot :inner_block, required: true

  def app(assigns) do
    ~H"""
    <div class="min-h-screen bg-base-200 text-base-content">
      <header class="navbar bg-base-100 border-b border-base-300 px-4 sm:px-6 lg:px-8 sticky top-0 z-30 shadow-sm">
        <div class="flex-1">
          <.link navigate={~p"/"} class="btn btn-ghost px-2 normal-case gap-2">
            <span class="text-base font-semibold tracking-tight">ExAutoresearch</span>
            <span class="text-[10px] font-mono opacity-60">
              v{ExAutoresearch.Changelog.current_version()}
            </span>
          </.link>
        </div>

        <div class="flex-none">
          <ul class="menu menu-horizontal px-1 gap-1 items-center">
            <li><.nav_link to={~p"/"} active={@active_nav == :dashboard}>Dashboard</.nav_link></li>

            <li>
              <.nav_link to={~p"/templates"} active={@active_nav == :templates}>Templates</.nav_link>
            </li>

            <li>
              <.nav_link to={~p"/schedules"} active={@active_nav == :schedules}>Schedules</.nav_link>
            </li>

            <li>
              <.nav_link to={~p"/settings"} active={@active_nav == :settings}>Settings</.nav_link>
            </li>

            <li class="ml-1"><.theme_toggle /></li>

            <li class="ml-1"><.user_menu current_user={@current_user} /></li>
          </ul>
        </div>
      </header>

      <main class="px-4 py-6 sm:px-6 lg:px-8">
        <div class={["mx-auto space-y-4", @container]}>{render_slot(@inner_block)}</div>
      </main>
    </div>
    <.flash_group flash={@flash} />
    """
  end

  attr :current_user, :map, default: nil

  defp user_menu(assigns) do
    ~H"""
    <%= if @current_user do %>
      <div class="dropdown dropdown-end">
        <div tabindex="0" role="button" class="btn btn-ghost btn-sm normal-case gap-2">
          <div class="avatar placeholder">
            <div class="bg-neutral text-neutral-content rounded-full w-7 h-7 flex items-center justify-center">
              <span class="text-xs font-semibold">
                {@current_user.email |> String.first() |> String.upcase()}
              </span>
            </div>
          </div>

          <span class="hidden sm:inline text-xs font-medium opacity-80 max-w-[12rem] truncate">
            {@current_user.email}
          </span>
        </div>

        <ul
          tabindex="0"
          class="dropdown-content menu menu-sm bg-base-100 rounded-box z-40 mt-2 w-48 p-2 shadow-lg border border-base-300"
        >
          <li class="menu-title px-2 py-1 text-xs">
            <span class="truncate">{@current_user.email}</span>
          </li>

          <li>
            <form method="get" action="/logout" class="contents">
              <button type="submit" class="text-error hover:bg-error/10">Sign out</button>
            </form>
          </li>
        </ul>
      </div>
    <% else %>
      <.link navigate={~p"/login"} class="btn btn-primary btn-sm">Sign in</.link>
    <% end %>
    """
  end

  attr :to, :string, required: true
  attr :active, :boolean, default: false
  slot :inner_block, required: true

  defp nav_link(assigns) do
    ~H"""
    <.link
      navigate={@to}
      class={[
        "btn btn-ghost btn-sm normal-case font-medium",
        @active && "btn-active text-primary"
      ]}
    >
      {render_slot(@inner_block)}
    </.link>
    """
  end

  @doc """
  Shows the flash group with standard titles and content.

  ## Examples

      <.flash_group flash={@flash} />
  """
  attr :flash, :map, required: true, doc: "the map of flash messages"
  attr :id, :string, default: "flash-group", doc: "the optional id of flash container"

  def flash_group(assigns) do
    ~H"""
    <div id={@id} aria-live="polite">
      <.flash kind={:info} flash={@flash} /> <.flash kind={:error} flash={@flash} />
      <.flash
        id="client-error"
        kind={:error}
        title={gettext("We can't find the internet")}
        phx-disconnected={show(".phx-client-error #client-error") |> JS.remove_attribute("hidden")}
        phx-connected={hide("#client-error") |> JS.set_attribute({"hidden", ""})}
        hidden
      >
        {gettext("Attempting to reconnect")}
        <.icon name="hero-arrow-path" class="ml-1 size-3 motion-safe:animate-spin" />
      </.flash>

      <.flash
        id="server-error"
        kind={:error}
        title={gettext("Something went wrong!")}
        phx-disconnected={show(".phx-server-error #server-error") |> JS.remove_attribute("hidden")}
        phx-connected={hide("#server-error") |> JS.set_attribute({"hidden", ""})}
        hidden
      >
        {gettext("Attempting to reconnect")}
        <.icon name="hero-arrow-path" class="ml-1 size-3 motion-safe:animate-spin" />
      </.flash>
    </div>
    """
  end

  @doc """
  Provides dark vs light theme toggle based on themes defined in app.css.

  See <head> in root.html.heex which applies the theme before page load.
  """
  def theme_toggle(assigns) do
    ~H"""
    <div class="card relative flex flex-row items-center border-2 border-base-300 bg-base-300 rounded-full">
      <div class="absolute w-1/3 h-full rounded-full border-1 border-base-200 bg-base-100 brightness-200 left-0 [[data-theme=light]_&]:left-1/3 [[data-theme=dark]_&]:left-2/3 transition-[left]" />
      <button
        class="flex p-2 cursor-pointer w-1/3"
        phx-click={JS.dispatch("phx:set-theme")}
        data-phx-theme="system"
      >
        <.icon name="hero-computer-desktop-micro" class="size-4 opacity-75 hover:opacity-100" />
      </button>
      <button
        class="flex p-2 cursor-pointer w-1/3"
        phx-click={JS.dispatch("phx:set-theme")}
        data-phx-theme="light"
      >
        <.icon name="hero-sun-micro" class="size-4 opacity-75 hover:opacity-100" />
      </button>
      <button
        class="flex p-2 cursor-pointer w-1/3"
        phx-click={JS.dispatch("phx:set-theme")}
        data-phx-theme="dark"
      >
        <.icon name="hero-moon-micro" class="size-4 opacity-75 hover:opacity-100" />
      </button>
    </div>
    """
  end
end
