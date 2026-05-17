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

    test ":plan_generated appends a Plan entry to the narrative panel", %{conn: conn} do
      {:ok, view, _html} = live(conn, ~p"/mission")

      send(
        view.pid,
        {:plan_generated,
         %{
           report_id: "rid-test",
           queries: ["what is foo", "how does bar work"]
         }}
      )

      html = render(view)

      assert html =~ ~s(id="narrative-panel")
      assert html =~ ~s(data-narrative-kind="plan")
      assert html =~ "what is foo"
      assert html =~ "how does bar work"
    end

    test ":investigation_started appends a Search sub-query entry", %{conn: conn} do
      {:ok, view, _html} = live(conn, ~p"/mission")

      send(
        view.pid,
        {:investigation_started,
         %{investigation_id: "i1", query: "compare jido vs phoenix presence", tool: :search}}
      )

      html = render(view)

      assert html =~ ~s(data-narrative-kind="subquery")
      assert html =~ "compare jido vs phoenix presence"
    end

    test ":learning_added appends a Learn entry with quality score", %{conn: conn} do
      {:ok, view, _html} = live(conn, ~p"/mission")

      send(
        view.pid,
        {:learning_added,
         %{
           report_id: "rid-test",
           investigation_id: "i1",
           query: "what is x",
           quality_score: 0.72,
           reasoning: "primary source",
           findings_excerpt: "X is defined as a synthetic monoid in category theory."
         }}
      )

      html = render(view)

      assert html =~ ~s(data-narrative-kind="learning")
      assert html =~ "X is defined as a synthetic monoid"
      assert html =~ "quality"
      assert html =~ "0.72"
    end
  end
end
