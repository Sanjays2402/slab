# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: v3.6.0 Compactor PUBLISHED. Ready for next pipeline pick.

**TICK 2026-05-23 06:51 PT** — MODE B finalize for v3.6.0.

- CI build run `26334187463` completed `success` (~12m).
- Downloaded 6 artifacts → `gh release create v3.6.0 --title "v3.6.0 — Compactor"`.
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.6.0
- Docker run `26334187486` already green (image on GHCR at `ghcr.io/sanjays2402/slab-server:v3.6.0`).
- Release notes lead with the marketing angle: "$239/yr Acrobat Pro reduce-file-size, free + offline."

Branch `feature/v3.6.0-compactor` can be deleted on next pruning pass.

LAST_WOW_TICK_AT: 2026-05-23T06:51 PT (Compactor shipped to users — real
PDF compression, the Adobe Acrobat Pro paid feature, free + offline.)

RECENTLY_CLOSED_ISSUES:
- v3.5.0 Veil — published earlier this morning
- v3.6.0 Compactor — published this tick (CI 26334187463)

### Disk warning

`~` was at 969 Mi free before tick — cleaned `/tmp/slab-release-3.*` to recover
1 Gi. Now at **1.9 Gi free**, 99% full. Next tick should consider:
- Pruning merged local feature branches (`v3.4.0-discovery`, `v3.5.0-veil`,
  `v3.6.0-compactor`, etc.) — they still exist locally.
- `cargo clean` on src-tauri/target if disk gets tighter (~5 Gi reclaim).

### Next pipeline candidates (pick on next non-blackout tick)

The roadmap has gotten ahead of the docs/plans pipeline. Pending big plans:
1. **v3.1.0 Loom (Slice 2)** — PDF/UA accessibility, branch
   `feature/v3.1.0-loom-slice-2` already in flight locally.
2. **v3.2.0 Press** — PDF/X prepress.
3. **v3.3.0 Bindery** — imposition (booklet, n-up).
4. **v3.7.0+** — open. Strong candidates by buy-button test:
   - **Bates numbering bulk** (Discovery v3.4.0 already shipped solo doc;
     do batch folder mode) — legal market demands this.
   - **Form filler (Quill v2.5.0 plan)** — promote from docs/plans.
   - **OCR (Lens v2.6.0 plan)** — every paying customer asks.
   - **Digital signing (Signet v2.7.0 plan)** — enterprise must-have.
5. **v3.7.0 / Foundry marketplace MVP** — Plugin Store UI from the roadmap;
   high "wow" but requires solid backend.

Issues backlog: `gh issue list` returned `[]` — none open right now, so the
top-priority override (#23–#27) is no longer blocking. Fall through to the
pipeline freely.

---

## ARCHIVED: v3.6.0 Compactor — MERGED + TAGGED earlier this morning

(See git: merge `67086d8`, tag `v3.6.0`, branch `feature/v3.6.0-compactor`.)

## ARCHIVED: v3.5.0 Veil — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.5.0
True PDF redaction (content-stream excision, not black bars).

## ARCHIVED: v3.4.0 Discovery — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.4.0
Bates numbering for legal discovery.
