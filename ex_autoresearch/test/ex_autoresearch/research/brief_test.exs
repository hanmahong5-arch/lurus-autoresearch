defmodule ExAutoresearch.Research.BriefTest do
  use ExAutoresearch.DataCase, async: false

  require Ash.Query

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.Research.{Brief, Report}
  alias ExAutoresearch.DeepResearch.ResearchEngine

  defp setup_tenant(suffix) do
    email = "brief_test_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org.id
  end

  defp create_brief(tenant_id, attrs \\ %{}) do
    base = %{
      name: "Test Brief #{System.unique_integer([:positive])}",
      question: "What is the market outlook for AI chips?",
      organization_id: tenant_id
    }

    Ash.create!(
      Brief,
      Map.merge(base, attrs),
      action: :create,
      tenant: tenant_id
    )
  end

  describe "Brief defaults" do
    test "cadence defaults to :manual" do
      tenant_id = setup_tenant("defaults")
      brief = create_brief(tenant_id)
      assert brief.cadence == :manual
    end

    test "enabled defaults to false" do
      tenant_id = setup_tenant("enabled_default")
      brief = create_brief(tenant_id)
      assert brief.enabled == false
    end

    test "category defaults to :custom" do
      tenant_id = setup_tenant("category_default")
      brief = create_brief(tenant_id)
      assert brief.category == :custom
    end

    test "model defaults to claude-sonnet-4" do
      tenant_id = setup_tenant("model_default")
      brief = create_brief(tenant_id)
      assert brief.model == "claude-sonnet-4"
    end

    test "max_depth defaults to 3" do
      tenant_id = setup_tenant("max_depth_default")
      brief = create_brief(tenant_id)
      assert brief.max_depth == 3
    end

    test "max_sources defaults to 25" do
      tenant_id = setup_tenant("max_sources_default")
      brief = create_brief(tenant_id)
      assert brief.max_sources == 25
    end

    test "allow_domains defaults to empty list" do
      tenant_id = setup_tenant("allow_domains_default")
      brief = create_brief(tenant_id)
      assert brief.allow_domains == []
    end
  end

  describe ":toggle action" do
    test "flips enabled from false to true" do
      tenant_id = setup_tenant("toggle_on")
      brief = create_brief(tenant_id, %{enabled: false})
      assert brief.enabled == false

      updated = Ash.update!(brief, %{enabled: true}, action: :toggle)
      assert updated.enabled == true
    end

    test "flips enabled from true to false" do
      tenant_id = setup_tenant("toggle_off")
      brief = create_brief(tenant_id, %{enabled: true})
      assert brief.enabled == true

      updated = Ash.update!(brief, %{enabled: false}, action: :toggle)
      assert updated.enabled == false
    end
  end

  describe ":mark_ran action" do
    test "sets last_run_at and next_run_at" do
      tenant_id = setup_tenant("mark_ran")
      brief = create_brief(tenant_id)

      now = DateTime.utc_now() |> DateTime.truncate(:microsecond)
      next = DateTime.add(now, 3600, :second) |> DateTime.truncate(:microsecond)

      updated = Ash.update!(brief, %{last_run_at: now, next_run_at: next}, action: :mark_ran)

      assert DateTime.compare(updated.last_run_at, now) == :eq
      assert DateTime.compare(updated.next_run_at, next) == :eq
    end
  end

  describe "Report brief_id + run_version" do
    test "report persists with brief_id and explicit run_version 2" do
      tenant_id = setup_tenant("report_brief_id")
      brief = create_brief(tenant_id)

      report =
        Ash.create!(
          Report,
          %{
            title: "Brief Report v2",
            query: "test query",
            organization_id: tenant_id,
            brief_id: brief.id,
            run_version: 2
          },
          action: :start,
          tenant: tenant_id
        )

      reloaded = Ash.get!(Report, report.id, tenant: tenant_id)
      assert reloaded.brief_id == brief.id
      assert reloaded.run_version == 2
    end

    test "report run_version defaults to 1 when omitted" do
      tenant_id = setup_tenant("run_version_default")

      report =
        Ash.create!(
          Report,
          %{
            title: "Default Version Report #{System.unique_integer([:positive])}",
            query: "test query",
            organization_id: tenant_id
          },
          action: :start,
          tenant: tenant_id
        )

      reloaded = Ash.get!(Report, report.id, tenant: tenant_id)
      assert reloaded.run_version == 1
    end
  end

  describe "ResearchEngine.next_run_version/1" do
    test "returns 1 when a brief has no reports" do
      tenant_id = setup_tenant("next_version_no_reports")
      brief = create_brief(tenant_id)

      assert ResearchEngine.next_run_version(brief) == 1
    end

    test "returns max(run_version) + 1 when reports exist" do
      tenant_id = setup_tenant("next_version_with_reports")
      brief = create_brief(tenant_id)

      Ash.create!(
        Report,
        %{
          title: "Brief Report v1",
          query: "test query",
          organization_id: tenant_id,
          brief_id: brief.id,
          run_version: 1
        },
        action: :start,
        tenant: tenant_id
      )

      Ash.create!(
        Report,
        %{
          title: "Brief Report v2",
          query: "test query",
          organization_id: tenant_id,
          brief_id: brief.id,
          run_version: 2
        },
        action: :start,
        tenant: tenant_id
      )

      assert ResearchEngine.next_run_version(brief) == 3
    end
  end
end
