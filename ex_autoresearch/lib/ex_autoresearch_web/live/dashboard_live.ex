defmodule ExAutoresearchWeb.DashboardLive do
  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.{Research, DeepResearch}
  require Ash.Query

  @impl true
  def mount(_params, _session, socket) do
    if connected?(socket),
      do: Phoenix.PubSub.subscribe(ExAutoresearch.PubSub, "research:events")

    # If user is authenticated (via plug), filter by org; else show all
    org_id = socket.assigns[:current_org_id]

    reports_query =
      Research.Report
      |> Ash.Query.sort(inserted_at: :desc)

    reports =
      case org_id do
        nil -> Ash.read!(reports_query)
        id -> Ash.read!(reports_query, tenant: id)
      end
      |> Enum.map(&format_report_summary/1)

    socket =
      socket
      |> assign(:reports, reports)
      |> assign(:query, "")
      |> assign(:title, "")
      |> assign(:max_depth, 3)
      |> assign(:max_sources, 25)
      |> assign(:model, "claude-sonnet-4")
      |> assign(:active_report, nil)
      |> assign(:research_step, nil)
      |> assign(:progress, 0)
      |> assign(:status, :idle)
      |> assign(:selected_template_id, nil)

    {:ok, socket}
  end

  @impl true
  def handle_params(params, _uri, socket) do
    action = Map.get(params, "action")

    case action do
      "index" -> handle_index(params, socket)
      "logout" -> handle_logout(params, socket)
      _ -> {:noreply, socket}
    end
  end

  defp handle_logout(_params, socket) do
    {:noreply, push_navigate(socket, to: "/logout")}
  end

  defp handle_index(_params, socket) do
    {:noreply, socket}
  end

  @impl true
  def handle_event("set_query", %{"query" => q}, socket), do: {:noreply, assign(socket, :query, q)}
  def handle_event("set_title", %{"title" => t}, socket), do: {:noreply, assign(socket, :title, t)}
  def handle_event("set_model", %{"model" => m}, socket), do: {:noreply, assign(socket, :model, m)}
  def handle_event("set_depth", %{"depth" => d}, socket), do: {:noreply, assign(socket, :max_depth, String.to_integer(d))}
  def handle_event("set_sources", %{"sources" => s}, socket), do: {:noreply, assign(socket, :max_sources, String.to_integer(s))}

  def handle_event("start_research", _params, socket) do
    query = String.trim(socket.assigns.query)

    if query != "" do
      title =
        if socket.assigns.title != "",
          do: socket.assigns.title,
          else: String.slice(query, 0, 80)

      org_id = socket.assigns[:current_org_id]

      research_params = [
        query: query,
        title: title,
        model: socket.assigns.model,
        max_depth: socket.assigns.max_depth,
        max_sources: socket.assigns.max_sources
      ]

      research_params =
        if org_id, do: Keyword.put(research_params, :organization_id, org_id), else: research_params

      DeepResearch.ResearchOrchestrator.start_research(research_params)

      {:noreply,
       socket
       |> assign(:query, "")
       |> assign(:status, :researching)}
    else
      {:noreply, put_flash(socket, :error, "Please enter a research query")}
    end
  end

  def handle_event("view_report", %{"id" => id}, socket) do
    case DeepResearch.ResearchOrchestrator.report_detail(id) do
      {:ok, report} ->
        {:noreply, assign(socket, :active_report, report)}

      _ ->
        {:noreply, put_flash(socket, :error, "Report not found")}
    end
  end

  def handle_event("back_to_list", _params, socket), do: {:noreply, assign(socket, :active_report, nil)}

  def handle_event("export_report", %{"id" => id}, socket), do: export_report(id, socket)

  @impl true
  def handle_info({:status_changed, %{status: status}}, socket),
    do: {:noreply, assign(socket, :status, status)}

  def handle_info({:research_step, step_info}, socket) do
    step =
      case step_info[:step] do
        "planning" -> "Generating search plan..."
        "searching" -> "Searching: #{step_info[:count] || "?"} queries..."
        "analyzing" -> "Analyzing findings..."
        "deep_dive" -> "Deep diving: #{step_info[:count] || "?"} more queries..."
        "writing" -> "Writing report..."
        "completed" -> "Research complete!"
        _ -> "Processing..."
      end

    {:noreply,
     socket
     |> assign(:research_step, step)
     |> assign(:progress, (step_info[:progress] || 0) * 100)}
  end

  def handle_info({:investigation_started, info}, socket) do
    {:noreply, assign(socket, :research_step, "Searching: #{info[:query]}")}
  end

  def handle_info({:quality_alert, alert}, socket) do
    msg =
      case alert[:type] do
        :low_quality -> "Low quality results, pivoting strategy..."
        :diminishing_returns -> "Diminishing returns detected..."
        _ -> "Quality alert"
      end

    {:noreply,
     socket
     |> assign(:research_step, msg)
     |> put_flash(:info, msg)}
  end

  def handle_info({:report_completed, %{report_id: report_id}}, socket) do
    # Refresh reports list
    {:noreply,
     socket
     |> put_flash(:info, "A research report has completed!")
     |> push_navigate(to: ~p"/?report_id=#{report_id}")}
  end

  def handle_info(_, socket), do: {:noreply, socket}

  # --- Rendering ---

  @impl true
  def render(assigns) do
    ~H"""
    <div class="min-h-screen bg-gray-50">
      <.app_header current_user={@current_user} />

      <div class="max-w-5xl mx-auto px-4 py-6">
        <%= if @active_report do %>
          <.report_detail report={@active_report} />
        <% else %>
          <.search_form
            query={@query}
            title={@title}
            model={@model}
            max_depth={@max_depth}
            max_sources={@max_sources}
          />

          <%= if @status != :idle do %>
            <.progress_bar step={@research_step} progress={@progress} status={@status} />
          <% end %>

          <.report_list reports={@reports} />
        <% end %>
      </div>
    </div>
    """
  end

  defp app_header(assigns) do
    ~H"""
    <header class="bg-white border-b">
      <div class="max-w-5xl mx-auto px-4 py-3 flex items-center justify-between">
        <div class="flex items-center gap-4">
          <h1 class="text-lg font-semibold text-gray-900">
            CodeXpert
            <span class="text-xs font-normal text-gray-500 ml-1">Competitive Intelligence</span>
          </h1>
          <nav class="flex gap-3 text-sm">
            <.link navigate={~p"/"} class={nav_link_class(true)}>Dashboard</.link>
            <.link navigate={~p"/templates"} class={nav_link_class(false)}>Templates</.link>
            <.link navigate={~p"/schedules"} class={nav_link_class(false)}>Schedules</.link>
            <.link navigate={~p"/settings"} class={nav_link_class(false)}>Settings</.link>
          </nav>
        </div>

        <div class="flex items-center gap-3">
          <%= if @current_user do %>
            <span class="text-sm text-gray-600">{@current_user.email}</span>
            <form method="get" action="/logout" class="inline">
              <button
                type="submit"
                class="text-sm text-blue-600 hover:underline"
              >
                Logout
              </button>
            </form>
          <% else %>
            <.link navigate={~p"/login"} class="text-sm text-blue-600 hover:underline">
              Login
            </.link>
          <% end %>
        </div>
      </div>
    </header>
    """
  end

  defp nav_link_class(_active), do: "text-gray-600 hover:text-gray-900"

  defp search_form(assigns) do
    ~H"""
    <div class="bg-white rounded-lg shadow p-6 mb-6">
      <form phx-submit="start_research">
        <div class="mb-4">
          <label class="block text-sm font-medium text-gray-700 mb-1">Research Question</label>
          <input
            type="text"
            name="query"
            value={@query}
            phx-change="set_query"
            placeholder="e.g., What are the latest competitor moves in our market?"
            class="w-full px-3 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
          />
        </div>

        <div class="mb-4">
          <label class="block text-sm font-medium text-gray-700 mb-1">Report Title (optional)</label>
          <input
            type="text"
            name="title"
            value={@title}
            phx-change="set_title"
            placeholder="Auto-generated from query"
            class="w-full px-3 py-2 border rounded-lg text-sm"
          />
        </div>

        <div class="grid grid-cols-3 gap-4 mb-4">
          <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">LLM Model</label>
            <select name="model" phx-change="set_model" class="w-full px-3 py-2 border rounded-lg text-sm">
              <option value="claude-sonnet-4" selected={@model == "claude-sonnet-4"}>Claude Sonnet 4</option>
              <option value="claude-opus-4-6" selected={@model == "claude-opus-4-6"}>Claude Opus 4.6</option>
              <option value="gpt-4.1" selected={@model == "gpt-4.1"}>GPT-4.1</option>
              <option value="gemini-2.5-pro" selected={@model == "gemini-2.5-pro"}>Gemini 2.5 Pro</option>
            </select>
          </div>

          <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Max Depth</label>
            <input
              type="number"
              name="depth"
              value={@max_depth}
              phx-change="set_depth"
              min="1"
              max="10"
              class="w-full px-3 py-2 border rounded-lg text-sm"
            />
          </div>

          <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Max Sources</label>
            <input
              type="number"
              name="sources"
              value={@max_sources}
              phx-change="set_sources"
              min="5"
              max="100"
              class="w-full px-3 py-2 border rounded-lg text-sm"
            />
          </div>
        </div>

        <div class="flex justify-end">
          <button
            type="submit"
            class="px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
          >
            Start Deep Research
          </button>
        </div>
      </form>
    </div>
    """
  end

  defp progress_bar(assigns) do
    ~H"""
    <div class="bg-white rounded-lg shadow p-4 mb-6">
      <div class="flex items-center gap-3 mb-2">
        <div class="animate-spin h-4 w-4 border-2 border-blue-600 border-t-transparent rounded-full"></div>
        <span class="text-sm font-medium text-gray-700">{@step || "Researching..."}</span>
      </div>
      <div class="w-full bg-gray-200 rounded-full h-2">
        <div
          class="bg-blue-600 h-2 rounded-full transition-all"
          style={"width: #{@progress}%"}
        ></div>
      </div>
    </div>
    """
  end

  defp report_list(assigns) do
    ~H"""
    <div>
      <h2 class="text-lg font-semibold text-gray-900 mb-3">Recent Reports</h2>

      <%= if @reports == [] do %>
        <p class="text-gray-500 text-sm">No reports yet. Start a research query above.</p>
      <% else %>
        <div class="space-y-2">
          <%= for report <- @reports do %>
            <button
              phx-click="view_report"
              phx-value-id={report.id}
              class="w-full text-left bg-white rounded-lg shadow p-4 hover:shadow-md cursor-pointer"
            >
              <div class="flex items-center justify-between">
                <div>
                  <h3 class="font-medium text-gray-900">{report.title}</h3>
                  <p class="text-sm text-gray-500 mt-1">{String.slice(report.query, 0, 100)}</p>
                </div>
                <div class="text-right">
                  <span class={"inline-block px-2 py-1 rounded text-xs #{status_class(report.status)}"}>
                    {status_label(report.status)}
                  </span>
                  <p class="text-xs text-gray-400 mt-1">{report.source_count || 0} sources</p>
                </div>
              </div>
            </button>
          <% end %>
        </div>
      <% end %>
    </div>
    """
  end

  defp report_detail(assigns) do
    ~H"""
    <div class="bg-white rounded-lg shadow">
      <div class="border-b px-6 py-4 flex items-center justify-between">
        <div>
          <h2 class="text-lg font-semibold text-gray-900">{@report.title}</h2>
          <p class="text-sm text-gray-500 mt-1">{@report.query}</p>
        </div>
        <div class="flex items-center gap-3">
          <%= if @report.status == :completed do %>
            <button phx-click="export_report" phx-value-id={@report.id} class="text-sm text-blue-600 hover:underline">
              Export
            </button>
          <% end %>
          <button phx-click="back_to_list" class="text-sm text-blue-600 hover:underline">
            &larr; Back
          </button>
        </div>
      </div>

      <div class="px-6 py-4">
        <%= if @report.status == :completed and @report.markdown_body do %>
          <div class="prose max-w-none">
            {format_markdown(@report.markdown_body)}
          </div>
        <% else %>
          <div class="text-center py-8">
            <div class="animate-spin h-8 w-8 border-3 border-blue-600 border-t-transparent rounded-full mx-auto mb-3"></div>
            <p class="text-gray-600">
              {case @report.status do
                :pending -> "Waiting to start..."
                :researching -> "Researching... (#{Float.round(@report.progress_pct * 100, 1)}%)"
                :analyzing -> "Analyzing findings..."
                :writing -> "Writing report..."
                :failed -> "Research failed"
                _ -> "In progress..."
              end}
            </p>
          </div>
        <% end %>
      </div>
    </div>
    """
  end

  # --- Helpers ---

  defp export_report(id, socket) do
    report_dir = "priv/reports"
    File.mkdir_p(report_dir)
    report_path = Path.join(report_dir, "report_#{id}.md")

    case Ash.get(Research.Report, id) do
      {:ok, %{markdown_body: body}} when is_binary(body) ->
        File.write(report_path, body)
        {:noreply, put_flash(socket, :info, "Report exported to #{report_path}")}

      _ ->
        {:noreply, put_flash(socket, :error, "No markdown body available for export")}
    end
  end

  defp format_report_summary(report) do
    report
    |> Map.put(:source_count, report.total_sources)
    |> Map.take([:id, :title, :query, :status, :source_count, :inserted_at])
  end

  defp status_class(:completed), do: "bg-green-100 text-green-800"
  defp status_class(:researching), do: "bg-blue-100 text-blue-800"
  defp status_class(:failed), do: "bg-red-100 text-red-800"
  defp status_class(:analyzing), do: "bg-yellow-100 text-yellow-800"
  defp status_class(:writing), do: "bg-purple-100 text-purple-800"
  defp status_class(_), do: "bg-gray-100 text-gray-800"

  defp status_label(:completed), do: "Completed"
  defp status_label(:researching), do: "Researching"
  defp status_label(:failed), do: "Failed"
  defp status_label(:analyzing), do: "Analyzing"
  defp status_label(:writing), do: "Writing"
  defp status_label(_), do: "Pending"

  defp format_markdown(text) do
    {:safe, MDEx.to_html!(text, extension: [strikethrough: true, tagfilter: false], render: [hardbreaks: true, unsafe_: true])}
  end
end
