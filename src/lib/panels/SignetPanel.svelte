<script lang="ts">
  // v3.10.0 "Signet" — PKCS#7 digital signature panel.
  //
  // Three flows in one panel:
  //   1. Load identity   — pick PEM cert + key, preview signer info.
  //   2. Sign            — pick input PDF, output path, optional reason/location, sign.
  //   3. Verify          — pick a signed PDF, list every signature with status.
  //
  // 100% offline. RustCrypto stack — Adobe Acrobat-compatible
  // adbe.pkcs7.detached signatures, RSA-PKCS#1 v1.5 with SHA-256.
  // ECDSA arrives in v3.10.1.

  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type Status } from "$lib/types";

  interface IdentityPreview {
    subject_cn: string;
    algorithm: string;
    not_before_unix: number;
    not_after_unix: number;
    chain_len: number;
  }

  interface SignResult {
    output_bytes: number;
    byte_range: [number, number, number, number];
    field_name: string;
    signature_hex_used: number;
    elapsed_ms: number;
  }

  type Coverage = "FullDocument" | "PartialDocument";
  type DigestStatus = "Match" | "Mismatch";
  type CryptoStatus = "Valid" | "Invalid" | "UnsupportedAlgorithm";
  type ChainStatus = "Trusted" | "Untrusted" | "Expired" | "ChainBroken" | "SelfSigned" | "NotYetValid";

  interface VerifiedSignature {
    field_name: string;
    signer_cn: string;
    signed_at_unix: number;
    byte_range: [number, number, number, number];
    coverage: Coverage;
    digest_status: DigestStatus;
    crypto_status: CryptoStatus;
    chain_status: ChainStatus;
    cert_subject: string;
    cert_issuer: string;
    cert_not_before: number;
    cert_not_after: number;
  }

  // ─── Identity state ────────────────────────────────────────────
  let certPath = $state<string | null>(null);
  let keyPath = $state<string | null>(null);
  let keyPassword = $state("");
  let identity = $state<IdentityPreview | null>(null);
  let identityStatus = $state<Status>(idle);

  // ─── Sign state ────────────────────────────────────────────────
  let inputPath = $state<string | null>(null);
  let outputPath = $state<string | null>(null);
  let reason = $state("");
  let location = $state("");
  let tsaUrl = $state("");
  // Visible signature appearance (v3.11.0).
  let visibleSignature = $state(false);
  let appearancePage = $state(1);
  let appearanceX = $state(50);
  let appearanceY = $state(50);
  let appearanceW = $state(220);
  let appearanceH = $state(80);
  let contactInfo = $state("");
  let fieldName = $state("");
  let signResult = $state<SignResult | null>(null);
  let signStatus = $state<Status>(idle);

  // ─── Verify state ──────────────────────────────────────────────
  let verifyInputPath = $state<string | null>(null);
  let verifyResults = $state<VerifiedSignature[]>([]);
  let verifyStatus = $state<Status>(idle);

  // ─── Identity actions ──────────────────────────────────────────
  async function pickCert() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "PEM Certificate", extensions: ["pem", "crt", "cer"] }],
    });
    if (typeof sel === "string") certPath = sel;
  }

  async function pickKey() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "PEM Private Key", extensions: ["pem", "key"] }],
    });
    if (typeof sel === "string") keyPath = sel;
  }

  async function loadIdentity() {
    if (!certPath || !keyPath) return;
    identityStatus = { kind: "working", msg: "Loading identity…" };
    try {
      const result = await invoke<IdentityPreview>("signet_load_identity", {
        certPemPath: certPath,
        keyPemPath: keyPath,
        keyPassword: keyPassword || null,
      });
      identity = result;
      identityStatus = { kind: "ok", msg: `Loaded ${result.subject_cn}` };
    } catch (err) {
      identity = null;
      identityStatus = { kind: "err", msg: String(err) };
    }
  }

  // ─── Sign actions ──────────────────────────────────────────────
  async function pickInputForSign() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof sel === "string") {
      inputPath = sel;
      // Auto-suggest output path: foo.pdf → foo.signed.pdf
      if (!outputPath) {
        outputPath = sel.replace(/\.pdf$/i, "") + ".signed.pdf";
      }
    }
  }

  async function pickOutputForSign() {
    const sel = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: outputPath ?? undefined,
    });
    if (sel) outputPath = sel;
  }

  async function signNow() {
    if (!inputPath || !outputPath || !certPath || !keyPath) return;
    signStatus = { kind: "working", msg: "Signing PDF…" };
    signResult = null;
    try {
      const result = await invoke<SignResult>("signet_sign", {
        args: {
          input_path: inputPath,
          output_path: outputPath,
          cert_pem_path: certPath,
          key_pem_path: keyPath,
          key_password: keyPassword || null,
          reason: reason || null,
          location: location || null,
          contact_info: contactInfo || null,
          field_name: fieldName || null,
          tsa_url: tsaUrl || null,
          appearance: visibleSignature
            ? {
                page: Math.max(1, Math.floor(appearancePage || 1)),
                rect: [
                  Number(appearanceX),
                  Number(appearanceY),
                  Number(appearanceX) + Number(appearanceW),
                  Number(appearanceY) + Number(appearanceH),
                ],
                show_name: true,
                show_date: true,
                show_reason: Boolean(reason),
                show_location: Boolean(location),
                font_size: 9.0,
              }
            : null,
        },
      });
      signResult = result;
      signStatus = {
        kind: "ok",
        msg: `Signed in ${result.elapsed_ms}ms · ${humanBytes(result.output_bytes)}`,
      };
    } catch (err) {
      signStatus = { kind: "err", msg: String(err) };
    }
  }

  // ─── Verify actions ────────────────────────────────────────────
  async function pickInputForVerify() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Signed PDF", extensions: ["pdf"] }],
    });
    if (typeof sel === "string") {
      verifyInputPath = sel;
      verifyResults = [];
      verifyStatus = idle;
    }
  }

  async function verifyNow() {
    if (!verifyInputPath) return;
    verifyStatus = { kind: "working", msg: "Verifying signatures…" };
    verifyResults = [];
    try {
      const result = await invoke<VerifiedSignature[]>("signet_verify", {
        inputPath: verifyInputPath,
      });
      verifyResults = result;
      verifyStatus = result.length
        ? { kind: "ok", msg: `${result.length} signature${result.length === 1 ? "" : "s"} found` }
        : { kind: "ok", msg: "No signatures in this PDF" };
    } catch (err) {
      verifyStatus = { kind: "err", msg: String(err) };
    }
  }

  // ─── Helpers ───────────────────────────────────────────────────
  function humanBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(2)} MB`;
  }

  function fmtUnix(t: number): string {
    if (!t) return "—";
    return new Date(t * 1000).toLocaleString();
  }

  function statusBadge(kind: string, ok: boolean): string {
    return ok ? "badge ok" : "badge warn";
  }

  const canLoadIdentity = $derived(!!certPath && !!keyPath);
  const canSign = $derived(!!inputPath && !!outputPath && !!identity);
  const canVerify = $derived(!!verifyInputPath);
</script>

<section class="signet">
  <header>
    <h2>Signet — Digital Signatures</h2>
    <p class="hint">
      Sign and verify PDFs offline. Adobe Acrobat-compatible PKCS#7 detached
      signatures (RSA-SHA-256). Revocation checks (CRL/OCSP) are not performed
      in this release.
    </p>
  </header>

  <!-- ─── Identity ─────────────────────────────────────────────── -->
  <fieldset>
    <legend>1. Signing identity</legend>
    <div class="row">
      <label>Certificate (PEM)</label>
      <button onclick={pickCert}>{certPath ? basename(certPath) : "Choose…"}</button>
    </div>
    <div class="row">
      <label>Private key (PEM)</label>
      <button onclick={pickKey}>{keyPath ? basename(keyPath) : "Choose…"}</button>
    </div>
    <div class="row">
      <label>Key password</label>
      <input type="password" bind:value={keyPassword} placeholder="optional" />
    </div>
    <div class="actions">
      <button class="primary" disabled={!canLoadIdentity} onclick={loadIdentity}>
        Load identity
      </button>
      {#if identityStatus.kind !== "idle"}
        <span class="status {identityStatus.kind}">{identityStatus.msg}</span>
      {/if}
    </div>

    {#if identity}
      <dl class="identity-preview">
        <dt>Signer</dt>
        <dd>{identity.subject_cn}</dd>
        <dt>Algorithm</dt>
        <dd>{identity.algorithm}</dd>
        <dt>Valid</dt>
        <dd>{fmtUnix(identity.not_before_unix)} → {fmtUnix(identity.not_after_unix)}</dd>
        <dt>Chain</dt>
        <dd>{identity.chain_len} intermediate cert{identity.chain_len === 1 ? "" : "s"}</dd>
      </dl>
    {/if}
  </fieldset>

  <!-- ─── Sign ─────────────────────────────────────────────────── -->
  <fieldset>
    <legend>2. Sign a PDF</legend>
    <div class="row">
      <label>Input PDF</label>
      <button onclick={pickInputForSign}>
        {inputPath ? basename(inputPath) : "Choose…"}
      </button>
    </div>
    <div class="row">
      <label>Output PDF</label>
      <button onclick={pickOutputForSign}>
        {outputPath ? basename(outputPath) : "Choose…"}
      </button>
    </div>
    <div class="row">
      <label>Reason</label>
      <input type="text" bind:value={reason} placeholder="e.g. I approve this contract" />
    </div>
    <div class="row">
      <label>Location</label>
      <input type="text" bind:value={location} placeholder="e.g. San Francisco, CA" />
    </div>
    <div class="row">
      <label>Contact</label>
      <input type="text" bind:value={contactInfo} placeholder="optional" />
    </div>
    <div class="row">
      <label>Field name</label>
      <input type="text" bind:value={fieldName} placeholder="Signature1" />
    </div>
    <div class="row">
      <label>TSA URL</label>
      <input
        type="text"
        bind:value={tsaUrl}
        placeholder="optional — RFC 3161 (e.g. http://timestamp.digicert.com)"
      />
    </div>
    <fieldset class="appearance">
      <legend>
        <label class="toggle">
          <input type="checkbox" bind:checked={visibleSignature} />
          <span>Visible signature stamp</span>
        </label>
      </legend>
      {#if visibleSignature}
        <p class="hint">
          Renders a Form XObject on the page so Acrobat / Preview / Foxit show
          the signature inline. Coordinates are in PDF user-space points
          (72&nbsp;pt&nbsp;=&nbsp;1&nbsp;inch, origin bottom-left).
        </p>
        <div class="grid">
          <label>Page<input type="number" min="1" bind:value={appearancePage} /></label>
          <label>X<input type="number" bind:value={appearanceX} /></label>
          <label>Y<input type="number" bind:value={appearanceY} /></label>
          <label>Width<input type="number" min="40" bind:value={appearanceW} /></label>
          <label>Height<input type="number" min="20" bind:value={appearanceH} /></label>
        </div>
      {/if}
    </fieldset>
    <div class="actions">
      <button class="primary" disabled={!canSign} onclick={signNow}>Sign PDF</button>
      {#if signStatus.kind !== "idle"}
        <span class="status {signStatus.kind}">{signStatus.msg}</span>
      {/if}
    </div>

    {#if signResult}
      <dl class="sign-result">
        <dt>Output</dt>
        <dd>{humanBytes(signResult.output_bytes)}</dd>
        <dt>Field</dt>
        <dd>{signResult.field_name}</dd>
        <dt>ByteRange</dt>
        <dd class="mono">[{signResult.byte_range.join(", ")}]</dd>
        <dt>Signature</dt>
        <dd>{signResult.signature_hex_used} hex chars used</dd>
      </dl>
    {/if}
  </fieldset>

  <!-- ─── Verify ───────────────────────────────────────────────── -->
  <fieldset>
    <legend>3. Verify a signed PDF</legend>
    <div class="row">
      <label>Signed PDF</label>
      <button onclick={pickInputForVerify}>
        {verifyInputPath ? basename(verifyInputPath) : "Choose…"}
      </button>
    </div>
    <div class="actions">
      <button class="primary" disabled={!canVerify} onclick={verifyNow}>Verify</button>
      {#if verifyStatus.kind !== "idle"}
        <span class="status {verifyStatus.kind}">{verifyStatus.msg}</span>
      {/if}
    </div>

    {#if verifyResults.length > 0}
      <ul class="verify-list">
        {#each verifyResults as sig (sig.field_name)}
          <li class="sig-card">
            <header>
              <strong>{sig.field_name}</strong>
              <span class="signer">{sig.signer_cn}</span>
              <time>{fmtUnix(sig.signed_at_unix)}</time>
            </header>
            <div class="badges">
              <span class={statusBadge(sig.digest_status, sig.digest_status === "Match")}>
                Digest: {sig.digest_status}
              </span>
              <span class={statusBadge(sig.crypto_status, sig.crypto_status === "Valid")}>
                Crypto: {sig.crypto_status}
              </span>
              <span class={statusBadge(sig.coverage, sig.coverage === "FullDocument")}>
                Coverage: {sig.coverage === "FullDocument" ? "Full" : "Partial"}
              </span>
              <span
                class={statusBadge(
                  sig.chain_status,
                  sig.chain_status === "Trusted" || sig.chain_status === "SelfSigned",
                )}
              >
                Chain: {sig.chain_status}
              </span>
            </div>
            <details>
              <summary>Certificate details</summary>
              <dl>
                <dt>Subject</dt>
                <dd class="mono">{sig.cert_subject}</dd>
                <dt>Issuer</dt>
                <dd class="mono">{sig.cert_issuer}</dd>
                <dt>Valid</dt>
                <dd>{fmtUnix(sig.cert_not_before)} → {fmtUnix(sig.cert_not_after)}</dd>
                <dt>ByteRange</dt>
                <dd class="mono">[{sig.byte_range.join(", ")}]</dd>
              </dl>
            </details>
          </li>
        {/each}
      </ul>
    {/if}
  </fieldset>
</section>

<style>
  .signet {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 16px;
    color: var(--text);
    max-width: 720px;
  }
  header h2 {
    margin: 0 0 4px;
    font-size: 18px;
    font-weight: 600;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  fieldset {
    border: 1px solid var(--border-subtle);
    border-radius: 10px;
    padding: 14px 16px;
    background: var(--panel-bg);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  legend {
    padding: 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .row {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 10px;
    align-items: center;
  }
  .row label {
    font-size: 12px;
    color: var(--text-muted);
  }
  .row button,
  .row input {
    width: 100%;
    padding: 6px 10px;
    border-radius: 6px;
    border: 1px solid var(--border-subtle);
    background: var(--input-bg);
    color: var(--text);
    font-size: 13px;
    text-align: left;
  }
  .row input::placeholder {
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 6px;
  }
  button.primary {
    padding: 7px 16px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: var(--accent);
    color: var(--accent-fg);
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
  }
  button.primary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .status {
    font-size: 12px;
  }
  .status.ok {
    color: var(--ok, #22c55e);
  }
  .status.err {
    color: var(--err, #ef4444);
  }
  .status.loading {
    color: var(--text-muted);
  }
  dl.identity-preview,
  dl.sign-result {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 4px 10px;
    margin: 0;
    padding-top: 8px;
    border-top: 1px solid var(--border-subtle);
    font-size: 12px;
  }
  dl dt {
    color: var(--text-muted);
  }
  dl dd {
    margin: 0;
  }
  .mono {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 11.5px;
  }
  ul.verify-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sig-card {
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 10px 12px;
    background: var(--input-bg);
  }
  .sig-card header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 8px;
  }
  .sig-card header strong {
    font-size: 13px;
  }
  .sig-card .signer {
    font-size: 12px;
    color: var(--text-muted);
  }
  .sig-card time {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-muted);
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 6px;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--border-subtle);
    background: var(--panel-bg);
  }
  .badge.ok {
    color: #16a34a;
    border-color: rgba(34, 197, 94, 0.4);
    background: rgba(34, 197, 94, 0.08);
  }
  .badge.warn {
    color: #d97706;
    border-color: rgba(245, 158, 11, 0.4);
    background: rgba(245, 158, 11, 0.08);
  }
  details summary {
    cursor: pointer;
    font-size: 11px;
    color: var(--text-muted);
  }
  details dl {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 4px 8px;
    margin: 6px 0 0;
    font-size: 11.5px;
  }
  fieldset.appearance {
    margin: 8px 0 4px;
    padding: 8px 12px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    background: var(--panel-bg);
  }
  fieldset.appearance legend {
    padding: 0 6px;
    font-size: 12px;
    color: var(--text-muted);
  }
  fieldset.appearance .toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  fieldset.appearance .hint {
    margin: 6px 0 8px;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  fieldset.appearance .grid {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 6px 10px;
  }
  fieldset.appearance .grid label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 11px;
    color: var(--text-muted);
  }
  fieldset.appearance .grid input {
    padding: 4px 6px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--input-bg, transparent);
    color: inherit;
  }
</style>
