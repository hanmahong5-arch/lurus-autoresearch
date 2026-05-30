defmodule ExAutoresearch.DeepResearch.VerifierTest do
  @moduledoc """
  Unit tests for the pure helpers in Verifier — no real LLM calls.
  """
  use ExUnit.Case, async: true

  alias ExAutoresearch.DeepResearch.Verifier

  describe "normalize/1" do
    test "downcases text" do
      assert Verifier.normalize("Hello WORLD") == "hello world"
    end

    test "collapses whitespace" do
      assert Verifier.normalize("foo   bar\nbaz") == "foo bar baz"
    end

    test "strips non-alphanumeric characters" do
      assert Verifier.normalize("foo! bar, baz.") == "foo bar baz"
    end

    test "handles empty string" do
      assert Verifier.normalize("") == ""
    end

    test "is idempotent" do
      text = "  The quick  Brown Fox! "
      once = Verifier.normalize(text)
      twice = Verifier.normalize(once)
      assert once == twice
    end
  end

  describe "claim_hash/1" do
    test "same text produces same hash" do
      assert Verifier.claim_hash("The sky is blue.") ==
               Verifier.claim_hash("The sky is blue.")
    end

    test "case-insensitive: same normalized content → same hash" do
      assert Verifier.claim_hash("The Sky Is Blue") ==
               Verifier.claim_hash("the sky is blue")
    end

    test "whitespace-insensitive: collapsed → same hash" do
      assert Verifier.claim_hash("foo  bar") == Verifier.claim_hash("foo bar")
    end

    test "different text produces different hash" do
      refute Verifier.claim_hash("Claim A") == Verifier.claim_hash("Claim B")
    end

    test "returns a 64-character hex string (SHA-256)" do
      hash = Verifier.claim_hash("some claim")
      assert String.length(hash) == 64
      assert hash =~ ~r/^[0-9a-f]+$/
    end
  end

  describe "verification_footer/1" do
    test "includes the tally line" do
      claims = [
        %{grounding: :grounded, confidence: 0.9, text: "Claim A", order_index: 0},
        %{grounding: :contradicted, confidence: 0.8, text: "Claim B", order_index: 1},
        %{grounding: :unsupported, confidence: 0.1, text: "Claim C", order_index: 2},
        %{grounding: :complementary, confidence: 0.6, text: "Claim D", order_index: 3}
      ]

      footer = Verifier.verification_footer(claims)

      assert footer =~ "1 grounded"
      assert footer =~ "1 contradicted"
      assert footer =~ "1 unsupported"
      assert footer =~ "1 complementary"
    end

    test "lists contradicted claims" do
      claims = [
        %{grounding: :contradicted, confidence: 0.75, text: "Bad claim here", order_index: 0}
      ]

      footer = Verifier.verification_footer(claims)
      assert footer =~ "Contradicted"
      assert footer =~ "Bad claim here"
    end

    test "lists unsupported claims" do
      claims = [
        %{grounding: :unsupported, confidence: 0.0, text: "Unsupported claim", order_index: 0}
      ]

      footer = Verifier.verification_footer(claims)
      assert footer =~ "Unsupported"
      assert footer =~ "Unsupported claim"
    end

    test "tally-only when all claims are grounded" do
      claims = [
        %{grounding: :grounded, confidence: 1.0, text: "Claim A", order_index: 0},
        %{grounding: :grounded, confidence: 0.95, text: "Claim B", order_index: 1}
      ]

      footer = Verifier.verification_footer(claims)
      assert footer =~ "2 grounded"
      assert footer =~ "0 contradicted"
      assert footer =~ "0 unsupported"
      # No individual claim lines for grounded
      refute footer =~ "Claim A"
      refute footer =~ "Claim B"
    end

    test "includes section heading" do
      footer = Verifier.verification_footer([])
      assert footer =~ "## ⚠ Verification Notes"
    end

    test "empty claims list still produces tally" do
      footer = Verifier.verification_footer([])
      assert footer =~ "0 grounded"
      assert footer =~ "0 contradicted"
    end
  end
end
