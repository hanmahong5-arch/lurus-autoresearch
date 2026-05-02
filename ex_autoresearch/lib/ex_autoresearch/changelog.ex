defmodule ExAutoresearch.Changelog do
  @moduledoc """
  Compile-time view of `CHANGELOG.md` plus build provenance.

  The file is read once at compile time via `@external_resource`, parsed into
  a list of version entries, and exposed through pure functions. Zero runtime
  IO; the BEAM module is the source of truth at runtime.

  Each `entry/0` looks like:

      %{
        version: "0.2.0",
        date: "2026-05-02",
        body_md: "### Added\\n- ...",
        body_html: "<h3>Added</h3>...",
        anchor: "v0-2-0"
      }

  `current_version/0` is read from `mix.exs` (`:ex_autoresearch` app spec) so
  the CHANGELOG.md heading and the binary's version stay in sync — a
  compile-time `IO.warn/1` fires if they drift.
  """

  @changelog_path Path.join([__DIR__, "..", "..", "CHANGELOG.md"])
  @external_resource @changelog_path

  @raw File.read!(@changelog_path)

  # --- Compile-time parsing ---------------------------------------------------

  # Match level-2 version headings: `## [0.2.0] — 2026-05-02`
  # (em-dash, en-dash, or hyphen all accepted).
  @heading_re ~r/^##\s*\[(?<version>[\w.\-+]+)\]\s*[—–\-]\s*(?<date>\d{4}-\d{2}-\d{2})\s*$/mu

  @offsets Regex.scan(@heading_re, @raw, return: :index)
           |> Enum.map(fn [{full_start, full_len} | _] -> {full_start, full_len} end)

  @named Regex.scan(@heading_re, @raw, capture: :all_names)
         |> Enum.map(fn [date, version] -> {version, date} end)

  @offsets_with_meta Enum.zip(@offsets, @named)

  @parsed_entries (
                    case @offsets_with_meta do
                      [] ->
                        []

                      list ->
                        list
                        |> Enum.with_index()
                        |> Enum.map(fn {{{start, len}, {version, date}}, idx} ->
                          body_start = start + len

                          body_end =
                            case Enum.at(list, idx + 1) do
                              nil -> byte_size(@raw)
                              {{next_start, _}, _} -> next_start
                            end

                          body =
                            @raw
                            |> binary_part(body_start, body_end - body_start)
                            |> String.trim()

                          {version, date, body}
                        end)
                    end
                  )

  @entries Enum.map(@parsed_entries, fn {version, date, body_md} ->
             body_html =
               try do
                 case MDEx.to_html(body_md) do
                   {:ok, html} -> html
                   {:error, _} -> "<pre>" <> Plug.HTML.html_escape(body_md) <> "</pre>"
                 end
               rescue
                 _ -> "<pre>" <> Plug.HTML.html_escape(body_md) <> "</pre>"
               end

             %{
               version: version,
               date: date,
               body_md: body_md,
               body_html: body_html,
               anchor: "v" <> String.replace(version, ".", "-")
             }
           end)

  # Build provenance — captured at compile time so prod images carry it.
  # Falls back to "unknown" if git isn't on PATH or the working tree isn't a
  # repo (e.g. release tarball builds).
  @build_commit (try do
                   case System.cmd("git", ["rev-parse", "--short", "HEAD"],
                          stderr_to_stdout: true,
                          cd: Path.dirname(@changelog_path)
                        ) do
                     {sha, 0} -> String.trim(sha)
                     _ -> "unknown"
                   end
                 rescue
                   _ -> "unknown"
                 end)

  @build_time DateTime.utc_now() |> DateTime.truncate(:second) |> DateTime.to_iso8601()

  # Compile-time sanity guard: top of CHANGELOG.md must match mix.exs.
  if @entries != [] do
    top_version = hd(@entries).version
    mix_version = Mix.Project.config()[:version]

    if top_version != mix_version do
      IO.warn(
        "CHANGELOG.md top entry is v#{top_version} but mix.exs says v#{mix_version}. " <>
          "Update CHANGELOG.md before tagging a release."
      )
    end
  end

  # --- Public API -------------------------------------------------------------

  @doc "Application version, read from mix.exs."
  @spec current_version() :: String.t()
  def current_version do
    Application.spec(:ex_autoresearch, :vsn) |> to_string()
  end

  @doc "Short git commit SHA captured at compile time, or `\"unknown\"`."
  @spec build_commit() :: String.t()
  def build_commit, do: @build_commit

  @doc "ISO-8601 UTC timestamp of when the module was compiled."
  @spec build_time() :: String.t()
  def build_time, do: @build_time

  @doc """
  All parsed changelog entries, newest first. Returns `[]` if `CHANGELOG.md`
  has no version blocks (defensive — should never happen in practice).
  """
  @spec entries() :: [
          %{
            version: String.t(),
            date: String.t(),
            body_md: String.t(),
            body_html: String.t(),
            anchor: String.t()
          }
        ]
  def entries, do: @entries

  @doc "Most recent changelog entry, or `nil` if the file is empty."
  @spec latest_entry() ::
          nil
          | %{
              version: String.t(),
              date: String.t(),
              body_md: String.t(),
              body_html: String.t(),
              anchor: String.t()
            }
  def latest_entry, do: List.first(@entries)
end
