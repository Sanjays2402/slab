//! Bundled ICC profile bytes for PDF/A OutputIntent injection.
//!
//! The sRGB v4 profile is vendored at compile time so every platform
//! (mac/linux/win) gets a byte-identical OutputIntent stream. Without an
//! OutputIntent, a PDF cannot be PDF/A — see ISO 19005-2 §6.2.2.

/// sRGB v4 ICC preference profile (3144 bytes).
///
/// Vendored from `assets/icc/sRGB_v4_ICC_preference.icc`. See that
/// directory's README for provenance + license + SHA-256.
pub static SRGB_V4_ICC: &[u8] = include_bytes!("../../../../assets/icc/sRGB_v4_ICC_preference.icc");

/// Stable human-readable name we put in the OutputIntent's
/// `/OutputConditionIdentifier`. Matches what every PDF/A validator
/// expects for an sRGB-tagged document.
pub const SRGB_OUTPUT_CONDITION_IDENTIFIER: &str = "sRGB IEC61966-2.1";

/// Stable info string for `/Info`. Some validators surface this in
/// their UI; PDF/A spec leaves the exact text to the producer.
pub const SRGB_INFO: &str = "sRGB IEC61966-2.1 (vendored, v4 preference)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icc_profile_is_nonempty_and_starts_with_acsp_header() {
        // ICC profiles have a 128-byte header; bytes 36..40 are 'acsp'.
        assert!(SRGB_V4_ICC.len() >= 128);
        assert_eq!(&SRGB_V4_ICC[36..40], b"acsp");
    }

    #[test]
    fn icc_profile_is_sized_reasonably_for_a_v4_srgb_profile() {
        // Real sRGB v4 profiles are 3-4 KB. Catch accidental truncation
        // (e.g. someone substitutes an empty placeholder).
        assert!(SRGB_V4_ICC.len() > 3_000);
        assert!(SRGB_V4_ICC.len() < 8_000);
    }

    #[test]
    fn output_condition_identifier_is_canonical() {
        assert_eq!(SRGB_OUTPUT_CONDITION_IDENTIFIER, "sRGB IEC61966-2.1");
    }
}
