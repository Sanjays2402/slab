<script lang="ts">
  // SmartCollectionBuilder — visual builder for Smart Collections (v3.33.0
  // "Atlas Smart"). Lets a user create or edit a smart collection (name,
  // icon, color, filter rules, sort) without ever writing JSON. The right
  // pane runs a debounced live preview against the current rule set.
  //
  // BUYER MAGNET: Smart Mailboxes for PDF. Neither Adobe Acrobat nor PDF
  // Expert ships this. The live-preview + count badge is the wow moment.

  import { onMount, onDestroy } from "svelte";
  import {
    type DocumentRecord,
    type FolderRecord,
    type LibraryFilter,
    type LibrarySortBy,
    type SmartCollectionRecord,
    type TagRecord,
    listDocuments,
    listFolders,
    listTags,
    smartCollectionCreate,
    smartCollectionUpdate,
  } from "$lib/library";

  type Props = {
    /** When set, builder is in edit mode. */
    editing: SmartCollectionRecord | null;
    onClose: () => void;
    onSaved: () => void;
  };

  let { editing, onClose, onSaved }: Props = $props();

  // ---------- Identity ----------
  const ICONS = [
    "📁",
    "⭐",
    "🧾",
    "🧪",
    "📚",
    "🔬",
    "📨",
    "🗂️",
    "🏷️",
    "🕒",
    "📜",
    "🧰",
  ];
  const COLORS = [
    "#a78bfa", // violet
    "#7cc4ff", // sky
    "#34d399", // emerald
    "#fbbf24", // amber
    "#fb7185", // rose
    "#ec4899", // pink
    "#6366f1", // indigo
    "#94a3b8", // slate
  ];

  let name = $state(editing?.name ?? "");
  let icon = $state<string>(editing?.icon ?? ICONS[0]);
  let color = $state<string>(editing?.color ?? COLORS[0]);

  // ---------- Rule rows ----------
  type RuleField = "title" | "tag" | "folder";
  type RuleRow = {
    id: number;
    field: RuleField;
    value: string | number[] | number | null;
  };
  let nextRuleId = 1;
  function newRow(field: RuleField = "title"): RuleRow {
    return {
      id: nextRuleId++,
      field,
      value: field === "title" ? "" : field === "tag" ? [] : null,
    };
  }

  // Hydrate rows from the editing record's existing filter, if any.
  function rowsFromFilter(f: LibraryFilter | null): RuleRow[] {
    if (!f) return [];
    const out: RuleRow[] = [];
    if (f.title_substring) {
      out.push({ id: nextRuleId++, field: "title", value: f.title_substring });
    }
    if (f.tag_ids && f.tag_ids.length) {
      out.push({ id: nextRuleId++, field: "tag", value: [...f.tag_ids] });
    }
    if (f.folder_id != null) {
      out.push({ id: nextRuleId++, field: "folder", value: f.folder_id });
    }
    return out;
  }

  let editingFilter: LibraryFilter | null = null;
  if (editing) {
    try {
      editingFilter = JSON.parse(editing.query_json) as LibraryFilter;
    } catch {
      editingFilter = null;
    }
  }
  let rules = $state<RuleRow[]>(rowsFromFilter(editingFilter));

  let sort = $state<LibrarySortBy>(editingFilter?.sort ?? "added_desc");

  // ---------- Catalog data for the dropdowns ----------
  let tags = $state<TagRecord[]>([]);
  let folders = $state<FolderRecord[]>([]);

  onMount(async () => {
    try {
      [tags, folders] = await Promise.all([listTags(), listFolders()]);
    } catch (e) {
      console.warn("smart-builder: failed to load tags/folders", e);
    }
    window.addEventListener("keydown", handleKey);
    schedulePreview();
  });
  onDestroy(() => {
    window.removeEventListener("keydown", handleKey);
    if (previewTimer) clearTimeout(previewTimer);
  });

  function handleKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  // ---------- Filter assembly ----------
  function buildFilter(): LibraryFilter {
    const f: LibraryFilter = { sort };
    for (const r of rules) {
      if (r.field === "title" && typeof r.value === "string" && r.value) {
        f.title_substring = r.value;
      } else if (
        r.field === "tag" &&
        Array.isArray(r.value) &&
        r.value.length
      ) {
        f.tag_ids = r.value;
      } else if (
        r.field === "folder" &&
        typeof r.value === "number" &&
        r.value != null
      ) {
        f.folder_id = r.value;
      }
    }
    return f;
  }

  // ---------- Debounced live preview ----------
  let previewDocs = $state<DocumentRecord[]>([]);
  let previewCount = $state<number>(0);
  let previewLoading = $state(false);
  let previewError = $state<string | null>(null);
  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  function schedulePreview() {
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(runPreview, 250);
  }

  async function runPreview() {
    previewLoading = true;
    previewError = null;
    try {
      const filter = buildFilter();
      const docs = await listDocuments(filter);
      previewCount = docs.length;
      previewDocs = docs.slice(0, 8);
    } catch (e) {
      previewError = (e as Error).message ?? String(e);
      previewDocs = [];
      previewCount = 0;
    } finally {
      previewLoading = false;
    }
  }

  // Re-run preview whenever rules / sort / icon-blob change.
  $effect(() => {
    // touch reactive deps so Svelte re-runs:
    void rules.length;
    void sort;
    for (const r of rules) {
      void r.field;
      void r.value;
    }
    schedulePreview();
  });

  // ---------- Save ----------
  let saving = $state(false);
  let error = $state<string | null>(null);

  async function handleSave() {
    if (!name.trim()) {
      error = "Give your smart collection a name.";
      return;
    }
    saving = true;
    error = null;
    try {
      const filter = buildFilter();
      if (editing) {
        await smartCollectionUpdate(editing.id, {
          name: name.trim(),
          icon,
          color,
          filter,
        });
      } else {
        await smartCollectionCreate({
          name: name.trim(),
          icon,
          color,
          filter,
        });
      }
      window.dispatchEvent(new CustomEvent("library-changed"));
      onSaved();
    } catch (e) {
      error = (e as Error).message ?? String(e);
    } finally {
      saving = false;
    }
  }

  function addRule() {
    rules = [...rules, newRow("title")];
  }
  function removeRule(id: number) {
    rules = rules.filter((r) => r.id !== id);
  }
  function setField(id: number, f: RuleField) {
    rules = rules.map((r) =>
      r.id === id
        ? {
            ...r,
            field: f,
            value: f === "title" ? "" : f === "tag" ? [] : null,
          }
        : r,
    );
  }
  function toggleTagIn(row: RuleRow, tagId: number) {
    const cur = Array.isArray(row.value) ? [...row.value] : [];
    const i = cur.indexOf(tagId);
    if (i === -1) cur.push(tagId);
    else cur.splice(i, 1);
    rules = rules.map((r) => (r.id === row.id ? { ...r, value: cur } : r));
  }
</script>

<div
  class="overlay"
  role="dialog"
  aria-modal="true"
  aria-labelledby="scb-title"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  onkeydown={() => {}}
  tabindex="-1"
>
  <div class="card">
    <header class="head">
      <div class="head-left">
        <span class="icon-chip" style="background: {color}20; color: {color}">
          {icon}
        </span>
        <h2 id="scb-title">
          {editing ? `Edit “${editing.name}”` : "New smart collection"}
        </h2>
      </div>
      <button class="close" aria-label="Close" onclick={onClose}>×</button>
    </header>

    <div class="body">
      <!-- LEFT: form -->
      <section class="form" aria-label="Smart collection definition">
        <label class="field">
          <span class="label">Name</span>
          <input
            type="text"
            bind:value={name}
            placeholder="Recent invoices"
            autofocus
          />
        </label>

        <div class="field">
          <span class="label">Icon</span>
          <div class="swatch-row">
            {#each ICONS as g}
              <button
                type="button"
                class="glyph"
                class:active={icon === g}
                aria-label={`Icon ${g}`}
                onclick={() => (icon = g)}
              >
                {g}
              </button>
            {/each}
          </div>
        </div>

        <div class="field">
          <span class="label">Color</span>
          <div class="swatch-row">
            {#each COLORS as c}
              <button
                type="button"
                class="swatch"
                class:active={color === c}
                style="background: {c}"
                aria-label={`Color ${c}`}
                onclick={() => (color = c)}
              ></button>
            {/each}
          </div>
        </div>

        <div class="field">
          <span class="label">Rules <small>(all must match)</small></span>
          <div class="rules">
            {#each rules as row (row.id)}
              <div class="rule-row">
                <select
                  value={row.field}
                  onchange={(e) =>
                    setField(row.id, (e.target as HTMLSelectElement).value as RuleField)}
                >
                  <option value="title">Title contains</option>
                  <option value="tag">Tag is</option>
                  <option value="folder">Folder is</option>
                </select>

                {#if row.field === "title"}
                  <input
                    type="text"
                    placeholder="invoice"
                    value={typeof row.value === "string" ? row.value : ""}
                    oninput={(e) => {
                      rules = rules.map((r) =>
                        r.id === row.id
                          ? {
                              ...r,
                              value: (e.target as HTMLInputElement).value,
                            }
                          : r,
                      );
                    }}
                  />
                {:else if row.field === "tag"}
                  <div class="tag-pills" role="group">
                    {#if tags.length === 0}
                      <span class="muted">No tags yet</span>
                    {/if}
                    {#each tags as t}
                      <button
                        type="button"
                        class="tag-pill"
                        class:active={Array.isArray(row.value) &&
                          row.value.includes(t.id)}
                        onclick={() => toggleTagIn(row, t.id)}
                      >
                        {t.name}
                      </button>
                    {/each}
                  </div>
                {:else}
                  <select
                    value={row.value == null ? "" : String(row.value)}
                    onchange={(e) => {
                      const v = (e.target as HTMLSelectElement).value;
                      const fid = v === "" ? null : Number(v);
                      rules = rules.map((r) =>
                        r.id === row.id ? { ...r, value: fid } : r,
                      );
                    }}
                  >
                    <option value="">— choose folder —</option>
                    {#each folders as f}
                      <option value={String(f.id)}>{f.path}</option>
                    {/each}
                  </select>
                {/if}

                <button
                  type="button"
                  class="trash"
                  aria-label="Remove rule"
                  onclick={() => removeRule(row.id)}
                >
                  ×
                </button>
              </div>
            {/each}
            <button type="button" class="add-rule" onclick={addRule}>
              + Add rule
            </button>
          </div>
        </div>

        <label class="field">
          <span class="label">Sort by</span>
          <select bind:value={sort}>
            <option value="added_desc">Recently added</option>
            <option value="title_asc">Title A → Z</option>
            <option value="last_seen_desc">Recently opened</option>
          </select>
        </label>

        {#if error}
          <p class="err" role="alert">{error}</p>
        {/if}
      </section>

      <!-- RIGHT: live preview -->
      <aside class="preview" aria-label="Live preview">
        <header class="preview-head">
          <span>Live preview</span>
          <span
            class="count"
            class:pulse={!previewLoading}
            style="background: {color}25; color: {color}"
          >
            {previewLoading ? "…" : `${previewCount} match${previewCount === 1 ? "" : "es"}`}
          </span>
        </header>
        {#if previewError}
          <p class="err small">{previewError}</p>
        {:else if previewDocs.length === 0 && !previewLoading}
          <div class="empty">
            <p>No documents match these rules yet.</p>
            <p class="muted small">
              Add a rule, or relax the existing ones.
            </p>
          </div>
        {:else}
          <ul class="doc-list">
            {#each previewDocs as d}
              <li class="doc-row">
                <span class="doc-title" title={d.path}>
                  {d.title ?? d.path.split("/").pop() ?? d.path}
                </span>
              </li>
            {/each}
            {#if previewCount > previewDocs.length}
              <li class="doc-more muted">
                +{previewCount - previewDocs.length} more…
              </li>
            {/if}
          </ul>
        {/if}
      </aside>
    </div>

    <footer class="foot">
      <button type="button" class="ghost" onclick={onClose} disabled={saving}>
        Cancel
      </button>
      <button
        type="button"
        class="primary"
        onclick={handleSave}
        disabled={saving || !name.trim()}
      >
        {saving ? "Saving…" : editing ? "Save changes" : "Create collection"}
      </button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(8, 10, 18, 0.55);
    backdrop-filter: blur(18px) saturate(140%);
    display: grid;
    place-items: center;
    z-index: 1000;
    padding: 32px;
    animation: fade-in 180ms ease-out;
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  .card {
    width: min(960px, 96vw);
    max-height: 90vh;
    background: rgba(22, 24, 33, 0.88);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 18px;
    box-shadow:
      0 24px 60px rgba(0, 0, 0, 0.55),
      inset 0 1px 0 rgba(255, 255, 255, 0.06);
    display: flex;
    flex-direction: column;
    color: rgba(235, 238, 246, 0.96);
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }
  .head-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .icon-chip {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    font-size: 18px;
    border: 1px solid currentColor;
  }
  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .close {
    background: transparent;
    border: none;
    color: rgba(235, 238, 246, 0.6);
    font-size: 24px;
    line-height: 1;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    cursor: pointer;
  }
  .close:hover {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(235, 238, 246, 0.95);
  }
  .body {
    display: grid;
    grid-template-columns: 1fr 320px;
    gap: 0;
    flex: 1;
    overflow: hidden;
  }
  .form {
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .preview {
    background: rgba(0, 0, 0, 0.15);
    border-left: 1px solid rgba(255, 255, 255, 0.06);
    padding: 16px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .preview-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: rgba(235, 238, 246, 0.55);
  }
  .count {
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0;
    text-transform: none;
    transition: transform 240ms ease;
  }
  .count.pulse {
    animation: badge-pulse 320ms ease-out;
  }
  @keyframes badge-pulse {
    0% {
      transform: scale(1);
    }
    40% {
      transform: scale(1.12);
    }
    100% {
      transform: scale(1);
    }
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .label {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: rgba(235, 238, 246, 0.55);
  }
  .label small {
    text-transform: none;
    letter-spacing: 0;
    color: rgba(235, 238, 246, 0.4);
    margin-left: 6px;
  }
  input[type="text"],
  select {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: rgba(235, 238, 246, 0.96);
    padding: 9px 11px;
    border-radius: 9px;
    font-size: 14px;
    outline: none;
    transition: border-color 120ms;
  }
  input[type="text"]:focus,
  select:focus {
    border-color: rgba(167, 139, 250, 0.5);
  }
  .swatch-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .glyph {
    width: 36px;
    height: 36px;
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.06);
    font-size: 18px;
    cursor: pointer;
    transition:
      transform 120ms,
      border-color 120ms;
  }
  .glyph:hover {
    transform: translateY(-1px);
  }
  .glyph.active {
    border-color: rgba(167, 139, 250, 0.8);
    background: rgba(167, 139, 250, 0.16);
  }
  .swatch {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.12);
    cursor: pointer;
    transition: transform 120ms;
  }
  .swatch:hover {
    transform: translateY(-1px);
  }
  .swatch.active {
    border-color: rgba(255, 255, 255, 0.9);
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.15);
  }
  .rules {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .rule-row {
    display: grid;
    grid-template-columns: 160px 1fr 32px;
    gap: 8px;
    align-items: center;
  }
  .trash {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    background: transparent;
    color: rgba(235, 238, 246, 0.5);
    font-size: 18px;
    cursor: pointer;
  }
  .trash:hover {
    color: rgba(251, 113, 133, 0.95);
    border-color: rgba(251, 113, 133, 0.35);
  }
  .add-rule {
    align-self: flex-start;
    background: transparent;
    border: 1px dashed rgba(255, 255, 255, 0.12);
    color: rgba(235, 238, 246, 0.7);
    padding: 7px 12px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .add-rule:hover {
    border-color: rgba(167, 139, 250, 0.6);
    color: rgba(235, 238, 246, 0.95);
  }
  .tag-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    min-height: 32px;
  }
  .tag-pill {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: rgba(235, 238, 246, 0.85);
    padding: 5px 10px;
    font-size: 12px;
    border-radius: 999px;
    cursor: pointer;
  }
  .tag-pill.active {
    background: rgba(167, 139, 250, 0.22);
    border-color: rgba(167, 139, 250, 0.8);
    color: rgba(255, 255, 255, 0.98);
  }
  .doc-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .doc-row {
    padding: 7px 10px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 7px;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .doc-more {
    padding: 4px 10px;
    font-size: 12px;
  }
  .empty {
    padding: 24px 10px;
    text-align: center;
    color: rgba(235, 238, 246, 0.6);
  }
  .empty p {
    margin: 4px 0;
  }
  .muted {
    color: rgba(235, 238, 246, 0.5);
  }
  .small {
    font-size: 12px;
  }
  .err {
    color: rgba(251, 113, 133, 0.95);
    background: rgba(251, 113, 133, 0.08);
    border: 1px solid rgba(251, 113, 133, 0.25);
    padding: 8px 11px;
    border-radius: 8px;
    font-size: 13px;
    margin: 0;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 14px 20px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    background: rgba(0, 0, 0, 0.12);
  }
  .ghost {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: rgba(235, 238, 246, 0.85);
    padding: 8px 16px;
    border-radius: 9px;
    cursor: pointer;
    font-size: 13px;
  }
  .ghost:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.04);
  }
  .primary {
    background: linear-gradient(135deg, #a78bfa, #7c3aed);
    border: none;
    color: white;
    padding: 8px 18px;
    border-radius: 9px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
    box-shadow: 0 6px 14px rgba(124, 58, 237, 0.35);
  }
  .primary:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .primary:disabled,
  .ghost:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  @media (max-width: 720px) {
    .body {
      grid-template-columns: 1fr;
    }
    .preview {
      border-left: none;
      border-top: 1px solid rgba(255, 255, 255, 0.06);
    }
  }
</style>
