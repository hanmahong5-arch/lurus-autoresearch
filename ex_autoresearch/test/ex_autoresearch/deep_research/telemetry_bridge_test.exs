defmodule ExAutoresearch.DeepResearch.TelemetryBridgeTest do
  use ExUnit.Case, async: false

  alias ExAutoresearch.DeepResearch.TelemetryBridge

  @event [:ex_autoresearch, :scraper, :fetch, :stop]

  setup do
    TelemetryBridge.attach()
    Phoenix.PubSub.subscribe(ExAutoresearch.PubSub, "research:events")

    on_exit(fn ->
      :telemetry.detach("ex-autoresearch-scraper-bridge")
      Phoenix.PubSub.unsubscribe(ExAutoresearch.PubSub, "research:events")
    end)

    :ok
  end

  test "broadcasts :scraper_progress when telemetry stop fires with report_id" do
    # Use a duration large enough to produce >=1ms after conversion regardless of native unit
    one_ms_native = System.convert_time_unit(1, :millisecond, :native)

    :telemetry.execute(
      @event,
      %{duration: one_ms_native},
      %{
        url: "u",
        primary_impl: ExAutoresearch.DeepResearch.Scraper.Crawl4ai,
        outcome: :primary_success,
        report_id: "r1"
      }
    )

    assert_receive {:scraper_progress,
                    %{report_id: "r1", outcome: :primary_success, message: msg}}

    assert msg =~ "Crawl4AI"
    assert msg =~ "1ms"
  end

  test "skips broadcast when report_id is nil" do
    :telemetry.execute(
      @event,
      %{duration: 1_000_000},
      %{
        url: "u",
        primary_impl: ExAutoresearch.DeepResearch.Scraper.Crawl4ai,
        outcome: :primary_success
      }
    )

    refute_receive {:scraper_progress, _}, 100
  end

  test "fallback_success message mentions both primary and native" do
    :telemetry.execute(
      @event,
      %{duration: 5_000_000},
      %{
        url: "u",
        primary_impl: ExAutoresearch.DeepResearch.Scraper.Crawl4ai,
        outcome: :fallback_success,
        primary_error: {:crawl4ai_failed, {:http_error, 503}},
        report_id: "r2"
      }
    )

    assert_receive {:scraper_progress, %{message: msg}}
    assert msg =~ "Crawl4AI"
    assert msg =~ "503"
    assert msg =~ "native"
  end

  test "both_failed message indicates both failed" do
    :telemetry.execute(
      @event,
      %{duration: 2_000_000},
      %{
        url: "u",
        primary_impl: ExAutoresearch.DeepResearch.Scraper.Crawl4ai,
        outcome: :both_failed,
        primary_error: :timeout,
        native_error: :econnrefused,
        report_id: "r3"
      }
    )

    assert_receive {:scraper_progress, %{message: msg}}
    assert msg =~ "Both"
    assert msg =~ "failed"
  end

  test "broadcasts :research_progress with status :degraded on relevance fallback" do
    :telemetry.execute(
      [:ex_autoresearch, :relevance, :fallback],
      %{source_count: 2},
      %{
        report_id: "rpt-bridge-test",
        investigation_id: "inv-99",
        query: "test query",
        reason: :relevance_judge_unavailable
      }
    )

    assert_receive {:research_progress, "rpt-bridge-test",
                    %{stage: :analyze, status: :degraded, detail: detail}}

    assert detail =~ "byte-count"
  end

  test "skips broadcast on relevance fallback when report_id is nil" do
    :telemetry.execute(
      [:ex_autoresearch, :relevance, :fallback],
      %{source_count: 1},
      %{report_id: nil, query: "q", reason: :relevance_judge_unavailable}
    )

    refute_receive {:research_progress, _, _}, 100
  end
end
