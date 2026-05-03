defmodule ExAutoresearch.DeepResearch.SourcesBlockTest do
  use ExAutoresearch.DataCase, async: false

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.DeepResearch.SourcesBlock
  alias ExAutoresearch.Research.{Report, Investigation}

  defp setup_tenant(suffix) do
    email = "sources_block_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org.id
  end

  defp create_report(tenant_id, title) do
    Ash.create!(
      Report,
      %{title: title, query: "test query for sources", organization_id: tenant_id},
      action: :start,
      tenant: tenant_id
    )
  end

  defp create_investigation(report, query) do
    Ash.create!(
      Investigation,
      %{report_id: report.id, tool: "search", query: query},
      action: :start
    )
  end

  describe "SourcesBlock.build/1" do
    test "renders a numbered markdown list with link, timestamp, and hash prefix" do
      tenant_id = setup_tenant("numbered")
      report = create_report(tenant_id, "Numbered Sources Test")

      inv1 = create_investigation(report, "First query")
      hash1 = "aabbccddeeff001122334455667788990011223344556677889900aabbccddee"

      Ash.update!(
        inv1,
        %{
          status: :completed,
          findings: "findings 1",
          url: "https://example.com/article1",
          fetched_at: ~U[2026-05-02 10:30:00.000000Z],
          content_hash: hash1,
          scraper_source: :crawl4ai,
          fallback_used: false
        },
        action: :complete
      )

      inv2 = create_investigation(report, "Second query")
      hash2 = "112233445566778899001122334455667788990011223344556677889900aabb"

      Ash.update!(
        inv2,
        %{
          status: :completed,
          findings: "findings 2",
          url: "https://example.com/article2",
          fetched_at: ~U[2026-05-02 11:00:00.000000Z],
          content_hash: hash2,
          scraper_source: :native,
          fallback_used: true
        },
        action: :complete
      )

      result = SourcesBlock.build(report)

      assert result =~ "## Sources"
      assert result =~ "[First query](https://example.com/article1)"
      assert result =~ "2026-05-02 10:30:00Z"
      assert result =~ "`aabbccddeeff`"
      assert result =~ "via crawl4ai"

      assert result =~ "[Second query](https://example.com/article2)"
      assert result =~ "⚠ via native, fallback used"
      assert result =~ "`112233445566`"

      assert result =~ "1. ["
      assert result =~ "2. ["
    end

    test "returns empty string when no completed investigations with URLs exist" do
      tenant_id = setup_tenant("empty")
      report = create_report(tenant_id, "Empty Sources Test")

      result = SourcesBlock.build(report)
      assert result == "" or String.trim(result) == ""
    end
  end
end
