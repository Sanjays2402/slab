<script lang="ts">
  // RecentInstallsDrawer — v3.39 Slice 57 + Slice 62 + Slice 77.
  //
  // 460px right-side slide-in drawer surfacing the corpus-wide
  // marketplace install log: every install / update / uninstall /
  // failure across all plugins, newest first. Triggered from the
  // PluginsPanel toolbar "History" button which is gated on the
  // log having at least one event (no nag UI on a brand-new install
  // with empty history).
  //
  // Slice 62 (Install-log export round-13) adds an "Export…" menu in
  // the footer with CSV and JSON entries that respect the currently
  // selected window (7d / 30d / All) so what you see is what gets
  // exported. The export goes through a native save-as dialog; the
  // Tauri layer owns the actual file write.
  //
  // Slice 77 (Install-log filter bar) adds a filter strip BELOW the
  // window strip with action chips ("Installs / Updates / Uninstalls /
  // Failures") that act as a multi-select toggle group AND a plugin
  // search input with autocomplete from the recent-activity history.
  // The filter runs server-side via slab_marketplace_install_log_list_filtered
  // (slice 73 reader, slice 74 command) so the result reflects the
  // full log, not just the most-recent 100 the drawer initially fetched
  // for the "All" window.
  //
  // The drawer is otherwise purely presentational — the parent owns
  // the open state, toast wiring, and the post-prune refresh.

  import { onMount } from "svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import {
    ALL_INSTALL_ACTIONS,
    describeActionSet,
    exportInstallLogCsv,
    exportInstallLogJson,
    formatBytes,
    formatInstallEventTime,
    formatLastAutoPrune,
    formatLogSpan,
    formatNextAutoPrune,
    getInstallLogRetentionPolicy,
    getPluginInstallHistogram,
    installEventGlyph,
    installLogSummary,
    listInstallEventsFiltered,
    listRecentInstallEvents,
    pluginQueryActiveLabel,
    pruneInstallLog,
    recentInstallPluginIds,
    runInstallLogAutoPrune,
    setInstallLogRetentionDays,
    suggestInstallLogExportFilename,
    summarizeHistogram,
    type InstallEvent,
    type InstallEventQuery,
    type InstallLogExportFilter,
    type InstallLogRetentionPolicy,
    type InstallLogSummary,
    type PluginHistogramResult,
  } from "$lib/marketplace";

  type Props = {
    onClose: () => void;
    /**
     * Called after a successful prune so the parent (PluginsPanel) can
     * refresh its toolbar badge gating. Optional — drawer works
     * standalone if the parent doesn't care.
     */
    onPruned?: (rowsRemoved: number) => void;
  };

  let { onClose, onPruned }: Props = $props();

  let events = $state<InstallEvent[]>([]);
  let summary = $state<InstallLogSummary>({
    total_events: 0,
    distinct_plugins: 0,
    oldest_occurred_at: null,
  });
  let loading = $state(false);
  let err = $state<string | null>(null);

  /** "Last 7d" / "Last 30d" / "All" filter over the loaded events. */
  let windowChoice = $state<"7d" | "30d" | "all">("all");
  /** The pruning is gated behind this small confirm dialog. */
  let confirmingPrune = $state(false);
  let pruning = $state(false);
  /** Open state for the footer Export… popover (Slice 62). */
  let exportMenuOpen = $state(false);
  /** True while a save-as dialog or backend write is in flight. */
  let exporting = $state(false);
  /** Slim 4-second toast for export success. */
  let exportToast = $state<string | null>(null);

  // ─── Action + plugin-id filter state (Slice 77) ───────────────────
  //
  // The action chips are a multi-select toggle group. An empty set
  // means "all actions" — same semantics as the backend's None.
  // The plugin substring search is debounced so a fast typist
  // doesn't fire one IPC per keystroke. The autocomplete suggestion
  // list (recently-active plugin ids) is fetched once on mount and
  // re-fetched after each prune so the source of suggestions stays
  // honest.

  /** Selected action set; empty == "all actions". */
  let actionFilter = $state<Set<InstallEvent["action"]>>(new Set());
  /** Raw text in the plugin search input — bound to the field. */
  let pluginQueryDraft = $state<string>("");
  /** Debounced copy that actually feeds the backend query. */
  let pluginQueryActive = $state<string>("");
  /** Recently-active plugin ids for autocomplete; loaded once on mount. */
  let pluginSuggestions = $state<string[]>([]);
  /** Open state of the autocomplete dropdown. */
  let suggestOpen = $state<boolean>(false);
  /** Debounce timer for the plugin-search input. */
  let queryDebounce: ReturnType<typeof setTimeout> | null = null;
  /** True when ANY of the filter axes (action / plugin) narrow. */
  let filterNarrowing = $derived.by<boolean>(() => {
    const fullActionSet =
      actionFilter.size === ALL_INSTALL_ACTIONS.length &&
      ALL_INSTALL_ACTIONS.every((a) => actionFilter.has(a));
    const actionNarrows = actionFilter.size > 0 && !fullActionSet;
    const pluginNarrows = pluginQueryActive.trim() !== "";
    return actionNarrows || pluginNarrows;
  });

  // ─── Retention policy section (Slice 67) ──────────────────────────
  //
  // Collapsible section between the window strip and the event list
  // surfacing the user-modifiable retain_days + a "Run auto-prune
  // now" button. The section is collapsed by default so the timeline
  // remains the drawer's primary content; users who want to tune the
  // retention click "Retention" to expand. The state of the
  // collapse persists for the lifetime of the drawer instance (no
  // cross-session persistence — opening the drawer fresh starts
  // collapsed, matching how the export popover behaves).

  /** Retention section expand/collapse. Defaults closed. */
  let retentionOpen = $state(false);
  /** Loaded policy. Null until the first fetch resolves. */
  let policy = $state<InstallLogRetentionPolicy | null>(null);
  /** Local edit buffer for retain_days. Initialised from policy on load. */
  let retainDaysDraft = $state<number>(365);
  /** True while a setRetentionDays or auto_prune call is in flight. */
  let retentionBusy = $state(false);
  /** Slim auto-clear toast for retention-section feedback. */
  let retentionToast = $state<string | null>(null);

  // ─── Top plugins histogram (Slice 87) ─────────────────────────────
  //
  // Collapsible "Top plugins" section under the retention block
  // showing per-plugin activity bars within the currently-selected
  // window. Same window axis as the timeline above; auto-refreshes
  // when the window changes. Defaults closed so the timeline stays
  // the primary content; cheap fetch (one GROUP BY scan + sort) so
  // the open/close is snappy with no spinner thrash.

  /** Section expand/collapse. Defaults closed. */
  let topPluginsOpen = $state(false);
  /** Loaded histogram. Null until the first fetch resolves. */
  let histogram = $state<PluginHistogramResult | null>(null);
  /** True while a histogram fetch is in flight. */
  let histogramLoading = $state(false);
  /** Last error from the histogram loader, surfaced inline. */
  let histogramError = $state<string | null>(null);
  /** How many plugins to load — matches the server default. */
  let histogramLimit = $state<number>(25);

  /** Dirty when the draft diverges from the persisted policy. */
  let retentionDirty = $derived.by<boolean>(() => {
    if (!policy) return false;
    return Math.max(policy.min_retain_days, Math.trunc(retainDaysDraft)) !==
      policy.retain_days;
  });

  /** "Last auto-prune: 2h ago" / "Never auto-pruned" subtitle. */
  let lastAutoPruneText = $derived.by<string>(() =>
    formatLastAutoPrune(policy?.last_auto_prune_at ?? null),
  );

  /** "Next auto-prune in 4h 12m" / "Due now" subtitle. */
  let nextAutoPruneText = $derived.by<string>(() => {
    if (!policy?.last_auto_prune_at) return "Next auto-prune: due on next launch";
    const nextDue = policy.last_auto_prune_at + policy.auto_prune_interval_secs;
    return formatNextAutoPrune(nextDue);
  });

  /**
   * Filtered events. With slice 77 the action + plugin axes are
   * applied SERVER-SIDE by reload (the events array IS the filtered
   * set); the window axis is still applied client-side from the
   * loaded buffer so toggling between 7d / 30d / All is instant.
   */
  let filteredEvents = $derived.by<InstallEvent[]>(() => {
    if (windowChoice === "all") return events;
    const nowSec = Math.floor(Date.now() / 1000);
    const cutoff = nowSec - (windowChoice === "7d" ? 7 : 30) * 86_400;
    return events.filter((ev) => ev.occurred_at >= cutoff);
  });

  /**
   * The since-unix cutoff implied by the current window choice — fed
   * into the export filter so a "Last 7d" export ships only the 7d
   * window rather than the whole loaded buffer.
   */
  let windowSinceUnix = $derived.by<number | null>(() => {
    if (windowChoice === "all") return null;
    const nowSec = Math.floor(Date.now() / 1000);
    return nowSec - (windowChoice === "7d" ? 7 : 30) * 86_400;
  });

  /**
   * "N events across X days" subtitle from the summary, plus the
   * filtered-count addendum when the window narrows the visible set
   * OR when the action/plugin filter is active.
   */
  let subtitleText = $derived.by<string>(() => {
    const base = formatLogSpan(summary);
    const filterLabel = pluginQueryActiveLabel({
      since_unix: windowSinceUnix,
      actions:
        actionFilter.size > 0 ? [...actionFilter] : null,
      plugin_id_substr: pluginQueryActive,
    });
    if (filterLabel) {
      // "23 events across 12d · showing 4 (3 filters active)"
      return `${base} · showing ${filteredEvents.length} (${filterLabel})`;
    }
    if (windowChoice === "all" || filteredEvents.length === events.length) {
      return base;
    }
    return `${base} · showing ${filteredEvents.length} in window`;
  });

  async function load(): Promise<void> {
    loading = true;
    err = null;
    try {
      const useFiltered = filterNarrowing;
      const filterQuery: InstallEventQuery = {
        // Window axis is applied client-side too; keeping it
        // server-side as well so a filtered + windowed query
        // doesn't accidentally hit the row cap on the All window
        // and miss recent rows.
        since_unix: windowSinceUnix,
        actions:
          actionFilter.size > 0 ? [...actionFilter] : null,
        plugin_id_substr: pluginQueryActive || null,
      };
      const [eventsResp, sm, pol, ids] = await Promise.all([
        useFiltered
          ? listInstallEventsFiltered(filterQuery).then((r) => r.events)
          : listRecentInstallEvents(100),
        installLogSummary(),
        getInstallLogRetentionPolicy(),
        // Don't refetch suggestions if we already have them — the
        // recent-active set is stable across action-filter toggles.
        pluginSuggestions.length === 0
          ? recentInstallPluginIds(25)
          : Promise.resolve(pluginSuggestions),
      ]);
      events = eventsResp;
      summary = sm;
      policy = pol;
      retainDaysDraft = pol.retain_days;
      pluginSuggestions = ids;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    return () => {
      if (queryDebounce) clearTimeout(queryDebounce);
    };
  });

  // ─── Slice 87: top-plugins histogram loader ───────────────────────
  //
  // Fetched on demand when the user expands the section; refreshed
  // automatically when the window choice changes (since the
  // histogram's since_unix is bound to it). Window changes that
  // happen while the section is closed don't trigger a fetch — the
  // next open will use the current window.

  async function refreshHistogram(): Promise<void> {
    if (!topPluginsOpen) return;
    histogramLoading = true;
    histogramError = null;
    try {
      histogram = await getPluginInstallHistogram({
        sinceUnix: windowSinceUnix,
        limit: histogramLimit,
      });
    } catch (e) {
      histogramError = e instanceof Error ? e.message : String(e);
      histogram = null;
    } finally {
      histogramLoading = false;
    }
  }

  function toggleTopPlugins() {
    topPluginsOpen = !topPluginsOpen;
    if (topPluginsOpen && histogram === null) {
      void refreshHistogram();
    }
  }

  // Refresh the histogram when the window choice changes (if open).
  let lastHistogramKey = $state<string>("");
  $effect(() => {
    const key = `${topPluginsOpen ? 1 : 0}|${windowSinceUnix ?? "all"}|${histogramLimit}`;
    if (key === lastHistogramKey) return;
    lastHistogramKey = key;
    if (topPluginsOpen) void refreshHistogram();
  });

  // ─── Slice 77: filter-driven reload ──────────────────────────────
  //
  // Reload events whenever the action set or the debounced plugin
  // query changes. Window changes don't trigger a reload — the
  // client-side filter is instant from the loaded buffer, AND a
  // server-side window refetch would lose context if the user is
  // mid-typing in the plugin search.

  let lastReloadKey = $state<string>("");
  $effect(() => {
    // Stringify the filter-narrowing inputs so we only refetch when
    // they actually change (not on every reactive ripple).
    const key = [...actionFilter].sort().join(",") + "|" + pluginQueryActive;
    if (key !== lastReloadKey) {
      lastReloadKey = key;
      // Skip the very first effect run — load() in onMount already
      // ran with the default empty filter.
      if (events.length === 0 && !loading && summary.total_events === 0) return;
      void load();
    }
  });

  /** Toggle a single action chip on/off. */
  function toggleAction(a: InstallEvent["action"]) {
    const next = new Set(actionFilter);
    if (next.has(a)) {
      next.delete(a);
    } else {
      next.add(a);
    }
    actionFilter = next;
  }

  /** Clear all filter axes at once. Wired to the "Clear filters"
   *  affordance in the filter strip when at least one axis narrows. */
  function clearFilters() {
    actionFilter = new Set();
    pluginQueryDraft = "";
    pluginQueryActive = "";
    if (queryDebounce) {
      clearTimeout(queryDebounce);
      queryDebounce = null;
    }
  }

  /** Debounced commit of the plugin-search draft into the active query. */
  function onPluginQueryInput(value: string) {
    pluginQueryDraft = value;
    if (queryDebounce) clearTimeout(queryDebounce);
    queryDebounce = setTimeout(() => {
      pluginQueryActive = pluginQueryDraft.trim();
    }, 220);
  }

  /** Filtered autocomplete: recent ids that match the current draft. */
  let suggestionMatches = $derived.by<string[]>(() => {
    const q = pluginQueryDraft.trim().toLowerCase();
    if (q === "") return pluginSuggestions.slice(0, 8);
    return pluginSuggestions
      .filter((id) => id.toLowerCase().includes(q))
      .slice(0, 8);
  });

  /** Apply a suggestion as the active query. */
  function applySuggestion(id: string) {
    pluginQueryDraft = id;
    pluginQueryActive = id;
    suggestOpen = false;
    if (queryDebounce) {
      clearTimeout(queryDebounce);
      queryDebounce = null;
    }
  }

  // ─── Slice 92 — histogram row click-to-filter ────────────────────────
  //
  // Click a "Top plugins" row to pivot the timeline below from
  // "everything in window" to "just this plugin's events" in one
  // click. Reuses the same plugin_id_substr filter axis the search
  // input feeds, so the timeline + the filter chip strip + the
  // export filenames all carry the narrow consistently — there's
  // ONE filter axis, the histogram row just populates it.
  //
  // Click semantics:
  //  - Row whose plugin_id != current filter → apply as filter.
  //  - Row whose plugin_id == current filter → CLEAR (Notion-style
  //    toggle, matches the slice 86 coverage-row open/close pattern).
  //
  // This makes the histogram bidirectional: it's both a view AND a
  // navigation surface. Without the toggle-off, the only way to
  // clear after a row click would be the search input — but the
  // user just clicked a bar, not the search input, so the natural
  // "undo" is to click the same bar again.

  function onHistogramRowClick(pluginId: string): void {
    if (pluginQueryActive === pluginId) {
      // Toggle off — clear the plugin axis. We don't also clear the
      // action axis: those are independent narrows, and clearing
      // both would feel surprising (the user only clicked one).
      pluginQueryDraft = "";
      pluginQueryActive = "";
      if (queryDebounce) {
        clearTimeout(queryDebounce);
        queryDebounce = null;
      }
      return;
    }
    applySuggestion(pluginId);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (suggestOpen) {
        suggestOpen = false;
      } else if (exportMenuOpen) {
        exportMenuOpen = false;
      } else if (confirmingPrune) {
        confirmingPrune = false;
      } else if (retentionOpen) {
        retentionOpen = false;
      } else if (filterNarrowing) {
        clearFilters();
      } else {
        onClose();
      }
    }
  }

  async function runPrune(retainDays: number): Promise<void> {
    pruning = true;
    try {
      const removed = await pruneInstallLog(retainDays);
      confirmingPrune = false;
      await load();
      onPruned?.(removed);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      pruning = false;
    }
  }

  function flashToast(msg: string): void {
    exportToast = msg;
    window.setTimeout(() => {
      exportToast = null;
    }, 4000);
  }

  function flashRetentionToast(msg: string): void {
    retentionToast = msg;
    window.setTimeout(() => {
      retentionToast = null;
    }, 4000);
  }

  /** Persist the draft retain_days. No-op when not dirty. The
   *  storage clamp is reflected back into the draft so a user
   *  typing 0 sees the field correct to 1 inline. */
  async function saveRetentionDays(): Promise<void> {
    if (!policy || !retentionDirty) return;
    retentionBusy = true;
    try {
      const stored = await setInstallLogRetentionDays(
        Math.trunc(retainDaysDraft),
      );
      // Refresh the policy from the source — set returns the
      // clamped retain_days only; last_auto_prune_at stays as it was.
      policy = { ...policy, retain_days: stored };
      retainDaysDraft = stored;
      const dayWord = stored === 1 ? "day" : "days";
      flashRetentionToast(`Retention set to ${stored} ${dayWord}`);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      retentionBusy = false;
    }
  }

  /** Drop the draft back to the persisted value (Cancel chip). */
  function resetRetentionDraft(): void {
    if (policy) retainDaysDraft = policy.retain_days;
  }

  /** Trigger the auto-prune. `force = true` skips the 24h debounce.
   *  The outcome discriminator drives the toast copy: "pruned N
   *  events" vs. "next due in X". After a prune we refresh the
   *  event list + policy + summary so the drawer reflects the
   *  changed log. */
  async function runAutoPruneNow(force = false): Promise<void> {
    retentionBusy = true;
    try {
      const outcome = await runInstallLogAutoPrune(force);
      if (outcome.outcome === "pruned") {
        const word = outcome.rows_removed === 1 ? "event" : "events";
        flashRetentionToast(
          outcome.rows_removed === 0
            ? "Auto-prune ran — nothing to remove."
            : `Auto-pruned ${outcome.rows_removed} ${word} older than ${outcome.retain_days}d.`,
        );
        // The prune changed the log → refresh everything that depends on it.
        await load();
        onPruned?.(outcome.rows_removed);
      } else {
        // skipped: debounce window not elapsed.
        flashRetentionToast(formatNextAutoPrune(outcome.next_due_unix));
      }
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      retentionBusy = false;
    }
  }

  async function runExport(kind: "csv" | "json"): Promise<void> {
    exportMenuOpen = false;
    exporting = true;
    try {
      const filter: InstallLogExportFilter = {
        since_unix: windowSinceUnix,
      };
      const defaultPath = suggestInstallLogExportFilename(filter, kind);
      const target = await saveDialog({
        defaultPath,
        filters: [
          kind === "csv"
            ? { name: "CSV", extensions: ["csv"] }
            : { name: "JSON", extensions: ["json"] },
        ],
      });
      if (!target) return; // user cancelled
      const bytes =
        kind === "csv"
          ? await exportInstallLogCsv(target, filter)
          : await exportInstallLogJson(target, filter);
      const count = filteredEvents.length;
      flashToast(
        `Exported ${count} event${count === 1 ? "" : "s"} (${formatBytes(bytes)})`,
      );
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      exporting = false;
    }
  }

  /**
   * Dismiss the export popover on outside click — matches the
   * Notion / Linear convention. The check uses .closest() on the
   * anchor wrapper so clicks INSIDE the menu (its buttons) don't
   * dismiss it before the handler fires.
   */
  function onWindowClick(e: MouseEvent): void {
    if (!exportMenuOpen) return;
    const target = e.target as HTMLElement | null;
    if (target && !target.closest(".export-anchor")) {
      exportMenuOpen = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} onclick={onWindowClick} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="recent-installs-title"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }}
  tabindex="-1"
>
  <aside class="drawer" role="document">
    <header>
      <div class="title-block">
        <h1 id="recent-installs-title">Recent installs</h1>
        <p class="sub">{subtitleText}</p>
      </div>
      <button type="button" class="icon-btn" aria-label="Close" onclick={onClose} disabled={pruning}
        >✕</button
      >
    </header>

    <div class="window-strip seg" role="tablist" aria-label="Time window">
      {#each [
        { id: "7d" as const, label: "Last 7d" },
        { id: "30d" as const, label: "Last 30d" },
        { id: "all" as const, label: "All" },
      ] as opt (opt.id)}
        <button
          type="button"
          role="tab"
          aria-selected={windowChoice === opt.id}
          class:active={windowChoice === opt.id}
          onclick={() => (windowChoice = opt.id)}>{opt.label}</button
        >
      {/each}
    </div>

    <section class="filter-strip" aria-label="Filter install log">
      <div class="action-chips" role="group" aria-label="Filter by action">
        {#each ALL_INSTALL_ACTIONS as a (a)}
          {@const chipLabel =
            a === "install"
              ? "Installs"
              : a === "update"
                ? "Updates"
                : a === "uninstall"
                  ? "Uninstalls"
                  : "Failures"}
          <button
            type="button"
            class="chip"
            class:on={actionFilter.has(a)}
            data-action={a}
            aria-pressed={actionFilter.has(a)}
            onclick={() => toggleAction(a)}
            title={actionFilter.has(a)
              ? `Hide ${chipLabel.toLowerCase()}`
              : `Show only ${chipLabel.toLowerCase()} (combine with others to widen)`}
          >
            <span class="chip-glyph" aria-hidden="true">{installEventGlyph(a)}</span>
            <span class="chip-label">{chipLabel}</span>
          </button>
        {/each}
      </div>

      <div class="plugin-search">
        <span class="search-glyph" aria-hidden="true">⌕</span>
        <input
          type="text"
          role="combobox"
          class="search-input"
          placeholder="Filter by plugin id…"
          value={pluginQueryDraft}
          oninput={(e) => onPluginQueryInput(e.currentTarget.value)}
          onfocus={() => (suggestOpen = true)}
          onblur={() => {
            // 120ms delay so a click on the suggestion below resolves
            // before blur dismisses the dropdown.
            setTimeout(() => (suggestOpen = false), 120);
          }}
          onkeydown={(e) => {
            if (e.key === "Escape") {
              if (suggestOpen) {
                suggestOpen = false;
              } else if (pluginQueryDraft !== "") {
                onPluginQueryInput("");
              }
            }
            if (e.key === "Enter" && suggestionMatches.length === 1) {
              applySuggestion(suggestionMatches[0]);
            }
          }}
          aria-label="Filter events by plugin id (case-insensitive substring)"
          aria-autocomplete="list"
          aria-controls="plugin-suggest-list"
          aria-expanded={suggestOpen &&
            (suggestionMatches.length > 0 || pluginQueryDraft.trim() !== "")}
        />
        {#if pluginQueryDraft !== ""}
          <button
            type="button"
            class="search-clear"
            aria-label="Clear plugin filter"
            onclick={() => onPluginQueryInput("")}>✕</button
          >
        {/if}
        {#if suggestOpen && suggestionMatches.length > 0}
          <ul class="suggest-list" id="plugin-suggest-list" role="listbox">
            {#each suggestionMatches as id (id)}
              <li>
                <button
                  type="button"
                  class="suggest-item"
                  role="option"
                  aria-selected={pluginQueryActive === id}
                  onmousedown={(e) => {
                    // Use mousedown not click — blur fires before click,
                    // and the 120ms delay we already have on blur is
                    // belt-and-suspenders. mousedown commits immediately.
                    e.preventDefault();
                    applySuggestion(id);
                  }}
                >
                  <span class="suggest-id">{id}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      {#if filterNarrowing}
        <div class="filter-summary">
          <span class="filter-desc"
            >{describeActionSet([...actionFilter])}{pluginQueryActive
              ? ` matching "${pluginQueryActive}"`
              : ""}</span
          >
          <button
            type="button"
            class="ghost mini"
            onclick={clearFilters}
            title="Clear all filters"
          >
            Clear filters
          </button>
        </div>
      {/if}
    </section>

    {#if policy}
      <section class="retention-block" aria-labelledby="retention-heading">
        <button
          type="button"
          class="retention-toggle"
          aria-expanded={retentionOpen}
          aria-controls="retention-body"
          onclick={() => (retentionOpen = !retentionOpen)}
        >
          <span class="retention-chevron" aria-hidden="true">
            {retentionOpen ? "▾" : "▸"}
          </span>
          <span class="retention-label" id="retention-heading">Retention</span>
          <span class="retention-meta">
            Keep {policy.retain_days}d · {lastAutoPruneText}
          </span>
        </button>
        {#if retentionOpen}
          <div class="retention-body" id="retention-body">
            <div class="retention-row">
              <label class="retention-field">
                <span class="field-label">Keep events for</span>
                <span class="field-input">
                  <input
                    type="number"
                    min={policy.min_retain_days}
                    max="3650"
                    step="1"
                    bind:value={retainDaysDraft}
                    disabled={retentionBusy}
                    aria-label="Retention window in days"
                  />
                  <span class="field-unit">days</span>
                </span>
              </label>
              <div class="retention-actions">
                {#if retentionDirty}
                  <button
                    type="button"
                    class="ghost mini"
                    onclick={resetRetentionDraft}
                    disabled={retentionBusy}>Reset</button
                  >
                  <button
                    type="button"
                    class="primary mini"
                    onclick={() => void saveRetentionDays()}
                    disabled={retentionBusy}
                  >
                    {retentionBusy ? "Saving…" : "Save"}
                  </button>
                {/if}
              </div>
            </div>
            <p class="retention-sub">
              Default {policy.default_retain_days}d · floor {policy.min_retain_days}d.
              Older events auto-prune on app launch (max once per 24h).
            </p>
            <div class="retention-row">
              <span class="retention-next">{nextAutoPruneText}</span>
              <button
                type="button"
                class="ghost mini"
                onclick={() => void runAutoPruneNow(true)}
                disabled={retentionBusy || summary.total_events === 0}
                title="Force a prune now, ignoring the 24h debounce"
              >
                {retentionBusy ? "Working…" : "Run now"}
              </button>
            </div>
            {#if retentionToast}
              <p class="retention-toast" role="status">{retentionToast}</p>
            {/if}
          </div>
        {/if}
      </section>
    {/if}

    <section class="top-plugins-block" aria-labelledby="top-plugins-heading">
      <button
        type="button"
        class="top-plugins-toggle"
        aria-expanded={topPluginsOpen}
        aria-controls="top-plugins-body"
        onclick={toggleTopPlugins}
      >
        <span class="top-plugins-chevron" aria-hidden="true">
          {topPluginsOpen ? "▾" : "▸"}
        </span>
        <span class="top-plugins-label" id="top-plugins-heading">Top plugins</span>
        <span class="top-plugins-meta">
          {#if histogram}
            {summarizeHistogram(histogram)} · {windowChoice}
          {:else if histogramLoading}
            Loading…
          {:else}
            Click to expand
          {/if}
        </span>
      </button>
      {#if topPluginsOpen}
        <div class="top-plugins-body" id="top-plugins-body">
          {#if histogramError}
            <p class="top-plugins-error" role="alert">
              Could not load histogram: {histogramError}
            </p>
          {:else if histogramLoading && !histogram}
            <p class="top-plugins-loading">Aggregating plugin activity…</p>
          {:else if histogram}
            {#if histogram.rows.length === 0}
              <p class="top-plugins-empty">
                No plugin activity in the last {windowChoice}. Try widening the window
                or installing a plugin from the Marketplace tab.
              </p>
            {:else}
              {@const maxTotal = histogram.rows[0]?.total ?? 1}
              <ul class="top-plugins-list" aria-label="Plugins by activity">
                {#each histogram.rows as row (row.plugin_id)}
                  {@const widthPct = maxTotal > 0 ? (row.total / maxTotal) * 100 : 0}
                  {@const installPct = row.total > 0 ? (row.installs / row.total) * widthPct : 0}
                  {@const updatePct = row.total > 0 ? (row.updates / row.total) * widthPct : 0}
                  {@const uninstallPct = row.total > 0 ? (row.uninstalls / row.total) * widthPct : 0}
                  {@const failedPct = row.total > 0 ? (row.failures / row.total) * widthPct : 0}
                  {@const isActiveFilter = pluginQueryActive === row.plugin_id}
                  <li>
                    <button
                      type="button"
                      class="top-plugin-row"
                      class:active={isActiveFilter}
                      onclick={() => onHistogramRowClick(row.plugin_id)}
                      aria-pressed={isActiveFilter}
                      title={isActiveFilter
                        ? `Clear filter on ${row.plugin_id}`
                        : `Filter timeline below to ${row.plugin_id}`}
                    >
                      <div class="tp-name" title={row.plugin_id}>
                        <span class="tp-id">{row.plugin_id}</span>
                        <span class="tp-time">
                          {formatInstallEventTime(row.last_occurred_at)}
                        </span>
                      </div>
                      <div class="tp-bar" title="{row.total} events: {row.installs}i {row.updates}u {row.uninstalls}x {row.failures}f">
                        {#if row.installs > 0}
                          <div class="tp-seg seg-install" style="width: {installPct}%"></div>
                        {/if}
                        {#if row.updates > 0}
                          <div class="tp-seg seg-update" style="width: {updatePct}%"></div>
                        {/if}
                        {#if row.uninstalls > 0}
                          <div class="tp-seg seg-uninstall" style="width: {uninstallPct}%"></div>
                        {/if}
                        {#if row.failures > 0}
                          <div class="tp-seg seg-failed" style="width: {failedPct}%"></div>
                        {/if}
                      </div>
                      <div class="tp-counts">
                        <span class="tp-total">{row.total}</span>
                        <span class="tp-breakdown">
                          {#if row.installs > 0}<span class="tp-glyph seg-install" title="{row.installs} install{row.installs === 1 ? '' : 's'}">{installEventGlyph("install")}{row.installs}</span>{/if}
                          {#if row.updates > 0}<span class="tp-glyph seg-update" title="{row.updates} update{row.updates === 1 ? '' : 's'}">{installEventGlyph("update")}{row.updates}</span>{/if}
                          {#if row.uninstalls > 0}<span class="tp-glyph seg-uninstall" title="{row.uninstalls} uninstall{row.uninstalls === 1 ? '' : 's'}">{installEventGlyph("uninstall")}{row.uninstalls}</span>{/if}
                          {#if row.failures > 0}<span class="tp-glyph seg-failed" title="{row.failures} failure{row.failures === 1 ? '' : 's'}">{installEventGlyph("failed")}{row.failures}</span>{/if}
                        </span>
                      </div>
                    </button>
                  </li>
                {/each}
              </ul>
              <p class="top-plugins-legend">
                Each row's bar is scaled relative to the most-active plugin. Stacked
                segments break down by action — installs (green), updates (accent),
                uninstalls (amber), failures (red). Click a row to filter the timeline
                below to that plugin; click again to clear.
              </p>
            {/if}
          {/if}
        </div>
      {/if}
    </section>

    {#if err}
      <p class="err">Could not load history: {err}</p>
    {:else if loading}
      <p class="loading">Loading history…</p>
    {:else if filteredEvents.length === 0}
      <p class="empty">
        {#if events.length === 0 && summary.total_events === 0}
          No install history yet. Browse the Marketplace tab to install your first plugin.
        {:else if filterNarrowing}
          No events match the current filter. Try widening with another action chip or clearing
          the plugin search.
        {:else}
          No events in the last {windowChoice}. Try widening the window.
        {/if}
      </p>
    {:else}
      <ul class="event-list" aria-label="Install events (newest first)">
        {#each filteredEvents as ev (ev.id)}
          <li class="event-row" data-action={ev.action}>
            <span class="ev-glyph" aria-hidden="true">{installEventGlyph(ev.action)}</span>
            <div class="ev-body">
              <span class="ev-line">
                <span class="ev-action">{ev.action}</span>
                <span class="ev-plugin">{ev.plugin_id}</span>
                <span class="ev-version">v{ev.version}</span>
                {#if ev.action === "update" && ev.prior_version}
                  <span class="ev-from">← v{ev.prior_version}</span>
                {/if}
              </span>
              {#if ev.error_msg}
                <span class="ev-err" title={ev.error_msg}>{ev.error_msg}</span>
              {/if}
            </div>
            <time class="ev-time" datetime={new Date(ev.occurred_at * 1000).toISOString()}>
              {formatInstallEventTime(ev.occurred_at)}
            </time>
          </li>
        {/each}
      </ul>
    {/if}

    <footer class="drawer-foot">
      {#if exportToast}
        <span class="export-toast" role="status">{exportToast}</span>
      {/if}
      {#if !confirmingPrune}
        <div class="export-anchor">
          <button
            type="button"
            class="ghost"
            onclick={() => (exportMenuOpen = !exportMenuOpen)}
            disabled={summary.total_events === 0 || pruning || exporting}
            aria-haspopup="menu"
            aria-expanded={exportMenuOpen}
          >
            {exporting ? "Exporting…" : "Export…"}
          </button>
          {#if exportMenuOpen}
            <div class="export-menu" role="menu" aria-label="Export install log">
              <button type="button" role="menuitem" onclick={() => void runExport("csv")}>
                <span class="menu-glyph" aria-hidden="true">⤓</span>
                <span class="menu-body">
                  <span class="menu-title">Export as CSV…</span>
                  <span class="menu-sub">
                    {windowChoice === "all"
                      ? "Whole log · spreadsheet-friendly"
                      : `Last ${windowChoice} · spreadsheet-friendly`}
                  </span>
                </span>
              </button>
              <button type="button" role="menuitem" onclick={() => void runExport("json")}>
                <span class="menu-glyph" aria-hidden="true">⤓</span>
                <span class="menu-body">
                  <span class="menu-title">Export as JSON…</span>
                  <span class="menu-sub">
                    {windowChoice === "all"
                      ? "Whole log · with envelope metadata"
                      : `Last ${windowChoice} · with envelope metadata`}
                  </span>
                </span>
              </button>
            </div>
          {/if}
        </div>
        <button
          type="button"
          class="ghost"
          onclick={() => (confirmingPrune = true)}
          disabled={summary.total_events === 0 || pruning || exporting}
        >
          Clear older than 90d…
        </button>
        <button
          type="button"
          class="primary"
          onclick={onClose}
          disabled={pruning || exporting}>Close</button
        >
      {:else}
        <span class="confirm-msg">Delete log entries older than 90 days?</span>
        <button
          type="button"
          class="ghost"
          onclick={() => (confirmingPrune = false)}
          disabled={pruning}>Cancel</button
        >
        <button
          type="button"
          class="primary danger"
          onclick={() => void runPrune(90)}
          disabled={pruning}>{pruning ? "Pruning…" : "Delete"}</button
        >
      {/if}
    </footer>
  </aside>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 110;
    background: rgba(0, 0, 0, 0.42);
    backdrop-filter: blur(2px);
    display: flex;
    justify-content: flex-end;
    animation: backdrop-in 0.16s ease-out;
  }
  @keyframes backdrop-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  .drawer {
    background: var(--bg-1);
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 30px rgba(0, 0, 0, 0.32);
    width: min(460px, 100%);
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 22px 24px 18px;
    gap: 12px;
    overflow-y: auto;
    animation: drawer-in 0.18s ease-out;
  }
  @keyframes drawer-in {
    from {
      transform: translateX(20px);
      opacity: 0;
    }
    to {
      transform: translateX(0);
      opacity: 1;
    }
  }
  header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 10px;
    align-items: start;
  }
  .title-block h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--text);
  }
  .title-block .sub {
    margin: 4px 0 0;
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .icon-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-3);
    width: 28px;
    height: 28px;
    border-radius: 6px;
    cursor: pointer;
    font: inherit;
    font-size: 13px;
    line-height: 1;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--bg-2);
    border-color: var(--border);
    color: var(--text);
  }
  .window-strip {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    align-self: flex-start;
  }
  .window-strip button {
    background: var(--bg-1);
    color: var(--text-3);
    border: none;
    border-right: 1px solid var(--border);
    padding: 4px 10px;
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }
  .window-strip button:last-child {
    border-right: none;
  }
  .window-strip button:hover:not(.active) {
    background: var(--bg-2);
    color: var(--text);
  }
  .window-strip button.active {
    background: var(--bg-3);
    color: var(--text);
  }

  /* ─── Filter strip (Slice 77) ─────────────────────────────────── */
  .filter-strip {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 2px;
  }
  .action-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-3);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
    line-height: 1.4;
    transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
  }
  .chip:hover:not(.on) {
    background: var(--bg-2);
    color: var(--text);
    border-color: var(--text-3);
  }
  .chip.on {
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-1));
    color: var(--text);
    border-color: var(--accent);
  }
  /* Action-specific glyph tint when chip is ON so the four chips
     read as four flavours, not a uniform "selected" block. */
  .chip.on[data-action="install"] .chip-glyph {
    color: #3fc88c;
  }
  .chip.on[data-action="update"] .chip-glyph {
    color: var(--accent);
  }
  .chip.on[data-action="uninstall"] .chip-glyph {
    color: #e0b450;
  }
  .chip.on[data-action="failed"] .chip-glyph {
    color: #ff6b6b;
  }
  .chip-glyph {
    font-size: 11px;
    color: var(--text-3);
  }
  .chip-label {
    font-weight: 500;
  }

  .plugin-search {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-1);
  }
  .plugin-search:focus-within {
    border-color: var(--accent);
  }
  .search-glyph {
    color: var(--text-3);
    font-size: 13px;
    line-height: 1;
  }
  .search-input {
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text);
    font: inherit;
    font-size: 12px;
    padding: 2px 0;
  }
  .search-input::placeholder {
    color: var(--text-3);
  }
  .search-clear {
    background: transparent;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    font-size: 11px;
    line-height: 1;
  }
  .search-clear:hover {
    background: var(--bg-2);
    color: var(--text);
  }

  .suggest-list {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 5;
    list-style: none;
    margin: 0;
    padding: 4px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.32);
    max-height: 220px;
    overflow-y: auto;
  }
  .suggest-list li {
    margin: 0;
  }
  .suggest-item {
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    color: var(--text);
    font: inherit;
    font-size: 12px;
    padding: 5px 8px;
    border-radius: 4px;
    cursor: pointer;
  }
  .suggest-item:hover,
  .suggest-item[aria-selected="true"] {
    background: var(--bg-3);
  }
  .suggest-id {
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
    font-size: 11.5px;
    color: var(--text);
  }

  .filter-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 4px 0;
  }
  .filter-desc {
    font-size: 11.5px;
    color: var(--text-3);
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .event-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .event-row {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) auto;
    align-items: start;
    gap: 8px;
    padding: 8px 10px;
    font-size: 12px;
    border-top: 1px solid var(--border);
  }
  .event-row:first-child {
    border-top: none;
  }
  .ev-glyph {
    color: var(--text-3);
    font-size: 12px;
    line-height: 1.4;
    text-align: center;
  }
  .event-row[data-action="failed"] .ev-glyph {
    color: var(--danger, rgb(255, 100, 100));
  }
  .event-row[data-action="update"] .ev-glyph {
    color: rgb(245, 180, 70);
  }
  .event-row[data-action="install"] .ev-glyph {
    color: var(--accent);
  }
  .ev-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .ev-line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }
  .ev-action {
    color: var(--text);
    text-transform: capitalize;
    font-weight: 500;
  }
  .ev-plugin {
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
  }
  .ev-version {
    color: var(--text-3);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .ev-from {
    color: var(--text-3);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .ev-err {
    color: var(--danger, rgb(255, 100, 100));
    font-size: 11px;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ev-time {
    color: var(--text-3);
    font-size: 11px;
    line-height: 1.4;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .err,
  .empty,
  .loading {
    margin: 0;
    color: var(--text-3);
    font-size: 12px;
    line-height: 1.5;
    padding: 12px;
    border: 1px dashed var(--border);
    border-radius: 6px;
  }
  .err {
    color: var(--danger, rgb(255, 100, 100));
    border-color: rgba(255, 100, 100, 0.4);
  }
  .drawer-foot {
    margin-top: auto;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
  }
  .drawer-foot button {
    font: inherit;
    padding: 7px 14px;
    border-radius: 6px;
    border: 1px solid var(--border);
    cursor: pointer;
  }
  .drawer-foot .ghost {
    background: transparent;
    color: var(--text-2);
  }
  .drawer-foot .ghost:hover:not(:disabled) {
    background: var(--bg-2);
    color: var(--text);
  }
  .drawer-foot .primary {
    background: var(--accent);
    color: var(--accent-fg, white);
    border-color: var(--accent);
  }
  .drawer-foot .primary.danger {
    background: var(--danger, rgb(255, 100, 100));
    border-color: var(--danger, rgb(255, 100, 100));
  }
  .drawer-foot button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .confirm-msg {
    flex: 1;
    color: var(--text);
    font-size: 12px;
  }
  .export-anchor {
    position: relative;
    margin-right: auto;
  }
  .export-menu {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    min-width: 260px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    z-index: 10;
  }
  .export-menu button {
    background: transparent;
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 8px 10px;
    text-align: left;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr);
    gap: 8px;
    align-items: start;
  }
  .export-menu button:hover {
    background: var(--bg-2);
  }
  .menu-glyph {
    color: var(--text-3);
    font-size: 13px;
    line-height: 1.4;
    text-align: center;
  }
  .menu-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .menu-title {
    color: var(--text);
    font-weight: 500;
  }
  .menu-sub {
    color: var(--text-3);
    font-size: 11px;
    line-height: 1.4;
  }
  .export-toast {
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1.3;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-2);
  }

  /* ─── Retention section (Slice 67) ────────────────────────────── */
  .retention-block {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-1);
    overflow: hidden;
  }
  .retention-toggle {
    width: 100%;
    background: transparent;
    border: none;
    color: var(--text-2);
    padding: 8px 10px;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    text-align: left;
    display: grid;
    grid-template-columns: 14px auto minmax(0, 1fr);
    align-items: center;
    gap: 6px;
  }
  .retention-toggle:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .retention-chevron {
    color: var(--text-3);
    font-size: 10px;
    line-height: 1;
  }
  .retention-label {
    color: var(--text);
    font-weight: 500;
  }
  .retention-meta {
    color: var(--text-3);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
  }
  .retention-body {
    padding: 4px 10px 10px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .retention-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
  }
  .retention-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }
  .field-label {
    color: var(--text-3);
    font-size: 11px;
  }
  .field-input {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .field-input input[type="number"] {
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 6px;
    padding: 5px 8px;
    width: 80px;
    font: inherit;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .field-input input[type="number"]:focus {
    outline: 1px solid var(--accent);
    outline-offset: 0;
    border-color: var(--accent);
  }
  .field-input input[type="number"]:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .field-unit {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .retention-actions {
    display: inline-flex;
    gap: 6px;
  }
  .retention-actions .mini {
    font-size: 11.5px;
    padding: 4px 10px;
    border-radius: 5px;
    border: 1px solid var(--border);
    cursor: pointer;
    font-family: inherit;
  }
  .retention-actions .ghost {
    background: transparent;
    color: var(--text-2);
  }
  .retention-actions .ghost:hover:not(:disabled) {
    background: var(--bg-2);
    color: var(--text);
  }
  .retention-actions .primary {
    background: var(--accent);
    color: var(--accent-fg, white);
    border-color: var(--accent);
  }
  .retention-actions button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .retention-sub {
    margin: 0;
    color: var(--text-3);
    font-size: 11px;
    line-height: 1.5;
  }
  .retention-next {
    color: var(--text-2);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .retention-toast {
    margin: 0;
    color: var(--text-2);
    font-size: 11px;
    line-height: 1.4;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg-2);
  }

  /* ─── Top plugins histogram (Slice 87) ────────────────────────── */
  .top-plugins-block {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-1);
    overflow: hidden;
    margin-top: 8px;
  }
  .top-plugins-toggle {
    width: 100%;
    background: transparent;
    border: none;
    color: var(--text-2);
    padding: 8px 10px;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    text-align: left;
    display: grid;
    grid-template-columns: 14px auto minmax(0, 1fr);
    align-items: center;
    gap: 6px;
  }
  .top-plugins-toggle:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .top-plugins-chevron {
    color: var(--text-3);
    font-size: 10px;
    line-height: 1;
  }
  .top-plugins-label {
    color: var(--text);
    font-weight: 500;
  }
  .top-plugins-meta {
    color: var(--text-3);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
  }
  .top-plugins-body {
    padding: 8px 10px 10px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .top-plugins-loading,
  .top-plugins-empty {
    margin: 0;
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.5;
  }
  .top-plugins-error {
    margin: 0;
    padding: 6px 10px;
    color: #ffb4b4;
    background: rgba(240, 80, 80, 0.1);
    border: 1px solid rgba(240, 80, 80, 0.3);
    border-radius: 5px;
    font-size: 11.5px;
  }
  .top-plugins-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    max-height: 360px;
    overflow-y: auto;
  }
  .top-plugins-list > li { margin: 0; padding: 0; }
  .top-plugin-row {
    display: grid;
    grid-template-columns: minmax(150px, 30%) 1fr auto;
    gap: 10px;
    align-items: center;
    padding: 4px 8px;
    width: 100%;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    color: inherit;
    text-align: left;
    font: inherit;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .top-plugin-row:hover {
    background: rgba(255, 255, 255, 0.025);
    border-color: rgba(255, 255, 255, 0.06);
  }
  .top-plugin-row:focus-visible {
    outline: none;
    border-color: rgba(124, 140, 255, 0.42);
    background: rgba(124, 140, 255, 0.06);
  }
  .top-plugin-row.active {
    background: rgba(124, 140, 255, 0.1);
    border-color: rgba(124, 140, 255, 0.34);
  }
  .top-plugin-row.active:hover {
    background: rgba(124, 140, 255, 0.14);
  }
  .tp-name {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 2px;
  }
  .tp-id {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11.5px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tp-time {
    font-size: 10.5px;
    color: var(--text-3);
  }
  .tp-bar {
    display: flex;
    height: 10px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 5px;
    overflow: hidden;
    min-width: 0;
  }
  .tp-seg {
    height: 100%;
    transition: width 220ms ease-out;
  }
  .seg-install { background: color-mix(in srgb, #6dd49a 75%, transparent); color: #6dd49a; }
  .seg-update  { background: color-mix(in srgb, #7c8cff 75%, transparent); color: #7c8cff; }
  .seg-uninstall { background: color-mix(in srgb, #d9b04c 75%, transparent); color: #d9b04c; }
  .seg-failed  { background: color-mix(in srgb, #ff5d6c 75%, transparent); color: #ff5d6c; }
  .tp-counts {
    display: flex;
    align-items: center;
    gap: 8px;
    font-variant-numeric: tabular-nums;
  }
  .tp-total {
    font-size: 12px;
    color: var(--text);
    font-weight: 600;
    min-width: 28px;
    text-align: right;
  }
  .tp-breakdown {
    display: inline-flex;
    gap: 4px;
    font-size: 10.5px;
  }
  .tp-glyph {
    display: inline-flex;
    gap: 1px;
    align-items: center;
    padding: 1px 4px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.04);
    /* Color comes from .seg-*; background uses a faint neutral. */
  }
  .top-plugins-legend {
    margin: 4px 0 0;
    color: var(--text-3);
    font-size: 11px;
    line-height: 1.5;
  }
</style>
