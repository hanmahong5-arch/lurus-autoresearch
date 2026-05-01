defmodule ExAutoresearch.DeepResearch.Scraper.Crawl4aiTest do
  use ExUnit.Case, async: true

  alias ExAutoresearch.DeepResearch.Scraper.Crawl4ai

  describe "fetch/2 — sync endpoint" do
    test "happy path: returns markdown from crawl_sync response" do
      stub_name = __MODULE__.SyncHappyStub

      Req.Test.stub(stub_name, fn conn ->
        Req.Test.json(conn, %{
          "results" => [
            %{"success" => true, "markdown" => "# Hello\n\nThis is the page content."}
          ]
        })
      end)

      assert {:ok, result} =
               Crawl4ai.fetch("http://example.com", plug: {Req.Test, stub_name})

      assert result.markdown =~ "Hello"
      assert result.metadata.source == :crawl4ai
      assert result.metadata.url == "http://example.com"
      assert %DateTime{} = result.metadata.fetched_at
    end

    test "returns error when crawl_sync reports success: false" do
      stub_name = __MODULE__.SyncFailStub

      Req.Test.stub(stub_name, fn conn ->
        Req.Test.json(conn, %{
          "results" => [%{"success" => false}]
        })
      end)

      assert {:error, {:crawl4ai_failed, :page_fetch_failed}} =
               Crawl4ai.fetch("http://example.com", plug: {Req.Test, stub_name})
    end

    test "5xx error returns crawl4ai_failed tuple" do
      stub_name = __MODULE__.SyncServerErrorStub

      Req.Test.stub(stub_name, fn conn ->
        Plug.Conn.send_resp(conn, 503, "service unavailable")
      end)

      assert {:error, {:crawl4ai_failed, {:http_error, 503}}} =
               Crawl4ai.fetch("http://example.com", plug: {Req.Test, stub_name})
    end

    test "transport error returns crawl4ai_failed tuple" do
      stub_name = __MODULE__.SyncTransportErrorStub

      Req.Test.stub(stub_name, fn conn ->
        Req.Test.transport_error(conn, :econnrefused)
      end)

      assert {:error, {:crawl4ai_failed, _reason}} =
               Crawl4ai.fetch("http://example.com",
                 plug: {Req.Test, stub_name},
                 retry: false
               )
    end
  end

  describe "fetch/2 — async fallback (crawl_sync returns 404)" do
    test "falls back to async poll when crawl_sync returns 404" do
      stub_name = __MODULE__.AsyncHappyStub

      Req.Test.stub(stub_name, fn conn ->
        cond do
          conn.request_path =~ "/crawl_sync" ->
            Plug.Conn.send_resp(conn, 404, "not found")

          conn.request_path =~ "/crawl" and conn.method == "POST" ->
            Req.Test.json(conn, %{"task_id" => "abc-123"})

          conn.request_path =~ "/task/abc-123" ->
            Req.Test.json(conn, %{
              "status" => "completed",
              "result" => %{"markdown" => "# Async result"}
            })

          true ->
            Plug.Conn.send_resp(conn, 404, "unknown")
        end
      end)

      assert {:ok, result} =
               Crawl4ai.fetch("http://example.com", plug: {Req.Test, stub_name})

      assert result.markdown =~ "Async result"
      assert result.metadata.source == :crawl4ai
    end
  end
end
