<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";

  type Feature = {
    id: string;
    label: string;
    icon: string;
    ready: boolean;
  };

  const features: Feature[] = [
    { id: "merge", label: "Merge", icon: "⧉", ready: true },
    { id: "split", label: "Split", icon: "⎯", ready: false },
    { id: "pages", label: "Pages", icon: "▦", ready: false },
    { id: "compress", label: "Compress", icon: "▼", ready: false },
    { id: "ocr", label: "OCR", icon: "✦", ready: false },
    { id: "convert", label: "Convert", icon: "↔", ready: false },
    { id: "encrypt", label: "Encrypt", icon: "▣", ready: false },
    { id: "watermark", label: "Watermark", icon: "○", ready: false },
    { id: "edit", label: "Edit", icon: "✎", ready: false },
    { id: "sign", label: "Sign", icon: "✍", ready: false },
  ];

  let active = $state("merge");
  let inputs = $state<string[]>([]);
  let status = $state<{ kind: "idle" | "working" | "ok" | "err"; msg: string }>(
    { kind: "idle", msg: "" }
  );
  let dragIndex = $state<number | null>(null);

  type CmdResult<T> = { kind: "ok"; value: T } | { kind: "err"; message: string };

  async function pickInputs() {
    const picked = await open({
      multiple: true,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked) return;
    const arr = Array.isArray(picked) ? picked : [picked];
    inputs = [...inputs, ...arr];
    status = { kind: "idle", msg: "" };
  }

  function removeInput(i: number) {
    inputs = inputs.filter((_, idx) => idx !== i);
  }

  function moveUp(i: number) {
    if (i === 0) return;
    const next = [...inputs];
    [next[i - 1], next[i]] = [next[i], next[i - 1]];
    inputs = next;
  }

  function moveDown(i: number) {
    if (i === inputs.length - 1) return;
    const next = [...inputs];
    [next[i + 1], next[i]] = [next[i], next[i + 1]];
    inputs = next;
  }

  function onDragStart(i: number) {
    dragIndex = i;
  }

  function onDragOver(e: DragEvent, i: number) {
    e.preventDefault();
    if (dragIndex === null || dragIndex === i) return;
  }

  function onDrop(i: number) {
    if (dragIndex === null || dragIndex === i) {
      dragIndex = null;
      return;
    }
    const next = [...inputs];
    const [moved] = next.splice(dragIndex, 1);
    next.splice(i, 0, moved);
    inputs = next;
    dragIndex = null;
  }

  async function runMerge() {
    if (inputs.length < 2) {
      status = { kind: "err", msg: "Add at least two PDFs to merge." };
      return;
    }
    const output = await save({
      defaultPath: "merged.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!output) return;

    status = { kind: "working", msg: "Merging…" };
    try {
      const res = await invoke<CmdResult<string>>("slab_merge", {
        inputs,
        output,
      });
      if (res.kind === "ok") {
        status = { kind: "ok", msg: `Saved → ${res.value}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function basename(p: string): string {
    const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return i >= 0 ? p.slice(i + 1) : p;
  }
</script>

<aside class="sidebar">
  <div class="brand">
    <span class="logo">▤</span>
    <span class="brand-name">Slab</span>
    <span class="brand-tag">offline pdf</span>
  </div>

  <nav>
    {#each features as f (f.id)}
      <button
        class="nav-item"
        class:active={active === f.id}
        class:locked={!f.ready}
        disabled={!f.ready}
        onclick={() => (active = f.id)}
      >
        <span class="nav-icon">{f.icon}</span>
        <span class="nav-label">{f.label}</span>
        {#if !f.ready}<span class="badge">soon</span>{/if}
      </button>
    {/each}
  </nav>

  <div class="footer">
    <span class="version">v0.0.1</span>
  </div>
</aside>

<main class="content">
  <header class="content-header">
    <h1>Merge PDFs</h1>
    <p class="subtitle">Combine multiple PDFs into one. Drag to reorder.</p>
  </header>

  <section class="merge-panel">
    {#if inputs.length === 0}
      <button class="dropzone" onclick={pickInputs}>
        <span class="dz-icon">+</span>
        <span class="dz-title">Add PDFs</span>
        <span class="dz-hint">Pick two or more files to merge</span>
      </button>
    {:else}
      <ul class="file-list">
        {#each inputs as path, i (path + i)}
          <li
            class="file-row"
            class:dragging={dragIndex === i}
            draggable="true"
            ondragstart={() => onDragStart(i)}
            ondragover={(e) => onDragOver(e, i)}
            ondrop={() => onDrop(i)}
          >
            <span class="row-handle" aria-hidden="true">⋮⋮</span>
            <span class="row-idx">{i + 1}</span>
            <span class="row-name" title={path}>{basename(path)}</span>
            <div class="row-actions">
              <button class="ghost" onclick={() => moveUp(i)} aria-label="Up"
                >↑</button
              >
              <button class="ghost" onclick={() => moveDown(i)} aria-label="Down"
                >↓</button
              >
              <button
                class="ghost remove"
                onclick={() => removeInput(i)}
                aria-label="Remove">✕</button
              >
            </div>
          </li>
        {/each}
      </ul>

      <div class="actions">
        <button onclick={pickInputs}>+ Add more</button>
        <button
          class="primary"
          onclick={runMerge}
          disabled={status.kind === "working" || inputs.length < 2}
        >
          {status.kind === "working" ? "Merging…" : `Merge ${inputs.length} files`}
        </button>
      </div>
    {/if}

    {#if status.kind === "ok"}
      <div class="status ok">✓ {status.msg}</div>
    {:else if status.kind === "err"}
      <div class="status err">✕ {status.msg}</div>
    {/if}
  </section>
</main>

<style>
  .sidebar {
    width: var(--sidebar-w);
    background: var(--bg-2);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 14px 10px;
    flex-shrink: 0;
  }

  .brand {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 8px 18px;
  }
  .logo {
    color: var(--accent);
    font-size: 18px;
  }
  .brand-name {
    font-weight: 700;
    font-size: 15px;
    letter-spacing: 0.2px;
  }
  .brand-tag {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.5px;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    overflow-y: auto;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 7px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .nav-item:hover:not(:disabled) {
    background: var(--bg-3);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .nav-item.locked {
    opacity: 0.55;
  }
  .nav-icon {
    width: 18px;
    text-align: center;
    color: var(--accent);
    opacity: 0.9;
  }
  .nav-label {
    flex: 1;
  }
  .badge {
    font-size: 9px;
    text-transform: uppercase;
    color: var(--text-3);
    background: var(--bg);
    padding: 2px 5px;
    border-radius: 4px;
    letter-spacing: 0.5px;
  }

  .footer {
    padding: 8px 10px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-3);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 28px 36px 36px;
  }

  .content-header h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.2px;
  }
  .subtitle {
    margin: 4px 0 24px;
    color: var(--text-2);
    font-size: 13px;
  }

  .merge-panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 60px;
    background: var(--bg-2);
    border: 1.5px dashed var(--border-strong);
    border-radius: var(--r-lg);
    color: var(--text-2);
    transition: background 0.15s, border-color 0.15s;
  }
  .dropzone:hover {
    background: var(--bg-3);
    border-color: var(--accent);
    color: var(--text);
  }
  .dz-icon {
    font-size: 32px;
    color: var(--accent);
    line-height: 1;
  }
  .dz-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .dz-hint {
    font-size: 12px;
    color: var(--text-3);
  }

  .file-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    cursor: grab;
  }
  .file-row.dragging {
    opacity: 0.5;
  }
  .file-row:hover {
    border-color: var(--border-strong);
  }
  .row-handle {
    color: var(--text-3);
    font-size: 11px;
    user-select: none;
  }
  .row-idx {
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: var(--bg-3);
    color: var(--text-2);
    font-size: 11px;
    font-weight: 600;
  }
  .row-name {
    flex: 1;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-actions {
    display: flex;
    gap: 4px;
  }
  .row-actions button {
    padding: 4px 8px;
    font-size: 12px;
    border-radius: 6px;
  }
  .row-actions .remove:hover {
    color: var(--danger);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .status {
    padding: 10px 14px;
    border-radius: var(--r-md);
    font-size: 13px;
    border: 1px solid var(--border);
  }
  .status.ok {
    background: rgba(94, 226, 165, 0.08);
    border-color: rgba(94, 226, 165, 0.3);
    color: var(--success);
  }
  .status.err {
    background: rgba(255, 90, 90, 0.08);
    border-color: rgba(255, 90, 90, 0.3);
    color: var(--danger);
  }
</style>
