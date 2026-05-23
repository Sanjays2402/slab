# sRGB v4 ICC Preference Profile

**File:** `sRGB_v4_ICC_preference.icc`
**Size:** 3144 bytes
**SHA-256:** `2b3aa1645779a9e634744faf9b01e9102b0c9b88fd6deced7934df86b949af7e`

## Source

Apple's bundled sRGB profile from
`/System/Library/ColorSync/Profiles/sRGB Profile.icc`, which is itself
derived from the ICC's published sRGB v4 preference profile published by
the International Color Consortium at https://www.color.org/srgbprofiles.xalter.

ICC profiles published by color.org are explicitly licensed for
redistribution as embedded resources in software.

## Purpose

Slab's Bedrock (v3.0.0) PDF/A archival converter injects this profile as
the document `OutputIntent` for every converted PDF/A document so colors
render consistently across viewers / printers / archival systems.

A valid `OutputIntent` is a **required** element of any PDF/A-2b file
(ISO 19005-2 §6.2.2). The whole document is therefore non-conforming
without it.

## Why ship it?

Many target systems (Linux containers, Windows Server, headless macOS
build machines) do not have an sRGB profile installed at a predictable
path. Vendoring 3.1 KB removes a class of "works on my machine" bugs
and keeps Bedrock offline-first.

## Updating

If a newer ICC profile is published, replace the file and update the
SHA-256 above. The Rust side reads bytes via
`include_bytes!("../../../assets/icc/sRGB_v4_ICC_preference.icc")`, so
nothing else needs to change.
