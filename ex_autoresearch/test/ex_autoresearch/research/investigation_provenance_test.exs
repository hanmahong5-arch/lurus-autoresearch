defmodule ExAutoresearch.Research.InvestigationProvenanceTest do
  use ExAutoresearch.DataCase, async: false

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.Research.{Report, Investigation}

  defp setup_tenant(suffix) do
    email = "provenance_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org.id
  end

  defp create_report(tenant_id, title) do
    Ash.create!(
      Report,
      %{title: title, query: "test query", organization_id: tenant_id},
      action: :start,
      tenant: tenant_id
    )
  end

  defp create_investigation(report) do
    Ash.create!(
      Investigation,
      %{report_id: report.id, tool: "search", query: "test query"},
      action: :start
    )
  end

  describe "provenance fields on :complete action" do
    test "accepts and persists provenance fields on :complete" do
      tenant_id = setup_tenant("persist")
      report = create_report(tenant_id, "Provenance Persist Test")
      inv = create_investigation(report)

      fetched_at = ~U[2026-05-02 12:00:00.000000Z]

      updated =
        Ash.update!(
          inv,
          %{
            status: :completed,
            findings: "some findings",
            quality_score: 0.9,
            fetched_at: fetched_at,
            content_hash: "abc123def456789012345678901234567890123456789012345678901234abcd",
            scraper_source: :crawl4ai,
            fallback_used: true
          },
          action: :complete
        )

      reloaded = Ash.get!(Investigation, updated.id)

      assert reloaded.fetched_at == fetched_at

      assert reloaded.content_hash ==
               "abc123def456789012345678901234567890123456789012345678901234abcd"

      assert reloaded.scraper_source == :crawl4ai
      assert reloaded.fallback_used == true
    end

    test "scraper_source defaults to :unknown when not provided" do
      tenant_id = setup_tenant("default")
      report = create_report(tenant_id, "Default Source Test")
      inv = create_investigation(report)

      Ash.update!(
        inv,
        %{
          status: :completed,
          findings: "findings"
        },
        action: :complete
      )

      reloaded = Ash.get!(Investigation, inv.id)
      assert reloaded.scraper_source == :unknown
    end

    test "scraper_source rejects values outside the constraint" do
      tenant_id = setup_tenant("constraint")
      report = create_report(tenant_id, "Constraint Test")
      inv = create_investigation(report)

      result =
        Ash.update(
          inv,
          %{
            status: :completed,
            scraper_source: :firecrawl
          },
          action: :complete
        )

      assert {:error, _} = result
    end
  end
end
