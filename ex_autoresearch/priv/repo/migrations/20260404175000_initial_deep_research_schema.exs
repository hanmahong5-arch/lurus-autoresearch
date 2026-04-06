defmodule ExAutoresearch.Repo.Migrations.InitialDeepResearchSchema do
  @moduledoc """
  Creates reports and investigations tables for deep research.
  """

  use Ecto.Migration

  def up do
    execute("CREATE TABLE IF NOT EXISTS reports (
      id TEXT NOT NULL PRIMARY KEY,
      title TEXT NOT NULL,
      query TEXT NOT NULL,
      status TEXT DEFAULT 'pending',
      model TEXT DEFAULT 'claude-sonnet-4',
      model_reasoning TEXT DEFAULT 'claude-sonnet-4',
      max_depth INTEGER DEFAULT 3,
      max_sources INTEGER DEFAULT 25,
      current_step TEXT,
      progress_pct REAL DEFAULT 0.0,
      total_sources INTEGER DEFAULT 0,
      total_investigations INTEGER DEFAULT 0,
      final_score REAL,
      markdown_body TEXT,
      summary TEXT,
      inserted_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    )")

    execute("CREATE UNIQUE INDEX IF NOT EXISTS reports_unique_title_index ON reports (title)")

    execute("CREATE TABLE IF NOT EXISTS investigations (
      id TEXT NOT NULL PRIMARY KEY,
      report_id TEXT NOT NULL REFERENCES reports(id),
      depth INTEGER DEFAULT 0,
      query TEXT,
      tool TEXT DEFAULT 'search',
      reasoning TEXT,
      status TEXT DEFAULT 'pending',
      findings TEXT,
      quality_score REAL,
      sources_count INTEGER DEFAULT 0,
      content_length INTEGER DEFAULT 0,
      url TEXT,
      error TEXT,
      inserted_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    )")
  end

  def down do
    execute("DROP TABLE IF EXISTS investigations")
    execute("DROP TABLE IF EXISTS reports")
  end
end
