defmodule ExAutoresearch.Test.AlwaysFailScraper do
  @behaviour ExAutoresearch.DeepResearch.Scraper
  def fetch(_url, _opts), do: {:error, :stub_always_fail}
end

defmodule ExAutoresearch.Test.AlwaysOkScraper do
  @behaviour ExAutoresearch.DeepResearch.Scraper
  def fetch(_url, _opts), do: {:ok, %{markdown: "stub", metadata: %{source: :stub}}}
end
