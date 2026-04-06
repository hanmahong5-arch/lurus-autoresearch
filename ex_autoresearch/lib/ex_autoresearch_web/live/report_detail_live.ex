defmodule ExAutoresearchWeb.ReportDetailLive do
  @moduledoc """
  LiveView showing a single research report with full markdown rendering.
  """

  use ExAutoresearchWeb, :live_view

  alias ExAutoresearch.Research
  require Ash.Query

  @impl true
  def mount(%{"id" => id}, _session, socket) do
    report = get_report(id)
    investigations = get_investigations(id)

    # Use mdex for proper markdown rendering
    html_body =
      if report && report.markdown_body do
        MDEx.to_html(report.markdown_body)
      else
        nil
      end

    {:ok,
     socket
     |> assign(:report, format_report(report))
     |> assign(:investigations, investigations)
     |> assign(:html_body, html_body)}
  end

  def mount(_params, _session, socket) do
    {:ok, push_navigate(socket, to: ~p"/")}
  end

  @impl true
  def handle_event("back", _params, socket) do
    {:noreply, push_navigate(socket, to: ~p"/")}
  end

  def handle_event("export", %{"id" => id}, socket) do
    case export_report(id) do
      {:ok, path} -> {:noreply, put_flash(socket, :info, "Exported to #{path}")}
      {:error, reason} -> {:noreply, put_flash(socket, :error, "Export failed: #{reason}")}
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
            <.link navigate={~p"/settings"} class="text-gray-600 hover:text-gray-900">Settings</.link>
          </nav>
          <span class="text-sm text-gray-600">{Map.get(assigns, :current_user, %{email: ""}).email}</span>
        </div>
      </header>

      <div class="max-w-5xl mx-auto px-4 py-6">
        <div class="flex items-center justify-between mb-6">
          <button phx-click="back" class="text-sm text-blue-600 hover:underline">
            &larr; Back to dashboard
          </button>
          <button
            phx-click="export"
            phx-value-id={@report && @report.id}
            class="px-3 py-1.5 bg-gray-200 text-gray-700 rounded text-sm hover:bg-gray-300"
          >
            Export Markdown
          </button>
        </div>

        <%= if @report do %>
          <div class="bg-white rounded-lg shadow overflow-hidden">
            <div class="border-b px-6 py-4">
              <h1 class="text-2xl font-bold text-gray-900">{@report.title}</h1>
              <p class="text-gray-500 mt-1">{@report.query}</p>
              <div class="flex gap-3 mt-3">
                <span class={"px-2 py-0.5 rounded text-xs #{status_class(@report.status)}"}>
                  {@report.status |> to_string() |> String.capitalize()}
                </span>
                <span class="px-2 py-0.5 rounded text-xs bg-gray-100 text-gray-600">
                  {@report.category}
                </span>
                <span class="text-xs text-gray-400">
                  {@report.total_sources} sources &middot; {@report.total_investigations} investigations
                </span>
              </div>
            </div>

            <div class="px-6 py-6">
              <%= if @html_body do %>
                <div class="prose max-w-none" inner_html={@html_body} />
              <% else %>
                <div class="text-center py-8 text-gray-500">
                  <%= if @report.status == :completed do %>
                    Report body not available.
                  <% else %>
                    {status_text(@report.status, @report.progress_pct)}
                  <% end %>
                </div>
              <% end %>
            </div>

            <%= if @investigations != [] do %>
              <div class="border-t px-6 py-4">
                <h3 class="font-medium text-gray-700 mb-3">Investigation Steps</h3>
                <div class="space-y-2">
                  <%= for inv <- @investigations do %>
                    <div class="flex items-start gap-3 text-sm">
                      <span class={"mt-1 h-2 w-2 rounded-full #{inv_status_class(inv.status)}"}></span>
                      <div>
                        <span class="font-medium">{inv.query}</span>
                        <span class="text-gray-500 ml-2">{inv.status}</span>
                      </div>
                    </div>
                  <% end %>
                </div>
              </div>
            <% end %>
          </div>
        <% else %>
          <div class="text-center py-8 text-gray-500">Report not found.</div>
        <% end %>
      </div>
    </div>
    """
  end

  defp get_report(id) do
    case Ash.get(Research.Report, id) do
      {:ok, report} -> report
      _ -> nil
    end
  end

  defp get_investigations(report_id) do
    Research.Investigation
    |> Ash.Query.filter(report_id == ^report_id)
    |> Ash.Query.sort(inserted_at: :asc)
    |> Ash.read!()
  end

  defp format_report(nil), do: nil

  defp format_report(report) do
    report
    |> Map.put(:summary_text, report.summary || "No summary available")
  end

  defp status_class(:completed), do: "bg-green-100 text-green-800"
  defp status_class(:researching), do: "bg-blue-100 text-blue-800"
  defp status_class(:failed), do: "bg-red-100 text-red-800"
  defp status_class(:analyzing), do: "bg-yellow-100 text-yellow-800"
  defp status_class(:writing), do: "bg-purple-100 text-purple-800"
  defp status_class(_), do: "bg-gray-100 text-gray-800"

  defp inv_status_class(:completed), do: "bg-green-500"
  defp inv_status_class(:failed), do: "bg-red-500"
  defp inv_status_class(:running), do: "bg-blue-500 animate-pulse"
  defp inv_status_class(_), do: "bg-gray-400"

  defp status_text(:researching, pct),
    do: "Researching... (#{Float.round(pct * 100, 1)}%)"

  defp status_text(:pending, _), do: "Waiting to start..."
  defp status_text(:analyzing, _), do: "Analyzing findings..."
  defp status_text(:writing, _), do: "Writing report..."
  defp status_text(:failed, _), do: "Research failed."
  defp status_text(_, _), do: "In progress..."

  defp export_report(id) do
    dir = "priv/reports"
    File.mkdir_p(dir)
    path = Path.join(dir, "report_#{id}.md")

    case Ash.get(Research.Report, id) do
      {:ok, %{markdown_body: body} = _report} when is_binary(body) ->
        File.write(path, body)
        {:ok, path}

      _ ->
        {:error, "No markdown body available"}
    end
  end
end
