defmodule ExAutoresearchWeb.DashboardLiveScraperProgressTest do
  use ExAutoresearchWeb.ConnCase

  import Phoenix.LiveViewTest

  setup %{conn: conn} do
    {conn, user, org} = register_and_log_in_user(conn)
    {:ok, conn: conn, user: user, org: org}
  end

  @tag :capture_log
  test "scraper_progress with primary_success shows URL and impl name", %{conn: conn} do
    {:ok, view, _html} = live(conn, "/")

    Phoenix.PubSub.broadcast(
      ExAutoresearch.PubSub,
      "research:events",
      {:status_changed, %{status: :researching}}
    )

    Phoenix.PubSub.broadcast(
      ExAutoresearch.PubSub,
      "research:events",
      {:scraper_progress,
       %{
         report_id: "test-report-1",
         url: "https://example.com/article",
         outcome: :primary_success,
         primary_impl: ExAutoresearch.DeepResearch.Scraper.Crawl4ai,
         primary_error: nil,
         native_error: nil,
         duration_ms: 412,
         fallback_used?: false,
         message: "Fetched via Crawl4AI (412ms)"
       }}
    )

    html = render(view)
    assert html =~ "https://example.com/article"
    assert html =~ "Crawl4AI"
  end

  @tag :capture_log
  test "scraper_progress with fallback_success shows warning indicator", %{conn: conn} do
    {:ok, view, _html} = live(conn, "/")

    Phoenix.PubSub.broadcast(
      ExAutoresearch.PubSub,
      "research:events",
      {:status_changed, %{status: :researching}}
    )

    Phoenix.PubSub.broadcast(
      ExAutoresearch.PubSub,
      "research:events",
      {:scraper_progress,
       %{
         report_id: "test-report-2",
         url: "https://example.com/fallback",
         outcome: :fallback_success,
         primary_impl: ExAutoresearch.DeepResearch.Scraper.Crawl4ai,
         primary_error: {:http_error, 503},
         native_error: nil,
         duration_ms: 850,
         fallback_used?: true,
         message: "Crawl4AI failed (HTTP 503), fell back to native scraper"
       }}
    )

    html = render(view)
    assert html =~ "fell back to native scraper"
    assert html =~ "alert-warning"
    assert html =~ "⚠"
  end
end
