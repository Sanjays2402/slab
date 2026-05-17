<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open = $bindable(false), onClose }: Props = $props();

  type Shortcut = { keys: string[]; label: string };
  type Group = { title: string; items: Shortcut[] };

  // Detect macOS once at module load. We render ⌘ on Mac, Ctrl elsewhere.
  const IS_MAC =
    typeof navigator !== "undefined" &&
    /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent || "");
  const MOD = IS_MAC ? "⌘" : "Ctrl";

  const groups: Group[] = [
    {
      title: "Global",
      items: [
        { keys: [MOD, "K"], label: "Open command palette" },
        { keys: ["?"], label: "Show keyboard shortcuts" },
        { keys: ["Esc"], label: "Close current overlay" },
      ],
    },
    {
      title: "Tabs (Reader)",
      items: [
        { keys: [MOD, "T"], label: "Open a PDF in a new tab" },
        { keys: [MOD, "W"], label: "Close current tab" },
        { keys: [MOD, "1", "…", "9"], label: "Jump to tab N" },
        { keys: ["Ctrl", "Tab"], label: "Next tab" },
        { keys: ["Ctrl", "Shift", "Tab"], label: "Previous tab" },
      ],
    },
    {
      title: "Reading",
      items: [
        { keys: [MOD, "F"], label: "Find in document" },
        { keys: [MOD, "+"], label: "Zoom in" },
        { keys: [MOD, "−"], label: "Zoom out" },
        { keys: ["↑", "↓"], label: "Scroll one line" },
        { keys: ["PgUp", "PgDn"], label: "Scroll one page" },
      ],
    },
    {
      title: "Beacon (AI chat)",
      items: [
        { keys: [MOD, "↵"], label: "Send message" },
        { keys: ["Shift", "↵"], label: "Newline in prompt" },
      ],
    },
    {
      title: "Navigation",
      items: [
        { keys: ["Click sidebar"], label: "Switch panel" },
        { keys: [MOD, "K"], label: "Then type to filter panels & recents" },
      ],
    },
  ];

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
      <span>Press <kbd>?</kbd> any time to open this sheet.</span>
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
