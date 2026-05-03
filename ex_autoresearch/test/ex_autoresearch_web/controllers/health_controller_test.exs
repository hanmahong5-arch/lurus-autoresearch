defmodule ExAutoresearchWeb.HealthControllerTest do
  use ExAutoresearchWeb.ConnCase

  describe "GET /healthz" do
    test "returns 200 with status ok and version", %{conn: conn} do
      conn = get(conn, ~p"/healthz")

      assert %{"status" => "ok", "version" => version} = json_response(conn, 200)
      assert is_binary(version)
    end

    test "does not require authentication", %{conn: conn} do
      conn = get(conn, ~p"/healthz")
      assert json_response(conn, 200)
    end
  end

  describe "GET /readyz" do
    test "returns 200 ready when Repo is up; reports crawl4ai status", %{conn: conn} do
      conn = get(conn, ~p"/readyz")

      body = json_response(conn, 200)
      assert body["status"] == "ready"
      assert body["checks"]["repo"] == "ok"
      assert body["checks"]["crawl4ai"] in ["ok", "degraded"]
      assert is_binary(body["version"])
    end

    test "does not require authentication", %{conn: conn} do
      conn = get(conn, ~p"/readyz")
      assert json_response(conn, 200)
    end
  end
end
