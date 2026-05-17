<script lang="ts">
  // Beacon PII Highlighter panel — find emails, phone numbers, SSNs,
  // credit-cards, and (optionally, via LLM) names + addresses in a PDF.
  //
  // Workflow:
  //   1. User picks a PDF (or it pre-fills via the global slab:open-recent
  //      event the Reader / Search panels use).
  //   2. User picks which kinds to scan for (default: all built-ins on,
  //      LLM pass off so the scan is instant).
  //   3. Optionally adds custom regex patterns with human-readable labels.
  //   4. Clicks "Find PII". We fire slab_beacon_pii_find which returns
  //      both per-hit list and a summary.
  //   5. Each hit is shown as a card grouped by kind. Click a card to
  //      jump to that page in the Reader (slab:beacon-goto-page event).
  //   6. User toggles which kinds to redact, picks an output path,
  //      clicks "Redact". We fire slab_beacon_pii_redact which reuses
  //      the existing auto_redact pipeline → opaque rectangles painted
  //      on top of the matched lines, output PDF saved.
  //
  // Errors map to friendly text: "provider unavailable" → "start Ollama
  // or switch provider in settings" (only when LLM pass is on).

  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, stripExt, type CmdResult, type Status } from "$lib/types";

  type PiiKind = "email" | "ssn" | "phone" | "creditcard" | "name" | "address" | "custom";

  type PiiHit = {
    page: number;
    kind: PiiKind;
    text: string;
    label: string;
  };

  type PiiSummary = {
    emails: number;
    ssns: number;
    phones: number;
    credit_cards: number;
    names: number;
    addresses: number;
    customs: number;
    total: number;
  };

  type PiiFindReport = {
    hits: PiiHit[];
    summary: PiiSummary;
  };

  type CustomPattern = { label: string; regex: string };

  // Reactive state
  let pdfPath = $state<string | null>(null);
  let status = $state<Status>(idle);
  let hits = $state<PiiHit[]>([]);
  let summary = $state<PiiSummary | null>(null);
  let includeLlmPass = $state(false);

  // Which built-in kinds to scan for (default: all on).
  let scanEmail = $state(true);
  let scanSsn = $state(true);
  let scanPhone = $state(true);
  let scanCC = $state(true);

  // Custom patterns the user adds in the UI.
  let customs = $state<CustomPattern[]>([]);
  let newCustomLabel = $state("");
  let newCustomRegex = $state("");

  // After scan: which kinds the user wants to redact. Names/Addresses/
  // Customs default OFF (lower confidence, need review) — others default ON.
  let redactEmail = $state(true);
  let redactSsn = $state(true);
  let redactPhone = $state(true);
  let redactCC = $state(true);
  let redactNames = $state(false);
  let redactAddresses = $state(false);
  let redactCustoms = $state(false);

  onMount(() => {
    const onOpenRecent = (e: Event) => {
      const d = (e as CustomEvent).detail as { path: string } | undefined;
      if (d?.path) {
        pdfPath = d.path;
        hits = [];
        summary = null;
        status = idle;
      }
    };
    window.addEventListener("slab:open-recent", onOpenRecent);
    return () => window.removeEventListener("slab:open-recent", onOpenRecent);
  });

  async function pickPdf() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    pdfPath = picked;
    hits = [];
    summary = null;
    status = idle;
  }

  function selectedKinds(): PiiKind[] {
    const out: PiiKind[] = [];
    if (scanEmail) out.push("email");
    if (scanSsn) out.push("ssn");
    if (scanPhone) out.push("phone");
    if (scanCC) out.push("creditcard");
    return out;
  }

  function addCustom() {
    const label = newCustomLabel.trim();
    const regex = newCustomRegex.trim();
    if (!label || !regex) return;
    customs = [...customs, { label, regex }];
    newCustomLabel = "";
    newCustomRegex = "";
  }

  function removeCustom(i: number) {
    customs = customs.filter((_, idx) => idx !== i);
  }

  async function runFind() {
    if (!pdfPath) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    if (!scanEmail && !scanSsn && !scanPhone && !scanCC && customs.length === 0 && !includeLlmPass) {
      status = { kind: "err", msg: "Select at least one kind or add a custom pattern." };
      return;
    }
    status = {
      kind: "working",
      msg: includeLlmPass ? "Scanning… running regex + LLM pass." : "Scanning…",
    };
    hits = [];
    summary = null;
    try {
      const res = await invoke<CmdResult<PiiFindReport>>("slab_beacon_pii_find", {
        pdfPath,
        includeLlmPass,
        // Pass kinds only when the user has narrowed them — empty defaults to all builtins
        kinds: selectedKinds().length === 4 ? null : selectedKinds(),
        customPatterns: customs.length > 0 ? customs : null,
      });
      if (res.kind === "ok") {
        hits = res.value.hits;
        summary = res.value.summary;
        status =
          hits.length === 0
            ? { kind: "ok", msg: "No PII matches found." }
            : { kind: "ok", msg: `Found ${hits.length} match${hits.length === 1 ? "" : "es"}.` };
      } else {
        status = { kind: "err", msg: friendlyError(res.message) };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function runRedact() {
    if (!pdfPath || hits.length === 0) return;
    const presets: string[] = [];
    if (redactEmail && (summary?.emails ?? 0) > 0) presets.push("email");
    if (redactSsn && (summary?.ssns ?? 0) > 0) presets.push("ssn");
    if (redactPhone && (summary?.phones ?? 0) > 0) presets.push("phone");
    if (redactCC && (summary?.credit_cards ?? 0) > 0) presets.push("cc");

    // For names / addresses / customs we build literal-match regexes from
    // the actual hits so we redact exactly what the user saw on screen.
    const patterns: string[] = [];
    const llmKinds = new Set<PiiKind>();
    if (redactNames) llmKinds.add("name");
    if (redactAddresses) llmKinds.add("address");
    for (const h of hits) {
      if (llmKinds.has(h.kind)) {
        patterns.push(escapeRegex(h.text));
      }
    }
    if (redactCustoms) {
      for (const c of customs) patterns.push(c.regex);
    }

    if (presets.length === 0 && patterns.length === 0) {
      status = { kind: "err", msg: "Pick at least one kind to redact." };
      return;
    }

    // Pick output path next to the input, defaulting to `<name>-redacted.pdf`.
    const defaultName = stripExt(basename(pdfPath)) + "-redacted.pdf";
    const out = await save({
      defaultPath: defaultName,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof out !== "string") return;

    status = { kind: "working", msg: "Redacting…" };
    try {
      const res = await invoke<CmdResult<number>>("slab_beacon_pii_redact", {
        input: pdfPath,
        output: out,
        presets,
        patterns,
      });
      if (res.kind === "ok") {
        status = {
          kind: "ok",
          msg: `Redacted ${res.value} match${res.value === 1 ? "" : "es"} → ${basename(out)}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function escapeRegex(s: string): string {
    return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }

  function gotoHit(h: PiiHit) {
    if (pdfPath) {
      window.dispatchEvent(
        new CustomEvent("slab:open-recent", { detail: { path: pdfPath } }),
      );
    }
    window.dispatchEvent(
      new CustomEvent("slab:beacon-goto-page", { detail: { page: h.page } }),
    );
  }

  function kindLabel(k: PiiKind): string {
    return {
      email: "Email",
      ssn: "SSN",
      phone: "Phone",
      creditcard: "Card",
      name: "Name",
      address: "Address",
      custom: "Custom",
    }[k];
  }

  function kindClass(k: PiiKind): string {
    return `pill pill-${k}`;
  }

  function friendlyError(msg: string): string {
    const m = msg.toLowerCase();
    if (m.includes("provider unavailable")) {
      if (m.includes("missing api key")) return msg;
      return (
        "AI provider unavailable. Start Ollama (ollama.com) or switch provider " +
        "in settings — or uncheck the \"Include names + addresses\" option."
      );
    }
    if (m.includes("bad regex")) return "Custom regex didn't compile: " + msg;
    return msg;
  }
</script>

<section class="pii">
  <header class="header">
    <div>
      <h2>
        ✦ PII Highlighter
        <span class="beta-tag">beta</span>
      </h2>
      <p class="muted">
        Find emails, phone numbers, SSNs, credit cards — and optionally names &amp; addresses — then redact in one click.
      </p>
    </div>
  </header>

  <div class="card">
    <div class="card-row">
      <label>
        <span class="lbl">PDF</span>
        <div class="picker">
          <input
            type="text"
            readonly
            value={pdfPath ? basename(pdfPath) : ""}
            placeholder="Pick a PDF to scan…"
            aria-label="Selected PDF"
          />
          <button class="secondary" onclick={pickPdf}>Browse</button>
        </div>
      </label>
    </div>

    <div class="card-row">
      <span class="lbl">Scan for</span>
      <div class="kind-row">
        <label class="kind-check"><input type="checkbox" bind:checked={scanEmail} /> Emails</label>
        <label class="kind-check"><input type="checkbox" bind:checked={scanSsn} /> SSNs</label>
        <label class="kind-check"><input type="checkbox" bind:checked={scanPhone} /> Phones</label>
        <label class="kind-check"><input type="checkbox" bind:checked={scanCC} /> Credit cards</label>
        <label class="kind-check llm-check" title="Slower; uses your configured AI provider to flag person names and street addresses">
          <input type="checkbox" bind:checked={includeLlmPass} /> Names &amp; addresses
          <span class="ai-badge">AI</span>
        </label>
      </div>
    </div>

    <details class="custom-patterns">
      <summary>Custom patterns ({customs.length})</summary>
      <div class="custom-add">
        <input
          type="text"
          placeholder="Label (e.g. Project ID)"
          bind:value={newCustomLabel}
          aria-label="Custom pattern label"
        />
        <input
          type="text"
          placeholder="Regex (e.g. SLAB-\d{4})"
          bind:value={newCustomRegex}
          aria-label="Custom pattern regex"
        />
        <button
          class="secondary"
          onclick={addCustom}
          disabled={!newCustomLabel.trim() || !newCustomRegex.trim()}
        >
          Add
        </button>
      </div>
      {#if customs.length > 0}
        <ul class="custom-list">
          {#each customs as c, i (i)}
            <li>
              <span class="custom-label">{c.label}</span>
              <code class="custom-regex">{c.regex}</code>
              <button class="link-btn" onclick={() => removeCustom(i)} aria-label="Remove pattern">✕</button>
            </li>
          {/each}
        </ul>
      {/if}
    </details>

    <div class="card-row card-actions-row">
      <button
        class="primary"
        onclick={runFind}
        disabled={!pdfPath || status.kind === "working"}
      >
        {status.kind === "working" && status.msg.startsWith("Scanning") ? "Scanning…" : "Find PII"}
      </button>
      {#if summary}
        <span class="summary-line">
          {summary.total} total
          {#if summary.emails > 0}<span class="pill pill-email">{summary.emails} email</span>{/if}
          {#if summary.ssns > 0}<span class="pill pill-ssn">{summary.ssns} SSN</span>{/if}
          {#if summary.phones > 0}<span class="pill pill-phone">{summary.phones} phone</span>{/if}
          {#if summary.credit_cards > 0}<span class="pill pill-creditcard">{summary.credit_cards} card</span>{/if}
          {#if summary.names > 0}<span class="pill pill-name">{summary.names} name</span>{/if}
          {#if summary.addresses > 0}<span class="pill pill-address">{summary.addresses} address</span>{/if}
          {#if summary.customs > 0}<span class="pill pill-custom">{summary.customs} custom</span>{/if}
        </span>
      {/if}
    </div>
  </div>

  {#if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {:else if status.kind === "working"}
    <div class="status working">{status.msg}</div>
  {:else if status.kind === "ok" && hits.length === 0}
    <div class="status done">{status.msg}</div>
  {:else if status.kind === "ok"}
    <div class="status done">{status.msg}</div>
  {/if}

  {#if hits.length > 0}
    <ol class="hits">
      {#each hits as h, i (i)}
        <li>
          <button class="hit-card" onclick={() => gotoHit(h)}>
            <div class="hit-head">
              <span class={kindClass(h.kind)}>{h.label || kindLabel(h.kind)}</span>
              <span class="hit-page">page {h.page}</span>
            </div>
            <div class="hit-text">{h.text}</div>
          </button>
        </li>
      {/each}
    </ol>

    <div class="redact-block">
      <span class="lbl">Redact</span>
      <div class="kind-row">
        {#if (summary?.emails ?? 0) > 0}
          <label class="kind-check"><input type="checkbox" bind:checked={redactEmail} /> Emails ({summary?.emails})</label>
        {/if}
        {#if (summary?.ssns ?? 0) > 0}
          <label class="kind-check"><input type="checkbox" bind:checked={redactSsn} /> SSNs ({summary?.ssns})</label>
        {/if}
        {#if (summary?.phones ?? 0) > 0}
          <label class="kind-check"><input type="checkbox" bind:checked={redactPhone} /> Phones ({summary?.phones})</label>
        {/if}
        {#if (summary?.credit_cards ?? 0) > 0}
          <label class="kind-check"><input type="checkbox" bind:checked={redactCC} /> Credit cards ({summary?.credit_cards})</label>
        {/if}
        {#if (summary?.names ?? 0) > 0}
          <label class="kind-check"><input type="checkbox" bind:checked={redactNames} /> Names ({summary?.names})</label>
        {/if}
        {#if (summary?.addresses ?? 0) > 0}
          <label class="kind-check"><input type="checkbox" bind:checked={redactAddresses} /> Addresses ({summary?.addresses})</label>
        {/if}
        {#if (summary?.customs ?? 0) > 0}
          <label class="kind-check"><input type="checkbox" bind:checked={redactCustoms} /> Customs ({summary?.customs})</label>
        {/if}
      </div>
      <button
        class="primary danger"
        onclick={runRedact}
        disabled={status.kind === "working"}
      >
        🔒 Redact selected → save as new PDF
      </button>
      <p class="muted footnote">
        Black bars are painted over matching regions. The PDF text stream is unchanged
        (use the Sanitize panel to flatten if you need permanent removal).
      </p>
    </div>
  {/if}
</section>

<style>
  .pii {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    flex: 1;
  }
  .header h2 {
    margin: 0;
  }
  .header .muted {
    margin: 4px 0 0;
    color: var(--text-3);
    font-size: 13px;
  }
  .beta-tag {
    font-size: 10px;
    color: var(--accent);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    margin-left: 8px;
    font-weight: 500;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    vertical-align: middle;
  }

  .card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .card-actions-row {
    flex-direction: row;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .lbl {
    font-size: 12px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .picker {
    display: flex;
    gap: 6px;
  }
  .picker input {
    flex: 1;
  }

  .kind-row {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 18px;
    align-items: center;
  }
  .kind-check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--text-2);
    cursor: pointer;
  }
  .kind-check input[type="checkbox"] {
    accent-color: var(--accent);
  }
  .llm-check {
    color: var(--text-2);
  }
  .ai-badge {
    font-size: 9px;
    background: var(--accent);
    color: #000;
    padding: 1px 5px;
    border-radius: 3px;
    margin-left: 4px;
    letter-spacing: 0.6px;
    font-weight: 600;
  }

  .custom-patterns {
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 8px 12px;
  }
  .custom-patterns summary {
    cursor: pointer;
    font-size: 12px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .custom-add {
    display: flex;
    gap: 6px;
    margin-top: 10px;
    align-items: stretch;
  }
  .custom-add input {
    flex: 1;
  }
  .custom-list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .custom-list li {
    display: flex;
    gap: 8px;
    align-items: center;
    background: var(--bg-2);
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 12px;
  }
  .custom-label {
    color: var(--accent);
    font-weight: 600;
  }
  .custom-regex {
    flex: 1;
    color: var(--text-2);
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link-btn {
    background: none;
    border: 0;
    color: var(--text-3);
    cursor: pointer;
    padding: 2px 6px;
  }
  .link-btn:hover {
    color: #fca5a5;
  }

  .summary-line {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 12px;
    color: var(--text-3);
    align-items: center;
  }

  .pill {
    display: inline-block;
    font-size: 11px;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-2);
  }
  .pill-email { color: #93c5fd; border-color: #1e3a8a; }
  .pill-ssn { color: #fca5a5; border-color: #7f1d1d; }
  .pill-phone { color: #6ee7b7; border-color: #064e3b; }
  .pill-creditcard { color: #fcd34d; border-color: #78350f; }
  .pill-name { color: #c4b5fd; border-color: #4c1d95; }
  .pill-address { color: #f9a8d4; border-color: #831843; }
  .pill-custom { color: var(--accent); border-color: var(--accent); }

  .status {
    padding: 8px 12px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .status.err {
    background: rgba(220, 38, 38, 0.08);
    color: #fca5a5;
    border: 1px solid rgba(220, 38, 38, 0.3);
  }
  .status.working {
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
  }
  .status.done {
    background: var(--bg-2);
    color: var(--text-3);
    border: 1px solid var(--border);
  }

  .hits {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .hit-card {
    width: 100%;
    text-align: left;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 8px 12px;
    cursor: pointer;
    color: var(--text-1);
    transition: border-color 80ms ease, background 80ms ease;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .hit-card:hover {
    background: var(--bg-3);
    border-color: var(--accent);
  }
  .hit-head {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 12px;
    color: var(--text-3);
  }
  .hit-page {
    font-size: 11px;
    color: var(--text-3);
  }
  .hit-text {
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
    color: var(--text-1);
    word-break: break-all;
  }

  .redact-block {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .footnote {
    font-size: 11px;
    margin: 0;
  }
  .primary.danger {
    background: #b91c1c;
    color: #fff;
  }
  .primary.danger:hover {
    background: #dc2626;
  }
  .muted {
    color: var(--text-3);
  }
</style>
