defmodule ExAutoresearch.Workers.DeltaWorkerTest do
  use ExAutoresearch.DataCase, async: false
  use Oban.Testing, repo: ExAutoresearch.Repo

  require Ash.Query

  alias ExAutoresearch.Workers.{DeltaWorker, NotifyWorker}
  alias ExAutoresearch.Research.{Brief, Report, Claim, Delta}
  alias ExAutoresearch.Accounts.Auth

  defp setup_org(suffix) do
    email = "delta_worker_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org
  end

  defp create_brief(org, extra \\ %{}) do
    Ash.create!(
      Brief,
      Map.merge(
        %{
          name: "Worker Test Brief #{System.unique_integer([:positive])}",
          question: "What is the AI landscape?",
          organization_id: org.id
        },
        extra
      ),
      action: :create,
      tenant: org.id
    )
  end

  defp create_report(org, brief, run_version, status \\ :completed) do
    r =
      Ash.create!(
        Report,
        %{
          title: "Test Report v#{run_version} #{System.unique_integer([:positive])}",
          query: "AI landscape query",
          organization_id: org.id,
          brief_id: brief.id,
          run_version: run_version
        },
        action: :start,
        tenant: org.id
      )

    if status == :completed do
      Ash.update!(r, %{status: :completed, markdown_body: "body"},
        action: :complete,
        tenant: org.id
      )
    else
      r
    end
  end

  defp create_claim(report_id, attrs) do
    Ash.create!(
      Claim,
      Map.merge(
        %{
          report_id: report_id,
          text: "Default claim text",
          claim_hash: "hash_#{System.unique_integer([:positive])}",
          grounding: :grounded
        },
        attrs
      ),
      action: :create
    )
  end

  # --- Unit tests for diff_claims/2 ---

  describe "diff_claims/2" do
    test "claims with shared hash are ignored (unchanged)" do
      shared_hash = "shared_abc"

      from_claims = [
        %{claim_hash: shared_hash, text: "Shared claim", grounding: :grounded}
      ]

      to_claims = [
        %{claim_hash: shared_hash, text: "Shared claim", grounding: :grounded}
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      assert result.added == []
      assert result.removed == []
      assert result.changed == []
      assert result.contradicted_count == 0
    end

    test "new claim (hash only in to) goes into added" do
      from_claims = [
        %{claim_hash: "old_hash", text: "Old claim", grounding: :grounded}
      ]

      to_claims = [
        %{claim_hash: "old_hash", text: "Old claim", grounding: :grounded},
        %{claim_hash: "new_hash", text: "Brand new claim about topic X", grounding: :grounded}
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      assert length(result.added) == 1
      assert hd(result.added).claim_hash == "new_hash"
      assert result.removed == []
    end

    test "missing claim (hash only in from) goes into removed" do
      from_claims = [
        %{claim_hash: "gone_hash", text: "This claim is gone", grounding: :grounded},
        %{claim_hash: "shared", text: "Shared", grounding: :grounded}
      ]

      to_claims = [
        %{claim_hash: "shared", text: "Shared", grounding: :grounded}
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      assert result.added == []
      assert length(result.removed) == 1
      assert hd(result.removed).claim_hash == "gone_hash"
    end

    test "Jaccard-similar pair (>= 0.5) becomes changed, not added/removed" do
      # "the quick brown fox" vs "the quick brown dog" — high overlap
      from_claims = [
        %{
          claim_hash: "hash_from",
          text: "the quick brown fox jumps over the lazy dog",
          grounding: :grounded
        }
      ]

      to_claims = [
        %{
          claim_hash: "hash_to",
          text: "the quick brown fox leaps over the lazy dog",
          grounding: :grounded
        }
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      assert result.added == []
      assert result.removed == []
      assert length(result.changed) == 1
      {from_c, to_c} = hd(result.changed)
      assert from_c.claim_hash == "hash_from"
      assert to_c.claim_hash == "hash_to"
    end

    test "dissimilar texts (Jaccard < 0.5) stay as added/removed" do
      from_claims = [
        %{claim_hash: "hash_from", text: "apples oranges bananas", grounding: :grounded}
      ]

      to_claims = [
        %{
          claim_hash: "hash_to",
          text: "quantum computing revolutionizes cryptography",
          grounding: :grounded
        }
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      assert length(result.added) == 1
      assert length(result.removed) == 1
      assert result.changed == []
    end

    test "contradicted added claim is counted in contradicted_count" do
      from_claims = []

      to_claims = [
        %{
          claim_hash: "contra_hash",
          text: "This finding contradicts previous research",
          grounding: :contradicted
        }
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      assert length(result.added) == 1
      assert result.contradicted_count == 1
    end

    test "contradicted claim in changed (to-side) counted" do
      from_claims = [
        %{
          claim_hash: "hash_from",
          text: "study shows vaccines are highly effective against variants",
          grounding: :grounded
        }
      ]

      to_claims = [
        %{
          claim_hash: "hash_to",
          text: "study shows vaccines are less effective against new variants",
          grounding: :contradicted
        }
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      assert length(result.changed) == 1
      assert result.contradicted_count == 1
    end

    test "each claim used once in greedy matching" do
      from_claims = [
        %{
          claim_hash: "f1",
          text: "the quick brown fox jumps over the lazy dog",
          grounding: :grounded
        },
        %{
          claim_hash: "f2",
          text: "the quick brown fox jumps over the lazy dog",
          grounding: :grounded
        }
      ]

      to_claims = [
        %{
          claim_hash: "t1",
          text: "the quick brown fox leaps over the lazy dog today",
          grounding: :grounded
        }
      ]

      result = DeltaWorker.diff_claims(from_claims, to_claims)

      # One from is matched (changed), one from is removed, to has 0 unmatched
      assert length(result.changed) == 1
      assert result.added == []
      # The second from claim becomes removed
      assert length(result.removed) == 1
    end
  end

  describe "fallback_digest/1" do
    test "builds a non-empty string from diff" do
      diff = %{
        added: [%{text: "New finding A"}],
        removed: [%{text: "Old finding B"}],
        changed: [],
        contradicted_count: 0
      }

      result = DeltaWorker.fallback_digest(diff)

      assert is_binary(result)
      assert String.length(result) > 0
      assert result =~ "Added"
      assert result =~ "New finding A"
    end

    test "handles empty diff gracefully" do
      diff = %{added: [], removed: [], changed: [], contradicted_count: 0}
      result = DeltaWorker.fallback_digest(diff)
      assert is_binary(result)
    end
  end

  # --- Integration tests ---

  describe "perform/1 integration" do
    test "first run (no prior report) — no Delta created", %{} do
      org = setup_org("first_run")
      brief = create_brief(org)
      report_v1 = create_report(org, brief, 1)

      # Seed some claims
      create_claim(report_v1.id, %{
        text: "AI chips are improving rapidly",
        claim_hash: "hash_001"
      })

      :ok =
        DeltaWorker.perform(%Oban.Job{
          args: %{
            "to_report_id" => report_v1.id,
            "brief_id" => brief.id,
            "organization_id" => org.id
          }
        })

      deltas =
        Delta
        |> Ash.Query.filter(brief_id == ^brief.id)
        |> Ash.read!(tenant: org.id)

      assert deltas == []
    end

    test "second run creates a Delta with correct counts", %{} do
      org = setup_org("second_run")
      brief = create_brief(org)

      report_v1 = create_report(org, brief, 1)
      report_v2 = create_report(org, brief, 2)

      # Shared claim (hash present in both)
      shared_hash = "shared_hash_xyz"

      create_claim(report_v1.id, %{
        text: "Shared finding about AI",
        claim_hash: shared_hash
      })

      create_claim(report_v2.id, %{
        text: "Shared finding about AI",
        claim_hash: shared_hash
      })

      # Claim only in v1 (removed)
      create_claim(report_v1.id, %{
        text: "Old finding that no longer applies",
        claim_hash: "old_hash_removed"
      })

      # Claim only in v2 (added)
      create_claim(report_v2.id, %{
        text: "Brand new finding discovered in run 2",
        claim_hash: "new_hash_added"
      })

      :ok =
        DeltaWorker.perform(%Oban.Job{
          args: %{
            "to_report_id" => report_v2.id,
            "brief_id" => brief.id,
            "organization_id" => org.id
          }
        })

      deltas =
        Delta
        |> Ash.Query.filter(brief_id == ^brief.id)
        |> Ash.read!(tenant: org.id)

      assert length(deltas) == 1
      delta = hd(deltas)

      assert delta.added_count == 1
      assert delta.removed_count == 1
      assert delta.to_report_id == report_v2.id
      assert delta.from_report_id == report_v1.id
      assert is_binary(delta.markdown_digest)
      assert String.length(delta.markdown_digest) > 0
    end

    test "Delta is created even without a working LLM (fallback digest)", %{} do
      org = setup_org("llm_fallback")
      brief = create_brief(org)

      report_v1 = create_report(org, brief, 1)
      report_v2 = create_report(org, brief, 2)

      create_claim(report_v1.id, %{
        text: "Old claim about market trends",
        claim_hash: "fallback_old"
      })

      create_claim(report_v2.id, %{
        text: "New claim about emerging technologies",
        claim_hash: "fallback_new"
      })

      # No LLM configured in test env — DeltaWorker must use fallback
      :ok =
        DeltaWorker.perform(%Oban.Job{
          args: %{
            "to_report_id" => report_v2.id,
            "brief_id" => brief.id,
            "organization_id" => org.id
          }
        })

      [delta] =
        Delta
        |> Ash.Query.filter(brief_id == ^brief.id)
        |> Ash.read!(tenant: org.id)

      assert is_binary(delta.markdown_digest)
      assert String.length(delta.markdown_digest) > 0
    end

    test "Brief with webhook channel enqueues a NotifyWorker job on second run", %{} do
      org = setup_org("notify_enqueue")

      brief =
        create_brief(org, %{notify_channels: ["webhook:https://example.test/hook"]})

      report_v1 = create_report(org, brief, 1)
      report_v2 = create_report(org, brief, 2)

      create_claim(report_v1.id, %{text: "Old finding", claim_hash: "nw_old"})
      create_claim(report_v2.id, %{text: "New finding", claim_hash: "nw_new"})

      :ok =
        DeltaWorker.perform(%Oban.Job{
          args: %{
            "to_report_id" => report_v2.id,
            "brief_id" => brief.id,
            "organization_id" => org.id
          }
        })

      assert_enqueued(worker: NotifyWorker)
    end

    test "Brief with no channels (empty list) does NOT enqueue NotifyWorker", %{} do
      org = setup_org("notify_no_enqueue_empty")
      brief = create_brief(org, %{notify_channels: []})

      report_v1 = create_report(org, brief, 1)
      report_v2 = create_report(org, brief, 2)

      create_claim(report_v1.id, %{text: "Old finding", claim_hash: "ne_old"})
      create_claim(report_v2.id, %{text: "New finding", claim_hash: "ne_new"})

      :ok =
        DeltaWorker.perform(%Oban.Job{
          args: %{
            "to_report_id" => report_v2.id,
            "brief_id" => brief.id,
            "organization_id" => org.id
          }
        })

      refute_enqueued(worker: NotifyWorker)
    end

    test "Brief with only inbox channel does NOT enqueue NotifyWorker", %{} do
      org = setup_org("notify_no_enqueue_inbox")
      brief = create_brief(org, %{notify_channels: ["inbox"]})

      report_v1 = create_report(org, brief, 1)
      report_v2 = create_report(org, brief, 2)

      create_claim(report_v1.id, %{text: "Old finding", claim_hash: "ni_old"})
      create_claim(report_v2.id, %{text: "New finding", claim_hash: "ni_new"})

      :ok =
        DeltaWorker.perform(%Oban.Job{
          args: %{
            "to_report_id" => report_v2.id,
            "brief_id" => brief.id,
            "organization_id" => org.id
          }
        })

      refute_enqueued(worker: NotifyWorker)
    end
  end
end
