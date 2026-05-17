defmodule ExAutoresearchWeb.Badges do
  @moduledoc """
  Shared, color-coded badge function components for finite enum values.

  Centralizes the visual taxonomy used across multiple LiveViews:

    * `report_status/1` — Report status (6 values, daisyUI palette).
    * `investigation_status/1` — Investigation status (5 values), renderable
      as a small chip or as a single colored dot.
    * `audit_event/1` — Audit event_type (8 values, semantic palette).
    * `audit_target/1` — Audit target_type (5 values).

  All renderers fall back to `badge-ghost` for unknown atoms rather than
  crash, mirroring the orchestrator's "errors are values" stance.
  """
  use Phoenix.Component

  # ── Report.status ─────────────────────────────────────────────────────

  @doc """
  Renders a colored daisyUI badge for `Report.status`.

  Known values: `:pending`, `:researching`, `:analyzing`, `:writing`,
  `:completed`, `:failed`.
  """
  attr :status, :atom, required: true
  attr :class, :string, default: nil

  def report_status(assigns) do
    ~H"""
    <span class={["badge badge-sm", report_status_class(@status), @class]}>
      {report_status_label(@status)}
    </span>
    """
  end

  defp report_status_class(:completed), do: "badge-success"
  defp report_status_class(:researching), do: "badge-info"
  defp report_status_class(:analyzing), do: "badge-warning"
  defp report_status_class(:writing), do: "badge-secondary"
  defp report_status_class(:failed), do: "badge-error"
  defp report_status_class(:pending), do: "badge-ghost"
  defp report_status_class(_), do: "badge-ghost"

  defp report_status_label(:pending), do: "Pending"
  defp report_status_label(:researching), do: "Researching"
  defp report_status_label(:analyzing), do: "Analyzing"
  defp report_status_label(:writing), do: "Writing"
  defp report_status_label(:completed), do: "Completed"
  defp report_status_label(:failed), do: "Failed"
  defp report_status_label(other), do: other |> to_string() |> String.capitalize()

  # ── Investigation.status ──────────────────────────────────────────────

  @doc """
  Renders an `Investigation.status` indicator.

  Set `as={:badge}` (default) for a chip with text; `as={:dot}` for a
  bare colored dot suitable next to a label.

  Known values: `:pending`, `:running`, `:completed`, `:failed`, `:abandoned`.
  """
  attr :status, :atom, required: true
  attr :as, :atom, default: :badge, values: [:badge, :dot]
  attr :class, :string, default: nil

  def investigation_status(assigns) do
    ~H"""
    <%= case @as do %>
      <% :dot -> %>
        <span
          class={["h-2 w-2 rounded-full shrink-0", inv_dot_class(@status), @class]}
          aria-hidden="true"
        />
      <% _ -> %>
        <span class={["badge badge-sm", inv_badge_class(@status), @class]}>
          {inv_label(@status)}
        </span>
    <% end %>
    """
  end

  defp inv_dot_class(:completed), do: "bg-success"
  defp inv_dot_class(:failed), do: "bg-error"
  defp inv_dot_class(:running), do: "bg-info animate-pulse"
  defp inv_dot_class(:pending), do: "bg-base-content/30"
  defp inv_dot_class(:abandoned), do: "bg-warning"
  defp inv_dot_class(_), do: "bg-base-content/30"

  defp inv_badge_class(:completed), do: "badge-success"
  defp inv_badge_class(:running), do: "badge-info"
  defp inv_badge_class(:failed), do: "badge-error"
  defp inv_badge_class(:pending), do: "badge-ghost"
  defp inv_badge_class(:abandoned), do: "badge-warning badge-outline"
  defp inv_badge_class(_), do: "badge-ghost"

  defp inv_label(:completed), do: "Completed"
  defp inv_label(:running), do: "Running"
  defp inv_label(:failed), do: "Failed"
  defp inv_label(:pending), do: "Pending"
  defp inv_label(:abandoned), do: "Abandoned"
  defp inv_label(other), do: other |> to_string() |> String.capitalize()

  # ── Audit.event_type ──────────────────────────────────────────────────

  @doc """
  Renders an Audit event_type badge.

  Known values: `:research_started`, `:report_completed`, `:report_failed`,
  `:report_exported`, `:template_created`, `:template_updated`,
  `:template_deleted`, `:user_logged_in`. `:all` is recognized as a filter
  selector and rendered as ghost.
  """
  attr :event, :atom, required: true
  attr :class, :string, default: nil

  def audit_event(assigns) do
    ~H"""
    <span class={["badge badge-sm", audit_event_class(@event), @class]}>
      {humanize_event(@event)}
    </span>
    """
  end

  defp audit_event_class(:research_started), do: "badge-info"
  defp audit_event_class(:report_completed), do: "badge-success"
  defp audit_event_class(:report_failed), do: "badge-error"
  defp audit_event_class(:report_exported), do: "badge-primary"
  defp audit_event_class(:template_created), do: "badge-accent"
  defp audit_event_class(:template_updated), do: "badge-accent badge-outline"
  defp audit_event_class(:template_deleted), do: "badge-warning"
  defp audit_event_class(:user_logged_in), do: "badge-ghost"
  defp audit_event_class(_), do: "badge-ghost"

  defp humanize_event(:all), do: "All"

  defp humanize_event(et) when is_atom(et) do
    et
    |> Atom.to_string()
    |> String.replace("_", " ")
    |> String.split()
    |> Enum.map_join(" ", &String.capitalize/1)
  end

  defp humanize_event(other), do: to_string(other)

  # ── Audit.target_type ─────────────────────────────────────────────────

  @doc """
  Renders an Audit target_type badge.

  Known values: `:report`, `:template`, `:investigation`, `:user`,
  `:system`. New in this module — previously rendered as plain gray text.
  """
  attr :target, :atom, required: true
  attr :class, :string, default: nil

  def audit_target(assigns) do
    ~H"""
    <span class={[
      "badge badge-sm badge-outline",
      audit_target_class(@target),
      @class
    ]}>
      {audit_target_label(@target)}
    </span>
    """
  end

  defp audit_target_class(:report), do: "badge-primary"
  defp audit_target_class(:template), do: "badge-accent"
  defp audit_target_class(:investigation), do: "badge-info"
  defp audit_target_class(:user), do: "badge-ghost"
  defp audit_target_class(:system), do: "badge-secondary"
  defp audit_target_class(_), do: "badge-ghost"

  defp audit_target_label(target) when is_atom(target),
    do: target |> Atom.to_string() |> String.capitalize()

  defp audit_target_label(other), do: to_string(other)
end
