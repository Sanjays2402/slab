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
    exportInstallLogHistogramCsv,
    exportInstallLogHistogramJson,
    exportInstallLogJson,
    exportInstallLogActivityTimelineCsv,
    exportInstallLogActivityTimelineJson,
    exportInstallLogBucketDrilldownCsv,
    exportInstallLogBucketDrilldownJson,
    formatBytes,
    formatInstallEventTime,
    formatLastAutoPrune,
    formatLogSpan,
    formatNextAutoPrune,
    getInstallLogRetentionPolicy,
    getPluginInstallHistogram,
    getActivityTimeline,
    getBucketDrilldown,
    densifyActivityTimeline,
    installEventGlyph,
    installLogSummary,
    listInstallEventsFiltered,
    listRecentInstallEvents,
    pluginQueryActiveLabel,
    pruneInstallLog,
    recentInstallPluginIds,
    runInstallLogAutoPrune,
    setInstallLogRetentionDays,
    suggestHistogramExportFilename,
    suggestActivityTimelineExportFilename,
    suggestBucketDrilldownExportFilename,
    suggestInstallLogExportFilename,
    summarizeHistogram,
    type HistogramExportFilter,
    type ActivityTimelineExportFilter,
    type ActivityTimelineResult,
    type BucketDrilldownExportFilter,
    type BucketDrilldownResult,
    type TimeBucketGranularity,
    TIME_BUCKET_GRANULARITIES,
    timeBucketLabel,
    type InstallEvent,
    type InstallEventQuery,
    type InstallLogExportFilter,
    type InstallLogRetentionPolicy,
    type InstallLogSummary,
    type PluginHistogramResult,
    type HistogramSortKey,
    HISTOGRAM_SORT_KEYS,
    sortHistogramRows,
    histogramSortLabel,
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
  /** v3.40 Slice 97 — client-side sort axis for the Top plugins
   *  histogram. Defaults to "total" matching the server's emit order;
   *  the user can pivot to installs / updates / failures / recent
   *  without a refetch (cheap pure-data sort on the loaded rows). */
  let histogramSort = $state<HistogramSortKey>("total");

  // ─── Histogram export state (v3.40 Slice 102) ─────────────────────
  //
  // Mirror of the install-log Export… popover in the drawer footer
  // (slice 62), scoped to the Top plugins section so the export
  // verb lives next to the artefact it exports. State cells live
  // alongside the histogram cells above so the section's open/close
  // can dismiss them cleanly.

  /** Open state for the histogram Export… popover. */
  let histogramExportMenuOpen = $state(false);
  /** True while a save-as dialog or backend write is in flight. */
  let histogramExporting = $state(false);
  /** Slim 4-second toast for histogram export success. */
  let histogramExportToast = $state<string | null>(null);
  /** Names timeout handle so back-to-back exports don't pile up. */
  let histogramExportToastTimer: ReturnType<typeof setTimeout> | null = null;

  // ─── Activity over time section (Slice 107 — round 22) ────────────
  //
  // Collapsible "Activity over time" section under the Top plugins
  // block showing per-bucket vertical bars within the currently-
  // selected window. Complementary axis to the Top plugins histogram
  // — that surface answers "WHICH plugins were active", this one
  // answers "WHEN was activity happening". Same window axis as the
  // timeline above; auto-refreshes when window or bucket-width
  // granularity changes. Defaults closed so the per-event timeline
  // stays the primary content; cheap fetch (one indexed scan + a
  // calendar floor pass) so the open/close is snappy.

  /** Section expand/collapse. Defaults closed. */
  let timelineOpen = $state(false);
  /** Loaded timeline result. Null until the first fetch resolves. */
  let timeline = $state<ActivityTimelineResult | null>(null);
  /** True while a timeline fetch is in flight. */
  let timelineLoading = $state(false);
  /** Last error from the timeline loader, surfaced inline. */
  let timelineError = $state<string | null>(null);
  /** Bucket width — day (default, matches the read endpoint), week,
   *  or month. Persisted across open/close for the drawer lifetime. */
  let timelineGranularity = $state<TimeBucketGranularity>("day");

  /** Open state for the timeline Export… popover (mirror of the
   *  histogram popover; lives in its own anchor so dismiss is
   *  independent). */
  let timelineExportMenuOpen = $state(false);
  /** True while a save-as dialog or backend write is in flight. */
  let timelineExporting = $state(false);
  /** Slim 4-second toast for timeline export success. */
  let timelineExportToast = $state<string | null>(null);
  /** Named timeout handle so back-to-back exports replace cleanly. */
  let timelineExportToastTimer: ReturnType<typeof setTimeout> | null = null;

  // ─── Bucket drilldown popover (Slice 112 — round 23) ──────────────
  //
  // Click a bar in the Activity over time chart -> open a focused
  // popover anchored to the bar showing the per-plugin breakdown
  // for that bucket. Answers the natural follow-up: "OK, that day
  // had 23 events — WHICH plugins drove it?". Same canonical
  // popover pattern as the hopper coverage drilldown (round 19).
  //
  // The popover is dismissed by:
  // - Clicking outside the popover (handled by the existing
  //   onWindowClick dispatcher)
  // - Pressing Escape (highest-priority entry in onKeydown's chain
  //   so it dismisses BEFORE the histogram + footer popovers)
  // - Clicking the same bar again (toggle off)
  // - Clicking another bar (the popover anchors to the new bar)
  //
  // State cells live alongside the timeline cells above so the
  // section's open/close can dismiss them cleanly. The export
  // popover has its own anchor so it dismisses independently from
  // the histogram + footer + timeline anchors.

  /** Drilldown result. Null when no bucket is selected. */
  let drilldown = $state<BucketDrilldownResult | null>(null);
  /** True while a drilldown fetch is in flight. */
  let drilldownLoading = $state(false);
  /** Last error from the drilldown loader, surfaced inline. */
  let drilldownError = $state<string | null>(null);
  /** Open state for the drilldown export popover. */
  let drilldownExportMenuOpen = $state(false);
  /** True while a drilldown save-as dialog or backend write is in flight. */
  let drilldownExporting = $state(false);
  /** Slim 4-second toast for drilldown export success. */
  let drilldownExportToast = $state<string | null>(null);
  /** Named timeout handle so back-to-back exports replace cleanly. */
  let drilldownExportToastTimer: ReturnType<typeof setTimeout> | null = null;

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
      if (histogramExportToastTimer) clearTimeout(histogramExportToastTimer);
      if (timelineExportToastTimer) clearTimeout(timelineExportToastTimer);
      if (drilldownExportToastTimer) clearTimeout(drilldownExportToastTimer);
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

  // ─── Slice 107: activity timeline loader ─────────────────────────
  //
  // Fetched on demand when the user expands the section; refreshed
  // automatically when the window choice OR granularity changes.
  // Changes while the section is closed don't trigger a fetch — the
  // next open will use the current window + granularity.

  async function refreshTimeline(): Promise<void> {
    if (!timelineOpen) return;
    timelineLoading = true;
    timelineError = null;
    try {
      timeline = await getActivityTimeline({
        sinceUnix: windowSinceUnix,
        granularity: timelineGranularity,
      });
    } catch (e) {
      timelineError = e instanceof Error ? e.message : String(e);
      timeline = null;
    } finally {
      timelineLoading = false;
    }
  }

  function toggleTimeline() {
    timelineOpen = !timelineOpen;
    if (timelineOpen && timeline === null) {
      void refreshTimeline();
    }
  }

  // Refresh when window or granularity changes (if open).
  let lastTimelineKey = $state<string>("");
  $effect(() => {
    const key = `${timelineOpen ? 1 : 0}|${windowSinceUnix ?? "all"}|${timelineGranularity}`;
    if (key === lastTimelineKey) return;
    lastTimelineKey = key;
    if (timelineOpen) void refreshTimeline();
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
      } else if (drilldownExportMenuOpen) {
        drilldownExportMenuOpen = false;
      } else if (drilldown !== null) {
        drilldown = null;
        drilldownError = null;
      } else if (timelineExportMenuOpen) {
        timelineExportMenuOpen = false;
      } else if (histogramExportMenuOpen) {
        histogramExportMenuOpen = false;
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

  /** Flash a 4s toast scoped to the Top plugins section. Uses its
   *  own timer handle so back-to-back exports replace cleanly. */
  function flashHistogramToast(msg: string): void {
    histogramExportToast = msg;
    if (histogramExportToastTimer) {
      clearTimeout(histogramExportToastTimer);
    }
    histogramExportToastTimer = setTimeout(() => {
      histogramExportToast = null;
      histogramExportToastTimer = null;
    }, 4000);
  }

  /**
   * Export the current Top plugins histogram as CSV or JSON. Mirrors
   * runExport (above) but scoped to the histogram view: window comes
   * from the same windowSinceUnix axis that drives the timeline, so
   * what the user sees is what they get; the row count shipped in
   * the toast matches what the section currently renders.
   *
   * The handler ships the in-state histogram VERBATIM rather than
   * re-fetching — a background prune / install can't sneak in a
   * different aggregate between "click Export" and "click Save".
   * The Tauri command re-queries the storage layer with the same
   * window so the file content matches the on-screen view down to
   * the row counts.
   */
  async function runHistogramExport(kind: "csv" | "json"): Promise<void> {
    histogramExportMenuOpen = false;
    histogramExporting = true;
    try {
      const filter: HistogramExportFilter = {
        since_unix: windowSinceUnix,
        limit: histogramLimit,
      };
      const defaultPath = suggestHistogramExportFilename(filter, kind);
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
          ? await exportInstallLogHistogramCsv(target, filter)
          : await exportInstallLogHistogramJson(target, filter);
      const count = histogram?.rows.length ?? 0;
      const label = kind === "csv" ? "CSV" : "JSON";
      flashHistogramToast(
        `Exported ${count} plugin${count === 1 ? "" : "s"} as ${label} (${formatBytes(bytes)})`,
      );
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      histogramExporting = false;
    }
  }

  /** Flash a 4s toast scoped to the Activity over time section. Uses
   *  its own timer handle so back-to-back exports replace cleanly. */
  function flashTimelineToast(msg: string): void {
    timelineExportToast = msg;
    if (timelineExportToastTimer) {
      clearTimeout(timelineExportToastTimer);
    }
    timelineExportToastTimer = setTimeout(() => {
      timelineExportToast = null;
      timelineExportToastTimer = null;
    }, 4000);
  }

  /**
   * Export the current activity timeline as CSV or JSON. Mirrors
   * runHistogramExport (above) but scoped to the timeline view:
   * window comes from the same windowSinceUnix axis, granularity
   * from the section's selector, so what the user sees is what they
   * get. Same in-state-snapshot semantics — the Tauri command
   * re-queries with the same params so file content matches the
   * on-screen view even if a background install lands between
   * click-Export and click-Save.
   */
  async function runTimelineExport(kind: "csv" | "json"): Promise<void> {
    timelineExportMenuOpen = false;
    timelineExporting = true;
    try {
      const filter: ActivityTimelineExportFilter = {
        since_unix: windowSinceUnix,
        granularity: timelineGranularity,
      };
      const defaultPath = suggestActivityTimelineExportFilename(filter, kind);
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
          ? await exportInstallLogActivityTimelineCsv(target, filter)
          : await exportInstallLogActivityTimelineJson(target, filter);
      const count = timeline?.buckets.length ?? 0;
      const label = kind === "csv" ? "CSV" : "JSON";
      flashTimelineToast(
        `Exported ${count} bucket${count === 1 ? "" : "s"} as ${label} (${formatBytes(bytes)})`,
      );
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      timelineExporting = false;
    }
  }

  // ─── Slice 112: bucket drilldown handlers ──────────────────────────

  /** Flash the drilldown export toast. Same 4s lifecycle + named-
   *  timer-replace contract as the histogram + timeline toasts. */
  function flashDrilldownToast(msg: string): void {
    if (drilldownExportToastTimer) {
      clearTimeout(drilldownExportToastTimer);
    }
    drilldownExportToast = msg;
    drilldownExportToastTimer = setTimeout(() => {
      drilldownExportToast = null;
      drilldownExportToastTimer = null;
    }, 4_000);
  }

  /**
   * Open the bucket drilldown for `bucketStartUnix`. If the popover
   * is already open for the same bucket, toggle it off. Otherwise
   * (closed OR open for a different bucket) load fresh.
   */
  async function openBucketDrilldown(bucketStartUnix: number): Promise<void> {
    // Toggle off when re-clicking the active bar.
    if (drilldown && drilldown.bucket_start_unix === bucketStartUnix) {
      drilldown = null;
      drilldownError = null;
      drilldownExportMenuOpen = false;
      return;
    }
    drilldownLoading = true;
    drilldownError = null;
    drilldownExportMenuOpen = false; // dismiss any stale popover from a prior bucket
    try {
      drilldown = await getBucketDrilldown({
        bucketStartUnix,
        granularity: timelineGranularity,
      });
    } catch (e) {
      drilldownError = e instanceof Error ? e.message : String(e);
      drilldown = null;
    } finally {
      drilldownLoading = false;
    }
  }

  /**
   * Save the bucket drilldown to disk as CSV / JSON. Mirrors
   * runTimelineExport's shape exactly: filter ships in-state
   * bucket coords, default filename via the slice-112 suggester,
   * save dialog opened with the kind-appropriate filter,
   * cancellation is a clean no-op, bytes returned surfaces in the
   * toast for visual feedback.
   */
  async function runDrilldownExport(kind: "csv" | "json"): Promise<void> {
    drilldownExportMenuOpen = false;
    if (!drilldown) return;
    drilldownExporting = true;
    try {
      const filter: BucketDrilldownExportFilter = {
        bucket_start_unix: drilldown.bucket_start_unix,
        granularity: drilldown.granularity,
      };
      const defaultPath = suggestBucketDrilldownExportFilename(filter, kind);
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
          ? await exportInstallLogBucketDrilldownCsv(target, filter)
          : await exportInstallLogBucketDrilldownJson(target, filter);
      const count = drilldown.rows.length;
      const label = kind === "csv" ? "CSV" : "JSON";
      flashDrilldownToast(
        `Exported ${count} plugin${count === 1 ? "" : "s"} as ${label} (${formatBytes(bytes)})`,
      );
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      drilldownExporting = false;
    }
  }

  /**
   * Dismiss the export popover on outside click — matches the
   * Notion / Linear convention. The check uses .closest() on the
   * anchor wrapper so clicks INSIDE the menu (its buttons) don't
   * dismiss it before the handler fires.
   *
   * Two anchors live in the drawer now (footer install-log Export…
   * and Top plugins histogram Export…). Each has its own anchor
   * class so an outside click on one doesn't accidentally close
   * the other.
   */
  function onWindowClick(e: MouseEvent): void {
    const target = e.target as HTMLElement | null;
    if (exportMenuOpen) {
      if (!target?.closest(".export-anchor")) {
        exportMenuOpen = false;
      }
    }
    if (histogramExportMenuOpen) {
      if (!target?.closest(".histogram-export-anchor")) {
        histogramExportMenuOpen = false;
      }
    }
    if (timelineExportMenuOpen) {
      if (!target?.closest(".timeline-export-anchor")) {
        timelineExportMenuOpen = false;
      }
    }
    if (drilldownExportMenuOpen) {
      if (!target?.closest(".drilldown-export-anchor")) {
        drilldownExportMenuOpen = false;
      }
    }
    // Dismiss the drilldown popover entirely when the click lands
    // OUTSIDE the popover AND outside the timeline chart (clicking
    // a different bar should anchor the popover to that bar, not
    // dismiss it — openBucketDrilldown handles the re-anchor).
    if (
      drilldown !== null &&
      !target?.closest(".bucket-drilldown-popover") &&
      !target?.closest(".timeline-bar")
    ) {
      drilldown = null;
      drilldownError = null;
      drilldownExportMenuOpen = false;
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
              <div class="top-plugins-sort">
                <label class="top-plugins-sort-label" for="top-plugins-sort-select">
                  Sort by
                </label>
                <select
                  id="top-plugins-sort-select"
                  class="top-plugins-sort-select"
                  bind:value={histogramSort}
                >
                  {#each HISTOGRAM_SORT_KEYS as key}
                    <option value={key}>{histogramSortLabel(key)}</option>
                  {/each}
                </select>
                <div class="histogram-export-anchor">
                  <button
                    type="button"
                    class="top-plugins-export-btn"
                    onclick={() => (histogramExportMenuOpen = !histogramExportMenuOpen)}
                    disabled={histogramExporting || histogramLoading}
                    aria-haspopup="menu"
                    aria-expanded={histogramExportMenuOpen}
                    title="Export the Top plugins histogram for the current window"
                  >
                    {histogramExporting ? "Exporting…" : "Export…"}
                  </button>
                  {#if histogramExportMenuOpen}
                    <div class="export-menu histogram-export-menu" role="menu" aria-label="Export top plugins histogram">
                      <button type="button" role="menuitem" onclick={() => void runHistogramExport("csv")}>
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
                      <button type="button" role="menuitem" onclick={() => void runHistogramExport("json")}>
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
              </div>
              {#if histogramExportToast}
                <p class="histogram-export-toast" role="status">{histogramExportToast}</p>
              {/if}
              {@const sortedRows = sortHistogramRows(histogram.rows, histogramSort)}
              {@const maxTotal = histogram.rows[0]?.total ?? 1}
              <ul class="top-plugins-list" aria-label="Plugins by activity">
                {#each sortedRows as row (row.plugin_id)}
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
                Each row's bar is scaled relative to the most-active plugin's total. Stacked
                segments break down by action — installs (green), updates (accent),
                uninstalls (amber), failures (red). Click a row to filter the timeline
                below to that plugin; click again to clear. Use the Sort by selector to
                pivot the order (the bars stay anchored to total activity). Export… ships
                the current window as a CSV (spreadsheet) or JSON (archive) snapshot.
              </p>
            {/if}
          {/if}
        </div>
      {/if}
    </section>

    <section class="timeline-block" aria-labelledby="timeline-heading">
      <button
        type="button"
        class="timeline-toggle"
        aria-expanded={timelineOpen}
        aria-controls="timeline-body"
        onclick={toggleTimeline}
      >
        <span class="timeline-chevron" aria-hidden="true">
          {timelineOpen ? "▾" : "▸"}
        </span>
        <span class="timeline-label" id="timeline-heading">Activity over time</span>
        <span class="timeline-meta">
          {#if timeline}
            {timeline.grand_total} event{timeline.grand_total === 1 ? "" : "s"} across {timeline.buckets.length} {timelineGranularity}{timeline.buckets.length === 1 ? "" : "s"} · {windowChoice}
          {:else if timelineLoading}
            Loading…
          {:else}
            Click to expand
          {/if}
        </span>
      </button>
      {#if timelineOpen}
        <div class="timeline-body" id="timeline-body">
          {#if timelineError}
            <p class="timeline-error" role="alert">
              Could not load timeline: {timelineError}
            </p>
          {:else if timelineLoading && !timeline}
            <p class="timeline-loading">Bucketing activity…</p>
          {:else if timeline}
            <div class="timeline-controls">
              <label class="timeline-control-label" for="timeline-granularity-select">
                Bucket width
              </label>
              <select
                id="timeline-granularity-select"
                class="timeline-granularity-select"
                bind:value={timelineGranularity}
              >
                {#each TIME_BUCKET_GRANULARITIES as g}
                  <option value={g}>{timeBucketLabel(g)}</option>
                {/each}
              </select>
              <div class="timeline-export-anchor">
                <button
                  type="button"
                  class="timeline-export-btn"
                  onclick={() => (timelineExportMenuOpen = !timelineExportMenuOpen)}
                  disabled={timelineExporting || timelineLoading || timeline.buckets.length === 0}
                  aria-haspopup="menu"
                  aria-expanded={timelineExportMenuOpen}
                  title={timeline.buckets.length === 0
                    ? "No activity buckets to export"
                    : "Export the activity timeline for the current window"}
                >
                  {timelineExporting ? "Exporting…" : "Export…"}
                </button>
                {#if timelineExportMenuOpen}
                  <div class="export-menu timeline-export-menu" role="menu" aria-label="Export activity timeline">
                    <button type="button" role="menuitem" onclick={() => void runTimelineExport("csv")}>
                      <span class="menu-glyph" aria-hidden="true">⤓</span>
                      <span class="menu-body">
                        <span class="menu-title">Export as CSV…</span>
                        <span class="menu-sub">
                          {timeBucketLabel(timelineGranularity)} · spreadsheet-friendly
                        </span>
                      </span>
                    </button>
                    <button type="button" role="menuitem" onclick={() => void runTimelineExport("json")}>
                      <span class="menu-glyph" aria-hidden="true">⤓</span>
                      <span class="menu-body">
                        <span class="menu-title">Export as JSON…</span>
                        <span class="menu-sub">
                          {timeBucketLabel(timelineGranularity)} · with envelope metadata
                        </span>
                      </span>
                    </button>
                  </div>
                {/if}
              </div>
            </div>
            {#if timelineExportToast}
              <p class="timeline-export-toast" role="status">{timelineExportToast}</p>
            {/if}
            {#if timeline.buckets.length === 0}
              <p class="timeline-empty">
                No activity in the last {windowChoice}. Try widening the window
                or installing a plugin from the Marketplace tab.
              </p>
            {:else}
              {@const denseBuckets = densifyActivityTimeline(timeline.buckets, timelineGranularity)}
              {@const maxBucketTotal = denseBuckets.reduce(
                (acc: number, b: { total: number }) => (b.total > acc ? b.total : acc),
                0,
              )}
              {@const firstStart = denseBuckets[0].bucket_start_unix}
              {@const lastStart = denseBuckets[denseBuckets.length - 1].bucket_start_unix}
              <div class="timeline-chart" role="img" aria-label="Activity timeline bar chart">
                {#each denseBuckets as b (b.bucket_start_unix)}
                  {@const heightPct = maxBucketTotal > 0 ? (b.total / maxBucketTotal) * 100 : 0}
                  {@const installPct = b.total > 0 ? (b.installs / b.total) * heightPct : 0}
                  {@const updatePct = b.total > 0 ? (b.updates / b.total) * heightPct : 0}
                  {@const uninstallPct = b.total > 0 ? (b.uninstalls / b.total) * heightPct : 0}
                  {@const failedPct = b.total > 0 ? (b.failures / b.total) * heightPct : 0}
                  {@const isActiveBucket = drilldown?.bucket_start_unix === b.bucket_start_unix}
                  <button
                    type="button"
                    class="timeline-bar"
                    class:empty-bar={b.total === 0}
                    class:active-bar={isActiveBucket}
                    disabled={b.total === 0}
                    aria-pressed={isActiveBucket}
                    aria-label={b.total === 0
                      ? `No events on ${new Date(b.bucket_start_unix * 1000).toISOString().slice(0, 10)}`
                      : `Drill into ${new Date(b.bucket_start_unix * 1000).toISOString().slice(0, 10)} — ${b.total} event${b.total === 1 ? "" : "s"}`}
                    title={b.total === 0
                      ? `${new Date(b.bucket_start_unix * 1000).toISOString().slice(0, 10)} — no events`
                      : `Click to drill into ${new Date(b.bucket_start_unix * 1000).toISOString().slice(0, 10)} (${b.total} event${b.total === 1 ? "" : "s"}: ${b.installs}i ${b.updates}u ${b.uninstalls}x ${b.failures}f)`}
                    onclick={() => {
                      if (b.total > 0) void openBucketDrilldown(b.bucket_start_unix);
                    }}
                  >
                    {#if b.failures > 0}
                      <div class="bar-seg seg-failed" style="height: {failedPct}%"></div>
                    {/if}
                    {#if b.uninstalls > 0}
                      <div class="bar-seg seg-uninstall" style="height: {uninstallPct}%"></div>
                    {/if}
                    {#if b.updates > 0}
                      <div class="bar-seg seg-update" style="height: {updatePct}%"></div>
                    {/if}
                    {#if b.installs > 0}
                      <div class="bar-seg seg-install" style="height: {installPct}%"></div>
                    {/if}
                    {#if b.total === 0}
                      <div class="bar-zero" aria-hidden="true"></div>
                    {/if}
                  </button>
                {/each}
              </div>
              {#if drilldownLoading || drilldownError || drilldown}
                <div class="bucket-drilldown-popover" role="region" aria-label="Bucket drilldown">
                  {#if drilldownLoading}
                    <p class="drilldown-loading">Loading bucket drilldown…</p>
                  {:else if drilldownError}
                    <p class="drilldown-error" role="alert">
                      Could not load drilldown: {drilldownError}
                    </p>
                  {:else if drilldown}
                    {@const bucketIso = new Date(drilldown.bucket_start_unix * 1000)
                      .toISOString()
                      .slice(0, 10)}
                    <div class="drilldown-head">
                      <div class="drilldown-title-block">
                        <span class="drilldown-title">{bucketIso}</span>
                        <span class="drilldown-sub">
                          {drilldown.grand_total} event{drilldown.grand_total === 1 ? "" : "s"} ·
                          {drilldown.rows.length} plugin{drilldown.rows.length === 1 ? "" : "s"} ·
                          {timeBucketLabel(drilldown.granularity)}
                        </span>
                      </div>
                      <div class="drilldown-actions">
                        <div class="drilldown-export-anchor">
                          <button
                            type="button"
                            class="drilldown-export-btn"
                            onclick={() => (drilldownExportMenuOpen = !drilldownExportMenuOpen)}
                            disabled={drilldownExporting || drilldown.rows.length === 0}
                            aria-haspopup="menu"
                            aria-expanded={drilldownExportMenuOpen}
                            title={drilldown.rows.length === 0
                              ? "No plugins to export"
                              : `Export the ${bucketIso} drilldown`}
                          >
                            {drilldownExporting ? "Exporting…" : "Export…"}
                          </button>
                          {#if drilldownExportMenuOpen}
                            <div
                              class="export-menu drilldown-export-menu"
                              role="menu"
                              aria-label="Export bucket drilldown"
                            >
                              <button
                                type="button"
                                role="menuitem"
                                onclick={() => void runDrilldownExport("csv")}
                              >
                                <span class="menu-glyph" aria-hidden="true">⤓</span>
                                <span class="menu-body">
                                  <span class="menu-title">Export as CSV…</span>
                                  <span class="menu-sub">
                                    {bucketIso} · spreadsheet-friendly
                                  </span>
                                </span>
                              </button>
                              <button
                                type="button"
                                role="menuitem"
                                onclick={() => void runDrilldownExport("json")}
                              >
                                <span class="menu-glyph" aria-hidden="true">⤓</span>
                                <span class="menu-body">
                                  <span class="menu-title">Export as JSON…</span>
                                  <span class="menu-sub">
                                    {bucketIso} · with envelope metadata
                                  </span>
                                </span>
                              </button>
                            </div>
                          {/if}
                        </div>
                        <button
                          type="button"
                          class="drilldown-close"
                          onclick={() => {
                            drilldown = null;
                            drilldownError = null;
                            drilldownExportMenuOpen = false;
                          }}
                          aria-label="Close drilldown"
                          title="Close drilldown (Esc)"
                        >
                          ✕
                        </button>
                      </div>
                    </div>
                    {#if drilldownExportToast}
                      <p class="drilldown-export-toast" role="status">{drilldownExportToast}</p>
                    {/if}
                    {#if drilldown.rows.length === 0}
                      <p class="drilldown-empty">
                        No plugin activity in this bucket. (Empty buckets shouldn't be
                        clickable — this is unexpected.)
                      </p>
                    {:else}
                      {@const maxRowTotal = drilldown.rows[0]?.total ?? 1}
                      <ul class="drilldown-list" aria-label="Plugins in bucket">
                        {#each drilldown.rows as row (row.plugin_id)}
                          {@const widthPct = maxRowTotal > 0 ? (row.total / maxRowTotal) * 100 : 0}
                          {@const installPct = row.total > 0 ? (row.installs / row.total) * widthPct : 0}
                          {@const updatePct = row.total > 0 ? (row.updates / row.total) * widthPct : 0}
                          {@const uninstallPct = row.total > 0 ? (row.uninstalls / row.total) * widthPct : 0}
                          {@const failedPct = row.total > 0 ? (row.failures / row.total) * widthPct : 0}
                          {@const isActiveFilter = pluginQueryActive === row.plugin_id}
                          <li>
                            <button
                              type="button"
                              class="drilldown-row"
                              class:active={isActiveFilter}
                              onclick={() => onHistogramRowClick(row.plugin_id)}
                              aria-pressed={isActiveFilter}
                              title={isActiveFilter
                                ? `Clear filter on ${row.plugin_id}`
                                : `Filter timeline below to ${row.plugin_id}`}
                            >
                              <div class="dd-name" title={row.plugin_id}>
                                <span class="dd-id">{row.plugin_id}</span>
                              </div>
                              <div class="dd-bar">
                                {#if row.installs > 0}
                                  <div class="dd-seg seg-install" style="width: {installPct}%"></div>
                                {/if}
                                {#if row.updates > 0}
                                  <div class="dd-seg seg-update" style="width: {updatePct}%"></div>
                                {/if}
                                {#if row.uninstalls > 0}
                                  <div class="dd-seg seg-uninstall" style="width: {uninstallPct}%"></div>
                                {/if}
                                {#if row.failures > 0}
                                  <div class="dd-seg seg-failed" style="width: {failedPct}%"></div>
                                {/if}
                              </div>
                              <div class="dd-counts">
                                <span class="dd-total">{row.total}</span>
                              </div>
                            </button>
                          </li>
                        {/each}
                      </ul>
                      <p class="drilldown-legend">
                        Per-plugin breakdown of activity in this {timeBucketLabel(drilldown.granularity).toLowerCase()}.
                        Stacked segments by action (install green, update accent, uninstall amber,
                        failed red). Click a row to filter the event list below to that plugin;
                        Export… saves the breakdown as CSV or JSON.
                      </p>
                    {/if}
                  {/if}
                </div>
              {/if}
              {@const firstStartLabel = new Date(firstStart * 1000).toISOString().slice(0, 10)}
              {@const lastStartLabel = new Date(lastStart * 1000).toISOString().slice(0, 10)}
              <div class="timeline-axis" aria-hidden="true">
                <span class="axis-label">{firstStartLabel}</span>
                <span class="axis-label axis-right">{lastStartLabel}</span>
              </div>
              <p class="timeline-legend">
                Each bar is one {timelineGranularity} from {firstStartLabel} to {lastStartLabel}, scaled to the busiest bucket.
                Stacked segments break down by action — installs (green), updates (accent),
                uninstalls (amber), failures (red). Empty buckets render as
                hairlines to keep the time axis honest. Change Bucket width to
                pivot day/week/month; Export… ships the current window as a
                CSV (spreadsheet) or JSON (archive) snapshot.
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
  /* Slice 97 — Sort by selector lives just above the histogram list,
     compact horizontal label + native select. Native dropdown keeps
     the keyboard-a11y story honest and matches the retention block's
     control vocabulary. */
  .top-plugins-sort {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 8px;
  }
  .top-plugins-sort-label {
    font-size: 11px;
    color: var(--text-3);
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .top-plugins-sort-select {
    appearance: none;
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-1);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 5px;
    padding: 3px 22px 3px 8px;
    font-size: 11.5px;
    line-height: 1.4;
    cursor: pointer;
    background-image:
      linear-gradient(45deg, transparent 50%, var(--text-3) 50%),
      linear-gradient(135deg, var(--text-3) 50%, transparent 50%);
    background-position:
      calc(100% - 12px) 50%,
      calc(100% - 8px) 50%;
    background-size:
      4px 4px,
      4px 4px;
    background-repeat: no-repeat;
  }
  .top-plugins-sort-select:hover {
    border-color: rgba(255, 255, 255, 0.16);
  }
  .top-plugins-sort-select:focus-visible {
    outline: 2px solid rgba(124, 140, 255, 0.55);
    outline-offset: 1px;
    border-color: rgba(124, 140, 255, 0.55);
  }

  /* Slice 102 — histogram Export… button + popover. The button lives
     beside the Sort by selector inside the top-plugins-sort row so
     the export verb sits next to the artefact it exports. The
     histogram-export-anchor pushes to the row's right so the sort
     selector stays at the left edge (the more-frequently-used
     control reads first). Popover anchors top-aligned BELOW the
     button so it cascades down into the histogram body — opposite
     of the footer's Export… popover which cascades UP into the
     drawer body. */
  .histogram-export-anchor {
    position: relative;
    margin-left: auto;
  }
  .top-plugins-export-btn {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-2);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 5px;
    padding: 3px 10px;
    font: inherit;
    font-size: 11.5px;
    line-height: 1.4;
    cursor: pointer;
  }
  .top-plugins-export-btn:hover:not(:disabled) {
    border-color: rgba(255, 255, 255, 0.16);
    color: var(--text);
  }
  .top-plugins-export-btn:focus-visible {
    outline: 2px solid rgba(124, 140, 255, 0.55);
    outline-offset: 1px;
    border-color: rgba(124, 140, 255, 0.55);
  }
  .top-plugins-export-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  /* Popover anchors DOWN+RIGHT-aligned beneath the button (the sort
     row sits at the top of the body, so opening UPWARDS would clip
     against the section toggle). Reuses the .export-menu look-and-
     feel from the footer popover so the two surfaces feel like one
     verb across the drawer. */
  .histogram-export-menu {
    bottom: auto;
    top: calc(100% + 6px);
    left: auto;
    right: 0;
    min-width: 240px;
  }
  /* Slice 102 toast — slim inline notice anchored to the histogram
     section so the user's eye doesn't jump to the footer. Same
     vocabulary as the install-log .export-toast but tinted accent-
     green to read as a positive write outcome. The 4s fade matches
     the install-log toast duration. */
  .histogram-export-toast {
    margin: 6px 0 4px;
    color: rgb(170, 230, 195);
    font-size: 11.5px;
    line-height: 1.3;
    padding: 4px 10px;
    border: 1px solid rgba(110, 220, 154, 0.36);
    border-radius: 6px;
    background: rgba(110, 220, 154, 0.06);
    animation: histogram-toast-in 0.16s ease-out;
  }
  @keyframes histogram-toast-in {
    from {
      opacity: 0;
      transform: translateY(-2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  /* ─── Slice 107 — Activity over time (round 22) ───────────────── */
  /* Mirrors the Top plugins block (Slice 87) on every shape except
     the chart itself — sibling .timeline-block under .top-plugins-
     block, same collapsible toggle, same body padding, same toast
     vocabulary. The chart inside the body is the only visually
     novel element: a row of vertical bars rendering the per-bucket
     activity stack, instead of the histogram's row of horizontal
     bars. */
  .timeline-block {
    margin-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    padding-top: 8px;
  }
  .timeline-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: none;
    border: none;
    color: var(--text);
    cursor: pointer;
    padding: 6px 4px;
    font: inherit;
    font-size: 12.5px;
    line-height: 1.4;
    text-align: left;
    border-radius: 5px;
  }
  .timeline-toggle:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .timeline-chevron {
    color: var(--text-2);
    width: 12px;
  }
  .timeline-label {
    font-weight: 500;
  }
  .timeline-meta {
    color: var(--text-2);
    font-size: 11.5px;
    margin-left: auto;
    white-space: nowrap;
  }
  .timeline-body {
    padding: 6px 4px 4px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .timeline-loading,
  .timeline-empty {
    color: var(--text-2);
    font-size: 12px;
    margin: 2px 4px;
    line-height: 1.4;
  }
  .timeline-error {
    color: rgb(255, 160, 160);
    font-size: 12px;
    padding: 6px 10px;
    border: 1px solid rgba(255, 110, 110, 0.32);
    background: rgba(255, 110, 110, 0.06);
    border-radius: 6px;
    margin: 4px 4px;
  }
  /* Controls row — bucket-width selector on the left, Export… on the
     right (margin-left: auto). Same layout pattern as the histogram's
     sort row so the two sections feel like siblings. */
  .timeline-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px;
  }
  .timeline-control-label {
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1;
  }
  /* Same custom dark-glass styling as the histogram sort-select for
     visual consistency. Custom chevron via two linear-gradient
     backgrounds so we don't ship an extra SVG asset. */
  .timeline-granularity-select {
    appearance: none;
    -webkit-appearance: none;
    background-color: rgba(255, 255, 255, 0.04);
    color: var(--text);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 5px;
    padding: 3px 22px 3px 8px;
    font: inherit;
    font-size: 11.5px;
    line-height: 1.4;
    cursor: pointer;
    background-image:
      linear-gradient(45deg, transparent 50%, var(--text-2) 50%),
      linear-gradient(135deg, var(--text-2) 50%, transparent 50%);
    background-position:
      calc(100% - 12px) 50%,
      calc(100% - 8px) 50%;
    background-size:
      4px 4px,
      4px 4px;
    background-repeat: no-repeat;
  }
  .timeline-granularity-select:hover {
    border-color: rgba(255, 255, 255, 0.16);
  }
  .timeline-granularity-select:focus-visible {
    outline: 2px solid rgba(124, 140, 255, 0.55);
    outline-offset: 1px;
    border-color: rgba(124, 140, 255, 0.55);
  }
  .timeline-export-anchor {
    position: relative;
    margin-left: auto;
  }
  .timeline-export-btn {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-2);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 5px;
    padding: 3px 10px;
    font: inherit;
    font-size: 11.5px;
    line-height: 1.4;
    cursor: pointer;
  }
  .timeline-export-btn:hover:not(:disabled) {
    border-color: rgba(255, 255, 255, 0.16);
    color: var(--text);
  }
  .timeline-export-btn:focus-visible {
    outline: 2px solid rgba(124, 140, 255, 0.55);
    outline-offset: 1px;
    border-color: rgba(124, 140, 255, 0.55);
  }
  .timeline-export-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .timeline-export-menu {
    bottom: auto;
    top: calc(100% + 6px);
    left: auto;
    right: 0;
    min-width: 240px;
  }
  .timeline-export-toast {
    margin: 4px 0 2px;
    color: rgb(170, 230, 195);
    font-size: 11.5px;
    line-height: 1.3;
    padding: 4px 10px;
    border: 1px solid rgba(110, 220, 154, 0.36);
    border-radius: 6px;
    background: rgba(110, 220, 154, 0.06);
    animation: histogram-toast-in 0.16s ease-out;
  }
  /* The chart proper: a flex row of fixed-min-width vertical bars.
     min-width-bar lets a long timeline scroll horizontally rather
     than squish the bars below readability; padding keeps the bars
     visually anchored above the date axis below. */
  .timeline-chart {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 96px;
    padding: 6px 4px 4px;
    overflow-x: auto;
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.015);
  }
  .timeline-bar {
    position: relative;
    flex: 1 1 0;
    min-width: 6px;
    height: 100%;
    display: flex;
    flex-direction: column-reverse;
    justify-content: flex-start;
    align-items: stretch;
    border: 0;
    padding: 0;
    margin: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font: inherit;
    border-radius: 2px 2px 0 0;
    overflow: hidden;
    transition:
      background-color 0.12s ease,
      box-shadow 0.12s ease;
  }
  .timeline-bar:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.03);
  }
  .timeline-bar:focus-visible {
    outline: 1px solid rgb(124, 140, 255);
    outline-offset: 1px;
  }
  .timeline-bar:disabled {
    cursor: default;
  }
  /* Active bucket: persistent outline + accent glow ring above the
     bar so the user always knows which bucket the drilldown popover
     is anchored to as they scroll the chart. */
  .timeline-bar.active-bar {
    background: rgba(124, 140, 255, 0.1);
    box-shadow: 0 -2px 0 0 rgb(124, 140, 255) inset;
  }
  .timeline-bar.active-bar:hover:not(:disabled) {
    background: rgba(124, 140, 255, 0.16);
  }
  .timeline-bar .bar-seg {
    width: 100%;
    flex: none;
  }
  .timeline-bar .bar-seg.seg-install {
    background: rgb(110, 220, 154);
  }
  .timeline-bar .bar-seg.seg-update {
    background: rgb(124, 140, 255);
  }
  .timeline-bar .bar-seg.seg-uninstall {
    background: rgb(240, 178, 100);
  }
  .timeline-bar .bar-seg.seg-failed {
    background: rgb(240, 130, 130);
  }
  /* Empty buckets get a hairline at the baseline so the time axis
     reads honestly — a gap of empty bars conveys "nothing happened
     these days" rather than collapsing the axis. */
  .timeline-bar .bar-zero {
    width: 100%;
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
  }
  .timeline-bar.empty-bar {
    opacity: 0.6;
  }
  /* Date axis labels — first bucket date on the left, last on the
     right. Light-on-dark, monospace for column alignment if a future
     export label widens to include time. */
  .timeline-axis {
    display: flex;
    justify-content: space-between;
    color: var(--text-2);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    padding: 0 4px;
  }
  .timeline-axis .axis-label {
    line-height: 1.2;
  }
  .timeline-axis .axis-right {
    text-align: right;
  }
  .timeline-legend {
    margin: 4px 4px 0;
    color: var(--text-2);
    font-size: 11px;
    line-height: 1.4;
  }

  /* ─── Slice 112: bucket drilldown popover ──────────────────────── */

  /* The popover renders BELOW the timeline chart (sibling to the
     axis + legend) so it doesn't have to be position:absolute over
     the chart — keeps the layout flow natural and lets the popover
     grow vertically with rows. */
  .bucket-drilldown-popover {
    margin: 8px 4px 0;
    padding: 12px;
    border: 1px solid rgba(124, 140, 255, 0.18);
    background: linear-gradient(
      180deg,
      rgba(124, 140, 255, 0.06) 0%,
      rgba(124, 140, 255, 0.02) 100%
    );
    border-radius: 8px;
    animation: drilldown-fade-in 0.18s ease;
  }
  @keyframes drilldown-fade-in {
    from {
      opacity: 0;
      transform: translateY(-2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .drilldown-loading {
    margin: 0;
    color: var(--text-2);
    font-size: 12px;
    font-style: italic;
  }
  .drilldown-error {
    margin: 0;
    color: rgb(240, 130, 130);
    font-size: 12px;
  }
  .drilldown-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
    margin-bottom: 8px;
  }
  .drilldown-title-block {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .drilldown-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
  }
  .drilldown-sub {
    font-size: 11px;
    color: var(--text-2);
  }
  .drilldown-actions {
    display: flex;
    gap: 6px;
    align-items: center;
    flex: 0 0 auto;
  }
  .drilldown-export-anchor {
    position: relative;
  }
  .drilldown-export-btn {
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-1);
    cursor: pointer;
    font-size: 11.5px;
    transition: background-color 0.12s ease;
  }
  .drilldown-export-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }
  .drilldown-export-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .drilldown-export-menu {
    right: 0;
    left: auto;
    top: calc(100% + 4px);
    bottom: auto;
  }
  .drilldown-close {
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    font-size: 12px;
    line-height: 1;
    transition: background-color 0.12s ease;
  }
  .drilldown-close:hover {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-1);
  }
  .drilldown-export-toast {
    margin: 0 0 8px;
    padding: 6px 10px;
    color: rgb(170, 230, 195);
    background: rgba(170, 230, 195, 0.08);
    border: 1px solid rgba(170, 230, 195, 0.18);
    border-radius: 6px;
    font-size: 11.5px;
    animation: drilldown-fade-in 0.16s ease;
  }
  .drilldown-empty {
    margin: 0;
    color: var(--text-2);
    font-size: 12px;
  }
  .drilldown-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    /* Cap the popover height so a busy bucket doesn't push the
       legend + axis labels off-screen. The scroll is local to the
       popover so the surrounding chart stays anchored. */
    max-height: 220px;
    overflow-y: auto;
  }
  .drilldown-list li {
    margin: 0;
  }
  .drilldown-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(60px, 1.4fr) auto;
    gap: 8px;
    align-items: center;
    width: 100%;
    padding: 6px 8px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
    font: inherit;
    transition:
      background-color 0.12s ease,
      border-color 0.12s ease;
  }
  .drilldown-row:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .drilldown-row.active {
    border-color: rgba(124, 140, 255, 0.4);
    background: rgba(124, 140, 255, 0.08);
  }
  .dd-name {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .dd-id {
    font-size: 11.5px;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dd-bar {
    height: 8px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 3px;
    overflow: hidden;
    display: flex;
  }
  .dd-seg {
    height: 100%;
    flex: none;
  }
  .dd-seg.seg-install {
    background: rgb(110, 220, 154);
  }
  .dd-seg.seg-update {
    background: rgb(124, 140, 255);
  }
  .dd-seg.seg-uninstall {
    background: rgb(240, 178, 100);
  }
  .dd-seg.seg-failed {
    background: rgb(240, 130, 130);
  }
  .dd-counts {
    font-size: 11.5px;
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
    min-width: 24px;
    text-align: right;
  }
  .dd-total {
    font-weight: 500;
  }
  .drilldown-legend {
    margin: 8px 0 0;
    color: var(--text-2);
    font-size: 10.5px;
    line-height: 1.4;
  }
</style>
