defmodule ExAutoresearch.Research.DeltaTest do
  use ExAutoresearch.DataCase, async: false

  require Ash.Query

  alias ExAutoresearch.Research.{Brief, Delta}
  alias ExAutoresearch.Accounts.Auth

  defp setup_tenant(suffix) do
    email = "delta_test_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org.id
  end

  defp create_brief(org_id) do
    Ash.create!(
      Brief,
      %{
        name: "Delta Test Brief #{System.unique_integer([:positive])}",
        question: "What is the state of AI research?",
        organization_id: org_id
      },
      action: :create,
      tenant: org_id
    )
  end

  defp create_delta(org_id, brief_id, attrs \\ %{}) do
    base = %{
      brief_id: brief_id,
      organization_id: org_id,
      to_report_id: Ash.UUID.generate(),
      generated_at: DateTime.utc_now()
    }

    Ash.create!(
      Delta,
      Map.merge(base, attrs),
      action: :create,
      tenant: org_id
    )
  end

  describe "Delta defaults" do
    test "added_count defaults to 0" do
      org_id = setup_tenant("delta_added_default")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)
      assert delta.added_count == 0
    end

    test "changed_count defaults to 0" do
      org_id = setup_tenant("delta_changed_default")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)
      assert delta.changed_count == 0
    end

    test "removed_count defaults to 0" do
      org_id = setup_tenant("delta_removed_default")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)
      assert delta.removed_count == 0
    end

    test "contradicted_count defaults to 0" do
      org_id = setup_tenant("delta_contradicted_default")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)
      assert delta.contradicted_count == 0
    end

    test "read_at defaults to nil" do
      org_id = setup_tenant("delta_read_at_default")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)
      assert is_nil(delta.read_at)
    end

    test "from_report_id is optional (nil)" do
      org_id = setup_tenant("delta_from_report_nil")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)
      assert is_nil(delta.from_report_id)
    end

    test "stores counts correctly when provided" do
      org_id = setup_tenant("delta_counts")
      brief = create_brief(org_id)

      delta =
        create_delta(org_id, brief.id, %{
          added_count: 5,
          changed_count: 3,
          removed_count: 2,
          contradicted_count: 1
        })

      assert delta.added_count == 5
      assert delta.changed_count == 3
      assert delta.removed_count == 2
      assert delta.contradicted_count == 1
    end
  end

  describe ":mark_read action" do
    test "sets read_at from nil to a datetime" do
      org_id = setup_tenant("delta_mark_read")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)

      assert is_nil(delta.read_at)

      now = DateTime.utc_now() |> DateTime.truncate(:microsecond)
      updated = Ash.update!(delta, %{read_at: now}, action: :mark_read, tenant: org_id)

      assert DateTime.compare(updated.read_at, now) == :eq
    end

    test "can re-mark read with a new timestamp" do
      org_id = setup_tenant("delta_remark_read")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)

      earlier =
        DateTime.add(DateTime.utc_now(), -3600, :second) |> DateTime.truncate(:microsecond)

      updated1 = Ash.update!(delta, %{read_at: earlier}, action: :mark_read, tenant: org_id)
      assert DateTime.compare(updated1.read_at, earlier) == :eq

      later = DateTime.utc_now() |> DateTime.truncate(:microsecond)
      updated2 = Ash.update!(updated1, %{read_at: later}, action: :mark_read, tenant: org_id)
      assert DateTime.compare(updated2.read_at, later) == :eq
    end
  end

  describe "tenant isolation" do
    test "delta is scoped to creating org" do
      org_id = setup_tenant("delta_tenant_isolation")
      brief = create_brief(org_id)
      delta = create_delta(org_id, brief.id)

      reloaded = Ash.get!(Delta, delta.id, tenant: org_id)
      assert reloaded.id == delta.id
    end
  end
end
