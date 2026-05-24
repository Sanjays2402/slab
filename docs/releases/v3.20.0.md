# Slab v3.20.0 — Hopper

**Drop a PDF in a folder. Walk away. Come back to a perfectly named, processed file.**

Hopper is Slab's new watched-folder automation engine. Point it at any folder on your machine — `~/Downloads`, a scanner output directory, a shared "inbox" — pair the folder with an Atelier recipe, and every PDF that lands triggers a full pipeline:

1. Your **Atelier recipe** runs on it (flatten, OCR, redact PII, watermark, page-number, sign — anything Atelier can do).
2. **Beacon** reads the first page and suggests a 4-6 word filename like `2026-04-21_NDA_Acme.pdf`.
3. The result is **filed into your output directory**.
4. A **live run log** streams in the Hopper panel as it happens.

All of it on your machine. No cloud, no API key, no quota, no upload.

## Why this matters

The competitors:

- **Hazel** ($42) is general-purpose and doesn't understand PDFs — it can move files by name, but can't OCR, flatten, redact, or rename based on contents.
- **Adobe AutoActions** require an enterprise Acrobat seat (~$300/yr per user), only run on Windows, and ship your file to Adobe's cloud for AI features.
- **Power Automate / Zapier** are SaaS, charge per run, and require uploading every file.

Slab Hopper is local, free, cross-platform, and does the full chain in one drop.

## Use cases shipped today

- **Scanner inbox** — every page your scanner spits out gets OCR'd, has PII auto-redacted, and lands in `~/Documents/Scanned` with a real name.
- **Legal triage** — drag a folder of contracts into `~/Cases/Acme/`, Slab flattens for compliance, names each by counterparty + date, files into matter folders.
- **Invoice processing** — `~/Downloads` watcher catches PDFs from email, extracts the vendor + total via Beacon, names accordingly, archives to accounting folders.
- **Research workflow** — drop arXiv downloads, Slab OCRs scanned figures, asks Beacon for title + authors, files into a "Library/2026/" tree.

## Open Hopper

- **⇧⌘H** (macOS) or **Ctrl+Shift+H** (Windows/Linux)
- Or **⌘K → "Hopper"** from anywhere
- Or click the **🪣** icon in the sidebar

## Under the hood

- `notify` 6.x cross-platform filesystem watcher (FSEvents on macOS, ReadDirectoryChangesW on Windows, inotify on Linux).
- 1-second debounce before processing — handles slow scanner writes and large multi-page jobs without firing mid-write.
- Per-watch parallel pipeline dispatch via `tokio::spawn`.
- Sqlite-backed registry of watches + full run history.
- All 33 hopper unit tests green in CI on macOS arm64, x86_64, Linux, and Windows.

## Everything else in v3.20.0

- Onboarding tour updated with a 6th step introducing Hopper to new users.
- Landing page (slab.app) gets a Hopper feature card and a new toolbox tile.
- README leads with Hopper as the v3.20 headline.
- All Marquee (v3.19.0) features remain — try.slab.app browser playground, markdown editor, page ops UI.

## Install

- **macOS** — download the `.dmg` for your arch (Apple Silicon or Intel) below.
- **Windows** — `.msi` (preferred) or `.exe` setup.
- **Linux** — `.deb` for Debian/Ubuntu or `.AppImage` for everything else.
- **Docker / server** — `docker run -p 8080:8080 ghcr.io/sanjays2402/slab:3.20.0`

## Verify

Every build is reproducible from source. Tag `v3.20.0` is signed; CI artifacts are built from the same SHA. See `docs/server.md` for the Docker image SBOM.

## Thanks

Hopper started as a back-of-napkin "Hazel but it speaks PDF" sketch on 2026-05-24 and shipped end-to-end the same day across 4 cron ticks. If it changes your workflow, [tell us](https://github.com/Sanjays2402/slab/discussions).

— The Slab team
