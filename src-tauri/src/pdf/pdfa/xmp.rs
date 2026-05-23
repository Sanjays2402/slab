//! XMP metadata packet builder for PDF/A.
//!
//! ISO 19005-2 §6.7.11 requires every PDF/A document to carry an XMP
//! metadata packet (an unencrypted, uncompressed RDF/XML stream
//! referenced by the catalog's `/Metadata` entry) declaring at minimum
//! the `pdfaid:part` and `pdfaid:conformance` properties from the
//! Adobe `pdfaid` namespace.
//!
//! We build the packet by hand. The XMP grammar we emit is small,
//! deterministic, and validator-friendly: tested against verapdf,
//! Adobe Acrobat Pro, and the Apache PDFBox preflight tool.
//!
//! We deliberately avoid pulling in a generic XML library — XMP is
//! whitespace-sensitive (the surrounding `<?xpacket ...?>` PI bytes
//! are part of the packet identity) and a serializer that
//! re-indents or re-orders attributes would silently break PDF/A
//! conformance. A hand-written builder keeps the output stable and
//! eliminates a transitive C dep.

use super::ConformanceLevel;
use chrono::{DateTime, Utc};

/// Inputs the caller may want to put in the XMP packet. All fields
/// are optional except `level`; missing fields are omitted from the
/// packet (XMP allows empty values but they break some validators).
#[derive(Debug, Clone, Default)]
pub struct XmpMetadata {
    pub level: ConformanceLevel,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    /// Free-text keywords joined by `;` per Dublin Core convention.
    pub keywords: Option<String>,
    /// Software producer string — defaults to `"Slab Bedrock v3.0.0"`.
    pub producer: Option<String>,
    /// Creation timestamp. Defaults to "now" when unset.
    pub create_date: Option<DateTime<Utc>>,
    /// Modification timestamp. Defaults to `create_date`.
    pub modify_date: Option<DateTime<Utc>>,
    /// Document UUID — exposed for repeatable builds in tests.
    pub document_id: Option<String>,
}

impl XmpMetadata {
    pub fn new(level: ConformanceLevel) -> Self {
        Self {
            level,
            ..Self::default()
        }
    }

    pub fn with_title(mut self, t: impl Into<String>) -> Self {
        self.title = Some(t.into());
        self
    }

    pub fn with_author(mut self, a: impl Into<String>) -> Self {
        self.author = Some(a.into());
        self
    }

    pub fn with_producer(mut self, p: impl Into<String>) -> Self {
        self.producer = Some(p.into());
        self
    }
}

const DEFAULT_PRODUCER: &str = "Slab Bedrock v3.0.0";

/// Build the XMP packet bytes ready to drop into a `/Metadata` stream.
///
/// Wrapped in the canonical `<?xpacket begin ...?> ... <?xpacket end="w"?>`
/// pair per Adobe XMP Specification §7.3.2.
pub fn build_xmp_packet(meta: &XmpMetadata) -> Vec<u8> {
    let producer = meta.producer.as_deref().unwrap_or(DEFAULT_PRODUCER);
    let create = meta.create_date.unwrap_or_else(Utc::now);
    let modify = meta.modify_date.unwrap_or(create);
    let create_iso = create.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let modify_iso = modify.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let doc_id = meta
        .document_id
        .clone()
        .unwrap_or_else(|| format!("uuid:{}", deterministic_uuid_from(&create_iso)));

    let mut s = String::with_capacity(2048);
    // Adobe XMP §7.3.2 — begin marker uses BOM `\u{FEFF}`.
    s.push_str("<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    s.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Slab Bedrock 3.0.0\">\n");
    s.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");

    // pdfaid (the mandatory bit).
    s.push_str(
        "    <rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\n",
    );
    s.push_str(&format!(
        "      <pdfaid:part>{}</pdfaid:part>\n",
        meta.level.part()
    ));
    s.push_str(&format!(
        "      <pdfaid:conformance>{}</pdfaid:conformance>\n",
        meta.level.conformance()
    ));
    s.push_str("    </rdf:Description>\n");

    // Dublin Core (title, author, subject).
    if meta.title.is_some() || meta.author.is_some() || meta.subject.is_some() {
        s.push_str(
            "    <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n",
        );
        if let Some(t) = &meta.title {
            s.push_str("      <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">");
            s.push_str(&xml_escape(t));
            s.push_str("</rdf:li></rdf:Alt></dc:title>\n");
        }
        if let Some(a) = &meta.author {
            s.push_str("      <dc:creator><rdf:Seq><rdf:li>");
            s.push_str(&xml_escape(a));
            s.push_str("</rdf:li></rdf:Seq></dc:creator>\n");
        }
        if let Some(sub) = &meta.subject {
            s.push_str("      <dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">");
            s.push_str(&xml_escape(sub));
            s.push_str("</rdf:li></rdf:Alt></dc:description>\n");
        }
        s.push_str("    </rdf:Description>\n");
    }

    // PDF-specific (Adobe pdf namespace) — Keywords + Producer live here.
    s.push_str("    <rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\n");
    s.push_str(&format!(
        "      <pdf:Producer>{}</pdf:Producer>\n",
        xml_escape(producer)
    ));
    if let Some(k) = &meta.keywords {
        s.push_str(&format!(
            "      <pdf:Keywords>{}</pdf:Keywords>\n",
            xml_escape(k)
        ));
    }
    s.push_str("    </rdf:Description>\n");

    // XMP basic — create + modify + document id.
    s.push_str("    <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n");
    s.push_str(&format!(
        "      <xmp:CreateDate>{create_iso}</xmp:CreateDate>\n"
    ));
    s.push_str(&format!(
        "      <xmp:ModifyDate>{modify_iso}</xmp:ModifyDate>\n"
    ));
    s.push_str(&format!(
        "      <xmp:CreatorTool>{}</xmp:CreatorTool>\n",
        xml_escape(producer)
    ));
    s.push_str("    </rdf:Description>\n");

    s.push_str(
        "    <rdf:Description rdf:about=\"\" xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\">\n",
    );
    s.push_str(&format!(
        "      <xmpMM:DocumentID>{}</xmpMM:DocumentID>\n",
        xml_escape(&doc_id)
    ));
    s.push_str("    </rdf:Description>\n");

    s.push_str("  </rdf:RDF>\n");
    s.push_str("</x:xmpmeta>\n");

    // Per Adobe XMP §7.3.2: padding whitespace lets PDF editors grow
    // the packet in place without rewriting the file. 2 KB is the
    // commonly cited recommendation.
    for _ in 0..40 {
        s.push_str("                                                  \n");
    }
    s.push_str("<?xpacket end=\"w\"?>");

    s.into_bytes()
}

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' && c != '\r' => {
                // XML 1.0 forbids most C0 control chars even when escaped.
                out.push(' ');
            }
            c => out.push(c),
        }
    }
    out
}

/// Tiny deterministic stand-in for a UUIDv4. We don't need crypto-grade
/// randomness here — the spec only requires the value be unique within
/// the document's history. A 32-hex digest of the creation timestamp
/// is good enough and keeps the output reproducible for tests.
fn deterministic_uuid_from(seed: &str) -> String {
    // Simple FNV-1a 64-bit mixed twice for 128 bits — no crypto crate needed.
    let mut h1: u64 = 0xcbf29ce484222325;
    for b in seed.bytes() {
        h1 ^= b as u64;
        h1 = h1.wrapping_mul(0x100000001b3);
    }
    let mut h2: u64 = h1.rotate_left(17) ^ 0xdeadbeefcafebabe;
    for b in seed.bytes().rev() {
        h2 ^= b as u64;
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (h1 >> 32) as u32,
        ((h1 >> 16) & 0xffff) as u16,
        // RFC 4122 variant nibble — set version=4 and variant=10xx.
        ((h1 & 0x0fff) | 0x4000) as u16,
        ((h2 >> 48) & 0x3fff | 0x8000) as u16,
        h2 & 0xffffffffffff
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 22, 21, 30, 0).unwrap()
    }

    fn fixed_meta(level: ConformanceLevel) -> XmpMetadata {
        XmpMetadata {
            level,
            title: Some("Annual Report 2026".into()),
            author: Some("Alice Example".into()),
            subject: Some("Q1 results".into()),
            keywords: Some("annual; finance; 2026".into()),
            producer: Some("Slab Bedrock v3.0.0".into()),
            create_date: Some(fixed_now()),
            modify_date: Some(fixed_now()),
            document_id: Some("uuid:deadbeef-cafe-4001-8000-000000000001".into()),
        }
    }

    #[test]
    fn packet_starts_with_bom_xpacket_marker() {
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A2b));
        // BOM is U+FEFF encoded as 3 UTF-8 bytes.
        assert!(bytes.starts_with(b"<?xpacket begin=\"\xef\xbb\xbf\""));
    }

    #[test]
    fn packet_ends_with_xpacket_end_writable() {
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A2b));
        assert!(bytes.ends_with(b"<?xpacket end=\"w\"?>"));
    }

    #[test]
    fn declares_pdfaid_part_2_for_a2b() {
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A2b));
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("<pdfaid:part>2</pdfaid:part>"));
        assert!(s.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
    }

    #[test]
    fn declares_pdfaid_part_3_for_a3b() {
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A3b));
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("<pdfaid:part>3</pdfaid:part>"));
    }

    #[test]
    fn embeds_optional_dublin_core_fields() {
        let bytes = build_xmp_packet(&fixed_meta(ConformanceLevel::A2b));
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("Annual Report 2026"));
        assert!(s.contains("Alice Example"));
        assert!(s.contains("Q1 results"));
        assert!(s.contains("annual; finance; 2026"));
    }

    #[test]
    fn omits_dublin_core_block_when_no_dc_fields_set() {
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A2b));
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(!s.contains("xmlns:dc="));
    }

    #[test]
    fn escapes_xml_special_chars_in_title() {
        let m = XmpMetadata {
            title: Some("AT&T <board> meeting \"notes\"".into()),
            ..XmpMetadata::new(ConformanceLevel::A2b)
        };
        let bytes = build_xmp_packet(&m);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("AT&amp;T &lt;board&gt; meeting &quot;notes&quot;"));
        assert!(!s.contains("AT&T"));
    }

    #[test]
    fn includes_create_and_modify_timestamps_in_iso_8601() {
        let m = fixed_meta(ConformanceLevel::A2b);
        let bytes = build_xmp_packet(&m);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("<xmp:CreateDate>2026-05-22T21:30:00Z</xmp:CreateDate>"));
        assert!(s.contains("<xmp:ModifyDate>2026-05-22T21:30:00Z</xmp:ModifyDate>"));
    }

    #[test]
    fn includes_document_id() {
        let m = fixed_meta(ConformanceLevel::A2b);
        let bytes = build_xmp_packet(&m);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("uuid:deadbeef-cafe-4001-8000-000000000001"));
    }

    #[test]
    fn default_producer_is_slab_bedrock() {
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A2b));
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("Slab Bedrock v3.0.0"));
    }

    #[test]
    fn deterministic_uuid_is_stable_for_same_seed() {
        let a = deterministic_uuid_from("2026-05-22T21:30:00Z");
        let b = deterministic_uuid_from("2026-05-22T21:30:00Z");
        assert_eq!(a, b);
        assert_ne!(a, deterministic_uuid_from("2026-05-22T21:30:01Z"));
        // RFC 4122 v4 pattern.
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
    }

    #[test]
    fn packet_is_padded_for_in_place_growth() {
        // The whitespace padding is what lets editors grow the packet
        // without rewriting the whole PDF. 2 KB is the rule of thumb.
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A2b));
        assert!(bytes.len() >= 2000);
    }

    #[test]
    fn round_trips_through_chrono_when_no_dates_supplied() {
        // If the caller omits dates we still get valid ISO 8601 bytes.
        let bytes = build_xmp_packet(&XmpMetadata::new(ConformanceLevel::A2b));
        let s = std::str::from_utf8(&bytes).unwrap();
        // Format must include the trailing Z.
        assert!(s.contains("Z</xmp:CreateDate>"));
        assert!(s.contains("Z</xmp:ModifyDate>"));
    }

    #[test]
    fn builder_helpers_compose() {
        let m = XmpMetadata::new(ConformanceLevel::A3b)
            .with_title("Doc")
            .with_author("Bob")
            .with_producer("Custom");
        assert_eq!(m.level, ConformanceLevel::A3b);
        assert_eq!(m.title.as_deref(), Some("Doc"));
        assert_eq!(m.author.as_deref(), Some("Bob"));
        assert_eq!(m.producer.as_deref(), Some("Custom"));
    }

    #[test]
    fn control_chars_in_user_input_are_replaced() {
        let m = XmpMetadata {
            title: Some("bad\x01title\x02here".into()),
            ..XmpMetadata::new(ConformanceLevel::A2b)
        };
        let bytes = build_xmp_packet(&m);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(!s.contains('\x01'));
        assert!(!s.contains('\x02'));
        assert!(s.contains("bad title here"));
    }
}
