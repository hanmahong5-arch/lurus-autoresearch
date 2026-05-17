defmodule ExAutoresearchWeb.PagesSmokeTest do
  @moduledoc """
  Smoke tests covering authenticated LiveView pages that previously had
  no automated coverage. Verifies each page mounts without crashing and
  renders the shared `ExAutoresearchWeb.Badges` taxonomy where applicable
  (e.g. `<.category>` on /templates and /schedules, `<.org_plan>` on
  /settings).
  """
  use ExAutoresearchWeb.ConnCase

  import Phoenix.LiveViewTest

  setup %{conn: conn} do
    {conn, user, org} = register_and_log_in_user(conn)
    {:ok, conn: conn, user: user, org: org}
  end

  describe "smoke" do
    test "GET /templates mounts (empty state, no badge rendered yet)", %{conn: conn} do
      {:ok, _view, html} = live(conn, ~p"/templates")
      assert html =~ "Research Templates"
      assert html =~ "No templates yet"
    end

    test "GET /schedules mounts", %{conn: conn} do
      {:ok, _view, html} = live(conn, ~p"/schedules")
      # Empty schedule state — must mount cleanly even with zero templates.
      assert html =~ "Schedule" or html =~ "scheduled"
    end

    test "GET /settings mounts and surfaces the org plan area", %{conn: conn} do
      {:ok, _view, html} = live(conn, ~p"/settings")
      assert html =~ "Plan"
      # <.org_plan> renders a daisyUI badge ("Free" for seed users) or an
      # em-dash placeholder when plan is nil. Either way the marker is
      # present in the rendered HTML.
      assert html =~ "badge" or html =~ "—"
    end
  end
end
