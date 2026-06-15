defmodule ExAutoresearch.DeepResearch.Verifier do
  @moduledoc """
  Trust-layer verification for completed research reports.

  Extracts atomic factual claims from the draft report body, grounds them
  against the numbered investigation evidence, persists `Claim` rows, and
  appends a `## ⚠ Verification Notes` footer to the body.

  `verify/3` ALWAYS returns `{:ok, annotated_body}`. Any internal failure
  logs a warning and returns the body unchanged — it never blocks report
  completion.
  """

  require Logger
  require Ash.Query

  alias ExAutoresearch.DeepResearch.SourcesBlock
  alias ExAutoresearch.Agent.LLMClient
  alias ExAutoresearch.Research.{Claim, Source}

  @verify_timeout :timer.seconds(90)
  @max_contradiction_claims 12

  @doc """
  Verifies the given `body` against `investigations`, persists claims, and
  returns `{:ok, annotated_body}` with a verification footer appended.
  """
  @spec verify(Ash.Resource.record(), String.t(), [map()]) :: {:ok, String.t()}
  def verify(report, body, investigations) do
    try do
      do_verify(report, body, investigations)
    rescue
      e ->
        Logger.warning("[Verifier] Unexpected error: #{Exception.message(e)}")
        {:ok, body}
    end
  end

  @doc "Normalizes a claim text: downcase, collapse whitespace, strip non-alphanumeric."
  @spec normalize(String.t()) :: String.t()
  def normalize(text) when is_binary(text) do
    text
    |> String.downcase()
    |> String.replace(~r/\s+/, " ")
    |> String.trim()
    |> String.replace(~r/[^a-z0-9 ]/, "")
    |> String.replace(~r/\s+/, " ")
    |> String.trim()
  end

  @doc "Returns the SHA-256 hex digest of the normalized claim text."
  @spec claim_hash(String.t()) :: String.t()
  def claim_hash(text) when is_binary(text) do
    text
    |> normalize()
    |> then(&:crypto.hash(:sha256, &1))
    |> Base.encode16(case: :lower)
  end

  @doc """
  Builds a `## ⚠ Verification Notes` markdown footer from a list of claim maps.

  Each claim map must have `:grounding`, `:confidence`, `:text` keys.
  Always includes a grounding tally line.  Lists `:contradicted` and
  `:unsupported` claims individually.
  """
  @spec verification_footer([map()]) :: String.t()
  def verification_footer(claims) when is_list(claims) do
    grounded = Enum.count(claims, &(&1.grounding == :grounded))
    contradicted = Enum.count(claims, &(&1.grounding == :contradicted))
    unsupported = Enum.count(claims, &(&1.grounding == :unsupported))
    complementary = Enum.count(claims, &(&1.grounding == :complementary))

    tally =
      "#{grounded} grounded · #{contradicted} contradicted · #{unsupported} unsupported · #{complementary} complementary"

    flagged =
      claims
      |> Enum.filter(&(&1.grounding in [:contradicted, :unsupported]))
      |> Enum.sort_by(& &1.order_index)

    flagged_lines =
      if flagged == [] do
        ""
      else
        lines =
          Enum.map_join(flagged, "\n", fn c ->
            pct = Float.round((c.confidence || 0.0) * 100, 0) |> trunc()

            suffix =
              if c.grounding == :contradicted and
                   not Enum.empty?(List.wrap(c[:contradicting_source_ids])),
                 do: " (sources disagree)",
                 else: ""

            "- **#{String.capitalize(to_string(c.grounding))}** (#{pct}%): #{c.text}#{suffix}"
          end)

        "\n\n" <> lines
      end

    "\n\n## ⚠ Verification Notes\n\n#{tally}#{flagged_lines}"
  end

  # ── Private implementation ──────────────────────────────────────────────────

  defp do_verify(report, body, _investigations) do
    numbered = SourcesBlock.numbered_sources(report)

    if numbered == [] do
      {:ok, body}
    else
      claims_data = extract_claims(report, body, numbered)
      grounded_claims = ground_claims(report, claims_data, numbered)
      grounded_claims = detect_contradictions(report, grounded_claims, numbered)
      persisted = persist_claims(report, grounded_claims, numbered)
      footer = verification_footer(persisted)
      broadcast_verified(report, persisted)
      {:ok, body <> footer}
    end
  end

  # Step 1: Extract claims via LLM
  defp extract_claims(report, body, numbered) do
    source_list =
      Enum.map_join(numbered, "\n", fn {idx, inv} -> "[#{idx}] #{inv.query}" end)

    prompt = """
    You are a fact-extraction assistant. Given the research report below, split it
    into atomic factual claims. For each claim, identify which inline citation
    numbers [n] it relies on.

    Numbered sources available:
    #{source_list}

    Report body:
    #{body}

    Return a JSON array ONLY (no prose, no markdown fences) of objects:
    [{"text": "...", "citations": [<n>, ...], "order_index": <int>}, ...]

    Use the order the claims appear in the report for order_index (starting at 0).
    If a claim has no citation, use an empty array for citations.
    """

    with {:ok, raw} <-
           LLMClient.complete(prompt,
             tier: :main,
             timeout: @verify_timeout,
             report_id: report.id
           ),
         {:ok, list} when is_list(list) <- parse_json_array(raw) do
      list
      |> Enum.with_index()
      |> Enum.map(fn {item, i} ->
        %{
          text: Map.get(item, "text", "") |> to_string(),
          citations: Map.get(item, "citations", []) |> List.wrap() |> Enum.filter(&is_integer/1),
          order_index: Map.get(item, "order_index", i)
        }
      end)
      |> Enum.filter(&(byte_size(&1.text) > 0))
    else
      _ ->
        Logger.warning("[Verifier] Claim extraction failed for report #{report.id}")
        []
    end
  end

  # Step 2: Ground claims via batched LLM call
  defp ground_claims(_report, [], _numbered), do: []

  defp ground_claims(report, claims, numbered) do
    evidence =
      Enum.map_join(numbered, "\n\n", fn {idx, inv} ->
        excerpt =
          String.slice(
            inv.findings || "",
            0,
            Application.get_env(:ex_autoresearch, :evidence_max_chars, 2000)
          )

        "[#{idx}] #{inv.query}\n#{excerpt}"
      end)

    numbered_claims =
      claims
      |> Enum.with_index()
      |> Enum.map_join("\n", fn {c, _i} ->
        "order_index=#{c.order_index}: #{c.text}"
      end)

    prompt = """
    You are a grounding verifier. For each claim listed below, determine whether
    the provided evidence supports, contradicts, or has no information about it.

    Claims:
    #{numbered_claims}

    Evidence:
    #{evidence}

    Return a JSON array ONLY (no prose, no markdown fences):
    [{"index": <order_index>, "grounding": "grounded|contradicted|unsupported|complementary", "confidence": 0.0-1.0}, ...]

    Grounding meanings:
    - grounded: evidence clearly supports the claim
    - contradicted: evidence contradicts the claim
    - unsupported: evidence does not address this claim
    - complementary: evidence adds context but neither proves nor disproves

    Return one entry per claim. Use the same order_index values from the input.
    """

    grounding_map =
      with {:ok, raw} <-
             LLMClient.complete(prompt,
               tier: :main,
               timeout: @verify_timeout,
               report_id: report.id
             ),
           {:ok, list} when is_list(list) <- parse_json_array(raw) do
        Enum.reduce(list, %{}, fn item, acc ->
          idx = Map.get(item, "index")
          grounding = parse_grounding(Map.get(item, "grounding"))
          confidence = Map.get(item, "confidence", 0.0) |> to_float()
          if is_integer(idx), do: Map.put(acc, idx, {grounding, confidence}), else: acc
        end)
      else
        _ ->
          Logger.warning("[Verifier] Grounding call failed for report #{report.id}")
          %{}
      end

    Enum.map(claims, fn claim ->
      {grounding, confidence} = Map.get(grounding_map, claim.order_index, {:unsupported, 0.0})
      Map.merge(claim, %{grounding: grounding, confidence: confidence})
    end)
  end

  # Step 2b: Detect cross-source contradictions via LLM; graceful — returns claims unchanged on failure
  defp detect_contradictions(_report, [], _numbered), do: []

  defp detect_contradictions(report, claims, numbered) do
    candidates =
      claims
      |> Enum.filter(fn c -> length(List.wrap(c[:citations])) >= 2 end)
      |> Enum.take(@max_contradiction_claims)

    if candidates == [] do
      claims
    else
      idx_to_inv = Map.new(numbered, fn {idx, inv} -> {idx, inv} end)
      max_chars = Application.get_env(:ex_autoresearch, :evidence_max_chars, 2000)

      candidates_block =
        Enum.map_join(candidates, "\n", fn c ->
          source_excerpts =
            c.citations
            |> List.wrap()
            |> Enum.map(fn idx ->
              inv = Map.get(idx_to_inv, idx)
              excerpt = String.slice((inv && inv.findings) || "", 0, max_chars)
              "[#{idx}] #{(inv && inv.query) || "unknown"}: #{excerpt}"
            end)
            |> Enum.join("\n")

          "order_index=#{c.order_index}: #{c.text}\nSources cited:\n#{source_excerpts}"
        end)

      prompt = """
      You are a contradiction detector. For each claim below, determine whether the SPECIFIC NUMBERED SOURCES cited for that claim directly disagree with each other (not whether the claim is right or wrong overall).

      Claims with their per-source evidence:
      #{candidates_block}

      Return a JSON array ONLY (no prose, no markdown fences):
      [{"index": <order_index>, "contradicting_sources": [<n>, <n>], "explanation": "..."}, ...]

      Only include entries where at least 2 source numbers directly contradict each other.
      If no contradiction exists for a claim, omit it from the array.
      """

      with {:ok, raw} <-
             LLMClient.complete(prompt,
               tier: :main,
               timeout: @verify_timeout,
               report_id: report.id
             ),
           {:ok, decoded} when is_list(decoded) <- parse_json_array(raw) do
        apply_contradictions(claims, decoded, numbered, report.id)
      else
        _ ->
          Logger.warning("[Verifier] Contradiction detection failed for report #{report.id}")
          claims
      end
    end
  end

  @doc """
  Pure function: merges contradiction detection results into the claims list.

  For each decoded item with a valid integer `index` and at least 2
  `contradicting_sources`, finds the matching claim by `order_index`, sets
  `grounding: :contradicted` and `contradicting_source_ids` to the resolved
  source UUIDs (nils dropped). Claims not referenced are returned unchanged.
  """
  @spec apply_contradictions(list(map()), list(map()), [{integer(), map()}], binary()) ::
          list(map())
  def apply_contradictions(claims, decoded, numbered, report_id) do
    idx_to_inv = Map.new(numbered, fn {idx, inv} -> {idx, inv} end)

    updates =
      Enum.reduce(decoded, %{}, fn item, acc ->
        order_index = Map.get(item, "index")
        sources = Map.get(item, "contradicting_sources", []) |> List.wrap()

        if is_integer(order_index) and length(sources) >= 2 do
          source_ids =
            sources
            |> Enum.map(fn n ->
              inv = Map.get(idx_to_inv, n)
              resolve_source_id(report_id, inv && inv.url)
            end)
            |> Enum.reject(&is_nil/1)

          Map.put(acc, order_index, source_ids)
        else
          acc
        end
      end)

    Enum.map(claims, fn claim ->
      case Map.get(updates, claim.order_index) do
        nil ->
          claim

        source_ids ->
          claim
          |> Map.put(:grounding, :contradicted)
          |> Map.put(:contradicting_source_ids, source_ids)
      end
    end)
  end

  # Step 3: Persist Claim rows; one bad row must not abort
  defp persist_claims(report, claims, numbered) do
    idx_to_inv = Map.new(numbered, fn {idx, inv} -> {idx, inv} end)

    Enum.map(claims, fn claim ->
      try do
        first_cited_idx = List.first(claim.citations)
        inv = first_cited_idx && Map.get(idx_to_inv, first_cited_idx)
        source_id = resolve_source_id(report.id, inv && inv.url)

        origin_subquery = inv && inv.query

        hash = claim_hash(claim.text)

        Ash.create!(
          Claim,
          %{
            report_id: report.id,
            source_id: source_id,
            text: claim.text,
            grounding: claim.grounding,
            confidence: claim.confidence,
            origin_subquery: origin_subquery,
            claim_hash: hash,
            order_index: claim.order_index,
            contradicting_source_ids: claim[:contradicting_source_ids]
          },
          action: :create
        )

        # Return a plain map for the footer (avoids Ash struct differences)
        %{
          grounding: claim.grounding,
          confidence: claim.confidence,
          text: claim.text,
          order_index: claim.order_index,
          contradicting_source_ids: claim[:contradicting_source_ids]
        }
      rescue
        e ->
          Logger.warning("[Verifier] Failed to persist claim: #{Exception.message(e)}")

          %{
            grounding: claim.grounding,
            confidence: claim.confidence || 0.0,
            text: claim.text,
            order_index: claim.order_index,
            contradicting_source_ids: claim[:contradicting_source_ids]
          }
      end
    end)
  end

  defp resolve_source_id(_report_id, nil), do: nil

  defp resolve_source_id(report_id, url) do
    Source
    |> Ash.Query.filter(report_id == ^report_id and url == ^url)
    |> Ash.read!()
    |> List.first()
    |> case do
      nil -> nil
      source -> source.id
    end
  rescue
    _ -> nil
  end

  # Step 4: Broadcast verification summary
  defp broadcast_verified(report, claims) do
    g = Enum.count(claims, &(&1.grounding == :grounded))
    c = Enum.count(claims, &(&1.grounding == :contradicted))
    u = Enum.count(claims, &(&1.grounding == :unsupported))
    cp = Enum.count(claims, &(&1.grounding == :complementary))

    Phoenix.PubSub.broadcast(
      ExAutoresearch.PubSub,
      "research:events",
      {:claims_verified,
       %{report_id: report.id, grounded: g, contradicted: c, unsupported: u, complementary: cp}}
    )
  rescue
    _ -> :ok
  end

  # ── Pure helpers ─────────────────────────────────────────────────────────────

  defp parse_json_array(body) when is_binary(body) do
    with [json] <- Regex.run(~r/\[.*\]/s, body),
         {:ok, list} when is_list(list) <- Jason.decode(json) do
      {:ok, list}
    else
      _ -> :error
    end
  end

  defp parse_json_array(_), do: :error

  defp parse_grounding("grounded"), do: :grounded
  defp parse_grounding("contradicted"), do: :contradicted
  defp parse_grounding("complementary"), do: :complementary
  defp parse_grounding(_), do: :unsupported

  defp to_float(v) when is_float(v), do: v
  defp to_float(v) when is_integer(v), do: v * 1.0
  defp to_float(_), do: 0.0
end
