<script lang="ts">
  /**
   * v3.9.0 "Quill" — PDF Forms inspector & fill (Slice 3).
   *
   * Adobe Acrobat Pro charges $239/yr for form work. Slab does it offline.
   *
   * Flow:
   *   1. Pick a PDF → call slab_forms_inspect → render a table of fields.
   *   2. Edit values inline (text fields → text input, button fields →
   *      <select> over /AP /N appearance state names).
   *   3. Click "Fill & Save" → call slab_forms_fill → write a new PDF
   *      with /NeedAppearances=true so any viewer regenerates appearances.
   *   4. Optional: "Export JSON" / "Import JSON" round-trips a field
   *      template — useful for batch fills, scripted workflows, or sharing
   *      a filled form template across team members.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { writeTextFile, readTextFile } from "@tauri-apps/plugin-fs";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";
  import { setInput as quillSetInput, recordFormsReport as quillRecordReport } from "$lib/quill";

  type FieldType = "text" | "button" | "choice" | "signature" | "unknown";

  type FormField = {
    name: string;
    type: FieldType;
    value: string | null;
    page: number | null;
    rect: [number, number, number, number] | null;
    options: string[];
    read_only: boolean;
  };

  type FormsReport = {
    has_acroform: boolean;
    need_appearances: boolean;
    has_xfa: boolean;
    fields: FormField[];
  };

  type FillReport = {
    filled: string[];
    unknown: string[];
    read_only_skipped: string[];
    need_appearances: boolean;
  };

  let input = $state<string | null>(null);
  let report = $state<FormsReport | null>(null);
  let values = $state<Record<string, string>>({});
  let status = $state<Status>(idle);
  let filter = $state("");

  // Derived: filtered field rows (search by qualified name).
  let visibleFields = $derived.by(() => {
    if (!report) return [] as FormField[];
    const q = filter.trim().toLowerCase();
    if (!q) return report.fields;
    return report.fields.filter((f) => f.name.toLowerCase().includes(q));
  });

  // Counts for the summary chips.
  let totalFields = $derived(report?.fields.length ?? 0);
  let textCount = $derived(report?.fields.filter((f) => f.type === "text").length ?? 0);
  let btnCount = $derived(report?.fields.filter((f) => f.type === "button").length ?? 0);
  let sigCount = $derived(report?.fields.filter((f) => f.type === "signature").length ?? 0);
  let editedCount = $derived(Object.keys(values).length);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    report = null;
    values = {};
    status = idle;
    quillSetInput(picked);
    await runInspect(picked);
  }

  async function runInspect(path: string) {
    status = { kind: "working", msg: "Reading form fields…" };
    try {
      const res = await invoke<CmdResult<FormsReport>>("slab_forms_inspect", { input: path });
      if (res.kind === "ok") {
        report = res.value;
        // Mirror to the Quill Hub store so the cross-tab CTA knows
        // an AcroForm is now available (suggests "Fill" → "Batch").
        quillRecordReport({
          has_acroform: res.value.has_acroform,
          need_appearances: res.value.need_appearances,
          has_xfa: res.value.has_xfa,
          fields: res.value.fields.map((f) => ({
            name: f.name,
            value: f.value,
            type: f.type,
          })),
        });
        // Pre-populate the editor with each field's current /V so the user
        // sees what's in the doc and can decide what to change.
        const v: Record<string, string> = {};
        for (const f of res.value.fields) {
          if (f.value != null) v[f.name] = f.value;
        }
        values = v;
        if (!res.value.has_acroform) {
          status = { kind: "err", msg: "No AcroForm fields found in this PDF." };
        } else if (res.value.has_xfa) {
          status = {
            kind: "err",
            msg: "This PDF uses XFA — Slab fills AcroForm only. XFA is deprecated in PDF 2.0.",
          };
        } else {
          status = { kind: "ok", msg: `Found ${res.value.fields.length} field(s).` };
        }
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function fillAndSave() {
    if (!input || !report) return;
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-filled.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Filling form…" };
    try {
      const res = await invoke<CmdResult<FillReport>>("slab_forms_fill", {
        input,
        values,
        output,
      });
      if (res.kind === "ok") {
        const r = res.value;
        const skipped =
          r.read_only_skipped.length > 0
            ? ` (${r.read_only_skipped.length} read-only skipped)`
            : "";
        status = {
          kind: "ok",
          msg: `Filled ${r.filled.length} field(s)${skipped} → ${basename(output)}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function exportJson() {
    if (!report) return;
    const dest = await save({
      defaultPath: input ? `${stripExt(basename(input))}-fields.json` : "fields.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof dest !== "string") return;
    const payload = {
      source: input,
      generator: "Slab Quill v3.9.0",
      has_acroform: report.has_acroform,
      need_appearances: report.need_appearances,
      has_xfa: report.has_xfa,
      fields: report.fields.map((f) => ({
        name: f.name,
        type: f.type,
        value: values[f.name] ?? f.value,
        page: f.page,
        rect: f.rect,
        options: f.options,
        read_only: f.read_only,
      })),
    };
    try {
      await writeTextFile(dest, JSON.stringify(payload, null, 2));
      status = { kind: "ok", msg: `Exported template → ${basename(dest)}` };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function importJson() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof picked !== "string") return;
    try {
      const txt = await readTextFile(picked);
      const parsed = JSON.parse(txt);
      if (!parsed || !Array.isArray(parsed.fields)) {
        status = { kind: "err", msg: "Invalid template: missing fields array." };
        return;
      }
      const next: Record<string, string> = { ...values };
      for (const f of parsed.fields) {
        if (typeof f.name === "string" && f.value != null) {
          next[f.name] = String(f.value);
        }
      }
      values = next;
      status = { kind: "ok", msg: `Imported ${parsed.fields.length} value(s) from template.` };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function clearAll() {
    if (!report) return;
    const next: Record<string, string> = {};
    for (const f of report.fields) {
      if (f.type === "button") next[f.name] = "Off";
      else next[f.name] = "";
    }
    values = next;
  }

  function badge(type: FieldType): string {
    switch (type) {
      case "text":
        return "Tx";
      case "button":
        return "Btn";
      case "choice":
        return "Ch";
      case "signature":
        return "Sig";
      default:
        return "?";
    }
  }
</script>

<header class="content-header">
  <h1>Forms</h1>
  <p class="subtitle">
    Inspect AcroForm fields, fill them inline, round-trip JSON templates. Fully
    offline — your tax return never leaves this machine.
  </p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a fillable PDF</span>
      <span class="dz-hint"
        >Government forms, contracts, invoices — anything with /AcroForm fields.</span
      >
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if report}
            {totalFields} field{totalFields === 1 ? "" : "s"}
            · {textCount} text · {btnCount} button{btnCount === 1 ? "" : "s"}{sigCount
              ? ` · ${sigCount} signature`
              : ""}
            {#if report.has_xfa}<span class="warn">· XFA</span>{/if}
          {:else}
            Loading…
          {/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    {#if report && report.has_acroform && !report.has_xfa}
      <div class="toolbar">
        <input
          class="search"
          type="search"
          placeholder="Filter by field name…"
          bind:value={filter}
        />
        <div class="spacer"></div>
        <button class="ghost" onclick={importJson}>Import JSON</button>
        <button class="ghost" onclick={exportJson}>Export JSON</button>
        <button class="ghost" onclick={clearAll}>Clear all</button>
        <button class="primary" onclick={fillAndSave} disabled={status.kind === "working"}>
          {status.kind === "working" ? status.msg : `Fill & Save (${editedCount})`}
        </button>
      </div>

      <div class="table-wrap" role="region" aria-label="Form fields">
        <table class="fields">
          <thead>
            <tr>
              <th class="col-type">Type</th>
              <th class="col-name">Name</th>
              <th class="col-page">Page</th>
              <th class="col-value">Value</th>
            </tr>
          </thead>
          <tbody>
            {#each visibleFields as f (f.name)}
              <tr class:read-only={f.read_only}>
                <td class="col-type">
                  <span class="type-badge type-{f.type}">{badge(f.type)}</span>
                </td>
                <td class="col-name">
                  <code>{f.name}</code>
                  {#if f.read_only}<span class="ro-tag">read-only</span>{/if}
                </td>
                <td class="col-page">{f.page ?? "—"}</td>
                <td class="col-value">
                  {#if f.type === "button" && f.options.length > 0}
                    <select
                      class="value-input"
                      disabled={f.read_only}
                      bind:value={values[f.name]}
                    >
                      {#each f.options as opt (opt)}
                        <option value={opt}>{opt}</option>
                      {/each}
                    </select>
                  {:else if f.read_only}
                    <input class="value-input" type="text" value={f.value ?? ""} disabled />
                  {:else}
                    <input
                      class="value-input"
                      type="text"
                      bind:value={values[f.name]}
                      placeholder={f.value ?? "(empty)"}
                    />
                  {/if}
                </td>
              </tr>
            {:else}
              <tr>
                <td colspan="4" class="empty">No fields match the current filter.</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="note">
        Filled output sets <code>/NeedAppearances=true</code> so any PDF reader
        (Acrobat, Preview, Firefox, Chrome) regenerates the visible appearance
        streams when the file is next opened. No cloud round-trip. No telemetry.
      </div>
    {/if}
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 0 10px;
    flex-wrap: wrap;
  }
  .search {
    flex: 0 1 280px;
    padding: 7px 10px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-1);
    font-size: 13px;
  }
  .spacer {
    flex: 1 1 auto;
  }
  .table-wrap {
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-2);
    overflow: auto;
    max-height: 520px;
  }
  table.fields {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  table.fields thead th {
    position: sticky;
    top: 0;
    background: var(--bg-1);
    text-align: left;
    padding: 9px 12px;
    font-weight: 600;
    color: var(--text-3);
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    z-index: 1;
  }
  table.fields tbody td {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  table.fields tbody tr:last-child td {
    border-bottom: none;
  }
  table.fields tbody tr:hover {
    background: var(--bg-3, rgba(127, 127, 127, 0.06));
  }
  tr.read-only {
    opacity: 0.65;
  }
  .col-type {
    width: 56px;
  }
  .col-page {
    width: 60px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .col-name code {
    font-family: var(--font-mono, ui-monospace, "SF Mono", monospace);
    font-size: 12px;
    color: var(--text-1);
  }
  .ro-tag {
    margin-left: 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 5px;
  }
  .type-badge {
    display: inline-block;
    min-width: 28px;
    text-align: center;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-2);
  }
  .type-badge.type-text {
    color: #1e6feb;
    border-color: rgba(30, 111, 235, 0.35);
  }
  .type-badge.type-button {
    color: #c0561e;
    border-color: rgba(192, 86, 30, 0.4);
  }
  .type-badge.type-choice {
    color: #6a4caf;
    border-color: rgba(106, 76, 175, 0.4);
  }
  .type-badge.type-signature {
    color: #2b8a3e;
    border-color: rgba(43, 138, 62, 0.4);
  }
  .value-input {
    width: 100%;
    padding: 6px 8px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-size: 13px;
    font-family: inherit;
  }
  .value-input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .empty {
    text-align: center;
    padding: 24px;
    color: var(--text-3);
    font-style: italic;
  }
  .note {
    margin-top: 12px;
    font-size: 12px;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    padding: 8px 12px;
    border-radius: var(--r-sm);
  }
  .note code {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 11px;
  }
  .warn {
    color: #c0561e;
    margin-left: 6px;
  }
</style>
