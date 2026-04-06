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
     |> assign(:form, to_form(%{
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
     }))}
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
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("max_depth", String.to_integer(v)))}

  def handle_event("set_sources", %{"value" => v}, socket),
    do: {:noreply, assign(socket, form: socket.assigns.form |> Map.put("max_sources", String.to_integer(v)))}

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
    <div class="min-h-screen bg-gray-50">
      <.app_navbar />

      <div class="max-w-5xl mx-auto px-4 py-6">
        <div class="flex items-center justify-between mb-6">
          <h1 class="text-xl font-semibold text-gray-900">Research Templates</h1>
          <button
            phx-click="toggle_form"
            class="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 text-sm"
          >
            <span :if={!@show_form}>+ New Template</span>
            <span :if={@show_form}>Cancel</span>
          </button>
        </div>

        <%= if @show_form do %>
          <.template_form form={@form} />
        <% end %>

        <div class="space-y-3">
          <%= for t <- @templates do %>
            <div class="bg-white rounded-lg shadow p-4 flex items-center justify-between">
              <div>
                <h3 class="font-medium text-gray-900">{t.name}</h3>
                <p class="text-sm text-gray-500 mt-1">{String.slice(t.query_template, 0, 120)}</p>
                <div class="flex gap-2 mt-2">
                  <span class={"px-2 py-0.5 rounded text-xs #{category_class(t.category)}"}>
                    {t.category}
                  </span>
                  <span :if={t.enabled} class="px-2 py-0.5 rounded text-xs bg-green-100 text-green-800">
                    Scheduled
                  </span>
                  <span :if={t.schedule_cron} class="text-xs text-gray-400">
                    #{t.schedule_cron}
                  </span>
                </div>
              </div>
              <div class="flex gap-2">
                <button
                  phx-click="launch"
                  phx-value-id={t.id}
                  class="px-3 py-1.5 bg-blue-600 text-white rounded text-sm hover:bg-blue-700"
                >
                  Launch
                </button>
                <button
                  phx-click="delete_template"
                  phx-value-id={t.id}
                  class="px-3 py-1.5 bg-red-100 text-red-700 rounded text-sm hover:bg-red-200"
                  data-confirm="Delete this template?"
                >
                  Delete
                </button>
              </div>
            </div>
          <% end %>
        </div>
      </div>
    </div>
    """
  end

  defp app_navbar(assigns) do
    ~H"""
    <header class="bg-white border-b">
      <div class="max-w-5xl mx-auto px-4 py-3 flex items-center justify-between">
        <h1 class="text-lg font-semibold text-gray-900">
          CodeXpert
        </h1>
        <nav class="flex gap-3 text-sm">
          <.link navigate={~p"/"} class="text-gray-600 hover:text-gray-900">Dashboard</.link>
          <.link navigate={~p"/templates"} class="text-gray-900 font-medium">Templates</.link>
          <.link navigate={~p"/schedules"} class="text-gray-600 hover:text-gray-900">Schedules</.link>
          <.link navigate={~p"/settings"} class="text-gray-600 hover:text-gray-900">Settings</.link>
        </nav>
        <span class="text-sm text-gray-600">{Map.get(assigns, :current_user, %{email: ""}).email}</span>
      </div>
    </header>
    """
  end

  defp template_form(assigns) do
    ~H"""
    <div class="bg-white rounded-lg shadow p-6 mb-6">
      <h2 class="text-lg font-semibold text-gray-900 mb-4">New Template</h2>

      <div class="mb-4">
        <label class="block text-sm font-medium text-gray-700 mb-1">Name</label>
        <input
          type="text"
          name="name"
          value={@form["name"]}
          phx-change="set_name"
          class="w-full px-3 py-2 border rounded-lg"
          placeholder="Competitor Weekly Brief"
        />
      </div>

      <div class="mb-4">
        <label class="block text-sm font-medium text-gray-700 mb-1">Query Template</label>
        <textarea
          name="query_template"
          value={@form["query_template"]}
          phx-change="set_query"
          rows="3"
          class="w-full px-3 py-2 border rounded-lg"
          placeholder="e.g., What are the latest product updates from {company}?"
        />
      </div>

      <div class="mb-4">
        <label class="block text-sm font-medium text-gray-700 mb-1">Description (optional)</label>
        <input
          type="text"
          name="description"
          value={@form["description"]}
          phx-change="set_description"
          class="w-full px-3 py-2 border rounded-lg"
        />
      </div>

      <div class="grid grid-cols-3 gap-4 mb-4">
        <div>
          <label class="block text-sm font-medium text-gray-700 mb-1">Category</label>
          <select name="category" phx-change="set_category" class="w-full px-3 py-2 border rounded-lg text-sm">
            <option value="competitor" selected={@form["category"] == "competitor"}>Competitor</option>
            <option value="market" selected={@form["category"] == "market"}>Market</option>
            <option value="policy" selected={@form["category"] == "policy"}>Policy</option>
            <option value="trend" selected={@form["category"] == "trend"}>Trend</option>
            <option value="custom" selected={@form["category"] == "custom"}>Custom</option>
          </select>
        </div>

        <div>
          <label class="block text-sm font-medium text-gray-700 mb-1">Model</label>
          <select name="model" phx-change="set_model" class="w-full px-3 py-2 border rounded-lg text-sm">
            <option value="claude-sonnet-4">Claude Sonnet 4</option>
            <option value="claude-opus-4-6">Claude Opus 4.6</option>
            <option value="gpt-4.1">GPT-4.1</option>
          </select>
        </div>

        <div>
          <label class="block text-sm font-medium text-gray-700 mb-1">Schedule Cron (optional)</label>
          <input
            type="text"
            name="cron"
            value={@form["schedule_cron"]}
            phx-change="set_cron"
            class="w-full px-3 py-2 border rounded-lg text-sm"
            placeholder="0 9 * * 1"
          />
        </div>
      </div>

      <button
        phx-click="save_template"
        class="px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
      >
        Create Template
      </button>
    </div>
    """
  end

  defp category_class(:competitor), do: "bg-blue-100 text-blue-800"
  defp category_class(:market), do: "bg-green-100 text-green-800"
  defp category_class(:policy), do: "bg-yellow-100 text-yellow-800"
  defp category_class(:trend), do: "bg-purple-100 text-purple-800"
  defp category_class(_), do: "bg-gray-100 text-gray-800"
end
