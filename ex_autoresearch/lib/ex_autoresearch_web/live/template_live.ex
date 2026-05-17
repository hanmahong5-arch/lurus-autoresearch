defmodule ExAutoresearchWeb.TemplateLive do
  @moduledoc """
  LiveViews for managing research templates.

  Actions:
  - List all templates for the current organization
  - Create / edit / delete templates
  - One-click launch from a template
  """

  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.{Research, DeepResearch}
  require Ash.Query

  @impl true
  def mount(_params, _session, socket) do
    org_id = socket.assigns[:current_org_id]

    query =
      Research.Template
      |> Ash.Query.sort(inserted_at: :desc)

    templates =
      case org_id do
        nil -> Ash.read!(query)
        id -> Ash.read!(query, tenant: id)
      end

    {:ok,
     socket
     |> assign(:templates, templates)
     |> assign(:editing_template, nil)
     |> assign(:show_form, false)
     |> assign(
       :form,
       to_form(%{
         "name" => "",
         "query_template" => "",
         "category" => "custom",
         "max_depth" => 3,
         "max_sources" => 25,
         "model" => "claude-sonnet-4",
         "schedule_cron" => "",
         "enabled" => false,
         "description" => "",
         "tags" => ""
       })
     )}
  end

  @impl true
  def handle_event("toggle_form", _params, socket) do
    {:noreply, assign(socket, :show_form, !socket.assigns.show_form)}
  end

  def handle_event("set_name", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("name", v))}

  def handle_event("set_query", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("query_template", v))}

  def handle_event("set_category", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("category", v))}

  def handle_event("set_description", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("description", v))}

  def handle_event("set_depth", %{"value" => v}, socket),
    do:
      {:noreply,
       assign(socket, form: socket.assigns.form |> Map.put("max_depth", String.to_integer(v)))}

  def handle_event("set_sources", %{"value" => v}, socket),
    do:
      {:noreply,
       assign(socket, form: socket.assigns.form |> Map.put("max_sources", String.to_integer(v)))}

  def handle_event("set_model", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("model", v))}

  def handle_event("set_cron", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("schedule_cron", v))}

  def handle_event("set_tags", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("tags", v))}

  def handle_event("save_template", _params, socket) do
    org_id = socket.assigns[:current_org_id]

    params = socket.assigns.form
    tags = String.split(params["tags"], ",", trim: true) |> Enum.map(&String.trim/1)

    template_params = %{
      name: params["name"],
      query_template: params["query_template"],
      category: String.to_atom(params["category"]),
      max_depth: params["max_depth"],
      max_sources: params["max_sources"],
      model: params["model"],
      schedule_cron: if(params["schedule_cron"] != "", do: params["schedule_cron"], else: nil),
      enabled: params["enabled"],
      description: if(params["description"] != "", do: params["description"], else: nil),
      tags: tags,
      organization_id: org_id
    }

    case Ash.create(Research.Template, template_params, action: :create) do
      {:ok, _template} ->
        {:noreply,
         socket
         |> put_flash(:info, "Template created")
         |> push_navigate(to: ~p"/templates")}

      {:error, reason} ->
        {:noreply, put_flash(socket, :error, "Failed: #{inspect(reason)}")}
    end
  end

  def handle_event("delete_template", %{"id" => id}, socket) do
    case Ash.destroy(Research.Template, id) do
      :ok -> {:noreply, put_flash(socket, :info, "Template deleted")}
      {:error, reason} -> {:noreply, put_flash(socket, :error, "Failed: #{inspect(reason)}")}
    end
  end

  def handle_event("launch", %{"id" => id}, socket) do
    case Ash.get(Research.Template, id) do
      {:ok, template} ->
        # Start a research session from the template
        DeepResearch.ResearchOrchestrator.start_research(
          query: template.query_template,
          title: template.name,
          model: template.model,
          max_depth: template.max_depth,
          max_sources: template.max_sources,
          organization_id: template.organization_id
        )

        {:noreply,
         socket
         |> put_flash(:info, "Research started from template: #{template.name}")
         |> push_navigate(to: ~p"/")}

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
      active_nav={:templates}
      container="max-w-5xl"
    >
      <div class="space-y-6">
        <div class="flex items-end justify-between gap-4">
          <div>
            <h1 class="text-2xl font-bold tracking-tight">Research Templates</h1>
            <p class="text-sm text-base-content/60 mt-1">
              Pre-configured research queries you can launch with one click or schedule on cron.
            </p>
          </div>
          <button
            phx-click="toggle_form"
            class={["btn btn-sm", if(@show_form, do: "btn-ghost", else: "btn-primary")]}
          >
            <span :if={!@show_form}>+ New template</span>
            <span :if={@show_form}>Cancel</span>
          </button>
        </div>

        <%= if @show_form do %>
          <.template_form form={@form} />
        <% end %>

        <%= if @templates == [] do %>
          <div class="card bg-base-100 border border-dashed border-base-300">
            <div class="card-body items-center text-center py-12">
              <p class="text-sm text-base-content/60">No templates yet.</p>
              <p class="text-xs text-base-content/50 mt-1">
                Create one to launch repeated research queries with one click.
              </p>
            </div>
          </div>
        <% else %>
          <ul class="space-y-3">
            <%= for t <- @templates do %>
              <li class="card bg-base-100 border border-base-300 shadow-sm">
                <div class="card-body py-4 px-5 flex flex-row items-start justify-between gap-4">
                  <div class="min-w-0 flex-1 space-y-2">
                    <h3 class="font-medium truncate">{t.name}</h3>
                    <p class="text-sm text-base-content/70 line-clamp-2">
                      {String.slice(t.query_template, 0, 200)}
                    </p>
                    <div class="flex flex-wrap gap-2 items-center text-xs">
                      <.category category={t.category} />
                      <span :if={t.enabled} class="badge badge-sm badge-success badge-soft">
                        Scheduled
                      </span>
                      <span :if={t.schedule_cron} class="font-mono text-base-content/50">
                        {t.schedule_cron}
                      </span>
                    </div>
                  </div>
                  <div class="flex gap-2 shrink-0">
                    <button
                      phx-click="launch"
                      phx-value-id={t.id}
                      class="btn btn-primary btn-sm"
                    >
                      Launch
                    </button>
                    <button
                      phx-click="delete_template"
                      phx-value-id={t.id}
                      class="btn btn-soft btn-error btn-sm"
                      data-confirm="Delete this template?"
                    >
                      Delete
                    </button>
                  </div>
                </div>
              </li>
            <% end %>
          </ul>
        <% end %>
      </div>
    </Layouts.app>
    """
  end

  defp template_form(assigns) do
    ~H"""
    <section class="card bg-base-100 border border-base-300 shadow-sm">
      <div class="card-body space-y-5">
        <h2 class="card-title text-lg">New Template</h2>

        <div class="form-control">
          <label class="label" for="tpl-name">
            <span class="label-text font-medium">Name</span>
          </label>
          <input
            id="tpl-name"
            type="text"
            name="name"
            value={@form["name"]}
            phx-change="set_name"
            phx-debounce="300"
            class="input input-bordered w-full"
            placeholder="Competitor Weekly Brief"
          />
        </div>

        <div class="form-control">
          <label class="label" for="tpl-query">
            <span class="label-text font-medium">Query Template</span>
            <span class="label-text-alt text-base-content/50">{} placeholders allowed</span>
          </label>
          <textarea
            id="tpl-query"
            name="query_template"
            phx-change="set_query"
            phx-debounce="300"
            rows="3"
            class="textarea textarea-bordered w-full font-mono text-sm"
            placeholder="e.g., What are the latest product updates from {company}?"
          >{@form["query_template"]}</textarea>
        </div>

        <div class="form-control">
          <label class="label" for="tpl-desc">
            <span class="label-text font-medium">Description</span>
            <span class="label-text-alt text-base-content/50">optional</span>
          </label>
          <input
            id="tpl-desc"
            type="text"
            name="description"
            value={@form["description"]}
            phx-change="set_description"
            phx-debounce="300"
            class="input input-bordered w-full"
          />
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
          <div class="form-control">
            <label class="label" for="tpl-category">
              <span class="label-text font-medium">Category</span>
            </label>
            <select
              id="tpl-category"
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
            <label class="label" for="tpl-model">
              <span class="label-text font-medium">Model</span>
            </label>
            <select
              id="tpl-model"
              name="model"
              phx-change="set_model"
              class="select select-bordered w-full"
            >
              <option value="claude-sonnet-4">Claude Sonnet 4</option>
              <option value="claude-opus-4-6">Claude Opus 4.6</option>
              <option value="gpt-4.1">GPT-4.1</option>
            </select>
          </div>

          <div class="form-control">
            <label class="label" for="tpl-cron">
              <span class="label-text font-medium">Schedule Cron</span>
              <span class="label-text-alt text-base-content/50">optional</span>
            </label>
            <input
              id="tpl-cron"
              type="text"
              name="cron"
              value={@form["schedule_cron"]}
              phx-change="set_cron"
              phx-debounce="300"
              class="input input-bordered input-sm w-full font-mono"
              placeholder="0 9 * * 1"
            />
          </div>
        </div>

        <div class="flex justify-end">
          <button phx-click="save_template" class="btn btn-primary">
            Create Template
          </button>
        </div>
      </div>
    </section>
    """
  end
end
