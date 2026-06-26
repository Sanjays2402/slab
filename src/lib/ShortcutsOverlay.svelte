<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { keymapView, prettyBinding, type ActionId } from "$lib/keymap";
  import {
    buildShortcutGroups,
    countShortcutRows,
    type ShortcutInfoSpec,
    type ShortcutRow,
  } from "$lib/shortcutsOverlay";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open = $bindable(false), onClose }: Props = $props();

  // Lexicon (v3.40.0) Slice 1: the bindable rows now come STRAIGHT off
  // the live `keymapView` store via `buildShortcutGroups`, instead of a
  // hand-maintained literal array that silently drifted from the real
  // keymap (a rebind was invisible until someone edited the array; a
  // newly-registered action never appeared at all). The Settings ->
  // Keyboard shortcuts panel writes the same store, so any user rebind
  // shows its custom keys here live, and any action added to the Rust
  // ACTIONS table appears automatically.
  //
  // The only hand-curated part left is a small set of genuinely
  // UN-bindable hints (Esc closes, reader scroll keys, the Theater
  // presenter pen tools, the documented Discovery/Stack panel chords) —
  // things that aren't registered keymap actions but are real, useful
  // shortcuts worth surfacing.

  const IS_MAC =
    typeof navigator !== "undefined" &&
    /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent || "");
  const MOD = IS_MAC ? "⌘" : "Ctrl";

  // Curated, non-keymap-bindable hints. Each carries pre-split display
  // keys. These are intentionally NOT in the Rust keymap (they're
  // panel-local or browser-native), so they live here as static rows.
  const INFO_HINTS: ShortcutInfoSpec[] = [
    { group: "Global", label: "Close current overlay", keys: ["Esc"] },
    { group: "Reading", label: "Scroll one line", keys: ["↑", "↓"] },
    { group: "Reading", label: "Scroll one page", keys: ["PgUp", "PgDn"] },
    { group: "Beacon", label: "Newline in prompt", keys: ["Shift", "↵"] },
    { group: "Theater", label: "Next page", keys: ["→", "Space"] },
    { group: "Theater", label: "Previous page", keys: ["←"] },
    { group: "Theater", label: "Whiteboard", keys: ["W"] },
    { group: "Theater", label: "Toggle laser pointer", keys: ["L"] },
    { group: "Theater", label: "Toggle spotlight cursor", keys: ["."] },
    { group: "Theater", label: "Undo last ink stroke", keys: ["U"] },
    { group: "Theater", label: "Clear all ink strokes", keys: ["C"] },
    { group: "Discovery", label: "Bates numbering panel", keys: [MOD, "Shift", "B"] },
    { group: "Discovery", label: "Legal Stamp panel", keys: [MOD, "Shift", "S"] },
    { group: "Stack", label: "Diff panel (word-level redline)", keys: [MOD, "Shift", "D"] },
    {
      group: "Stack",
      label: "Stack Pro three-way compare",
      keys: [MOD, "Shift", "3"],
    },
  ];

  // Split a canonical binding string into display key-caps. On mac the
  // pretty form is glued ("⌘⇧K") so we split per-modifier-char; elsewhere
  // it's "+"-joined.
  function bindingKeys(s: string): string[] {
    const pretty = prettyBinding(s);
    if (IS_MAC) {
      const out: string[] = [];
      const sym = "⌘⌃⌥⇧";
      let i = 0;
      while (i < pretty.length && sym.includes(pretty[i])) {
        out.push(pretty[i]);
        i++;
      }
      const rest = pretty.slice(i);
      if (rest) out.push(rest);
      return out;
    }
    return pretty.split("+");
  }

  // Resolve a row to its display key-caps: live binding for bindable
  // rows, the curated static keys for info rows.
  function rowKeys(row: ShortcutRow): string[] {
    if (row.actionId) return bindingKeys(row.binding);
    return row.staticKeys;
  }

  let groups = $derived(buildShortcutGroups($keymapView.actions, INFO_HINTS));
  let totalRows = $derived(countShortcutRows(groups));

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
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
  <div class="scrim" onclick={onClose} role="presentation"></div>
  <div class="sheet" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
    <header>
      <div class="title-row">
        <h2>Keyboard shortcuts</h2>
        <span class="count">{totalRows}</span>
      </div>
      <button class="close" onclick={onClose} title="Close (Esc)">esc</button>
    </header>
    <div class="grid">
      {#each groups as g (g.title)}
        <section>
          <h3>{g.title}</h3>
          <ul>
            {#each g.rows as row (row.key)}
              <li>
                <span class="keys">
                  {#each rowKeys(row) as k, j (j)}
                    {#if j > 0}<span class="plus">+</span>{/if}
                    <kbd>{k}</kbd>
                  {/each}
                </span>
                <span class="label">
                  {row.label}
                  {#if row.isOverride}<span class="custom" title="Rebound from default">custom</span>{/if}
                </span>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
    <footer>
      <span>Press <kbd>?</kbd> any time to open this sheet. Rebind any action in Settings → Keyboard shortcuts.</span>
    </footer>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(4px);
    z-index: 90;
  }
  .sheet {
    position: fixed;
    top: 8vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(820px, 94vw);
    max-height: 84vh;
    z-index: 100;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    letter-spacing: 0.2px;
  }
  .count {
    font-size: 10px;
    color: var(--text-3);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 1px 7px;
    font-variant-numeric: tabular-nums;
  }
  .close {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-3);
    border-radius: 4px;
    padding: 3px 7px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    cursor: pointer;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px 28px;
    padding: 16px 20px;
    overflow-y: auto;
  }
  section h3 {
    margin: 10px 0 8px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-3);
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .keys {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    min-width: 140px;
    flex-shrink: 0;
  }
  .label {
    color: var(--text-2);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .custom {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: 3px;
    padding: 1px 4px;
  }
  kbd {
    display: inline-block;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 6px;
    font-family: inherit;
    font-size: 11px;
    color: var(--text);
    line-height: 1.2;
    min-width: 18px;
    text-align: center;
  }
  .plus {
    color: var(--text-3);
    font-size: 10px;
    margin: 0 1px;
  }
  footer {
    padding: 10px 18px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-3);
    background: var(--bg);
  }
  footer kbd {
    margin: 0 2px;
  }
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
