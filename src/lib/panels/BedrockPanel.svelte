<script lang="ts">
  // BedrockPanel — v3.0.0 "Bedrock" PDF/A archival conversion.
  //
  // One-pane workflow:
  //   1. Pick input PDF
  //   2. (auto) run font audit so the user knows what's coming
  //   3. Choose target level (2b default, 3b for e-invoicing)
  //   4. Optional XMP metadata (title / author / subject)
  //   5. Convert → atomic write + post-validation summary
  //
  // The wow moment is the post-convert hero card: a big green "PDF/A-2b
  // ✓" badge with the validation finding count, the bytes shipped, and
  // a tiny strata animation that sweeps gold across the card on success.

  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  // ---- Types mirror the Rust DTOs (src-tauri/src/pdf/pdfa/{convert,validate,font_audit}.rs)

  type Level = "2b" | "3b";

  type ConvertOpts = {
    level: Level;
    title?: string;
    author?: string;
    subject?: string;
    allow_unembedded_fonts: boolean;
  };

  type Severity = "Error" | "Warning" | "Info";

  type ValidationFinding = {
    severity: Severity;
    iso_section: string;
    message: string;
  };

  type ValidationReport = {
    findings: ValidationFinding[];
  };

  type ConvertReport = {
    level: Level;
    sanitized_entries: string[];
    added_output_intent: boolean;
    added_xmp_metadata: boolean;
    font_count: number;
    fonts_embedded: number;
    fonts_missing_embed: string[];
    output_bytes: number;
    validation: ValidationReport;
  };

  type FontEntry = {
    name: string;
    subtype: string;
    embedded: boolean;
    has_to_unicode: boolean;
    is_standard14: boolean;
  };

  type FontAuditReport = {
    fonts: Record<string, FontEntry>;
  };

  // ---- State

  let input = $state<string | null>(null);
  let output = $state<string | null>(null);
  let level = $state<Level>("2b");
  let title = $state("");
  let author = $state("");
  let subject = $state("");
  let allowUnembedded = $state(false);

  let audit = $state<FontAuditReport | null>(null);
  let auditStatus = $state<Status>(idle);

  let convertStatus = $state<Status>(idle);
  let report = $state<ConvertReport | null>(null);

  // ---- Derived

  let missingFonts = $derived.by(() => {
    if (!audit) return [] as FontEntry[];
    return Object.values(audit.fonts).filter((f) => !f.embedded);
  });

  let errorCount = $derived(report?.validation.findings.filter((f) => f.severity === "Error").length ?? 0);
  let warningCount = $derived(report?.validation.findings.filter((f) => f.severity === "Warning").length ?? 0);
  let passed = $derived(report !== null && errorCount === 0);

  function prettySize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
  }

  // ---- Actions

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked || Array.isArray(picked)) return;
    input = picked as string;
    // suggest "<name>.pdfa.pdf" as output
    const ext = ".pdf";
    const base = stripExt(basename(input));
    output = input.replace(/[^/\\]+$/, "") + base + ".pdfa" + ext;
    // auto-run font audit
    await runAudit();
    report = null;
  }

  async function pickOutput() {
    const picked = await save({
      defaultPath: output ?? undefined,
      filters: [{ name: "PDF/A", extensions: ["pdf"] }],
    });
    if (picked) output = picked as string;
  }

  async function runAudit() {
    if (!input) return;
    auditStatus = { kind: "working", msg: "Auditing fonts…" };
    audit = null;
    try {
      const r: CmdResult<FontAuditReport> = await invoke("slab_pdfa_font_audit", { input });
      if (r.kind === "ok") {
        audit = r.value;
        const missing = Object.values(audit.fonts).filter((f) => !f.embedded).length;
        auditStatus = {
          kind: "ok",
          msg:
            missing === 0
              ? `${Object.keys(audit.fonts).length} font(s), all embedded ✓`
              : `${missing} of ${Object.keys(audit.fonts).length} font(s) NOT embedded`,
        };
      } else {
        auditStatus = { kind: "err", msg: r.message };
      }
    } catch (e) {
      auditStatus = { kind: "err", msg: String(e) };
    }
  }

  async function runConvert() {
    if (!input || !output) return;
    convertStatus = { kind: "working", msg: `Converting to PDF/A-${level}…` };
    report = null;
    const opts: ConvertOpts = {
      level,
      title: title.trim() || undefined,
      author: author.trim() || undefined,
      subject: subject.trim() || undefined,
      allow_unembedded_fonts: allowUnembedded,
    };
    try {
      const r: CmdResult<ConvertReport> = await invoke("slab_pdfa_convert", { input, output, opts });
      if (r.kind === "ok") {
        report = r.value;
        const errs = report.validation.findings.filter((f) => f.severity === "Error").length;
        convertStatus = {
          kind: errs === 0 ? "ok" : "err",
          msg:
            errs === 0
              ? `Shipped PDF/A-${report.level} (${prettySize(report.output_bytes)})`
              : `Wrote file but validator flagged ${errs} error(s)`,
        };
      } else {
        convertStatus = { kind: "err", msg: r.message };
      }
    } catch (e) {
      convertStatus = { kind: "err", msg: String(e) };
    }
  }
</script>

<section class="bedrock">
  <header class="hdr">
    <div class="title">
      <span class="icon" aria-hidden="true">📐</span>
      <h2>Archive as PDF/A</h2>
    </div>
    <p class="subtitle">
      ISO 19005-2 conversion for legal, government, and long-term archival.
      Works fully offline — your file never leaves this machine.
    </p>
  </header>

  <div class="grid">
    <!-- Input picker -->
    <div class="card">
      <label class="lbl">Source PDF</label>
      <div class="row">
        <button class="btn-secondary" onclick={pickInput}>Choose file…</button>
        <span class="path" title={input ?? ""}>{input ? basename(input) : "no file chosen"}</span>
      </div>
      {#if auditStatus.kind !== "idle"}
        <div class="status status-{auditStatus.kind}">{auditStatus.msg}</div>
      {/if}
      {#if missingFonts.length > 0}
        <details class="warn">
          <summary>{missingFonts.length} font(s) need embedding</summary>
          <ul class="font-list">
            {#each missingFonts as f}
              <li>
                <span class="font-name">{f.name}</span>
                <span class="font-sub">{f.subtype}{f.is_standard14 ? " · standard-14" : ""}</span>
              </li>
            {/each}
          </ul>
          <p class="hint">
            Slab will auto-embed DejaVu substitutes for any Standard-14 font (Helvetica, Times, Courier
            and their variants) — no action needed. For truly custom fonts shown above, re-export from
            the source app with "embed all fonts" enabled, or check "Allow unembedded" below.
          </p>
        </details>
      {/if}
    </div>

    <!-- Options -->
    <div class="card">
      <label class="lbl">Conformance level</label>
      <div class="seg">
        <button class:active={level === "2b"} onclick={() => (level = "2b")}>
          <strong>PDF/A-2b</strong>
          <span>general archival</span>
        </button>
        <button class:active={level === "3b"} onclick={() => (level = "3b")}>
          <strong>PDF/A-3b</strong>
          <span>e-invoicing · attachments OK</span>
        </button>
      </div>

      <label class="lbl">Metadata (XMP, optional)</label>
      <input class="ti" placeholder="Title" bind:value={title} />
      <input class="ti" placeholder="Author" bind:value={author} />
      <input class="ti" placeholder="Subject" bind:value={subject} />

      <label class="chk">
        <input type="checkbox" bind:checked={allowUnembedded} />
        <span>Allow unembedded custom fonts (skip strict validation)</span>
      </label>
    </div>

    <!-- Output + run -->
    <div class="card">
      <label class="lbl">Output</label>
      <div class="row">
        <button class="btn-secondary" onclick={pickOutput} disabled={!input}>Save as…</button>
        <span class="path" title={output ?? ""}>{output ? basename(output) : "—"}</span>
      </div>
      <button
        class="btn-primary go"
        onclick={runConvert}
        disabled={!input || !output || convertStatus.kind === "working"}
      >
        {convertStatus.kind === "working" ? "Converting…" : `Convert to PDF/A-${level}`}
      </button>
      {#if convertStatus.kind === "err"}
        <div class="status status-err">{convertStatus.msg}</div>
      {/if}
    </div>
  </div>

  {#if report}
    <div class="hero" class:passed class:failed={!passed}>
      <div class="hero-badge">
        <span class="hero-mark">{passed ? "✓" : "!"}</span>
        <div class="hero-label">
          <div class="hero-title">
            PDF/A-{report.level} {passed ? "validated" : "wrote with warnings"}
          </div>
          <div class="hero-meta">
            {prettySize(report.output_bytes)} ·
            {report.font_count} fonts ·
            {errorCount} errors ·
            {warningCount} warnings
          </div>
        </div>
        <div class="strata" aria-hidden="true"></div>
      </div>

      <div class="hero-grid">
        <div class="stat">
          <div class="stat-num">{report.sanitized_entries.length}</div>
          <div class="stat-cap">entries sanitized</div>
        </div>
        <div class="stat">
          <div class="stat-num">{report.added_output_intent ? "✓" : "—"}</div>
          <div class="stat-cap">sRGB OutputIntent</div>
        </div>
        <div class="stat">
          <div class="stat-num">{report.added_xmp_metadata ? "✓" : "—"}</div>
          <div class="stat-cap">XMP metadata</div>
        </div>
        <div class="stat">
          <div class="stat-num">{report.fonts_embedded}</div>
          <div class="stat-cap">fonts auto-embedded</div>
        </div>
        <div class="stat">
          <div class="stat-num">{report.fonts_missing_embed.length}</div>
          <div class="stat-cap">fonts unembedded</div>
        </div>
      </div>

      {#if report.validation.findings.length > 0}
        <details class="findings" open={!passed}>
          <summary>{report.validation.findings.length} validation finding(s)</summary>
          <ul>
            {#each report.validation.findings as f}
              <li class="finding finding-{f.severity.toLowerCase()}">
                <span class="sev">{f.severity}</span>
                <span class="iso">§{f.iso_section}</span>
                <span class="msg">{f.message}</span>
              </li>
            {/each}
          </ul>
        </details>
      {/if}
    </div>
  {/if}
</section>

<style>
  .bedrock {
    padding: 24px 32px;
    max-width: 1100px;
    margin: 0 auto;
    color: var(--text, #e9eaee);
  }
  .hdr {
    margin-bottom: 20px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .icon {
    font-size: 26px;
  }
  h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .subtitle {
    margin: 6px 0 0;
    color: var(--text-muted, #98a0ad);
    font-size: 13px;
    max-width: 640px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 14px;
  }
  .card {
    background: var(--surface, #181a20);
    border: 1px solid var(--border, #2a2e38);
    border-radius: 12px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .lbl {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted, #98a0ad);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .path {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px;
    color: var(--text-muted, #98a0ad);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .btn-secondary,
  .btn-primary {
    font: inherit;
    border-radius: 8px;
    padding: 7px 12px;
    cursor: pointer;
    border: 1px solid var(--border, #2a2e38);
    background: var(--surface-2, #20232c);
    color: var(--text, #e9eaee);
    transition: background 0.12s, border-color 0.12s;
  }
  .btn-secondary:hover {
    background: var(--surface-3, #262932);
    border-color: var(--border-strong, #3a3f4d);
  }
  .btn-primary {
    background: linear-gradient(180deg, #d4a017, #b88c10);
    color: #1a1300;
    font-weight: 600;
    border-color: transparent;
  }
  .btn-primary:hover:not(:disabled) {
    background: linear-gradient(180deg, #e0ad1f, #c69510);
  }
  .btn-primary:disabled,
  .btn-secondary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .go {
    margin-top: 6px;
  }
  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .seg button {
    text-align: left;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border, #2a2e38);
    background: var(--surface-2, #20232c);
    color: inherit;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
    transition: border-color 0.12s, background 0.12s;
  }
  .seg button strong {
    font-size: 13px;
  }
  .seg button span {
    font-size: 11px;
    color: var(--text-muted, #98a0ad);
  }
  .seg button.active {
    border-color: #d4a017;
    background: rgba(212, 160, 23, 0.08);
  }
  .ti {
    font: inherit;
    background: var(--surface-2, #20232c);
    border: 1px solid var(--border, #2a2e38);
    color: inherit;
    padding: 7px 10px;
    border-radius: 8px;
  }
  .ti:focus {
    outline: 2px solid #d4a017;
    outline-offset: -1px;
  }
  .chk {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-muted, #98a0ad);
    user-select: none;
  }
  .status {
    font-size: 12px;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--surface-2, #20232c);
  }
  .status-ok {
    color: #6fdc8c;
  }
  .status-warn {
    color: #f1c40f;
  }
  .status-err {
    color: #ff7a7a;
  }
  .status-working {
    color: #98a0ad;
  }
  .warn {
    background: rgba(241, 196, 15, 0.06);
    border: 1px solid rgba(241, 196, 15, 0.25);
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 12px;
  }
  .warn summary {
    cursor: pointer;
    font-weight: 600;
    color: #f1c40f;
  }
  .font-list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    max-height: 140px;
    overflow-y: auto;
  }
  .font-list li {
    display: flex;
    justify-content: space-between;
    padding: 3px 0;
    border-bottom: 1px solid var(--border, #2a2e38);
  }
  .font-name {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .font-sub {
    color: var(--text-muted, #98a0ad);
    font-size: 11px;
  }
  .hint {
    margin: 8px 0 0;
    color: var(--text-muted, #98a0ad);
    font-size: 11px;
  }

  /* Hero post-convert card with the gold strata animation */
  .hero {
    margin-top: 22px;
    position: relative;
    border-radius: 14px;
    overflow: hidden;
    border: 1px solid var(--border, #2a2e38);
    background: var(--surface, #181a20);
  }
  .hero.passed {
    border-color: rgba(212, 160, 23, 0.55);
    box-shadow: 0 0 0 1px rgba(212, 160, 23, 0.2);
  }
  .hero.failed {
    border-color: rgba(255, 122, 122, 0.5);
  }
  .hero-badge {
    position: relative;
    padding: 18px 22px;
    display: flex;
    align-items: center;
    gap: 14px;
    background: linear-gradient(135deg, rgba(212, 160, 23, 0.12), transparent 60%);
  }
  .hero.failed .hero-badge {
    background: linear-gradient(135deg, rgba(255, 122, 122, 0.12), transparent 60%);
  }
  .hero-mark {
    width: 44px;
    height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: #d4a017;
    color: #1a1300;
    font-weight: 800;
    font-size: 22px;
    flex-shrink: 0;
  }
  .hero.failed .hero-mark {
    background: #ff7a7a;
    color: #2a0808;
  }
  .hero-title {
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .hero-meta {
    font-size: 12px;
    color: var(--text-muted, #98a0ad);
    margin-top: 2px;
  }
  /* Strata sweep — gold band sweeping L→R on mount */
  .strata {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(
      110deg,
      transparent 0%,
      transparent 35%,
      rgba(212, 160, 23, 0.18) 50%,
      transparent 65%,
      transparent 100%
    );
    transform: translateX(-100%);
    animation: strata-sweep 1.4s cubic-bezier(0.34, 1.56, 0.64, 1) 1 forwards;
  }
  .hero.failed .strata {
    display: none;
  }
  @keyframes strata-sweep {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(100%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .strata {
      animation: none;
      opacity: 0;
    }
  }
  .hero-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 1px;
    background: var(--border, #2a2e38);
  }
  .stat {
    padding: 14px;
    text-align: center;
    background: var(--surface, #181a20);
  }
  .stat-num {
    font-size: 22px;
    font-weight: 600;
    color: #d4a017;
  }
  .stat-cap {
    margin-top: 2px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted, #98a0ad);
  }
  .findings {
    padding: 14px 22px;
    border-top: 1px solid var(--border, #2a2e38);
  }
  .findings summary {
    cursor: pointer;
    font-weight: 600;
    font-size: 13px;
  }
  .findings ul {
    list-style: none;
    margin: 10px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .finding {
    display: grid;
    grid-template-columns: 70px 70px 1fr;
    gap: 10px;
    align-items: baseline;
    font-size: 12px;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--surface-2, #20232c);
  }
  .finding-error .sev {
    color: #ff7a7a;
    font-weight: 600;
  }
  .finding-warning .sev {
    color: #f1c40f;
    font-weight: 600;
  }
  .finding-info .sev {
    color: #6fdc8c;
  }
  .iso {
    color: var(--text-muted, #98a0ad);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
</style>
