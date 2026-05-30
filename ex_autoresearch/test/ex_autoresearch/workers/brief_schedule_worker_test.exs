defmodule ExAutoresearch.Workers.BriefScheduleWorkerTest do
  use ExAutoresearch.DataCase
  use Oban.Testing, repo: ExAutoresearch.Repo

  alias ExAutoresearch.Workers.BriefScheduleWorker
  alias ExAutoresearch.Workers.ResearchWorker
  alias ExAutoresearch.Research.Brief

  setup do
    suffix = System.unique_integer([:positive])
    email = "worker-test-#{suffix}@example.test"

    {:ok, _user, org} =
      ExAutoresearch.Accounts.Auth.register(email, "test-password-#{suffix}", "Worker Test")

    {:ok, org: org}
  end

  defp create_brief(org, attrs) do
    {next_run_at, attrs} = Map.pop(attrs, :next_run_at)

    defaults = %{
      name: "Test Brief",
      question: "What is happening in AI?",
      organization_id: org.id,
      category: :market,
      model: "claude-sonnet-4",
      max_depth: 2,
      max_sources: 10
    }

    params = Map.merge(defaults, attrs)
    brief = Ash.create!(Brief, params, action: :create, tenant: org.id)

    if next_run_at do
      Ash.update!(brief, %{next_run_at: next_run_at}, action: :mark_ran, tenant: org.id)
    else
      brief
    end
  end

  describe "perform/1" do
    test "enqueues a ResearchWorker job for an enabled, due daily brief", %{org: org} do
      one_hour_ago = DateTime.add(DateTime.utc_now(), -3600, :second)

      brief =
        create_brief(org, %{
          enabled: true,
          cadence: :daily,
          next_run_at: one_hour_ago
        })

      :ok = BriefScheduleWorker.perform(%Oban.Job{args: %{}})

      assert_enqueued(worker: ResearchWorker, args: %{"brief_id" => brief.id})

      updated = Ash.get!(Brief, brief.id, tenant: org.id)
      assert updated.last_run_at != nil

      assert updated.next_run_at != nil

      expected_next = DateTime.add(DateTime.utc_now(), 86_400, :second)
      diff = DateTime.diff(updated.next_run_at, expected_next, :second)
      assert abs(diff) < 60, "next_run_at should be ~1 day from now"
    end

    test "does NOT enqueue a job for a :manual cadence brief", %{org: org} do
      create_brief(org, %{
        enabled: true,
        cadence: :manual,
        next_run_at: DateTime.add(DateTime.utc_now(), -3600, :second)
      })

      :ok = BriefScheduleWorker.perform(%Oban.Job{args: %{}})

      refute_enqueued(worker: ResearchWorker)
    end

    test "does NOT enqueue a job for a disabled brief", %{org: org} do
      create_brief(org, %{
        enabled: false,
        cadence: :daily,
        next_run_at: DateTime.add(DateTime.utc_now(), -3600, :second)
      })

      :ok = BriefScheduleWorker.perform(%Oban.Job{args: %{}})

      refute_enqueued(worker: ResearchWorker)
    end

    test "does NOT enqueue a job for a brief with future next_run_at", %{org: org} do
      create_brief(org, %{
        enabled: true,
        cadence: :daily,
        next_run_at: DateTime.add(DateTime.utc_now(), 3600, :second)
      })

      :ok = BriefScheduleWorker.perform(%Oban.Job{args: %{}})

      refute_enqueued(worker: ResearchWorker)
    end
  end
end
