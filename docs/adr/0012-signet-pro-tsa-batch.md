# ADR 0012 — Signet Pro: RFC 3161 timestamping, visible appearances, batch signing

**Status:** Accepted (v3.11.0)
**Date:** 2026-05-23
**Supersedes:** none
**Related:** [ADR 0011 — PKCS#7 signatures via RustCrypto](0011-pkcs7-signatures-rustcrypto.md)

## Context

v3.10.0 Signet shipped offline PKCS#7-detached signing and verification
(CAdES-BES grade). That closes the basic "sign a PDF" gap with Adobe Acrobat,
but three enterprise capabilities remain before a 500-person law firm or IT
admin would buy seats of Slab over Acrobat Pro:

1. **Long-term validation (RFC 3161 timestamping).** Courts and regulated
   archives require signatures to remain verifiable after the signing
   certificate expires. CAdES-BES alone doesn't provide that — only an
   external timestamp from a trusted TSA does.
2. **Visible appearances.** Acrobat Reader is the canonical PDF viewer, and
   it renders signatures as visible stamps on the page (signer name, date,
   reason). Without that, end users believe the document "isn't signed."
3. **Bulk signing.** A paralegal processing 200 contracts a day cannot
   click-sign each one. Acrobat Pro charges $239/yr in part for this
   workflow; LibreOffice and macOS Preview don't ship it at all.

## Decision

### 1. RFC 3161 timestamping — opt-in, offline-default

- `SignOptions` gains `tsa_url: Option<String>` and `tsa_verify_chain: bool`.
- Default is `None` (no network call). When set, the signer:
  1. computes SHA-256 over the CMS `SignerInfo.signature` field;
  2. builds a `TimeStampReq` via `der`/`x509-cert` crates;
  3. POSTs it to `tsa_url` with `Content-Type: application/timestamp-query`
     using `reqwest` in `rustls-tls-only` mode (no OpenSSL);
  4. parses the `TimeStampResp`;
  5. embeds the returned TST as an **unsigned attribute** with OID
     `1.2.840.113549.1.9.16.2.14` (`id-aa-timeStampToken`) inside the CMS
     SignerInfo.
- Trust anchors for TSA certs live in
  `~/.config/slab/signet/tsa-trust/*.pem` (separate from signing trust store).
- Verifier (in `pdf/signet/verify.rs`) is taught to detect the unsigned attr
  and surface `timestamped_at: Option<DateTime<Utc>>` in `VerifiedSignature`.

**Why RustCrypto/rustls only?** Same rationale as ADR 0011 — single-binary
deploy, no OpenSSL surface, consistent across mac/win/linux.

**Why opt-in?** Slab's wedge is "your file never leaves your machine." A
default-on TSA call would phone home on every sign and break the promise.

### 2. Visible appearances — Form XObject in AP/N

- `SignOptions` gains `appearance: Option<AppearanceSpec>`.
- When set, the existing Widget annotation gets a non-zero `/Rect` and an
  `/AP` dictionary whose `/N` entry references a Form XObject with a content
  stream rendering signer CN, signing time, reason, location.
- Helvetica is embedded as a standard 14 font (no external font files).
- Optional `image: Vec<u8>` (PNG/JPEG) is embedded as an Image XObject and
  drawn as a background watermark.

**Why a Form XObject, not raw page content?** PDF spec §12.7.4.5 — Acrobat
will only re-render an appearance on viewer changes (zoom, print) if it's
inside `/AP /N`. Drawing directly on the page bakes the appearance at sign
time and looks wrong at other zoom levels.

### 3. Batch sign — rayon over input folder

- New `batch::sign_folder(input_dir, output_dir, &identity, &opts) -> BatchReport`.
- Uses `rayon::par_iter` over `*.pdf` entries — `SigningIdentity` is `Send +
  Sync` so this is cheap.
- Per-file errors don't abort the batch — they're collected into the report.
- Progress is surfaced via a `&dyn Fn(done, total)` callback so the Tauri
  command can emit `batch-sign-progress` events to the frontend without
  taking a hard dependency on Tauri inside the crate.

**Why rayon, not tokio?** Signing is CPU-bound (SHA-256 + RSA). No I/O
multiplexing benefit. Rayon's work-stealing scheduler matches the workload.

## Consequences

**Positive:**
- CAdES-T grade signatures unlock long-term archival use cases (notaries,
  legal contracts, regulatory filings).
- Visible appearances make signatures look right in Acrobat Reader —
  removes the "is this even signed?" support question.
- Batch sign is a flagship demo: 50 contracts in 8 seconds, offline.

**Negative / risks:**
- TSA call is the first outbound network request Signet makes. Mitigated by:
  (a) it's opt-in, (b) it's a single POST with no PII (just a SHA-256
  digest), (c) Settings UI flags it clearly.
- Appearance rendering is a Helvetica subset only — no Unicode beyond
  Latin-1. Acceptable for v3.11.0; full Unicode appearances deferred to
  v3.11.1 if customers ask.
- Batch sign multiplies disk I/O — fine for SSDs (the cron's deploy target)
  but could spike load on spinning rust. Document this in the help tooltip.

**Reversibility:** All three are additive — existing v3.10.0 sign/verify
flows continue to work unchanged when the new `SignOptions` fields are
`None`/`Default`.

## Implementation plan

See [`docs/plans/2026-05-23-v3.11.0-signet-pro.md`](../plans/2026-05-23-v3.11.0-signet-pro.md)
for the 8-task bite-sized breakdown.
