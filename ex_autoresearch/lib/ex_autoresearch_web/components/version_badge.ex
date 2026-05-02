defmodule ExAutoresearchWeb.Components.VersionBadge do
  @moduledoc """
  Floating "v X.Y.Z" badge + slide-in changelog drawer.

  Mounted from `root.html.heex` so every page (LiveView or controller-rendered)
  shows it. Pure dead-view HTML + a self-contained `<script>` block; no
  LiveView socket dependency, so the badge keeps working even before LV
  connects or if it disconnects.

  Behaviour:
    * First-time visitor (no `localStorage["ex_autoresearch:last_seen_version"]`):
      badge shown, no red dot, no auto-popup. Visit is recorded immediately so
      the user is never spammed with a backlog of past versions.
    * Returning visitor whose stored version differs from
      `Changelog.current_version/0`: red dot + drawer auto-opens once.
    * Closing the drawer (X / "Got it" / ESC / outside-click) writes the
      current version into localStorage, clearing the dot.

  Also offers `<.section />` for embedding the full changelog inside a
  LiveView (e.g. settings page).
  """

  use Phoenix.Component

  alias ExAutoresearch.Changelog

  @doc """
  Floating badge anchored bottom-right + slide-in drawer.

  Drop into `root.html.heex` once. Independent of LiveView.
  """
  attr :id, :string, default: "version-badge"

  def floating(assigns) do
    assigns =
      assigns
      |> assign(:version, Changelog.current_version())
      |> assign(:commit, Changelog.build_commit())
      |> assign(:built_at, Changelog.build_time())
      |> assign(:entries, Changelog.entries())

    ~H"""
    <div
      id={@id}
      data-version-badge
      data-version={@version}
      data-storage-key="ex_autoresearch:last_seen_version"
      data-auto-open-delay="600"
    >
      <button
        type="button"
        id={"#{@id}-trigger"}
        data-version-badge-trigger
        title={"Build #{@commit} · #{@built_at} · click for changelog"}
        aria-label="Show what's new"
        class={[
          "fixed bottom-4 right-4 z-40 inline-flex items-center gap-1.5",
          "px-3 py-1.5 rounded-full",
          "bg-neutral text-neutral-content border border-base-300 shadow-lg",
          "text-xs font-mono tracking-tight",
          "hover:opacity-90 hover:scale-105",
          "transition-all duration-150 cursor-pointer",
          "backdrop-blur-sm"
        ]}
      >
        <span class="opacity-60">v</span>{@version}
        <span
          id={"#{@id}-dot"}
          data-version-badge-dot
          hidden
          aria-hidden="true"
          class="inline-block w-2 h-2 rounded-full bg-error ring-2 ring-base-100 -mt-3 -mr-1"
        />
      </button>

      <div
        id={"#{@id}-backdrop"}
        data-version-badge-backdrop
        hidden
        class="fixed inset-0 z-40 bg-black/40 backdrop-blur-[2px] transition-opacity"
      />

      <aside
        id={"#{@id}-drawer"}
        data-version-badge-drawer
        hidden
        role="dialog"
        aria-modal="true"
        aria-labelledby={"#{@id}-drawer-title"}
        class={[
          "fixed top-0 right-0 z-50 h-full w-full max-w-md",
          "bg-base-100 text-base-content",
          "shadow-2xl border-l border-base-300",
          "flex flex-col",
          "transform transition-transform duration-200 ease-out"
        ]}
      >
        <header class="flex items-start justify-between px-5 py-4 border-b border-base-300">
          <div>
            <h2 id={"#{@id}-drawer-title"} class="text-base font-semibold">What's new</h2>
            <p class="text-xs text-base-content/60 mt-0.5 font-mono">
              v{@version} · {String.slice(@commit, 0, 7)} · {String.slice(@built_at, 0, 10)}
            </p>
          </div>
          <button
            type="button"
            id={"#{@id}-close"}
            data-version-badge-close
            aria-label="Close"
            class="btn btn-ghost btn-sm btn-circle"
          >
            <svg class="w-5 h-5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
              <path
                fill-rule="evenodd"
                d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z"
                clip-rule="evenodd"
              />
            </svg>
          </button>
        </header>

        <div class="flex-1 overflow-y-auto px-5 py-4 space-y-6">
          <%= if @entries == [] do %>
            <p class="text-sm text-base-content/60">
              No changelog entries yet.
            </p>
          <% else %>
            <%= for entry <- @entries do %>
              <article id={"#{@id}-entry-#{entry.anchor}"} class="space-y-2">
                <h3 class="flex items-baseline gap-2 text-sm font-semibold">
                  <span class="font-mono text-primary">
                    v{entry.version}
                  </span>
                  <span class="text-xs text-base-content/60 font-normal">
                    {entry.date}
                  </span>
                </h3>
                <div class="text-sm leading-relaxed space-y-1 [&_h3]:text-[10px] [&_h3]:uppercase [&_h3]:tracking-wider [&_h3]:text-base-content/60 [&_h3]:font-semibold [&_h3]:mt-3 [&_h3]:mb-1 [&_ul]:list-disc [&_ul]:pl-5 [&_ul]:my-1 [&_li]:my-0.5 [&_p]:my-1 [&_code]:text-xs [&_code]:bg-base-200 [&_code]:text-base-content [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_strong]:font-semibold">
                  {Phoenix.HTML.raw(entry.body_html)}
                </div>
              </article>
            <% end %>
          <% end %>
        </div>

        <footer class="flex justify-end gap-2 px-5 py-3 border-t border-base-300 bg-base-200/50">
          <button
            type="button"
            id={"#{@id}-ack"}
            data-version-badge-ack
            class="btn btn-primary btn-sm"
          >
            Got it
          </button>
        </footer>
      </aside>
    </div>

    <script>
      (() => {
        function init() {
          const root = document.querySelector('[data-version-badge]');
          if (!root) return;
          const trigger = root.querySelector('[data-version-badge-trigger]');
          const dot = root.querySelector('[data-version-badge-dot]');
          const backdrop = root.querySelector('[data-version-badge-backdrop]');
          const drawer = root.querySelector('[data-version-badge-drawer]');
          const closeBtn = root.querySelector('[data-version-badge-close]');
          const ackBtn = root.querySelector('[data-version-badge-ack]');
          if (!trigger || !drawer) return;

          const VERSION = root.dataset.version;
          const STORAGE_KEY = root.dataset.storageKey;
          const AUTO_OPEN_DELAY_MS = parseInt(root.dataset.autoOpenDelay || "600", 10);

          // Drawer enters from the right via translate.
          drawer.style.transform = "translateX(100%)";

          let lastSeen = null;
          try { lastSeen = localStorage.getItem(STORAGE_KEY); } catch (_) {}
          const isFirstVisit = lastSeen === null;
          const versionChanged = !isFirstVisit && lastSeen !== VERSION;

          if (isFirstVisit) {
            try { localStorage.setItem(STORAGE_KEY, VERSION); } catch (_) {}
          }
          if (versionChanged) {
            dot.hidden = false;
          }

          let isOpen = false;
          function open() {
            if (isOpen) return;
            isOpen = true;
            backdrop.hidden = false;
            drawer.hidden = false;
            void drawer.offsetWidth;
            drawer.style.transform = "translateX(0%)";
          }
          function markSeen() {
            try { localStorage.setItem(STORAGE_KEY, VERSION); } catch (_) {}
            dot.hidden = true;
          }
          function close() {
            if (!isOpen) return;
            isOpen = false;
            drawer.style.transform = "translateX(100%)";
            backdrop.hidden = true;
            markSeen();
            setTimeout(() => { if (!isOpen) drawer.hidden = true; }, 220);
          }

          trigger.addEventListener("click", () => isOpen ? close() : open());
          closeBtn && closeBtn.addEventListener("click", close);
          ackBtn && ackBtn.addEventListener("click", close);
          backdrop && backdrop.addEventListener("click", close);
          document.addEventListener("keydown", (e) => {
            if (e.key === "Escape" && isOpen) close();
          });

          if (versionChanged) {
            setTimeout(open, AUTO_OPEN_DELAY_MS);
          }
        }

        if (document.readyState === "loading") {
          document.addEventListener("DOMContentLoaded", init);
        } else {
          init();
        }
      })();
    </script>
    """
  end

  @doc """
  Permanent in-page changelog section, e.g. for the Settings page.

  Same data as the floating drawer, no JS, no localStorage.
  """
  attr :id, :string, default: "version-section"

  def section(assigns) do
    assigns =
      assigns
      |> assign(:version, Changelog.current_version())
      |> assign(:commit, Changelog.build_commit())
      |> assign(:built_at, Changelog.build_time())
      |> assign(:entries, Changelog.entries())

    ~H"""
    <section id={@id} class="card bg-base-100 border border-base-300 shadow-sm">
      <div class="card-body space-y-4">
        <header class="flex items-baseline justify-between gap-3 border-b border-base-300 pb-3">
          <div>
            <h2 class="text-lg font-semibold">Version & Changelog</h2>
            <p class="text-xs text-base-content/60 mt-0.5 font-mono">
              running v{@version} · build {String.slice(@commit, 0, 7)} · built {String.slice(
                @built_at,
                0,
                10
              )}
            </p>
          </div>
        </header>

        <%= if @entries == [] do %>
          <p class="text-sm text-base-content/60">No changelog entries yet.</p>
        <% else %>
          <div class="space-y-5">
            <%= for entry <- @entries do %>
              <article id={"#{@id}-entry-#{entry.anchor}"} class="space-y-2">
                <h3 class="flex items-baseline gap-2 text-sm font-semibold">
                  <span class="font-mono text-primary">v{entry.version}</span>
                  <span class="text-xs text-base-content/60 font-normal">{entry.date}</span>
                </h3>
                <div class="text-sm leading-relaxed space-y-1 [&_h3]:text-[10px] [&_h3]:uppercase [&_h3]:tracking-wider [&_h3]:text-base-content/60 [&_h3]:font-semibold [&_h3]:mt-3 [&_h3]:mb-1 [&_ul]:list-disc [&_ul]:pl-5 [&_ul]:my-1 [&_li]:my-0.5 [&_p]:my-1 [&_code]:text-xs [&_code]:bg-base-200 [&_code]:text-base-content [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_strong]:font-semibold">
                  {Phoenix.HTML.raw(entry.body_html)}
                </div>
              </article>
            <% end %>
          </div>
        <% end %>
      </div>
    </section>
    """
  end
end
