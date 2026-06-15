defmodule ExAutoresearch.Workers.ResearchWorkerTest do
  use ExAutoresearch.DataCase, async: false
  use Oban.Testing, repo: ExAutoresearch.Repo

  alias ExAutoresearch.Workers.{ResearchWorker, DeltaWorker}
  alias ExAutoresearch.Research.{Brief, Report}
  alias ExAutoresearch.Accounts.Auth

  defp setup_org do
    email = "research_worker_test_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = Auth.register(email, "password123")
    org
  end

  defp create_brief(org) do
    Ash.create!(
      Brief,
      %{
        name: "RW Test Brief #{System.unique_integer([:positive])}",
        question: "What is the future of AI?",
        organization_id: org.id
      },
      action: :create,
      tenant: org.id
    )
  end

  defp create_completed_report(org, attrs \\ %{}) do
    base = %{
      title: "RW Test Report #{System.unique_integer([:positive])}",
      query: "test query",
      organization_id: org.id
    }

    r =
      Ash.create!(
        Report,
        Map.merge(base, attrs),
        action: :start,
        tenant: org.id
      )

    Ash.update!(r, %{status: :completed, markdown_body: "body"},
      action: :complete,
      tenant: org.id
    )
  end

  describe "maybe_enqueue_delta/1" do
    test "enqueues DeltaWorker when report has a brief_id" do
      org = setup_org()
      brief = create_brief(org)
      report = create_completed_report(org, %{brief_id: brief.id, run_version: 1})

      ResearchWorker.maybe_enqueue_delta(report)

      assert_enqueued(
        worker: DeltaWorker,
        args: %{
          "to_report_id" => report.id,
          "brief_id" => brief.id,
          "organization_id" => org.id
        }
      )
    end

    test "does NOT enqueue DeltaWorker when report has no brief_id (one-off research)" do
      org = setup_org()
      report = create_completed_report(org)

      ResearchWorker.maybe_enqueue_delta(report)

      refute_enqueued(worker: DeltaWorker)
    end
  end
end
