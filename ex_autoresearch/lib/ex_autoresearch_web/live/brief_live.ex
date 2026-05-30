defmodule ExAutoresearchWeb.BriefLive do
  @moduledoc """
  LiveView for managing Research Briefs.

  Features:
  - List the org's briefs (stream, tenant: org_id)
  - Create a brief via an inline form
  - Enable/Disable toggle via the :toggle action
  - "Run now" button that enqueues research non-blocking via enqueue_brief/1
  """

  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.Research.Brief
  alias ExAutoresearch.DeepResearch.ResearchEngine
  alias ExAutoresearchWeb.Badges

  require Ash.Query

  @impl true
  def mount(_params, _session, socket) do
    org_id = socket.assigns[:current_org_id]

    briefs =
      case org_id do
        nil ->
          []

        id ->
          Brief
          |> Ash.Query.sort(inserted_at: :desc)
          |> Ash.read!(tenant: id)
      end

    {:ok,
     socket
     |> stream(:briefs, briefs)
     |> assign(:org_id, org_id)
     |> assign(:brief_count, length(briefs))
     |> assign(:show_form, false)
     |> assign(
       :form,
       %{
         "name" => "",
         "question" => "",
         "category" => "custom",
         "cadence" => "manual",
         "model" => "claude-sonnet-4",
         "max_depth" => 3,
         "max_sources" => 25
       }
     )}
  end

  @impl true
  def handle_event("toggle_form", _params, socket) do
    {:noreply, assign(socket, :show_form, !socket.assigns.show_form)}
  end

  def handle_event("set_name", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: Map.put(socket.assigns.form, "name", v))}

  def handle_event("set_question", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: Map.put(socket.assigns.form, "question", v))}

  def handle_event("set_category", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: Map.put(socket.assigns.form, "category", v))}

  def handle_event("set_cadence", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: Map.put(socket.assigns.form, "cadence", v))}

  def handle_event("set_model", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: Map.put(socket.assigns.form, "model", v))}

  def handle_event("set_depth", %{"value" => v}, socket),
    do:
      {:noreply,
       assign(socket, form: Map.put(socket.assigns.form, "max_depth", String.to_integer(v)))}

  def handle_event("set_sources", %{"value" => v}, socket),
    do:
      {:noreply,
       assign(socket, form: Map.put(socket.assigns.form, "max_sources", String.to_integer(v)))}

  def handle_event("save_brief", _params, socket) do
    org_id = socket.assigns.org_id

    if is_nil(org_id) do
      {:noreply, put_flash(socket, :error, "No organization found. Please log in.")}
    else
      params = socket.assigns.form

      brief_params = %{
        name: params["name"],
        question: params["question"],
        category: String.to_atom(params["category"]),
        cadence: String.to_atom(params["cadence"]),
        model: params["model"],
        max_depth: params["max_depth"],
        max_sources: params["max_sources"],
        organization_id: org_id,
        enabled: false
      }

      case Ash.create(Brief, brief_params, action: :create, tenant: org_id) do
        {:ok, brief} ->
          {:noreply,
           socket
           |> stream_insert(:briefs, brief, at: 0)
           |> assign(:brief_count, socket.assigns.brief_count + 1)
           |> assign(:show_form, false)
           |> put_flash(:info, "Brief \"#{brief.name}\" created")}

        {:error, reason} ->
          {:noreply, put_flash(socket, :error, "Failed to create brief: #{inspect(reason)}")}
      end
    end
  end

  def handle_event("toggle_enabled", %{"id" => id}, socket) do
    org_id = socket.assigns.org_id

    with {:ok, brief} <- Ash.get(Brief, id, tenant: org_id),
         {:ok, updated} <-
           Ash.update(brief, %{enabled: !brief.enabled}, action: :toggle, tenant: org_id) do
      {:noreply,
       socket
       |> stream_insert(:briefs, updated)
       |> put_flash(:info, "Brief #{if(updated.enabled, do: "enabled", else: "disabled")}")}
    else
      {:error, reason} ->
        {:noreply, put_flash(socket, :error, "Toggle failed: #{inspect(reason)}")}
    end
  end

  def handle_event("run_now", %{"id" => id}, socket) do
    org_id = socket.assigns.org_id

    case Ash.get(Brief, id, tenant: org_id) do
      {:ok, brief} ->
        case ResearchEngine.enqueue_brief(brief) do
          {:ok, _job} ->
            {:noreply, put_flash(socket, :info, "Run queued for \"#{brief.name}\"")}

          {:error, reason} ->
            {:noreply, put_flash(socket, :error, "Failed to enqueue: #{inspect(reason)}")}
        end

      {:error, _} ->
        {:noreply, put_flash(socket, :error, "Brief not found")}
    end
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app
      flash={@flash}
      current_user={assigns[:current_user]}
      active_nav={:briefs}
      container="max-w-5xl"
    >
      <div class="space-y-6">
        <div class="flex items-end justify-between gap-4">
          <div>
            <h1 class="text-2xl font-bold tracking-tight">Research Briefs</h1>
            <p class="text-sm text-base-content/60 mt-1">
              Recurring research topics — run on demand or on a schedule.
            </p>
          </div>
          <button
            id="toggle-form-btn"
            phx-click="toggle_form"
            class={["btn btn-sm", if(@show_form, do: "btn-ghost", else: "btn-primary")]}
          >
            <span :if={!@show_form}>+ New brief</span>
            <span :if={@show_form}>Cancel</span>
          </button>
        </div>

        <%= if @show_form do %>
          <.brief_form form={@form} />
        <% end %>

        <ul id="briefs-list" phx-update="stream" class="space-y-3">
          <li
            :for={{dom_id, brief} <- @streams.briefs}
            id={dom_id}
            class="card bg-base-100 border border-base-300 shadow-sm"
          >
            <div class="card-body py-4 px-5 flex flex-row items-start justify-between gap-4">
              <div class="min-w-0 flex-1 space-y-2">
                <h3 class="font-medium truncate">{brief.name}</h3>
                <p class="text-sm text-base-content/70 line-clamp-2">
                  {String.slice(brief.question, 0, 200)}
                </p>
                <div class="flex flex-wrap gap-2 items-center text-xs">
                  <Badges.category category={brief.category} />
                  <span class="badge badge-sm badge-ghost font-mono">{brief.cadence}</span>
                  <span
                    :if={brief.enabled}
                    class="badge badge-sm badge-success badge-soft"
                  >
                    Enabled
                  </span>
                  <span
                    :if={!brief.enabled}
                    class="badge badge-sm badge-ghost"
                  >
                    Disabled
                  </span>
                  <span
                    :if={brief.last_run_at}
                    class="text-base-content/50"
                  >
                    Last: {format_dt(brief.last_run_at)}
                  </span>
                  <span
                    :if={brief.next_run_at}
                    class="text-base-content/50"
                  >
                    Next: {format_dt(brief.next_run_at)}
                  </span>
                </div>
              </div>
              <div class="flex gap-2 shrink-0">
                <button
                  id={"run-now-#{brief.id}"}
                  phx-click="run_now"
                  phx-value-id={brief.id}
                  class="btn btn-primary btn-sm"
                >
                  Run now
                </button>
                <button
                  id={"toggle-#{brief.id}"}
                  phx-click="toggle_enabled"
                  phx-value-id={brief.id}
                  class={[
                    "btn btn-sm",
                    if(brief.enabled, do: "btn-soft btn-warning", else: "btn-soft btn-success")
                  ]}
                >
                  {if brief.enabled, do: "Disable", else: "Enable"}
                </button>
              </div>
            </div>
          </li>
        </ul>

        <div
          :if={@brief_count == 0}
          class="card bg-base-100 border border-dashed border-base-300"
        >
          <div class="card-body items-center text-center py-12">
            <p class="text-sm text-base-content/60">No briefs yet.</p>
            <p class="text-xs text-base-content/50 mt-1">
              Create one to run recurring research on a schedule or on demand.
            </p>
          </div>
        </div>
      </div>
    </Layouts.app>
    """
  end

  defp brief_form(assigns) do
    ~H"""
    <section class="card bg-base-100 border border-base-300 shadow-sm">
      <div class="card-body space-y-5">
        <h2 class="card-title text-lg">New Brief</h2>

        <div class="form-control">
          <label class="label" for="brief-name">
            <span class="label-text font-medium">Name</span>
          </label>
          <input
            id="brief-name"
            type="text"
            name="name"
            value={@form["name"]}
            phx-change="set_name"
            phx-debounce="300"
            class="input input-bordered w-full"
            placeholder="Weekly Competitor Brief"
          />
        </div>

        <div class="form-control">
          <label class="label" for="brief-question">
            <span class="label-text font-medium">Research Question</span>
          </label>
          <textarea
            id="brief-question"
            name="question"
            phx-change="set_question"
            phx-debounce="300"
            rows="3"
            class="textarea textarea-bordered w-full"
            placeholder="What are the latest product updates from our top competitors?"
          >{@form["question"]}</textarea>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div class="form-control">
            <label class="label" for="brief-category">
              <span class="label-text font-medium">Category</span>
            </label>
            <select
              id="brief-category"
              name="category"
              phx-change="set_category"
              class="select select-bordered w-full"
            >
              <option value="competitor" selected={@form["category"] == "competitor"}>
                Competitor
              </option>
              <option value="market" selected={@form["category"] == "market"}>Market</option>
              <option value="policy" selected={@form["category"] == "policy"}>Policy</option>
              <option value="trend" selected={@form["category"] == "trend"}>Trend</option>
              <option value="custom" selected={@form["category"] == "custom"}>Custom</option>
            </select>
          </div>

          <div class="form-control">
            <label class="label" for="brief-cadence">
              <span class="label-text font-medium">Cadence</span>
            </label>
            <select
              id="brief-cadence"
              name="cadence"
              phx-change="set_cadence"
              class="select select-bordered w-full"
            >
              <option value="manual" selected={@form["cadence"] == "manual"}>Manual</option>
              <option value="hourly" selected={@form["cadence"] == "hourly"}>Hourly</option>
              <option value="daily" selected={@form["cadence"] == "daily"}>Daily</option>
              <option value="weekly" selected={@form["cadence"] == "weekly"}>Weekly</option>
            </select>
          </div>

          <div class="form-control">
            <label class="label" for="brief-model">
              <span class="label-text font-medium">Model</span>
            </label>
            <select
              id="brief-model"
              name="model"
              phx-change="set_model"
              class="select select-bordered w-full"
            >
              <option value="claude-sonnet-4">Claude Sonnet 4</option>
              <option value="claude-opus-4-6">Claude Opus 4.6</option>
              <option value="gpt-4.1">GPT-4.1</option>
            </select>
          </div>
        </div>

        <div class="flex justify-end">
          <button id="save-brief-btn" phx-click="save_brief" class="btn btn-primary">
            Create Brief
          </button>
        </div>
      </div>
    </section>
    """
  end

  defp format_dt(nil), do: "—"
  defp format_dt(dt), do: Calendar.strftime(dt, "%Y-%m-%d %H:%M")
end
