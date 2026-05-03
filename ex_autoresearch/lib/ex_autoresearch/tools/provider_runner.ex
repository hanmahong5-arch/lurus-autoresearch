defmodule ExAutoresearch.Tools.ProviderRunner do
  @moduledoc """
  Generic telemetry-wrapped, primary-with-fallback dispatcher.

  Calls `apply(primary, callback, args)` inside a `:telemetry.span/3` block.
  When the primary fails and `primary != fallback`, retries with `fallback`.
  Both-failed errors are tagged `{:error, {:both_failed, primary: …, fallback: …}}`.

  ## Outcome atoms in telemetry metadata

  | Situation | `:outcome` value |
  |---|---|
  | Primary succeeds | `:primary_success` |
  | Primary fails, fallback succeeds | `:fallback_success` |
  | Primary fails, primary == fallback | `:primary_only_failed` (or override via `opts`) |
  | Both fail | `:both_failed` |

  ## opts

    * `:outcome_names` – `%{primary_only_failed: atom, ...}` — remap any default outcome atom.
    * `:transform_ok` – `(result, outcome_atom, primary_error_or_nil -> result)` — called on
      `{:ok, result}` before returning, letting the caller annotate shape-specific fields
      (e.g. injecting `fallback_used`/`primary_error` into `result.metadata`).
    * `:stop_metadata` – `(map -> map)` — applied to the telemetry stop metadata just before
      emission; use to rename or add keys to satisfy existing telemetry consumers
      (e.g. renaming `fallback_error` to `native_error` for the scraper bridge).
  """

  require Logger

  @doc """
  Run `apply(primary, callback, args)` under telemetry, falling back to `fallback` on error.

  ## Parameters

    * `primary` – module implementing `callback`
    * `fallback` – fallback module; if equal to `primary`, no retry is attempted
    * `callback` – function name (atom) called on both modules
    * `args` – argument list forwarded verbatim
    * `telemetry_event` – base event name, e.g. `[:ex_autoresearch, :scraper, :fetch]`
    * `base_metadata` – map merged into every telemetry stop/exception metadata
    * `opts` – optional keyword list (see module doc)

  Returns `{:ok, result}` or `{:error, reason}`.
  """
  @spec run(module(), module(), atom(), list(), [atom()], map(), keyword()) ::
          {:ok, term()} | {:error, term()}
  def run(primary, fallback, callback, args, telemetry_event, base_metadata, opts \\ []) do
    outcome_names = Keyword.get(opts, :outcome_names, %{})
    transform_ok = Keyword.get(opts, :transform_ok, fn result, _outcome, _err -> result end)
    stop_meta_fn = Keyword.get(opts, :stop_metadata, &Function.identity/1)

    name = fn key -> Map.get(outcome_names, key, key) end
    meta = fn m -> stop_meta_fn.(m) end

    :telemetry.span(telemetry_event, base_metadata, fn ->
      case safe_apply(primary, callback, args) do
        {:ok, result} ->
          outcome = name.(:primary_success)

          {{:ok, transform_ok.(result, outcome, nil)},
           meta.(Map.put(base_metadata, :outcome, outcome))}

        {:error, reason} when primary != fallback ->
          Logger.warning(
            "Provider #{inspect(primary)} failed (#{callback}): #{inspect(reason)}" <>
              " — falling back to #{inspect(fallback)}"
          )

          case safe_apply(fallback, callback, args) do
            {:ok, result} ->
              outcome = name.(:fallback_success)

              {{:ok, transform_ok.(result, outcome, reason)},
               meta.(Map.merge(base_metadata, %{outcome: outcome, primary_error: reason}))}

            {:error, fallback_reason} ->
              outcome = name.(:both_failed)

              Logger.warning(
                "Both #{inspect(primary)} and #{inspect(fallback)} failed" <>
                  " (#{callback}): primary=#{inspect(reason)}" <>
                  " fallback=#{inspect(fallback_reason)}"
              )

              {{:error, {:both_failed, primary: reason, fallback: fallback_reason}},
               meta.(
                 Map.merge(base_metadata, %{
                   outcome: outcome,
                   primary_error: reason,
                   fallback_error: fallback_reason
                 })
               )}
          end

        {:error, reason} ->
          outcome = name.(:primary_only_failed)

          {{:error, reason}, meta.(Map.merge(base_metadata, %{outcome: outcome}))}
      end
    end)
  end

  defp safe_apply(module, callback, args) do
    apply(module, callback, args)
  rescue
    e ->
      Logger.error("Provider #{inspect(module)}.#{callback} crashed: #{Exception.message(e)}")

      {:error, {:exception, Exception.message(e)}}
  end
end
