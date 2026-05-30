defmodule ExAutoresearchWeb.InboxLiveTest do
  use ExAutoresearchWeb.ConnCase

  import Phoenix.LiveViewTest

  alias ExAutoresearch.Research.{Brief, Delta}

  setup %{conn: conn} do
    {conn, user, org} = register_and_log_in_user(conn)
    {:ok, conn: conn, user: user, org: org}
  end

  defp create_brief(org) do
    Ash.create!(
      Brief,
      %{
        name: "Inbox Test Brief #{System.unique_integer([:positive])}",
        question: "What is the current AI landscape?",
        organization_id: org.id
      },
      action: :create,
      tenant: org.id
    )
  end

  defp create_delta(org, brief, attrs \\ %{}) do
    base = %{
      brief_id: brief.id,
      organization_id: org.id,
      to_report_id: Ash.UUID.generate(),
      generated_at: DateTime.utc_now(),
      added_count: 2,
      changed_count: 1,
      removed_count: 0,
      markdown_digest: "## Summary\n\nSome important changes happened."
    }

    Ash.create!(
      Delta,
      Map.merge(base, attrs),
      action: :create,
      tenant: org.id
    )
  end

  describe "GET /inbox" do
    test "renders inbox page", %{conn: conn} do
      {:ok, _view, html} = live(conn, ~p"/inbox")

      assert html =~ "Inbox"
      assert html =~ "Research deltas"
    end

    test "shows delta row with unread highlight when delta has no read_at", %{
      conn: conn,
      org: org
    } do
      brief = create_brief(org)
      delta = create_delta(org, brief)

      {:ok, view, _html} = live(conn, ~p"/inbox")
      html = render(view)

      assert html =~ "delta-#{delta.id}"
      # Unread badge
      assert html =~ "New"
    end

    test "clicking a delta shows its digest and marks it read", %{conn: conn, org: org} do
      brief = create_brief(org)
      delta = create_delta(org, brief)

      {:ok, view, _html} = live(conn, ~p"/inbox")

      assert is_nil(delta.read_at)

      view
      |> element("#delta-#{delta.id}")
      |> render_click()

      html = render(view)

      # Digest should appear
      assert html =~ "Some important changes happened"

      # Row should now show as read (no "New" badge for this delta)
      # The delta detail panel should show with unique id
      assert html =~ "delta-detail-#{delta.id}"
      assert html =~ "Delta Summary"

      # Verify read_at was set in the DB
      reloaded = Ash.get!(Delta, delta.id, tenant: org.id)
      assert not is_nil(reloaded.read_at)
    end

    test "shows count badges on delta rows", %{conn: conn, org: org} do
      brief = create_brief(org)
      create_delta(org, brief, %{added_count: 3, changed_count: 2, removed_count: 1})

      {:ok, view, _html} = live(conn, ~p"/inbox")
      html = render(view)

      assert html =~ "added"
      assert html =~ "changed"
      assert html =~ "removed"
    end

    test "PubSub delta_created inserts new delta at top of stream", %{conn: conn, org: org} do
      brief = create_brief(org)
      {:ok, view, _html} = live(conn, ~p"/inbox")

      # Create a new delta and broadcast as the worker would
      new_delta = create_delta(org, brief, %{added_count: 5})

      Phoenix.PubSub.broadcast(
        ExAutoresearch.PubSub,
        "deltas:events",
        {:delta_created, %{delta_id: new_delta.id, brief_id: brief.id, organization_id: org.id}}
      )

      html = render(view)
      assert html =~ "delta-#{new_delta.id}"
    end

    test "tenant isolation: deltas from another org not shown", %{conn: conn} do
      {_other_conn, _other_user, other_org} = register_and_log_in_user(build_conn())
      other_brief = create_brief(other_org)
      other_delta = create_delta(other_org, other_brief)

      {:ok, view, _html} = live(conn, ~p"/inbox")

      refute render(view) =~ other_delta.id
    end
  end
end
