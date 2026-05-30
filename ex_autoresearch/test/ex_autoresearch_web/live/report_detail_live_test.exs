defmodule ExAutoresearchWeb.ReportDetailLiveTest do
  @moduledoc """
  Tests for the ReportDetailLive grounding badge / Verified Claims section.
  """
  use ExAutoresearchWeb.ConnCase, async: false

  import Phoenix.LiveViewTest

  alias ExAutoresearch.Research.{Report, Claim}

  setup %{conn: conn} do
    {conn, _user, org} = register_and_log_in_user(conn)
    {:ok, conn: conn, org: org}
  end

  defp create_completed_report(org_id) do
    report =
      Ash.create!(
        Report,
        %{
          title: "Test Report",
          query: "test query for claims",
          organization_id: org_id
        },
        action: :start,
        tenant: org_id
      )

    Ash.update!(
      report,
      %{status: :completed, markdown_body: "# Test\n\nSome content."},
      action: :complete,
      tenant: org_id
    )
  end

  defp create_claim(report_id, attrs) do
    defaults = %{
      report_id: report_id,
      text: "A factual claim about something important",
      grounding: :grounded,
      confidence: 0.82,
      order_index: 0
    }

    Ash.create!(Claim, Map.merge(defaults, attrs), action: :create)
  end

  describe "Verified Claims section" do
    test "renders empty state when no claims exist", %{conn: conn, org: org} do
      report = create_completed_report(org.id)

      {:ok, _view, html} = live(conn, ~p"/reports/#{report.id}")

      assert html =~ "Verified Claims"
      assert html =~ "No verified claims yet"
    end

    test "renders claim with grounding badge and id attribute", %{conn: conn, org: org} do
      report = create_completed_report(org.id)
      claim = create_claim(report.id, %{grounding: :grounded, confidence: 0.82})

      {:ok, _view, html} = live(conn, ~p"/reports/#{report.id}")

      assert html =~ "Verified Claims"
      assert html =~ "id=\"claim-#{claim.id}\""
      # grounding badge text
      assert html =~ "Grounded"
      # confidence value
      assert html =~ "0.82"
      assert html =~ claim.text
    end

    test "renders contradicted badge for contradicted claim", %{conn: conn, org: org} do
      report = create_completed_report(org.id)

      _claim =
        create_claim(report.id, %{grounding: :contradicted, confidence: 0.6, order_index: 0})

      {:ok, _view, html} = live(conn, ~p"/reports/#{report.id}")

      assert html =~ "Contradicted"
    end

    test "renders multiple claims sorted by order_index", %{conn: conn, org: org} do
      report = create_completed_report(org.id)
      _c0 = create_claim(report.id, %{text: "First claim", order_index: 0})

      _c1 =
        create_claim(report.id, %{text: "Second claim", order_index: 1, grounding: :unsupported})

      {:ok, _view, html} = live(conn, ~p"/reports/#{report.id}")

      first_pos = :binary.match(html, "First claim") |> elem(0)
      second_pos = :binary.match(html, "Second claim") |> elem(0)
      assert first_pos < second_pos
    end
  end
end
