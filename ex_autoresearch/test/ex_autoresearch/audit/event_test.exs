defmodule ExAutoresearch.Audit.EventTest do
  use ExAutoresearch.DataCase, async: false

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.Audit

  defp register_user(suffix) do
    email = "audit_event_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, user, org} = Auth.register(email, "password123")
    {user, org}
  end

  test "records an event scoped to the tenant" do
    {user, org} = register_user("scope")

    assert {:ok, event} =
             Audit.record(:research_started, %{
               organization_id: org.id,
               user_id: user.id,
               summary: "test"
             })

    assert event.event_type == :research_started
    assert event.summary == "test"

    assert {:ok, events} = Audit.list(org.id)
    assert length(events) == 1
  end

  test "tenant isolation: events written to org A are invisible to org B" do
    {_user_a, org_a} = register_user("iso_a")
    {_user_b, org_b} = register_user("iso_b")

    Audit.record!(:research_started, %{
      organization_id: org_a.id,
      summary: "org_a event"
    })

    assert {:ok, []} = Audit.list(org_b.id)
  end

  test "rejects unknown event_type" do
    {_user, org} = register_user("reject")

    assert {:error, _} =
             Audit.record(:bogus_event, %{organization_id: org.id})
  end

  test "Event resource exposes no update or destroy action" do
    action_names =
      Ash.Resource.Info.actions(ExAutoresearch.Audit.Event)
      |> Enum.map(& &1.name)
      |> Enum.sort()

    assert action_names == [:read, :record]
  end
end
