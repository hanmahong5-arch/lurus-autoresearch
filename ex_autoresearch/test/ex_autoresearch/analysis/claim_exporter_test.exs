defmodule ExAutoresearch.Analysis.ClaimExporterTest do
  use ExUnit.Case, async: true

  alias ExAutoresearch.Analysis.ClaimExporter

  @sample_rows [
    %{
      order_index: 0,
      text: "The sky is blue",
      grounding: :grounded,
      confidence: 0.95,
      origin_subquery: "sky color",
      source_url: "https://example.com/sky",
      claim_hash: "abc123"
    },
    %{
      order_index: 1,
      text: "Water is wet",
      grounding: :grounded,
      confidence: 0.99,
      origin_subquery: nil,
      source_url: nil,
      claim_hash: "def456"
    }
  ]

  describe "to_csv/1" do
    test "produces a header row as first line" do
      csv = ClaimExporter.to_csv(@sample_rows)
      [header | _] = String.split(csv, "\n")

      assert header ==
               "order_index,text,grounding,confidence,origin_subquery,source_url,claim_hash"
    end

    test "produces one data row per claim" do
      csv = ClaimExporter.to_csv(@sample_rows)
      lines = csv |> String.trim_trailing("\n") |> String.split("\n")
      # header + 2 data rows
      assert length(lines) == 3
    end

    test "first data row contains claim text and grounding" do
      csv = ClaimExporter.to_csv(@sample_rows)
      [_header, first_row | _] = String.split(csv, "\n")
      assert String.contains?(first_row, "The sky is blue")
      assert String.contains?(first_row, "grounded")
    end

    test "field containing a comma is wrapped in double-quotes" do
      rows = [
        %{
          order_index: 0,
          text: "claim, with comma",
          grounding: :grounded,
          confidence: 0.5,
          origin_subquery: nil,
          source_url: nil,
          claim_hash: nil
        }
      ]

      csv = ClaimExporter.to_csv(rows)
      assert String.contains?(csv, "\"claim, with comma\"")
    end

    test "field containing a double-quote is escaped by doubling" do
      rows = [
        %{
          order_index: 0,
          text: ~s(he said "hello"),
          grounding: :unsupported,
          confidence: 0.3,
          origin_subquery: nil,
          source_url: nil,
          claim_hash: nil
        }
      ]

      csv = ClaimExporter.to_csv(rows)
      # The field must be wrapped in quotes with the inner quote doubled
      assert String.contains?(csv, ~s("he said ""hello"""))
    end

    test "empty rows list produces only the header" do
      csv = ClaimExporter.to_csv([])
      lines = csv |> String.trim_trailing("\n") |> String.split("\n")
      assert length(lines) == 1
    end
  end

  describe "to_json/1" do
    test "round-trips via Jason.decode!" do
      json = ClaimExporter.to_json(@sample_rows)
      decoded = Jason.decode!(json)
      assert length(decoded) == 2
    end

    test "decoded entries contain expected keys as strings" do
      json = ClaimExporter.to_json(@sample_rows)
      [first | _] = Jason.decode!(json)
      assert Map.has_key?(first, "text")
      assert Map.has_key?(first, "grounding")
      assert Map.has_key?(first, "confidence")
      assert Map.has_key?(first, "order_index")
    end

    test "atom grounding value is encoded as string" do
      json = ClaimExporter.to_json(@sample_rows)
      [first | _] = Jason.decode!(json)
      assert first["grounding"] == "grounded"
    end

    test "nil values are preserved as JSON null" do
      json = ClaimExporter.to_json(@sample_rows)
      decoded = Jason.decode!(json)
      second = Enum.at(decoded, 1)
      assert is_nil(second["source_url"])
      assert is_nil(second["origin_subquery"])
    end

    test "empty list encodes as empty JSON array" do
      assert ClaimExporter.to_json([]) == "[]"
    end
  end
end
