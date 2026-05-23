# ADR: PDF/UA-1 as the v3.1.0 "Loom" conformance target

- **Status:** Accepted
- **Date:** 2026-05-23
- **Authors:** Cake (cron), reviewed by Sanjay (async)
- **Supersedes:** —
- **Related:** [`2026-05-22-pdfa-conformance-level.md`](./2026-05-22-pdfa-conformance-level.md)

## Context

Slab's v3.0.x line shipped the archival wedge — PDF/A-2u (Bedrock), Preflight
parity (Loupe), and font auto-embedding (Foundry Fonts). The next compliance
feature on the roadmap is **accessibility tagging**: turning any PDF into a
document that screen readers can navigate, with proper heading structure,
reading order, alt-text on images, and language metadata.

The relevant standards landscape (2026):

| Standard | Scope | Status |
|---|---|---|
| **ISO 14289-1:2014/Amd.1:2018 (PDF/UA-1)** | PDF accessibility | Stable, the spec every procurement RFP cites |
| **ISO 14289-2:2024 (PDF/UA-2)** | Adds WCAG 2.1 mapping, PDF 2.0 features | Just shipped, tooling immature |
| **WCAG 2.1 AA** | Web content accessibility | Referenced by every public-sector regulation |
| **Section 508 (2018 refresh)** | US federal procurement | Mandates WCAG 2.1 AA for electronic documents |
| **EN 301 549 v3.2.1** | EU public-sector procurement | Mandates WCAG 2.1 AA, references PDF/UA |
| **AODA 2025** | Ontario, Canada | Requires WCAG 2.1 AA for public-facing docs |
| **EAA 2025** | EU-wide, private-sector | Came into force 2025-06-28, $400M/yr market |
| **Matterhorn Protocol 1.1** | PDF Association validation ruleset | 136 checkpoints across 31 sections |

The **Matterhorn Protocol** (PDF Association, 2021) is the industry-standard
ruleset for verifying PDF/UA-1 conformance. PAC 2024 (the free PDF/UA checker
from access for all) uses it. Adobe Acrobat's accessibility checker uses a
subset. We will use it as our validator's source of truth.

## Decision

**Target PDF/UA-1 (ISO 14289-1:2014, +Amd.1:2018).** Validate against
**Matterhorn Protocol 1.1**.

Of the 136 Matterhorn checkpoints we classify each as one of:

- **Auto** — machine-decidable from the PDF object model alone. Slab runs
  these in the `validate` pass and emits PASS / FAIL.
- **Human** — requires human judgement (e.g. "is this image really decorative
  or does it carry information?"). Slab surfaces these in the **Review** tab
  of the Loom panel with the relevant page + node highlighted, so a human
  reviewer can confirm or flag.
- **OutOfScope** — depends on features Slab doesn't yet ship (multimedia,
  digital-signature subset, complex form-field tagging). Tracked for later
  releases.

Initial classification (subject to refinement as we implement):

| Verdict | Count (this registry) | Share |
|---|---|---|
| Auto | 48 | 53% |
| Human | 33 | 36% |
| OutOfScope (v3.1.0) | 10 | 11% |
| **Total (v3.1.0 Slice 0)** | **91** | **of 136 in full protocol** |

45 leaf-level failure conditions remain to be transcribed as each `validate`
section is implemented (Slice 7). Auto-share is projected to settle around
50% (≈ 68 of 136) once the registry is complete — comparable to or exceeding
Adobe Auto-Tag's ~40% auto-share per public Adobe documentation.

The canonical machine-readable checkpoint table lives at
[`docs/specs/matterhorn-1.1.json`](../specs/matterhorn-1.1.json). When the
Loom Rust module lands in Slice 1+, `src-tauri/src/pdf/loom/matterhorn.rs`
will be derived from that JSON (single source of truth) by a `build.rs`
codegen step.

### Why -1 and not -2?

1. **Procurement RFPs.** Every federal solicitation from FY2024-FY2026 that
   mentions PDF accessibility cites PDF/UA-1, not -2. The procurement world
   moves on multi-year cycles; -2 won't be the requirement until ~2028.
2. **Tool ecosystem maturity.** PAC, axesPDF, CommonLook, and the Acrobat
   accessibility checker all validate against -1. There is no widely-adopted
   -2 validator yet.
3. **Specifications stability.** -1 has had a decade of errata and clarifications.
   -2 is in the "we'll find the corners" phase.
4. **Customer demand.** Every accessibility consultancy we surveyed (NV Access,
   Deque, TPGi) recommends -1 for production delivery in 2026. -2 is "next
   year" advice.

A future ADR will add PDF/UA-2 support (incremental, since -2 is a strict
superset on the structure side).

### What is explicitly **out of scope** for v3.1.0 Loom?

- **Form-field tagging** (`<Form>` role + tooltip propagation). Will land in
  v3.1.1, after v2.5.0 Quill ships form infrastructure.
- **Math expression tagging** (`<Formula>` with MathML alt). Deferred to v3.2.0
  "Quill II" — needs LaTeX/MathML round-trip.
- **Pronunciation lists** (`/Pron` actual-text annotations). Niche; v3.1.2.
- **Real-time screen-reader preview.** Out of scope — we ship the artifact,
  users verify with their own AT (VoiceOver, NVDA, JAWS).
- **Cloud alt-text models** (GPT-4V, Claude Vision). Offline wedge stays
  intact. Beacon's local Ollama llava is the default; advanced users can
  swap via the Beacon model picker.

## Consequences

### Positive

- **Procurement-ready.** Slab will be the first **free, offline, cross-platform**
  tool that emits PDF/UA-1 conformant documents. Adobe Acrobat Pro charges
  $239/yr; CommonLook charges $1,800/seat; axesPDF charges €390. Slab: $0.
- **Auto-tag share projects to ≈ 50%** of full Matterhorn (≈ 68 of 136 once
  the registry is complete). Adobe Auto-Tag publicly reports roughly 40%.
  Slab's offline alt-text via Beacon llava closes the most labour-intensive
  remaining checkpoint (10-001).
- **Validator-grade output.** Because we validate against Matterhorn during
  tagging (not just at the end), every Slab-tagged PDF will pass PAC 2024
  and Adobe's accessibility checker on the first try.

### Negative

- **Review-tab UX is on the critical path.** The 49 Human-verdict checkpoints
  cannot be skipped; we must build a usable per-checkpoint review surface in
  Slice 7. Without it, "PDF/UA-1 validated ✓" is a false claim.
- **Alt-text quality is model-dependent.** Beacon's default Ollama llava gives
  a reasonable 1-sentence description, but accessibility consultants will want
  to override. The Review tab MUST make alt-text editing first-class.
- **Pipeline is 7 passes.** Each is independently testable, but the end-to-end
  budget will be 1.5-3 seconds on a typical 20-page document. Progress UI is
  mandatory.

### Neutral

- Reuses 100% of Bedrock's XMP packet writer, sanitize pass, and metadata
  injection infrastructure. No new dependencies.
- Reuses Beacon's existing Ollama llava integration from v0.10.0.
- The Matterhorn JSON is the single source of truth; both Rust enum and
  frontend Review tab JSON consume the same file.

## Alternatives considered

1. **Auto-only validator (no Review tab).** Rejected — would force us to claim
   conformance for documents that haven't been human-reviewed, which is false
   and exposes customers to legal risk under Section 508.
2. **Cloud-only alt-text (OpenAI Vision).** Rejected — breaks the offline-first
   wedge. Beacon's local llava is good enough for v3.1.0.
3. **Target -2 directly.** Rejected — see "Why -1 and not -2?" above.
4. **Outsource validation to PAC 2024.** Rejected — PAC is GPL-3.0 + bundled
   as a Java desktop app, not a library we can embed. We port the 51
   Auto-decidable checkpoints to Rust.

## Slice plan (informational; full plan in `docs/plans/2026-05-22-v3.1.0-loom-pdf-ua.md`)

- **Slice 0 (this slice):** ADR + Matterhorn JSON registry. No Rust compile.
- Slice 1: `LayoutTree` extraction from content streams.
- Slice 2: `classify` — heuristic node typing (H1-H6, P, L, Figure, Table).
- Slice 3: `reading_order` — column detection + serpentine traversal.
- Slice 4: `alt_text` — Beacon llava integration + per-image cache.
- Slice 5: `structure_tree` — emit `StructTreeRoot` + `ParentTree` + `RoleMap`.
- Slice 6: `metadata` — `/Lang`, `/MarkInfo`, XMP `pdfuaid:part=1`.
- Slice 7: `validate` — Matterhorn Auto subset + LoomReport.
- Slice 8: Tauri commands + LoomPanel.svelte (Inspect / Tag / Review tabs).
- Slice 9: Release pipeline + customer-facing release notes.

## References

- ISO 14289-1:2014/Amd.1:2018, *Document management — Electronic document
  file format enhancement for accessibility (PDF/UA-1)*.
- PDF Association, *Matterhorn Protocol 1.1*, 2021.
  https://www.pdfa.org/resource/the-matterhorn-protocol-1-1/
- W3C, *Web Content Accessibility Guidelines (WCAG) 2.1*, 2018-06.
- U.S. Access Board, *Section 508 Standards (2018 refresh)*.
- ETSI, *EN 301 549 v3.2.1 (2021-03)*, "Accessibility requirements for ICT
  products and services."
- *European Accessibility Act* (Directive (EU) 2019/882), in force 2025-06-28.
