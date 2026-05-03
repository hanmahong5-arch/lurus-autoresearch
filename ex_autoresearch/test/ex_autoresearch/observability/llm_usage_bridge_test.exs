defmodule ExAutoresearch.Observability.LLMUsageBridgeTest do
  use ExAutoresearch.DataCase, async: false

  alias ExAutoresearch.Observability.LLMUsageBridge
  alias ExAutoresearch.Accounts.Auth
  alias ExAutoresearch.Research.Report

  @event [:ex_autoresearch, :llm, :complete, :stop]

  setup do
    LLMUsageBridge.attach()

    on_exit(fn ->
      :telemetry.detach("ex-autoresearch-llm-usage-bridge")
    end)

    :ok
  end

  defp register_org(suffix) do
    email = "llm_bridge_#{suffix}_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org
  end

  defp create_report(org) do
    Ash.create!(
      Report,
      %{title: "LLM Bridge Test Report", query: "test query", organization_id: org.id},
      action: :start,
      tenant: org.id
    )
  end

  test "accumulates tokens onto the report tagged with report_id" do
    org = register_org("accum")
    report = create_report(org)

    :telemetry.execute(
      @event,
      %{duration: 100},
      %{
        report_id: report.id,
        organization_id: org.id,
        outcome: :ok,
        input_tokens: 1000,
        output_tokens: 500
      }
    )

    # Give the synchronous telemetry handler time to complete (it runs in-process)
    reloaded = Ash.get!(Report, report.id, tenant: org.id)

    assert reloaded.total_input_tokens == 1000
    assert reloaded.total_output_tokens == 500
    assert reloaded.llm_calls_count == 1
  end

  test "skips when report_id is missing" do
    org = register_org("skip_missing")
    report = create_report(org)

    :telemetry.execute(
      @event,
      %{duration: 100},
      %{
        organization_id: org.id,
        outcome: :ok,
        input_tokens: 1000,
        output_tokens: 500
      }
    )

    reloaded = Ash.get!(Report, report.id, tenant: org.id)
    assert reloaded.total_input_tokens == 0
    assert reloaded.total_output_tokens == 0
    assert reloaded.llm_calls_count == 0
  end

  test "skips when outcome is :error" do
    org = register_org("skip_error")
    report = create_report(org)

    :telemetry.execute(
      @event,
      %{duration: 100},
      %{
        report_id: report.id,
        organization_id: org.id,
        outcome: :error,
        input_tokens: 500,
        output_tokens: 200
      }
    )

    reloaded = Ash.get!(Report, report.id, tenant: org.id)
    assert reloaded.total_input_tokens == 0
    assert reloaded.total_output_tokens == 0
    assert reloaded.llm_calls_count == 0
  end
end
