defmodule ExAutoresearch.DeepResearch.ResearchEngineTest do
  use ExAutoresearch.DataCase, async: false

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.DeepResearch.ResearchEngine
  alias ExAutoresearch.Research

  # Helper: create a throwaway tenant (organization) for tests that need one.
  defp setup_tenant do
    email = "engine_test_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org.id
  end

  defp create_report(tenant_id) do
    Ash.create!(
      Research.Report,
      %{
        title: "Test Report",
        query: "test query",
        model: "claude-sonnet-4",
        max_depth: 1,
        max_sources: 5,
        organization_id: tenant_id
      },
      action: :start,
      tenant: tenant_id
    )
  end

  describe "quality_alert?/1" do
    test "returns :low_quality when 3 scores average below 0.3" do
      scores = List.duplicate(0.1, 3)
      assert ResearchEngine.quality_alert?(scores) == :low_quality
    end

    test "returns :diminishing_returns or :low_quality for 3 high then 5 very low scores" do
      # Overall avg = (0.9*3 + 0.05*5)/8 = 0.37 >= 0.3 → should be :diminishing_returns
      # Last-5 avg = 0.05*5/5 = 0.05 < 0.15 → :diminishing_returns
      scores = [0.9, 0.9, 0.9, 0.05, 0.05, 0.05, 0.05, 0.05]
      result = ResearchEngine.quality_alert?(scores)
      assert result in [:diminishing_returns, :low_quality]
    end

    test "returns nil when only 2 scores provided" do
      assert ResearchEngine.quality_alert?([0.5, 0.5]) == nil
    end

    test "returns nil for healthy scores" do
      scores = [0.8, 0.7, 0.9, 0.6, 0.75]
      assert ResearchEngine.quality_alert?(scores) == nil
    end

    test "returns :diminishing_returns when overall avg >= 0.3 but last-5 avg < 0.15" do
      # Overall: (0.9*3 + 0.05*5) / 8 = 0.37 >= 0.3 → not :low_quality
      # Last 5: all 0.05 → avg = 0.05 < 0.15 → :diminishing_returns
      scores = [0.9, 0.9, 0.9, 0.05, 0.05, 0.05, 0.05, 0.05]
      assert ResearchEngine.quality_alert?(scores) == :diminishing_returns
    end
  end

  describe "continue?/1" do
    test "returns :halted for a completed report" do
      tenant_id = setup_tenant()
      report = create_report(tenant_id)
      Ash.update!(report, %{status: :completed}, action: :update_status)

      assert ResearchEngine.continue?(report) == :halted
    end

    test "returns :halted for a failed report" do
      tenant_id = setup_tenant()
      report = create_report(tenant_id)

      Ash.update!(report, %{status: :failed, summary: "test failure"}, action: :update_status)

      assert ResearchEngine.continue?(report) == :halted
    end

    test "returns {:ok, fresh_report} for an active (pending) report" do
      tenant_id = setup_tenant()
      report = create_report(tenant_id)

      # :pending is the initial status from :start action — not in [:completed, :failed]
      assert {:ok, fresh} = ResearchEngine.continue?(report)
      assert fresh.id == report.id
    end
  end

  # Full-pipeline and concurrency tests are deferred:
  # LLMClient and ResearchRunner do not expose an Application-env swappable seam
  # for mock injection in unit tests (they use runtime config for provider selection,
  # not a behaviour/adapter that can be substituted via config in the test env).
  # Adding such a seam would require code changes beyond the scope of this refactor.
end
