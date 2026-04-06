defmodule ExAutoresearch.Analysis.ReportExporter do
  @moduledoc """
  Export completed reports as Markdown files.

  Creates a timestamped report file with metadata header.
  """

  require Logger

  @export_dir "reports"

  @doc """
  Export a completed report to a Markdown file.
  Returns {:ok, path} | {:error, reason}
  """
  @spec export(map()) :: {:ok, String.t()} | {:error, term()}
  def export(report) do
    timestamp = DateTime.utc_now() |> DateTime.to_iso8601() |> String.replace(":", "-")
    safe_title = slugify(report.title)
    filename = "#{timestamp}_#{safe_title}.md"
    dir = Path.join(File.cwd!(), @export_dir)
    path = Path.join(dir, filename)

    with :ok <- File.mkdir_p(dir),
         content <- render_report(report),
         :ok <- File.write(path, content) do
      Logger.info("Report exported to: #{path}")
      {:ok, path}
    end
  end

  defp render_report(report) do
    """
    ---
    title: #{escape(report.title)}
    query: #{escape(report.query)}
    model: #{report.model || "unknown"}
    date: #{report.inserted_at || DateTime.utc_now()}
    sources: #{report.total_sources || 0}
    status: #{report.status}
    ---

    #{report.markdown_body || "No content generated."}
    """
  end

  defp slugify(text) do
    text
    |> String.downcase()
    |> String.replace(~r/[^a-z0-9\s-]/, "")
    |> String.replace(~r/\s+/, "_")
    |> String.slice(0, 50)
  end

  defp escape(text) do
    text
    |> String.replace("\\", "\\\\")
    |> String.replace("\"", "\\\"")
    |> String.replace("\n", " ")
  end
end
