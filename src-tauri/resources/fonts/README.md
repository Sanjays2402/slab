# Bundled fonts

Slab bundles DejaVu Fonts 2.37 (https://dejavu-fonts.github.io) under the
Bitstream Vera + Arev + DejaVu permissive license (effectively public-domain
for redistribution; see `LICENSE.dejavu`).

These are used by the Bedrock PDF/A converter to substitute and embed the
Standard-14 PostScript fonts (Helvetica, Times-Roman, Courier and their
weight/style variants) that an input PDF references without embedding.
DejaVu's metrics are intentionally close to those families:

- `DejaVu Sans`       → Helvetica
- `DejaVu Serif`      → Times-Roman
- `DejaVu Sans Mono`  → Courier

Symbol and ZapfDingbats fall back to DejaVu Sans (best-effort).

12 TTF files, ~5.3 MB on disk, compiled into the binary at build time via
`include_bytes!` from `src-tauri/src/pdf/pdfa/font_table.rs`.
