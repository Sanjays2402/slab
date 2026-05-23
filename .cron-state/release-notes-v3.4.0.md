# Slab v3.4.0 — Discovery

**The Adobe Acrobat Pro DC discovery workflow, free and offline.**

Paralegals and litigation support teams spend their day Bates-numbering productions and stamping documents CONFIDENTIAL. Adobe charges $239/yr for it, batches ~5 files at a time, and ships your privileged documents to their cloud to do it. Slab now does the entire production in one click, emits a Relativity-compatible load file, and never touches the network.

## ✨ Bates numbering

- Prefix + starting number + zero-padded digit width (e.g. `ACME000001`)
- Nine corner positions, opacity slider, rotation
- **Live SVG preview** re-renders on every keystroke and slider tick — you see `ACME000001` land in the corner before you touch a document
- Apply to a single file, or batch across N files with the counter chained automatically

## 📑 Batch driver + load file

Drop a folder. Slab numbers every page of every file, threads the Bates counter from one document to the next, and writes a `loadfile.csv` (or JSON) that imports straight into Relativity, Concordance, or Everlaw. The whole production, one click.

## 🏷️ Legal stamps

Four canonical presets:
- CONFIDENTIAL
- ATTORNEYS' EYES ONLY
- PRIVILEGED & CONFIDENTIAL
- DRAFT

Plus custom text. Diagonal watermark mode for the across-the-page look. Opacity slider. Same live preview as Bates.

## ⌨️ Keyboard + palette

- `Cmd/Ctrl+Shift+B` — Bates
- `Cmd/Ctrl+Shift+S` — Legal Stamp
- Both reachable from the `Cmd/Ctrl+K` command palette
- Both listed in the shortcuts cheat sheet (`Cmd+/`)

## 🌍 Localized

Feature names translated across English, Spanish, French, Arabic, Hindi, Tamil, Telugu.

## 🛠️ Under the hood

- 17 new unit tests, all green
- Pure Rust `bates_label_for()` helper for deterministic output
- Three new Tauri IPC entrypoints: `slab_bates_apply`, `slab_bates_batch`, `slab_legal_stamp_apply`
- Zero new runtime dependencies

---

**Free. Offline. Yours.** Download below.
