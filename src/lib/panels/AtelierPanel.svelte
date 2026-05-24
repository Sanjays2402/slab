<script lang="ts">
  // AtelierPanel — v3.12.0 Atelier UI.
  //
  // Three-column Liquid Glass surface:
  //
  //   1. Palette       — buttons for each Step kind. Click → append to recipe.
  //   2. Recipe builder — ordered list with per-step inputs, drag-to-reorder,
  //                       delete; Save + Load + Delete recipe controls.
  //   3. Runner        — input/output folder pickers, big "Run batch" button,
  //                       and a live per-file × per-step matrix that lights
  //                       up green/red as each cell completes. THIS is the
  //                       Atelier wow moment — hundreds of files marching
  //                       through every step in parallel under your eyes.
  //
  // Backend lives in `src-tauri/src/pdf/atelier/`. See `$lib/atelier.ts` for
  // the wire-format types.

  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { isInTauri } from "$lib/tauri";
  import { matches, prettyBindingFor } from "$lib/keymap";
  import {
    listRecipes,
    saveRecipe,
    deleteRecipe,
    runBatch,
    defaultStep,
    stepLabel,
    stepGlyph,
    type Recipe,
    type Step,
    type StepKind,
    type BatchProgress,
    type BatchReport,
  } from "$lib/atelier";

  type CellState = "pending" | "running" | "done" | "fail";

  interface FileRow {
    index: number;
    name: string;
    path: string;
    status: "queued" | "running" | "done" | "fail";
    error?: string;
    cells: CellState[];
  }

  const PALETTE: Array<{ kind: StepKind; label: string; sub: string }> = [
    { kind: "ocr", label: "OCR", sub: "Searchable text layer" },
    { kind: "auto-redact", label: "Auto-Redact", sub: "PII patterns + presets" },
    { kind: "bates", label: "Bates", sub: "Discovery numbering" },
    { kind: "watermark", label: "Watermark", sub: "Diagonal text overlay" },
    { kind: "flatten", label: "Flatten", sub: "Raster + immutable" },
    { kind: "compactor", label: "Compact", sub: "Strip unused objects" },
    { kind: "linearize", label: "Fast Web View", sub: "Stream page 1 instantly" },
    { kind: "convert-to-docx", label: "Convert to Word", sub: "PDF → .docx, offline" },
    { kind: "convert-to-xlsx", label: "Convert to Excel", sub: "PDF tables → .xlsx, offline" },
  ];

  let recipe = $state<Recipe>({ name: "Untitled", version: 1, steps: [] });
  let saved = $state<Recipe[]>([]);
  let selectedRecipeName = $state("");

  let inDir = $state("");
  let outDir = $state("");
  let busy = $state(false);
  let error = $state("");
  let info = $state("");

  let rows = $state<FileRow[]>([]);
  let rowsByIndex = new Map<number, number>(); // file_index → rows array idx
  let report = $state<BatchReport | null>(null);

  // Drag-to-reorder state
  let dragIndex: number | null = $state(null);

  $effect(() => {
    void refresh();
  });

  async function refresh() {
    if (!isInTauri()) return;
    try {
      saved = await listRecipes();
    } catch (e) {
      error = String(e);
    }
  }

  function addStep(kind: StepKind) {
    recipe.steps = [...recipe.steps, defaultStep(kind)];
  }
  function removeStep(i: number) {
    recipe.steps = recipe.steps.filter((_, j) => j !== i);
  }
  function moveStep(from: number, to: number) {
    if (from === to || to < 0 || to >= recipe.steps.length) return;
    const next = [...recipe.steps];
    const [s] = next.splice(from, 1);
    next.splice(to, 0, s);
    recipe.steps = next;
  }
  function onDragStart(i: number) {
    dragIndex = i;
  }
  function onDragOver(e: DragEvent, i: number) {
    e.preventDefault();
    if (dragIndex === null || dragIndex === i) return;
  }
  function onDrop(i: number) {
    if (dragIndex === null) return;
    moveStep(dragIndex, i);
    dragIndex = null;
  }

  async function onSave() {
    info = "";
    error = "";
    if (!recipe.name.trim()) {
      error = "Recipe needs a name.";
      return;
    }
    try {
      await saveRecipe(recipe);
      info = `Saved "${recipe.name}".`;
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function onDelete() {
    if (!selectedRecipeName) return;
    try {
      await deleteRecipe(selectedRecipeName);
      info = `Deleted "${selectedRecipeName}".`;
      selectedRecipeName = "";
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function onLoad() {
    if (!selectedRecipeName) return;
    const r = saved.find((s) => s.name === selectedRecipeName);
    if (!r) return;
    // Deep clone so edits don't mutate the saved copy in the dropdown list.
    recipe = JSON.parse(JSON.stringify(r));
    info = `Loaded "${recipe.name}".`;
  }

  async function pickFolder(target: "in" | "out") {
    error = "";
    try {
      const p = await openDialog({ directory: true, multiple: false });
      if (typeof p === "string") {
        if (target === "in") inDir = p;
        else outDir = p;
      }
    } catch (e) {
      error = String(e);
    }
  }

  function ensureRow(file_index: number, path: string): FileRow {
    let idx = rowsByIndex.get(file_index);
    if (idx === undefined) {
      const row: FileRow = {
        index: file_index,
        name: path.split(/[\\/]/).pop() ?? path,
        path,
        status: "running",
        cells: recipe.steps.map(() => "pending"),
      };
      idx = rows.length;
      rowsByIndex.set(file_index, idx);
      rows = [...rows, row];
    }
    return rows[idx];
  }

  function patchRow(file_index: number, mut: (r: FileRow) => void) {
    const idx = rowsByIndex.get(file_index);
    if (idx === undefined) return;
    const next = [...rows];
    const r = { ...next[idx], cells: [...next[idx].cells] };
    mut(r);
    next[idx] = r;
    rows = next;
  }

  function onProgress(e: BatchProgress) {
    switch (e.event) {
      case "file-started":
        ensureRow(e.file_index, e.path);
        break;
      case "step-progress": {
        ensureRow(e.file_index, "");
        const si = e.inner.step_index;
        if (e.inner.event === "started") {
          patchRow(e.file_index, (r) => {
            if (si < r.cells.length) r.cells[si] = "running";
          });
        } else if (e.inner.event === "completed") {
          patchRow(e.file_index, (r) => {
            if (si < r.cells.length) r.cells[si] = "done";
          });
        } else if (e.inner.event === "failed") {
          patchRow(e.file_index, (r) => {
            if (si < r.cells.length) r.cells[si] = "fail";
            r.error = e.inner.kind === "failed" ? "" : "";
          });
        }
        break;
      }
      case "file-completed":
        patchRow(e.file_index, (r) => {
          r.status = "done";
        });
        break;
      case "file-failed":
        patchRow(e.file_index, (r) => {
          r.status = "fail";
          r.error = e.error;
        });
        break;
    }
  }

  async function onRun() {
    error = "";
    info = "";
    report = null;
    if (!inDir || !outDir) {
      error = "Pick input and output folders.";
      return;
    }
    if (recipe.steps.length === 0) {
      error = "Recipe has no steps — add at least one.";
      return;
    }
    rows = [];
    rowsByIndex = new Map();
    busy = true;
    try {
      report = await runBatch(inDir, outDir, recipe, onProgress);
      info = `${report.succeeded} of ${report.total} succeeded.`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  // Cmd+Shift+A is already bound globally to "bedrock.open". Atelier doesn't
  // claim a global shortcut for now — accessed via sidebar + command palette.
  // Local shortcut: Cmd/Ctrl+Enter inside the panel triggers Run.
  function onKey(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && !busy) {
      e.preventDefault();
      void onRun();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Suppress unused-import lint while keymap helpers wait for future use.
  void matches;
  void prettyBindingFor;
</script>

<section class="atelier">
  <header class="hdr">
    <h2>Atelier</h2>
    <p class="tagline">
      Chain OCR, redaction, Bates, watermarks, flatten &amp; compact into a
      named recipe. Drop a folder; Slab runs every PDF through every step in
      parallel — fully offline.
    </p>
  </header>

  {#if error}
    <div class="banner err">{error}</div>
  {/if}
  {#if info && !error}
    <div class="banner ok">{info}</div>
  {/if}

  <div class="cols">
    <!-- ───────── Column 1: Step palette ───────── -->
    <aside class="col palette">
      <h3>Steps</h3>
      <ul class="step-list">
        {#each PALETTE as p (p.kind)}
          <li>
            <button
              type="button"
              class="step-btn"
              onclick={() => addStep(p.kind)}
              title="Append {p.label} to the recipe"
            >
              <span class="glyph">{stepGlyph(defaultStep(p.kind))}</span>
              <span class="ttl">{p.label}</span>
              <span class="sub">{p.sub}</span>
            </button>
          </li>
        {/each}
      </ul>
    </aside>

    <!-- ───────── Column 2: Recipe builder ───────── -->
    <section class="col builder">
      <div class="row name-row">
        <label class="lbl">
          <span>Recipe name</span>
          <input bind:value={recipe.name} type="text" placeholder="Untitled" />
        </label>
        <div class="saved">
          <select bind:value={selectedRecipeName}>
            <option value="">— Load saved —</option>
            {#each saved as r (r.name)}
              <option value={r.name}>{r.name}</option>
            {/each}
          </select>
          <button type="button" onclick={onLoad} disabled={!selectedRecipeName}>Load</button>
          <button
            type="button"
            class="danger"
            onclick={onDelete}
            disabled={!selectedRecipeName}
          >
            Delete
          </button>
        </div>
      </div>

      <ol class="recipe-list">
        {#if recipe.steps.length === 0}
          <li class="empty">
            <p>Empty recipe. Click any step on the left to add it.</p>
          </li>
        {/if}
        {#each recipe.steps as s, i (i)}
          <li
            class="step"
            class:dragging={dragIndex === i}
            draggable="true"
            ondragstart={() => onDragStart(i)}
            ondragover={(e) => onDragOver(e, i)}
            ondrop={() => onDrop(i)}
          >
            <div class="step-head">
              <span class="grip" aria-hidden="true">⋮⋮</span>
              <span class="idx">{i + 1}</span>
              <span class="glyph">{stepGlyph(s)}</span>
              <span class="kind">{stepLabel(s)}</span>
              <span class="actions">
                <button
                  type="button"
                  class="ic"
                  title="Move up"
                  onclick={() => moveStep(i, i - 1)}
                  disabled={i === 0}>↑</button
                >
                <button
                  type="button"
                  class="ic"
                  title="Move down"
                  onclick={() => moveStep(i, i + 1)}
                  disabled={i === recipe.steps.length - 1}>↓</button
                >
                <button
                  type="button"
                  class="ic danger"
                  title="Remove"
                  onclick={() => removeStep(i)}>✕</button
                >
              </span>
            </div>
            <div class="step-params">
              {#if s.kind === "ocr"}
                <label class="lbl">
                  <span>Language</span>
                  <input
                    type="text"
                    value={s.language}
                    oninput={(e) =>
                      ((recipe.steps[i] as { kind: "ocr"; language: string }).language =
                        (e.currentTarget as HTMLInputElement).value)}
                    placeholder="eng"
                  />
                </label>
              {:else if s.kind === "auto-redact"}
                <label class="lbl">
                  <span>Presets (comma-sep)</span>
                  <input
                    type="text"
                    value={s.presets.join(",")}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "auto-redact";
                        presets: string[];
                        patterns: string[];
                      }).presets = (e.currentTarget as HTMLInputElement).value
                        .split(",")
                        .map((x) => x.trim())
                        .filter(Boolean))}
                    placeholder="ssn,email,phone"
                  />
                </label>
                <label class="lbl">
                  <span>Custom patterns (regex, comma-sep)</span>
                  <input
                    type="text"
                    value={s.patterns.join(",")}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "auto-redact";
                        presets: string[];
                        patterns: string[];
                      }).patterns = (e.currentTarget as HTMLInputElement).value
                        .split(",")
                        .map((x) => x.trim())
                        .filter(Boolean))}
                  />
                </label>
              {:else if s.kind === "bates"}
                <label class="lbl">
                  <span>Prefix</span>
                  <input
                    type="text"
                    value={s.prefix}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "bates";
                        prefix: string;
                        start: number;
                        digits: number;
                      }).prefix = (e.currentTarget as HTMLInputElement).value)}
                  />
                </label>
                <label class="lbl narrow">
                  <span>Start</span>
                  <input
                    type="number"
                    min="0"
                    value={s.start}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "bates";
                        prefix: string;
                        start: number;
                        digits: number;
                      }).start =
                        Number((e.currentTarget as HTMLInputElement).value) || 0)}
                  />
                </label>
                <label class="lbl narrow">
                  <span>Digits</span>
                  <input
                    type="number"
                    min="1"
                    max="10"
                    value={s.digits}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "bates";
                        prefix: string;
                        start: number;
                        digits: number;
                      }).digits =
                        Number((e.currentTarget as HTMLInputElement).value) || 6)}
                  />
                </label>
              {:else if s.kind === "watermark"}
                <label class="lbl">
                  <span>Text</span>
                  <input
                    type="text"
                    value={s.text}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "watermark";
                        text: string;
                        opacity: number;
                      }).text = (e.currentTarget as HTMLInputElement).value)}
                  />
                </label>
                <label class="lbl narrow">
                  <span>Opacity</span>
                  <input
                    type="number"
                    step="0.05"
                    min="0"
                    max="1"
                    value={s.opacity}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "watermark";
                        text: string;
                        opacity: number;
                      }).opacity =
                        Number((e.currentTarget as HTMLInputElement).value) || 0.25)}
                  />
                </label>
              {:else if s.kind === "flatten"}
                <label class="lbl narrow">
                  <span>DPI</span>
                  <input
                    type="number"
                    min="72"
                    max="600"
                    value={s.dpi}
                    oninput={(e) =>
                      ((recipe.steps[i] as { kind: "flatten"; dpi: number }).dpi =
                        Number((e.currentTarget as HTMLInputElement).value) || 150)}
                  />
                </label>
              {:else if s.kind === "compactor"}
                <p class="muted">No parameters.</p>
              {:else if s.kind === "linearize"}
                <p class="muted">No parameters. Output streams page 1 to readers before the rest downloads.</p>
              {:else if s.kind === "convert-to-docx"}
                <p class="muted small">
                  Terminal step. Output files are written as <code>.docx</code>
                  (not <code>.pdf</code>) — paralegals open them straight in Word.
                  Acrobat $239/yr; Slab free + offline.
                </p>
                <label>
                  <input
                    type="checkbox"
                    checked={s.detect_tables}
                    onchange={(e) =>
                      ((recipe.steps[i] as {
                        kind: "convert-to-docx";
                        detect_tables: boolean;
                        detect_lists: boolean;
                        heading_size_ratio: number;
                      }).detect_tables = (e.currentTarget as HTMLInputElement).checked)}
                  />
                  Detect tables (column clustering)
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={s.detect_lists}
                    onchange={(e) =>
                      ((recipe.steps[i] as {
                        kind: "convert-to-docx";
                        detect_tables: boolean;
                        detect_lists: boolean;
                        heading_size_ratio: number;
                      }).detect_lists = (e.currentTarget as HTMLInputElement).checked)}
                  />
                  Detect bullets &amp; numbered lists
                </label>
                <label>
                  Heading size ratio
                  <input
                    type="number"
                    step="0.05"
                    min="1"
                    max="3"
                    value={s.heading_size_ratio}
                    oninput={(e) =>
                      ((recipe.steps[i] as {
                        kind: "convert-to-docx";
                        detect_tables: boolean;
                        detect_lists: boolean;
                        heading_size_ratio: number;
                      }).heading_size_ratio =
                        Number((e.currentTarget as HTMLInputElement).value) || 1.25)}
                  />
                </label>
              {:else if s.kind === "convert-to-xlsx"}
                <p class="muted small">
                  Terminal step. Output files are written as <code>.xlsx</code>
                  — open straight in Excel or Numbers. Adobe Acrobat Pro
                  $239/yr cloud-only; Slab free + offline + batch.
                </p>
                <label>
                  <input
                    type="checkbox"
                    checked={s.type_numbers}
                    onchange={(e) =>
                      ((recipe.steps[i] as {
                        kind: "convert-to-xlsx";
                        type_numbers: boolean;
                        type_dates: boolean;
                        include_non_table_text: boolean;
                      }).type_numbers = (e.currentTarget as HTMLInputElement).checked)}
                  />
                  Type numeric cells (US/EU formats)
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={s.type_dates}
                    onchange={(e) =>
                      ((recipe.steps[i] as {
                        kind: "convert-to-xlsx";
                        type_numbers: boolean;
                        type_dates: boolean;
                        include_non_table_text: boolean;
                      }).type_dates = (e.currentTarget as HTMLInputElement).checked)}
                  />
                  Detect dates (ISO, US, EU, long-month)
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={s.include_non_table_text}
                    onchange={(e) =>
                      ((recipe.steps[i] as {
                        kind: "convert-to-xlsx";
                        type_numbers: boolean;
                        type_dates: boolean;
                        include_non_table_text: boolean;
                      }).include_non_table_text = (e.currentTarget as HTMLInputElement).checked)}
                  />
                  Include surrounding paragraphs as text rows
                </label>
              {/if}
            </div>
          </li>
        {/each}
      </ol>

      <div class="row save-row">
        <button type="button" class="primary" onclick={onSave}>
          Save recipe
        </button>
        <span class="muted small">
          {recipe.steps.length} step{recipe.steps.length === 1 ? "" : "s"}
        </span>
      </div>
    </section>

    <!-- ───────── Column 3: Runner + live matrix ───────── -->
    <section class="col runner">
      <h3>Run batch</h3>

      <div class="folder">
        <label class="lbl">
          <span>Input folder</span>
          <div class="path-row">
            <input
              type="text"
              bind:value={inDir}
              placeholder="/path/to/inbox"
              readonly
            />
            <button type="button" onclick={() => pickFolder("in")}>Pick…</button>
          </div>
        </label>
        <label class="lbl">
          <span>Output folder</span>
          <div class="path-row">
            <input
              type="text"
              bind:value={outDir}
              placeholder="/path/to/outbox"
              readonly
            />
            <button type="button" onclick={() => pickFolder("out")}>Pick…</button>
          </div>
        </label>
      </div>

      <button type="button" class="primary big" onclick={onRun} disabled={busy}>
        {busy ? "Running…" : "▶  Run batch"}
      </button>

      {#if report}
        <div class="report" class:fail={report.failed > 0}>
          <strong>{report.succeeded}</strong> of <strong>{report.total}</strong>
          succeeded · <strong>{report.failed}</strong> failed
        </div>
      {/if}

      {#if rows.length > 0}
        <div class="matrix-wrap">
          <table class="matrix">
            <thead>
              <tr>
                <th class="sticky-l">File</th>
                {#each recipe.steps as s, j (j)}
                  <th class="step-col" title={stepLabel(s)}>{stepGlyph(s)}</th>
                {/each}
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each rows as r (r.index)}
                <tr>
                  <td class="sticky-l name" title={r.path}>{r.name}</td>
                  {#each recipe.steps as _s, j (j)}
                    <td class="cell {r.cells[j] ?? 'pending'}" aria-label={r.cells[j] ?? "pending"}>
                      {#if r.cells[j] === "done"}✓
                      {:else if r.cells[j] === "fail"}✗
                      {:else if r.cells[j] === "running"}…
                      {:else}·
                      {/if}
                    </td>
                  {/each}
                  <td class="status {r.status}" title={r.error ?? ""}>
                    {#if r.status === "done"}✓ done
                    {:else if r.status === "fail"}✗ {r.error ?? "failed"}
                    {:else if r.status === "running"}⏳ running
                    {:else}queued{/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else if !busy}
        <div class="empty-matrix">
          <p class="muted">
            Pick folders and hit Run. Each row will be a PDF; each column a
            step. Cells light up green as the work completes.
          </p>
        </div>
      {/if}
    </section>
  </div>
</section>

<style>
  .atelier {
    padding: 16px 18px 28px;
    color: var(--text-1);
  }
  .hdr h2 {
    font-size: 19px;
    margin: 0 0 4px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .hdr .tagline {
    margin: 0 0 14px;
    font-size: 13px;
    color: var(--text-2);
    max-width: 760px;
    line-height: 1.5;
  }
  .banner {
    margin-bottom: 12px;
    padding: 9px 12px;
    border-radius: var(--r-sm);
    font-size: 13px;
    border: 1px solid var(--border);
    background: var(--bg-2);
  }
  .banner.err {
    border-color: rgba(220, 80, 80, 0.45);
    background: rgba(220, 80, 80, 0.08);
  }
  .banner.ok {
    border-color: rgba(80, 170, 110, 0.45);
    background: rgba(80, 170, 110, 0.08);
  }
  .cols {
    display: grid;
    grid-template-columns: 220px 1fr 1.2fr;
    gap: 14px;
    align-items: start;
  }
  .col {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 12px 12px 14px;
  }
  .col h3 {
    margin: 0 0 10px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-1);
    letter-spacing: 0.02em;
    text-transform: uppercase;
    opacity: 0.78;
  }
  .step-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .step-btn {
    display: grid;
    grid-template-columns: 22px 1fr;
    grid-template-rows: auto auto;
    grid-column-gap: 8px;
    width: 100%;
    text-align: left;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 7px 9px;
    color: var(--text-1);
    cursor: pointer;
    font: inherit;
  }
  .step-btn:hover {
    background: var(--bg-3, var(--bg-1));
    border-color: var(--accent, #5b8def);
  }
  .step-btn .glyph {
    grid-row: 1 / span 2;
    align-self: center;
    font-size: 16px;
    color: var(--accent, #5b8def);
  }
  .step-btn .ttl {
    font-size: 13px;
    font-weight: 600;
  }
  .step-btn .sub {
    grid-column: 2;
    font-size: 11px;
    color: var(--text-2);
  }
  .row {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 10px;
  }
  .lbl {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-2);
    flex: 1 1 auto;
  }
  .lbl.narrow {
    flex: 0 0 90px;
  }
  .lbl input,
  .lbl select {
    padding: 6px 9px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font: inherit;
    font-size: 13px;
  }
  .saved {
    display: flex;
    gap: 6px;
    align-items: flex-end;
  }
  .saved select {
    padding: 6px 9px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
  }
  button {
    padding: 6px 11px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    border-color: var(--accent, #5b8def);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button.primary {
    background: var(--accent, #5b8def);
    color: white;
    border-color: var(--accent, #5b8def);
  }
  button.primary.big {
    width: 100%;
    padding: 11px 14px;
    font-size: 14px;
    font-weight: 600;
    margin-top: 4px;
  }
  button.danger {
    border-color: rgba(220, 80, 80, 0.45);
  }
  button.danger:hover:not(:disabled) {
    background: rgba(220, 80, 80, 0.12);
  }
  .recipe-list {
    list-style: none;
    margin: 0 0 12px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 480px;
    overflow-y: auto;
  }
  .recipe-list .empty {
    border: 1px dashed var(--border);
    border-radius: var(--r-sm);
    padding: 18px;
    text-align: center;
    color: var(--text-2);
    font-size: 13px;
  }
  .step {
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-1);
    padding: 8px 10px;
    transition: border-color 120ms;
  }
  .step.dragging {
    opacity: 0.55;
  }
  .step-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .step-head .grip {
    cursor: grab;
    color: var(--text-2);
    font-size: 11px;
  }
  .step-head .idx {
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
    width: 18px;
    text-align: right;
  }
  .step-head .glyph {
    color: var(--accent, #5b8def);
  }
  .step-head .kind {
    flex: 1 1 auto;
    font-weight: 500;
  }
  .step-head .actions {
    display: flex;
    gap: 4px;
  }
  .ic {
    padding: 2px 6px;
    font-size: 12px;
    line-height: 1;
  }
  .step-params {
    margin-top: 8px;
    display: flex;
    flex-wrap: wrap;
    gap: 8px 10px;
    padding-left: 26px;
  }
  .muted {
    color: var(--text-2);
  }
  .small {
    font-size: 11px;
  }
  .save-row {
    margin-top: 4px;
    align-items: center;
  }
  .folder {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 12px;
  }
  .path-row {
    display: flex;
    gap: 6px;
  }
  .path-row input {
    flex: 1 1 auto;
    padding: 6px 9px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font: inherit;
    font-size: 13px;
  }
  .report {
    margin: 10px 0 4px;
    padding: 8px 12px;
    border-radius: var(--r-sm);
    background: rgba(80, 170, 110, 0.1);
    border: 1px solid rgba(80, 170, 110, 0.4);
    font-size: 13px;
  }
  .report.fail {
    background: rgba(220, 80, 80, 0.08);
    border-color: rgba(220, 80, 80, 0.45);
  }
  .matrix-wrap {
    margin-top: 10px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-1);
    overflow: auto;
    max-height: 360px;
  }
  table.matrix {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  table.matrix thead th {
    position: sticky;
    top: 0;
    background: var(--bg-2);
    color: var(--text-2);
    padding: 6px 8px;
    text-align: center;
    font-weight: 600;
    border-bottom: 1px solid var(--border);
    z-index: 1;
  }
  table.matrix th.sticky-l,
  table.matrix td.sticky-l {
    position: sticky;
    left: 0;
    background: var(--bg-2);
    text-align: left;
    z-index: 2;
  }
  table.matrix td {
    padding: 4px 8px;
    text-align: center;
    border-bottom: 1px solid var(--border);
  }
  table.matrix td.name {
    text-align: left;
    max-width: 260px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    background: var(--bg-1);
  }
  th.step-col {
    min-width: 30px;
  }
  td.cell {
    width: 28px;
    color: var(--text-2);
    transition:
      background-color 220ms ease,
      color 220ms ease;
  }
  td.cell.running {
    background: rgba(91, 141, 239, 0.18);
    color: var(--accent, #5b8def);
  }
  td.cell.done {
    background: rgba(80, 170, 110, 0.18);
    color: rgb(80, 170, 110);
    font-weight: 700;
  }
  td.cell.fail {
    background: rgba(220, 80, 80, 0.18);
    color: rgb(220, 80, 80);
    font-weight: 700;
  }
  td.status {
    font-weight: 500;
  }
  td.status.done {
    color: rgb(80, 170, 110);
  }
  td.status.fail {
    color: rgb(220, 80, 80);
  }
  td.status.running {
    color: var(--accent, #5b8def);
  }
  .empty-matrix {
    margin-top: 12px;
    padding: 18px;
    border: 1px dashed var(--border);
    border-radius: var(--r-sm);
    text-align: center;
    font-size: 13px;
  }
</style>
