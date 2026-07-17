<script lang="ts">
  // PluginsPanel — v1.3.0 "Foundry" Slice 10.
  //
  // Settings-style control surface for the plugin system. Lists every
  // discovered plugin with: name + author + version + description,
  // enable/disable toggle, error chip when the manifest fails to parse,
  // contribution counts, and an expandable per-row "what does this
  // plugin actually contribute" drilldown for debugging.
  //
  // No backend code lives here — every action delegates to
  // `$lib/plugins`. The panel is purely a view + a few thin command
  // dispatches.

  import { onMount } from "svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    pluginsStore,
    refreshPlugins,
    reloadPlugins,
    setPluginEnabled,
    pluginsDir,
    getPluginGrants,
    setPluginGrants,
    resetPluginGrants,
    emptyPluginGrants,
    type Plugin,
    type PluginGrants,
    type PluginsSnapshot,
    isBundled,
    BUNDLED_PLUGIN_IDS,
  } from "$lib/plugins";
  import {
    marketplaceStore,
    refreshMarketplace,
    installPlugin,
    uninstallPluginById,
    marketplaceAvailable,
    formatBytes,
    compareSemver,
    installLogSummary,
    listUpdateTargets,
    updateAllPlugins,
    listenUpdateProgress,
    pluralizeUpdates,
    formatUpdateSummary,
    type IndexEntry,
    type InstallLogSummary,
    type MarketplaceState,
    type UpdatePlan,
    type UpdateTarget,
    type BatchUpdateReport,
    type UpdateProgress,
  } from "$lib/marketplace";
  import { notify } from "$lib/notify";
  import { tStore, t } from "$lib/i18n";
  import PluginDetailDrawer from "$lib/components/PluginDetailDrawer.svelte";
  import RecentInstallsDrawer from "$lib/components/RecentInstallsDrawer.svelte";
  import InstallProgressModal from "$lib/components/InstallProgressModal.svelte";
  import UninstallConfirmModal from "$lib/components/UninstallConfirmModal.svelte";
  import PluginConsentModal from "$lib/components/PluginConsentModal.svelte";
  import BulkUpdateProgressOverlay, {
    type BulkUpdateRowState,
  } from "$lib/components/BulkUpdateProgressOverlay.svelte";
  import { fuzzyMatchEntry, highlightHTML, type EntryFuzzyResult } from "$lib/marketplace/fuzzy";

  // ---------------- Installed-tab state (unchanged from Foundry) ----
  // Local mirror of the store so we can render reactively.
  let snap = $state<PluginsSnapshot>({
    plugins: [],
    themes: [],
    locales: [],
    commands: [],
    aiProviders: [],
    pdfActions: [],
    loadedAt: 0,
  });
  let dirPath = $state<string | null>(null);
  // Per-plugin expand-state for the contributions drilldown.
  let expanded = $state<Record<string, boolean>>({});
  // Per-plugin "currently writing to backend" flag so we can disable
  // the toggle while a flip is in flight and avoid double-clicks.
  let busy = $state<Record<string, boolean>>({});

  // ---------------- v3.39 Slice 57 — Recent installs drawer state -----
  /** Slim summary of the install log — used to gate the History
   *  button so it only appears when there's something to show. */
  let installLog = $state<InstallLogSummary>({
    total_events: 0,
    distinct_plugins: 0,
    oldest_occurred_at: null,
  });
  /** Drawer open flag. */
  let recentInstallsOpen = $state(false);

  /** Refresh the slim install-log summary. Cheap (three small
   *  queries); we call this on mount and after every install /
   *  uninstall / prune so the toolbar badge stays current. */
  async function refreshInstallLog(): Promise<void> {
    try {
      installLog = await installLogSummary();
    } catch (e) {
      // Non-fatal — surface the failure to the console but leave
      // the badge dark rather than nag the user.
      console.warn("[slab] installLogSummary failed", e);
    }
  }

  // ---------------- v3.39 Round-15 — Updates available banner ------
  /** The current update plan, or null when not yet loaded. Banner
   *  hides when null OR when targets.length === 0. */
  let updatePlan = $state<UpdatePlan | null>(null);
  /** True while listUpdateTargets() or updateAllPlugins() is in flight.
   *  Disables the "Update all" button. */
  let updateBusy = $state(false);
  /** Per-row in-flight flag during the bulk update — keyed by plugin
   *  id. Slice 72 grows this into a {phase, error} map for the
   *  per-step overlay; slice 71 keeps it as a simple bool gate so the
   *  banner's per-row Update buttons disable correctly. */
  let updateRowBusy = $state<Record<string, boolean>>({});
  /** Expanded/collapsed banner state. Collapsed by default so the
   *  user sees the summary at a glance and only expands when they
   *  want to review per-target details. */
  let updatesExpanded = $state(false);
  /** Per-session dismiss flag. Hides the banner until the user takes
   *  an action that calls refreshUpdateTargets() with a non-empty
   *  plan (install, uninstall, panel reload). Does NOT persist
   *  across reloads — Sanjay's house style is "banner you can
   *  ignore but never permanently kill". */
  let updatesDismissed = $state(false);

  // ---------------- v3.39 Round-15 — Bulk-update overlay state -----
  /** Type-tagged so the overlay can render the right state per row
   *  without re-importing the union from the marketplace module. */
  type OverlayState = {
    rows: BulkUpdateRowState[];
    currentIndex: number;
    finished: boolean;
    summary: string;
    batchId: number;
  } | null;
  /** When non-null, render the BulkUpdateProgressOverlay component.
   *  Slice 72 surface for the per-step live progress. */
  let bulkOverlay = $state<OverlayState>(null);

  /** True iff the banner should render: a plan exists, has targets,
   *  and the user hasn't dismissed it this session. */
  let showUpdatesBanner = $derived(
    !updatesDismissed &&
      updatePlan !== null &&
      updatePlan.targets.length > 0,
  );

  /** Refresh the update plan from the backend. Cheap (one cache-aware
   *  index fetch + one in-memory diff against the registry). Best-
   *  effort — failures warn to console rather than nag the user. */
  async function refreshUpdateTargets(): Promise<void> {
    if (!marketplaceAvailable()) {
      updatePlan = { targets: [], total_bytes: 0 };
      return;
    }
    try {
      updatePlan = await listUpdateTargets();
    } catch (e) {
      // Common path: no network + no cache. Keep the prior plan if
      // any; surface only to console.
      console.warn("[slab] listUpdateTargets failed", e);
    }
  }

  /** "Update all" — kick off the bulk update for every target in the
   *  current plan. The banner gates its own button while this is in
   *  flight; on completion we refresh both the registry (so the
   *  installed list shows the bumped versions) and the plan (so the
   *  banner auto-clears once everything succeeded). */
  async function onUpdateAll(): Promise<void> {
    if (!updatePlan || updatePlan.targets.length === 0 || updateBusy) return;
    const ids = updatePlan.targets.map((t: UpdateTarget) => t.id);
    await runUpdateBatch(ids);
  }

  /** Single-row "Update" — same backend call as Update-all but for
   *  one id. Useful when the user wants to update only a subset
   *  (e.g. defer a heavyweight one for later). */
  async function onUpdateOne(target: UpdateTarget): Promise<void> {
    if (updateRowBusy[target.id] || updateBusy) return;
    await runUpdateBatch([target.id]);
  }

  /** Shared runner for both Update-all and single-row Update. Owns
   *  the busy flag transitions, toast notifications, and post-run
   *  refresh of the plan + registry + install log. The slice-72
   *  overlay state is also driven here — we set up the per-row
   *  reducer + subscribe to `marketplace://update-progress` BEFORE
   *  firing the backend call so early `starting` events are
   *  captured. */
  async function runUpdateBatch(ids: string[]): Promise<void> {
    if (ids.length === 0 || !updatePlan) return;

    // Resolve UpdateTarget rows from the current plan; ids not in
    // the plan are silently skipped (shouldn't happen in practice).
    const planById = new Map<string, UpdateTarget>(
      updatePlan.targets.map((t: UpdateTarget) => [t.id, t]),
    );
    const initialRows: BulkUpdateRowState[] = ids
      .map((id) => planById.get(id))
      .filter((t): t is UpdateTarget => t !== undefined)
      .map((target) => ({ target, phase: "pending", error: null }));
    if (initialRows.length === 0) return;

    updateBusy = true;
    for (const id of ids) updateRowBusy[id] = true;

    const batchId = Date.now();
    bulkOverlay = {
      rows: initialRows,
      currentIndex: 0,
      finished: false,
      summary: "",
      batchId,
    };

    // Subscribe to the progress channel BEFORE firing the backend
    // call. The handler mutates bulkOverlay.rows in place so the
    // overlay re-renders per event.
    const unlisten = await listenUpdateProgress((progress: UpdateProgress) => {
      // Filter out events from other in-flight batches (the UI
      // never fires more than one at a time, but the contract
      // honours batch_id correlation).
      if (!bulkOverlay || progress.batch_id !== bulkOverlay.batchId) return;
      const rowIdx = bulkOverlay.rows.findIndex(
        (r) => r.target.id === progress.plugin_id,
      );
      if (rowIdx === -1) return;
      const nextRows = bulkOverlay.rows.slice();
      const existing = nextRows[rowIdx];
      if (progress.phase === "starting") {
        nextRows[rowIdx] = { ...existing, phase: "updating", error: null };
      } else if (progress.phase === "done") {
        nextRows[rowIdx] = { ...existing, phase: "done", error: null };
      } else if (progress.phase === "error") {
        nextRows[rowIdx] = {
          ...existing,
          phase: "failed",
          error: progress.error ?? "Unknown error",
        };
      }
      bulkOverlay = {
        ...bulkOverlay,
        rows: nextRows,
        currentIndex: progress.index,
      };
    });

    try {
      const report = await updateAllPlugins(batchId, ids);
      // Pull fresh data after the batch — registry first so the
      // installed list shows bumped versions, then the plan so the
      // banner re-derives (and likely hides) based on the new state.
      await refreshPlugins();
      void refreshInstallLog();
      await refreshUpdateTargets();
      // Finalise the overlay summary + mark finished. The user
      // dismisses the overlay manually so they can read the per-row
      // outcomes after the batch lands.
      const summary = formatUpdateSummary(report);
      if (bulkOverlay && bulkOverlay.batchId === batchId) {
        bulkOverlay = { ...bulkOverlay, finished: true, summary };
      }
      // Toast summary mirrors the overlay summary so the user gets a
      // bottom-right confirmation too. Distinct grammar per outcome
      // path matches the round-15 design.
      if (report.failed === 0) {
        notify.success(summary);
      } else if (report.succeeded === 0) {
        notify.error(summary, {
          detail: firstErrorDetail(report),
        });
      } else {
        notify.warning(summary, {
          detail: firstErrorDetail(report),
        });
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      notify.error("Bulk update failed", { detail: msg });
      // Surface the error on the overlay too (and mark finished so
      // the user can dismiss).
      if (bulkOverlay && bulkOverlay.batchId === batchId) {
        bulkOverlay = {
          ...bulkOverlay,
          finished: true,
          summary: "Bulk update failed",
        };
      }
    } finally {
      await unlisten();
      updateBusy = false;
      for (const id of ids) delete updateRowBusy[id];
    }
  }

  /** Dismiss the bulk-update overlay. The overlay's own button
   *  refuses to fire this while `finished === false`, but we keep
   *  the guard here too as defence-in-depth. */
  function dismissBulkOverlay(): void {
    if (bulkOverlay && bulkOverlay.finished) {
      bulkOverlay = null;
    }
  }

  /** Pull the first failed-outcome error message from a batch report
   *  for the toast's detail line. Mirrors the existing single-install
   *  modal's error-message surfacing — show ONE concrete cause rather
   *  than a generic "N failed" with no actionable info. */
  function firstErrorDetail(report: BatchUpdateReport): string | undefined {
    for (const o of report.outcomes) {
      if (o.kind === "failed") return `${o.plugin_id}: ${o.error}`;
    }
    return undefined;
  }

  // ---------------- Bench (v1.4.0) — Browse-tab state --------------
  /** Which tab is showing — 'installed' or 'browse'. The Browse tab
   *  fetches the marketplace index lazily on first activation. */
  let tab = $state<"installed" | "browse">("installed");
  let marketplace = $state<MarketplaceState>({
    phase: "idle",
    index: null,
    isStale: false,
    isEmbeddedSeed: false,
    error: null,
    loadedAt: 0,
    busy: {},
  });

  // ---------------- v2.0.2 "Workshop Marketplace" — Browse-tab UX -----
  /** Free-text query typed into the Browse search box. Empty = show all. */
  let browseQuery = $state<string>("");
  /** Category chip filter. `null` = "All categories". Single-select. */
  let browseCategory = $state<string | null>(null);
  /**
   * Sort mode for the Browse tab.
   * - `relevance` — default when a query is active; falls back to
   *   `popular` when query is empty.
   * - `popular`   — sort by `installs` descending.
   * - `newest`    — sort by `version` (semver-aware) descending then name.
   * - `name`      — A→Z by name (case-insensitive).
   */
  let browseSort = $state<"relevance" | "popular" | "newest" | "name">("relevance");

  /** Distinct, sorted list of categories present in the current index. */
  let browseCategories = $derived<string[]>(
    (() => {
      const set = new Set<string>();
      for (const p of marketplace.index?.plugins ?? []) {
        for (const c of p.categories ?? []) set.add(c);
      }
      return Array.from(set).sort((a, b) => a.localeCompare(b));
    })()
  );

  /**
   * Filtered + sorted Browse list. Each entry is bundled with the
   * field-level highlight ranges from the fuzzy matcher so the
   * template can render <mark>-wrapped HTML inline.
   */
  type RankedEntry = {
    entry: IndexEntry;
    score: number;
    matches: EntryFuzzyResult["fieldRanges"];
  };
  let browseRanked = $derived<RankedEntry[]>(
    (() => {
      const all = marketplace.index?.plugins ?? [];
      const q = browseQuery.trim();
      const filteredByCategory = browseCategory
        ? all.filter((p) => (p.categories ?? []).includes(browseCategory!))
        : all;

      const scored = filteredByCategory
        .map((entry) => {
          const r = fuzzyMatchEntry(q, entry);
          return { entry, score: r.score, matches: r.fieldRanges };
        })
        // When a query is active, drop non-matching entries.
        .filter((r) => (q ? r.score > 0 : true));

      // Sort mode resolution. With an active query, default "relevance"
      // truly means relevance; without a query we don't have a useful
      // relevance signal so we fall back to "popular".
      const effectiveSort =
        browseSort === "relevance" && !q ? "popular" : browseSort;

      switch (effectiveSort) {
        case "popular":
          scored.sort(
            (a, b) =>
              (b.entry.installs ?? 0) - (a.entry.installs ?? 0) ||
              a.entry.name.localeCompare(b.entry.name)
          );
          break;
        case "newest":
          scored.sort(
            (a, b) =>
              compareSemver(b.entry.version, a.entry.version) ||
              a.entry.name.localeCompare(b.entry.name)
          );
          break;
        case "name":
          scored.sort((a, b) => a.entry.name.localeCompare(b.entry.name));
          break;
        case "relevance":
        default:
          scored.sort(
            (a, b) =>
              b.score - a.score ||
              (b.entry.installs ?? 0) - (a.entry.installs ?? 0) ||
              a.entry.name.localeCompare(b.entry.name)
          );
      }
      return scored;
    })()
  );

  /** Total count of plugins matching the current filter, for the
   *  result-count chip in the toolbar. */
  let browseResultCount = $derived(browseRanked.length);

  // ---------------- Slice 8 — drawer + install modal state ---------
  /** When set, render the PluginDetailDrawer for this entry. */
  let drawerEntry = $state<IndexEntry | null>(null);

  /**
   * When set, render the InstallProgressModal. The shape mirrors the
   * modal's `Props` so we can splat it directly. Phase transitions
   * are driven by setTimeout heuristics in `onInstall` — the backend
   * doesn't emit progress events yet, but this UI is the contract we
   * want to honor when it does.
   */
  type InstallModalState = {
    entry: IndexEntry;
    phase: "verifying" | "downloading" | "extracting" | "done" | "error";
    error: string | null;
  } | null;
  let installModal = $state<InstallModalState>(null);

  /**
   * Slice 9 — uninstall confirmation modal state. When set, render the
   * UninstallConfirmModal. `busy` flips true while the backend uninstall
   * call is in flight so the modal can disable its buttons + show a
   * "Uninstalling…" label. The modal stays mounted until the call
   * either succeeds (clear state + show success toast) or fails
   * (clear state + show error toast), so the user gets clear feedback
   * either way.
   */
  type UninstallModalState = {
    entry: IndexEntry;
    installedVersion: string;
    busy: boolean;
  } | null;
  let uninstallModal = $state<UninstallModalState>(null);

  // ---------------- Slice 5 (v2.0.0) — consent modal state ----------
  /**
   * When set, render the PluginConsentModal. The modal is opened in
   * two scenarios:
   *
   *   1. First-enable flow — `toggleEnabled(p, true)` checked
   *      `getPluginGrants(id).has_decision` and got `false`. The
   *      enable is *pending* until the user approves; we stash the
   *      callback in `onResolve` so we can resume.
   *   2. Re-review flow — user clicked "Review permissions" on an
   *      already-enabled plugin. `initial` is pre-filled with their
   *      current grants and `onResolve` is `null` (we just persist
   *      the new grants, no enable flow to resume).
   *
   * `busy` flips true during the `setPluginGrants` write so the
   * modal can disable its buttons.
   */
  type ConsentModalState = {
    plugin: Plugin;
    initial: PluginGrants | null;
    /** Called after grants are written. `approved` distinguishes
     *  Approve from Deny so the parent can decide whether to flip
     *  the enable flag. `null` for the re-review flow. */
    onResolve: ((approved: boolean) => void) | null;
  } | null;
  let consentModal = $state<ConsentModalState>(null);

  $effect(() => {
    const unsubPlugins = pluginsStore.subscribe((v) => (snap = v));
    const unsubMarketplace = marketplaceStore.subscribe((v) => (marketplace = v));
    return () => {
      unsubPlugins();
      unsubMarketplace();
    };
  });

  onMount(() => {
    // Layout boot already refreshes plugins, but call here too so the
    // panel works even if the user opens it before boot completes.
    void refreshPlugins().then(() => {
      // Slice 8: if any plugins are installed, opportunistically fetch
      // the marketplace index so the Installed tab can show
      // "update available" badges without waiting for the user to
      // visit Browse first. Idle-only — no-op if already loaded.
      if (snap.plugins.length > 0 && marketplace.phase === "idle" && marketplaceAvailable()) {
        void refreshMarketplace();
      }
      // Round-15: compute the bulk-update plan as soon as plugins +
      // index are reachable so the "Updates available" banner can
      // render without waiting for an explicit refresh. Best-effort.
      void refreshUpdateTargets();
    });
    pluginsDir()
      .then((p) => (dirPath = p))
      .catch(() => (dirPath = null));
    // Slice 57: load the install-log summary so the History button
    // can gate on `total_events > 0`. Cheap; safe in browser mode
    // (returns the empty summary).
    void refreshInstallLog();
  });

  /** Fetch marketplace index the first time the Browse tab opens. */
  function ensureMarketplaceLoaded() {
    if (marketplace.phase === "idle" && marketplaceAvailable()) {
      void refreshMarketplace();
    }
  }

  function selectTab(next: "installed" | "browse") {
    tab = next;
    if (next === "browse") ensureMarketplaceLoaded();
  }

  /** Map plugin id → installed version (or null if not installed). */
  let installedVersion = $derived.by<Record<string, string | null>>(() => {
    const out: Record<string, string | null> = {};
    for (const p of snap.plugins) {
      out[p.id] = p.manifest?.version ?? null;
    }
    return out;
  });

  /**
   * Slice 8: cross-tab "update available" lookup. For every installed
   * plugin, check whether the marketplace index has a strictly newer
   * version. Returns a map id → the index entry to use for the
   * upgrade. Empty when the marketplace hasn't loaded yet.
   */
  let availableUpdates = $derived.by<Record<string, IndexEntry>>(() => {
    const out: Record<string, IndexEntry> = {};
    if (marketplace.phase !== "ready" || !marketplace.index) return out;
    for (const p of snap.plugins) {
      const installed = p.manifest?.version;
      if (!installed) continue;
      const entry = marketplace.index.plugins.find((e) => e.id === p.id);
      if (!entry) continue;
      if (compareSemver(entry.version, installed) > 0) {
        out[p.id] = entry;
      }
    }
    return out;
  });

  function entryStatus(entry: IndexEntry): "install" | "installed" | "update" {
    const cur = installedVersion[entry.id];
    if (!cur) return "install";
    return compareSemver(entry.version, cur) > 0 ? "update" : "installed";
  }

  /**
   * Format an install count for the Browse-card pill. Compact form
   * for big numbers so the chip stays one line wide on small windows.
   *   42       → "42"
   *   1_234    → "1.2k"
   *   1_500_000 → "1.5M"
   */
  function formatInstalls(n: number): string {
    if (n < 1000) return n.toString();
    if (n < 1_000_000) {
      const k = n / 1000;
      return (k < 10 ? k.toFixed(1) : Math.round(k).toString()) + "k";
    }
    const m = n / 1_000_000;
    return (m < 10 ? m.toFixed(1) : Math.round(m).toString()) + "M";
  }

  // ---------------- Slice 8a — drawer helpers ----------------------
  function openDrawer(entry: IndexEntry) {
    drawerEntry = entry;
  }
  function closeDrawer() {
    drawerEntry = null;
  }
  /**
   * Click handler for the Installed-tab update badge. Switches to the
   * Browse tab AND opens the detail drawer at the matching entry, so
   * the user gets a one-click path from "I see there's an update" to
   * "I'm reading the release notes about it."
   */
  function jumpToUpdate(entry: IndexEntry) {
    selectTab("browse");
    drawerEntry = entry;
  }

  /**
   * Slice 8a/8b unified install flow. Wraps the original Slice 7
   * install with an InstallProgressModal that shows phased status.
   * Phase timing is heuristic (no backend events yet); the modal's
   * contract is intentionally narrow so a future PR can replace the
   * timers with real Tauri progress events without changing the UI.
   */
  async function onInstall(entry: IndexEntry) {
    if (!marketplaceAvailable()) {
      notify.error(t("plugins.notify.browserOnly"));
      return;
    }
    installModal = { entry, phase: "verifying", error: null };
    // Heuristic phase ticks: verify → download (400ms) → extract (2s).
    // We guard each transition against the install having advanced or
    // changed entry to avoid stomping a real terminal state.
    const downloadTimer = setTimeout(() => {
      if (installModal && installModal.entry.id === entry.id && installModal.phase === "verifying") {
        installModal = { ...installModal, phase: "downloading" };
      }
    }, 400);
    const extractTimer = setTimeout(() => {
      if (
        installModal &&
        installModal.entry.id === entry.id &&
        installModal.phase === "downloading"
      ) {
        installModal = { ...installModal, phase: "extracting" };
      }
    }, 2000);
    try {
      await installPlugin(entry);
      await refreshPlugins();
      // Slice 57: install row landed in the log; refresh the
      // summary so the History toolbar button appears (or its count
      // increments) without requiring a panel remount.
      void refreshInstallLog();
      // Round-15: re-derive the bulk update plan — installing /
      // updating a single plugin removes it from the banner.
      void refreshUpdateTargets();
      clearTimeout(downloadTimer);
      clearTimeout(extractTimer);
      installModal = { entry, phase: "done", error: null };
      // Auto-dismiss success after a beat — the toast already
      // confirms success in the bottom-right.
      setTimeout(() => {
        if (
          installModal &&
          installModal.entry.id === entry.id &&
          installModal.phase === "done"
        ) {
          installModal = null;
        }
      }, 1800);
      notify.success(t("plugins.notify.installOk", { name: entry.name }));
    } catch (e) {
      clearTimeout(downloadTimer);
      clearTimeout(extractTimer);
      const msg = e instanceof Error ? e.message : String(e);
      installModal = { entry, phase: "error", error: msg };
      // Slice 57: backend has logged a `failed` row for this attempt;
      // refresh the summary so the History badge surfaces the
      // failure path even when no installs ever succeeded.
      void refreshInstallLog();
      notify.error(t("plugins.notify.installFailed"), { detail: msg });
    }
  }

  function dismissInstallModal() {
    installModal = null;
  }

  /**
   * Slice 9 — open the uninstall confirmation modal instead of firing
   * the destructive call immediately. Resolves the currently-installed
   * version (which may differ from `entry.version` if there's an
   * update pending) so the modal can show what's actually on disk.
   */
  async function onUninstall(entry: IndexEntry) {
    if (!marketplaceAvailable()) return;
    const shown = installedVersion[entry.id] ?? entry.version;
    uninstallModal = { entry, installedVersion: shown, busy: false };
  }

  /**
   * Slice 9 — backdrop click / Cancel / Esc on the modal. Refuses to
   * close while the uninstall is in flight so the user can't strand a
   * half-finished filesystem op.
   */
  function dismissUninstallModal() {
    if (uninstallModal?.busy) return;
    uninstallModal = null;
  }

  /**
   * Slice 9 — the user pressed "Uninstall". Run the backend call,
   * surface success / failure via toast + close the modal.
   */
  async function confirmUninstall() {
    if (!uninstallModal || uninstallModal.busy) return;
    const entry = uninstallModal.entry;
    uninstallModal = { ...uninstallModal, busy: true };
    try {
      const removed = await uninstallPluginById(entry.id);
      if (removed) {
        await refreshPlugins();
        // Slice 57: uninstall row landed; refresh the summary so
        // the History badge keeps current. Skip when nothing was
        // removed (no log row would have been written).
        void refreshInstallLog();
        // Round-15: an uninstalled plugin disappears from the
        // update plan entirely. Refresh so the banner count and
        // total bytes both re-derive.
        void refreshUpdateTargets();
        notify.success(t("plugins.notify.uninstallOk", { name: entry.name }));
      }
      // Close the detail drawer if it was showing the now-removed plugin.
      // Otherwise the user is staring at a drawer for a plugin that no
      // longer exists on disk, and the Uninstall button there would
      // pop a confirmation for a non-installed plugin (status flips to
      // "available" but the drawer caches the old `status` until close).
      if (drawerEntry?.id === entry.id) {
        drawerEntry = null;
      }
      uninstallModal = null;
    } catch (e) {
      uninstallModal = null;
      notify.error(t("plugins.notify.uninstallFailed"), {
        detail: e instanceof Error ? e.message : String(e),
      });
    }
  }

  async function toggleEnabled(p: Plugin, next: boolean) {
    if (busy[p.id]) return;

    // Slice 5 (v2.0.0): when *enabling* a v2.0.0 runtime plugin for
    // the first time, gate on the user's consent. We only ask for
    // plugins with a `[runtime]` section — declarative-only v1.x
    // plugins skip this entirely (no JS, no caps to grant).
    //
    // The flag we check is `has_decision`, not "grants are non-zero":
    // an explicit "deny everything" decision should be remembered so
    // we don't badger the user every enable cycle. They can clear it
    // via "Reset permissions".
    if (next && p.manifest?.runtime) {
      try {
        const resp = await getPluginGrants(p.id);
        if (!resp.has_decision) {
          await openConsentForEnable(p);
          return; // openConsentForEnable resumes the toggle on approve
        }
      } catch (e) {
        // If the grants subsystem is broken, fail loud rather than
        // silently enabling without consent — that would be worse.
        notify.error(t("plugins.error"), {
          detail: e instanceof Error ? e.message : String(e),
        });
        return;
      }
    }

    busy[p.id] = true;
    try {
      const ok = await setPluginEnabled(p.id, next);
      if (!ok) {
        notify.error(t("plugins.error"), { detail: `Plugin ${p.id} not found` });
      }
    } catch (e) {
      notify.error(t("plugins.error"), { detail: e instanceof Error ? e.message : String(e) });
    } finally {
      busy[p.id] = false;
    }
  }

  // ---------------- Slice 5 (v2.0.0) — consent flow helpers --------

  /**
   * Open the consent modal in *first-enable* mode. Returns to the
   * caller immediately; the actual `setPluginEnabled` call happens
   * inside `onConsentApprove` once the user clicks Approve.
   */
  function openConsentForEnable(p: Plugin): Promise<void> {
    return new Promise((resolve) => {
      consentModal = {
        plugin: p,
        initial: null,
        onResolve: async (approved) => {
          if (approved) {
            busy[p.id] = true;
            try {
              const ok = await setPluginEnabled(p.id, true);
              if (!ok) {
                notify.error(t("plugins.error"), { detail: `Plugin ${p.id} not found` });
              }
            } catch (e) {
              notify.error(t("plugins.error"), {
                detail: e instanceof Error ? e.message : String(e),
              });
            } finally {
              busy[p.id] = false;
            }
          }
          resolve();
        },
      };
    });
  }

  /**
   * Re-review path — user clicked "Review permissions" on a
   * currently-installed plugin. Pre-fills the modal with their
   * existing grants. No enable side effect; Approve just writes
   * the (possibly updated) grants, Deny closes without writing.
   */
  async function onReviewPermissions(p: Plugin) {
    if (!p.manifest?.runtime) return;
    let initial: PluginGrants | null = null;
    try {
      const resp = await getPluginGrants(p.id);
      initial = resp.has_decision ? resp.grants : null;
    } catch (e) {
      notify.error(t("plugins.error"), {
        detail: e instanceof Error ? e.message : String(e),
      });
      return;
    }
    consentModal = {
      plugin: p,
      initial,
      onResolve: null, // re-review: no enable flow to resume
    };
  }

  /**
   * Forget the user's grant decision for this plugin. Next time
   * they enable it, the consent modal re-appears. Doesn't change
   * the enable state itself — Sanjay's design is "permissions are
   * an axis orthogonal to enabled/disabled".
   */
  async function onResetPermissions(p: Plugin) {
    if (!p.manifest?.runtime) return;
    try {
      await resetPluginGrants(p.id);
      notify.success(t("plugins.consent.notify.reset", { name: p.manifest.name }));
    } catch (e) {
      notify.error(t("plugins.error"), {
        detail: e instanceof Error ? e.message : String(e),
      });
    }
  }

  /**
   * Modal Approve callback. Writes the chosen grants, fires the
   * resume callback (which decides whether to flip enable), shows a
   * success toast.
   */
  async function onConsentApprove(grants: PluginGrants) {
    if (!consentModal) return;
    const { plugin, onResolve } = consentModal;
    try {
      await setPluginGrants(plugin.id, grants);
      notify.success(
        t("plugins.consent.notify.approved", { name: plugin.manifest?.name ?? plugin.id }),
      );
    } catch (e) {
      notify.error(t("plugins.error"), {
        detail: e instanceof Error ? e.message : String(e),
      });
      consentModal = null;
      return;
    }
    consentModal = null;
    if (onResolve) await onResolve(true);
  }

  /**
   * Modal Deny callback. Persists an explicit deny-all decision so
   * we remember the user's choice (and don't re-prompt on every
   * subsequent enable attempt). Aborts the enable flow if one is
   * pending.
   */
  async function onConsentDeny() {
    if (!consentModal) return;
    const { plugin, onResolve } = consentModal;
    // Only persist a deny-all decision in the first-enable flow.
    // For re-review, Deny means "I changed my mind, keep existing
    // grants" — we just close.
    if (onResolve) {
      try {
        await setPluginGrants(plugin.id, emptyPluginGrants());
      } catch (e) {
        notify.error(t("plugins.error"), {
          detail: e instanceof Error ? e.message : String(e),
        });
      }
      notify.info(
        t("plugins.consent.notify.denied", { name: plugin.manifest?.name ?? plugin.id }),
      );
    }
    consentModal = null;
    if (onResolve) await onResolve(false);
  }

  async function onReload() {
    try {
      const fresh = await reloadPlugins();
      notify.success(t("plugins.reloadDone"), { detail: `${fresh.length} plugin(s)` });
      // Round-15: explicit reload re-checks the index against the
      // freshly-discovered plugin set so the banner reflects any
      // out-of-band install / uninstall (e.g. user dropped a tarball
      // into ~/.slab/plugins/ manually).
      void refreshUpdateTargets();
    } catch (e) {
      notify.error(t("plugins.error"), { detail: e instanceof Error ? e.message : String(e) });
    }
  }

  async function onOpenDir() {
    try {
      let p = dirPath;
      if (!p) {
        p = await pluginsDir();
        dirPath = p;
      }
      await revealItemInDir(p);
    } catch (e) {
      notify.error(t("plugins.error"), { detail: e instanceof Error ? e.message : String(e) });
    }
  }

  function toggleExpand(id: string) {
    expanded[id] = !expanded[id];
  }

  type ContribCount = { n: number; label: string };

  function contribCounts(p: Plugin): ContribCount[] {
    const c = p.manifest?.contributions;
    if (!c) return [];
    return [
      { n: c.themes.length, label: t("plugins.contrib.themes") },
      { n: c.locales.length, label: t("plugins.contrib.locales") },
      { n: c.commands.length, label: t("plugins.contrib.commands") },
      { n: c.ai_providers.length, label: t("plugins.contrib.aiProviders") },
      { n: c.pdf_actions.length, label: t("plugins.contrib.pdfActions") },
    ].filter((x) => x.n > 0);
  }
</script>

<section class="panel plugins-panel">
  <div class="content-header">
    <h1>{$tStore("plugins.title")}</h1>
    <p class="subtitle">{$tStore("plugins.subtitle")}</p>
  </div>

  <div class="tab-strip seg" role="tablist" aria-label="Plugin tabs">
    <button
      type="button"
      role="tab"
      aria-selected={tab === "installed"}
      class:tab-active={tab === "installed"}
      onclick={() => selectTab("installed")}
    >
      {$tStore("plugins.tabs.installed")}
      <span class="tab-count">{snap.plugins.length}</span>
    </button>
    <button
      type="button"
      role="tab"
      aria-selected={tab === "browse"}
      class:tab-active={tab === "browse"}
      onclick={() => selectTab("browse")}
    >
      {$tStore("plugins.tabs.browse")}
      {#if marketplace.index}
        <span class="tab-count">{marketplace.index.plugins.length}</span>
      {/if}
    </button>
  </div>

  {#if tab === "installed"}
    <div class="toolbar">
      <button type="button" class="ghost" onclick={onOpenDir}
        >📁 {$tStore("plugins.openDir")}</button
      >
      <button type="button" class="ghost" onclick={onReload}>↻ {$tStore("plugins.reload")}</button>
      {#if installLog.total_events > 0}
        <button
          type="button"
          class="ghost history-btn"
          onclick={() => (recentInstallsOpen = true)}
          title="View install / update / uninstall history"
        >
          ⏱ History
          <span class="history-count" aria-label="event count">{installLog.total_events}</span>
        </button>
      {/if}
    </div>

    <!-- Round-15: Updates available banner ───────────────────────── -->
    {#if showUpdatesBanner && updatePlan}
      <div class="updates-banner" role="region" aria-label="Updates available">
        <div class="updates-banner-head">
          <button
            type="button"
            class="updates-toggle"
            onclick={() => (updatesExpanded = !updatesExpanded)}
            aria-expanded={updatesExpanded}
            aria-controls="updates-banner-body"
          >
            <span class="updates-chev" aria-hidden="true">
              {updatesExpanded ? "▾" : "▸"}
            </span>
            <span class="updates-arrow" aria-hidden="true">↑</span>
            <span class="updates-headline">
              {pluralizeUpdates(updatePlan.targets.length)}
            </span>
            <span class="updates-meta">
              {formatBytes(updatePlan.total_bytes)}
              · {updatesExpanded ? "Hide list" : "Review"}
            </span>
          </button>
          <div class="updates-actions">
            <button
              type="button"
              class="updates-update-all"
              onclick={onUpdateAll}
              disabled={updateBusy}
            >
              {updateBusy ? "Updating…" : "Update all"}
            </button>
            <button
              type="button"
              class="updates-dismiss"
              onclick={() => (updatesDismissed = true)}
              title="Hide until next install / uninstall / reload"
              aria-label="Dismiss updates banner"
            >
              ×
            </button>
          </div>
        </div>
        {#if updatesExpanded}
          <ul class="updates-list" id="updates-banner-body">
            {#each updatePlan.targets as target (target.id)}
              <li class="updates-row">
                <div class="updates-row-meta">
                  <span class="updates-row-name">{target.entry.name}</span>
                  <span class="updates-row-versions">
                    <span class="updates-row-prior">v{target.installed_version}</span>
                    <span class="updates-row-arrow" aria-hidden="true">→</span>
                    <span class="updates-row-next">v{target.available_version}</span>
                  </span>
                  <span class="updates-row-size">{formatBytes(target.size_bytes)}</span>
                </div>
                <button
                  type="button"
                  class="updates-row-update"
                  onclick={() => void onUpdateOne(target)}
                  disabled={updateBusy || updateRowBusy[target.id]}
                  title="Update {target.entry.name} to v{target.available_version}"
                >
                  {updateRowBusy[target.id] ? "Updating…" : "Update"}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    {#if snap.plugins.length === 0}
      <div class="empty-state">
        <h2>{$tStore("plugins.empty.title")}</h2>
        <p>{$tStore("plugins.empty.body")}</p>
        {#if dirPath}
          <code class="path">{dirPath}</code>
        {/if}
        <button type="button" class="ghost" onclick={onOpenDir}
          >{$tStore("plugins.empty.cta")}</button
        >
      </div>
    {:else}
      <ul class="plugin-list">
        {#each snap.plugins as p (p.id)}
          <li class="plugin-row" class:has-error={!!p.error}>
            <div class="plugin-head">
              <div class="plugin-meta">
                <h2>
                  {p.manifest?.name ?? p.id}
                  {#if isBundled(p.id)}
                    <span
                      class="bundled-pill"
                      title={t("plugins.installed.bundled_tooltip")}
                      aria-label={t("plugins.installed.bundled_tooltip")}
                    >
                      {t("plugins.installed.bundled_pill")}
                    </span>
                  {/if}
                </h2>
                <p class="muted">
                  {#if p.manifest}
                    <span>{t("plugins.version", { version: p.manifest.version })}</span>
                    {#if p.manifest.author}
                      <span> · {t("plugins.byAuthor", { author: p.manifest.author })}</span>
                    {/if}
                    <span> · <span class="mono">{p.id}</span></span>
                  {:else}
                    <span class="mono">{p.id}</span>
                  {/if}
                </p>
                {#if p.manifest?.description}
                  <p class="desc">{p.manifest.description}</p>
                {/if}
                {#if availableUpdates[p.id]}
                  <button
                    type="button"
                    class="chip update-badge"
                    title={t("plugins.installed.updateBadge", {
                      version: availableUpdates[p.id].version,
                    })}
                    onclick={() => jumpToUpdate(availableUpdates[p.id])}
                  >
                    ↑ v{availableUpdates[p.id].version} —
                    {$tStore("plugins.installed.updateAvailable")}
                  </button>
                {/if}
              </div>
              <div class="plugin-status">
                {#if p.error}
                  <span class="chip err">{$tStore("plugins.error")}</span>
                {:else}
                  <div class="seg" role="radiogroup" aria-label={p.id}>
                    <button
                      type="button"
                      role="radio"
                      aria-checked={!p.enabled}
                      class:tab-active={!p.enabled}
                      disabled={busy[p.id]}
                      onclick={() => toggleEnabled(p, false)}>{$tStore("plugins.disabled")}</button
                    >
                    <button
                      type="button"
                      role="radio"
                      aria-checked={p.enabled}
                      class:tab-active={p.enabled}
                      disabled={busy[p.id]}
                      onclick={() => toggleEnabled(p, true)}>{$tStore("plugins.enabled")}</button
                    >
                  </div>
                {/if}
              </div>
            </div>

            {#if !p.error && p.manifest}
              {@const counts = contribCounts(p)}
              {#if counts.length > 0}
                <div class="contribs">
                  <div class="contrib-chips">
                    {#each counts as c}
                      <span class="chip count">{c.n} {c.label}</span>
                    {/each}
                  </div>
                  <button type="button" class="linkish" onclick={() => toggleExpand(p.id)}>
                    {expanded[p.id] ? $tStore("plugins.collapse") : $tStore("plugins.expand")}
                  </button>
                </div>
                {#if expanded[p.id]}
                  <div class="contrib-detail">
                    {#each p.manifest.contributions.themes as th}
                      <div class="contrib-item">
                        <span class="kind">theme</span>
                        <code>{th.id}</code> — {th.label}
                        {th.dark ? "(dark)" : ""}
                      </div>
                    {/each}
                    {#each p.manifest.contributions.locales as lo}
                      <div class="contrib-item">
                        <span class="kind">locale</span>
                        <code>{lo.locale}</code> — {lo.bundle}
                      </div>
                    {/each}
                    {#each p.manifest.contributions.commands as cm}
                      <div class="contrib-item">
                        <span class="kind">command</span>
                        <code>{cm.id}</code> — {cm.label}
                        {cm.url ? "(url)" : "(shell)"}
                      </div>
                    {/each}
                    {#each p.manifest.contributions.ai_providers as ai}
                      <div class="contrib-item">
                        <span class="kind">ai</span>
                        <code>{ai.id}</code> — {ai.label} ({ai.kind} @ {ai.base_url})
                      </div>
                    {/each}
                    {#each p.manifest.contributions.pdf_actions as pa}
                      <div class="contrib-item">
                        <span class="kind">pdf-action</span>
                        <code>{pa.id}</code> — {pa.label}
                        (<span class="mono">{pa.cli}</span>)
                      </div>
                    {/each}
                    {#if p.manifest.runtime}
                      <div class="permissions-row">
                        <div class="permissions-meta">
                          <span class="kind">{$tStore("plugins.permissions.granted")}</span>
                          <code class="mono">{p.manifest.runtime.entry}</code>
                        </div>
                        <div class="permissions-actions">
                          <button
                            type="button"
                            class="linkish"
                            onclick={() => onReviewPermissions(p)}
                          >
                            {$tStore("plugins.permissions.review")}
                          </button>
                          <button
                            type="button"
                            class="linkish danger"
                            onclick={() => onResetPermissions(p)}
                          >
                            {$tStore("plugins.permissions.reset")}
                          </button>
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}
              {/if}
            {/if}

            {#if p.error}
              <details class="error-detail">
                <summary>{$tStore("plugins.error")}</summary>
                <pre>{p.error}</pre>
                <p class="mono path">{p.dir}</p>
              </details>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if dirPath && snap.plugins.length > 0}
      <p class="config-path">
        <code>{dirPath}</code>
      </p>
    {/if}
  {:else if tab === "browse"}
    <!-- Bench (v1.4.0) → Workshop Marketplace (v2.0.2): Browse tab. -->
    <div class="browse-toolbar">
      <div class="browse-search">
        <span class="browse-search-icon" aria-hidden="true">🔍</span>
        <input
          type="search"
          class="browse-search-input"
          placeholder={$tStore("plugins.browse.searchPlaceholder")}
          aria-label={$tStore("plugins.browse.searchAriaLabel")}
          bind:value={browseQuery}
        />
        {#if browseQuery}
          <button
            type="button"
            class="browse-search-clear"
            aria-label={$tStore("plugins.browse.clearSearch")}
            onclick={() => (browseQuery = "")}
          >
            ✕
          </button>
        {/if}
      </div>

      <select
        class="browse-sort"
        aria-label={$tStore("plugins.browse.sortAriaLabel")}
        bind:value={browseSort}
      >
        <option value="relevance">{$tStore("plugins.browse.sort.relevance")}</option>
        <option value="popular">{$tStore("plugins.browse.sort.popular")}</option>
        <option value="newest">{$tStore("plugins.browse.sort.newest")}</option>
        <option value="name">{$tStore("plugins.browse.sort.name")}</option>
      </select>

      <button
        type="button"
        class="ghost"
        disabled={marketplace.phase === "loading"}
        onclick={() => void refreshMarketplace()}>↻ {$tStore("plugins.browse.refresh")}</button
      >
    </div>

    {#if browseCategories.length > 0}
      <div class="browse-chips" role="tablist" aria-label={$tStore("plugins.browse.categoriesLabel")}>
        <button
          type="button"
          class="browse-chip"
          class:active={browseCategory === null}
          role="tab"
          aria-selected={browseCategory === null}
          onclick={() => (browseCategory = null)}
        >
          {$tStore("plugins.browse.allCategories")}
        </button>
        {#each browseCategories as cat (cat)}
          <button
            type="button"
            class="browse-chip"
            class:active={browseCategory === cat}
            role="tab"
            aria-selected={browseCategory === cat}
            onclick={() => (browseCategory = browseCategory === cat ? null : cat)}
          >
            {cat}
          </button>
        {/each}
      </div>
    {/if}

    {#if marketplace.isStale && marketplace.error}
      <div class="banner banner-warn" role="status">
        {t("plugins.browse.stale", { error: marketplace.error })}
      </div>
    {/if}

    {#if marketplace.isEmbeddedSeed}
      <div class="banner banner-info" role="status">
        {$tStore("plugins.browse.embeddedSeed")}
      </div>
    {/if}

    <!-- v2.0.2 Slice 7: hero card on empty Browse state (no query, no filter,
         index loaded, has entries) -->
    {#if !browseQuery && browseCategory === null && marketplace.index && marketplace.index.plugins.length > 0}
      {@const bundledEntries = marketplace.index.plugins
        .filter((p) => BUNDLED_PLUGIN_IDS.includes(p.id))
        .slice(0, 3)}
      {#if bundledEntries.length > 0}
        <section class="browse-hero" aria-labelledby="browse-hero-title">
          <div class="browse-hero-icon" aria-hidden="true">🧩</div>
          <div class="browse-hero-body">
            <h2 id="browse-hero-title">{$tStore("plugins.browse.hero.title")}</h2>
            <p>{$tStore("plugins.browse.hero.body")}</p>
            <div class="browse-hero-quickpicks" role="group" aria-label={$tStore("plugins.browse.hero.quickpicksLabel")}>
              {#each bundledEntries as quick (quick.id)}
                <button
                  type="button"
                  class="browse-hero-chip"
                  onclick={() => openDrawer(quick)}
                >
                  {quick.name}
                </button>
              {/each}
            </div>
          </div>
        </section>
      {/if}
    {/if}

    {#if marketplace.phase === "loading" && !marketplace.index}
      <div class="empty-state">
        <p>{$tStore("plugins.browse.loading")}</p>
      </div>
    {:else if marketplace.phase === "error"}
      <div class="empty-state error-state">
        <p>
          {t("plugins.browse.fetchError", { error: marketplace.error ?? "unknown error" })}
        </p>
        <button type="button" class="ghost" onclick={() => void refreshMarketplace()}
          >{$tStore("plugins.browse.retry")}</button
        >
      </div>
    {:else if marketplace.index && marketplace.index.plugins.length === 0}
      <div class="empty-state">
        <h2>{$tStore("plugins.browse.empty.title")}</h2>
        <p>{$tStore("plugins.browse.empty.body")}</p>
      </div>
    {:else if marketplace.index}
      {#if browseRanked.length === 0}
        <div class="empty-state">
          <h2>{$tStore("plugins.browse.noMatches.title")}</h2>
          <p>{$tStore("plugins.browse.noMatches.body")}</p>
          {#if browseQuery || browseCategory}
            <button
              type="button"
              class="ghost"
              onclick={() => {
                browseQuery = "";
                browseCategory = null;
              }}>{$tStore("plugins.browse.clearFilters")}</button
            >
          {/if}
        </div>
      {:else}
        <p class="browse-count" aria-live="polite">
          {t("plugins.browse.resultsCount", { count: String(browseResultCount) })}
        </p>
        <div class="market-grid">
          {#each browseRanked as ranked (ranked.entry.id)}
            {@const entry = ranked.entry}
            {@const status = entryStatus(entry)}
            {@const inFlight = !!marketplace.busy[entry.id]}
            <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
            <article
              class="market-card"
              class:status-installed={status === "installed"}
              class:status-update={status === "update"}
              role="button"
              tabindex="0"
              onclick={() => openDrawer(entry)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  openDrawer(entry);
                }
              }}
            >
              <header class="market-card-head">
                <h3>{@html highlightHTML(entry.name, ranked.matches.name)}</h3>
                <span class="chip ver-pill">v{entry.version}</span>
              </header>
              <p class="market-card-meta">
                {@html t("plugins.browse.author", {
                  author: highlightHTML(entry.author, ranked.matches.author),
                })} ·
                <span class="mono">{@html highlightHTML(entry.id, ranked.matches.id)}</span>
              </p>
              <p class="market-card-desc">
                {@html highlightHTML(entry.description, ranked.matches.description)}
              </p>
              {#if (entry.categories ?? []).length > 0 || (entry.tags ?? []).length > 0}
                <div class="market-card-taxonomy">
                  {#each entry.categories ?? [] as c (c)}
                    <span class="chip category-chip">{c}</span>
                  {/each}
                  {#each (entry.tags ?? []).slice(0, 4) as tag (tag)}
                    <span class="chip tag-chip">#{tag}</span>
                  {/each}
                </div>
              {/if}
              {#if (entry.installs ?? 0) >= 10}
                <p class="market-card-installs" aria-label={$tStore("plugins.browse.installsLabel")}>
                  ⬇ {formatInstalls(entry.installs ?? 0)}
                </p>
              {/if}
              <footer class="market-card-foot">
                <span class="market-card-spec mono">
                  {t("plugins.browse.size", { size: formatBytes(entry.size_bytes) })}
                  · {t("plugins.browse.compat", { compat: entry.slab_compat })}
                </span>
                <div
                  class="market-card-actions"
                  role="presentation"
                  onclick={(e) => e.stopPropagation()}
                  onkeydown={(e) => e.stopPropagation()}
                >
                  {#if status === "installed"}
                    <span class="chip status-pill">{$tStore("plugins.browse.installed")}</span>
                    <button
                      type="button"
                      class="ghost danger"
                      disabled={inFlight}
                      onclick={() => onUninstall(entry)}
                    >
                      {inFlight
                        ? $tStore("plugins.browse.uninstalling")
                      : $tStore("plugins.browse.uninstall")}
                  </button>
                {:else if status === "update"}
                  <button
                    type="button"
                    class="primary"
                    disabled={inFlight}
                    onclick={() => onInstall(entry)}
                  >
                    {inFlight
                      ? $tStore("plugins.browse.installing")
                      : t("plugins.browse.update", { version: entry.version })}
                  </button>
                {:else}
                  <button
                    type="button"
                    class="primary"
                    disabled={inFlight}
                    onclick={() => onInstall(entry)}
                  >
                    {inFlight
                      ? $tStore("plugins.browse.installing")
                      : $tStore("plugins.browse.install")}
                  </button>
                {/if}
              </div>
            </footer>
          </article>
        {/each}
      </div>
      {/if}
    {/if}
  {/if}
</section>

<!-- Slice 8 — plugin detail drawer + install progress modal. Both
     mount outside the section so their fixed-position backdrops cover
     the whole panel, not just the scroll container. -->
{#if drawerEntry}
  {@const drawerStatus = entryStatus(drawerEntry)}
  {@const drawerInFlight = !!marketplace.busy[drawerEntry.id]}
  <PluginDetailDrawer
    entry={drawerEntry}
    status={drawerStatus}
    installedVersion={installedVersion[drawerEntry.id]}
    inFlight={drawerInFlight}
    onClose={closeDrawer}
    onAction={() => {
      if (!drawerEntry) return;
      const e = drawerEntry;
      if (drawerStatus === "installed") void onUninstall(e);
      else void onInstall(e);
    }}
  />
{/if}

{#if recentInstallsOpen}
  <RecentInstallsDrawer
    onClose={() => (recentInstallsOpen = false)}
    onPruned={() => {
      // Drawer triggered a prune — refresh the summary so the
      // toolbar count + History-button gating both update.
      void refreshInstallLog();
    }}
  />
{/if}

{#if installModal}
  <InstallProgressModal
    entry={installModal.entry}
    phase={installModal.phase}
    error={installModal.error}
    onDismiss={dismissInstallModal}
  />
{/if}

{#if uninstallModal}
  <UninstallConfirmModal
    name={uninstallModal.entry.name}
    version={uninstallModal.installedVersion}
    id={uninstallModal.entry.id}
    busy={uninstallModal.busy}
    onConfirm={confirmUninstall}
    onCancel={dismissUninstallModal}
  />
{/if}

{#if bulkOverlay}
  <BulkUpdateProgressOverlay
    rows={bulkOverlay.rows}
    currentIndex={bulkOverlay.currentIndex}
    finished={bulkOverlay.finished}
    summary={bulkOverlay.summary}
    onDismiss={dismissBulkOverlay}
  />
{/if}

{#if consentModal && consentModal.plugin.manifest}
  <PluginConsentModal
    manifest={consentModal.plugin.manifest}
    initial={consentModal.initial}
    onApprove={onConsentApprove}
    onDeny={onConsentDeny}
  />
{/if}

<style>
  .plugins-panel {
    max-width: 820px;
    padding: 32px 36px 48px;
    overflow-y: auto;
  }
  .content-header {
    margin-bottom: 18px;
  }
  .content-header h1 {
    margin: 0 0 4px;
    font-size: 20px;
    color: var(--text);
  }
  .subtitle {
    margin: 0;
    color: var(--text-2);
    font-size: 13px;
    line-height: 1.5;
  }
  .toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 18px;
  }
  /* Slice 57 — History button + count chip */
  .history-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .history-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 16px;
    padding: 0 5px;
    border-radius: 8px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-3);
    font-size: 10.5px;
    font-family: var(--font-mono);
    line-height: 1;
  }

  /* ---- Round-15 — Updates available banner --------------------- */
  .updates-banner {
    margin-bottom: 18px;
    border: 1px solid var(--accent);
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--accent) 6%, var(--bg-2));
    overflow: hidden;
  }
  .updates-banner-head {
    display: flex;
    align-items: stretch;
    gap: 8px;
    padding: 10px 12px;
  }
  .updates-toggle {
    flex: 1;
    display: grid;
    grid-template-columns: 14px 14px auto 1fr;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: none;
    padding: 0;
    text-align: left;
    color: var(--text-1);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    min-width: 0;
  }
  .updates-toggle:hover .updates-headline {
    text-decoration: underline;
  }
  .updates-chev {
    color: var(--text-3);
    font-size: 11px;
    line-height: 1;
  }
  .updates-arrow {
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
    line-height: 1;
  }
  .updates-headline {
    font-weight: 600;
    color: var(--text-1);
  }
  .updates-meta {
    color: var(--text-3);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .updates-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .updates-update-all {
    display: inline-flex;
    align-items: center;
    padding: 6px 12px;
    border: 1px solid var(--accent);
    border-radius: var(--r-sm);
    background: var(--accent);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    line-height: 1;
    cursor: pointer;
  }
  .updates-update-all:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .updates-update-all:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .updates-dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-3);
    font-size: 16px;
    font-family: inherit;
    line-height: 1;
    cursor: pointer;
  }
  .updates-dismiss:hover {
    background: var(--bg-3);
    color: var(--text-1);
  }
  .updates-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border);
  }
  .updates-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px 8px 36px; /* indent past the chevron column */
    border-bottom: 1px solid var(--border);
  }
  .updates-row:last-child {
    border-bottom: none;
  }
  .updates-row-meta {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(120px, 1fr) auto auto;
    align-items: baseline;
    gap: 12px;
    min-width: 0;
  }
  .updates-row-name {
    color: var(--text-1);
    font-size: 13px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .updates-row-versions {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    color: var(--text-3);
    font-size: 12px;
    font-family: var(--font-mono);
  }
  .updates-row-prior {
    color: var(--text-3);
    text-decoration: line-through;
    text-decoration-color: var(--text-3);
  }
  .updates-row-arrow {
    color: var(--text-3);
  }
  .updates-row-next {
    color: var(--accent);
    font-weight: 600;
  }
  .updates-row-size {
    color: var(--text-3);
    font-size: 11.5px;
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .updates-row-update {
    display: inline-flex;
    align-items: center;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-1);
    color: var(--text-1);
    font-size: 12px;
    font-family: inherit;
    line-height: 1;
    cursor: pointer;
    flex-shrink: 0;
  }
  .updates-row-update:hover:not(:disabled) {
    background: var(--bg-3);
    border-color: var(--accent);
  }
  .updates-row-update:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .empty-state {
    text-align: center;
    border: 1px dashed var(--border);
    border-radius: var(--r-md);
    padding: 40px 24px;
    background: var(--bg-2);
    margin: 24px 0;
  }
  .empty-state h2 {
    margin: 0 0 8px;
    font-size: 16px;
    color: var(--text);
  }
  .empty-state p {
    margin: 0 0 16px;
    color: var(--text-2);
    font-size: 13px;
    line-height: 1.6;
    max-width: 520px;
    margin-left: auto;
    margin-right: auto;
  }
  .empty-state .path {
    display: inline-block;
    background: var(--bg-3);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-family: var(--font-mono);
    margin-bottom: 16px;
  }
  .plugin-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .plugin-row {
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-2);
    padding: 16px 18px;
    transition: border-color 0.12s;
  }
  .plugin-row:hover {
    border-color: var(--border-strong);
  }
  .plugin-row.has-error {
    border-color: rgba(255, 90, 90, 0.45);
  }
  .plugin-head {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 16px;
    align-items: start;
  }
  .plugin-meta h2 {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .plugin-meta .muted {
    margin: 0 0 6px;
    color: var(--text-3);
    font-size: 11px;
  }
  .plugin-meta .desc {
    margin: 0;
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.5;
  }
  .chip {
    display: inline-block;
    font-size: 11px;
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-2);
    padding: 2px 8px;
    border-radius: 999px;
    font-family: var(--font-mono);
  }
  .chip.err {
    background: rgba(255, 90, 90, 0.1);
    border-color: rgba(255, 90, 90, 0.35);
    color: var(--danger);
  }
  .chip.count {
    background: var(--bg-3);
    font-family: inherit;
  }
  .contribs {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .contrib-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .linkish {
    background: transparent;
    border: none;
    color: var(--accent);
    font-size: 12px;
    cursor: pointer;
    padding: 4px 6px;
  }
  .linkish:hover {
    text-decoration: underline;
  }
  .contrib-detail {
    margin-top: 8px;
    padding: 10px 12px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .contrib-item {
    font-size: 11px;
    color: var(--text-2);
    line-height: 1.6;
  }
  .contrib-item .kind {
    display: inline-block;
    min-width: 80px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 10px;
    font-weight: 600;
  }
  .contrib-item code {
    background: var(--bg-2);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 11px;
    color: var(--text);
  }
  .permissions-row {
    margin-top: 6px;
    padding-top: 8px;
    border-top: 1px dashed var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .permissions-meta {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 11px;
    color: var(--text-2);
  }
  .permissions-meta .kind {
    min-width: 80px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 10px;
    font-weight: 600;
  }
  .permissions-meta code {
    background: var(--bg-2);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 11px;
    color: var(--text);
  }
  .permissions-actions {
    display: flex;
    gap: 12px;
  }
  .permissions-actions .linkish {
    font-size: 12px;
  }
  .permissions-actions .linkish.danger {
    color: var(--danger, #e54);
  }
  .error-detail {
    margin-top: 12px;
    padding: 10px 12px;
    background: rgba(255, 90, 90, 0.08);
    border: 1px solid rgba(255, 90, 90, 0.25);
    border-radius: 6px;
  }
  .error-detail summary {
    cursor: pointer;
    font-size: 12px;
    color: var(--danger);
    font-weight: 600;
  }
  .error-detail pre {
    margin: 8px 0 4px;
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-2);
  }
  .error-detail .path {
    margin: 0;
    font-size: 10px;
    color: var(--text-3);
  }
  .mono {
    font-family: var(--font-mono);
  }
  .config-path {
    margin-top: 24px;
    color: var(--text-3);
    font-size: 11px;
  }
  .config-path code {
    background: var(--bg-3);
    border: 1px solid var(--border);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
  }

  /* Segmented control — match Settings panel conventions. */
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-3);
    overflow: hidden;
  }
  .seg button {
    background: transparent;
    border: none;
    color: var(--text-2);
    padding: 6px 12px;
    font-size: 12px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .seg button:hover:not(:disabled) {
    background: color-mix(in oklab, var(--accent) 8%, transparent);
  }
  .seg button.tab-active {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    color: var(--text);
  }
  .seg button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* ===== Bench (v1.4.0) — tab strip + Browse tab ===== */

  .tab-strip {
    display: flex;
    margin-bottom: 18px;
  }
  .tab-strip button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 9px 16px;
    font-size: 13px;
    font-weight: 500;
  }
  .tab-strip .tab-count {
    display: inline-block;
    min-width: 22px;
    text-align: center;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-3);
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    font-family: var(--font-mono);
  }
  .tab-strip button.tab-active .tab-count {
    background: color-mix(in oklab, var(--accent) 20%, transparent);
    border-color: color-mix(in oklab, var(--accent) 30%, var(--border));
    color: var(--text);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-2);
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.5;
    margin-bottom: 14px;
  }
  .banner-warn {
    background: rgba(255, 180, 60, 0.08);
    border-color: rgba(255, 180, 60, 0.32);
    color: var(--text);
  }
  .banner-info {
    background: rgba(96, 165, 250, 0.08);
    border-color: rgba(96, 165, 250, 0.32);
    color: var(--text);
  }

  /* ---- v2.0.2 Workshop Marketplace — Browse toolbar / chips / sort ---- */
  .browse-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .browse-search {
    position: relative;
    flex: 1 1 280px;
    min-width: 200px;
    display: flex;
    align-items: center;
  }
  .browse-search-icon {
    position: absolute;
    left: 12px;
    pointer-events: none;
    opacity: 0.55;
    font-size: 13px;
  }
  .browse-search-input {
    width: 100%;
    padding: 9px 36px 9px 34px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-1);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    transition: border-color 120ms ease, box-shadow 120ms ease;
  }
  .browse-search-input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 25%, transparent);
  }
  .browse-search-clear {
    position: absolute;
    right: 6px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    padding: 4px 8px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .browse-search-clear:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .browse-sort {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-1);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  .browse-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 14px;
  }
  .browse-chip {
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-2);
    padding: 5px 12px;
    border-radius: 999px;
    font-size: 12px;
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease, border-color 120ms ease;
  }
  .browse-chip:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .browse-chip.active {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .browse-count {
    margin: 0 0 10px;
    font-size: 12px;
    color: var(--text-2);
  }

  /* v2.0.2 Slice 7 — hero card on empty Browse */
  .browse-hero {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    margin: 0 0 16px;
    padding: 14px 16px;
    border: 1px solid color-mix(in oklab, var(--accent) 30%, transparent);
    border-radius: var(--r-md);
    background: color-mix(in oklab, var(--accent) 8%, var(--bg-2));
  }
  .browse-hero-icon {
    font-size: 28px;
    line-height: 1;
    flex: 0 0 auto;
    margin-top: 2px;
  }
  .browse-hero-body {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .browse-hero-body h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .browse-hero-body p {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .browse-hero-quickpicks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .browse-hero-chip {
    appearance: none;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    font-size: 12px;
    padding: 4px 10px;
    border-radius: 999px;
    cursor: pointer;
    transition: background 80ms ease, border-color 80ms ease, color 80ms ease;
  }
  .browse-hero-chip:hover {
    background: color-mix(in oklab, var(--accent) 16%, var(--bg-2));
    border-color: color-mix(in oklab, var(--accent) 40%, var(--border));
  }
  .browse-hero-chip:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* Highlighted matched substrings inside Browse cards. The mark
     wrapper is generated by `highlightHTML` in $lib/marketplace/fuzzy. */
  .market-card :global(mark) {
    background: color-mix(in oklab, var(--accent) 25%, transparent);
    color: var(--text);
    border-radius: 3px;
    padding: 0 2px;
  }

  .market-card-taxonomy {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0 0;
  }
  .category-chip {
    background: color-mix(in oklab, var(--accent) 14%, transparent);
    border-color: color-mix(in oklab, var(--accent) 26%, transparent);
    color: var(--text);
    font-size: 11px;
  }
  .tag-chip {
    background: var(--bg-2);
    color: var(--text-2);
    font-size: 11px;
  }
  .market-card-installs {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--text-2);
  }

  .error-state p {
    color: var(--danger);
  }

  .market-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 12px;
  }
  @media (min-width: 640px) {
    .market-grid {
      grid-template-columns: 1fr 1fr;
    }
  }

  .market-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-2);
    padding: 14px 16px;
    transition:
      border-color 0.12s,
      box-shadow 0.12s,
      transform 0.12s;
  }
  .market-card:hover {
    border-color: var(--border-strong);
    box-shadow: 0 1px 8px rgba(0, 0, 0, 0.04);
  }
  .market-card.status-installed {
    border-color: color-mix(in oklab, var(--accent) 22%, var(--border));
  }
  .market-card.status-update {
    border-color: rgba(255, 180, 60, 0.45);
    box-shadow: 0 0 0 1px rgba(255, 180, 60, 0.15);
  }
  .market-card-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }
  .market-card-head h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    line-height: 1.3;
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ver-pill {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-2);
    flex-shrink: 0;
  }
  .market-card-meta {
    margin: 0;
    font-size: 11px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .market-card-desc {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.5;
    /* Clamp to 3 lines for visual consistency in the grid. */
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .market-card-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: auto;
    padding-top: 8px;
    flex-wrap: wrap;
  }
  .market-card-spec {
    font-size: 10.5px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .market-card-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .status-pill {
    color: color-mix(in oklab, var(--accent) 80%, var(--text));
    border-color: color-mix(in oklab, var(--accent) 35%, var(--border));
    background: color-mix(in oklab, var(--accent) 8%, var(--bg-3));
    font-family: inherit;
  }
  .market-card-actions .primary {
    padding: 6px 12px;
    font-size: 12px;
  }
  .market-card-actions .ghost.danger {
    padding: 6px 12px;
    font-size: 12px;
    color: var(--text-2);
  }
  .market-card-actions .ghost.danger:hover:not(:disabled) {
    color: var(--danger);
    border-color: rgba(255, 90, 90, 0.45);
  }

  /* ===== Slice 8 — interactive market cards + update badge ===== */

  /* The whole card is a click target now (opens the detail drawer),
   * so cue it visually with a pointer + slightly stronger hover.
   * Buttons inside still feel like buttons because they keep their
   * own borders + the action wrapper stops click propagation. */
  .market-card[role="button"] {
    cursor: pointer;
  }
  .market-card[role="button"]:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .market-card[role="button"]:hover {
    border-color: var(--border-strong);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.06);
  }

  /* Amber pill on Installed-tab rows when the marketplace knows about
   * a newer version. Matches the .status-update card styling so the
   * two surfaces visually agree on "this needs attention." */
  .update-badge {
    background: rgba(255, 180, 60, 0.12);
    border-color: rgba(255, 180, 60, 0.45);
    color: var(--text);
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    margin-top: 8px;
    padding: 3px 10px;
    transition:
      background 0.12s,
      border-color 0.12s;
  }
  .update-badge:hover {
    background: rgba(255, 180, 60, 0.22);
    border-color: rgba(255, 180, 60, 0.6);
  }
  .update-badge:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* v2.0.1 — "Bundled" pill for first-party plugins seeded by the
     binary on first boot. Subtle: doesn't try to compete with the
     plugin name, but is visible enough to answer "why is this here?". */
  .bundled-pill {
    display: inline-block;
    padding: 1px 6px;
    margin-left: 8px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    vertical-align: middle;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: 999px;
    cursor: help;
  }
</style>
