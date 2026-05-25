<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { keymapView, prettyBinding, type ActionId } from "$lib/keymap";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open = $bindable(false), onClose }: Props = $props();

  // Glass Slice 7: render the bindings live from the keymap store
  // rather than a hardcoded array. The Settings → Keyboard shortcuts
  // panel writes to the same store, so any user-rebound action shows
  // its custom keys here without a reload.
  //
  // Curated grouping: we want a richer overlay than just the bindable
  // actions list (e.g. "Esc closes overlays" — not technically
  // bindable, but useful to surface). So we keep a hand-edited array
  // of "info" rows and a parallel set of action-id rows pulled from
  // the live store.

  // Detect macOS once at module load. Used for the "static" rows
  // that aren't part of the bindable action set.
  const IS_MAC =
    typeof navigator !== "undefined" &&
    /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent || "");
  const MOD = IS_MAC ? "⌘" : "Ctrl";

  type Item = { keys: string[]; label: string };
  type Group = { title: string; items: Item[] };

  function bindingKeys(s: string): string[] {
    // Pretty-print then split on the appropriate separator. On mac
    // the pretty form is glued (e.g. "⌘⇧K") so we split per-char;
    // elsewhere it's "+"-joined.
    const pretty = prettyBinding(s);
    if (IS_MAC) {
      // Each modifier char is one symbol; the last segment can be
      // multi-char (e.g. "Tab", "PgUp"). Naive but works because
      // modifiers are always one codepoint.
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

  function lookup(id: ActionId, fallback: string): string {
    const a = $keymapView.actions.find((x) => x.id === id);
    return a ? a.binding : fallback;
  }

  let groups = $derived<Group[]>([
    {
      title: "Global",
      items: [
        { keys: bindingKeys(lookup("palette.open", "Mod+K")), label: "Open command palette" },
        { keys: bindingKeys(lookup("shortcuts.show", "?")), label: "Show keyboard shortcuts" },
        { keys: ["Esc"], label: "Close current overlay" },
      ],
    },
    {
      title: "Tabs (Reader)",
      items: [
        { keys: bindingKeys(lookup("tabs.new", "Mod+T")), label: "Open a PDF in a new tab" },
        { keys: bindingKeys(lookup("tabs.close", "Mod+W")), label: "Close current tab" },
        // Jump-to-tab N — show the canonical 1 + "…" + 9.
        {
          keys: [
            ...bindingKeys(lookup("tabs.goto1", "Mod+1")),
            "…",
            bindingKeys(lookup("tabs.goto9", "Mod+9")).slice(-1)[0] ?? "9",
          ],
          label: "Jump to tab N",
        },
        { keys: bindingKeys(lookup("tabs.next", "Ctrl+Tab")), label: "Next tab" },
        { keys: bindingKeys(lookup("tabs.prev", "Ctrl+Shift+Tab")), label: "Previous tab" },
      ],
    },
    {
      title: "Reading",
      items: [
        { keys: bindingKeys(lookup("find.open", "Mod+F")), label: "Find in document" },
        { keys: bindingKeys(lookup("zoom.in", "Mod++")), label: "Zoom in" },
        { keys: bindingKeys(lookup("zoom.out", "Mod+-")), label: "Zoom out" },
        { keys: ["↑", "↓"], label: "Scroll one line" },
        { keys: ["PgUp", "PgDn"], label: "Scroll one page" },
      ],
    },
    {
      title: "Beacon (AI chat)",
      items: [
        { keys: bindingKeys(lookup("beacon.send", "Mod+Enter")), label: "Send message" },
        { keys: ["Shift", "↵"], label: "Newline in prompt" },
      ],
    },
    {
      title: "Theater (presenter mode)",
      items: [
        { keys: [MOD, "Shift", "T"], label: "Open Theater panel" },
        { keys: ["→", "Space"], label: "Next page" },
        { keys: ["←"], label: "Previous page" },
        { keys: ["B"], label: "Blackout audience" },
        { keys: ["W"], label: "Whiteboard" },
        { keys: ["L"], label: "Toggle laser pointer" },
        { keys: ["I"], label: "Toggle ink mode" },
        { keys: ["."], label: "Toggle spotlight cursor" },
        { keys: ["U"], label: "Undo last ink stroke" },
        { keys: ["C"], label: "Clear all ink strokes" },
        { keys: ["Esc"], label: "Exit presentation" },
      ],
    },
    {
      title: "Discovery (Bates & Stamps)",
      items: [
        { keys: [MOD, "Shift", "B"], label: "Open Bates numbering panel" },
        { keys: [MOD, "Shift", "S"], label: "Open Legal Stamp panel" },
      ],
    },
    {
      title: "Hopper (folder automation)",
      items: [
        { keys: [MOD, "Shift", "H"], label: "Backfill folder with current rules" },
      ],
    },
    {
      // v3.28.0 Quill Hub: one panel, four sub-tabs. The shortcuts each
      // open the Forms hub on the corresponding tab — no more memorising
      // four separate routes.
      title: "Forms (Quill Hub)",
      items: [
        {
          keys: bindingKeys(lookup("forms.open", "Mod+Shift+F")),
          label: "Open Forms hub (last-used tab)",
        },
        {
          keys: bindingKeys(lookup("quill.autodetect", "Mod+Shift+Y")),
          label: "Forms → Auto-Detect tab",
        },
        {
          keys: bindingKeys(lookup("quill.designer", "Mod+Shift+D")),
          label: "Forms → Designer tab",
        },
        {
          keys: bindingKeys(lookup("quill.batch", "Mod+Shift+B")),
          label: "Forms → Batch (CSV) tab",
        },
      ],
    },
    {
      title: "Stack (compare two PDFs)",
      items: [
        { keys: [MOD, "Shift", "D"], label: "Open Diff panel for word-level redline" },
        { keys: [MOD, "Shift", "3"], label: "Open Stack Pro for three-way compare (base/mine/theirs)" },
      ],
    },
    {
      title: "Customise",
      items: [
        { keys: [MOD, "K"], label: "Open palette → \"Customize shortcuts\"" },
      ],
    },
  ]);

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
      <h2>Keyboard shortcuts</h2>
      <button class="close" onclick={onClose} title="Close (Esc)">esc</button>
    </header>
    <div class="grid">
      {#each groups as g (g.title)}
        <section>
          <h3>{g.title}</h3>
          <ul>
            {#each g.items as s, i (i)}
              <li>
                <span class="keys">
                  {#each s.keys as k, j (j)}
                    {#if j > 0}<span class="plus">+</span>{/if}
                    <kbd>{k}</kbd>
                  {/each}
                </span>
                <span class="label">{s.label}</span>
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
  header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    letter-spacing: 0.2px;
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
