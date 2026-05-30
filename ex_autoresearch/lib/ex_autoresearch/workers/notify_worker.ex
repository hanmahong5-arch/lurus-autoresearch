defmodule ExAutoresearch.Workers.NotifyWorker do
  @moduledoc """
  Oban worker that dispatches per-Brief notifications after a Delta is created.

  Reads `brief.notify_channels` and fans out to inbox (no-op), Slack, generic
  webhook, or email.  Each channel send is isolated — one failure does not abort
  the others or the job.
  """

  use Oban.Worker, queue: :default, max_attempts: 3

  require Logger

  alias ExAutoresearch.Notifications.Webhook
  alias ExAutoresearch.Research.{Brief, Delta}

  @impl Oban.Worker
  def perform(%Oban.Job{
        args: %{
          "delta_id" => delta_id,
          "brief_id" => brief_id,
          "organization_id" => org_id
        }
      }) do
    delta = Ash.get!(Delta, delta_id, tenant: org_id)
    brief = Ash.get!(Brief, brief_id)

    channels = brief.notify_channels || []

    Enum.each(channels, fn raw ->
      channel = parse_channel(raw)

      :telemetry.span(
        [:ex_autoresearch, :notify, :dispatch],
        %{channel: elem(channel, 0), brief_id: brief_id},
        fn ->
          result = dispatch_channel(channel, brief, delta)
          {result, %{channel: elem(channel, 0)}}
        end
      )
    end)

    :ok
  end

  @doc """
  Parses a notify_channels string into a tagged tuple.

  Recognised forms:
    - `"inbox"`              → `{:inbox}`
    - `"slack:<url>"`        → `{:slack, url}`
    - `"webhook:<url>"`      → `{:webhook, url}`
    - `"email:<addr>"`       → `{:email, addr}`
    - anything else          → `{:unknown, raw}`

  The split is on the FIRST `:` only so `https://` survives intact.
  """
  @spec parse_channel(String.t()) ::
          {:inbox}
          | {:slack, String.t()}
          | {:webhook, String.t()}
          | {:email, String.t()}
          | {:unknown, String.t()}
  def parse_channel("inbox"), do: {:inbox}

  def parse_channel(raw) when is_binary(raw) do
    case String.split(raw, ":", parts: 2) do
      ["slack", rest] -> {:slack, rest}
      ["webhook", rest] -> {:webhook, rest}
      ["email", rest] -> {:email, rest}
      _ -> {:unknown, raw}
    end
  end

  def parse_channel(raw), do: {:unknown, inspect(raw)}

  # --- Private dispatch ---

  defp dispatch_channel({:inbox}, _brief, _delta) do
    :ok
  end

  defp dispatch_channel({:slack, url}, brief, delta) do
    try do
      payload = %{text: slack_text(brief, delta)}

      case Webhook.post(url, payload) do
        :ok ->
          Logger.info("[NotifyWorker] Slack sent for brief #{brief.id}")
          :ok

        {:error, reason} ->
          Logger.warning("[NotifyWorker] Slack failed for brief #{brief.id}: #{inspect(reason)}")
          :ok
      end
    rescue
      e ->
        Logger.warning(
          "[NotifyWorker] Slack exception for brief #{brief.id}: #{Exception.message(e)}"
        )

        :ok
    end
  end

  defp dispatch_channel({:webhook, url}, brief, delta) do
    try do
      payload = delta_payload(brief, delta)

      case Webhook.post(url, payload) do
        :ok ->
          Logger.info("[NotifyWorker] Webhook sent for brief #{brief.id}")
          :ok

        {:error, reason} ->
          Logger.warning(
            "[NotifyWorker] Webhook failed for brief #{brief.id}: #{inspect(reason)}"
          )

          :ok
      end
    rescue
      e ->
        Logger.warning(
          "[NotifyWorker] Webhook exception for brief #{brief.id}: #{Exception.message(e)}"
        )

        :ok
    end
  end

  defp dispatch_channel({:email, addr}, brief, delta) do
    try do
      mail =
        Swoosh.Email.new(
          from: {"CodeXpert Research", "noreply@codexpert.local"},
          to: {addr, addr},
          subject: "Research Update: #{brief.name}",
          text_body: email_body(brief, delta)
        )

      case ExAutoresearch.Mailer.deliver(mail) do
        {:ok, _} ->
          Logger.info("[NotifyWorker] Email sent to #{addr} for brief #{brief.id}")
          :ok

        {:error, reason} ->
          Logger.warning(
            "[NotifyWorker] Email failed to #{addr} for brief #{brief.id}: #{inspect(reason)}"
          )

          :ok
      end
    rescue
      e ->
        Logger.warning(
          "[NotifyWorker] Email exception for #{addr}, brief #{brief.id}: #{Exception.message(e)}"
        )

        :ok
    end
  end

  defp dispatch_channel({:unknown, raw}, _brief, _delta) do
    Logger.warning("[NotifyWorker] Unknown channel format — skipping: #{inspect(raw)}")
    :ok
  end

  # --- Payload helpers ---

  @doc false
  def slack_text(brief, delta) do
    summary = String.slice(delta.markdown_digest || "", 0, 280)

    "[#{brief.name}] +#{delta.added_count} added, " <>
      "#{delta.changed_count} changed, " <>
      "#{delta.removed_count} removed, " <>
      "#{delta.contradicted_count} contradicted\n#{summary}"
  end

  @doc false
  def delta_payload(brief, delta) do
    %{
      brief_id: brief.id,
      brief_name: brief.name,
      delta_id: delta.id,
      added_count: delta.added_count,
      changed_count: delta.changed_count,
      removed_count: delta.removed_count,
      contradicted_count: delta.contradicted_count,
      digest: delta.markdown_digest
    }
  end

  defp email_body(brief, delta) do
    """
    Research Update: #{brief.name}

    Added: #{delta.added_count} | Changed: #{delta.changed_count} | \
    Removed: #{delta.removed_count} | Contradicted: #{delta.contradicted_count}

    #{delta.markdown_digest || "No digest available."}
    """
  end
end
