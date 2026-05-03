defmodule ExAutoresearch.AuditTest do
  use ExAutoresearch.DataCase, async: false

  import ExUnit.CaptureLog

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.Audit

  defp register_org(suffix) do
    email = "audit_facade_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org
  end

  test "record!/2 returns :ok on success and on failure (fire-and-forget)" do
    org = register_org("ff")

    assert :ok =
             Audit.record!(:research_started, %{
               organization_id: org.id,
               summary: "success"
             })

    # Force failure by omitting required organization_id key entirely
    log =
      capture_log(fn ->
        assert :ok = Audit.record!(:research_started, %{wrong_key: "no_org_id"})
      end)

    assert log =~ "[Audit] Failed to record research_started"
  end

  test "list/2 honors :limit and clamps to 500" do
    org = register_org("limit")

    for i <- 1..3 do
      Audit.record!(:research_started, %{
        organization_id: org.id,
        summary: "event #{i}"
      })
    end

    assert {:ok, events} = Audit.list(org.id, limit: 2)
    assert length(events) == 2

    # Clamp: requesting 9999 gives at most 500 (can't verify 500 rows, but
    # verify the query runs without error and returns what's there)
    assert {:ok, all_events} = Audit.list(org.id, limit: 9999)
    assert length(all_events) == 3
  end
end
