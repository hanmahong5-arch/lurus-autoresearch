defmodule ExAutoresearch.DeepResearch.ScraperTest do
  use ExUnit.Case, async: false

  alias ExAutoresearch.DeepResearch.Scraper

  setup do
    original = Application.fetch_env!(:ex_autoresearch, :scraper)
    on_exit(fn -> Application.put_env(:ex_autoresearch, :scraper, original) end)
    :ok
  end

  describe "fetch/2 wrapper" do
    test "delegates to configured impl on success" do
      Application.put_env(:ex_autoresearch, :scraper, ExAutoresearch.Test.AlwaysOkScraper)

      assert {:ok, %{metadata: %{fallback_used: false, primary_error: nil}}} =
               Scraper.fetch("http://example.com")
    end

    test "falls back to Native when primary errors" do
      Application.put_env(:ex_autoresearch, :scraper, ExAutoresearch.Test.AlwaysFailScraper)

      stub_name = __MODULE__.FallbackNativeStub

      Req.Test.stub(stub_name, fn conn ->
        conn
        |> Plug.Conn.put_resp_content_type("text/html")
        |> Plug.Conn.send_resp(200, "<html><body><p>Native fallback content</p></body></html>")
      end)

      assert {:ok,
              %{
                metadata: %{
                  fallback_used: true,
                  primary_error: :stub_always_fail,
                  source: :native
                }
              }} = Scraper.fetch("http://example.com", plug: {Req.Test, stub_name})
    end

    test "returns {:error, {:both_failed, ...}} when both fail" do
      Application.put_env(:ex_autoresearch, :scraper, ExAutoresearch.Test.AlwaysFailScraper)

      stub_name = __MODULE__.BothFailStub

      Req.Test.stub(stub_name, fn conn ->
        Req.Test.transport_error(conn, :econnrefused)
      end)

      assert {:error, {:both_failed, primary: :stub_always_fail, native: _}} =
               Scraper.fetch("http://example.com", plug: {Req.Test, stub_name}, retry: false)
    end

    test "does not double-fallback when primary IS Native" do
      Application.put_env(
        :ex_autoresearch,
        :scraper,
        ExAutoresearch.DeepResearch.Scraper.Native
      )

      stub_name = __MODULE__.NativePrimaryFailStub

      Req.Test.stub(stub_name, fn conn ->
        Req.Test.transport_error(conn, :econnrefused)
      end)

      result = Scraper.fetch("http://example.com", plug: {Req.Test, stub_name}, retry: false)
      assert {:error, error} = result
      refute match?({:both_failed, _}, error)
    end
  end
end
