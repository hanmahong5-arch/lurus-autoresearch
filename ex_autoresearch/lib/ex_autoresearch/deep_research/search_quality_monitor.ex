defmodule ExAutoresearch.DeepResearch.SearchQualityMonitor do
  @moduledoc """
  Monitors the quality of research investigations.

  Subscribes to investigation completion events. Tracks:
  - Average quality score across investigations
  - Diminishing returns (new sources yielding less relevant info)
  - Whether to stop digging or go deeper

  The quality monitor can signal the orchestrator to:
  - STOP current branch (quality too low, diminishing returns)
  - CONTINUE with more searches (quality high, new angles emerging)
  - SWITCH to a different query strategy
  """

  use GenServer

  require Logger

  defstruct [:report_id, scores: %{}, total_score: 0.0, count: 0]

  @quality_threshold 0.3
  @diminishing_threshold 0.15

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts)
  end

  @impl true
  def init(opts) do
    report_id = Keyword.fetch!(opts, :report_id)
    Phoenix.PubSub.subscribe(ExAutoresearch.PubSub, "research:events")
    {:ok, %__MODULE__{report_id: report_id}}
  end

  @impl true
  def handle_info({:investigation_completed, %{investigation_id: id, quality_score: score}}, state)
      when is_number(score) do
    state = %{state | scores: Map.put(state.scores, id, score)}
    state = %{state | total_score: state.total_score + score, count: state.count + 1}

    avg_score = state.total_score / state.count

    # Check for diminishing returns
    recent_scores = state.scores |> Map.values() |> Enum.take(-5)

    cond do
      length(recent_scores) >= 3 and avg_score < @quality_threshold ->
        Logger.info("[Monitor] Quality too low (avg: #{Float.round(avg_score, 2)}), signaling pivot")
        broadcast(:quality_alert, %{
          type: :low_quality,
          avg_score: avg_score,
          report_id: state.report_id
        })

      length(recent_scores) >= 3 ->
        recent_avg = Enum.sum(recent_scores) / length(recent_scores)
        if recent_avg < @diminishing_threshold do
          Logger.info("[Monitor] Diminishing returns detected (recent avg: #{Float.round(recent_avg, 2)})")
          broadcast(:quality_alert, %{
            type: :diminishing_returns,
            avg_score: avg_score,
            recent_avg: recent_avg,
            report_id: state.report_id
          })
        end

      true ->
        :ok
    end

    {:noreply, state}
  end

  def handle_info({:investigation_started, _}, state), do: {:noreply, state}
  def handle_info(_, state), do: {:noreply, state}

  @doc "Get current quality statistics."
  def stats(pid) do
    GenServer.call(pid, :stats)
  end

  @impl true
  def handle_call(:stats, _from, state) do
    avg = if state.count > 0, do: state.total_score / state.count, else: nil
    {:reply, %{avg_score: avg, count: state.count, scores: state.scores}, state}
  end

  defp broadcast(event, payload) do
    Phoenix.PubSub.broadcast(ExAutoresearch.PubSub, "research:events", {event, payload})
  rescue
    _ -> :ok
  end
end
