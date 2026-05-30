defmodule ExAutoresearchWeb.InboxLive do
  @moduledoc """
  Inbox — shows Delta records for the current org, newest first.

  Unread deltas (read_at is nil) are visually highlighted.
  Clicking a delta renders its markdown_digest and marks it as read.

  Subscribes to "deltas:events" PubSub so new deltas appear live.
  """

  use ExAutoresearchWeb, :live_view

  require Ash.Query
  require Logger

  alias ExAutoresearch.Research.Delta
  alias ExAutoresearch.Research.Brief

  @impl true
  def mount(_params, _session, socket) do
    org_id = socket.assigns[:current_org_id]

    if connected?(socket) do
      Phoenix.PubSub.subscribe(ExAutoresearch.PubSub, "deltas:events")
    end

    deltas = load_deltas(org_id)

    {:ok,
     socket
     |> assign(:org_id, org_id)
     |> assign(:selected_delta, nil)
     |> assign(:selected_html, nil)
     |> stream(:deltas, deltas, dom_id: &"delta-#{&1.id}")}
  end

  @impl true
  def handle_event("select_delta", %{"id" => delta_id}, socket) do
    org_id = socket.assigns.org_id

    case Ash.get(Delta, delta_id, tenant: org_id) do
      {:ok, delta} ->
        delta =
          if is_nil(delta.read_at) do
            case Ash.update(delta, %{read_at: DateTime.utc_now()},
                   action: :mark_read,
                   tenant: org_id
                 ) do
              {:ok, updated} -> updated
              _ -> delta
            end
          else
            delta
          end

        html = render_markdown(delta.markdown_digest)

        {:noreply,
         socket
         |> assign(:selected_delta, delta)
         |> assign(:selected_html, html)
         |> stream_insert(:deltas, delta)}

      {:error, _} ->
        {:noreply, socket}
    end
  end

  @impl true
  def handle_info({:delta_created, %{organization_id: org_id, delta_id: delta_id}}, socket)
      when org_id == socket.assigns.org_id do
    case Ash.get(Delta, delta_id, tenant: org_id) do
      {:ok, delta} ->
        {:noreply, stream_insert(socket, :deltas, delta, at: 0)}

      _ ->
        {:noreply, socket}
    end
  end

  def handle_info({:delta_created, _}, socket), do: {:noreply, socket}
  def handle_info(_, socket), do: {:noreply, socket}

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app
      flash={@flash}
      current_user={@current_user}
      active_nav={:inbox}
      container="max-w-6xl"
    >
      <div class="space-y-6">
        <div class="flex items-start justify-between gap-4 flex-wrap">
          <div>
            <h1 class="text-2xl font-bold tracking-tight">Inbox</h1>
            <p class="text-sm text-base-content/60 mt-1">
              Research deltas — what changed between consecutive brief runs.
            </p>
          </div>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <%!-- Delta list --%>
          <div class="space-y-2">
            <div id="delta-list" phx-update="stream">
              <%= for {dom_id, delta} <- @streams.deltas do %>
                <div
                  id={dom_id}
                  phx-click="select_delta"
                  phx-value-id={delta.id}
                  class={[
                    "card bg-base-100 border shadow-sm cursor-pointer hover:shadow-md transition-shadow",
                    if(is_nil(delta.read_at),
                      do: "border-primary/50 bg-primary/5",
                      else: "border-base-300"
                    )
                  ]}
                >
                  <div class="card-body p-4 gap-2">
                    <div class="flex items-center justify-between gap-2">
                      <div class="flex items-center gap-2 min-w-0">
                        <%= if is_nil(delta.read_at) do %>
                          <span class="badge badge-primary badge-xs shrink-0">New</span>
                        <% end %>
                        <span class="font-medium text-sm truncate">
                          {brief_name(delta.brief_id)}
                        </span>
                      </div>
                      <span class="text-xs text-base-content/50 shrink-0 whitespace-nowrap">
                        {format_relative(delta.generated_at)}
                      </span>
                    </div>

                    <div class="flex flex-wrap gap-1.5">
                      <.count_badge label="added" count={delta.added_count} color="success" />
                      <.count_badge label="changed" count={delta.changed_count} color="warning" />
                      <.count_badge label="removed" count={delta.removed_count} color="error" />
                      <%= if delta.contradicted_count > 0 do %>
                        <.count_badge
                          label="contradicted"
                          count={delta.contradicted_count}
                          color="error"
                        />
                      <% end %>
                    </div>
                  </div>
                </div>
              <% end %>
            </div>

            <div id="inbox-empty-hint" />
          </div>

          <%!-- Delta detail panel --%>
          <div>
            <%= if @selected_delta do %>
              <div
                class="card bg-base-100 border border-base-300 shadow-sm"
                id={"delta-detail-#{@selected_delta.id}"}
              >
                <div class="card-body p-6 space-y-4">
                  <div class="flex items-center justify-between gap-2">
                    <h2 class="font-semibold text-base">Delta Summary</h2>
                    <span class="text-xs text-base-content/50">
                      {format_relative(@selected_delta.generated_at)}
                    </span>
                  </div>

                  <div class="flex flex-wrap gap-1.5">
                    <.count_badge label="added" count={@selected_delta.added_count} color="success" />
                    <.count_badge
                      label="changed"
                      count={@selected_delta.changed_count}
                      color="warning"
                    />
                    <.count_badge
                      label="removed"
                      count={@selected_delta.removed_count}
                      color="error"
                    />
                    <%= if @selected_delta.contradicted_count > 0 do %>
                      <.count_badge
                        label="contradicted"
                        count={@selected_delta.contradicted_count}
                        color="error"
                      />
                    <% end %>
                  </div>

                  <%= if @selected_html do %>
                    <div class="prose prose-sm max-w-none text-sm leading-relaxed [&_h1]:text-xl [&_h1]:font-bold [&_h1]:mt-4 [&_h1]:mb-2 [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:mt-3 [&_h2]:mb-1 [&_h3]:font-semibold [&_h3]:mt-3 [&_h3]:mb-1 [&_p]:my-2 [&_ul]:list-disc [&_ul]:pl-5 [&_ul]:my-2 [&_li]:my-0.5 [&_strong]:font-semibold">
                      {Phoenix.HTML.raw(@selected_html)}
                    </div>
                  <% else %>
                    <p class="text-sm text-base-content/50">No digest available.</p>
                  <% end %>
                </div>
              </div>
            <% else %>
              <div class="card bg-base-100 border border-dashed border-base-300">
                <div class="card-body items-center text-center py-16">
                  <.icon name="hero-inbox" class="size-12 text-base-content/30" />
                  <p class="text-sm text-base-content/60 mt-3">
                    Select a delta to view its digest.
                  </p>
                </div>
              </div>
            <% end %>
          </div>
        </div>
      </div>
    </Layouts.app>
    """
  end

  attr :label, :string, required: true
  attr :count, :integer, required: true
  attr :color, :string, default: "neutral"

  defp count_badge(assigns) do
    ~H"""
    <span class={["badge badge-xs gap-1", "badge-#{@color}", @count == 0 && "opacity-40"]}>
      {@count} {@label}
    </span>
    """
  end

  # --- Data helpers ---

  defp load_deltas(nil), do: []

  defp load_deltas(org_id) do
    Delta
    |> Ash.Query.sort(generated_at: :desc)
    |> Ash.read!(tenant: org_id)
  rescue
    _ -> []
  end

  defp brief_name(nil), do: "Unknown brief"

  defp brief_name(brief_id) do
    case Ash.get(Brief, brief_id) do
      {:ok, brief} -> brief.name
      _ -> String.slice(brief_id, 0, 8)
    end
  rescue
    _ -> String.slice(brief_id, 0, 8)
  end

  defp render_markdown(nil), do: nil
  defp render_markdown(""), do: nil

  defp render_markdown(markdown) do
    case MDEx.to_html(markdown,
           extension: [strikethrough: true, tagfilter: false],
           render: [hardbreaks: true, unsafe_: true]
         ) do
      {:ok, html} ->
        html

      _ ->
        safe = Phoenix.HTML.html_escape(markdown)
        "<pre>#{Phoenix.HTML.safe_to_string(safe)}</pre>"
    end
  rescue
    _ -> nil
  end

  defp format_relative(nil), do: "—"

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
end
