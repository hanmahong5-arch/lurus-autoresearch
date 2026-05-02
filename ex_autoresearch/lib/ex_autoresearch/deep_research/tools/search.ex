defmodule ExAutoresearch.DeepResearch.Tools.Search do
  @moduledoc """
  Thin wrapper for backward compatibility. The actual dispatcher lives in
  `ExAutoresearch.DeepResearch.Search`. New code should call that module
  directly.
  """

  @spec search(String.t(), keyword()) ::
          {:ok, [ExAutoresearch.DeepResearch.Search.result()]} | {:error, term()}
  defdelegate search(query, opts \\ []), to: ExAutoresearch.DeepResearch.Search
end
