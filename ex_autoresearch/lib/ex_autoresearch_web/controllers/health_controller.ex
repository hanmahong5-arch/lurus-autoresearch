defmodule ExAutoresearchWeb.HealthController do
  @moduledoc """
  Liveness and readiness probes for K8s / docker-compose / load balancers.

  - `GET /healthz` — liveness. Always 200 if the BEAM is up and the controller
    can render. Returns `{status: "ok", version: "x.y.z"}`.

  - `GET /readyz` — readiness. 200 if the Repo accepts a ping; 503 otherwise.
    Crawl4AI reachability is reported as an informational field but never
    flips readiness — the auto-fallback to Native means the app can still
    serve research traffic without Crawl4AI.

  Neither route requires auth.
  """

  use ExAutoresearchWeb, :controller

  alias ExAutoresearch.Repo

  @version Mix.Project.config()[:version] || "unknown"
  @ping_timeout_ms 1_000

  def liveness(conn, _params) do
    json(conn, %{status: "ok", version: @version})
  end

  def readiness(conn, _params) do
    repo_state = check_repo()
    crawl4ai_state = check_crawl4ai()

    body = %{
      status: if(repo_state == :ok, do: "ready", else: "not_ready"),
      version: @version,
      checks: %{
        repo: state_to_string(repo_state),
        crawl4ai: state_to_string(crawl4ai_state)
      }
    }

    code = if repo_state == :ok, do: 200, else: 503

    conn
    |> put_status(code)
    |> json(body)
  end

  defp check_repo do
    Repo.query("SELECT 1", [], timeout: @ping_timeout_ms)
    :ok
  rescue
    _ -> :error
  catch
    :exit, _ -> :error
  end

  defp check_crawl4ai do
    base_url = crawl4ai_base_url()

    case Req.get(base_url <> "/health",
           receive_timeout: @ping_timeout_ms,
           retry: false,
           connect_options: [timeout: @ping_timeout_ms]
         ) do
      {:ok, %Req.Response{status: status}} when status in 200..299 -> :ok
      _ -> :degraded
    end
  rescue
    _ -> :degraded
  end

  defp crawl4ai_base_url do
    Application.get_env(:ex_autoresearch, :crawl4ai, [])
    |> Keyword.get(:base_url, "http://localhost:11235")
  end

  defp state_to_string(:ok), do: "ok"
  defp state_to_string(:degraded), do: "degraded"
  defp state_to_string(:error), do: "error"
end
