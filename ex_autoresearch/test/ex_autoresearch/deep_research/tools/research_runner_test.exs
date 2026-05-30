defmodule ExAutoresearch.DeepResearch.Tools.ResearchRunnerTest do
  use ExUnit.Case, async: true

  alias ExAutoresearch.DeepResearch.Tools.ResearchRunner

  describe "filter_domains/2" do
    defp result(url), do: %{url: url, title: "title", snippet: "snip"}

    test "empty allow + empty block returns input unchanged" do
      results = [result("https://example.com/page"), result("https://other.org/page")]
      assert ResearchRunner.filter_domains(results, []) == results
    end

    test "allow-list keeps only results from allowed hosts" do
      results = [
        result("https://allowed.com/a"),
        result("https://blocked.com/b"),
        result("https://allowed.com/c")
      ]

      filtered = ResearchRunner.filter_domains(results, allow_domains: ["allowed.com"])
      urls = Enum.map(filtered, & &1.url)
      assert "https://allowed.com/a" in urls
      assert "https://allowed.com/c" in urls
      refute "https://blocked.com/b" in urls
    end

    test "block-list drops results from blocked hosts" do
      results = [
        result("https://keep.com/page"),
        result("https://drop.com/page")
      ]

      filtered = ResearchRunner.filter_domains(results, block_domains: ["drop.com"])
      assert length(filtered) == 1
      assert hd(filtered).url == "https://keep.com/page"
    end

    test "nil-host result is dropped when allow is non-empty" do
      bad = %{url: "not-a-url", title: "t", snippet: "s"}
      good = result("https://allowed.com/page")

      filtered =
        ResearchRunner.filter_domains([bad, good], allow_domains: ["allowed.com"])

      refute bad in filtered
      assert good in filtered
    end

    test "nil-host result is kept when allow is empty and not blocked" do
      bad = %{url: "not-a-url", title: "t", snippet: "s"}
      results = [bad]
      assert ResearchRunner.filter_domains(results, block_domains: []) == results
    end
  end

  describe "parse_relevance_scores/2" do
    test "parses a well-formed JSON array into aligned floats" do
      body =
        ~s([{"index": 1, "relevance": 0.9}, {"index": 2, "relevance": 0.3}, {"index": 3, "relevance": 0.6}])

      result = ResearchRunner.parse_relevance_scores(body, 3)

      assert is_list(result)
      assert length(result) == 3
      assert Enum.at(result, 0) == 0.9
      assert Enum.at(result, 1) == 0.3
      assert Enum.at(result, 2) == 0.6
    end

    test "returns list of length equal to count even when wrapped in prose" do
      body = """
      Here are the scores:
      [{"index": 1, "relevance": 0.75}, {"index": 2, "relevance": 0.55}]
      That's all.
      """

      result = ResearchRunner.parse_relevance_scores(body, 2)

      assert is_list(result)
      assert length(result) == 2
      assert Enum.at(result, 0) == 0.75
      assert Enum.at(result, 1) == 0.55
    end

    test "defaults missing index to 0.5" do
      # LLM only returns index 1 and 3 — index 2 must default
      body = ~s([{"index": 1, "relevance": 0.8}, {"index": 3, "relevance": 0.4}])
      result = ResearchRunner.parse_relevance_scores(body, 3)

      assert length(result) == 3
      assert Enum.at(result, 0) == 0.8
      assert Enum.at(result, 1) == 0.5
      assert Enum.at(result, 2) == 0.4
    end

    test "returns :error on completely invalid input" do
      assert ResearchRunner.parse_relevance_scores("not json at all", 3) == :error
    end

    test "returns :error on empty string" do
      assert ResearchRunner.parse_relevance_scores("", 2) == :error
    end

    test "returns :error on nil body" do
      assert ResearchRunner.parse_relevance_scores(nil, 2) == :error
    end

    test "returns empty list when count is 0" do
      result = ResearchRunner.parse_relevance_scores("[]", 0)
      assert result == []
    end

    test "clamps relevance values to [0.0, 1.0]" do
      body = ~s([{"index": 1, "relevance": 1.5}, {"index": 2, "relevance": -0.2}])
      result = ResearchRunner.parse_relevance_scores(body, 2)

      assert is_list(result)
      assert Enum.at(result, 0) == 1.0
      assert Enum.at(result, 1) == 0.0
    end
  end
end
