//! PDF/A archival conversion (ISO 19005-2). v3.0.0 "Bedrock".
//!
//! Public entry point: [`convert_to_pdfa`] — added in slice 4 once all
//! passes (sanitize, fonts, output_intent, xmp, validate) are wired.
//!
//! Current state: slice 1 — sanitize pass + ICC profile bytes.

pub mod icc;
pub mod sanitize;

/// Conformance level requested by the caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConformanceLevel {
    /// PDF/A-2b — basic visual reproducibility. Default for general use.
    #[default]
    #[serde(rename = "2b")]
    A2b,
    /// PDF/A-3b — like 2b but allows arbitrary file attachments (e-invoicing).
    #[serde(rename = "3b")]
    A3b,
}

impl ConformanceLevel {
    /// XMP `pdfaid:part` integer ("2" or "3").
    pub fn part(self) -> u8 {
        match self {
            Self::A2b => 2,
            Self::A3b => 3,
        }
    }

    /// XMP `pdfaid:conformance` letter ("B" for now — A/U are future work).
    pub fn conformance(self) -> &'static str {
        "B"
    }

    /// Human-readable label for UI ("PDF/A-2b" / "PDF/A-3b").
    pub fn label(self) -> &'static str {
        match self {
            Self::A2b => "PDF/A-2b",
            Self::A3b => "PDF/A-3b",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_2b() {
        assert_eq!(ConformanceLevel::default(), ConformanceLevel::A2b);
    }

    #[test]
    fn part_numbers() {
        assert_eq!(ConformanceLevel::A2b.part(), 2);
        assert_eq!(ConformanceLevel::A3b.part(), 3);
    }

    #[test]
    fn conformance_letter_is_b() {
        assert_eq!(ConformanceLevel::A2b.conformance(), "B");
        assert_eq!(ConformanceLevel::A3b.conformance(), "B");
    }

    #[test]
    fn labels() {
        assert_eq!(ConformanceLevel::A2b.label(), "PDF/A-2b");
        assert_eq!(ConformanceLevel::A3b.label(), "PDF/A-3b");
    }

    #[test]
    fn round_trip_serde() {
        let json = serde_json::to_string(&ConformanceLevel::A2b).unwrap();
        assert_eq!(json, "\"2b\"");
        let back: ConformanceLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConformanceLevel::A2b);
    }
}
