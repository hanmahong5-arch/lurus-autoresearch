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
    tab_atom =
      case tab do
        "general" -> :general
        "integrations" -> :integrations
        "notifications" -> :notifications
        _ -> :general
      end

    {:noreply, assign(socket, :tab, tab_atom)}
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
    <Layouts.app
      flash={@flash}
      current_user={assigns[:current_user]}
      active_nav={:settings}
      container="max-w-4xl"
    >
      <div class="space-y-6">
        <div>
          <h1 class="text-2xl font-bold tracking-tight">Settings</h1>
          <p class="text-sm text-base-content/60 mt-1">
            Configure your organization, integrations, and notifications.
          </p>
        </div>

        <div role="tablist" class="tabs tabs-bordered">
          <a
            role="tab"
            phx-click="switch_tab"
            phx-value-tab="general"
            class={["tab cursor-pointer", @tab == :general && "tab-active"]}
          >
            General
          </a>
          <a
            role="tab"
            phx-click="switch_tab"
            phx-value-tab="integrations"
            class={["tab cursor-pointer", @tab == :integrations && "tab-active"]}
          >
            Integrations
          </a>
          <a
            role="tab"
            phx-click="switch_tab"
            phx-value-tab="notifications"
            class={["tab cursor-pointer", @tab == :notifications && "tab-active"]}
          >
            Notifications
          </a>
        </div>

        <%= if @tab == :general do %>
          <.general_tab org={@org} />
        <% end %>

        <%= if @tab == :integrations do %>
          <.integrations_tab />
        <% end %>

        <%= if @tab == :notifications do %>
          <.notifications_tab notifications_email={@notifications_email} />
        <% end %>

        <ExAutoresearchWeb.Components.VersionBadge.section id="settings-version-section" />
      </div>
    </Layouts.app>
    """
  end

  defp general_tab(assigns) do
    ~H"""
    <section class="card bg-base-100 border border-base-300 shadow-sm">
      <div class="card-body space-y-5">
        <div>
          <h2 class="card-title text-lg">Organization</h2>
          <p class="text-xs text-base-content/60 mt-0.5">
            Identity used in reports and webhook notifications.
          </p>
        </div>

        <div class="form-control">
          <label class="label" for="org-name-input">
            <span class="label-text font-medium">Organization Name</span>
          </label>
          <input
            id="org-name-input"
            type="text"
            name="org_name"
            value={(@org && @org.name) || ""}
            placeholder="Not set"
            phx-change="set_org_name"
            phx-debounce="500"
            class="input input-bordered w-full"
          />
          <label class="label">
            <span class="label-text-alt text-base-content/50">
              Auto-saved as you type.
            </span>
          </label>
        </div>

        <div class="form-control">
          <label class="label">
            <span class="label-text font-medium">Plan</span>
          </label>
          <div class="flex items-center gap-2">
            <span class="font-mono text-sm">{(@org && @org.plan) || "—"}</span>
            <span class="badge badge-ghost badge-sm">Free — upgrade coming soon</span>
          </div>
        </div>
      </div>
    </section>
    """
  end

  defp integrations_tab(assigns) do
    ~H"""
    <section class="card bg-base-100 border border-base-300 shadow-sm">
      <div class="card-body space-y-5">
        <div>
          <h2 class="card-title text-lg">API Keys & Integrations</h2>
          <p class="text-xs text-base-content/60 mt-0.5">
            Stored locally on this server only. Never transmitted to third parties.
          </p>
        </div>

        <div class="form-control">
          <label class="label" for="serper-key-input">
            <span class="label-text font-medium">Serper API Key</span>
            <span class="label-text-alt badge badge-soft badge-info badge-sm">Search</span>
          </label>
          <input
            id="serper-key-input"
            type="password"
            name="serper_key"
            phx-change="set_serper_key"
            phx-debounce="800"
            placeholder="Enter Serper API key"
            class="input input-bordered w-full font-mono text-sm"
          />
          <label class="label">
            <span class="label-text-alt text-base-content/50">
              Required for web search.
              <a href="https://serper.dev" target="_blank" rel="noopener" class="link link-primary">
                Get one at serper.dev
              </a>
            </span>
          </label>
        </div>

        <div class="form-control">
          <label class="label" for="anthropic-key-input">
            <span class="label-text font-medium">Anthropic API Key</span>
            <span class="label-text-alt badge badge-soft badge-secondary badge-sm">LLM</span>
          </label>
          <input
            id="anthropic-key-input"
            type="password"
            name="anthropic_key"
            phx-change="set_anthropic_key"
            phx-debounce="800"
            placeholder="Enter Anthropic API key"
            class="input input-bordered w-full font-mono text-sm"
          />
          <label class="label">
            <span class="label-text-alt text-base-content/50">
              For Claude models.
              <a
                href="https://console.anthropic.com"
                target="_blank"
                rel="noopener"
                class="link link-primary"
              >
                console.anthropic.com
              </a>
            </span>
          </label>
        </div>

        <div class="divider my-1"></div>

        <div class="form-control">
          <label class="label" for="webhook-url-input">
            <span class="label-text font-medium">Webhook URL</span>
            <span class="label-text-alt badge badge-ghost badge-sm">企业微信 / 飞书 / Slack</span>
          </label>
          <input
            id="webhook-url-input"
            type="text"
            name="webhook_url"
            value={System.get_env("WEBHOOK_URL", "")}
            phx-change="set_webhook"
            phx-debounce="500"
            placeholder="https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=..."
            class="input input-bordered w-full font-mono text-sm"
          />
          <div class="mt-2">
            <button phx-click="save_webhook" class="btn btn-primary btn-sm">
              Save webhook
            </button>
          </div>
        </div>
      </div>
    </section>
    """
  end

  defp notifications_tab(assigns) do
    ~H"""
    <section class="card bg-base-100 border border-base-300 shadow-sm">
      <div class="card-body space-y-5">
        <div>
          <h2 class="card-title text-lg">Notifications</h2>
          <p class="text-xs text-base-content/60 mt-0.5">
            Where to send a ping when a report completes.
          </p>
        </div>

        <form phx-submit="save_email_notify" class="form-control space-y-3">
          <label class="label" for="notify-email-input">
            <span class="label-text font-medium">Notification Email</span>
          </label>
          <input
            id="notify-email-input"
            type="email"
            name="email"
            value={@notifications_email}
            placeholder="you@example.com"
            class="input input-bordered w-full"
          />
          <div>
            <button type="submit" class="btn btn-primary btn-sm">
              Save email
            </button>
          </div>
        </form>

        <div class="divider my-1"></div>

        <div>
          <h3 class="font-medium mb-1">Webhook Notifications</h3>
          <p class="text-sm text-base-content/70">
            When a webhook URL is configured in <strong>Integrations</strong>,
            a POST notification is sent on each report completion. Supports
            <span class="font-medium">企业微信</span>,
            <span class="font-medium">飞书 / Lark</span>,
            <span class="font-medium">钉钉</span>, and
            <span class="font-medium">Slack</span>.
          </p>
        </div>
      </div>
    </section>
    """
  end

  defp get_org(socket) do
    case ExAutoresearch.Accounts.list_user_organizations(socket.assigns.current_user.id) do
      [org | _] -> org
      _ -> nil
    end
  end
end
