//! PDF/X-4 print-production module (ISO 15930-7).
//!
//! Slab's "Press" pipeline takes any PDF and produces a fully ISO 15930-7
//! compliant **PDF/X-4** document — a print shop will accept the result
//! without manual fixup. Architecture: 6 ordered passes over a
//! [`lopdf::Document`], five of which delegate to the shared
//! [`crate::pdf::pdfa`] substrate (which already implements sanitize,
//! font_embed, XMP writing, and output-intent injection for PDF/A-2b).
//! Only the PDF/X-specific behaviours live here:
//!
//! 1. `sanitize`             — reuse `pdfa::sanitize` (strip JS, AA, Encrypt).
//! 2. `font_embed`           — reuse `pdfa::font_embed`.
//! 3. `color::normalize_color` — DeviceRGB → ICC-tagged rewrite, 16→8-bit
//!    image downsample. (Slice 2.)
//! 4. `geometry::ensure_print_boxes` — synthesize `TrimBox`, optional 3mm
//!    `BleedBox`. (Slice 3.)
//! 5. `metadata`             — extend `pdfa::xmp::XmpBuilder` with the
//!    `pdfxid` namespace and write `pdfxid:GTS_PDFXVersion=PDF/X-4`.
//!    (Slice 4.)
//! 6. `output_intent`        — write `/Catalog /OutputIntents` with
//!    `/S /GTS_PDFX` instead of `/GTS_PDFA1`. (Slice 4.)
//!
//! Validation lives in [`validate`] — 32 ISO 15930-7 auto-decidable rules
//! grouped by category.
//!
//! See `docs/adr/2026-05-23-pdfx-conformance-level.md` for the decision
//! record and `docs/plans/2026-05-23-v3.8.0-press-pdf-x.md` for the full
//! implementation plan.

pub mod color;
pub mod geometry;

/// The two output-intent ICC profiles Slab v3.8.0 ships with.
///
/// Both are vendored at compile time via `include_bytes!` from
/// `assets/icc/press/` so every platform produces byte-identical
/// `/DestOutputProfile` streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputIntent {
    /// FOGRA51 (PSO Coated v3) — European coated offset.
    Fogra51Coated,
    /// GRACoL2013 CRPC6 — North American sheet-fed coated.
    Gracol2013Crpc6,
}

impl OutputIntent {
    /// Stable name suitable for `/OutputConditionIdentifier`.
    pub fn condition_identifier(&self) -> &'static str {
        match self {
            Self::Fogra51Coated => "FOGRA51",
            Self::Gracol2013Crpc6 => "GRACoL2013_CRPC6",
        }
    }

    /// Always the canonical ICC registry URL for PDF/X output intents.
    pub fn registry_name(&self) -> &'static str {
        "http://www.color.org"
    }

    /// Human-readable description placed in `/Info` of the OutputIntent.
    pub fn info_string(&self) -> &'static str {
        match self {
            Self::Fogra51Coated => "PSO Coated v3 (FOGRA51) — ECI/IDEAlliance, vendored",
            Self::Gracol2013Crpc6 => "GRACoL2013 CRPC6 — IDEAlliance, vendored",
        }
    }

    /// Vendored ICC profile bytes for `/DestOutputProfile`.
    pub fn profile_bytes(&self) -> &'static [u8] {
        match self {
            Self::Fogra51Coated => FOGRA51_ICC,
            Self::Gracol2013Crpc6 => GRACOL2013_ICC,
        }
    }

    /// Parse a wire-format identifier (used by Tauri commands /
    /// `slab_press_convert`'s `intent` string).
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "fogra51" | "FOGRA51" => Some(Self::Fogra51Coated),
            "gracol2013" | "GRACoL2013" | "GRACoL2013_CRPC6" => Some(Self::Gracol2013Crpc6),
            _ => None,
        }
    }

    /// Wire identifier — the inverse of [`Self::from_wire`].
    pub fn to_wire(&self) -> &'static str {
        match self {
            Self::Fogra51Coated => "fogra51",
            Self::Gracol2013Crpc6 => "gracol2013",
        }
    }
}

/// FOGRA51 (PSO Coated v3) — European coated offset.
pub static FOGRA51_ICC: &[u8] = include_bytes!("../../../../assets/icc/press/PSOcoated_v3.icc");

/// GRACoL2013 CRPC6 — North American sheet-fed coated.
pub static GRACOL2013_ICC: &[u8] =
    include_bytes!("../../../../assets/icc/press/GRACoL2013_CRPC6.icc");

#[cfg(test)]
mod tests {
    use super::*;

    /// Every ICC profile has a 128-byte header; bytes 36..40 are `acsp`.
    fn assert_valid_icc(profile: &[u8], min: usize) {
        assert!(
            profile.len() >= min,
            "profile too small: got {}, expected >= {}",
            profile.len(),
            min
        );
        assert_eq!(
            &profile[36..40],
            b"acsp",
            "ICC magic 'acsp' missing at offset 36"
        );
    }

    #[test]
    fn fogra51_profile_is_valid_icc() {
        assert_valid_icc(FOGRA51_ICC, 100_000);
    }

    #[test]
    fn gracol_profile_is_valid_icc() {
        assert_valid_icc(GRACOL2013_ICC, 100_000);
    }

    #[test]
    fn output_intent_round_trips_via_wire() {
        for &intent in &[OutputIntent::Fogra51Coated, OutputIntent::Gracol2013Crpc6] {
            let wire = intent.to_wire();
            assert_eq!(OutputIntent::from_wire(wire), Some(intent));
        }
    }

    #[test]
    fn output_intent_from_wire_rejects_garbage() {
        assert_eq!(OutputIntent::from_wire("nope"), None);
        assert_eq!(OutputIntent::from_wire(""), None);
        assert_eq!(OutputIntent::from_wire("pdfx"), None);
    }

    #[test]
    fn output_intent_aliases_accepted() {
        assert_eq!(
            OutputIntent::from_wire("FOGRA51"),
            Some(OutputIntent::Fogra51Coated)
        );
        assert_eq!(
            OutputIntent::from_wire("GRACoL2013_CRPC6"),
            Some(OutputIntent::Gracol2013Crpc6)
        );
    }

    #[test]
    fn condition_identifiers_are_canonical() {
        assert_eq!(
            OutputIntent::Fogra51Coated.condition_identifier(),
            "FOGRA51"
        );
        assert_eq!(
            OutputIntent::Gracol2013Crpc6.condition_identifier(),
            "GRACoL2013_CRPC6"
        );
    }

    #[test]
    fn registry_name_is_always_icc_org() {
        assert_eq!(
            OutputIntent::Fogra51Coated.registry_name(),
            "http://www.color.org"
        );
        assert_eq!(
            OutputIntent::Gracol2013Crpc6.registry_name(),
            "http://www.color.org"
        );
    }

    #[test]
    fn profile_bytes_match_static_constants() {
        assert!(std::ptr::eq(
            OutputIntent::Fogra51Coated.profile_bytes().as_ptr(),
            FOGRA51_ICC.as_ptr()
        ));
        assert!(std::ptr::eq(
            OutputIntent::Gracol2013Crpc6.profile_bytes().as_ptr(),
            GRACOL2013_ICC.as_ptr()
        ));
    }
}
