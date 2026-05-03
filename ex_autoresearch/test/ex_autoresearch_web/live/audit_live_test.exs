defmodule ExAutoresearchWeb.AuditLiveTest do
  use ExAutoresearchWeb.ConnCase

  import Phoenix.LiveViewTest

  alias ExAutoresearch.Audit

  setup %{conn: conn} do
    {conn, user, org} = register_and_log_in_user(conn)
    {:ok, conn: conn, user: user, org: org}
  end

  describe "GET /audit" do
    test "renders empty state when no events exist", %{conn: conn} do
      {:ok, _view, html} = live(conn, ~p"/audit")

      assert html =~ "Audit Log"
      assert html =~ "No audit events yet"
      assert html =~ "Start a research query"
    end

    test "renders recorded events in the table", %{conn: conn, user: user, org: org} do
      {:ok, _} =
        Audit.record(:research_started, %{
          organization_id: org.id,
          user_id: user.id,
          target_type: :report,
          summary: "What is the latest ruling on data sovereignty?"
        })

      {:ok, view, _html} = live(conn, ~p"/audit")
      html = render(view)

      assert html =~ "Research Started"
      assert html =~ "data sovereignty"
      assert html =~ "audit-row-"
    end

    test "filter chip narrows visible rows", %{conn: conn, user: user, org: org} do
      {:ok, _} =
        Audit.record(:research_started, %{
          organization_id: org.id,
          user_id: user.id,
          summary: "alpha query"
        })

      {:ok, _} =
        Audit.record(:report_completed, %{
          organization_id: org.id,
          target_type: :report,
          summary: "beta complete"
        })

      {:ok, view, _html} = live(conn, ~p"/audit")

      view
      |> element("button[phx-value-event_type=research_started]")
      |> render_click()

      html = render(view)
      assert html =~ "alpha query"
      refute html =~ "beta complete"
    end

    test "PubSub broadcast appends new event live", %{conn: conn, org: org} do
      {:ok, view, _html} = live(conn, ~p"/audit")
      assert render(view) =~ "No audit events yet"

      {:ok, _} =
        Audit.record(:report_exported, %{
          organization_id: org.id,
          target_type: :report,
          target_id: "abc-123",
          summary: "exported test report"
        })

      html = render(view)
      assert html =~ "Report Exported"
      assert html =~ "exported test report"
    end

    test "tenant isolation: events from another org are not shown", %{conn: conn} do
      {_other_conn, _other_user, other_org} = register_and_log_in_user(build_conn())

      {:ok, _} =
        Audit.record(:research_started, %{
          organization_id: other_org.id,
          summary: "should not appear"
        })

      {:ok, view, _html} = live(conn, ~p"/audit")
      refute render(view) =~ "should not appear"
    end
  end
end
