defmodule ExAutoresearch.Research.SourceTest do
  use ExAutoresearch.DataCase, async: false

  require Ash.Query

  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.Research.{Report, Source}

  defp setup_tenant(suffix) do
    email = "source_test_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org.id
  end

  defp create_report(tenant_id) do
    Ash.create!(
      Report,
      %{
        title: "Source Test Report #{System.unique_integer([:positive])}",
        query: "test query",
        organization_id: tenant_id
      },
      action: :start,
      tenant: tenant_id
    )
  end

  defp create_source(report, url, attrs \\ %{}) do
    Ash.create!(
      Source,
      Map.merge(%{report_id: report.id, url: url}, attrs),
      action: :create
    )
  end

  describe "Source upsert dedup by (report_id, url)" do
    test "two creates with same (report_id, url) yield ONE row" do
      tenant_id = setup_tenant("dedup")
      report = create_report(tenant_id)
      rid = report.id

      _first = create_source(report, "https://example.com/page", %{title: "First"})
      _second = create_source(report, "https://example.com/page", %{title: "Updated"})

      rows =
        Source
        |> Ash.Query.filter(report_id == ^rid)
        |> Ash.read!()

      assert length(rows) == 1
    end

    test "two distinct urls yield two rows" do
      tenant_id = setup_tenant("twourls")
      report = create_report(tenant_id)
      rid = report.id

      _s1 = create_source(report, "https://example.com/alpha")
      _s2 = create_source(report, "https://example.com/beta")

      rows =
        Source
        |> Ash.Query.filter(report_id == ^rid)
        |> Ash.read!()

      assert length(rows) == 2
    end

    test "scraper_source defaults to :unknown when not provided" do
      tenant_id = setup_tenant("default_scraper")
      report = create_report(tenant_id)

      source = create_source(report, "https://example.com/default")
      reloaded = Ash.get!(Source, source.id)

      assert reloaded.scraper_source == :unknown
    end
  end
end
