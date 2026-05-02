defmodule ExAutoresearch.DeepResearch.Search.Serper do
  @moduledoc """
  Web search via serper.dev (Google results). Requires an API key from
  serper.dev. Configure with `SERPER_API_KEY` env or
  `:ex_autoresearch, :search, serper_api_key: "..."`.
  """

  @behaviour ExAutoresearch.DeepResearch.Search

  require Logger

  @impl true
  def search(query, opts \\ []) do
    num_results = Keyword.get(opts, :num_results, 10)

    case api_key() do
      nil ->
        {:error, :no_api_key}

      key ->
        do_search(query, key, num_results)
    end
  end

  defp do_search(query, api_key, num_results) do
    body = Jason.encode!(%{q: query, num: num_results, gl: "us", hl: "en"})

    case Req.post("https://google.serper.dev/search",
           headers: %{"x-api-key" => api_key, "content-type" => "application/json"},
           body: body,
           retry: false,
           receive_timeout: 20_000
         ) do
      {:ok, %Req.Response{status: 200, body: data}} ->
        results =
          data
          |> Map.get("organic", [])
          |> Enum.map(fn item ->
            %{
              title: Map.get(item, "title", ""),
              url: Map.get(item, "link", ""),
              snippet: Map.get(item, "snippet", "")
            }
          end)

        {:ok, results}

      {:ok, %Req.Response{status: status} = resp} ->
        Logger.error("Serper returned #{status}: #{inspect(resp.body, limit: 200)}")
        {:error, {:api_error, status}}

      {:error, reason} ->
        {:error, {:request_failed, reason}}
    end
  end

  defp api_key do
    System.get_env("SERPER_API_KEY") ||
      case Application.get_env(:ex_autoresearch, :search) do
        kw when is_list(kw) -> Keyword.get(kw, :serper_api_key)
        m when is_map(m) -> Map.get(m, :serper_api_key)
        _ -> nil
      end
  end
end
