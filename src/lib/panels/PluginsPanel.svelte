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
    type Plugin,
    type PluginsSnapshot,
  } from "$lib/plugins";
  import { notify } from "$lib/notify";
  import { tStore, t } from "$lib/i18n";

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

  $effect(() => {
    const unsub = pluginsStore.subscribe((v) => (snap = v));
    return unsub;
  });

  onMount(() => {
    // Layout boot already refreshes plugins, but call here too so the
    // panel works even if the user opens it before boot completes.
    void refreshPlugins();
    pluginsDir()
      .then((p) => (dirPath = p))
      .catch(() => (dirPath = null));
  });

  async function toggleEnabled(p: Plugin, next: boolean) {
    if (busy[p.id]) return;
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
              <h2>{p.manifest?.name ?? p.id}</h2>
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
</section>

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
</style>
