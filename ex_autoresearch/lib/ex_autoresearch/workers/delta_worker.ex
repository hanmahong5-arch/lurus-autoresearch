defmodule ExAutoresearch.Workers.DeltaWorker do
  @moduledoc """
  Oban worker that computes the claim-level diff between consecutive Brief report runs
  and persists a Delta record with a markdown digest.

  Enqueued after a ResearchWorker completes a report that has a brief_id.
  """

  use Oban.Worker, queue: :default, max_attempts: 3

  require Logger
  require Ash.Query

  alias ExAutoresearch.Research.{Report, Claim, Delta, Brief}
  alias ExAutoresearch.Agent.LLMClient
  alias ExAutoresearch.Workers.NotifyWorker

  @impl Oban.Worker
  def perform(%Oban.Job{
        args: %{
          "to_report_id" => to_report_id,
          "brief_id" => brief_id,
          "organization_id" => org_id
        }
      }) do
    try do
      to_report = Ash.get!(Report, to_report_id, tenant: org_id)

      from_report = find_prior_report(brief_id, to_report.run_version, org_id)

      if is_nil(from_report) do
        Logger.info("[DeltaWorker] First run for brief #{brief_id}, no delta to compute")

        :ok
      else
        compute_and_store_delta(to_report, from_report, brief_id, org_id)
      end
    rescue
      e ->
        Logger.error(
          "[DeltaWorker] Failed: #{Exception.message(e)}\n#{Exception.format_stacktrace(__STACKTRACE__)}"
        )

        {:error, Exception.message(e)}
    end
  end

  # --- Public testable functions ---

  @doc """
  Computes the structural diff between two lists of claims (matched by claim_hash).

  Returns a map with:
    - added: to_claims not in from (after removing fuzzy-matched changed pairs)
    - removed: from_claims not in to (after removing fuzzy-matched changed pairs)
    - changed: [{from_claim, to_claim}] pairs with Jaccard similarity >= 0.5
    - contradicted_count: added + changed(to-side) with grounding == :contradicted
  """
  @spec diff_claims(list(map()), list(map())) :: %{
          added: list(),
          removed: list(),
          changed: list(),
          contradicted_count: integer()
        }
  def diff_claims(from_claims, to_claims) do
    from_hashes = MapSet.new(from_claims, & &1.claim_hash)
    to_hashes = MapSet.new(to_claims, & &1.claim_hash)

    raw_added = Enum.filter(to_claims, &(&1.claim_hash not in from_hashes))
    raw_removed = Enum.filter(from_claims, &(&1.claim_hash not in to_hashes))

    {changed, remaining_added, remaining_removed} =
      fuzzy_match_changed(raw_added, raw_removed)

    contradicted_count =
      Enum.count(remaining_added, &(&1.grounding == :contradicted)) +
        Enum.count(changed, fn {_from, to} -> to.grounding == :contradicted end)

    %{
      added: remaining_added,
      removed: remaining_removed,
      changed: changed,
      contradicted_count: contradicted_count
    }
  end

  @doc """
  Builds a fallback markdown digest from the diff without using an LLM.
  Always succeeds — used when LLM is unavailable or fails.
  """
  @spec fallback_digest(map()) :: String.t()
  def fallback_digest(%{
        added: added,
        removed: removed,
        changed: changed,
        contradicted_count: contradicted_count
      }) do
    lines = ["## Research Delta Summary\n"]

    lines =
      lines ++
        [
          "**Added:** #{length(added)} claims | **Changed:** #{length(changed)} | **Removed:** #{length(removed)} | **Contradicted:** #{contradicted_count}\n"
        ]

    lines =
      if added != [] do
        items = Enum.map(added, &"- #{String.slice(&1.text || "", 0, 200)}")
        lines ++ ["\n### Added Claims\n" | items]
      else
        lines
      end

    lines =
      if changed != [] do
        items =
          Enum.map(changed, fn {_from, to} ->
            "- #{String.slice(to.text || "", 0, 200)}"
          end)

        lines ++ ["\n### Changed Claims\n" | items]
      else
        lines
      end

    lines =
      if removed != [] do
        items = Enum.map(removed, &"- #{String.slice(&1.text || "", 0, 200)}")
        lines ++ ["\n### Removed Claims\n" | items]
      else
        lines
      end

    Enum.join(lines, "\n")
  end

  # --- Private helpers ---

  defp find_prior_report(brief_id, current_version, org_id) do
    Report
    |> Ash.Query.filter(
      brief_id == ^brief_id and
        status == :completed and
        run_version < ^current_version
    )
    |> Ash.Query.sort(run_version: :desc)
    |> Ash.Query.limit(1)
    |> Ash.read!(tenant: org_id)
    |> List.first()
  rescue
    _ -> nil
  end

  defp load_claims(report_id) do
    Claim
    |> Ash.Query.filter(report_id == ^report_id)
    |> Ash.read!()
  rescue
    _ -> []
  end

  defp compute_and_store_delta(to_report, from_report, brief_id, org_id) do
    from_claims = load_claims(from_report.id)
    to_claims = load_claims(to_report.id)

    diff = diff_claims(from_claims, to_claims)

    %{added: added, removed: removed, changed: changed, contradicted_count: contradicted_count} =
      diff

    markdown_digest = build_digest(diff, to_report)

    delta =
      Ash.create!(
        Delta,
        %{
          brief_id: brief_id,
          organization_id: org_id,
          from_report_id: from_report.id,
          to_report_id: to_report.id,
          markdown_digest: markdown_digest,
          added_count: length(added),
          changed_count: length(changed),
          removed_count: length(removed),
          contradicted_count: contradicted_count,
          generated_at: DateTime.utc_now()
        },
        action: :create,
        tenant: org_id
      )

    try do
      Phoenix.PubSub.broadcast(
        ExAutoresearch.PubSub,
        "deltas:events",
        {:delta_created, %{delta_id: delta.id, brief_id: brief_id, organization_id: org_id}}
      )
    rescue
      _ -> :ok
    end

    maybe_enqueue_notify(delta, brief_id, org_id)

    :ok
  end

  defp maybe_enqueue_notify(delta, brief_id, org_id) do
    case Ash.get(Brief, brief_id) do
      {:ok, brief} ->
        channels = brief.notify_channels || []
        non_inbox = Enum.any?(channels, fn ch -> ch != "inbox" end)

        if non_inbox do
          Oban.insert(
            NotifyWorker.new(%{
              "delta_id" => delta.id,
              "brief_id" => brief_id,
              "organization_id" => org_id
            })
          )
        end

      _ ->
        :ok
    end
  rescue
    _ -> :ok
  end

  defp build_digest(diff, to_report) do
    prompt = build_llm_prompt(diff, to_report)

    case LLMClient.complete(prompt, tier: :main, timeout: 90_000, report_id: to_report.id) do
      {:ok, text} when is_binary(text) and text != "" ->
        text

      _ ->
        fallback_digest(diff)
    end
  rescue
    _ -> fallback_digest(diff)
  end

  defp build_llm_prompt(
         %{
           added: added,
           removed: removed,
           changed: changed,
           contradicted_count: contradicted_count
         },
         to_report
       ) do
    added_texts = Enum.map_join(added, "\n", &"- #{&1.text}")
    removed_texts = Enum.map_join(removed, "\n", &"- #{&1.text}")

    changed_texts =
      Enum.map_join(changed, "\n", fn {from_c, to_c} ->
        "- FROM: #{from_c.text}\n  TO:   #{to_c.text}"
      end)

    """
    You are summarizing what changed between two research runs on the same topic.

    Report title: #{to_report.title}
    Query: #{to_report.query}

    Statistics: #{length(added)} added, #{length(changed)} changed, #{length(removed)} removed, #{contradicted_count} contradicted

    Added claims (new findings):
    #{if added == [], do: "(none)", else: added_texts}

    Changed claims (updated):
    #{if changed == [], do: "(none)", else: changed_texts}

    Removed claims (no longer present):
    #{if removed == [], do: "(none)", else: removed_texts}

    Write a concise 2-4 paragraph markdown summary of what changed since the last run and why it matters.
    Focus on significance, not enumeration.
    """
  end

  # --- Fuzzy matching ---

  defp fuzzy_match_changed(added_claims, removed_claims) do
    do_fuzzy_match(added_claims, removed_claims, [])
  end

  defp do_fuzzy_match([], remaining_removed, pairs) do
    {pairs, [], remaining_removed}
  end

  defp do_fuzzy_match(remaining_added, [], pairs) do
    {pairs, remaining_added, []}
  end

  defp do_fuzzy_match([added | rest_added], removed_claims, pairs) do
    best =
      removed_claims
      |> Enum.map(fn removed ->
        {removed, jaccard_similarity(added.text, removed.text)}
      end)
      |> Enum.filter(fn {_, score} -> score >= 0.5 end)
      |> Enum.max_by(fn {_, score} -> score end, fn -> nil end)

    case best do
      nil ->
        {final_pairs, final_added, final_removed} =
          do_fuzzy_match(rest_added, removed_claims, pairs)

        {final_pairs, [added | final_added], final_removed}

      {matched_removed, _score} ->
        remaining_removed = List.delete(removed_claims, matched_removed)

        do_fuzzy_match(rest_added, remaining_removed, [{matched_removed, added} | pairs])
    end
  end

  defp jaccard_similarity(text_a, text_b) when is_binary(text_a) and is_binary(text_b) do
    tokens_a = tokenize(text_a)
    tokens_b = tokenize(text_b)

    intersection_size = MapSet.intersection(tokens_a, tokens_b) |> MapSet.size()
    union_size = MapSet.union(tokens_a, tokens_b) |> MapSet.size()

    if union_size == 0, do: 0.0, else: intersection_size / union_size
  end

  defp jaccard_similarity(_, _), do: 0.0

  defp tokenize(text) do
    text
    |> String.downcase()
    |> String.split(~r/[^a-z0-9]+/, trim: true)
    |> MapSet.new()
  end
end
