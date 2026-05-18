// JSON sidecar cache for Beacon Glossary reports.
//
// Why JSON not sqlite? Glossary entries are tiny (≤ 500 per doc).
// One read = one fs::read + serde_json. No migration story needed
// beyond a top-level `version` field. Mirrors how plugin manifests
// are persisted in `plugins::registry`.

use super::glossary::GlossaryReport;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 1;

/// Default cache dir: `~/.slab/glossary/`.
pub fn cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".slab").join("glossary")
}

/// Cache envelope written to disk.
#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    version: u32,
    report: GlossaryReport,
}

fn entry_path(dir: &Path, pdf_hash: &str) -> PathBuf {
    dir.join(format!("{}.json", pdf_hash))
}

/// Load the cached report for a `pdf_hash`, or `Ok(None)` if absent /
/// version-mismatched / malformed. Stale-version files are kept on disk
/// so a downgrade still finds them — only NEW reports overwrite.
pub fn load(pdf_hash: &str, dir: &Path) -> io::Result<Option<GlossaryReport>> {
    let path = entry_path(dir, pdf_hash);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)?;
    let env: Envelope = match serde_json::from_str(&raw) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    if env.version != CACHE_VERSION {
        return Ok(None);
    }
    Ok(Some(env.report))
}

/// Save (overwrite) the cached report for a `pdf_hash`.
pub fn save(pdf_hash: &str, report: &GlossaryReport, dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let env = Envelope {
        version: CACHE_VERSION,
        report: report.clone(),
    };
    let path = entry_path(dir, pdf_hash);
    let json = serde_json::to_string_pretty(&env)
        .map_err(|e| io::Error::other(format!("serialize: {e}")))?;
    fs::write(&path, json)
}

/// Remove the cached report for a `pdf_hash`. No-op if absent.
pub fn clear(pdf_hash: &str, dir: &Path) -> io::Result<()> {
    let path = entry_path(dir, pdf_hash);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::glossary::{CandidateKind, GlossaryEntry, GlossarySummary};
    use tempfile::tempdir;

    fn sample_report() -> GlossaryReport {
        GlossaryReport {
            entries: vec![GlossaryEntry {
                term: "RAG".into(),
                definition: "Retrieval-augmented generation.".into(),
                page: 3,
                confidence: 0.92,
                kind: CandidateKind::Acronym,
                source_snippet: "Using RAG, …".into(),
            }],
            summary: GlossarySummary {
                candidates_total: 4,
                accepted: 1,
                rejected: 3,
                kept_acronyms: 1,
                ..Default::default()
            },
            model: "ollama/llama3.1".into(),
        }
    }

    #[test]
    fn load_missing_returns_none() {
        let d = tempdir().unwrap();
        let got = load("deadbeef", d.path()).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let d = tempdir().unwrap();
        let r = sample_report();
        save("hashA", &r, d.path()).unwrap();
        let got = load("hashA", d.path()).unwrap().expect("must be cached");
        assert_eq!(got.entries.len(), 1);
        assert_eq!(got.entries[0].term, "RAG");
    }

    #[test]
    fn clear_removes_the_file() {
        let d = tempdir().unwrap();
        let r = sample_report();
        save("hashB", &r, d.path()).unwrap();
        clear("hashB", d.path()).unwrap();
        assert!(load("hashB", d.path()).unwrap().is_none());
    }

    #[test]
    fn version_mismatch_returns_none() {
        let d = tempdir().unwrap();
        let path = d.path().join("mismatch.json");
        std::fs::write(
            &path,
            r#"{"version":999,"report":{"entries":[],"summary":{"candidates_total":0,"accepted":0,"rejected":0,"kept_acronyms":0,"kept_defined_first_use":0,"kept_italicised":0,"kept_capitalised_phrase":0},"model":""}}"#,
        )
        .unwrap();
        assert!(load("mismatch", d.path()).unwrap().is_none());
    }

    #[test]
    fn malformed_json_returns_none() {
        let d = tempdir().unwrap();
        let path = d.path().join("bad.json");
        std::fs::write(&path, "not json at all").unwrap();
        assert!(load("bad", d.path()).unwrap().is_none());
    }
}
