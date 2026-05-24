# Slab v3.12.0 — Atelier

**Workflow automation that runs while you sleep.**

Adobe Action Wizard costs $239/yr and ships your PDFs to Adobe's cloud. Slab Atelier does the same thing — _better_ — free, offline, parallel, on every OS.

## What's new

**Build a recipe once. Run it on a thousand PDFs.**

Drag steps into the order you want — OCR, auto-redact PII, Bates-stamp, flatten for legal/compliance, sign, watermark — name it "Nightly Discovery" or "Monthly Filings" or whatever, save it, then point it at a folder and walk away. Every PDF in the folder runs the entire recipe in parallel.

### Highlights

- **Recipe builder** — drag-reorder steps from a palette into an ordered pipeline. Save / load / delete named recipes. Ships with a "Nightly Discovery" preset out of the box.
- **Live progress matrix** — watch every file × every step light up in real-time. Pending grey → running blue → done green → failed red. The screenshot writes itself.
- **Parallel execution** — rayon-backed batch driver. 200 PDFs through OCR + Bates + flatten + sign on a quiet weekend night. Failures recorded, batch never aborts.
- **Native file pickers** for input folder + output folder. Non-PDFs skipped automatically. Deterministic ordering.
- **Keyboard-first** — `Mod+Shift+R` opens Atelier from anywhere. `Cmd/Ctrl+Enter` runs the active recipe. Command palette wired.
- **Same UX on Mac, Windows, Linux** — Adobe doesn't ship Linux. We do.

### Why it matters

If you're a paralegal, a compliance officer, an IT admin, an academic processing scanned archives — this is the feature that previously required a Pro subscription and a cloud account. Slab gives it to you free, with your files never leaving your machine.

### Under the hood

- New `pdf::atelier` module: typed `Recipe` / `Step` model with serde JSON round-trip + forward-compat version field.
- `run_recipe` driver bridges to the existing `pdf::*` primitives (watermark, flatten, compactor, ocr, auto_redact, bates) using their real opts shapes.
- `run_recipe_batch` parallel folder driver via rayon with per-file × per-step `BatchProgress` events.
- 4 new Tauri commands: `atelier_run_batch` (Channel-streamed), `atelier_save_recipe`, `atelier_load_recipes`, `atelier_delete_recipe`. Filename sanitization prevents path escape.
- 20 new unit tests; full suite **1418 / 1418 passing**.

### Get it

Pick your platform from the release artifacts below. Open Slab. Press `Mod+Shift+R`. Drag a step. Pick a folder. Watch the matrix light up.

---

_Slab is free, offline, and open source. Adobe charges $239/yr to ship your files to their cloud. We don't ship anywhere._
