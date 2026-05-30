defmodule ExAutoresearchWeb.BriefLiveTest do
  use ExAutoresearchWeb.ConnCase

  import Phoenix.LiveViewTest

  alias ExAutoresearch.Research.Brief

  setup %{conn: conn} do
    {conn, user, org} = register_and_log_in_user(conn)
    {:ok, conn: conn, user: user, org: org}
  end

  describe "GET /briefs" do
    test "mounts and shows empty state when no briefs exist", %{conn: conn} do
      {:ok, _view, html} = live(conn, ~p"/briefs")

      assert html =~ "Research Briefs"
      assert html =~ "No briefs yet"
    end

    test "shows existing briefs on mount", %{conn: conn, org: org} do
      brief =
        Ash.create!(
          Brief,
          %{
            name: "Test Competitor Brief",
            question: "What are competitors doing?",
            organization_id: org.id,
            category: :competitor,
            cadence: :daily
          },
          action: :create,
          tenant: org.id
        )

      {:ok, _view, html} = live(conn, ~p"/briefs")

      assert html =~ brief.name
      assert html =~ "competitor"
    end

    test "creates a new brief via the form and it appears in the list", %{conn: conn} do
      {:ok, view, _html} = live(conn, ~p"/briefs")

      view
      |> element("#toggle-form-btn")
      |> render_click()

      assert render(view) =~ "New Brief"

      render_hook(view, "set_name", %{"value" => "My Market Brief"})
      render_hook(view, "set_question", %{"value" => "What is happening in AI research?"})
      render_hook(view, "set_category", %{"value" => "market"})

      view
      |> element("#save-brief-btn")
      |> render_click()

      html = render(view)
      assert html =~ "My Market Brief"
    end

    test "enable/disable toggle changes the brief enabled state", %{conn: conn, org: org} do
      brief =
        Ash.create!(
          Brief,
          %{
            name: "Toggle Brief",
            question: "Does the toggle work?",
            organization_id: org.id,
            category: :custom,
            cadence: :weekly,
            enabled: false
          },
          action: :create,
          tenant: org.id
        )

      {:ok, view, _html} = live(conn, ~p"/briefs")

      view
      |> element("#toggle-#{brief.id}")
      |> render_click()

      html = render(view)
      updated = Ash.get!(Brief, brief.id, tenant: org.id)
      assert updated.enabled == true
      assert html =~ "Enabled"
    end
  end
end
