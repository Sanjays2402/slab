<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listRecent, formatRelTime, type RecentFile } from "$lib/recent";
  import { setUiConfig, ACCENT_COLORS, type ThemeMode, type Density } from "$lib/theme";
  import { recordMru, mruRanks, clearMru } from "$lib/cmdMru";
  import { openPanelWindow } from "$lib/windows";
  import { isInTauri } from "$lib/tauri";

  // Cabinet (v1.1.0) Slice 5 — panels that can be detached into their own
  // native window. Must stay in sync with `DETACHABLE_PANELS` in
  // `+page.svelte`. Duplicated as a typed string-literal union here so the
  // palette doesn't have to import that route module at runtime.
  const DETACHABLE_PANELS = new Set<string>([
    "reader",
    "library",
    "beacon",
    "search",
    "pii",
    "pages",
    "pages-list",
    "diff",
    "slides",
    "tables",
    "markdown",
  ]);

  type Action = {
    id: string;
    title: string;
    subtitle?: string;
    icon: string;
    group: string;
    run: () => void;
    keywords?: string;
  };

  type Props = {
    open: boolean;
    panels: { id: string; label: string; icon: string; ready: boolean }[];
    activePanel: string;
    onClose: () => void;
    onSelectPanel: (id: string) => void;
    onOpenRecent: (file: RecentFile) => void;
    onShowShortcuts?: () => void;
  };

  let {
    open = $bindable(false),
    panels,
    activePanel,
    onClose,
    onSelectPanel,
    onOpenRecent,
    onShowShortcuts,
  }: Props = $props();

  let query = $state("");
  let selected = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();
  let recents = $state<RecentFile[]>([]);
  // Glass Slice 5: MRU ranks for actions (id → rank, lower = more recent).
  let mru = $state<Record<string, number>>({});

  function refreshRecents() {
    recents = listRecent();
  }
  function refreshMru() {
    mru = mruRanks();
  }

  $effect(() => {
    if (open) {
      query = "";
      selected = 0;
      refreshRecents();
      refreshMru();
      // focus next tick once mounted
      queueMicrotask(() => inputEl?.focus());
    }
  });

  // Build the action list each time inputs change.
  let actions = $derived.by<Action[]>(() => {
    const out: Action[] = [];
    for (const p of panels) {
      if (!p.ready) continue;
      out.push({
        id: `panel:${p.id}`,
        title: p.label,
        subtitle: p.id === activePanel ? "Current panel" : `Switch to ${p.label}`,
        icon: p.icon,
        group: "Panels",
        run: () => onSelectPanel(p.id),
        keywords: `${p.label} panel ${p.id}`,
      });
    }
    // Cabinet Slice 5: "Open <panel> in new window" — only inside Tauri
    // (no detached windows in vanilla browser dev), and only for panels
    // that have a real detached experience.
    if (isInTauri()) {
      for (const p of panels) {
        if (!p.ready) continue;
        if (!DETACHABLE_PANELS.has(p.id)) continue;
        out.push({
          id: `panel-window:${p.id}`,
          title: `Open ${p.label} in new window`,
          subtitle: "Detach into its own native window",
          icon: "⤢",
          group: "Windows",
          run: () => {
            void openPanelWindow(p.id);
          },
          keywords: `${p.label} ${p.id} detach window new open float separate panel cabinet`,
        });
      }
    }
    for (const r of recents) {
      out.push({
        id: `recent:${r.path}`,
        title: r.name,
        subtitle: `Open · ${formatRelTime(r.openedAt)}${r.pageCount ? ` · ${r.pageCount} pages` : ""}`,
        icon: "▥",
        group: "Recent files",
        run: () => onOpenRecent(r),
        keywords: `${r.name} ${r.path} pdf recent`,
      });
    }
    // Theme quick actions
    const themes: { id: ThemeMode; label: string; icon: string }[] = [
      { id: "auto", label: "Auto (match system)", icon: "◐" },
      { id: "light", label: "Light", icon: "☀" },
      { id: "dark", label: "Dark", icon: "☾" },
    ];
    for (const t of themes) {
      out.push({
        id: `theme:${t.id}`,
        title: `Theme: ${t.label}`,
        subtitle: "Switch appearance",
        icon: t.icon,
        group: "Appearance",
        run: () => void setUiConfig({ theme: t.id }),
        keywords: `theme appearance ${t.id} ${t.label} light dark auto`,
      });
    }
    for (const a of ACCENT_COLORS) {
      out.push({
        id: `accent:${a.id}`,
        title: `Accent: ${a.label}`,
        subtitle: a.hex,
        icon: "●",
        group: "Appearance",
        run: () => void setUiConfig({ accent: a.id }),
        keywords: `accent color ${a.id} ${a.label}`,
      });
    }
    const densities: { id: Density; label: string; icon: string }[] = [
      { id: "comfortable", label: "Comfortable", icon: "▭" },
      { id: "compact", label: "Compact", icon: "▬" },
    ];
    for (const d of densities) {
      out.push({
        id: `density:${d.id}`,
        title: `Density: ${d.label}`,
        subtitle: "Spacing",
        icon: d.icon,
        group: "Appearance",
        run: () => void setUiConfig({ density: d.id }),
        keywords: `density spacing ${d.id} ${d.label}`,
      });
    }
    if (onShowShortcuts) {
      out.push({
        id: "help:shortcuts",
        title: "Keyboard shortcuts",
        subtitle: "Show the full reference (?)",
        icon: "⌨",
        group: "Help",
        run: () => onShowShortcuts!(),
        keywords: "keyboard shortcuts help reference cheatsheet bindings",
      });
    }
    // Glass Slice 7: jump straight to the customisation panel.
    out.push({
      id: "settings:keymap",
      title: "Customize keyboard shortcuts",
      subtitle: "Rebind any global action",
      icon: "⌨",
      group: "Settings",
      run: () => onSelectPanel("keymap"),
      keywords: "shortcuts keymap rebind customize keys hotkeys bindings",
    });
    // Glass Slice 6: re-trigger the onboarding tour
    out.push({
      id: "help:onboarding",
      title: "Show onboarding tour",
      subtitle: "5-step walkthrough",
      icon: "🍰",
      group: "Help",
      run: () => {
        window.dispatchEvent(new CustomEvent("slab:show-onboarding"));
      },
      keywords: "onboarding tour welcome walkthrough tutorial intro first launch",
    });
    // Glass Slice 5: MRU management
    if (Object.keys(mru).length > 0) {
      out.push({
        id: "settings:clear-mru",
        title: "Clear command history",
        subtitle: `${Object.keys(mru).length} remembered command${Object.keys(mru).length === 1 ? "" : "s"}`,
        icon: "↺",
        group: "Settings",
        run: () => {
          clearMru();
          mru = {};
        },
        keywords: "clear mru reset recent commands history forget",
      });
    }
    return out;
  });

  // Lightweight fuzzy match: every character of the query must appear in order
  // in the haystack. Score = (matched / haystack.length), with bonus for
  // prefix and contiguous matches.
  function fuzzyScore(q: string, hay: string): number {
    if (!q) return 1;
    const Q = q.toLowerCase();
    const H = hay.toLowerCase();
    if (H.startsWith(Q)) return 2 + 1 / H.length;
    if (H.includes(Q)) return 1.5 + 1 / H.length;
    let qi = 0;
    let lastIdx = -1;
    let contiguous = 0;
    let bestContiguous = 0;
    for (let hi = 0; hi < H.length && qi < Q.length; hi++) {
      if (H[hi] === Q[qi]) {
        if (hi === lastIdx + 1) contiguous++;
        else contiguous = 1;
        bestContiguous = Math.max(bestContiguous, contiguous);
        lastIdx = hi;
        qi++;
      }
    }
    if (qi < Q.length) return 0;
    return 1 + bestContiguous / Q.length + 0.1 / H.length;
  }

  let filtered = $derived.by(() => {
    if (!query.trim()) {
      // Empty-query view: MRU floats to top, in MRU order.
      // Actions not in MRU keep their natural order after.
      const recent: Action[] = [];
      const rest: Action[] = [];
      for (const a of actions) {
        if (a.id in mru) recent.push(a);
        else rest.push(a);
      }
      recent.sort((a, b) => mru[a.id] - mru[b.id]);
      // Cap the "Recent" pseudo-group to the 6 most recent, the rest fall back
      // into their natural group so the palette doesn't feel front-loaded.
      const top = recent.slice(0, 6);
      const overflow = recent.slice(6);
      return [...top, ...overflow, ...rest];
    }
    const q = query.trim();
    const scored = actions
      .map((a) => ({ a, score: fuzzyScore(q, `${a.title} ${a.keywords ?? ""}`) }))
      .filter((x) => x.score > 0)
      .sort((a, b) => {
        // Primary: fuzzy score. Tie-breaker: MRU rank (lower = more recent).
        if (b.score !== a.score) return b.score - a.score;
        const ar = mru[a.a.id] ?? 9999;
        const br = mru[b.a.id] ?? 9999;
        return ar - br;
      });
    return scored.map((x) => x.a);
  });

  // Group preserving filtered order. When query is empty AND there are MRU
  // entries, the first N items get pulled into a synthetic "Recently used"
  // group so the user sees their muscle-memory commands first.
  let grouped = $derived.by(() => {
    const map = new Map<string, Action[]>();
    const showMruHeader = !query.trim() && Object.keys(mru).length > 0;
    let mruShown = 0;
    const mruCap = 6;
    for (const a of filtered) {
      if (showMruHeader && a.id in mru && mruShown < mruCap) {
        const key = "Recently used";
        if (!map.has(key)) map.set(key, []);
        map.get(key)!.push(a);
        mruShown++;
        continue;
      }
      if (!map.has(a.group)) map.set(a.group, []);
      map.get(a.group)!.push(a);
    }
    return Array.from(map.entries());
  });

  // Clamp selection when filter shrinks list
  $effect(() => {
    if (selected >= filtered.length) selected = Math.max(0, filtered.length - 1);
  });

  function runSelected() {
    const a = filtered[selected];
    if (!a) return;
    // Record into MRU before running so the next palette open shows it on top.
    // Skip the "clear MRU" action itself so it doesn't become its own bait.
    if (a.id !== "settings:clear-mru") recordMru(a.id);
    onClose();
    queueMicrotask(() => a.run());
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      selected = Math.min(filtered.length - 1, selected + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selected = Math.max(0, selected - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      runSelected();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
  });
</script>

{#if open}
  <div class="palette-scrim" onclick={onClose} role="presentation"></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
    <div class="palette-input-row">
      <span class="palette-kbd-leading">⌘K</span>
      <input
        bind:this={inputEl}
        bind:value={query}
        placeholder="Jump to anything…"
        autocomplete="off"
        spellcheck="false"
      />
      <button class="palette-close" onclick={onClose} title="Close (Esc)">esc</button>
    </div>
    <div class="palette-list">
      {#if filtered.length === 0}
        <div class="palette-empty">No matches for “{query}”</div>
      {:else}
        {#each grouped as [group, items] (group)}
          <div class="palette-group-label">{group}</div>
          {#each items as a (a.id)}
            {@const idx = filtered.indexOf(a)}
            <button
              class="palette-item"
              class:active={idx === selected}
              onmouseenter={() => (selected = idx)}
              onclick={runSelected}
            >
              <span class="palette-icon">{a.icon}</span>
              <span class="palette-text">
                <span class="palette-title">{a.title}</span>
                {#if a.subtitle}<span class="palette-subtitle">{a.subtitle}</span>{/if}
              </span>
              {#if idx === selected}<span class="palette-enter">↵</span>{/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>
    <div class="palette-footer">
      <span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
      <span><kbd>↵</kbd> select</span>
      <span><kbd>esc</kbd> close</span>
    </div>
  </div>
{/if}

<style>
  .palette-scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(4px);
    z-index: 90;
  }
  .palette {
    position: fixed;
    top: 14vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(620px, 92vw);
    max-height: 70vh;
    z-index: 100;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .palette-input-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .palette-input-row input {
    flex: 1;
    background: transparent;
    border: 0;
    outline: 0;
    color: var(--text);
    font-size: 15px;
    padding: 4px 0;
  }
  .palette-kbd-leading {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    background: var(--bg-3);
    border: 1px solid var(--border);
    padding: 3px 6px;
    border-radius: 4px;
    letter-spacing: 0.5px;
  }
  .palette-close {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-3);
    border-radius: 4px;
    padding: 3px 7px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .palette-list {
    flex: 1;
    overflow-y: auto;
    padding: 6px 4px;
  }
  .palette-empty {
    padding: 20px;
    text-align: center;
    color: var(--text-3);
    font-size: 13px;
  }
  .palette-group-label {
    padding: 8px 12px 4px;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.6px;
  }
  .palette-item {
    width: 100%;
    background: transparent;
    border: 0;
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-radius: var(--r-sm);
    text-align: left;
    cursor: pointer;
  }
  .palette-item.active {
    background: var(--bg-3);
    color: var(--text);
  }
  .palette-icon {
    width: 20px;
    text-align: center;
    color: var(--accent);
    font-size: 14px;
  }
  .palette-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .palette-title {
    font-size: 13px;
    color: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .palette-subtitle {
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .palette-enter {
    font-size: 12px;
    color: var(--accent);
  }
  .palette-footer {
    display: flex;
    gap: 14px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    font-size: 10px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .palette-footer kbd {
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 4px;
    margin-right: 3px;
    font-family: inherit;
  }
</style>
