defmodule ExAutoresearchWeb.SettingsLive do
  @moduledoc """
  LiveView for organization settings: notification config, API keys, team management.
  """

  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.Settings

  @impl true
  def mount(_params, _session, socket) do
    org = get_org(socket)

    socket =
      Enum.reduce(Settings.field_ids(), socket, fn id, s ->
        assign(s, id, Settings.get(id))
      end)

    {:ok,
     socket
     |> assign(:org, org)
     |> assign(:webhook_url, System.get_env("WEBHOOK_URL", ""))
     |> assign(:notifications_email, "")
     |> assign(:search_backend, Settings.search_backend())
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

  def handle_event("set_field", %{"field" => name, "value" => v}, socket) do
    field_id = String.to_existing_atom(name)
    Settings.put(field_id, v)
    socket = assign(socket, field_id, v)

    socket =
      case Settings.field(field_id) do
        %{flash: msg} when is_binary(msg) -> put_flash(socket, :info, msg)
        _ -> socket
      end

    {:noreply, socket}
  end

  def handle_event("set_search_backend", %{"value" => v}, socket) do
    Settings.put_search_backend(v)
    {:noreply, assign(socket, :search_backend, v) |> put_flash(:info, "Search backend: #{v}")}
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
          <.integrations_tab
            openai_compat_base_url={@openai_compat_base_url}
            openai_compat_model_main={@openai_compat_model_main}
            openai_compat_model_fast={@openai_compat_model_fast}
            search_backend={@search_backend}
            searxng_base_url={@searxng_base_url}
          />
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
            <span class="label-text-alt text-base-content/50">Auto-saved as you type.</span>
          </label>
        </div>

        <div class="form-control">
          <label class="label"><span class="label-text font-medium">Plan</span></label>
          <div class="flex items-center gap-2">
            <.org_plan plan={@org && @org.plan} />
            <span class="text-xs text-base-content/50">— upgrade coming soon</span>
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

        <div class="rounded-box border border-primary/30 bg-primary/5 p-4 space-y-4">
          <div class="flex items-baseline justify-between">
            <h3 class="font-semibold text-sm flex items-center gap-2">
              OpenAI-Compatible Gateway <span class="badge badge-primary badge-sm">Recommended</span>
            </h3>
            <span class="text-xs text-base-content/60">中转站 / vLLM / LM Studio</span>
          </div>

          <p class="text-xs text-base-content/70">
            Two model tiers: <strong>Main</strong>
            handles planning, deep evaluation, and final
            report writing; <strong>Fast</strong>
            handles per-investigation sub-query generation.
            Pair e.g. <code class="text-xs">deepseek-v4-pro</code>
            / <code class="text-xs">deepseek-v4-flash</code>.
          </p>

          <form
            phx-change="set_field"
            phx-value-field="openai_compat_base_url"
            autocomplete="off"
            class="form-control"
          >
            <label class="label" for="oc-base-url-input">
              <span class="label-text font-medium text-sm">Base URL</span>
            </label>
            <input
              id="oc-base-url-input"
              type="text"
              name="value"
              value={@openai_compat_base_url}
              phx-debounce="500"
              placeholder="https://newapi.lurus.cn/v1"
              class="input input-bordered w-full font-mono text-sm"
            />
            <label class="label">
              <span class="label-text-alt text-base-content/50">
                Without trailing slash. <code>/chat/completions</code> is appended automatically.
              </span>
            </label>
          </form>

          <form
            phx-change="set_field"
            phx-value-field="openai_compat_api_key"
            autocomplete="off"
            class="form-control"
          >
            <label class="label" for="oc-api-key-input">
              <span class="label-text font-medium text-sm">API Key</span>
            </label>
            <input
              id="oc-api-key-input"
              type="password"
              name="value"
              phx-debounce="800"
              placeholder="sk-..."
              class="input input-bordered w-full font-mono text-sm"
            />
          </form>

          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <form
              phx-change="set_field"
              phx-value-field="openai_compat_model_main"
              autocomplete="off"
              class="form-control"
            >
              <label class="label" for="oc-model-main-input">
                <span class="label-text font-medium text-sm">Model (Main)</span>
                <span class="label-text-alt badge badge-soft badge-primary badge-sm">Pro</span>
              </label>
              <input
                id="oc-model-main-input"
                type="text"
                name="value"
                value={@openai_compat_model_main}
                phx-debounce="500"
                placeholder="deepseek-v4-pro"
                class="input input-bordered w-full font-mono text-sm"
              />
            </form>

            <form
              phx-change="set_field"
              phx-value-field="openai_compat_model_fast"
              autocomplete="off"
              class="form-control"
            >
              <label class="label" for="oc-model-fast-input">
                <span class="label-text font-medium text-sm">Model (Fast)</span>
                <span class="label-text-alt badge badge-soft badge-secondary badge-sm">Flash</span>
              </label>
              <input
                id="oc-model-fast-input"
                type="text"
                name="value"
                value={@openai_compat_model_fast}
                phx-debounce="500"
                placeholder="deepseek-v4-flash"
                class="input input-bordered w-full font-mono text-sm"
              />
            </form>
          </div>
        </div>

        <div class="divider my-1"></div>

        <form phx-change="set_search_backend" class="form-control">
          <label class="label" for="search-backend-select">
            <span class="label-text font-medium">Web Search Backend</span>
          </label>
          <select
            id="search-backend-select"
            name="value"
            class="select select-bordered w-full"
          >
            <option value="searxng" selected={@search_backend == "searxng"}>
              SearXNG · self-hosted metasearch (recommended)
            </option>

            <option value="duckduckgo" selected={@search_backend == "duckduckgo"}>
              DuckDuckGo · zero-config (may be rate-limited)
            </option>

            <option value="serper" selected={@search_backend == "serper"}>
              Serper · Google results (requires API key below)
            </option>
          </select>
          <label class="label">
            <span class="label-text-alt text-base-content/50">
              Auto-falls back to DuckDuckGo if the selected backend errors.
            </span>
          </label>
        </form>

        <%= if @search_backend == "searxng" do %>
          <form
            phx-change="set_field"
            phx-value-field="searxng_base_url"
            autocomplete="off"
            class="form-control"
          >
            <label class="label" for="searxng-base-url-input">
              <span class="label-text font-medium text-sm">SearXNG Base URL</span>
            </label>
            <input
              id="searxng-base-url-input"
              type="text"
              name="value"
              value={@searxng_base_url}
              phx-debounce="500"
              placeholder="http://localhost:8888"
              class="input input-bordered w-full font-mono text-sm"
            />
            <label class="label">
              <span class="label-text-alt text-base-content/50">
                Run with: <code class="text-xs">docker run -d -p 8888:8080 searxng/searxng</code>
                — make sure the instance has <code>formats: [html, json]</code>
                in its <code>settings.yml</code>.
              </span>
            </label>
          </form>
        <% end %>

        <form
          phx-change="set_field"
          phx-value-field="serper_api_key"
          autocomplete="off"
          class="form-control"
        >
          <label class="label" for="serper-key-input">
            <span class="label-text font-medium">Serper API Key</span>
            <span class="label-text-alt badge badge-soft badge-info badge-sm">Search</span>
          </label>
          <input
            id="serper-key-input"
            type="password"
            name="value"
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
        </form>

        <form
          phx-change="set_field"
          phx-value-field="anthropic_api_key"
          autocomplete="off"
          class="form-control"
        >
          <label class="label" for="anthropic-key-input">
            <span class="label-text font-medium">Anthropic API Key</span>
            <span class="label-text-alt badge badge-soft badge-secondary badge-sm">LLM</span>
          </label>
          <input
            id="anthropic-key-input"
            type="password"
            name="value"
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
        </form>

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
            <button phx-click="save_webhook" class="btn btn-primary btn-sm">Save webhook</button>
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
          <div><button type="submit" class="btn btn-primary btn-sm">Save email</button></div>
        </form>

        <div class="divider my-1"></div>

        <div>
          <h3 class="font-medium mb-1">Webhook Notifications</h3>

          <p class="text-sm text-base-content/70">
            When a webhook URL is configured in <strong>Integrations</strong>,
            a POST notification is sent on each report completion. Supports <span class="font-medium">企业微信</span>, <span class="font-medium">飞书 / Lark</span>, <span class="font-medium">钉钉</span>, and <span class="font-medium">Slack</span>.
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
