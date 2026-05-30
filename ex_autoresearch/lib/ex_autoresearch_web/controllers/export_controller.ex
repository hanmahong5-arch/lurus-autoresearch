defmodule ExAutoresearchWeb.ExportController do
  @moduledoc """
  Download claims for a report as CSV or JSON.

  Enforces org-level tenancy: a report owned by another org returns 404.
  """

  use ExAutoresearchWeb, :controller

  alias ExAutoresearch.Accounts
  alias ExAutoresearch.Analysis.ClaimExporter
  alias ExAutoresearch.Research
  alias ExAutoresearch.Research.{Claim, Source}

  require Ash.Query

  @doc """
  GET /reports/:id/claims/:format  (format = "csv" | "json")
  """
  def claims(conn, %{"id" => id, "format" => fmt}) when fmt in ["csv", "json"] do
    with {:ok, org_id} <- resolve_org(conn),
         {:ok, _report} <- load_report(id, org_id),
         {:ok, rows} <- load_rows(id) do
      send_export(conn, rows, id, fmt)
    else
      {:error, :no_org} ->
        conn |> redirect(to: "/") |> halt()

      {:error, :not_found} ->
        send_resp(conn, 404, "Not found")

      {:error, _reason} ->
        send_resp(conn, 404, "Not found")
    end
  end

  def claims(conn, _params) do
    send_resp(conn, 400, "Invalid format. Use csv or json.")
  end

  # --- private ---

  defp resolve_org(conn) do
    user = conn.assigns[:current_user]

    if is_nil(user) do
      {:error, :no_org}
    else
      orgs = Accounts.list_user_organizations(user.id)

      case orgs do
        [%{id: org_id} | _] -> {:ok, org_id}
        _ -> {:error, :no_org}
      end
    end
  end

  defp load_report(id, org_id) do
    case Ash.get(Research.Report, id, tenant: org_id) do
      {:ok, report} -> {:ok, report}
      _ -> {:error, :not_found}
    end
  end

  defp load_rows(report_id) do
    claims =
      Claim
      |> Ash.Query.filter(report_id == ^report_id)
      |> Ash.Query.sort(order_index: :asc)
      |> Ash.read!()

    sources =
      Source
      |> Ash.Query.filter(report_id == ^report_id)
      |> Ash.read!()

    url_map = Map.new(sources, fn s -> {s.id, s.url} end)

    rows =
      Enum.map(claims, fn claim ->
        %{
          order_index: claim.order_index,
          text: claim.text,
          grounding: claim.grounding,
          confidence: claim.confidence,
          origin_subquery: claim.origin_subquery,
          source_url: url_map[claim.source_id],
          claim_hash: claim.claim_hash
        }
      end)

    {:ok, rows}
  rescue
    e -> {:error, e}
  end

  defp send_export(conn, rows, id, "csv") do
    content = ClaimExporter.to_csv(rows)
    filename = "claims-#{id}.csv"

    conn
    |> put_resp_content_type("text/csv")
    |> put_resp_header("content-disposition", ~s(attachment; filename="#{filename}"))
    |> send_resp(200, content)
  end

  defp send_export(conn, rows, id, "json") do
    content = ClaimExporter.to_json(rows)
    filename = "claims-#{id}.json"

    conn
    |> put_resp_content_type("application/json")
    |> put_resp_header("content-disposition", ~s(attachment; filename="#{filename}"))
    |> send_resp(200, content)
  end
end
