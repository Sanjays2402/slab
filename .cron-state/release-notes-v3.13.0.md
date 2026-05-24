v3.13.0 "Streamline" — Fast Web View, free and offline.

Adobe Acrobat Pro charges $239/yr for "Optimize for Fast Web View".
PDF Expert doesn't ship it. Foxit Phantom locks it behind a $129/yr
seat. As of today, Slab does it in one click — entirely on your
machine, no cloud, no subscription.

Drop a PDF onto the Streamline panel and watch the headline: your
reader starts showing page 1 after a few KB of header instead of
megabytes of trailing objects. Combine it with the Atelier workflow
engine to linearize a whole folder of court filings in parallel.

What's new
- Inspector: detect linearization status, primary-hint coverage,
  first-page byte budget, and the producer string of any PDF.
- Linearizer (writer): rewrite arbitrary PDFs into a PDF 1.4 §F
  conformant layout — first page reachable set up front, linearization
  parameter dictionary right after the header, hint stream pointing
  at every page offset.
- Batch audit: drop a folder, get a sortable / filterable Fast Web
  View report with CSV export — the paralegal-auditing-500-discovery-PDFs
  workflow Adobe charges Acrobat Pro seats for.
- Atelier step: new `linearize` recipe step. Bundle it with OCR,
  redact, flatten, bates into a one-click pipeline.
- Streamline panel UI: live inspector, one-click "Optimize for Fast
  Web View", before/after stats.

Known limitations
- Encrypted PDFs are detected but not yet rewritten — open them
  first, save unencrypted, then linearize. (Tracked for v3.13.1.)
- Cross-reference streams (PDF 1.5+) are linearized as classic
  cross-reference tables. Behaviour is identical from a reader's
  perspective; round-trip output is a touch larger.

Cross-platform
- macOS arm64 + x86_64 (.dmg)
- Linux x86_64 (.deb + .AppImage)
- Windows x86_64 (.msi + NSIS .exe)

All four built locally + tested on every push. No cloud round-trip,
no telemetry, no subscription. Same binary every machine.
