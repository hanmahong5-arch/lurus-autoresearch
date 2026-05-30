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

  describe "SourcesBlock.numbered_sources/1" do
    test "returns exactly the url'd completed investigations, 1-indexed, in inserted_at asc order" do
      tenant_id = setup_tenant("numbered_sources")
      report = create_report(tenant_id, "Numbered Sources Function Test")

      # inv_a and inv_b: completed with URLs
      inv_a = create_investigation(report, "Query A")

      Ash.update!(
        inv_a,
        %{
          status: :completed,
          findings: "findings A",
          url: "https://example.com/a",
          scraper_source: :native,
          fallback_used: false
        },
        action: :complete
      )

      inv_b = create_investigation(report, "Query B")

      Ash.update!(
        inv_b,
        %{
          status: :completed,
          findings: "findings B",
          url: "https://example.com/b",
          scraper_source: :native,
          fallback_used: false
        },
        action: :complete
      )

      # inv_c: completed without URL — must be excluded
      inv_c = create_investigation(report, "Query C no URL")

      Ash.update!(
        inv_c,
        %{
          status: :completed,
          findings: "findings C",
          scraper_source: :native,
          fallback_used: false
        },
        action: :complete
      )

      # inv_d: failed — must be excluded
      inv_d = create_investigation(report, "Query D failed")
      Ash.update!(inv_d, %{status: :failed, error: "boom"}, action: :fail)

      pairs = SourcesBlock.numbered_sources(report)

      assert length(pairs) == 2

      [{idx1, r1}, {idx2, r2}] = pairs
      assert idx1 == 1
      assert idx2 == 2

      # inserted_at asc means inv_a (created first) should be index 1
      assert r1.url == "https://example.com/a"
      assert r2.url == "https://example.com/b"
    end

    test "build/1 line count equals length(numbered_sources/1)" do
      tenant_id = setup_tenant("line_count")
      report = create_report(tenant_id, "Line Count Test")

      for i <- 1..3 do
        inv = create_investigation(report, "Query #{i}")

        Ash.update!(
          inv,
          %{
            status: :completed,
            findings: "findings #{i}",
            url: "https://example.com/#{i}",
            scraper_source: :native,
            fallback_used: false
          },
          action: :complete
        )
      end

      pairs = SourcesBlock.numbered_sources(report)
      build_result = SourcesBlock.build(report)

      # build/1 produces one line per source (no blank lines between entries)
      source_lines =
        build_result
        |> String.split("\n")
        |> Enum.filter(&String.match?(&1, ~r/^\d+\. \[/))

      assert length(source_lines) == length(pairs)
    end

    test "returns [] when no qualifying investigations exist" do
      tenant_id = setup_tenant("empty_numbered")
      report = create_report(tenant_id, "Empty Numbered Sources Test")

      assert SourcesBlock.numbered_sources(report) == []
    end
  end
end
