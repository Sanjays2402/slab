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
  } from "$lib/plugins";
  import {
    marketplaceStore,
    refreshMarketplace,
    installPlugin,
    uninstallPluginById,
    marketplaceAvailable,
    formatBytes,
    compareSemver,
    type IndexEntry,
    type MarketplaceState,
  } from "$lib/marketplace";
  import { notify } from "$lib/notify";
  import { tStore, t } from "$lib/i18n";
  import PluginDetailDrawer from "$lib/components/PluginDetailDrawer.svelte";
  import InstallProgressModal from "$lib/components/InstallProgressModal.svelte";
  import UninstallConfirmModal from "$lib/components/UninstallConfirmModal.svelte";
  import PluginConsentModal from "$lib/components/PluginConsentModal.svelte";

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

  // ---------------- Bench (v1.4.0) — Browse-tab state --------------
  /** Which tab is showing — 'installed' or 'browse'. The Browse tab
   *  fetches the marketplace index lazily on first activation. */
  let tab = $state<"installed" | "browse">("installed");
  let marketplace = $state<MarketplaceState>({
    phase: "idle",
    index: null,
    isStale: false,
    error: null,
    loadedAt: 0,
    busy: {},
  });

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
    });
    pluginsDir()
      .then((p) => (dirPath = p))
      .catch(() => (dirPath = null));
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
    </div>

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
    <!-- Bench (v1.4.0): Browse tab — curated marketplace. -->
    <div class="toolbar">
      <button
        type="button"
        class="ghost"
        disabled={marketplace.phase === "loading"}
        onclick={() => void refreshMarketplace()}>↻ {$tStore("plugins.browse.refresh")}</button
      >
    </div>

    {#if marketplace.isStale && marketplace.error}
      <div class="banner banner-warn" role="status">
        {t("plugins.browse.stale", { error: marketplace.error })}
      </div>
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
      <div class="market-grid">
        {#each marketplace.index.plugins as entry (entry.id)}
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
              <h3>{entry.name}</h3>
              <span class="chip ver-pill">v{entry.version}</span>
            </header>
            <p class="market-card-meta">
              {t("plugins.browse.author", { author: entry.author })} ·
              <span class="mono">{entry.id}</span>
            </p>
            <p class="market-card-desc">{entry.description}</p>
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
