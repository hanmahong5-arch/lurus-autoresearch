defmodule ExAutoresearch.Research.ClaimTest do
  use ExAutoresearch.DataCase, async: false

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.Research.{Report, Claim}

  defp setup_tenant(suffix) do
    email = "claim_test_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org.id
  end

  defp create_report(tenant_id) do
    Ash.create!(
      Report,
      %{
        title: "Claim Test Report #{System.unique_integer([:positive])}",
        query: "test query",
        organization_id: tenant_id
      },
      action: :start,
      tenant: tenant_id
    )
  end

  describe "Claim contradicting_source_ids" do
    test "create with contradicting_source_ids round-trips correctly" do
      tenant_id = setup_tenant("contra_ids")
      report = create_report(tenant_id)

      id1 = Ash.UUIDv7.generate()
      id2 = Ash.UUIDv7.generate()

      claim =
        Ash.create!(
          Claim,
          %{
            report_id: report.id,
            text: "Contradicted claim",
            grounding: :contradicted,
            contradicting_source_ids: [id1, id2]
          },
          action: :create
        )

      reloaded = Ash.get!(Claim, claim.id)
      assert reloaded.contradicting_source_ids == [id1, id2]
    end

    test "create without contradicting_source_ids reads back nil (back-compat)" do
      tenant_id = setup_tenant("contra_nil")
      report = create_report(tenant_id)

      claim =
        Ash.create!(
          Claim,
          %{report_id: report.id, text: "No contradiction data"},
          action: :create
        )

      reloaded = Ash.get!(Claim, claim.id)
      assert is_nil(reloaded.contradicting_source_ids)
    end
  end

  describe "Claim grounding" do
    test "grounding defaults to :unsupported when not provided" do
      tenant_id = setup_tenant("default_grounding")
      report = create_report(tenant_id)

      claim =
        Ash.create!(
          Claim,
          %{report_id: report.id, text: "Some claim text"},
          action: :create
        )

      reloaded = Ash.get!(Claim, claim.id)
      assert reloaded.grounding == :unsupported
    end

    test "create with grounding :grounded persists correctly" do
      tenant_id = setup_tenant("grounded")
      report = create_report(tenant_id)

      claim =
        Ash.create!(
          Claim,
          %{report_id: report.id, text: "A grounded claim", grounding: :grounded},
          action: :create
        )

      reloaded = Ash.get!(Claim, claim.id)
      assert reloaded.grounding == :grounded
    end

    test "all valid grounding values can be persisted" do
      tenant_id = setup_tenant("all_groundings")
      report = create_report(tenant_id)

      for grounding <- [:grounded, :contradicted, :unsupported, :complementary] do
        claim =
          Ash.create!(
            Claim,
            %{report_id: report.id, text: "Claim for #{grounding}", grounding: grounding},
            action: :create
          )

        reloaded = Ash.get!(Claim, claim.id)
        assert reloaded.grounding == grounding
      end
    end
  end
end
