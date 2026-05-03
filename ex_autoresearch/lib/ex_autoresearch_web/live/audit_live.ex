defmodule ExAutoresearchWeb.AuditLive do
  @moduledoc """
  Audit log viewer — newest-first table of organization-scoped audit events.

  Listens on the `"audit:events"` PubSub topic so newly-recorded events appear
  without a manual refresh. Filtering is client-side over the most recent
  N events; for deeper history, expose a date range later.
  """

  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.Audit

  @page_size 100
  @event_types [
    :research_started,
    :report_completed,
    :report_failed,
    :report_exported,
    :template_created,
    :template_updated,
    :template_deleted,
    :user_logged_in
  ]

  @impl true
  def mount(_params, _session, socket) do
    if connected?(socket),
      do: Phoenix.PubSub.subscribe(ExAutoresearch.PubSub, "audit:events")

    org_id = socket.assigns[:current_org_id]
    events = load_events(org_id)

    {:ok,
     socket
     |> assign(:events, events)
     |> assign(:filter_event_type, :all)
     |> assign(:org_id, org_id)
     |> assign(:event_types, @event_types)}
  end

  @impl true
  def handle_event("filter", %{"event_type" => "all"}, socket) do
    {:noreply, assign(socket, :filter_event_type, :all)}
  end

  def handle_event("filter", %{"event_type" => et}, socket) do
    {:noreply, assign(socket, :filter_event_type, String.to_existing_atom(et))}
  end

  def handle_event("refresh", _params, socket) do
    events = load_events(socket.assigns.org_id)
    {:noreply, assign(socket, :events, events)}
  end

  @impl true
  def handle_info({:audit_event_recorded, event}, socket) do
    if event.organization_id == socket.assigns.org_id do
      {:noreply,
       assign(socket, :events, [event | socket.assigns.events] |> Enum.take(@page_size))}
    else
      {:noreply, socket}
    end
  end

  def handle_info(_, socket), do: {:noreply, socket}

  defp load_events(nil), do: []

  defp load_events(org_id) do
    case Audit.list(org_id, limit: @page_size) do
      {:ok, events} -> events
      _ -> []
    end
  end

  defp visible_events(%{filter_event_type: :all, events: events}), do: events

  defp visible_events(%{filter_event_type: et, events: events}),
    do: Enum.filter(events, &(&1.event_type == et))

  @impl true
  def render(assigns) do
    assigns = assign(assigns, :visible, visible_events(assigns))

    ~H"""
    <Layouts.app
      flash={@flash}
      current_user={@current_user}
      active_nav={:audit}
      container="max-w-6xl"
    >
      <div class="space-y-6">
        <div class="flex items-start justify-between gap-4 flex-wrap">
          <div>
            <h1 class="text-2xl font-bold tracking-tight">Audit Log</h1>

            <p class="text-sm text-base-content/60 mt-1">
              Append-only record of every research, report, and template action in this organization.
              <span class="font-mono text-xs opacity-70">
                Showing {length(@visible)} of {length(@events)} events.
              </span>
            </p>
          </div>

          <div class="flex items-center gap-2">
            <button
              type="button"
              phx-click="refresh"
              class="btn btn-ghost btn-sm gap-2"
              id="audit-refresh"
            >
              <.icon name="hero-arrow-path" class="size-4" /> Refresh
            </button>
          </div>
        </div>

        <div class="flex flex-wrap gap-2 items-center">
          <span class="text-xs uppercase tracking-wider text-base-content/50">Filter:</span>
          <.filter_chip label="All" active={@filter_event_type == :all} value="all" />
          <%= for et <- @event_types do %>
            <.filter_chip
              label={humanize_event_type(et)}
              active={@filter_event_type == et}
              value={Atom.to_string(et)}
            />
          <% end %>
        </div>

        <%= if @visible == [] do %>
          <.empty_state has_any?={@events != []} filter={@filter_event_type} />
        <% else %>
          <div class="card bg-base-100 border border-base-300 shadow-sm overflow-hidden">
            <div class="overflow-x-auto">
              <table class="table table-zebra table-sm" id="audit-table">
                <thead>
                  <tr>
                    <th class="w-44">Time</th>

                    <th class="w-44">Event</th>

                    <th>Target</th>

                    <th>Summary</th>

                    <th class="w-44">User</th>
                  </tr>
                </thead>

                <tbody>
                  <%= for ev <- @visible do %>
                    <tr id={"audit-row-#{ev.id}"}>
                      <td class="font-mono text-xs whitespace-nowrap">
                        <span title={DateTime.to_iso8601(ev.inserted_at)}>
                          {format_relative(ev.inserted_at)}
                        </span>
                      </td>

                      <td>
                        <span class={["badge badge-sm", event_badge_class(ev.event_type)]}>
                          {humanize_event_type(ev.event_type)}
                        </span>
                      </td>

                      <td class="text-xs">
                        <%= if ev.target_type do %>
                          <span class="text-base-content/60">{ev.target_type}</span>
                          <%= if ev.target_id do %>
                            <span class="font-mono text-base-content/50">
                              {String.slice(ev.target_id || "", 0, 8)}
                            </span>
                          <% end %>
                        <% else %>
                          <span class="text-base-content/30">—</span>
                        <% end %>
                      </td>

                      <td class="text-sm">
                        <span class="line-clamp-1 max-w-md">
                          {ev.summary || "—"}
                        </span>
                      </td>

                      <td class="text-xs font-mono text-base-content/60">
                        {short_user_id(ev.user_id)}
                      </td>
                    </tr>
                  <% end %>
                </tbody>
              </table>
            </div>
          </div>
        <% end %>
      </div>
    </Layouts.app>
    """
  end

  attr :label, :string, required: true
  attr :active, :boolean, required: true
  attr :value, :string, required: true

  defp filter_chip(assigns) do
    ~H"""
    <button
      type="button"
      phx-click="filter"
      phx-value-event_type={@value}
      class={[
        "btn btn-xs normal-case",
        if(@active, do: "btn-primary", else: "btn-ghost border border-base-300")
      ]}
    >
      {@label}
    </button>
    """
  end

  attr :has_any?, :boolean, required: true
  attr :filter, :atom, required: true

  defp empty_state(assigns) do
    ~H"""
    <div class="card bg-base-100 border border-dashed border-base-300">
      <div class="card-body items-center text-center py-16">
        <.icon name="hero-document-text" class="size-12 text-base-content/30" />
        <%= if @has_any? do %>
          <p class="text-sm text-base-content/70 mt-3">
            No events match the <span class="font-medium">{humanize_event_type(@filter)}</span>
            filter.
          </p>

          <button type="button" phx-click="filter" phx-value-event_type="all" class="btn btn-sm mt-3">
            Clear filter
          </button>
        <% else %>
          <p class="text-sm text-base-content/70 mt-3">
            No audit events yet.
          </p>

          <p class="text-xs text-base-content/50 mt-1 max-w-md">
            Events accumulate as users run research, export reports, or manage templates.
            This log is append-only and scoped to your organization.
          </p>

          <.link navigate={~p"/"} class="btn btn-primary btn-sm mt-4">
            Start a research query
          </.link>
        <% end %>
      </div>
    </div>
    """
  end

  defp humanize_event_type(:all), do: "All"

  defp humanize_event_type(et) when is_atom(et) do
    et
    |> Atom.to_string()
    |> String.replace("_", " ")
    |> String.split()
    |> Enum.map_join(" ", &String.capitalize/1)
  end

  defp event_badge_class(:research_started), do: "badge-info"
  defp event_badge_class(:report_completed), do: "badge-success"
  defp event_badge_class(:report_failed), do: "badge-error"
  defp event_badge_class(:report_exported), do: "badge-primary"
  defp event_badge_class(:template_created), do: "badge-accent"
  defp event_badge_class(:template_updated), do: "badge-accent badge-outline"
  defp event_badge_class(:template_deleted), do: "badge-warning"
  defp event_badge_class(:user_logged_in), do: "badge-ghost"
  defp event_badge_class(_), do: "badge-ghost"

  defp format_relative(%DateTime{} = dt) do
    diff = DateTime.diff(DateTime.utc_now(), dt, :second)

    cond do
      diff < 5 -> "just now"
      diff < 60 -> "#{diff}s ago"
      diff < 3600 -> "#{div(diff, 60)}m ago"
      diff < 86_400 -> "#{div(diff, 3600)}h ago"
      diff < 604_800 -> "#{div(diff, 86_400)}d ago"
      true -> Calendar.strftime(dt, "%Y-%m-%d")
    end
  end

  defp format_relative(_), do: "—"

  defp short_user_id(nil), do: "system"

  defp short_user_id(uid) when is_binary(uid) do
    uid |> String.slice(0, 8)
  end

  defp short_user_id(_), do: "—"
end
