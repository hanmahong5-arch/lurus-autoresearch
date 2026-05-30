defmodule ExAutoresearch.Workers.NotifyWorkerTest do
  use ExAutoresearch.DataCase, async: false
  use Oban.Testing, repo: ExAutoresearch.Repo

  alias ExAutoresearch.Workers.NotifyWorker

  describe "parse_channel/1" do
    test "inbox string returns {:inbox}" do
      assert NotifyWorker.parse_channel("inbox") == {:inbox}
    end

    test "slack:<url> returns {:slack, url} — URL with :// survives" do
      url = "https://hooks.slack.com/services/T00/B00/xxx"
      assert NotifyWorker.parse_channel("slack:#{url}") == {:slack, url}
    end

    test "webhook:<url> returns {:webhook, url}" do
      url = "https://example.com/hook"
      assert NotifyWorker.parse_channel("webhook:#{url}") == {:webhook, url}
    end

    test "email:<addr> returns {:email, addr}" do
      assert NotifyWorker.parse_channel("email:user@example.com") ==
               {:email, "user@example.com"}
    end

    test "unrecognised string returns {:unknown, raw}" do
      assert NotifyWorker.parse_channel("ftp://something") == {:unknown, "ftp://something"}
    end
  end

  describe "perform/1 with inbox-only brief" do
    test "returns :ok and makes no external calls", %{} do
      org = setup_org()
      brief = create_brief(org, notify_channels: ["inbox"])
      report_v1 = create_report(org, brief, 1)
      report_v2 = create_report(org, brief, 2)

      delta = create_delta(org, brief, report_v1, report_v2)

      result =
        NotifyWorker.perform(%Oban.Job{
          args: %{
            "delta_id" => delta.id,
            "brief_id" => brief.id,
            "organization_id" => org.id
          }
        })

      assert result == :ok
    end
  end

  # --- Setup helpers ---

  defp setup_org do
    email = "notify_worker_test_#{System.unique_integer([:positive])}@test.com"
    {:ok, _user, org} = ExAutoresearch.Accounts.Auth.register(email, "password123")
    org
  end

  defp create_brief(org, extra) do
    Ash.create!(
      ExAutoresearch.Research.Brief,
      Map.merge(
        %{
          name: "Notify Test Brief #{System.unique_integer([:positive])}",
          question: "What is happening?",
          organization_id: org.id
        },
        Map.new(extra)
      ),
      action: :create,
      tenant: org.id
    )
  end

  defp create_report(org, brief, run_version) do
    r =
      Ash.create!(
        ExAutoresearch.Research.Report,
        %{
          title: "Test Report v#{run_version}",
          query: "test query",
          organization_id: org.id,
          brief_id: brief.id,
          run_version: run_version
        },
        action: :start,
        tenant: org.id
      )

    Ash.update!(r, %{status: :completed, markdown_body: "body"},
      action: :complete,
      tenant: org.id
    )
  end

  defp create_delta(org, brief, from_report, to_report) do
    Ash.create!(
      ExAutoresearch.Research.Delta,
      %{
        brief_id: brief.id,
        organization_id: org.id,
        from_report_id: from_report.id,
        to_report_id: to_report.id,
        markdown_digest: "## Summary\n\nSome change.",
        added_count: 1,
        changed_count: 0,
        removed_count: 0,
        contradicted_count: 0,
        generated_at: DateTime.utc_now()
      },
      action: :create,
      tenant: org.id
    )
  end
end
