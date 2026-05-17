<script lang="ts">
  // Keyboard shortcuts panel — v1.0.0 "Glass" Slice 7.
  //
  // Customise every global keyboard binding in Slab. Backed by the
  // `slab_keymap_*` Tauri commands; persists to `~/.slab/config.toml`
  // under `[keymap]` as a sparse override map (defaults reconstituted
  // at materialise time so adding new actions in future releases
  // never breaks an older config).
  //
  // Design intent (Linear / Raycast bar):
  //   - Each row = label on the left + a click-to-capture pill on the
  //     right + (optionally) an "override" badge.
  //   - Clicking a pill enters capture mode: the next non-modifier
  //     keypress becomes the new binding. Esc cancels, Backspace
  //     resets to default.
  //   - Group headers (Global / Tabs / Reading / Beacon) section the
  //     rows by `action.group`.
  //   - Reset-all button at the bottom for "I broke everything".
  //   - Conflicts (two actions sharing one binding) surface as an
  //     inline error toast + revert. The backend rejects collisions;
  //     we never persist a broken keymap.

  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import {
    keymapView,
    bootKeymap,
    writeKeymap,
    resetKeymap,
    bindingFromEvent,
    prettyBinding,
    type ActionId,
    type KeymapAction,
  } from "$lib/keymap";
  import { notify } from "$lib/notify";

  let capturing = $state<ActionId | null>(null);
  let busy = $state(false);
  let lastError = $state<string | null>(null);

  // Detect macOS once for the legend.
  const IS_MAC =
    typeof navigator !== "undefined" &&
    /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent || "");

  onMount(() => {
    void bootKeymap();
  });

  function startCapture(id: ActionId) {
    if (busy) return;
    capturing = id;
    lastError = null;
  }

  function cancelCapture() {
    capturing = null;
  }

  async function onCaptureKey(e: KeyboardEvent) {
    if (capturing == null) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      cancelCapture();
      return;
    }
    if (e.key === "Backspace") {
      // Reset just this action: write an empty-overrides batch by
      // first clearing all (resetKeymap is per-everything), then
      // re-applying every other current override. That's the only
      // way without a backend "reset one" command — but for the
      // common case of "reset the action I'm currently editing"
      // it's simpler to flip it back to its default via writeKeymap.
      const action = $keymapView.actions.find((a) => a.id === capturing);
      if (!action) {
        cancelCapture();
        return;
      }
      busy = true;
      try {
        await writeKeymap([[capturing, action.default_binding]]);
        notify.success(`Reset ${action.label} to ${prettyBinding(action.default_binding)}`);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        lastError = msg;
        notify.error("Couldn't reset binding", { detail: msg });
      } finally {
        busy = false;
        capturing = null;
      }
      return;
    }
    const binding = bindingFromEvent(e);
    if (binding == null) return; // wait for a non-modifier key
    const targetId = capturing;
    capturing = null;
    busy = true;
    try {
      const view = await writeKeymap([[targetId, binding]]);
      const a = view.actions.find((x) => x.id === targetId);
      notify.success(`Bound ${a?.label ?? targetId} → ${prettyBinding(binding)}`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      lastError = msg;
      // Parse the conflict message for a friendlier toast.
      const m = /bound to multiple actions:\s*\[(.*?)\]/.exec(msg);
      if (m) {
        notify.error("Shortcut already in use", { detail: m[1].replace(/"/g, "") });
      } else {
        notify.error("Couldn't save shortcut", { detail: msg });
      }
    } finally {
      busy = false;
    }
  }

  async function onResetAll() {
    if (busy) return;
    busy = true;
    try {
      await resetKeymap();
      notify.success("All shortcuts reset to defaults");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      lastError = msg;
      notify.error("Couldn't reset shortcuts", { detail: msg });
    } finally {
      busy = false;
    }
  }

  async function onResetOne(id: ActionId, defaultBinding: string) {
    if (busy) return;
    busy = true;
    try {
      await writeKeymap([[id, defaultBinding]]);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      lastError = msg;
      notify.error("Couldn't reset binding", { detail: msg });
    } finally {
      busy = false;
    }
  }

  // Group actions by `action.group` while preserving the canonical
  // registration order (Global → Tabs → Reading → Beacon).
  let grouped = $derived.by(() => {
    const map = new Map<string, KeymapAction[]>();
    for (const a of $keymapView.actions) {
      const arr = map.get(a.group) ?? [];
      arr.push(a);
      map.set(a.group, arr);
    }
    return [...map.entries()];
  });

  let overrideCount = $derived($keymapView.actions.filter((a) => a.is_override).length);
</script>

<svelte:window onkeydown={onCaptureKey} />

<section class="panel keymap-panel">
  <div class="content-header">
    <h1>Keyboard shortcuts</h1>
    <p class="subtitle">
      Customise every global shortcut in Slab. Click a binding to capture a new
      key combination — press <kbd>Esc</kbd> to cancel, <kbd>Backspace</kbd> to reset.
    </p>
  </div>

  <div class="toolbar">
    <span class="legend">
      {#if overrideCount === 0}
        <span class="muted">All shortcuts at factory defaults</span>
      {:else}
        <span class="badge">{overrideCount} customised</span>
      {/if}
    </span>
    <button
      type="button"
      class="ghost"
      onclick={onResetAll}
      disabled={busy || $keymapView.is_default}
      title="Discard every custom binding"
    >
      Reset all to defaults
    </button>
  </div>

  {#if $keymapView.actions.length === 0}
    <div class="empty">Loading keymap…</div>
  {/if}

  {#each grouped as [group, rows] (group)}
    <h2 class="group-heading">{group}</h2>
    <ul class="rows">
      {#each rows as row (row.id)}
        <li class="row" class:override={row.is_override}>
          <div class="row-info">
            <span class="label">{row.label}</span>
            <span class="id">{row.id}</span>
          </div>
          <div class="row-control">
            {#if row.is_override}
              <button
                class="reset-one"
                onclick={() => onResetOne(row.id as ActionId, row.default_binding)}
                disabled={busy}
                title="Reset to {prettyBinding(row.default_binding)}"
              >
                ↺ reset
              </button>
            {/if}
            <button
              type="button"
              class="pill"
              class:capturing={capturing === row.id}
              class:override-pill={row.is_override}
              onclick={() => startCapture(row.id as ActionId)}
              disabled={busy && capturing !== row.id}
            >
              {#if capturing === row.id}
                <span class="capture-hint" transition:fly={{ y: -4, duration: 140 }}>
                  Press a key combo…
                </span>
              {:else}
                <span class="binding">{prettyBinding(row.binding)}</span>
              {/if}
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/each}

  <div class="footer">
    <p class="hint">
      <strong>Tip:</strong> <code>Mod</code> means {IS_MAC ? "⌘ Cmd on macOS" : "Ctrl on this OS"}.
      Stored at <code>~/.slab/config.toml</code> under <code>[keymap]</code> — hand-editable.
    </p>
    {#if lastError}
      <p class="error">{lastError}</p>
    {/if}
  </div>
</section>

<style>
  .keymap-panel {
    max-width: 760px;
    padding: 32px 36px 48px;
    overflow-y: auto;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 14px 0 24px;
    padding: 10px 14px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .legend {
    font-size: 12px;
    color: var(--text-2);
  }
  .legend .muted {
    color: var(--text-3);
  }
  .badge {
    display: inline-block;
    background: color-mix(in oklab, var(--accent) 14%, var(--bg-2));
    border: 1px solid var(--accent);
    color: var(--accent);
    border-radius: 999px;
    padding: 2px 10px;
    font-size: 11px;
    font-weight: 600;
  }

  .empty {
    padding: 24px;
    text-align: center;
    color: var(--text-3);
    font-size: 13px;
  }

  .group-heading {
    margin: 24px 0 8px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-3);
    font-weight: 700;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-2);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .row:last-child {
    border-bottom: none;
  }
  .row.override {
    background: color-mix(in oklab, var(--accent) 4%, var(--bg-2));
  }

  .row-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    font-size: 13px;
    color: var(--text);
  }
  .id {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 10px;
    color: var(--text-3);
    letter-spacing: 0.2px;
    margin-top: 2px;
  }

  .row-control {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .pill {
    min-width: 120px;
    text-align: center;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 12px;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    transition: background 0.12s, border-color 0.12s, color 0.12s;
  }
  .pill:hover:not(:disabled) {
    border-color: var(--border-strong);
    background: var(--bg-2);
  }
  .pill.override-pill {
    border-color: var(--accent);
    color: var(--accent);
  }
  .pill.capturing {
    background: color-mix(in oklab, var(--accent) 22%, var(--bg-3));
    border-color: var(--accent);
    color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .pill:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .capture-hint {
    font-style: italic;
    font-size: 11px;
  }

  .reset-one {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-3);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 11px;
    cursor: pointer;
  }
  .reset-one:hover:not(:disabled) {
    color: var(--text-2);
    border-color: var(--border-strong);
  }
  .reset-one:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .ghost {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-2);
    border-radius: 6px;
    padding: 6px 12px;
    font-size: 12px;
    cursor: pointer;
  }
  .ghost:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }
  .ghost:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .footer {
    margin-top: 24px;
  }
  .hint {
    color: var(--text-3);
    font-size: 11px;
    margin: 0;
  }
  .hint code {
    background: var(--bg-3);
    border: 1px solid var(--border);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 11px;
  }
  .error {
    margin-top: 8px;
    color: var(--danger);
    font-size: 12px;
  }

  kbd {
    display: inline-block;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
    font-family: inherit;
    font-size: 11px;
    color: var(--text-2);
    min-width: 18px;
    text-align: center;
  }
</style>
