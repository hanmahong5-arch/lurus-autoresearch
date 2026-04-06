defmodule ExAutoresearch.DeepResearch.Tools.Search do
  @moduledoc """
  Web search tool using Serper (Google API) or Brave Search.

  Returns ranked results with title, URL, snippet, and relevance info.
  """

  require Logger

  @type result :: %{
          title: String.t(),
          url: String.t(),
          snippet: String.t()
        }

  @doc """
  Perform a web search and return results.
  """
  @spec search(String.t(), keyword()) :: {:ok, [result()]} | {:error, term()}
  def search(query, opts \\ []) do
    num_results = Keyword.get(opts, :num_results, 10)
    api_key = get_api_key("SERPER_API_KEY", :serper_api_key)

    if api_key do
      search_serper(query, api_key, num_results)
    else
      # No API key configured — LLM will have to search manually
      {:error, :no_api_key}
    end
  end

  defp get_api_key(env_var, config_atom) do
    System.get_env(env_var) ||
      case Application.get_env(:ex_autoresearch, :search) do
        map when is_map(map) -> Map.get(map, config_atom)
        _ -> nil
      end
  end

  defp search_serper(query, api_key, num_results) do
    body =
      Jason.encode!(%{
        q: query,
        num: num_results,
        gl: "us",
        hl: "en"
      })

    case Req.post(
           "https://google.serper.dev/search",
           headers: %{"x-api-key" => api_key, "content-type" => "application/json"},
           body: body,
           retry: :temporary,
           retry_max: 2
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
        Logger.error("Serper returned status #{status}: #{inspect(resp.body, limit: 200)}")
        {:error, {:api_error, status, resp.body["message"]}}

      {:error, reason} ->
        {:error, {:search_failed, reason}}
    end
  end
end
