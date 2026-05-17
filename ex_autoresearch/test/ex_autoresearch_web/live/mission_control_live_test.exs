defmodule ExAutoresearchWeb.MissionControlLiveTest do
  use ExAutoresearchWeb.ConnCase

  import Phoenix.LiveViewTest

  setup %{conn: conn} do
    {conn, user, org} = register_and_log_in_user(conn)
    {:ok, conn: conn, user: user, org: org}
  end

  describe "GET /mission" do
    test "mounts and renders the 3D shell with JS-hook anchors", %{conn: conn} do
      {:ok, _view, html} = live(conn, ~p"/mission")

      # Anchors the MissionControl JS hook needs to find via document.querySelector.
      assert html =~ ~s(id="mission-canvas")
      assert html =~ ~s(phx-hook="MissionControl")
      assert html =~ ~s(phx-update="ignore")
      assert html =~ "data-mc-status"
      assert html =~ "data-mc-step"
      assert html =~ "data-mc-tokens"
      assert html =~ "data-mc-sat-count"
      assert html =~ "data-mc-alerts"

      # Status panel reflects the initial :idle assign.
      assert html =~ "IDLE"
      assert html =~ "Agent Core"
      assert html =~ "Token Reactor"
      assert html =~ "Investigation Swarm"
    end

    test "submitting an empty query surfaces a flash error", %{conn: conn} do
      {:ok, view, _html} = live(conn, ~p"/mission")

      html =
        view
        |> form("#mission-control-form", %{"query" => ""})
        |> render_submit()

      assert html =~ "Enter a research question first"
    end

    test "focus_satellite opens the investigation drawer, close_drawer hides it",
         %{conn: conn} do
      {:ok, view, _html} = live(conn, ~p"/mission")

      opened =
        render_hook(view, "intent:focus_satellite", %{
          "url" => "https://example.com/article",
          "outcome" => "primary_success"
        })

      assert opened =~ ~s(id="mc-drawer")
      assert opened =~ "https://example.com/article"
      assert opened =~ "primary_success"

      closed = render_hook(view, "close_drawer", %{})
      refute closed =~ ~s(id="mc-drawer")
    end

    test "PubSub :research_step updates the step label via step_label/2", %{conn: conn} do
      {:ok, view, _html} = live(conn, ~p"/mission")

      send(view.pid, {:research_step, %{step: "searching", count: 3, progress: 25}})
      html = render(view)

      assert html =~ "Searching: 3 queries"
    end
  end
end
