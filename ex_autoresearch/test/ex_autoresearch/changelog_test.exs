defmodule ExAutoresearch.ChangelogTest do
  use ExUnit.Case, async: true

  alias ExAutoresearch.Changelog

  describe "current_version/0" do
    test "matches mix.exs" do
      mix_version = Mix.Project.config()[:version]
      assert Changelog.current_version() == mix_version
    end

    test "matches the top entry of CHANGELOG.md" do
      assert hd(Changelog.entries()).version == Changelog.current_version()
    end
  end

  describe "entries/0" do
    test "returns at least one entry" do
      assert [_ | _] = Changelog.entries()
    end

    test "every entry has the expected shape" do
      for entry <- Changelog.entries() do
        assert is_binary(entry.version)
        assert entry.version =~ ~r/^\d+\.\d+\.\d+/
        assert entry.date =~ ~r/^\d{4}-\d{2}-\d{2}$/
        assert is_binary(entry.body_md) and entry.body_md != ""
        assert is_binary(entry.body_html) and entry.body_html != ""
        assert entry.anchor =~ ~r/^v[\d\-]+/
      end
    end

    test "entries are returned newest-first (sorted by date descending)" do
      dates = Enum.map(Changelog.entries(), & &1.date)
      assert dates == Enum.sort(dates, :desc)
    end

    test "body_html contains expected markdown rendering" do
      [latest | _] = Changelog.entries()
      # mdex turns `### Added` into <h3> and `- foo` into <ul><li>foo</li></ul>
      assert latest.body_html =~ "<h3>"
      assert latest.body_html =~ "<ul>" or latest.body_html =~ "<li>"
    end
  end

  describe "latest_entry/0" do
    test "is the head of entries/0" do
      assert Changelog.latest_entry() == hd(Changelog.entries())
    end
  end

  describe "build provenance" do
    test "build_commit/0 is a 7-char hex sha or 'unknown'" do
      commit = Changelog.build_commit()
      assert commit == "unknown" or commit =~ ~r/^[0-9a-f]{7,}$/
    end

    test "build_time/0 is a parseable ISO-8601 timestamp" do
      assert {:ok, _, _} = DateTime.from_iso8601(Changelog.build_time())
    end
  end
end
