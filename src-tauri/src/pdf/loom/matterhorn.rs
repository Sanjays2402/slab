// @generated — DO NOT EDIT BY HAND.
// Source: docs/specs/matterhorn-1.1.json
// Schema: docs/specs/matterhorn-1.1.schema.json
// Generator: scripts/loom/codegen-matterhorn.mjs
//
// To regenerate, run `pnpm loom:codegen` from the repo root.
// CI verifies this file is in sync via `pnpm loom:codegen --check`.
//
// Matterhorn Protocol 1.1, published by the PDF Association (2021),
// covers ISO 14289-1:2014/Amd.1:2018 (PDF/UA-1). See
// docs/adr/2026-05-23-pdf-ua-conformance.md for the conformance target.

#![allow(dead_code)] // helpers consumed in Slice 2+ of v3.1.0 Loom

use serde::Serialize;

/// Whether a Matterhorn failure condition is decidable by the validate
/// pass alone (`Auto`), requires human review (`Human`), or depends on
/// Slab features not yet shipped (`OutOfScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Verdict {
    Auto,
    Human,
    OutOfScope,
}

impl Verdict {
    pub const fn as_str(self) -> &'static str {
        match self {
            Verdict::Auto => "auto",
            Verdict::Human => "human",
            Verdict::OutOfScope => "outOfScope",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FailureCondition {
    /// Hyphenated id, e.g. `"01-007"`.
    pub id: &'static str,
    /// One-line description, verbatim from the Matterhorn Protocol.
    pub title: &'static str,
    pub verdict: Verdict,
    pub section_id: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub iso_clause: &'static str,
    pub conditions: &'static [FailureCondition],
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Totals {
    pub sections: usize,
    pub failure_conditions_in_this_registry: usize,
    pub failure_conditions_in_full_protocol: usize,
    pub auto: usize,
    pub human: usize,
    pub out_of_scope: usize,
    pub not_yet_transcribed: usize,
}

pub const TOTALS: Totals = Totals {
    sections: 31,
    failure_conditions_in_this_registry: 91,
    failure_conditions_in_full_protocol: 136,
    auto: 48,
    human: 33,
    out_of_scope: 10,
    not_yet_transcribed: 45,
};

pub const PROTOCOL_VERSION: &str = "1.1";
pub const APPLIES_TO: &str = "ISO 14289-1:2014/Amd.1:2018 (PDF/UA-1)";

const SECTION_01_CONDITIONS: &[FailureCondition] = &[
    FailureCondition { id: "01-001", title: "Some real content is not tagged.", verdict: Verdict::Auto, section_id: "01" },
    FailureCondition { id: "01-002", title: "Some tag does not include all of the real content it describes.", verdict: Verdict::Auto, section_id: "01" },
    FailureCondition { id: "01-003", title: "Content is marked as Artifact but is in fact real content.", verdict: Verdict::Human, section_id: "01" },
    FailureCondition { id: "01-004", title: "Tagged content is incorrectly marked as Artifact (suppresses real content from AT).", verdict: Verdict::Human, section_id: "01" },
    FailureCondition { id: "01-005", title: "Content other than Artifacts is not inside a marked-content sequence (no MCID).", verdict: Verdict::Auto, section_id: "01" },
    FailureCondition { id: "01-006", title: "The structure tree is missing or empty.", verdict: Verdict::Auto, section_id: "01" },
    FailureCondition { id: "01-007", title: "Content is tagged in a way that does not represent the document's logical reading order.", verdict: Verdict::Human, section_id: "01" },
];

const SECTION_02_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "02-001",
        title: "One or more non-standard tag's mapping is not mapped to a standard PDF 1.7 type.",
        verdict: Verdict::Auto,
        section_id: "02",
    },
    FailureCondition {
        id: "02-002",
        title: "A non-standard tag's mapping is semantically inappropriate.",
        verdict: Verdict::Human,
        section_id: "02",
    },
    FailureCondition {
        id: "02-003",
        title: "A standard PDF 1.7 type is remapped to a non-standard type.",
        verdict: Verdict::Auto,
        section_id: "02",
    },
    FailureCondition {
        id: "02-004",
        title: "A circular role mapping exists.",
        verdict: Verdict::Auto,
        section_id: "02",
    },
];

const SECTION_03_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "03-001",
        title: "Tag's parent-child relationship is not permitted by ISO 14289-1.",
        verdict: Verdict::Auto,
        section_id: "03",
    },
    FailureCondition {
        id: "03-002",
        title: "A structure element's K array references an object that is not allowed.",
        verdict: Verdict::Auto,
        section_id: "03",
    },
    FailureCondition {
        id: "03-003",
        title: "Document does not contain a Document tag at the root of the structure tree.",
        verdict: Verdict::Auto,
        section_id: "03",
    },
];

const SECTION_04_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "04-001",
        title: "Text is mapped to Unicode PUA without an ActualText entry.",
        verdict: Verdict::Auto,
        section_id: "04",
    },
    FailureCondition {
        id: "04-002",
        title: "A glyph's ToUnicode mapping does not produce a character that is reasonable.",
        verdict: Verdict::Human,
        section_id: "04",
    },
    FailureCondition {
        id: "04-003",
        title: "Font without ToUnicode CMap is used for text content.",
        verdict: Verdict::Auto,
        section_id: "04",
    },
    FailureCondition {
        id: "04-004",
        title: "Stretchable characters lack proper ActualText.",
        verdict: Verdict::Human,
        section_id: "04",
    },
];

const SECTION_05_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "05-001",
        title: "Natural language for document is not specified (Catalog /Lang missing).",
        verdict: Verdict::Auto,
        section_id: "05",
    },
    FailureCondition {
        id: "05-002",
        title: "Natural language for content differs from default but is not specified.",
        verdict: Verdict::Human,
        section_id: "05",
    },
    FailureCondition {
        id: "05-003",
        title: "Lang entry uses a non-conformant BCP 47 tag.",
        verdict: Verdict::Auto,
        section_id: "05",
    },
];

const SECTION_06_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "06-001",
        title: "/ViewerPreferences DisplayDocTitle is false or missing.",
        verdict: Verdict::Auto,
        section_id: "06",
    },
    FailureCondition {
        id: "06-002",
        title: "/Info /Title is empty or absent.",
        verdict: Verdict::Auto,
        section_id: "06",
    },
    FailureCondition {
        id: "06-003",
        title: "Document title in metadata is the filename or otherwise not meaningful.",
        verdict: Verdict::Human,
        section_id: "06",
    },
];

const SECTION_07_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "07-001",
        title: "File is not a valid PDF.",
        verdict: Verdict::Auto,
        section_id: "07",
    },
    FailureCondition {
        id: "07-002",
        title: "Cross-reference table is broken.",
        verdict: Verdict::Auto,
        section_id: "07",
    },
    FailureCondition {
        id: "07-003",
        title: "Linearized PDF with broken hint stream.",
        verdict: Verdict::Auto,
        section_id: "07",
    },
];

const SECTION_08_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "08-001",
        title: "Embedded file lacks an MD5 hash or modification date.",
        verdict: Verdict::Auto,
        section_id: "08",
    },
    FailureCondition {
        id: "08-002",
        title: "Embedded file is not described by a /Desc entry.",
        verdict: Verdict::Auto,
        section_id: "08",
    },
];

const SECTION_09_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "09-001",
        title: "Abbreviation lacks /E expansion in tag.",
        verdict: Verdict::Human,
        section_id: "09",
    },
    FailureCondition {
        id: "09-002",
        title: "Stylised text (e.g. drop cap) lacks /ActualText.",
        verdict: Verdict::Human,
        section_id: "09",
    },
];

const SECTION_10_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "10-001",
        title: "Figure without /Alt attribute.",
        verdict: Verdict::Auto,
        section_id: "10",
    },
    FailureCondition {
        id: "10-002",
        title: "/Alt value is empty for a non-decorative figure.",
        verdict: Verdict::Human,
        section_id: "10",
    },
    FailureCondition {
        id: "10-003",
        title: "/Alt is identical to surrounding text (redundant).",
        verdict: Verdict::Human,
        section_id: "10",
    },
    FailureCondition {
        id: "10-004",
        title: "Decorative image is not marked as Artifact.",
        verdict: Verdict::Human,
        section_id: "10",
    },
    FailureCondition {
        id: "10-005",
        title: "/Alt describes the image's visual properties rather than its meaning.",
        verdict: Verdict::Human,
        section_id: "10",
    },
];

const SECTION_11_CONDITIONS: &[FailureCondition] = &[FailureCondition {
    id: "11-001",
    title: "Word requires phonetic pronunciation but /Pron is absent.",
    verdict: Verdict::Human,
    section_id: "11",
}];

const SECTION_12_CONDITIONS: &[FailureCondition] = &[
    FailureCondition { id: "12-001", title: "Heading levels are not nested in semantic order (skipped level).", verdict: Verdict::Auto, section_id: "12" },
    FailureCondition { id: "12-002", title: "Document uses H without numbered heading variants in strongly-structured doc.", verdict: Verdict::Human, section_id: "12" },
    FailureCondition { id: "12-003", title: "Document uses numbered heading variants but a strongly-structured doc requires sequential.", verdict: Verdict::Auto, section_id: "12" },
    FailureCondition { id: "12-004", title: "Visible heading is tagged as a paragraph.", verdict: Verdict::Human, section_id: "12" },
    FailureCondition { id: "12-005", title: "Paragraph is tagged as a heading.", verdict: Verdict::Human, section_id: "12" },
];

const SECTION_13_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "13-001",
        title: "Visible list is not tagged as L / LI.",
        verdict: Verdict::Human,
        section_id: "13",
    },
    FailureCondition {
        id: "13-002",
        title: "L tag has /ListNumbering value that does not match visible markers.",
        verdict: Verdict::Human,
        section_id: "13",
    },
    FailureCondition {
        id: "13-003",
        title: "Nested lists are not nested in the structure tree.",
        verdict: Verdict::Auto,
        section_id: "13",
    },
];

const SECTION_14_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "14-001",
        title: "Table is not tagged as Table / TR / TD / TH.",
        verdict: Verdict::Human,
        section_id: "14",
    },
    FailureCondition {
        id: "14-002",
        title: "Header cell is tagged as TD instead of TH.",
        verdict: Verdict::Human,
        section_id: "14",
    },
    FailureCondition {
        id: "14-003",
        title: "Header cell lacks /Scope (Row, Column, Both).",
        verdict: Verdict::Auto,
        section_id: "14",
    },
    FailureCondition {
        id: "14-004",
        title: "Complex header/data association lacks /Headers IDs.",
        verdict: Verdict::Human,
        section_id: "14",
    },
    FailureCondition {
        id: "14-005",
        title: "Visible TH does not map to the data cells it heads.",
        verdict: Verdict::Human,
        section_id: "14",
    },
    FailureCondition {
        id: "14-006",
        title: "Table has merged cells but no /ColSpan or /RowSpan attribute.",
        verdict: Verdict::Auto,
        section_id: "14",
    },
];

const SECTION_15_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "15-001",
        title: "Reading order in tag tree does not follow visible reading order.",
        verdict: Verdict::Human,
        section_id: "15",
    },
    FailureCondition {
        id: "15-002",
        title: "Columns are tagged in display order rather than reading order.",
        verdict: Verdict::Human,
        section_id: "15",
    },
];

const SECTION_16_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "16-001",
        title: "Running header is not marked as /Artifact /Type/Pagination.",
        verdict: Verdict::Auto,
        section_id: "16",
    },
    FailureCondition {
        id: "16-002",
        title: "Page number is included in tagged content (not artifact).",
        verdict: Verdict::Auto,
        section_id: "16",
    },
    FailureCondition {
        id: "16-003",
        title: "Watermark is not marked as /Artifact /Type/Layout.",
        verdict: Verdict::Auto,
        section_id: "16",
    },
    FailureCondition {
        id: "16-004",
        title: "Background image with no informational value is not Artifact.",
        verdict: Verdict::Human,
        section_id: "16",
    },
];

const SECTION_17_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "17-001",
        title: "Information is conveyed by color alone.",
        verdict: Verdict::Human,
        section_id: "17",
    },
    FailureCondition {
        id: "17-002",
        title: "Text contrast against background fails WCAG 2.1 AA (4.5:1).",
        verdict: Verdict::Auto,
        section_id: "17",
    },
    FailureCondition {
        id: "17-003",
        title: "Large text contrast fails WCAG 2.1 AA (3:1).",
        verdict: Verdict::Auto,
        section_id: "17",
    },
];

const SECTION_18_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "18-001",
        title: "Annotation lacks /Contents entry.",
        verdict: Verdict::Auto,
        section_id: "18",
    },
    FailureCondition {
        id: "18-002",
        title: "Link annotation is not tagged as /Link with /Link-OBJR.",
        verdict: Verdict::Auto,
        section_id: "18",
    },
    FailureCondition {
        id: "18-003",
        title: "/Annot is not contained in a proper structure element.",
        verdict: Verdict::Auto,
        section_id: "18",
    },
    FailureCondition {
        id: "18-004",
        title: "Link text is generic (\"click here\", \"more\").",
        verdict: Verdict::Human,
        section_id: "18",
    },
];

const SECTION_19_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "19-001",
        title: "Form field lacks /TU tooltip.",
        verdict: Verdict::OutOfScope,
        section_id: "19",
    },
    FailureCondition {
        id: "19-002",
        title: "Form field /T name is the only label.",
        verdict: Verdict::OutOfScope,
        section_id: "19",
    },
    FailureCondition {
        id: "19-003",
        title: "Form is not tagged as /Form with proper OBJR.",
        verdict: Verdict::OutOfScope,
        section_id: "19",
    },
    FailureCondition {
        id: "19-004",
        title: "Required field lacks indication other than color.",
        verdict: Verdict::OutOfScope,
        section_id: "19",
    },
];

const SECTION_20_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "20-001",
        title: "Optional content group lacks /Name.",
        verdict: Verdict::Auto,
        section_id: "20",
    },
    FailureCondition {
        id: "20-002",
        title: "Optional content default visibility hides accessible content.",
        verdict: Verdict::Human,
        section_id: "20",
    },
];

const SECTION_21_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "21-001",
        title: "Font used in real content is not embedded.",
        verdict: Verdict::Auto,
        section_id: "21",
    },
    FailureCondition {
        id: "21-002",
        title: "Font subset is missing glyphs referenced by content.",
        verdict: Verdict::Auto,
        section_id: "21",
    },
    FailureCondition {
        id: "21-003",
        title: "CIDFont lacks /CIDToGIDMap.",
        verdict: Verdict::Auto,
        section_id: "21",
    },
];

const SECTION_22_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "22-001",
        title: "XMP packet missing /pdfuaid:part.",
        verdict: Verdict::Auto,
        section_id: "22",
    },
    FailureCondition {
        id: "22-002",
        title: "/pdfuaid:part value is not 1.",
        verdict: Verdict::Auto,
        section_id: "22",
    },
    FailureCondition {
        id: "22-003",
        title: "XMP metadata schema for PDF/UA is malformed.",
        verdict: Verdict::Auto,
        section_id: "22",
    },
];

const SECTION_23_CONDITIONS: &[FailureCondition] = &[FailureCondition {
    id: "23-001",
    title: "Document permissions prevent text extraction for AT.",
    verdict: Verdict::Auto,
    section_id: "23",
}];

const SECTION_24_CONDITIONS: &[FailureCondition] = &[FailureCondition {
    id: "24-001",
    title: "Article thread is broken or out of reading order.",
    verdict: Verdict::Human,
    section_id: "24",
}];

const SECTION_25_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "25-001",
        title: "/MarkInfo /Marked is false or absent.",
        verdict: Verdict::Auto,
        section_id: "25",
    },
    FailureCondition {
        id: "25-002",
        title: "/MarkInfo /Suspects is true.",
        verdict: Verdict::Auto,
        section_id: "25",
    },
];

const SECTION_26_CONDITIONS: &[FailureCondition] = &[FailureCondition {
    id: "26-001",
    title: "Tag tree contains /Suspect marker that has not been resolved.",
    verdict: Verdict::Auto,
    section_id: "26",
}];

const SECTION_27_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "27-001",
        title: "Video annotation lacks captioning track.",
        verdict: Verdict::OutOfScope,
        section_id: "27",
    },
    FailureCondition {
        id: "27-002",
        title: "Audio annotation lacks transcript.",
        verdict: Verdict::OutOfScope,
        section_id: "27",
    },
    FailureCondition {
        id: "27-003",
        title: "RichMedia annotation lacks /Contents description.",
        verdict: Verdict::OutOfScope,
        section_id: "27",
    },
];

const SECTION_28_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "28-001",
        title: "Signature appearance contains real content not in tag tree.",
        verdict: Verdict::OutOfScope,
        section_id: "28",
    },
    FailureCondition {
        id: "28-002",
        title: "Visible signature lacks alt-text.",
        verdict: Verdict::OutOfScope,
        section_id: "28",
    },
];

const SECTION_29_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "29-001",
        title: "Text is rendered as an image without /ActualText.",
        verdict: Verdict::Auto,
        section_id: "29",
    },
    FailureCondition {
        id: "29-002",
        title: "Text rotation makes content non-extractable.",
        verdict: Verdict::Human,
        section_id: "29",
    },
];

const SECTION_30_CONDITIONS: &[FailureCondition] = &[FailureCondition {
    id: "30-001",
    title: "JavaScript action affects accessibility without notification.",
    verdict: Verdict::OutOfScope,
    section_id: "30",
}];

const SECTION_31_CONDITIONS: &[FailureCondition] = &[
    FailureCondition {
        id: "31-001",
        title: "Document fails PAC 2024 ruleset for reasons not enumerated above.",
        verdict: Verdict::Human,
        section_id: "31",
    },
    FailureCondition {
        id: "31-002",
        title: "Conformance claim (XMP) does not match actual conformance.",
        verdict: Verdict::Auto,
        section_id: "31",
    },
];

pub const SECTIONS: &[Section] = &[
    Section {
        id: "01",
        title: "Real content (real vs artifact)",
        iso_clause: "7.1",
        conditions: SECTION_01_CONDITIONS,
    },
    Section {
        id: "02",
        title: "Role mapping",
        iso_clause: "7.10",
        conditions: SECTION_02_CONDITIONS,
    },
    Section {
        id: "03",
        title: "Parent-child relationships",
        iso_clause: "7.1",
        conditions: SECTION_03_CONDITIONS,
    },
    Section {
        id: "04",
        title: "Character mappings (ActualText / ToUnicode)",
        iso_clause: "7.2",
        conditions: SECTION_04_CONDITIONS,
    },
    Section {
        id: "05",
        title: "Natural language (Lang)",
        iso_clause: "7.2",
        conditions: SECTION_05_CONDITIONS,
    },
    Section {
        id: "06",
        title: "Document title (DisplayDocTitle)",
        iso_clause: "7.1",
        conditions: SECTION_06_CONDITIONS,
    },
    Section {
        id: "07",
        title: "PDF objects, file structure",
        iso_clause: "7.1",
        conditions: SECTION_07_CONDITIONS,
    },
    Section {
        id: "08",
        title: "Embedded files",
        iso_clause: "7.11",
        conditions: SECTION_08_CONDITIONS,
    },
    Section {
        id: "09",
        title: "Replacement text / Abbreviations (E entry)",
        iso_clause: "7.5",
        conditions: SECTION_09_CONDITIONS,
    },
    Section {
        id: "10",
        title: "Alternative descriptions (Alt) on figures",
        iso_clause: "7.3",
        conditions: SECTION_10_CONDITIONS,
    },
    Section {
        id: "11",
        title: "Pronunciation",
        iso_clause: "7.6",
        conditions: SECTION_11_CONDITIONS,
    },
    Section {
        id: "12",
        title: "Headings",
        iso_clause: "7.4",
        conditions: SECTION_12_CONDITIONS,
    },
    Section {
        id: "13",
        title: "Lists",
        iso_clause: "7.6",
        conditions: SECTION_13_CONDITIONS,
    },
    Section {
        id: "14",
        title: "Tables",
        iso_clause: "7.5",
        conditions: SECTION_14_CONDITIONS,
    },
    Section {
        id: "15",
        title: "Layout (multi-column reading order)",
        iso_clause: "7.5",
        conditions: SECTION_15_CONDITIONS,
    },
    Section {
        id: "16",
        title: "Header / footer / page artifacts",
        iso_clause: "7.8",
        conditions: SECTION_16_CONDITIONS,
    },
    Section {
        id: "17",
        title: "Color and contrast",
        iso_clause: "\u{2014}",
        conditions: SECTION_17_CONDITIONS,
    },
    Section {
        id: "18",
        title: "Annotations (links, notes)",
        iso_clause: "7.18",
        conditions: SECTION_18_CONDITIONS,
    },
    Section {
        id: "19",
        title: "Forms",
        iso_clause: "7.18",
        conditions: SECTION_19_CONDITIONS,
    },
    Section {
        id: "20",
        title: "Optional content",
        iso_clause: "7.20",
        conditions: SECTION_20_CONDITIONS,
    },
    Section {
        id: "21",
        title: "Embedded fonts",
        iso_clause: "7.21",
        conditions: SECTION_21_CONDITIONS,
    },
    Section {
        id: "22",
        title: "Metadata (XMP)",
        iso_clause: "7.1",
        conditions: SECTION_22_CONDITIONS,
    },
    Section {
        id: "23",
        title: "Encryption / permissions",
        iso_clause: "7.1",
        conditions: SECTION_23_CONDITIONS,
    },
    Section {
        id: "24",
        title: "Article threads",
        iso_clause: "7.16",
        conditions: SECTION_24_CONDITIONS,
    },
    Section {
        id: "25",
        title: "MarkInfo flag",
        iso_clause: "7.1",
        conditions: SECTION_25_CONDITIONS,
    },
    Section {
        id: "26",
        title: "Suspect content",
        iso_clause: "7.1",
        conditions: SECTION_26_CONDITIONS,
    },
    Section {
        id: "27",
        title: "Multimedia (RichMedia)",
        iso_clause: "7.17",
        conditions: SECTION_27_CONDITIONS,
    },
    Section {
        id: "28",
        title: "Digital signatures",
        iso_clause: "7.19",
        conditions: SECTION_28_CONDITIONS,
    },
    Section {
        id: "29",
        title: "Visual appearance",
        iso_clause: "7.5",
        conditions: SECTION_29_CONDITIONS,
    },
    Section {
        id: "30",
        title: "JavaScript / interactivity",
        iso_clause: "7.1",
        conditions: SECTION_30_CONDITIONS,
    },
    Section {
        id: "31",
        title: "Miscellaneous",
        iso_clause: "\u{2014}",
        conditions: SECTION_31_CONDITIONS,
    },
];

pub const CONDITIONS_COUNT: usize = 91;

/// Returns every failure condition in the registry, in registry order.
pub fn all_conditions() -> impl Iterator<Item = &'static FailureCondition> {
    SECTIONS.iter().flat_map(|s| s.conditions.iter())
}

/// Look up a single failure condition by hyphenated id (e.g. "01-007").
pub fn find_condition(id: &str) -> Option<&'static FailureCondition> {
    all_conditions().find(|c| c.id == id)
}

/// Look up a section by two-digit id (e.g. "01").
pub fn section_by_id(id: &str) -> Option<&'static Section> {
    SECTIONS.iter().find(|s| s.id == id)
}

pub fn auto_conditions() -> impl Iterator<Item = &'static FailureCondition> {
    all_conditions().filter(|c| c.verdict == Verdict::Auto)
}

pub fn human_conditions() -> impl Iterator<Item = &'static FailureCondition> {
    all_conditions().filter(|c| c.verdict == Verdict::Human)
}

pub fn out_of_scope_conditions() -> impl Iterator<Item = &'static FailureCondition> {
    all_conditions().filter(|c| c.verdict == Verdict::OutOfScope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_match_condition_counts() {
        let auto = auto_conditions().count();
        let human = human_conditions().count();
        let oos = out_of_scope_conditions().count();
        assert_eq!(auto, TOTALS.auto, "auto count drift");
        assert_eq!(human, TOTALS.human, "human count drift");
        assert_eq!(oos, TOTALS.out_of_scope, "out_of_scope count drift");
        assert_eq!(
            auto + human + oos,
            TOTALS.failure_conditions_in_this_registry,
            "sum != registry total",
        );
    }

    #[test]
    fn every_section_has_at_least_one_condition() {
        for s in SECTIONS {
            assert!(!s.conditions.is_empty(), "section {} empty", s.id);
        }
    }

    #[test]
    fn condition_ids_are_unique() {
        let mut ids: Vec<&str> = all_conditions().map(|c| c.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate condition ids");
    }

    #[test]
    fn find_condition_round_trips() {
        for c in all_conditions() {
            let got = find_condition(c.id).expect("found");
            assert_eq!(got.id, c.id);
            assert_eq!(got.section_id, c.section_id);
        }
    }

    #[test]
    fn section_prefix_matches_condition_ids() {
        for s in SECTIONS {
            for c in s.conditions {
                assert_eq!(c.section_id, s.id);
                assert!(
                    c.id.starts_with(&format!("{}-", s.id)),
                    "condition {} not in section {}",
                    c.id,
                    s.id,
                );
            }
        }
    }

    #[test]
    fn coverage_at_least_two_thirds() {
        // Slice 0 transcribed 91 of 136 conditions = 66.9%. This test
        // guards against accidental deletions in future slices; it does
        // NOT prevent intentional growth.
        let ratio = TOTALS.failure_conditions_in_this_registry as f64
            / TOTALS.failure_conditions_in_full_protocol as f64;
        assert!(ratio > 0.66, "registry coverage regressed: {ratio}");
    }
}
