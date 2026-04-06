defmodule ExAutoresearchWeb.SettingsLive do
  @moduledoc """
  LiveView for organization settings: notification config, API keys, team management.
  """

  use ExAutoresearchWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    org = get_org(socket)

    {:ok,
     socket
     |> assign(:org, org)
     |> assign(:webhook_url, System.get_env("WEBHOOK_URL", ""))
     |> assign(:notifications_email, "")
     |> assign(:tab, :general)}
  end

  @impl true
  def handle_event("switch_tab", %{"tab" => tab}, socket) do
    {:noreply, assign(socket, :tab, tab)}
  end

  def handle_event("set_org_name", %{"value" => v}, socket) do
    org = socket.assigns.org

    if org && v != org.name && v != "" do
      Ash.update!(org, %{name: v}, action: :update)
    end

    {:noreply, socket}
  end

  def handle_event("set_serper_key", %{"value" => v}, socket) do
    Application.put_env(:ex_autoresearch, :search, %{serper_api_key: v})
    System.put_env("SERPER_API_KEY", v)
    {:noreply, put_flash(socket, :info, "Serper API key saved")}
  end

  def handle_event("set_anthropic_key", %{"value" => v}, socket) do
    Application.put_env(:ex_autoresearch, :llm, %{anthropic_api_key: v})
    System.put_env("ANTHROPIC_API_KEY", v)
    {:noreply, put_flash(socket, :info, "Anthropic API key saved")}
  end

  def handle_event("set_webhook", %{"value" => v}, socket) do
    System.put_env("WEBHOOK_URL", v)
    {:noreply, socket}
  end

  def handle_event("save_webhook", _params, socket) do
    {:noreply, put_flash(socket, :info, "Webhook URL saved")}
  end

  def handle_event("save_email_notify", %{"email" => email}, socket) do
    Application.put_env(
      :ex_autoresearch,
      :notifications,
      %{email_to: email}
    )

    {:noreply,
     socket
     |> assign(:notifications_email, email)
     |> put_flash(:info, "Email notifications enabled")}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <div class="min-h-screen bg-gray-50">
      <header class="bg-white border-b">
        <div class="max-w-5xl mx-auto px-4 py-3 flex items-center justify-between">
          <h1 class="text-lg font-semibold text-gray-900">CodeXpert</h1>
          <nav class="flex gap-3 text-sm">
            <.link navigate={~p"/"} class="text-gray-600 hover:text-gray-900">Dashboard</.link>
            <.link navigate={~p"/templates"} class="text-gray-600 hover:text-gray-900">Templates</.link>
            <.link navigate={~p"/schedules"} class="text-gray-600 hover:text-gray-900">Schedules</.link>
            <.link navigate={~p"/settings"} class="text-gray-900 font-medium">Settings</.link>
          </nav>
          <span class="text-sm text-gray-600">{Map.get(assigns, :current_user, %{email: ""}).email}</span>
        </div>
      </header>

      <div class="max-w-5xl mx-auto px-4 py-6">
        <h1 class="text-xl font-semibold text-gray-900 mb-6">Settings</h1>

        <div class="flex gap-4 mb-6 border-b">
          <button
            phx-click="switch_tab"
            phx-value-tab="general"
            class={"pb-2 text-sm border-b-2 #{if @tab == :general, do: "border-blue-600 text-blue-600", else: "border-transparent text-gray-600"}"}
          >
            General
          </button>
          <button
            phx-click="switch_tab"
            phx-value-tab="integrations"
            class={"pb-2 text-sm border-b-2 #{if @tab == :integrations, do: "border-blue-600 text-blue-600", else: "border-transparent text-gray-600"}"}
          >
            Integrations
          </button>
          <button
            phx-click="switch_tab"
            phx-value-tab="notifications"
            class={"pb-2 text-sm border-b-2 #{if @tab == :notifications, do: "border-blue-600 text-blue-600", else: "border-transparent text-gray-600"}"}
          >
            Notifications
          </button>
        </div>

        <%= if @tab == :general do %>
          <.general_tab org={@org} />
        <% end %>

        <%= if @tab == :integrations do %>
          <.integrations_tab />
        <% end %>

        <%= if @tab == :notifications do %>
          <.notifications_tab />
        <% end %>
      </div>
    </div>
    """
  end

  defp general_tab(assigns) do
    ~H"""
    <div class="bg-white rounded-lg shadow p-6 space-y-4">
      <h2 class="text-lg font-semibold text-gray-900">Organization</h2>

      <div>
        <label class="block text-sm font-medium text-gray-700 mb-1">Organization Name</label>
        <input
          type="text"
          name="org_name"
          value={@org && @org.name || "Not set"}
          phx-change="set_org_name"
          class="w-full px-3 py-2 border rounded-lg"
        />
      </div>

      <div>
        <label class="block text-sm font-medium text-gray-700 mb-1">Plan</label>
        <p class="text-gray-600">{@org && @org.plan || "—"}
          <span class="text-gray-400 text-sm">(Free — upgrade coming soon)</span>
        </p>
      </div>
    </div>
    """
  end

  defp integrations_tab(assigns) do
    ~H"""
    <div class="bg-white rounded-lg shadow p-6 space-y-4">
      <h2 class="text-lg font-semibold text-gray-900">API Keys</h2>

      <div>
        <label class="block text-sm font-medium text-gray-700 mb-1">Serper API Key (Search)</label>
        <input
          type="password"
          name="serper_key"
          phx-change="set_serper_key"
          placeholder="Enter Serper API key"
          class="w-full px-3 py-2 border rounded-lg"
        />
        <p class="text-xs text-gray-400 mt-1">Required for web search. Get one at serper.dev</p>
      </div>

      <div>
        <label class="block text-sm font-medium text-gray-700 mb-1">Anthropic API Key (LLM)</label>
        <input
          type="password"
          name="anthropic_key"
          phx-change="set_anthropic_key"
          placeholder="Enter Anthropic API key"
          class="w-full px-3 py-2 border rounded-lg"
        />
        <p class="text-xs text-gray-400 mt-1">For Claude models. Get one at console.anthropic.com</p>
      </div>

      <div>
        <label class="block text-sm font-medium text-gray-700 mb-1">Webhook URL (企业微信 / 飞书)</label>
        <input
          type="text"
          name="webhook_url"
          value={System.get_env("WEBHOOK_URL", "")}
          phx-change="set_webhook"
          placeholder="https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=..."
          class="w-full px-3 py-2 border rounded-lg"
        />
        <button
          phx-click="save_webhook"
          class="mt-2 px-4 py-1.5 bg-blue-600 text-white rounded text-sm hover:bg-blue-700"
        >
          Save
        </button>
      </div>
    </div>
    """
  end

  defp notifications_tab(assigns) do
    ~H"""
    <div class="bg-white rounded-lg shadow p-6 space-y-4">
      <h2 class="text-lg font-semibold text-gray-900">Notifications</h2>

      <form phx-submit="save_email_notify">
        <div>
          <label class="block text-sm font-medium text-gray-700 mb-1">Notification Email</label>
          <input
            type="email"
            name="email"
            value=""
            placeholder="you@example.com"
            class="w-full px-3 py-2 border rounded-lg"
          />
        </div>
        <button type="submit" class="mt-3 px-4 py-1.5 bg-blue-600 text-white rounded text-sm hover:bg-blue-700">
          Save Email
        </button>
      </form>

      <div class="border-t pt-4 mt-6">
        <h3 class="font-medium text-gray-700 mb-2">Webhook Notifications</h3>
        <p class="text-sm text-gray-500">
          When a webhook URL is configured in Integrations, a POST notification is sent on each report completion.
          Supports enterprise chat integrations including 企业微信, 飞书/Lark, 钉钉, and Slack.
        </p>
      </div>
    </div>
    """
  end

  defp get_org(socket) do
    case ExAutoresearch.Accounts.list_user_organizations(socket.assigns.current_user.id) do
      [org | _] -> org
      _ -> nil
    end
  end
end
