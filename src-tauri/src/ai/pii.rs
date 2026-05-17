// Beacon PII Detector — discover personally-identifiable info in a PDF.
//
// Workflow (Slice 8 of v0.10.0 Beacon):
//   1. Extract per-page text via `pdf::extract::extract_text`.
//   2. Run a battery of fast regex passes (email / SSN / phone / credit-card,
//      plus optional custom patterns). These are deterministic and cheap.
//   3. (Optional) ask the configured `AiProvider` to flag *names and
//      addresses* — categories that regex can't catch reliably. The LLM
//      sees per-page text and is asked to return JSON of `{kind, text}`
//      hits. Disabled by default to keep the panel snappy; the UI toggles
//      it on with a checkbox.
//   4. De-duplicate (same kind + same canonical text on same page).
//   5. Return a `Vec<PiiHit>` to the front-end.
//
// The detector does *not* mutate the PDF. The user then either:
//   - Clicks "Redact all" → we call `pdf::auto_redact` with the kinds the
//     user kept ticked. This reuses the existing redaction pipeline so we
//     have a single source of truth for "what black bars look like".
//   - Clicks individual entries to highlight on a single page (UI-only).
//
// All HTTP surfaces are tested via in-memory `MockProvider` — no real
// Ollama / OpenAI traffic in CI. The regex passes are tested against
// fixture strings; we don't render real PDFs in the regex unit tests
// because the regex implementation is content-agnostic.

use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use crate::pdf::auto_redact::{PRESET_CC, PRESET_EMAIL, PRESET_PHONE, PRESET_SSN};
use crate::pdf::extract::extract_text;
use crate::pdf::PdfError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// What kind of PII a hit is. The front-end uses this to render a
/// coloured pill, and `Redact all` translates these into auto-redact
/// preset names.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum PiiKind {
    Email,
    Ssn,
    Phone,
    CreditCard,
    /// LLM-suggested person name. Lower confidence than the regex kinds —
    /// the UI distinguishes these so users can review before redacting.
    Name,
    /// LLM-suggested street address.
    Address,
    /// User-provided custom pattern. The `label` field on the hit will
    /// be the human-readable name the user gave it.
    Custom,
}

impl PiiKind {
    /// Map this kind to the `auto_redact` preset name, if one exists.
    /// `Name`, `Address`, `Custom` return `None` — the UI must hand
    /// auto-redact an explicit regex for those.
    pub fn auto_redact_preset(&self) -> Option<&'static str> {
        match self {
            PiiKind::Email => Some("email"),
            PiiKind::Ssn => Some("ssn"),
            PiiKind::Phone => Some("phone"),
            PiiKind::CreditCard => Some("cc"),
            PiiKind::Name | PiiKind::Address | PiiKind::Custom => None,
        }
    }
}

/// A single PII finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PiiHit {
    /// 1-indexed page number where the match was found.
    pub page: u32,
    /// The kind of PII.
    pub kind: PiiKind,
    /// The matched text verbatim (e.g. `"jane@example.com"`).
    pub text: String,
    /// Optional human-readable label for `Custom` hits (the user-provided
    /// name for the custom pattern). Empty string for built-in kinds.
    pub label: String,
}

/// Knobs for `find_pii`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PiiOpts {
    /// If true, ask the configured `AiProvider` to flag names + addresses
    /// per page. Adds latency but catches things regex can't.
    #[serde(default)]
    pub include_llm_pass: bool,
    /// Extra patterns to scan for. Each tuple is `(label, regex)`. The
    /// label shows up in the UI chip.
    #[serde(default)]
    pub custom_patterns: Vec<CustomPattern>,
    /// Restrict the regex scan to these kinds. Empty = all four built-in
    /// regex kinds run.
    #[serde(default)]
    pub kinds: Vec<PiiKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPattern {
    pub label: String,
    pub regex: String,
}

/// Errors from `find_pii` — kept narrow so the UI layer can map them to
/// friendly strings.
#[derive(Debug, thiserror::Error)]
pub enum PiiError {
    #[error("read PDF: {0}")]
    Pdf(#[from] PdfError),

    #[error("bad regex {pattern:?}: {error}")]
    BadRegex { pattern: String, error: String },

    #[error("AI provider: {0}")]
    Provider(#[from] AiError),
}

/// Pure-regex pass over `pages`. Returns hits in (page, start-offset)
/// order. Pulled out for testability — no FS / no PDF parsing.
pub fn scan_pages_with_regex(pages: &[String], opts: &PiiOpts) -> Result<Vec<PiiHit>, PiiError> {
    // Decide which built-in kinds to run.
    let want = |k: PiiKind| -> bool {
        if opts.kinds.is_empty() {
            return matches!(
                k,
                PiiKind::Email | PiiKind::Ssn | PiiKind::Phone | PiiKind::CreditCard
            );
        }
        opts.kinds.contains(&k)
    };

    // Compile up front so a bad custom pattern fails fast.
    let mut builtins: Vec<(PiiKind, Regex)> = Vec::new();
    if want(PiiKind::Email) {
        builtins.push((
            PiiKind::Email,
            Regex::new(PRESET_EMAIL).expect("built-in email regex valid"),
        ));
    }
    if want(PiiKind::Ssn) {
        builtins.push((
            PiiKind::Ssn,
            Regex::new(PRESET_SSN).expect("built-in ssn regex valid"),
        ));
    }
    if want(PiiKind::Phone) {
        builtins.push((
            PiiKind::Phone,
            Regex::new(PRESET_PHONE).expect("built-in phone regex valid"),
        ));
    }
    if want(PiiKind::CreditCard) {
        builtins.push((
            PiiKind::CreditCard,
            Regex::new(PRESET_CC).expect("built-in cc regex valid"),
        ));
    }
    let mut customs: Vec<(String, Regex)> = Vec::new();
    for cp in &opts.custom_patterns {
        let re = Regex::new(&cp.regex).map_err(|e| PiiError::BadRegex {
            pattern: cp.regex.clone(),
            error: e.to_string(),
        })?;
        customs.push((cp.label.clone(), re));
    }

    let mut hits: Vec<PiiHit> = Vec::new();
    for (idx, page_text) in pages.iter().enumerate() {
        let page_no = (idx + 1) as u32;
        for (kind, re) in &builtins {
            for m in re.find_iter(page_text) {
                hits.push(PiiHit {
                    page: page_no,
                    kind: *kind,
                    text: m.as_str().to_string(),
                    label: String::new(),
                });
            }
        }
        for (label, re) in &customs {
            for m in re.find_iter(page_text) {
                hits.push(PiiHit {
                    page: page_no,
                    kind: PiiKind::Custom,
                    text: m.as_str().to_string(),
                    label: label.clone(),
                });
            }
        }
    }

    Ok(dedupe(hits))
}

/// De-duplicate by (page, kind, normalized text). Keeps first occurrence
/// so the order on the page is preserved.
fn dedupe(hits: Vec<PiiHit>) -> Vec<PiiHit> {
    let mut seen: BTreeSet<(u32, PiiKind, String)> = BTreeSet::new();
    let mut out: Vec<PiiHit> = Vec::with_capacity(hits.len());
    for h in hits {
        let key = (h.page, h.kind, h.text.to_lowercase());
        if seen.insert(key) {
            out.push(h);
        }
    }
    out
}

/// Prompt the LLM gets when we ask it to flag names + addresses on a page.
/// Pulled out for testability — pin the wording.
fn llm_system_prompt() -> &'static str {
    "You are a privacy auditor. The user will paste one page of a PDF. \
     Your job: list every PERSONAL NAME and STREET ADDRESS that appears. \
     Output strict JSON only, no prose, no markdown. Schema: \
     {\"hits\":[{\"kind\":\"name\"|\"address\",\"text\":\"<exact substring>\"}]}. \
     If there are no names or addresses, output {\"hits\":[]}."
}

#[derive(Debug, Deserialize)]
struct LlmHitWire {
    kind: String,
    text: String,
}
#[derive(Debug, Deserialize)]
struct LlmReplyWire {
    hits: Vec<LlmHitWire>,
}

/// Ask the provider to scan one page. Returns hits or an empty vec on
/// any parse failure — the LLM pass is best-effort and never blocks
/// regex hits from surfacing.
async fn llm_scan_page(
    provider: &dyn AiProvider,
    page_no: u32,
    page_text: &str,
) -> Result<Vec<PiiHit>, AiError> {
    // Skip empty pages entirely — saves a round trip.
    if page_text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let msgs = vec![
        ChatMessage {
            role: ChatRole::System,
            content: llm_system_prompt().to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: format!("<page {page_no}>\n{page_text}\n</page {page_no}>"),
        },
    ];
    let opts = ChatOpts {
        // Lower temp than chat — we want deterministic extraction.
        temperature: Some(0.0),
        max_tokens: Some(1024),
        ..Default::default()
    };
    let resp = provider.chat(&msgs, &opts).await?;
    let parsed = parse_llm_reply(&resp.content);
    Ok(parsed
        .into_iter()
        .map(|(kind, text)| PiiHit {
            page: page_no,
            kind,
            text,
            label: String::new(),
        })
        .collect())
}

/// Liberal JSON parser. Strips ```json ... ``` fences if present, then
/// finds the outermost `{...}` and tries `serde_json`. Silently returns
/// an empty list on any parse failure (the LLM occasionally surrounds
/// JSON with chatty prose despite the system prompt — we forgive that
/// rather than failing the whole panel).
pub fn parse_llm_reply(raw: &str) -> Vec<(PiiKind, String)> {
    let s = raw.trim();
    // Strip a markdown fence if the model wrapped its output.
    let body = if let Some(rest) = s.strip_prefix("```json") {
        rest.trim_end_matches("```").trim()
    } else if let Some(rest) = s.strip_prefix("```") {
        rest.trim_end_matches("```").trim()
    } else {
        s
    };
    // Find the outermost {...} so trailing chatter doesn't break parse.
    let start = match body.find('{') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let end = match body.rfind('}') {
        Some(i) => i,
        None => return Vec::new(),
    };
    if end <= start {
        return Vec::new();
    }
    let json = &body[start..=end];
    let reply: LlmReplyWire = match serde_json::from_str(json) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<(PiiKind, String)> = Vec::with_capacity(reply.hits.len());
    for h in reply.hits {
        let kind = match h.kind.to_lowercase().as_str() {
            "name" | "person" | "person_name" => PiiKind::Name,
            "address" | "street_address" | "location" => PiiKind::Address,
            _ => continue, // ignore anything else the LLM hallucinated
        };
        let text = h.text.trim().to_string();
        if !text.is_empty() {
            out.push((kind, text));
        }
    }
    out
}

/// Top-level entry: read the PDF, run regex, optionally run LLM, return
/// merged hit list.
pub async fn find_pii(
    input: &Path,
    provider: Option<&dyn AiProvider>,
    opts: PiiOpts,
) -> Result<Vec<PiiHit>, PiiError> {
    let pages = extract_text(input)?;
    let mut hits = scan_pages_with_regex(&pages, &opts)?;
    if opts.include_llm_pass {
        let prov = provider.ok_or_else(|| {
            PiiError::Provider(AiError::ProviderUnavailable(
                "LLM pass requested but no provider configured".to_string(),
            ))
        })?;
        for (idx, page_text) in pages.iter().enumerate() {
            let page_no = (idx + 1) as u32;
            // Best-effort: a failed page doesn't abort the whole scan,
            // but we DO surface the first hard error (auth, network) so
            // the UI can show a useful message.
            match llm_scan_page(prov, page_no, page_text).await {
                Ok(mut page_hits) => hits.append(&mut page_hits),
                Err(AiError::ProviderUnavailable(msg)) => {
                    return Err(PiiError::Provider(AiError::ProviderUnavailable(msg)));
                }
                Err(AiError::RateLimited) => {
                    return Err(PiiError::Provider(AiError::RateLimited));
                }
                // InvalidResponse / Network on a single page → skip page,
                // continue. We err on the side of "show what we got".
                Err(_) => continue,
            }
        }
        hits = dedupe(hits);
    }
    Ok(hits)
}

/// Summary stats for the UI footer.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PiiSummary {
    pub emails: u32,
    pub ssns: u32,
    pub phones: u32,
    pub credit_cards: u32,
    pub names: u32,
    pub addresses: u32,
    pub customs: u32,
    pub total: u32,
}

impl PiiSummary {
    pub fn from_hits(hits: &[PiiHit]) -> Self {
        let mut s = PiiSummary::default();
        for h in hits {
            match h.kind {
                PiiKind::Email => s.emails += 1,
                PiiKind::Ssn => s.ssns += 1,
                PiiKind::Phone => s.phones += 1,
                PiiKind::CreditCard => s.credit_cards += 1,
                PiiKind::Name => s.names += 1,
                PiiKind::Address => s.addresses += 1,
                PiiKind::Custom => s.customs += 1,
            }
        }
        s.total = hits.len() as u32;
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ChatResponse;
    use async_trait::async_trait;
    use std::sync::Mutex;

    // --- Regex pass ------------------------------------------------------

    #[test]
    fn regex_finds_email_ssn_phone_cc_on_correct_pages() {
        let pages = vec![
            "Contact jane.doe@example.com for details.".to_string(),
            "SSN: 123-45-6789. Phone: (415) 555-1234.".to_string(),
            "Card: 4111 1111 1111 1111 (expired).".to_string(),
        ];
        let hits = scan_pages_with_regex(&pages, &PiiOpts::default()).unwrap();
        assert_eq!(hits.len(), 4, "got {hits:?}");
        let kinds: Vec<_> = hits.iter().map(|h| (h.page, h.kind)).collect();
        assert!(kinds.contains(&(1, PiiKind::Email)));
        assert!(kinds.contains(&(2, PiiKind::Ssn)));
        assert!(kinds.contains(&(2, PiiKind::Phone)));
        assert!(kinds.contains(&(3, PiiKind::CreditCard)));
    }

    #[test]
    fn regex_dedupes_same_match_per_page() {
        let pages = vec!["a@b.com a@b.com A@B.COM".to_string()];
        let hits = scan_pages_with_regex(&pages, &PiiOpts::default()).unwrap();
        // Three textual occurrences but two distinct case-insensitive
        // canonical forms collapse to one — we lowercase the dedupe key.
        // First occurrence wins → "a@b.com".
        let emails: Vec<_> = hits.iter().filter(|h| h.kind == PiiKind::Email).collect();
        assert_eq!(emails.len(), 1);
        assert_eq!(emails[0].text, "a@b.com");
    }

    #[test]
    fn regex_kinds_filter_excludes_other_kinds() {
        let pages = vec!["a@b.com 123-45-6789".to_string()];
        let opts = PiiOpts {
            kinds: vec![PiiKind::Ssn],
            ..PiiOpts::default()
        };
        let hits = scan_pages_with_regex(&pages, &opts).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, PiiKind::Ssn);
    }

    #[test]
    fn regex_custom_pattern_attaches_label() {
        let pages = vec!["Project codenames: SLAB-1234, SLAB-9999".to_string()];
        let opts = PiiOpts {
            custom_patterns: vec![CustomPattern {
                label: "Project ID".to_string(),
                regex: r"SLAB-\d{4}".to_string(),
            }],
            kinds: vec![PiiKind::Email], // turn off built-ins so test stays focused
            ..PiiOpts::default()
        };
        let hits = scan_pages_with_regex(&pages, &opts).unwrap();
        assert_eq!(hits.len(), 2);
        for h in &hits {
            assert_eq!(h.kind, PiiKind::Custom);
            assert_eq!(h.label, "Project ID");
        }
        assert_eq!(hits[0].text, "SLAB-1234");
        assert_eq!(hits[1].text, "SLAB-9999");
    }

    #[test]
    fn regex_bad_custom_pattern_returns_bad_regex_error() {
        let pages = vec!["whatever".to_string()];
        let opts = PiiOpts {
            custom_patterns: vec![CustomPattern {
                label: "Bad".to_string(),
                regex: r"(?P<".to_string(), // un-closed group
            }],
            ..PiiOpts::default()
        };
        let err = scan_pages_with_regex(&pages, &opts).unwrap_err();
        match err {
            PiiError::BadRegex { pattern, .. } => assert_eq!(pattern, "(?P<"),
            other => panic!("expected BadRegex, got {other:?}"),
        }
    }

    #[test]
    fn regex_empty_pages_yields_no_hits() {
        let pages: Vec<String> = vec![String::new(), String::new()];
        let hits = scan_pages_with_regex(&pages, &PiiOpts::default()).unwrap();
        assert!(hits.is_empty());
    }

    // --- LLM reply parser ----------------------------------------------

    #[test]
    fn parse_llm_reply_plain_json() {
        let raw = r#"{"hits":[{"kind":"name","text":"Jane Doe"},{"kind":"address","text":"742 Evergreen Terrace"}]}"#;
        let parsed = parse_llm_reply(raw);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], (PiiKind::Name, "Jane Doe".to_string()));
        assert_eq!(
            parsed[1],
            (PiiKind::Address, "742 Evergreen Terrace".to_string())
        );
    }

    #[test]
    fn parse_llm_reply_fenced_json() {
        let raw = "```json\n{\"hits\":[{\"kind\":\"name\",\"text\":\"Bob\"}]}\n```";
        let parsed = parse_llm_reply(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], (PiiKind::Name, "Bob".to_string()));
    }

    #[test]
    fn parse_llm_reply_with_chatty_prefix_still_parses() {
        let raw = "Sure! Here's the JSON: {\"hits\":[{\"kind\":\"name\",\"text\":\"Alice\"}]}";
        let parsed = parse_llm_reply(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], (PiiKind::Name, "Alice".to_string()));
    }

    #[test]
    fn parse_llm_reply_unknown_kind_filtered_out() {
        let raw = r#"{"hits":[{"kind":"name","text":"OK"},{"kind":"weather","text":"sunny"}]}"#;
        let parsed = parse_llm_reply(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], (PiiKind::Name, "OK".to_string()));
    }

    #[test]
    fn parse_llm_reply_garbage_is_empty() {
        assert!(parse_llm_reply("nope").is_empty());
        assert!(parse_llm_reply("").is_empty());
        assert!(parse_llm_reply("{").is_empty());
    }

    // --- LLM integration via in-memory MockProvider --------------------

    /// In-memory provider; returns the next pre-queued reply on each
    /// `chat` call. `embed` panics — PII doesn't use it.
    struct MockProvider {
        replies: Mutex<Vec<Result<ChatResponse, AiError>>>,
    }

    impl MockProvider {
        fn new(replies: Vec<Result<ChatResponse, AiError>>) -> Self {
            Self {
                replies: Mutex::new(replies),
            }
        }
    }

    #[async_trait]
    impl AiProvider for MockProvider {
        async fn chat(
            &self,
            _msgs: &[ChatMessage],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            let mut q = self.replies.lock().unwrap();
            if q.is_empty() {
                return Err(AiError::InvalidResponse("mock queue empty".into()));
            }
            q.remove(0)
        }
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            panic!("PII never embeds")
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn llm_scan_page_skips_empty_page_without_calling_provider() {
        // Empty mock queue: any call would error. We rely on the early return.
        let prov = MockProvider::new(vec![]);
        let hits = llm_scan_page(&prov, 5, "   \n  ").await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn llm_scan_page_parses_names_and_addresses() {
        let prov = MockProvider::new(vec![Ok(ChatResponse {
            content: r#"{"hits":[{"kind":"name","text":"Jane Doe"},{"kind":"address","text":"1 Main St"}]}"#
                .to_string(),
            model: "mock".to_string(),
        })]);
        let hits = llm_scan_page(&prov, 3, "Jane Doe lives at 1 Main St.")
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].page, 3);
        assert_eq!(hits[0].kind, PiiKind::Name);
        assert_eq!(hits[1].kind, PiiKind::Address);
    }

    #[tokio::test]
    async fn llm_pass_provider_unavailable_aborts_scan() {
        let prov = MockProvider::new(vec![Err(AiError::ProviderUnavailable(
            "connection refused".to_string(),
        ))]);
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("src.pdf");
        crate::pdf::test_fixtures::make_n_page_pdf(&p, 1);
        let opts = PiiOpts {
            include_llm_pass: true,
            ..PiiOpts::default()
        };
        let err = find_pii(&p, Some(&prov), opts).await.unwrap_err();
        matches!(err, PiiError::Provider(AiError::ProviderUnavailable(_)));
    }

    #[tokio::test]
    async fn llm_pass_per_page_invalid_response_is_skipped_not_fatal() {
        // 2-page fixture, 2 replies: page 1 garbage, page 2 valid.
        let prov = MockProvider::new(vec![
            Err(AiError::InvalidResponse("nope".to_string())),
            Ok(ChatResponse {
                content: r#"{"hits":[{"kind":"name","text":"Carol"}]}"#.to_string(),
                model: "mock".to_string(),
            }),
        ]);
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("src.pdf");
        crate::pdf::test_fixtures::make_n_page_pdf(&p, 2);
        let opts = PiiOpts {
            include_llm_pass: true,
            ..PiiOpts::default()
        };
        let hits = find_pii(&p, Some(&prov), opts).await.unwrap();
        // The fixture pages contain no emails/SSN/etc, so we should only
        // see the LLM "Carol" hit from page 2.
        let names: Vec<_> = hits.iter().filter(|h| h.kind == PiiKind::Name).collect();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].text, "Carol");
        assert_eq!(names[0].page, 2);
    }

    // --- Summary -------------------------------------------------------

    #[test]
    fn summary_counts_per_kind() {
        let hits = vec![
            PiiHit {
                page: 1,
                kind: PiiKind::Email,
                text: "a@b.com".into(),
                label: String::new(),
            },
            PiiHit {
                page: 1,
                kind: PiiKind::Email,
                text: "c@d.com".into(),
                label: String::new(),
            },
            PiiHit {
                page: 2,
                kind: PiiKind::Ssn,
                text: "123-45-6789".into(),
                label: String::new(),
            },
            PiiHit {
                page: 2,
                kind: PiiKind::Name,
                text: "Eve".into(),
                label: String::new(),
            },
        ];
        let s = PiiSummary::from_hits(&hits);
        assert_eq!(s.emails, 2);
        assert_eq!(s.ssns, 1);
        assert_eq!(s.names, 1);
        assert_eq!(s.total, 4);
        assert_eq!(s.phones, 0);
    }

    #[test]
    fn pii_kind_auto_redact_preset_mapping() {
        assert_eq!(PiiKind::Email.auto_redact_preset(), Some("email"));
        assert_eq!(PiiKind::Ssn.auto_redact_preset(), Some("ssn"));
        assert_eq!(PiiKind::Phone.auto_redact_preset(), Some("phone"));
        assert_eq!(PiiKind::CreditCard.auto_redact_preset(), Some("cc"));
        assert_eq!(PiiKind::Name.auto_redact_preset(), None);
        assert_eq!(PiiKind::Address.auto_redact_preset(), None);
        assert_eq!(PiiKind::Custom.auto_redact_preset(), None);
    }
}
