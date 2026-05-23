<script lang="ts">
  // LoupePanel — v3.0.1 "Loupe" PDF/A Compliance Inspector.
  //
  // Slab's answer to Adobe Acrobat Preflight ($239/yr). Drop any PDF,
  // get a per-rule pass/fail report against ISO 19005-2 (PDF/A-2b + 3b),
  // and copy the whole thing as a Markdown audit artifact ready to paste
  // into a compliance ticket.
  //
  // 100% local. Read-only. Never touches the input file.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  // ---- Types mirror the Rust DTOs in src-tauri/src/pdf/pdfa/inspect.rs

  type Level = "2b" | "3b";
  type Verdict = "pass" | "achievable_with_fixes" | "fail";
  type Severity = "error" | "warning" | "info";

  type ValidationFinding = {
    severity: Severity;
    iso_section: string;
    message: string;
  };

  type ValidationReport = { findings: ValidationFinding[] };

  type LevelAssessment = {
    level: Level;
    verdict: Verdict;
    blocking_errors: number;
    auto_fixable: number;
    validation: ValidationReport;
  };

  type FontEntry = {
    name: string;
    subtype: string;
    embedded: boolean;
    has_to_unicode: boolean;
    is_standard14: boolean;
  };

  type FontAuditReport = { fonts: Record<string, FontEntry> };

  type InspectionReport = {
    input_path: string;
    pdf_version: string;
    page_count: number;
    file_bytes: number;
    encrypted: boolean;
    fonts: FontAuditReport;
    sanitize_preview: string[];
    levels: LevelAssessment[];
    suggestions: string[];
  };

  // ---- State

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);
  let report = $state<InspectionReport | null>(null);
  let showFonts = $state(true);
  let showSanitize = $state(true);
  let showFindings = $state(false);
  let copied = $state(false);

  // ---- Derived

  let fontList = $derived.by(() => {
    if (!report) return [] as FontEntry[];
    return Object.values(report.fonts.fonts).sort((a, b) => a.name.localeCompare(b.name));
  });

  let allEmbedded = $derived(fontList.length === 0 || fontList.every((f) => f.embedded));

  function prettySize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
  }

  function levelLabel(l: Level): string {
    return l === "2b" ? "PDF/A-2b" : "PDF/A-3b";
  }

  function levelTagline(l: Level): string {
    return l === "2b"
      ? "Standard archival — legal, government, healthcare"
      : "Embedded attachments — e-invoicing (ZUGFeRD, Factur-X)";
  }

  function verdictLabel(v: Verdict): string {
    return v === "pass"
      ? "Compliant"
      : v === "achievable_with_fixes"
        ? "Achievable"
        : "Fail";
  }

  function verdictGlyph(v: Verdict): string {
    return v === "pass" ? "✓" : v === "achievable_with_fixes" ? "▲" : "✕";
  }

  // ---- Actions

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked || Array.isArray(picked)) return;
    input = picked as string;
    await runInspect();
  }

  async function runInspect() {
    if (!input) return;
    status = { kind: "working", msg: "Inspecting…" };
    report = null;
    try {
      const r: CmdResult<InspectionReport> = await invoke("slab_pdfa_inspect", { input });
      if (r.kind === "ok") {
        report = r.value;
        status = { kind: "ok", msg: "" };
      } else {
        status = { kind: "err", msg: r.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function asMarkdown(): string {
    if (!report) return "";
    const r = report;
    const name = basename(r.input_path);
    const rows = r.levels
      .map((a) => {
        const label = levelLabel(a.level);
        const verdict =
          a.verdict === "pass"
            ? "✅ Compliant"
            : a.verdict === "achievable_with_fixes"
              ? "▲ Achievable with Bedrock conversion"
              : "❌ Fail";
        const errs = a.blocking_errors === 0 ? "0" : `${a.blocking_errors} (${a.auto_fixable} auto-fixable)`;
        return `| ${label} | ${verdict} | ${errs} |`;
      })
      .join("\n");

    const fontRows = fontList.length
      ? fontList
          .map(
            (f) =>
              `- **${f.name}** — ${f.subtype}${f.is_standard14 ? " (Standard-14)" : ""} — ` +
              `${f.embedded ? "✅ embedded" : "❌ not embedded"} — ` +
              `${f.has_to_unicode ? "✅ ToUnicode" : "✕ no ToUnicode"}`,
          )
          .join("\n")
      : "_No fonts referenced._";

    const sanitize = r.sanitize_preview.length
      ? r.sanitize_preview.map((e) => `- \`/${e}\``).join("\n")
      : "_Nothing to strip — already clean._";

    const suggestions = r.suggestions.length
      ? r.suggestions.map((s, i) => `${i + 1}. ${s}`).join("\n")
      : "_No suggestions — file is in great shape._";

    return [
      `# PDF/A Compliance Report — ${name}`,
      `_Generated by Slab Loupe • ${new Date().toISOString().replace("T", " ").slice(0, 19)} UTC • 100% local, never sent to a server_`,
      ``,
      `**File:** \`${r.input_path}\`  `,
      `**PDF version:** ${r.pdf_version}  `,
      `**Pages:** ${r.page_count}  `,
      `**Size:** ${prettySize(r.file_bytes)}  `,
      `**Encrypted:** ${r.encrypted ? "yes" : "no"}`,
      ``,
      `## Conformance Levels`,
      ``,
      `| Level | Verdict | Errors |`,
      `|-------|---------|--------|`,
      rows,
      ``,
      `## Fonts (${fontList.length})`,
      ``,
      fontRows,
      ``,
      `## Forbidden Entries (Bedrock will strip on conversion)`,
      ``,
      sanitize,
      ``,
      `## Recommended Actions`,
      ``,
      suggestions,
      ``,
      `---`,
      `Generated by [Slab](https://github.com/Sanjays2402/slab) — free, offline, ISO 19005-2 compliance inspector. No file ever left your machine.`,
    ].join("\n");
  }

  async function copyMarkdown() {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(asMarkdown());
      copied = true;
      setTimeout(() => (copied = false), 1800);
    } catch (e) {
      console.error("clipboard write failed", e);
    }
  }

  function clearReport() {
    input = null;
    report = null;
    status = idle;
  }
</script>

<div class="loupe">
  <header class="head">
    <div class="head-left">
      <span class="lens">⌕</span>
      <div>
        <h1>Loupe</h1>
        <p class="subtitle">PDF/A Compliance Inspector — ISO 19005-2, fully offline</p>
      </div>
    </div>
    {#if report}
      <div class="head-actions">
        <button class="btn" onclick={runInspect} disabled={status.kind === "working"}>Re-inspect</button>
        <button class="btn primary" onclick={copyMarkdown}>
          {copied ? "Copied ✓" : "Copy report as Markdown"}
        </button>
        <button class="btn ghost" onclick={clearReport}>Clear</button>
      </div>
    {/if}
  </header>

  {#if !input}
    <button class="dropzone" onclick={pickInput} aria-label="Pick a PDF to inspect">
      <div class="drop-art" aria-hidden="true">
        <div class="paper"></div>
        <div class="lens-big">⌕</div>
      </div>
      <h2>Drop a PDF — or click to pick one</h2>
      <p>
        Inspect any PDF against ISO 19005-2 archival rules in milliseconds.
        100% local. Slab never sends your file to a server.
      </p>
      <p class="competitive">
        Adobe Acrobat Pro Preflight: <strong>$239/yr</strong>. Slab Loupe: <strong>free, forever</strong>.
      </p>
    </button>
  {:else if status.kind === "working"}
    <div class="loading">
      <div class="pulse"></div>
      <p>Inspecting <code>{basename(input)}</code>…</p>
    </div>
  {:else if status.kind === "err"}
    <div class="error">
      <h2>Could not read PDF</h2>
      <p>{status.msg}</p>
      <button class="btn" onclick={pickInput}>Pick another file</button>
    </div>
  {:else if report}
    <section class="meta">
      <div><span class="meta-k">File</span><span class="meta-v">{basename(report.input_path)}</span></div>
      <div><span class="meta-k">Pages</span><span class="meta-v">{report.page_count}</span></div>
      <div><span class="meta-k">Size</span><span class="meta-v">{prettySize(report.file_bytes)}</span></div>
      <div><span class="meta-k">PDF</span><span class="meta-v">v{report.pdf_version}</span></div>
      <div><span class="meta-k">Encrypted</span><span class="meta-v">{report.encrypted ? "yes" : "no"}</span></div>
      <div><span class="meta-k">Fonts</span><span class="meta-v">{fontList.length}</span></div>
    </section>

    <section class="levels">
      {#each report.levels as a (a.level)}
        <article class="level-card" data-verdict={a.verdict}>
          <div class="level-head">
            <h3>{levelLabel(a.level)}</h3>
            <span class="pill">{verdictGlyph(a.verdict)} {verdictLabel(a.verdict)}</span>
          </div>
          <p class="tagline">{levelTagline(a.level)}</p>
          <div class="stats">
            <div>
              <span class="stat-n">{a.blocking_errors}</span>
              <span class="stat-l">blocking errors</span>
            </div>
            <div>
              <span class="stat-n">{a.auto_fixable}</span>
              <span class="stat-l">auto-fixable</span>
            </div>
          </div>
        </article>
      {/each}
    </section>

    {#if report.suggestions.length > 0}
      <section class="suggestions">
        <h2>Recommended actions</h2>
        <ol>
          {#each report.suggestions as s}
            <li>{s}</li>
          {/each}
        </ol>
      </section>
    {/if}

    <section class="section">
      <button class="section-head" onclick={() => (showFonts = !showFonts)}>
        <span>Fonts ({fontList.length})</span>
        <span class="badge" data-good={allEmbedded}>
          {allEmbedded ? "All embedded ✓" : `${fontList.filter((f) => !f.embedded).length} not embedded`}
        </span>
        <span class="caret">{showFonts ? "▾" : "▸"}</span>
      </button>
      {#if showFonts}
        {#if fontList.length === 0}
          <p class="empty">No fonts referenced by this PDF.</p>
        {:else}
          <table>
            <thead>
              <tr><th>Name</th><th>Type</th><th>Embedded</th><th>ToUnicode</th><th>Standard-14</th></tr>
            </thead>
            <tbody>
              {#each fontList as f}
                <tr>
                  <td class="mono">{f.name}</td>
                  <td>{f.subtype}</td>
                  <td class={f.embedded ? "yes" : "no"}>{f.embedded ? "✓" : "✕"}</td>
                  <td class={f.has_to_unicode ? "yes" : "warn"}>{f.has_to_unicode ? "✓" : "✕"}</td>
                  <td>{f.is_standard14 ? "yes" : ""}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </section>

    <section class="section">
      <button class="section-head" onclick={() => (showSanitize = !showSanitize)}>
        <span>Forbidden entries ({report.sanitize_preview.length})</span>
        <span class="badge" data-good={report.sanitize_preview.length === 0}>
          {report.sanitize_preview.length === 0 ? "Clean ✓" : "Will be stripped on conversion"}
        </span>
        <span class="caret">{showSanitize ? "▾" : "▸"}</span>
      </button>
      {#if showSanitize}
        {#if report.sanitize_preview.length === 0}
          <p class="empty">Nothing forbidden in this document.</p>
        {:else}
          <div class="chips">
            {#each report.sanitize_preview as entry}
              <span class="chip">/{entry}</span>
            {/each}
          </div>
        {/if}
      {/if}
    </section>

    <section class="section">
      <button class="section-head" onclick={() => (showFindings = !showFindings)}>
        <span>Raw findings ({report.levels.reduce((n, a) => n + a.validation.findings.length, 0)})</span>
        <span class="caret">{showFindings ? "▾" : "▸"}</span>
      </button>
      {#if showFindings}
        {#each report.levels as a (a.level)}
          <h4 class="findings-head">{levelLabel(a.level)}</h4>
          {#if a.validation.findings.length === 0}
            <p class="empty">No findings.</p>
          {:else}
            <ul class="findings">
              {#each a.validation.findings as f}
                <li data-sev={f.severity}>
                  <span class="sev">{f.severity}</span>
                  <span class="iso">§{f.iso_section}</span>
                  <span class="msg">{f.message}</span>
                </li>
              {/each}
            </ul>
          {/if}
        {/each}
      {/if}
    </section>
  {/if}
</div>

<style>
  .loupe {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    padding: 1.5rem 2rem;
    max-width: 1100px;
    margin: 0 auto;
    color: var(--fg, #1a1a1f);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }
  .head-left { display: flex; gap: 0.85rem; align-items: center; }
  .head h1 { margin: 0; font-size: 1.4rem; letter-spacing: -0.01em; }
  .subtitle { margin: 0; color: var(--fg-muted, #6a6a73); font-size: 0.86rem; }
  .lens {
    font-size: 1.8rem;
    background: linear-gradient(135deg, #d4af37 0%, #a07d18 100%);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .head-actions { display: flex; gap: 0.5rem; }

  .btn {
    appearance: none;
    border: 1px solid var(--border, rgba(0,0,0,0.10));
    background: var(--surface-1, rgba(255,255,255,0.6));
    color: inherit;
    padding: 0.45rem 0.9rem;
    border-radius: 8px;
    font-size: 0.86rem;
    cursor: pointer;
    transition: transform 80ms ease, background 120ms ease;
    backdrop-filter: blur(8px);
  }
  .btn:hover { background: var(--surface-2, rgba(255,255,255,0.85)); }
  .btn:active { transform: translateY(1px); }
  .btn.primary {
    background: linear-gradient(135deg, #2c2c33 0%, #1a1a1f 100%);
    color: white;
    border-color: transparent;
  }
  .btn.primary:hover { filter: brightness(1.1); }
  .btn.ghost { background: transparent; }

  .dropzone {
    appearance: none;
    border: 2px dashed var(--border, rgba(0,0,0,0.15));
    border-radius: 18px;
    padding: 3.5rem 2rem;
    background:
      radial-gradient(800px 200px at 50% 0%, rgba(212,175,55,0.10), transparent 60%),
      linear-gradient(180deg, rgba(255,255,255,0.55), rgba(255,255,255,0.25));
    backdrop-filter: blur(10px);
    text-align: center;
    cursor: pointer;
    color: inherit;
    transition: border-color 160ms ease, transform 160ms ease;
  }
  .dropzone:hover {
    border-color: #d4af37;
    transform: translateY(-2px);
  }
  .dropzone h2 { margin: 1rem 0 0.4rem; font-size: 1.25rem; }
  .dropzone p { margin: 0.25rem 0; color: var(--fg-muted, #6a6a73); }
  .competitive { margin-top: 1rem !important; font-size: 0.9rem; }
  .competitive strong { color: #1a1a1f; }
  .drop-art {
    position: relative;
    width: 96px;
    height: 96px;
    margin: 0 auto;
  }
  .drop-art .paper {
    position: absolute;
    inset: 8px 16px 8px 16px;
    background: white;
    border: 1px solid rgba(0,0,0,0.12);
    border-radius: 6px;
    box-shadow: 0 4px 14px rgba(0,0,0,0.06);
  }
  .drop-art .paper::before, .drop-art .paper::after {
    content: ""; position: absolute; left: 12%; right: 12%; height: 4px;
    background: rgba(0,0,0,0.08); border-radius: 2px;
  }
  .drop-art .paper::before { top: 22%; }
  .drop-art .paper::after { top: 42%; right: 40%; }
  .lens-big {
    position: absolute;
    right: -8px; bottom: -8px;
    font-size: 2.6rem;
    color: #d4af37;
    text-shadow: 0 2px 8px rgba(0,0,0,0.18);
    animation: float 2.6s ease-in-out infinite;
  }
  @keyframes float {
    0%, 100% { transform: translate(0, 0) rotate(-8deg); }
    50%      { transform: translate(-3px, -3px) rotate(-2deg); }
  }

  .loading { text-align: center; padding: 3rem; }
  .pulse {
    width: 56px; height: 56px; margin: 0 auto 1rem;
    border-radius: 50%;
    background: radial-gradient(circle, #d4af37, transparent 70%);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { transform: scale(0.9); opacity: 0.7; }
    50%      { transform: scale(1.15); opacity: 1; }
  }
  .error { padding: 2rem; border-radius: 12px; background: rgba(220, 60, 60, 0.08); border: 1px solid rgba(220, 60, 60, 0.25); }

  .meta {
    display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 0.5rem;
    padding: 0.9rem 1rem;
    background: var(--surface-1, rgba(255,255,255,0.55));
    border: 1px solid var(--border, rgba(0,0,0,0.08));
    border-radius: 12px;
    backdrop-filter: blur(8px);
  }
  .meta > div { display: flex; flex-direction: column; gap: 0.15rem; }
  .meta-k { font-size: 0.72rem; color: var(--fg-muted, #6a6a73); text-transform: uppercase; letter-spacing: 0.06em; }
  .meta-v { font-size: 0.95rem; font-weight: 500; }

  .levels {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 1rem;
  }
  .level-card {
    padding: 1rem 1.1rem;
    border-radius: 14px;
    border: 1px solid var(--border, rgba(0,0,0,0.10));
    background: var(--surface-1, rgba(255,255,255,0.55));
    backdrop-filter: blur(10px);
    position: relative;
    overflow: hidden;
  }
  .level-card[data-verdict="pass"] {
    background:
      radial-gradient(400px 150px at 100% 0%, rgba(46, 167, 96, 0.18), transparent 70%),
      var(--surface-1, rgba(255,255,255,0.55));
    border-color: rgba(46,167,96,0.35);
  }
  .level-card[data-verdict="achievable_with_fixes"] {
    background:
      radial-gradient(400px 150px at 100% 0%, rgba(212,175,55,0.18), transparent 70%),
      var(--surface-1, rgba(255,255,255,0.55));
    border-color: rgba(212,175,55,0.35);
  }
  .level-card[data-verdict="fail"] {
    background:
      radial-gradient(400px 150px at 100% 0%, rgba(220, 60, 60, 0.14), transparent 70%),
      var(--surface-1, rgba(255,255,255,0.55));
    border-color: rgba(220, 60, 60, 0.30);
  }
  .level-head { display: flex; justify-content: space-between; align-items: center; }
  .level-head h3 { margin: 0; font-size: 1.05rem; }
  .pill {
    font-size: 0.78rem;
    padding: 0.22rem 0.55rem;
    border-radius: 999px;
    background: rgba(0,0,0,0.05);
    font-weight: 600;
  }
  .level-card[data-verdict="pass"] .pill { background: rgba(46, 167, 96, 0.18); color: #1f7a4b; }
  .level-card[data-verdict="achievable_with_fixes"] .pill { background: rgba(212,175,55,0.22); color: #7d5a10; }
  .level-card[data-verdict="fail"] .pill { background: rgba(220, 60, 60, 0.15); color: #a32525; }
  .tagline { margin: 0.35rem 0 0.8rem; font-size: 0.82rem; color: var(--fg-muted, #6a6a73); }
  .stats { display: flex; gap: 1.5rem; }
  .stats > div { display: flex; flex-direction: column; gap: 0.1rem; }
  .stat-n { font-size: 1.4rem; font-weight: 700; line-height: 1; }
  .stat-l { font-size: 0.72rem; color: var(--fg-muted, #6a6a73); text-transform: uppercase; letter-spacing: 0.04em; }

  .suggestions {
    padding: 1rem 1.25rem;
    border-radius: 12px;
    background: var(--surface-1, rgba(255,255,255,0.55));
    border: 1px solid var(--border, rgba(0,0,0,0.08));
    backdrop-filter: blur(8px);
  }
  .suggestions h2 { margin: 0 0 0.5rem; font-size: 1rem; }
  .suggestions ol { margin: 0; padding-left: 1.4rem; }
  .suggestions li { padding: 0.2rem 0; font-size: 0.9rem; }

  .section {
    border-radius: 12px;
    background: var(--surface-1, rgba(255,255,255,0.55));
    border: 1px solid var(--border, rgba(0,0,0,0.08));
    backdrop-filter: blur(8px);
    overflow: hidden;
  }
  .section-head {
    width: 100%;
    display: flex; justify-content: space-between; align-items: center; gap: 0.75rem;
    appearance: none;
    background: transparent; color: inherit; border: 0;
    padding: 0.75rem 1rem;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
  }
  .section-head:hover { background: rgba(0,0,0,0.03); }
  .badge { font-size: 0.74rem; padding: 0.18rem 0.5rem; border-radius: 999px; background: rgba(220, 60, 60, 0.15); color: #a32525; font-weight: 600; }
  .badge[data-good="true"] { background: rgba(46, 167, 96, 0.18); color: #1f7a4b; }
  .caret { color: var(--fg-muted, #6a6a73); margin-left: 0.25rem; }

  table { width: 100%; border-collapse: collapse; font-size: 0.86rem; }
  th, td { text-align: left; padding: 0.4rem 1rem; }
  thead th { font-weight: 600; color: var(--fg-muted, #6a6a73); border-bottom: 1px solid var(--border, rgba(0,0,0,0.08)); font-size: 0.74rem; text-transform: uppercase; letter-spacing: 0.05em; }
  tbody tr:nth-child(even) { background: rgba(0,0,0,0.02); }
  td.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
  td.yes { color: #1f7a4b; font-weight: 700; }
  td.no  { color: #a32525; font-weight: 700; }
  td.warn { color: #7d5a10; font-weight: 700; }

  .chips { display: flex; flex-wrap: wrap; gap: 0.4rem; padding: 0.5rem 1rem 1rem; }
  .chip {
    padding: 0.25rem 0.6rem;
    border-radius: 6px;
    background: rgba(220,60,60,0.10);
    color: #a32525;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.82rem;
  }

  .empty { padding: 0.5rem 1rem 1rem; color: var(--fg-muted, #6a6a73); font-size: 0.86rem; }
  .findings-head { margin: 0.75rem 1rem 0.25rem; font-size: 0.86rem; color: var(--fg-muted, #6a6a73); }
  .findings { list-style: none; margin: 0; padding: 0 1rem 0.75rem; }
  .findings li { display: grid; grid-template-columns: 70px 70px 1fr; gap: 0.5rem; padding: 0.3rem 0; align-items: baseline; font-size: 0.86rem; }
  .findings li[data-sev="error"]   .sev { color: #a32525; }
  .findings li[data-sev="warning"] .sev { color: #7d5a10; }
  .findings li[data-sev="info"]    .sev { color: #1f5a8c; }
  .sev { font-weight: 600; text-transform: uppercase; font-size: 0.72rem; letter-spacing: 0.05em; }
  .iso { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--fg-muted, #6a6a73); }
</style>
