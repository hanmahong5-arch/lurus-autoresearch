defmodule ExAutoresearch.DeepResearch.Scraper.NativeTest do
  use ExUnit.Case, async: true

  alias ExAutoresearch.DeepResearch.Scraper.Native

  describe "extract_text_from_html/1" do
    test "strips HTML tags" do
      html = "<html><body><h1>Hello</h1><p>World</p></body></html>"
      result = Native.extract_text_from_html(html)
      assert result =~ "Hello"
      assert result =~ "World"
      refute result =~ "<h1>"
      refute result =~ "<p>"
    end

    test "strips script and style tags with content" do
      html = """
      <html>
        <head>
          <style>body { color: red; }</style>
          <script>alert('xss')</script>
        </head>
        <body><p>Content</p></body>
      </html>
      """

      result = Native.extract_text_from_html(html)
      assert result =~ "Content"
      refute result =~ "color: red"
      refute result =~ "alert"
    end

    test "collapses whitespace" do
      html = "<p>Hello    world</p>"
      result = Native.extract_text_from_html(html)
      assert result == "Hello world"
    end
  end

  describe "fetch/2" do
    test "happy path: returns markdown from HTML response" do
      stub_name = __MODULE__.HappyPathStub

      Req.Test.stub(stub_name, fn conn ->
        conn
        |> Plug.Conn.put_resp_content_type("text/html")
        |> Plug.Conn.send_resp(200, "<html><body><p>Test content</p></body></html>")
      end)

      assert {:ok, %{markdown: text, metadata: %{source: :native}}} =
               Native.fetch("http://example.com", plug: {Req.Test, stub_name})

      assert text =~ "Test content"
    end

    test "returns error on non-200 status" do
      stub_name = __MODULE__.ErrorStub

      Req.Test.stub(stub_name, fn conn ->
        Plug.Conn.send_resp(conn, 404, "not found")
      end)

      assert {:error, {:http_error, 404}} =
               Native.fetch("http://example.com", plug: {Req.Test, stub_name})
    end
  end
end
