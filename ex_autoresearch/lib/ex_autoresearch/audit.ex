defmodule ExAutoresearch.Audit do
  @moduledoc """
  Audit domain — append-only compliance log scoped per organization.
  """

  use Ash.Domain, otp_app: :ex_autoresearch, backwards_compatible_interface?: false

  resources do
    resource ExAutoresearch.Audit.Event
  end

  @doc """
  Record an audit event. Returns `{:ok, event}` or `{:error, reason}`.

  `attrs` MUST include `:organization_id` (tenant). Optional: `:user_id`,
  `:target_type`, `:target_id`, `:summary`, `:metadata`. Failures are logged
  and swallowed at the call-site via `record!/2` if you want fire-and-forget.
  """
  def record(event_type, attrs) when is_atom(event_type) and is_map(attrs) do
    case Map.fetch(attrs, :organization_id) do
      {:ok, org_id} ->
        params = Map.put(attrs, :event_type, event_type)

        case Ash.create(__MODULE__.Event, params, action: :record, tenant: org_id) do
          {:ok, event} = ok ->
            broadcast(event)
            ok

          {:error, _} = err ->
            err
        end

      :error ->
        {:error, "organization_id is required"}
    end
  end

  defp broadcast(event) do
    Phoenix.PubSub.broadcast(
      ExAutoresearch.PubSub,
      "audit:events",
      {:audit_event_recorded, event}
    )
  rescue
    _ -> :ok
  end

  @doc """
  Fire-and-forget variant — logs and swallows errors. Returns `:ok`.
  """
  def record!(event_type, attrs) do
    case record(event_type, attrs) do
      {:ok, _} ->
        :ok

      {:error, reason} ->
        require Logger
        Logger.warning("[Audit] Failed to record #{event_type}: #{inspect(reason)}")
        :ok
    end
  end

  @doc """
  List events for an organization, newest-first. Optional :limit (default 100, max 500).
  """
  def list(organization_id, opts \\ []) do
    limit = opts |> Keyword.get(:limit, 100) |> min(500)
    require Ash.Query

    __MODULE__.Event
    |> Ash.Query.sort(inserted_at: :desc)
    |> Ash.Query.limit(limit)
    |> Ash.read(tenant: organization_id)
  end
end
