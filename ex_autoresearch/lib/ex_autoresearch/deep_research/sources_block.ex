defmodule ExAutoresearch.DeepResearch.SourcesBlock do
  @moduledoc """
  Builds the `## Sources` markdown block appended to every completed research report.

  Each completed Investigation with a non-nil URL becomes a numbered entry:

      ## Sources

      1. [Title or URL](https://url) — fetched 2026-05-02 12:34:56Z, hash `abc123def456` via crawl4ai
      2. [Title](https://url2) — fetched 2026-05-02 12:35:00Z, hash `none` via unknown ⚠ via unknown, fallback used

  Returns `""` when there are no completed investigations with URLs.
  """

  require Ash.Query
  require Logger

  alias ExAutoresearch.Research

  @doc """
  Builds the sources markdown block for the given report.

  Queries all completed Investigations for `report.id` that have a non-nil URL,
  sorted by `inserted_at` ascending, and returns a markdown string starting with
  `\\n\\n## Sources\\n\\n` followed by a numbered list.

  Returns `""` if no qualifying investigations exist.
  """
  @spec build(Ash.Resource.record()) :: String.t()
  def build(report) do
    investigations = load_investigations(report.id)

    case investigations do
      [] ->
        ""

      invs ->
        lines =
          invs
          |> Enum.with_index(1)
          |> Enum.map(fn {inv, idx} -> format_line(idx, inv) end)

        "\n\n## Sources\n\n" <> Enum.join(lines, "\n")
    end
  end

  defp load_investigations(report_id) do
    Research.Investigation
    |> Ash.Query.filter(report_id == ^report_id and status == :completed and not is_nil(url))
    |> Ash.Query.sort(inserted_at: :asc)
    |> Ash.read!()
  rescue
    e ->
      Logger.warning("[SourcesBlock] Failed to read investigations: #{Exception.message(e)}")
      []
  end

  defp format_line(idx, inv) do
    label =
      (inv.query || inv.url || "Source #{idx}")
      |> String.slice(0, 80)

    url = inv.url

    timestamp =
      case inv.fetched_at do
        nil -> "unknown"
        dt -> Calendar.strftime(dt, "%Y-%m-%d %H:%M:%SZ")
      end

    hash_str =
      case inv.content_hash do
        nil -> "none"
        h -> String.slice(h, 0, 12)
      end

    source = inv.scraper_source || :unknown

    fallback_suffix =
      if inv.fallback_used do
        " ⚠ via #{source}, fallback used"
      else
        " via #{source}"
      end

    "#{idx}. [#{label}](#{url}) — fetched #{timestamp}, hash `#{hash_str}`#{fallback_suffix}"
  end
end
