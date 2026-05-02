defmodule ExAutoresearchWeb.ScheduleLive do
  @moduledoc """
  LiveView for managing scheduled research tasks.

  Shows templates with cron schedules and allows enabling/discheduling.

  Note: Actual Oban cron scheduling is handled at the application level.
  This LiveView manages the template.enabled flag and cron expression.
  """

  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.Research
  require Ash.Query

  @doc """
  Common examples for cron expressions:

    Every Monday at 9am:    "0 9 * * 1"
    Every day at 6pm:       "0 18 * * *"
    Every 2 hours:           "0 */2 * * *"
    Weekdays at 10am:        "0 10 * * 1-5"
    First of month at 8am:   "0 8 1 * *"
  """

  @impl true
  def mount(_params, _session, socket) do
    org_id = socket.assigns[:current_org_id]

    templates =
      case org_id do
        nil ->
          []

        id ->
          Research.Template
          |> Ash.Query.sort(inserted_at: :desc)
          |> Ash.read!(tenant: id)
      end

    scheduled = Enum.filter(templates, & &1.enabled)
    unscheduled = Enum.reject(templates, & &1.enabled)

    {:ok,
     socket
     |> assign(:scheduled, scheduled)
     |> assign(:unscheduled, unscheduled)}
  end

  @impl true
  def handle_event("toggle", %{"id" => id}, socket) do
    case Ash.get(Research.Template, id) do
      {:ok, template} ->
        Ash.update!(template, %{enabled: !template.enabled}, action: :toggle)
        {:noreply, push_navigate(socket, to: ~p"/schedules")}

      _ ->
        {:noreply, put_flash(socket, :error, "Template not found")}
    end
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app
      flash={@flash}
      current_user={assigns[:current_user]}
      active_nav={:schedules}
      container="max-w-5xl"
    >
      <div class="space-y-8">
        <div>
          <h1 class="text-2xl font-bold tracking-tight">Scheduled Research</h1>
          <p class="text-sm text-base-content/60 mt-1">
            Templates with cron schedules run automatically in the background.
          </p>
        </div>

        <section>
          <h2 class="text-sm font-semibold uppercase tracking-wider text-base-content/60 mb-3 flex items-center gap-2">
            <span class="h-2 w-2 bg-success rounded-full animate-pulse" aria-hidden="true"></span>
            Active Schedules
          </h2>

          <%= if @scheduled == [] do %>
            <div class="card bg-base-100 border border-dashed border-base-300">
              <div class="card-body items-center text-center py-8">
                <p class="text-sm text-base-content/60">No active schedules.</p>
                <p class="text-xs text-base-content/50 mt-1">
                  Enable a template below to schedule it.
                </p>
              </div>
            </div>
          <% else %>
            <div class="space-y-2">
              <%= for t <- @scheduled do %>
                <div class="card bg-base-100 border border-base-300 shadow-sm">
                  <div class="card-body py-4 px-5 flex flex-row items-center justify-between gap-4">
                    <div class="min-w-0">
                      <h3 class="font-medium truncate">{t.name}</h3>
                      <p class="text-xs text-base-content/60 mt-1 font-mono">
                        {t.schedule_cron || "No cron set"}
                      </p>
                    </div>
                    <button
                      phx-click="toggle"
                      phx-value-id={t.id}
                      class="btn btn-sm btn-soft btn-error shrink-0"
                    >
                      Disable
                    </button>
                  </div>
                </div>
              <% end %>
            </div>
          <% end %>
        </section>

        <section>
          <h2 class="text-sm font-semibold uppercase tracking-wider text-base-content/60 mb-3">
            Inactive Templates
          </h2>

          <%= if @unscheduled == [] do %>
            <div class="card bg-base-100 border border-dashed border-base-300">
              <div class="card-body items-center text-center py-8">
                <p class="text-sm text-base-content/60">All templates are scheduled.</p>
              </div>
            </div>
          <% else %>
            <div class="space-y-2">
              <%= for t <- @unscheduled do %>
                <div class="card bg-base-100 border border-base-300 shadow-sm">
                  <div class="card-body py-4 px-5 flex flex-row items-center justify-between gap-4">
                    <div class="min-w-0">
                      <h3 class="font-medium truncate">{t.name}</h3>
                      <p class="text-xs text-base-content/60 mt-1">{t.category}</p>
                    </div>
                    <button
                      phx-click="toggle"
                      phx-value-id={t.id}
                      class="btn btn-sm btn-soft btn-success shrink-0"
                    >
                      Enable
                    </button>
                  </div>
                </div>
              <% end %>
            </div>
          <% end %>
        </section>
      </div>
    </Layouts.app>
    """
  end
end
