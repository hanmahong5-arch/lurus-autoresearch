defmodule ExAutoresearch.Notifications.Notifier do
  @moduledoc """
  Notification dispatcher — sends alerts when reports complete or changes are detected.

  Supports:
  - Email (via Swoosh)
  - Webhook (generic POST for enterprise chat integrations)
  """

  require Logger

  alias ExAutoresearch.Notifications.Webhook
  alias ExAutoresearch.Research.Report

  @doc """
  Called when a report reaches :completed status.
  """
  def report_completed(%Report{} = report) do
    Logger.info("[Notifier] Report completed: #{report.id}")

    send_email(report)
    send_webhook(report)
  end

  @doc """
  Called when a report fails (status: :failed).
  """
  def report_failed(%Report{} = report) do
    Logger.warning("[Notifier] Report failed: #{report.id}")
  end

  ## Email notifications

  defp send_email(report) do
    # In production, this would load organization members and send them emails.
    # For now, we rely on Swoosh local delivery in dev and SMTP adapter in prod.
    case Application.get_env(:ex_autoresearch, :notifications) do
      %{email_to: email} when is_binary(email) ->
        mail =
          Swoosh.Email.new(
            from: {"CodeXpert", "noreply@codexpert.local"},
            to: {email, email},
            subject: "Research Report Completed: #{report.title}",
            text_body: render_text_summary(report),
            html_body: render_html_summary(report)
          )

        case ExAutoresearch.Mailer.deliver(mail) do
          {:ok, _} -> :ok
          {:error, reason} -> Logger.error("[Notifier] Email failed: #{inspect(reason)}")
        end

      nil ->
        Logger.debug("[Notifier] No email recipients configured — skipping")

      _ ->
        Logger.debug("[Notifier] No email config — skipping")
    end
  end

  ## Webhook notifications

  defp send_webhook(report) do
    webhook_url = System.get_env("WEBHOOK_URL")

    if webhook_url && webhook_url != "" do
      payload = %{
        report_id: report.id,
        title: report.title,
        query: report.query,
        status: report.status,
        summary: report.summary,
        sources: report.total_sources
      }

      case Webhook.post(webhook_url, payload) do
        :ok -> Logger.info("[Notifier] Webhook sent for report #{report.id}")
        {:error, reason} -> Logger.error("[Notifier] Webhook failed: #{inspect(reason)}")
      end
    end
  end

  ## Renderers

  defp render_text_summary(report) do
    """
    Research Report: #{report.title}

    Query: #{report.query}
    Sources: #{report.total_sources}
    Summary: #{report.summary || "No summary available"}

    View the full report in your dashboard.
    """
  end

  defp render_html_summary(report) do
    """
    <h2>Research Report Completed</h2>
    <h3>#{report.title}</h3>
    <p>Query: #{report.query}</p>
    <p>Sources: #{report.total_sources}</p>
    <p>Summary: #{report.summary || "No summary available"}</p>
    <p>Open your dashboard to view the full report.</p>
    """
  end
end
