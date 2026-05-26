// Atlas Suggest — heuristic Smart Folder suggestion engine.
//
// Reads the recent library search log, groups queries by their dominant
// non-stopword token, and emits up to 3 candidate Smart Folder presets.
// Honors `library_suggestion_dismissed` so the user never sees the same
// suggestion twice.
//
// This is the deterministic always-on baseline. A future AI-powered
// variant (`crate::ai::folder_suggest_ai`) can post-process these to
// produce nicer names — but the heuristic alone must be production-quality.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::registry::{LibraryDb, LibraryError};
use super::search_log;

/// Minimum recorded queries before we emit any suggestions. Below this,
/// the UI hides the "Suggested for you" section entirely.
pub const MIN_LOG_SIZE: usize = 10;
/// Minimum cluster size to be worth suggesting.
const MIN_CLUSTER_SUPPORT: usize = 3;
/// Hard cap on suggestions returned per call.
const MAX_SUGGESTIONS: usize = 3;
/// Number of recent queries to consider.
const LOOKBACK: usize = 50;

const STOPWORDS: &[&str] = &[
    "the", "and", "or", "of", "a", "an", "to", "in", "for", "with", "on", "at", "by", "from", "is",
    "it", "be", "as", "this", "that", "these", "those", "i", "my", "me", "you", "your", "we", "us",
    "our", "they", "them", "their", "but", "if", "not", "no", "yes", "do", "does", "did", "have",
    "has", "had", "was", "were", "are", "am", "been", "being", "so", "than", "then", "into", "out",
    "up", "down", "over", "under", "about",
];

const PALETTE: &[&str] = &[
    "#7c3aed", // violet-600
    "#0891b2", // cyan-600
    "#059669", // emerald-600
    "#ea580c", // orange-600
    "#dc2626", // red-600
    "#2563eb", // blue-600
];

/// One suggested Smart Folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Suggestion {
    /// Human-friendly folder name, e.g. "Invoice".
    pub name: String,
    /// Single-char emoji icon picked by `icon_for_token`.
    pub icon: String,
    /// Hex color picked from PALETTE deterministically by cluster_hash.
    pub color: String,
    /// The dominant token, ready to drop into a preset's search filter.
    pub query_template: String,
    /// Plain-English why ("You searched 'invoice' 8 times this week").
    pub reason: String,
    /// Stable hash used for dismissal bookkeeping.
    pub cluster_hash: String,
    /// Number of queries in the cluster.
    pub support: usize,
}

fn tokenize(q: &str) -> Vec<String> {
    q.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .filter(|t| !STOPWORDS.contains(t))
        // Drop pure-numeric tokens ("2025", "1099") — they're rarely a
        // useful folder name on their own and tend to tie with the more
        // meaningful word in the query ("invoice 2025").
        .filter(|t| t.chars().any(|c| c.is_alphabetic()))
        .map(|s| s.to_string())
        .collect()
}

/// Pick an emoji for a token using a tiny domain table. Falls back to 📁.
fn icon_for_token(token: &str) -> &'static str {
    match token {
        "invoice" | "invoices" | "bill" | "bills" | "receipt" | "receipts" => "🧾",
        "contract" | "contracts" | "agreement" | "agreements" | "nda" | "ndas" => "📜",
        "tax" | "taxes" | "w2" | "1099" | "irs" => "💰",
        "resume" | "cv" | "cover" | "letter" => "📄",
        "paper" | "papers" | "research" | "arxiv" | "preprint" => "🔬",
        "report" | "reports" | "annual" | "quarterly" => "📊",
        "manual" | "guide" | "handbook" | "spec" | "specification" | "rfc" => "📘",
        "photo" | "photos" | "image" | "images" | "scan" | "scans" => "📷",
        "ticket" | "tickets" | "boarding" | "itinerary" | "travel" => "✈️",
        "medical" | "health" | "doctor" | "prescription" | "rx" => "🏥",
        "legal" | "lawsuit" | "court" | "deposition" => "⚖️",
        "patent" | "patents" | "trademark" => "💡",
        _ => "📁",
    }
}

/// Deterministic, stable hash of the cluster identity (dominant token +
/// sorted list of contributing query strings). Used for dismissal and
/// for picking a palette color.
fn cluster_hash(token: &str, members: &[&str]) -> String {
    let mut sorted: Vec<&str> = members.to_vec();
    sorted.sort();
    sorted.dedup();
    let payload = format!("{}::{}", token, sorted.join("\u{1f}"));
    // FNV-1a 64-bit — no extra deps, deterministic across platforms.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in payload.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

fn title_case(token: &str) -> String {
    let mut c = token.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Run the heuristic suggester. Returns at most `MAX_SUGGESTIONS` candidates,
/// sorted by support descending, with dismissed clusters excluded.
pub fn suggest(db: &LibraryDb) -> Result<Vec<Suggestion>, LibraryError> {
    let recent = search_log::recent_queries(db, LOOKBACK)?;
    if recent.len() < MIN_LOG_SIZE {
        return Ok(Vec::new());
    }

    // Map: token -> list of queries that contain it (with that token as
    // their dominant token). For each query we attribute it to its single
    // highest-frequency token (computed against the whole log).
    let mut global_freq: HashMap<String, usize> = HashMap::new();
    let tokenized: Vec<(String, Vec<String>)> = recent
        .iter()
        .map(|r| {
            let toks = tokenize(&r.query);
            for t in &toks {
                *global_freq.entry(t.clone()).or_insert(0) += 1;
            }
            (r.query.clone(), toks)
        })
        .collect();

    // Group queries by their dominant token (one with highest global_freq).
    let mut clusters: HashMap<String, Vec<String>> = HashMap::new();
    for (q, toks) in &tokenized {
        if toks.is_empty() {
            continue;
        }
        let dominant = toks
            .iter()
            .max_by_key(|t| global_freq.get(*t).copied().unwrap_or(0))
            .cloned()
            .unwrap();
        clusters.entry(dominant).or_default().push(q.clone());
    }

    let mut out: Vec<Suggestion> = Vec::new();
    for (token, queries) in clusters {
        if queries.len() < MIN_CLUSTER_SUPPORT {
            continue;
        }
        let qrefs: Vec<&str> = queries.iter().map(|s| s.as_str()).collect();
        let hash = cluster_hash(&token, &qrefs);
        if search_log::is_dismissed(db, &hash)? {
            continue;
        }
        // Deterministic palette pick from the cluster hash.
        let color_idx = u64::from_str_radix(&hash[..8], 16).unwrap_or(0) as usize % PALETTE.len();
        out.push(Suggestion {
            name: title_case(&token),
            icon: icon_for_token(&token).to_string(),
            color: PALETTE[color_idx].to_string(),
            query_template: token.clone(),
            reason: format!("You searched '{}' {} times recently", token, queries.len()),
            cluster_hash: hash,
            support: queries.len(),
        });
    }

    out.sort_by(|a, b| b.support.cmp(&a.support).then(a.name.cmp(&b.name)));
    out.truncate(MAX_SUGGESTIONS);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> LibraryDb {
        LibraryDb::open_in_memory().unwrap()
    }

    #[test]
    fn empty_log_returns_no_suggestions() {
        let d = db();
        assert!(suggest(&d).unwrap().is_empty());
    }

    #[test]
    fn under_threshold_returns_no_suggestions() {
        let d = db();
        for i in 0..5 {
            search_log::record(&d, &format!("invoice {}", i), 0).unwrap();
        }
        assert!(suggest(&d).unwrap().is_empty());
    }

    #[test]
    fn clusters_invoice_queries() {
        let d = db();
        // 8 invoice queries + 4 noise (so log >= MIN_LOG_SIZE).
        for i in 0..8 {
            search_log::record(&d, &format!("invoice 2025-{:02}", i), 1).unwrap();
        }
        for w in ["alpha", "beta", "gamma", "delta"] {
            search_log::record(&d, w, 0).unwrap();
        }
        let s = suggest(&d).unwrap();
        assert!(!s.is_empty(), "expected at least one suggestion");
        let inv = s
            .iter()
            .find(|x| x.query_template == "invoice")
            .expect("invoice cluster");
        assert_eq!(inv.icon, "🧾");
        assert_eq!(inv.name, "Invoice");
        assert!(inv.support >= 8);
        assert!(inv.reason.contains("invoice"));
        assert!(inv.color.starts_with('#'));
    }

    #[test]
    fn ignores_stopwords() {
        let d = db();
        for _ in 0..12 {
            search_log::record(&d, "the and of", 0).unwrap();
        }
        assert!(suggest(&d).unwrap().is_empty());
    }

    #[test]
    fn dismissed_cluster_excluded() {
        let d = db();
        for i in 0..8 {
            search_log::record(&d, &format!("contract v{}", i), 1).unwrap();
        }
        for w in ["alpha", "beta", "gamma", "delta"] {
            search_log::record(&d, w, 0).unwrap();
        }
        let first = suggest(&d).unwrap();
        let contract = first
            .iter()
            .find(|x| x.query_template == "contract")
            .expect("contract cluster present initially");
        search_log::dismiss(&d, &contract.cluster_hash).unwrap();
        let after = suggest(&d).unwrap();
        assert!(after.iter().all(|x| x.query_template != "contract"));
    }

    #[test]
    fn max_three_suggestions_returned() {
        let d = db();
        // 5 distinct clusters each with 4 members.
        for token in ["invoice", "contract", "tax", "report", "patent"] {
            for i in 0..4 {
                search_log::record(&d, &format!("{} {}", token, i), 0).unwrap();
            }
        }
        let s = suggest(&d).unwrap();
        assert!(s.len() <= MAX_SUGGESTIONS);
    }

    #[test]
    fn cluster_hash_stable() {
        let h1 = cluster_hash("invoice", &["invoice 1", "invoice 2", "invoice 3"]);
        let h2 = cluster_hash("invoice", &["invoice 3", "invoice 1", "invoice 2"]);
        assert_eq!(h1, h2);
    }
}
