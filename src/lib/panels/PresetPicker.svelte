<!--
  Preset Picker modal (v3.35.0 "Atlas Presets").

  Renders a grid of built-in smart-collection templates the user can
  add to their library in one click. Used by:
    - CollectionsSidebar (the "+ Preset" button next to "Smart")
    - CommandPalette ("Add smart collection from preset…")
    - Keyboard shortcut Cmd/Ctrl+Shift+P (registered in App.svelte)

  Each preset card shows icon (emoji-fallback), name, description, and
  an Add button. Already-applied presets are disabled with a small
  "Added" tag — they can be re-added only after the user deletes the
  matching smart collection.

  All work is local — no network, no AI. Perfect for offline demos.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    presetList,
    presetApply,
    presetAlreadyApplied,
    personalPresetList,
    personalPresetApply,
    personalPresetDelete,
    personalPresetsExport,
    personalPresetsImport,
    type PresetInfo,
    type PersonalPresetRecord,
  } from "$lib/library";

  let {
    onClose = () => {},
    onApplied = (_: PresetInfo) => {},
  }: {
    onClose?: () => void;
    onApplied?: (preset: PresetInfo) => void;
  } = $props();

  let presets = $state<PresetInfo[]>([]);
  let personal = $state<PersonalPresetRecord[]>([]);
  let applied = $state<Set<string>>(new Set());
  let busyId = $state<string | null>(null);
  let busyPersonalId = $state<number | null>(null);
  let error = $state<string | null>(null);
  let info = $state<string | null>(null);
  let loading = $state(true);
  let query = $state("");

  async function refresh() {
    const [list, alreadyIds, mine] = await Promise.all([
      presetList(),
      presetAlreadyApplied(),
      personalPresetList(),
    ]);
    presets = list;
    applied = new Set(alreadyIds);
    personal = mine;
  }

  onMount(async () => {
    try {
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  let filtered = $derived(() => {
    const q = query.trim().toLowerCase();
    if (!q) return presets;
    return presets.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q),
    );
  });

  async function addPreset(p: PresetInfo) {
    if (applied.has(p.id) || busyId) return;
    busyId = p.id;
    error = null;
    try {
      await presetApply(p.id);
      applied = new Set([...applied, p.id]);
      onApplied(p);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busyId = null;
    }
  }

  async function addPersonal(p: PersonalPresetRecord) {
    if (busyPersonalId) return;
    busyPersonalId = p.id;
    error = null;
    info = null;
    try {
      const sc = await personalPresetApply(p.id);
      info = `Added “${sc.name}”.`;
      onApplied({
        id: `personal-${p.id}`,
        name: sc.name,
        icon: p.icon ?? "📁",
        color: p.color ?? "#3b82f6",
        description: p.description ?? "",
      });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busyPersonalId = null;
    }
  }

  async function deletePersonal(p: PersonalPresetRecord) {
    if (
      !confirm(
        `Delete personal preset “${p.name}”? (Existing smart collections built from it are NOT affected.)`,
      )
    )
      return;
    error = null;
    try {
      await personalPresetDelete(p.id);
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function exportPack() {
    if (personal.length === 0) {
      error = "No personal presets to export yet. Save one first.";
      return;
    }
    try {
      const json = await personalPresetsExport([]);
      // Browser-style download — works in Tauri webview too.
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `slab-presets-${new Date().toISOString().slice(0, 10)}.slabpresets`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      info = `Exported ${personal.length} preset(s).`;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  function importPackClick() {
    const inp = document.createElement("input");
    inp.type = "file";
    inp.accept = ".slabpresets,application/json";
    inp.onchange = async () => {
      const file = inp.files?.[0];
      if (!file) return;
      try {
        const text = await file.text();
        const report = await personalPresetsImport(text, true);
        info =
          `Imported ${report.imported}` +
          (report.renamed ? `, renamed ${report.renamed}` : "") +
          (report.skipped ? `, skipped ${report.skipped}` : "") +
          (report.errors.length ? `, ${report.errors.length} error(s)` : "") +
          ".";
        await refresh();
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      }
    };
    inp.click();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  // Simple icon mapping — the backend ships icon names, we render
  // emoji fallbacks so we don't ship a heavyweight icon set just
  // for the picker.
  const iconEmoji: Record<string, string> = {
    sparkles: "✨",
    "receipt-text": "🧾",
    "file-text": "📄",
    "scan-line": "🧮",
    "file-signature": "✍️",
    scale: "⚖️",
    "book-open": "📖",
    book: "📚",
    scan: "🖨️",
    "tag-off": "🏷️",
  };
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="pp-backdrop"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
>
  <div class="pp-modal" role="dialog" aria-modal="true" aria-labelledby="pp-title">
    <header class="pp-head">
      <div>
        <h2 id="pp-title">Add a smart collection from a preset</h2>
        <p class="pp-sub">
          One click → a fully-rigged smart collection. Tags are
          auto-created. Edit the rules anytime after.
        </p>
      </div>
      <button class="pp-close" aria-label="Close" onclick={onClose}>×</button>
    </header>

    <div class="pp-search">
      <input
        type="search"
        placeholder="Search presets…"
        bind:value={query}
        autofocus
      />
    </div>

    {#if error}
      <div class="pp-error">{error}</div>
    {/if}
    {#if info}
      <div class="pp-info">{info}</div>
    {/if}

    {#if loading}
      <div class="pp-empty">Loading presets…</div>
    {:else}
      {#if personal.length > 0}
        <h3 class="pp-section">★ Personal presets</h3>
        <ul class="pp-grid">
          {#each personal as p (p.id)}
            {@const busy = busyPersonalId === p.id}
            <li class="pp-card personal">
              <div
                class="pp-icon"
                style="background:{(p.color ?? '#3b82f6')}22; color:{p.color ??
                  '#3b82f6'};"
              >
                <span aria-hidden="true">{p.icon ?? "📌"}</span>
              </div>
              <div class="pp-body">
                <div class="pp-name">{p.name}</div>
                <div class="pp-desc">{p.description ?? "Personal preset"}</div>
              </div>
              <div class="pp-actions">
                <button
                  class="pp-add"
                  disabled={busy}
                  onclick={() => addPersonal(p)}
                >
                  {busy ? "Adding…" : "+ Add"}
                </button>
                <button
                  class="pp-del"
                  title="Delete personal preset"
                  aria-label="Delete personal preset"
                  onclick={() => deletePersonal(p)}
                >
                  ×
                </button>
              </div>
            </li>
          {/each}
        </ul>
        <h3 class="pp-section">Built-in presets</h3>
      {/if}
      {#if filtered().length === 0}
        <div class="pp-empty">No built-in presets match “{query}”.</div>
      {:else}
        <ul class="pp-grid">
        {#each filtered() as p (p.id)}
          {@const added = applied.has(p.id)}
          {@const busy = busyId === p.id}
          <li class="pp-card" class:added>
            <div class="pp-icon" style="background:{p.color}22; color:{p.color};">
              <span aria-hidden="true">{iconEmoji[p.icon] ?? "📁"}</span>
            </div>
            <div class="pp-body">
              <div class="pp-name">{p.name}</div>
              <div class="pp-desc">{p.description}</div>
            </div>
            <button
              class="pp-add"
              disabled={added || busy}
              onclick={() => addPreset(p)}
            >
              {#if added}
                Added ✓
              {:else if busy}
                Adding…
              {:else}
                + Add
              {/if}
            </button>
          </li>
        {/each}
        </ul>
      {/if}
    {/if}

    <footer class="pp-foot">
      <div class="pp-foot-left">
        <button class="pp-link" onclick={importPackClick}>Import pack…</button>
        <button class="pp-link" onclick={exportPack}>Export pack…</button>
      </div>
      <span class="pp-hint">Esc to close · ⌘⇧P</span>
      <button class="pp-done" onclick={onClose}>Done</button>
    </footer>
  </div>
</div>

<style>
  .pp-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    animation: fade 120ms ease-out;
  }
  @keyframes fade {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
  .pp-modal {
    width: min(760px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--surface-1, #181a1f);
    color: var(--text-1, #e8e8ec);
    border: 1px solid var(--border-1, rgba(255,255,255,0.08));
    border-radius: 14px;
    box-shadow: 0 30px 60px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }
  .pp-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 18px 20px 8px;
    gap: 16px;
  }
  .pp-head h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .pp-sub {
    margin: 4px 0 0;
    font-size: 12.5px;
    color: var(--text-3, #8e94a3);
    max-width: 540px;
  }
  .pp-close {
    background: transparent;
    border: none;
    color: var(--text-3, #8e94a3);
    font-size: 22px;
    line-height: 1;
    cursor: pointer;
    padding: 0 6px;
    border-radius: 6px;
  }
  .pp-close:hover {
    background: rgba(255,255,255,0.06);
    color: var(--text-1);
  }
  .pp-search {
    padding: 4px 20px 12px;
  }
  .pp-search input {
    width: 100%;
    padding: 9px 12px;
    background: var(--surface-2, #11131a);
    border: 1px solid var(--border-1, rgba(255,255,255,0.08));
    border-radius: 8px;
    color: var(--text-1);
    font-size: 13px;
    outline: none;
  }
  .pp-search input:focus {
    border-color: var(--accent, #7cc4ff);
  }
  .pp-error {
    margin: 0 20px 12px;
    padding: 8px 10px;
    background: rgba(248, 113, 113, 0.12);
    border: 1px solid rgba(248, 113, 113, 0.35);
    color: #fca5a5;
    border-radius: 6px;
    font-size: 12.5px;
  }
  .pp-empty {
    padding: 36px 24px;
    color: var(--text-3);
    text-align: center;
    font-size: 13px;
  }
  .pp-grid {
    list-style: none;
    margin: 0;
    padding: 0 12px 8px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(330px, 1fr));
    gap: 8px;
    overflow-y: auto;
    flex: 1;
  }
  .pp-card {
    display: grid;
    grid-template-columns: 36px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--surface-2, #11131a);
    border: 1px solid var(--border-1, rgba(255,255,255,0.06));
    border-radius: 10px;
    transition: border-color 120ms, background 120ms, transform 120ms;
  }
  .pp-card:hover {
    border-color: var(--border-2, rgba(255,255,255,0.14));
    transform: translateY(-1px);
  }
  .pp-card.added {
    opacity: 0.6;
  }
  .pp-icon {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    font-size: 18px;
  }
  .pp-name {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-1);
  }
  .pp-desc {
    font-size: 11.5px;
    color: var(--text-3, #8e94a3);
    line-height: 1.35;
    margin-top: 2px;
  }
  .pp-add {
    background: var(--accent, #7cc4ff);
    color: #0a0d14;
    border: none;
    padding: 6px 12px;
    border-radius: 6px;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
  }
  .pp-add:hover:not(:disabled) {
    filter: brightness(1.1);
  }
  .pp-add:disabled {
    background: var(--surface-3, rgba(255,255,255,0.08));
    color: var(--text-3);
    cursor: default;
  }
  .pp-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 18px 14px;
    border-top: 1px solid var(--border-1, rgba(255,255,255,0.06));
    margin-top: 6px;
  }
  .pp-hint {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .pp-done {
    background: transparent;
    border: 1px solid var(--border-2, rgba(255,255,255,0.14));
    color: var(--text-1);
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12.5px;
  }
  .pp-done:hover {
    background: rgba(255,255,255,0.05);
  }
</style>
