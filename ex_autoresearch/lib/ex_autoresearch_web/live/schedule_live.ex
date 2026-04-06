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
          Ash.read!(Ash.Query.sort(Research.Template, inserted_at: :desc))

        id ->
          Ash.read!(
            Ash.Query.filter(Research.Template, organization_id == ^id)
            |> Ash.Query.sort(inserted_at: :desc)
          )
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
    <div class="min-h-screen bg-gray-50">
      <header class="bg-white border-b">
        <div class="max-w-5xl mx-auto px-4 py-3 flex items-center justify-between">
          <h1 class="text-lg font-semibold text-gray-900">CodeXpert</h1>
          <nav class="flex gap-3 text-sm">
            <.link navigate={~p"/"} class="text-gray-600 hover:text-gray-900">Dashboard</.link>
            <.link navigate={~p"/templates"} class="text-gray-600 hover:text-gray-900">Templates</.link>
            <.link navigate={~p"/schedules"} class="text-gray-900 font-medium">Schedules</.link>
            <.link navigate={~p"/settings"} class="text-gray-600 hover:text-gray-900">Settings</.link>
          </nav>
          <span class="text-sm text-gray-600">{Map.get(assigns, :current_user, %{email: ""}).email}</span>
        </div>
      </header>

      <div class="max-w-5xl mx-auto px-4 py-6">
        <h1 class="text-xl font-semibold text-gray-900 mb-6">Scheduled Research</h1>

        <div class="mb-8">
          <h2 class="text-lg font-semibold text-gray-800 mb-3 flex items-center gap-2">
            <span class="h-3 w-3 bg-green-500 rounded-full animate-pulse"></span>
            Active Schedules
          </h2>

          <%= if @scheduled == [] do %>
            <p class="text-gray-400 text-sm">No active schedules.</p>
          <% else %>
            <div class="space-y-2">
              <%= for t <- @scheduled do %>
                <div class="bg-white rounded-lg shadow p-4 flex items-center justify-between">
                  <div>
                    <h3 class="font-medium text-gray-900">{t.name}</h3>
                    <p class="text-sm text-gray-500 mt-1">{t.schedule_cron || "No cron set"}</p>
                  </div>
                  <button
                    phx-click="toggle"
                    phx-value-id={t.id}
                    class="px-3 py-1.5 bg-red-100 text-red-700 rounded text-sm hover:bg-red-200"
                  >
                    Disable
                  </button>
                </div>
              <% end %>
            </div>
          <% end %>
        </div>

        <div>
          <h2 class="text-lg font-semibold text-gray-600 mb-3">Inactive Templates</h2>

          <%= if @unscheduled == [] do %>
            <p class="text-gray-400 text-sm">All templates are scheduled.</p>
          <% else %>
            <div class="space-y-2">
              <%= for t <- @unscheduled do %>
                <div class="bg-white rounded-lg shadow p-4 flex items-center justify-between">
                  <div>
                    <h3 class="font-medium text-gray-900">{t.name}</h3>
                    <p class="text-sm text-gray-500 mt-1">{t.category}</p>
                  </div>
                  <button
                    phx-click="toggle"
                    phx-value-id={t.id}
                    class="px-3 py-1.5 bg-green-100 text-green-700 rounded text-sm hover:bg-green-200"
                  >
                    Enable
                  </button>
                </div>
              <% end %>
            </div>
          <% end %>
        </div>
      </div>
    </div>
    """
  end
end
