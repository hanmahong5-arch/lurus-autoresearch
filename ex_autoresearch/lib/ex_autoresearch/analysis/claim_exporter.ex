defmodule ExAutoresearch.Analysis.ClaimExporter do
  @moduledoc """
  Pure, IO-free export of claim rows to CSV or JSON.

  Accepts a list of plain maps with keys:
    :order_index, :text, :grounding, :confidence,
    :origin_subquery, :source_url, :claim_hash
  """

  @columns [
    :order_index,
    :text,
    :grounding,
    :confidence,
    :origin_subquery,
    :source_url,
    :claim_hash
  ]

  @doc """
  Encode a list of claim maps as CSV (header + one row per claim).
  Fields containing commas, double-quotes, or newlines are wrapped in
  double-quotes, with internal double-quotes doubled.
  """
  @spec to_csv([map()]) :: String.t()
  def to_csv(rows) do
    header = @columns |> Enum.map(&to_string/1) |> encode_row()

    data_lines =
      Enum.map(rows, fn row ->
        @columns
        |> Enum.map(fn key -> row[key] end)
        |> Enum.map(&csv_value/1)
        |> encode_row()
      end)

    Enum.join([header | data_lines], "\n") <> "\n"
  end

  @doc """
  Encode a list of claim maps as a JSON array.
  Atom keys are converted to string keys via Jason.
  """
  @spec to_json([map()]) :: String.t()
  def to_json(rows) do
    string_keyed =
      Enum.map(rows, fn row ->
        Map.new(row, fn {k, v} -> {to_string(k), to_json_value(v)} end)
      end)

    Jason.encode!(string_keyed)
  end

  # --- private helpers ---

  defp csv_value(nil), do: ""
  defp csv_value(v) when is_atom(v), do: to_string(v)
  defp csv_value(v) when is_float(v), do: Float.to_string(v)
  defp csv_value(v) when is_integer(v), do: Integer.to_string(v)
  defp csv_value(v) when is_binary(v), do: v

  defp encode_row(fields) do
    fields
    |> Enum.map(&escape_field/1)
    |> Enum.join(",")
  end

  defp escape_field(value) do
    str = to_string(value)

    if String.contains?(str, [",", "\"", "\n", "\r"]) do
      "\"" <> String.replace(str, "\"", "\"\"") <> "\""
    else
      str
    end
  end

  defp to_json_value(nil), do: nil
  defp to_json_value(v) when is_atom(v), do: to_string(v)
  defp to_json_value(v), do: v
end
