defmodule ExAutoresearch.DeepResearch.Scraper.TelemetryTest do
  use ExUnit.Case, async: false

  alias ExAutoresearch.DeepResearch.Scraper

  setup do
    original = Application.fetch_env!(:ex_autoresearch, :scraper)
    on_exit(fn -> Application.put_env(:ex_autoresearch, :scraper, original) end)

    test_pid = self()
    ref = make_ref()

    :telemetry.attach_many(
      ref,
      [
        [:ex_autoresearch, :scraper, :fetch, :start],
        [:ex_autoresearch, :scraper, :fetch, :stop]
      ],
      fn name, measurements, metadata, _ ->
        send(test_pid, {:telemetry, name, measurements, metadata})
      end,
      nil
    )

    on_exit(fn -> :telemetry.detach(ref) end)
    :ok
  end

  describe "telemetry events" do
    test "fires start and stop events on primary success" do
      Application.put_env(:ex_autoresearch, :scraper, ExAutoresearch.Test.AlwaysOkScraper)

      Scraper.fetch("http://example.com")

      assert_receive {:telemetry, [:ex_autoresearch, :scraper, :fetch, :start], _measurements,
                      _metadata}

      assert_receive {:telemetry, [:ex_autoresearch, :scraper, :fetch, :stop], _measurements,
                      stop_meta}

      assert stop_meta.outcome == :primary_success
    end

    test "stop metadata has outcome: :fallback_success and primary_error set on fallback" do
      Application.put_env(:ex_autoresearch, :scraper, ExAutoresearch.Test.AlwaysFailScraper)

      stub_name = __MODULE__.TelemetryFallbackStub

      Req.Test.stub(stub_name, fn conn ->
        conn
        |> Plug.Conn.put_resp_content_type("text/html")
        |> Plug.Conn.send_resp(200, "<html><body><p>fallback</p></body></html>")
      end)

      Scraper.fetch("http://example.com", plug: {Req.Test, stub_name})

      assert_receive {:telemetry, [:ex_autoresearch, :scraper, :fetch, :stop], _measurements,
                      stop_meta}

      assert stop_meta.outcome == :fallback_success
      assert stop_meta.primary_error == :stub_always_fail
    end

    test "stop metadata has outcome: :both_failed on both-fail" do
      Application.put_env(:ex_autoresearch, :scraper, ExAutoresearch.Test.AlwaysFailScraper)

      stub_name = __MODULE__.TelemetryBothFailStub

      Req.Test.stub(stub_name, fn conn ->
        Req.Test.transport_error(conn, :econnrefused)
      end)

      Scraper.fetch("http://example.com", plug: {Req.Test, stub_name}, retry: false)

      assert_receive {:telemetry, [:ex_autoresearch, :scraper, :fetch, :stop], _measurements,
                      stop_meta}

      assert stop_meta.outcome == :both_failed
    end
  end
end
