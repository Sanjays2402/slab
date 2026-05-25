<script lang="ts">
  /**
   * QuillAutodetectPanel — v3.27.0 "Quill Auto-Detect".
   *
   * Drag a flat PDF in. Watch it become fillable. The backend scans every
   * page's content stream for horizontal rules, printed checkbox glyphs, and
   * labels-ending-in-colon to propose AcroForm field candidates. The user
   * curates them (per-row keep/skip, "accept >= X confidence" threshold)
   * then commits via the existing v3.26.0 Designer `slab_forms_design_add`
   * Tauri command — so detection, review and commit all reuse the same
   * round-trip the Designer already validated.
   *
   * Adobe Acrobat Pro's "Prepare Form" is the single most-cited reason
   * firms keep $239/yr Acrobat seats. PDF Expert and Preview can't do
   * this at all. Slab does it offline, free, cross-platform.
   *
   * Backend: src-tauri/src/pdf/forms_detect.rs (15 unit tests).
   */
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  // ---- Types mirror src-tauri/src/pdf/forms_detect.rs ---------------------

  type CandidateKindTag = "text" | "checkbox" | "signature";

  type EvidenceDto =
    | { type: "horizontal_rule"; line_width_pt: number }
    | { type: "empty_box"; side_pt: number }
    | { type: "labeled_blank" }
    | { type: "signature_line" };

  type FieldCandidate = {
    suggested_name: string;
    kind: CandidateKindTag;
    multiline?: boolean;
    page: number;
    rect: [number, number, number, number];
    label: string | null;
    evidence: EvidenceDto;
    confidence: number;
  };

  type DetectionReport = {
    candidates: FieldCandidate[];
    pages_scanned: number;
    already_has_acroform: boolean;
    warnings: string[];
  };

  // FieldDraft shape from v3.26.0 Designer (kept in sync with forms_design.rs).
  type TextDraft = {
    kind: "text";
    multiline: boolean;
    max_len: number | null;
    default: string | null;
  };
  type CheckboxDraft = { kind: "checkbox"; default_checked: boolean };
  type SignatureDraft = { kind: "signature" };
  type Draft = TextDraft | CheckboxDraft | SignatureDraft;
  type FieldDraft = Draft & {
    name: string;
    page: number;
    rect: [number, number, number, number];
    required: boolean;
    read_only: boolean;
    tooltip: string | null;
  };

  type DesignReport = {
    added: string[];
    edited: string[];
    deleted: string[];
    unknown: string[];
    errors: string[];
  };

  // ---- State -------------------------------------------------------------

  let input = $state<string | null>(null);
  let report = $state<DetectionReport | null>(null);
  let kept = $state<Set<number>>(new Set());
  let threshold = $state(0.70);
  let nameOverrides = $state<Record<number, string>>({});
  let status = $state<Status>(idle);
  let commitResult = $state<DesignReport | null>(null);

  // ---- Derived -----------------------------------------------------------

  let candidates = $derived(report?.candidates ?? []);

  let summary = $derived.by(() => {
    if (!report) return { total: 0, text: 0, checkbox: 0, signature: 0 };
    const s = { total: report.candidates.length, text: 0, checkbox: 0, signature: 0 };
    for (const c of report.candidates) {
      if (c.kind === "text") s.text++;
      else if (c.kind === "checkbox") s.checkbox++;
      else if (c.kind === "signature") s.signature++;
    }
    return s;
  });

  let keptCount = $derived(kept.size);

  // ---- Effects -----------------------------------------------------------

  // Re-apply the threshold whenever it (or the candidate list) changes.
  let lastAutoSignature = $state<string>("");
  $effect(() => {
    if (!report) return;
    const sig = `${report.candidates.length}:${threshold.toFixed(2)}`;
    if (sig === lastAutoSignature) return;
    lastAutoSignature = sig;
    const next = new Set<number>();
    report.candidates.forEach((c, i) => {
      if (c.confidence >= threshold) next.add(i);
    });
    kept = next;
  });

  // ---- Helpers -----------------------------------------------------------

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    report = null;
    commitResult = null;
    kept = new Set();
    nameOverrides = {};
    status = idle;
    await runDetect(picked);
  }

  async function runDetect(path: string) {
    status = { kind: "working", msg: "Scanning every page for blanks…" };
    try {
      const res = await invoke<CmdResult<DetectionReport>>(
        "slab_forms_autodetect",
        { input: path },
      );
      if (res.kind === "ok") {
        report = res.value;
        const found = res.value.candidates.length;
        status = {
          kind: "ok",
          msg: found
            ? `Found ${found} candidate field${found === 1 ? "" : "s"} on ${res.value.pages_scanned} page${res.value.pages_scanned === 1 ? "" : "s"}.`
            : `No candidates on ${res.value.pages_scanned} page${res.value.pages_scanned === 1 ? "" : "s"}. This may be a scan — try Lens OCR first.`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function toggleRow(idx: number) {
    const next = new Set(kept);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    kept = next;
  }

  function selectAll() {
    if (!report) return;
    kept = new Set(report.candidates.map((_, i) => i));
  }

  function clearAll() {
    kept = new Set();
  }

  function setName(idx: number, value: string) {
    nameOverrides = { ...nameOverrides, [idx]: value };
  }

  function effectiveName(c: FieldCandidate, idx: number): string {
    return (nameOverrides[idx] ?? c.suggested_name).trim() || c.suggested_name;
  }

  function candidateToDraft(c: FieldCandidate, name: string): FieldDraft {
    if (c.kind === "checkbox") {
      return {
        kind: "checkbox",
        default_checked: false,
        name,
        page: c.page,
        rect: c.rect,
        required: false,
        read_only: false,
        tooltip: c.label,
      };
    }
    if (c.kind === "signature") {
      return {
        kind: "signature",
        name,
        page: c.page,
        rect: c.rect,
        required: false,
        read_only: false,
        tooltip: c.label,
      };
    }
    // text
    return {
      kind: "text",
      multiline: !!c.multiline,
      max_len: null,
      default: null,
      name,
      page: c.page,
      rect: c.rect,
      required: false,
      read_only: false,
      tooltip: c.label,
    };
  }

  async function commitKept() {
    if (!input || !report) return;
    if (kept.size === 0) {
      status = { kind: "err", msg: "Keep at least one candidate first." };
      return;
    }
    const output = await save({
      title: "Save form-enabled PDF as…",
      defaultPath: input.replace(/\.pdf$/i, "") + "-form.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!output) return;

    // Build draft list, ensuring unique names after user edits.
    const seen = new Set<string>();
    const drafts: FieldDraft[] = [];
    const ordered = [...kept].sort((a, b) => a - b);
    for (const idx of ordered) {
      const c = report.candidates[idx];
      let name = effectiveName(c, idx);
      let n = 2;
      const base = name;
      while (seen.has(name)) {
        name = `${base}_${n++}`;
      }
      seen.add(name);
      drafts.push(candidateToDraft(c, name));
    }

    status = { kind: "working", msg: `Adding ${drafts.length} field(s)…` };
    try {
      const res = await invoke<CmdResult<DesignReport>>(
        "slab_forms_design_add",
        { input, drafts, output },
      );
      if (res.kind === "ok") {
        commitResult = res.value;
        const okCount = res.value.added.length;
        const errCount = res.value.errors.length;
        status = {
          kind: errCount ? "err" : "ok",
          msg: errCount
            ? `Added ${okCount}, ${errCount} error(s) → ${basename(output)}`
            : `Added ${okCount} field(s) → ${basename(output)} — open it in the Fill panel to test.`,
        };
        input = output;
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function evidenceLabel(e: EvidenceDto): string {
    switch (e.type) {
      case "horizontal_rule":
        return `underline ${e.line_width_pt.toFixed(1)}pt`;
      case "empty_box":
        return `${e.side_pt.toFixed(0)}pt box`;
      case "labeled_blank":
        return "labeled blank";
      case "signature_line":
        return "signature line";
    }
  }

  function confidenceTone(c: number): string {
    if (c >= 0.85) return "high";
    if (c >= 0.65) return "med";
    return "low";
  }

  function kindLabel(c: FieldCandidate): string {
    if (c.kind === "text") return c.multiline ? "Text (multiline)" : "Text";
    if (c.kind === "checkbox") return "Checkbox";
    return "Signature";
  }
</script>

<section class="panel" aria-labelledby="quill-autodetect-heading">
  <header class="head">
    <h1 id="quill-autodetect-heading">Quill Auto-Detect</h1>
    <p class="sub">
      Drag a flat PDF in. Watch it become fillable. Slab scans every page for
      underline-blanks, checkbox glyphs, and labeled rules — then proposes
      AcroForm fields you curate and ship.
    </p>
  </header>

  <div class="row">
    <button class="btn primary" type="button" onclick={pickInput}>
      {input ? "Change PDF…" : "Pick a flat PDF…"}
    </button>
    <span class="path" title={input ?? ""}>
      {input ? basename(input) : "no file selected"}
    </span>
  </div>

  {#if status.kind !== "idle"}
    <p class="status {status.kind}">{status.msg}</p>
  {/if}

  {#if report}
    {#if report.already_has_acroform}
      <p class="warn">
        Heads up: this PDF already has an AcroForm. Detected fields will be
        merged in, not replacing the existing ones.
      </p>
    {/if}
    {#each report.warnings as w}
      <p class="warn">{w}</p>
    {/each}

    <div class="summary-cards">
      <div class="summary-card">
        <span class="big">{summary.total}</span>
        <span class="small">candidates</span>
      </div>
      <div class="summary-card">
        <span class="big">{summary.text}</span>
        <span class="small">text</span>
      </div>
      <div class="summary-card">
        <span class="big">{summary.checkbox}</span>
        <span class="small">checkboxes</span>
      </div>
      <div class="summary-card">
        <span class="big">{summary.signature}</span>
        <span class="small">signatures</span>
      </div>
    </div>

    {#if summary.total > 0}
      <div class="controls">
        <label class="threshold">
          <span>Keep candidates with confidence ≥</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            bind:value={threshold}
          />
          <span class="pct">{Math.round(threshold * 100)}%</span>
        </label>
        <div class="bulk">
          <button class="btn ghost" type="button" onclick={selectAll}>
            Keep all
          </button>
          <button class="btn ghost" type="button" onclick={clearAll}>
            Clear
          </button>
        </div>
      </div>

      <ul class="cands">
        {#each candidates as c, i (i)}
          {@const conf = confidenceTone(c.confidence)}
          <li class="cand" class:kept={kept.has(i)}>
            <label class="keep">
              <input
                type="checkbox"
                checked={kept.has(i)}
                onchange={() => toggleRow(i)}
                aria-label="Keep this candidate"
              />
            </label>
            <div class="meta">
              <div class="line1">
                <input
                  class="name-input"
                  type="text"
                  value={nameOverrides[i] ?? c.suggested_name}
                  oninput={(e) => setName(i, e.currentTarget.value)}
                  aria-label="Field name"
                />
                <span class="kind-pill kind-{c.kind}">{kindLabel(c)}</span>
                <span class="conf conf-{conf}" title="Confidence">
                  {Math.round(c.confidence * 100)}%
                </span>
              </div>
              <div class="line2">
                <span class="page">p.{c.page}</span>
                {#if c.label}
                  <span class="label">“{c.label}”</span>
                {/if}
                <span class="evidence">{evidenceLabel(c.evidence)}</span>
                <span class="rect">
                  [{c.rect[0].toFixed(0)}, {c.rect[1].toFixed(0)},
                  {c.rect[2].toFixed(0)}, {c.rect[3].toFixed(0)}]
                </span>
              </div>
            </div>
          </li>
        {/each}
      </ul>

      <div class="commit-row">
        <button
          class="btn primary"
          type="button"
          onclick={commitKept}
          disabled={keptCount === 0}
        >
          Add {keptCount} field{keptCount === 1 ? "" : "s"} → new PDF
        </button>
      </div>

      {#if commitResult}
        <article class="result-card" aria-live="polite">
          <h3>Last commit</h3>
          <ul>
            <li><strong>Added:</strong> {commitResult.added.join(", ") || "—"}</li>
            {#if commitResult.errors.length > 0}
              <li class="err"><strong>Errors:</strong> {commitResult.errors.join("; ")}</li>
            {/if}
          </ul>
        </article>
      {/if}
    {:else if !status.msg}
      <p class="empty">
        No candidates found. This PDF may already be fully interactive — or it
        may be a scan with no vector lines (try Lens OCR first).
      </p>
    {/if}
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    padding: 1.25rem 1.5rem 2rem;
    max-width: 1100px;
  }
  .head h1 {
    margin: 0;
    font-size: 1.45rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .head .sub {
    margin: 0.2rem 0 0;
    color: var(--text-muted, #6b7280);
    font-size: 0.92rem;
    max-width: 70ch;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .btn {
    border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
    background: var(--surface-2, rgba(255, 255, 255, 0.04));
    color: inherit;
    border-radius: 8px;
    padding: 0.45rem 0.85rem;
    font: inherit;
    cursor: pointer;
    transition: background 120ms ease, transform 80ms ease;
  }
  .btn:hover {
    background: var(--surface-3, rgba(255, 255, 255, 0.08));
  }
  .btn.primary {
    background: var(--accent, #4f46e5);
    color: white;
    border-color: transparent;
  }
  .btn.primary:hover {
    filter: brightness(1.07);
  }
  .btn.ghost {
    background: transparent;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .path {
    color: var(--text-muted, #6b7280);
    font-size: 0.9rem;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  .status {
    margin: 0;
    padding: 0.5rem 0.75rem;
    border-radius: 8px;
    font-size: 0.9rem;
  }
  .status.working {
    background: rgba(99, 102, 241, 0.1);
    color: var(--accent, #4f46e5);
  }
  .status.ok {
    background: rgba(34, 197, 94, 0.1);
    color: #16a34a;
  }
  .status.err {
    background: rgba(239, 68, 68, 0.1);
    color: #dc2626;
  }
  .warn {
    margin: 0;
    padding: 0.5rem 0.75rem;
    border-radius: 8px;
    background: rgba(234, 179, 8, 0.12);
    color: #b45309;
    font-size: 0.88rem;
  }
  .summary-cards {
    display: grid;
    grid-template-columns: repeat(4, minmax(120px, 1fr));
    gap: 0.6rem;
  }
  .summary-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 0.7rem 0.9rem;
    border-radius: 10px;
    background: var(--surface-2, rgba(255, 255, 255, 0.04));
    border: 1px solid var(--border, rgba(255, 255, 255, 0.08));
  }
  .summary-card .big {
    font-size: 1.6rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .summary-card .small {
    font-size: 0.78rem;
    color: var(--text-muted, #6b7280);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    padding: 0.6rem 0.85rem;
    border-radius: 10px;
    background: var(--surface-2, rgba(255, 255, 255, 0.04));
    border: 1px solid var(--border, rgba(255, 255, 255, 0.08));
  }
  .controls .threshold {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-size: 0.9rem;
  }
  .controls .threshold input[type="range"] {
    width: 220px;
  }
  .controls .pct {
    min-width: 3ch;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .bulk {
    display: flex;
    gap: 0.4rem;
  }
  .cands {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }
  .cand {
    display: flex;
    gap: 0.6rem;
    padding: 0.55rem 0.7rem;
    border-radius: 10px;
    background: var(--surface-2, rgba(255, 255, 255, 0.04));
    border: 1px solid var(--border, rgba(255, 255, 255, 0.08));
    transition: background 120ms ease, border-color 120ms ease;
  }
  .cand.kept {
    border-color: var(--accent, #4f46e5);
    background: rgba(99, 102, 241, 0.06);
  }
  .keep {
    display: flex;
    align-items: center;
    padding-top: 0.2rem;
  }
  .keep input {
    width: 1.05rem;
    height: 1.05rem;
    cursor: pointer;
  }
  .meta {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .line1 {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .name-input {
    flex: 1 1 220px;
    min-width: 160px;
    background: transparent;
    border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
    border-radius: 6px;
    color: inherit;
    padding: 0.32rem 0.5rem;
    font: inherit;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.85rem;
  }
  .name-input:focus {
    outline: 2px solid var(--accent, #4f46e5);
    outline-offset: 1px;
  }
  .kind-pill {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    background: var(--surface-3, rgba(255, 255, 255, 0.08));
  }
  .kind-text {
    color: #2563eb;
    background: rgba(37, 99, 235, 0.12);
  }
  .kind-checkbox {
    color: #059669;
    background: rgba(5, 150, 105, 0.12);
  }
  .kind-signature {
    color: #b45309;
    background: rgba(180, 83, 9, 0.12);
  }
  .conf {
    font-size: 0.72rem;
    font-weight: 600;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    font-variant-numeric: tabular-nums;
  }
  .conf-high {
    background: rgba(34, 197, 94, 0.14);
    color: #16a34a;
  }
  .conf-med {
    background: rgba(234, 179, 8, 0.16);
    color: #b45309;
  }
  .conf-low {
    background: rgba(239, 68, 68, 0.14);
    color: #dc2626;
  }
  .line2 {
    display: flex;
    gap: 0.55rem;
    align-items: center;
    flex-wrap: wrap;
    font-size: 0.78rem;
    color: var(--text-muted, #6b7280);
  }
  .line2 .page {
    font-weight: 600;
  }
  .line2 .label {
    font-style: italic;
  }
  .line2 .rect {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.72rem;
    opacity: 0.7;
  }
  .commit-row {
    display: flex;
    justify-content: flex-end;
    padding-top: 0.4rem;
  }
  .result-card {
    padding: 0.8rem 1rem;
    border-radius: 10px;
    background: var(--surface-2, rgba(255, 255, 255, 0.04));
    border: 1px solid var(--border, rgba(255, 255, 255, 0.08));
  }
  .result-card h3 {
    margin: 0 0 0.4rem;
    font-size: 0.95rem;
  }
  .result-card ul {
    margin: 0;
    padding-left: 1.1rem;
    font-size: 0.88rem;
  }
  .result-card .err {
    color: #dc2626;
  }
  .empty {
    margin: 0.5rem 0 0;
    color: var(--text-muted, #6b7280);
    font-size: 0.9rem;
  }
</style>
