# Slab v3.0.2 — "Foundry Fonts" 🎨

**Bedrock now converts any PDF.**

Until today, Slab's PDF/A converter refused any document that referenced
Helvetica, Times-Roman, or Courier without an embedded font file — exactly
the situation most "small office" PDFs are in. The escape hatch was a
`skip_font_check` boolean that produced output your validator wouldn't accept.

v3.0.2 fixes this for real. We ship 12 DejaVu TTF subsets — Sans, Serif,
Mono in 4 weights/styles each — embedded directly into the binary, and the
converter now auto-substitutes them whenever your PDF leans on a Standard-14
base font. The output is a fully ISO 19005-2 conformant PDF/A-2b.

## What this means

- **Any PDF in → archival PDF out.** No more "re-export from the source app
  with embed-all-fonts enabled" dance.
- **Adobe parity, achieved.** Acrobat Pro's $239/yr font-embed pipeline is
  now matched by free, offline, cross-platform Slab.
- **Visible feedback.** The Bedrock panel shows a new "fonts auto-embedded"
  stat tile after every conversion — you can see exactly which substitutes
  were spliced in.
- **Bundle cost:** ~6 MB added to the installer. Worth it for unbreakable
  PDF/A.

## Under the hood

- New `font_embed` pass runs between audit and sanitize.
- New `font_table` module maps every Standard-14 PostScript name to its
  closest DejaVu match by family + weight + slant.
- `FontFile2` streams + synthesised `FontDescriptor` (Ascent, Descent,
  CapHeight, FontBBox, ItalicAngle, Flags) computed via the `ttf-parser`
  crate.
- `skip_font_check` → `allow_unembedded_fonts` (serde alias preserves
  config compat for pre-v3.0.2 callers).
- 17 new unit tests cover the substitution table, the embed pass, the
  orchestrator's default-on behaviour, and the legacy escape hatch.

## Try it

1. Open any PDF that previously failed PDF/A conversion in Slab.
2. `Cmd/Ctrl+Shift+B` for the Bedrock panel.
3. Convert. Watch the new "N fonts auto-embedded" stat in the report.
4. Drop the output into [veraPDF](https://verapdf.org) or Adobe Preflight.
   It passes.

## Credits

DejaVu Fonts 2.37 (https://dejavu-fonts.github.io) under the permissive
Bitstream Vera + Arev + DejaVu license. Two decades of open-source font
work in the public domain — thank you to that project.

## Compatibility

- Slab now requires ~6 MB more disk per installation (bundled DejaVu).
- Stored configs using `skip_font_check: true` still work — serde alias
  routes them to `allow_unembedded_fonts`.
- `ConvertReport.fonts_embedded` is a new field; clients that ignored
  unknown JSON fields keep working.
