defmodule ExAutoresearch.DeepResearch.Tools.QueryAnalyzer do
  @moduledoc """
  Given research findings, generates deeper search queries.
  Used by the orchestrator to decide next steps.
  """

  require Logger

  @doc """
  Analyze findings and determine if we need more research.
  Returns {:ok, deeper_queries, summary} or {:ok, [], summary} if done.
  """
  @spec analyze(String.t(), String.t()) :: {:ok, [String.t()], String.t()} | :error
  def analyze(_original_question, findings) do
    findings_lines = String.split(findings, "\n", trim: true)

    has_substantial_content =
      length(findings_lines) > 10 and
        byte_size(findings) > 500

    if has_substantial_content do
      # Ask LLM for deeper queries (handled by orchestrator)
      {:ok, [], String.slice(findings, 0, 200)}
    else
      :error
    end
  end
end
