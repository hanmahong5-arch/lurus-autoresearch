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

    html_body =
      if report && report.markdown_body do
        case MDEx.to_html(report.markdown_body,
               extension: [strikethrough: true, tagfilter: false],
               render: [hardbreaks: true, unsafe_: true]
             ) do
          {:ok, html} -> html
          {:error, _} -> nil
        end
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
    <Layouts.app
      flash={@flash}
      current_user={assigns[:current_user]}
      active_nav={:dashboard}
      container="max-w-5xl"
    >
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <button phx-click="back" class="btn btn-ghost btn-sm gap-1">
            <span aria-hidden="true">&larr;</span> Back to dashboard
          </button>
          <%= if @report do %>
            <button
              phx-click="export"
              phx-value-id={@report.id}
              class="btn btn-soft btn-sm"
            >
              Export Markdown
            </button>
          <% end %>
        </div>

        <%= if @report do %>
          <article class="card bg-base-100 border border-base-300 shadow-sm overflow-hidden">
            <header class="border-b border-base-300 px-6 py-5 space-y-3">
              <div>
                <h1 class="text-2xl font-bold tracking-tight">{@report.title}</h1>
                <p class="text-sm text-base-content/70 mt-1">{@report.query}</p>
              </div>
              <div class="flex flex-wrap gap-2 items-center text-xs">
                <.report_status status={@report.status} />
                <span class="badge badge-sm badge-ghost">
                  {@report.category}
                </span>
                <span class="text-base-content/50 ml-1">
                  {@report.total_sources} sources · {@report.total_investigations} investigations
                </span>
              </div>
            </header>

            <div class="px-6 py-6">
              <%= if @html_body do %>
                <div class="text-sm leading-relaxed max-w-none [&_h1]:text-2xl [&_h1]:font-bold [&_h1]:mt-6 [&_h1]:mb-3 [&_h2]:text-xl [&_h2]:font-semibold [&_h2]:mt-5 [&_h2]:mb-2 [&_h3]:text-base [&_h3]:font-semibold [&_h3]:mt-4 [&_h3]:mb-1 [&_p]:my-2 [&_ul]:list-disc [&_ul]:pl-6 [&_ul]:my-2 [&_ol]:list-decimal [&_ol]:pl-6 [&_ol]:my-2 [&_li]:my-1 [&_a]:link [&_a]:link-primary [&_code]:text-xs [&_code]:bg-base-200 [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_pre]:bg-base-200 [&_pre]:p-3 [&_pre]:rounded-md [&_pre]:overflow-x-auto [&_pre]:text-xs [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_blockquote]:border-l-4 [&_blockquote]:border-base-300 [&_blockquote]:pl-4 [&_blockquote]:italic [&_blockquote]:text-base-content/70 [&_strong]:font-semibold [&_table]:w-full [&_table]:my-3 [&_th]:text-left [&_th]:font-semibold [&_th]:border-b [&_th]:border-base-300 [&_th]:py-2 [&_th]:px-3 [&_td]:border-b [&_td]:border-base-300 [&_td]:py-2 [&_td]:px-3">
                  {Phoenix.HTML.raw(@html_body)}
                </div>
              <% else %>
                <div class="text-center py-12 text-base-content/60">
                  <%= if @report.status == :completed do %>
                    <p>Report body not available.</p>
                  <% else %>
                    <span class="loading loading-spinner loading-md text-primary mb-3"></span>
                    <p class="text-sm">{status_text(@report.status, @report.progress_pct)}</p>
                  <% end %>
                </div>
              <% end %>
            </div>

            <%= if @investigations != [] do %>
              <footer class="border-t border-base-300 px-6 py-4 bg-base-200/30">
                <h3 class="text-xs font-semibold uppercase tracking-wider text-base-content/60 mb-3">
                  Investigation Steps
                </h3>
                <ul class="space-y-2">
                  <%= for inv <- @investigations do %>
                    <li class="flex items-start gap-3 text-sm">
                      <.investigation_status status={inv.status} as={:dot} class="mt-1.5" />
                      <div class="min-w-0 flex-1">
                        <p class="font-medium truncate">{inv.query}</p>
                        <p class="text-xs text-base-content/60 mt-0.5">{inv.status}</p>
                      </div>
                    </li>
                  <% end %>
                </ul>
              </footer>
            <% end %>
          </article>
        <% else %>
          <div class="card bg-base-100 border border-dashed border-base-300">
            <div class="card-body items-center text-center py-12">
              <p class="text-base-content/60">Report not found.</p>
            </div>
          </div>
        <% end %>
      </div>
    </Layouts.app>
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

  defp status_text(:researching, pct),
    do: "Researching... (#{Float.round((pct || 0) * 100 / 1, 1)}%)"

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
