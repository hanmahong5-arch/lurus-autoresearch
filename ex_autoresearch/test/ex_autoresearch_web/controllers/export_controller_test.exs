defmodule ExAutoresearchWeb.ExportControllerTest do
  use ExAutoresearchWeb.ConnCase, async: false

  alias ExAutoresearch.Research.{Report, Claim, Source}

  # --- helpers ---

  defp create_report(org_id) do
    report =
      Ash.create!(
        Report,
        %{title: "Export Test Report", query: "export test query", organization_id: org_id},
        action: :start,
        tenant: org_id
      )

    Ash.update!(
      report,
      %{status: :completed, markdown_body: "# Done"},
      action: :complete,
      tenant: org_id
    )
  end

  defp create_source(report_id, url) do
    Ash.create!(
      Source,
      %{report_id: report_id, url: url},
      action: :create
    )
  end

  defp create_claim(report_id, attrs) do
    defaults = %{
      report_id: report_id,
      text: "A claim for export testing",
      grounding: :grounded,
      confidence: 0.88,
      order_index: 0
    }

    Ash.create!(Claim, Map.merge(defaults, attrs), action: :create)
  end

  # --- tests ---

  describe "GET /reports/:id/claims/csv" do
    setup %{conn: conn} do
      {conn, _user, org} = register_and_log_in_user(conn)
      report = create_report(org.id)
      source = create_source(report.id, "https://example.com/article")
      _c1 = create_claim(report.id, %{text: "First claim", order_index: 0, source_id: source.id})
      _c2 = create_claim(report.id, %{text: "Second claim", order_index: 1})
      {:ok, conn: conn, report: report}
    end

    test "returns 200 with text/csv content-type", %{conn: conn, report: report} do
      response = get(conn, "/reports/#{report.id}/claims/csv")
      assert response.status == 200
      assert get_resp_header(response, "content-type") |> hd() =~ "text/csv"
    end

    test "body contains CSV header row", %{conn: conn, report: report} do
      response = get(conn, "/reports/#{report.id}/claims/csv")
      assert response.resp_body =~ "order_index,text,grounding"
    end

    test "body contains claim text", %{conn: conn, report: report} do
      response = get(conn, "/reports/#{report.id}/claims/csv")
      assert response.resp_body =~ "First claim"
      assert response.resp_body =~ "Second claim"
    end

    test "content-disposition header marks file as attachment", %{conn: conn, report: report} do
      response = get(conn, "/reports/#{report.id}/claims/csv")
      disposition = get_resp_header(response, "content-disposition") |> hd()
      assert disposition =~ "attachment"
      assert disposition =~ "claims-#{report.id}.csv"
    end
  end

  describe "GET /reports/:id/claims/json" do
    setup %{conn: conn} do
      {conn, _user, org} = register_and_log_in_user(conn)
      report = create_report(org.id)
      source = create_source(report.id, "https://example.com/article")
      _c1 = create_claim(report.id, %{text: "Alpha claim", order_index: 0, source_id: source.id})
      _c2 = create_claim(report.id, %{text: "Beta claim", order_index: 1})
      {:ok, conn: conn, report: report}
    end

    test "returns 200 with application/json content-type", %{conn: conn, report: report} do
      response = get(conn, "/reports/#{report.id}/claims/json")
      assert response.status == 200
      assert get_resp_header(response, "content-type") |> hd() =~ "application/json"
    end

    test "body is valid JSON with 2 entries", %{conn: conn, report: report} do
      response = get(conn, "/reports/#{report.id}/claims/json")
      decoded = Jason.decode!(response.resp_body)
      assert length(decoded) == 2
    end

    test "JSON entries contain expected keys", %{conn: conn, report: report} do
      response = get(conn, "/reports/#{report.id}/claims/json")
      [first | _] = Jason.decode!(response.resp_body)
      assert Map.has_key?(first, "text")
      assert Map.has_key?(first, "grounding")
      assert Map.has_key?(first, "confidence")
    end
  end

  describe "cross-tenant isolation" do
    test "report belonging to another org returns 404", %{conn: conn} do
      # First user owns the report
      {_conn1, _user1, org1} = register_and_log_in_user(build_conn())
      report = create_report(org1.id)

      # Second user is authenticated but belongs to a different org
      {conn2, _user2, _org2} = register_and_log_in_user(conn)

      response = get(conn2, "/reports/#{report.id}/claims/csv")
      assert response.status == 404
    end
  end
end
