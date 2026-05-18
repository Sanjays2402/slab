// Sqlite-backed persistent store for Study Mode flashcards.
//
// One table `study_cards` keyed by `(pdf_hash, q_norm)` so re-running
// `generate_deck` on the same PDF doesn't duplicate. Schema versioned
// via `PRAGMA user_version`. In-memory open for tests.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::sm2::{schedule, CardState, Ease, DEFAULT_EF};
use super::study::Flashcard;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StudyError {
    #[error("sqlite: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("study: {0}")]
    Other(String),
}

/// One stored card. Front-end shape — `serde` to JSON for the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredCard {
    pub id: i64,
    pub pdf_hash: String,
    pub page: u32,
    pub q: String,
    pub a: String,
    pub ease_factor: f32,
    pub interval_days: u32,
    pub due_at: i64,
    pub last_seen_at: i64,
}

/// Session-wide review counts surfaced in the UI footer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StudyStats {
    pub total_cards: u32,
    pub due_now: u32,
    /// Reviews completed in the trailing 24 hours.
    pub reviewed_last_24h: u32,
}

/// Default DB path: `~/.slab/study.sqlite`.
pub fn default_db_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".slab").join("study.sqlite")
}

/// Owning handle. Open once per app run; tests use `open_in_memory`.
pub struct StudyStore {
    conn: Connection,
}

impl StudyStore {
    pub fn open(path: &Path) -> Result<Self, StudyError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self, StudyError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<(), StudyError> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS study_cards (
                    id INTEGER PRIMARY KEY,
                    pdf_hash TEXT NOT NULL,
                    page INTEGER NOT NULL,
                    q TEXT NOT NULL,
                    q_norm TEXT NOT NULL,
                    a TEXT NOT NULL,
                    ease_factor REAL NOT NULL,
                    interval_days INTEGER NOT NULL,
                    due_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL,
                    UNIQUE(pdf_hash, q_norm) ON CONFLICT IGNORE
                );
                CREATE INDEX IF NOT EXISTS idx_study_due ON study_cards(due_at);
                CREATE INDEX IF NOT EXISTS idx_study_pdf ON study_cards(pdf_hash);

                CREATE TABLE IF NOT EXISTS study_reviews (
                    id INTEGER PRIMARY KEY,
                    card_id INTEGER NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
                    reviewed_at INTEGER NOT NULL,
                    ease INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_review_time ON study_reviews(reviewed_at);
                "#,
            )?;
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<u32, StudyError> {
        let v: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(v)
    }

    /// Insert a freshly-generated deck for `pdf_hash`. Existing cards
    /// matching `(pdf_hash, q_norm)` are left untouched (ON CONFLICT
    /// IGNORE). Returns the number of *new* rows inserted.
    pub fn insert_deck(&mut self, pdf_hash: &str, cards: &[Flashcard]) -> Result<u32, StudyError> {
        let now = now_unix();
        let tx = self.conn.transaction()?;
        let mut inserted = 0u32;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO study_cards \
                 (pdf_hash, page, q, q_norm, a, ease_factor, interval_days, due_at, last_seen_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for c in cards {
                let n = stmt.execute(params![
                    pdf_hash,
                    c.page,
                    c.q,
                    normalise_q(&c.q),
                    c.a,
                    DEFAULT_EF as f64,
                    0_u32,
                    now,   // due immediately
                    0_i64, // never seen
                ])?;
                inserted += n as u32;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Cards whose `due_at <= now`, oldest first. Optionally scoped to one PDF.
    pub fn due_cards(
        &self,
        pdf_hash: Option<&str>,
        limit: u32,
    ) -> Result<Vec<StoredCard>, StudyError> {
        let now = now_unix();
        let map = |row: &rusqlite::Row| -> rusqlite::Result<StoredCard> {
            Ok(StoredCard {
                id: row.get(0)?,
                pdf_hash: row.get(1)?,
                page: row.get(2)?,
                q: row.get(3)?,
                a: row.get(4)?,
                ease_factor: row.get::<_, f64>(5)? as f32,
                interval_days: row.get(6)?,
                due_at: row.get(7)?,
                last_seen_at: row.get(8)?,
            })
        };
        let rows = if let Some(h) = pdf_hash {
            let mut stmt = self.conn.prepare(
                "SELECT id, pdf_hash, page, q, a, ease_factor, interval_days, due_at, last_seen_at \
                 FROM study_cards WHERE due_at <= ?1 AND pdf_hash = ?2 \
                 ORDER BY due_at ASC LIMIT ?3",
            )?;
            let collected: Vec<StoredCard> = stmt
                .query_map(params![now, h, limit], map)?
                .collect::<Result<Vec<_>, _>>()?;
            collected
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, pdf_hash, page, q, a, ease_factor, interval_days, due_at, last_seen_at \
                 FROM study_cards WHERE due_at <= ?1 \
                 ORDER BY due_at ASC LIMIT ?2",
            )?;
            let collected: Vec<StoredCard> = stmt
                .query_map(params![now, limit], map)?
                .collect::<Result<Vec<_>, _>>()?;
            collected
        };
        Ok(rows)
    }

    /// Record one review: apply SM-2-lite, write the new card state, log
    /// the review event. Returns the updated `StoredCard`.
    pub fn review(&mut self, card_id: i64, ease: Ease) -> Result<StoredCard, StudyError> {
        let now = now_unix();
        // Fetch current state.
        let prev: StoredCard = self
            .conn
            .query_row(
                "SELECT id, pdf_hash, page, q, a, ease_factor, interval_days, due_at, last_seen_at \
                 FROM study_cards WHERE id = ?1",
                params![card_id],
                |row| {
                    Ok(StoredCard {
                        id: row.get(0)?,
                        pdf_hash: row.get(1)?,
                        page: row.get(2)?,
                        q: row.get(3)?,
                        a: row.get(4)?,
                        ease_factor: row.get::<_, f64>(5)? as f32,
                        interval_days: row.get(6)?,
                        due_at: row.get(7)?,
                        last_seen_at: row.get(8)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StudyError::Other(format!("card {card_id} not found")))?;

        let next_state = schedule(
            CardState {
                ease_factor: prev.ease_factor,
                interval_days: prev.interval_days,
                due_at: prev.due_at,
                last_seen_at: prev.last_seen_at,
            },
            ease,
            now,
        );

        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE study_cards \
             SET ease_factor = ?1, interval_days = ?2, due_at = ?3, last_seen_at = ?4 \
             WHERE id = ?5",
            params![
                next_state.ease_factor as f64,
                next_state.interval_days,
                next_state.due_at,
                next_state.last_seen_at,
                card_id,
            ],
        )?;
        let ease_n: i64 = match ease {
            Ease::Again => 0,
            Ease::Hard => 1,
            Ease::Good => 2,
            Ease::Easy => 3,
        };
        tx.execute(
            "INSERT INTO study_reviews (card_id, reviewed_at, ease) VALUES (?1, ?2, ?3)",
            params![card_id, now, ease_n],
        )?;
        tx.commit()?;

        Ok(StoredCard {
            ease_factor: next_state.ease_factor,
            interval_days: next_state.interval_days,
            due_at: next_state.due_at,
            last_seen_at: next_state.last_seen_at,
            ..prev
        })
    }

    /// Footer stats. Cheap — one COUNT per metric.
    pub fn stats(&self, pdf_hash: Option<&str>) -> Result<StudyStats, StudyError> {
        let now = now_unix();
        let day_ago = now - 86_400;
        let (total, due_now) = if let Some(h) = pdf_hash {
            let t: u32 = self.conn.query_row(
                "SELECT COUNT(*) FROM study_cards WHERE pdf_hash = ?1",
                params![h],
                |r| r.get(0),
            )?;
            let d: u32 = self.conn.query_row(
                "SELECT COUNT(*) FROM study_cards WHERE pdf_hash = ?1 AND due_at <= ?2",
                params![h, now],
                |r| r.get(0),
            )?;
            (t, d)
        } else {
            let t: u32 = self
                .conn
                .query_row("SELECT COUNT(*) FROM study_cards", [], |r| r.get(0))?;
            let d: u32 = self.conn.query_row(
                "SELECT COUNT(*) FROM study_cards WHERE due_at <= ?1",
                params![now],
                |r| r.get(0),
            )?;
            (t, d)
        };
        let reviewed_last_24h: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_reviews WHERE reviewed_at >= ?1",
            params![day_ago],
            |r| r.get(0),
        )?;
        Ok(StudyStats {
            total_cards: total,
            due_now,
            reviewed_last_24h,
        })
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whitespace-collapsed lowercase question, for dedupe across re-generates.
fn normalise_q(q: &str) -> String {
    q.split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> StudyStore {
        StudyStore::open_in_memory().expect("open in-memory study store")
    }

    fn card(page: u32, q: &str, a: &str) -> Flashcard {
        Flashcard {
            page,
            q: q.into(),
            a: a.into(),
        }
    }

    #[test]
    fn schema_inits_to_version_one() {
        let s = store();
        assert_eq!(s.schema_version().unwrap(), 1);
    }

    #[test]
    fn insert_deck_dedupes_on_normalised_q() {
        let mut s = store();
        let n1 = s
            .insert_deck(
                "hash1",
                &[card(1, "What is X?", "Y"), card(1, "what  is  X?", "Z")],
            )
            .unwrap();
        // Whitespace + case collapse → both rows hash the same q_norm,
        // second is dropped.
        assert_eq!(n1, 1);
    }

    #[test]
    fn due_cards_returns_only_due_rows() {
        let mut s = store();
        let n = s
            .insert_deck("hash1", &[card(1, "Q1", "A1"), card(2, "Q2", "A2")])
            .unwrap();
        assert_eq!(n, 2);
        let due = s.due_cards(Some("hash1"), 50).unwrap();
        assert_eq!(due.len(), 2);
        // After Good review, card is no longer due now (interval=1 day).
        let first = due[0].clone();
        let updated = s.review(first.id, Ease::Good).unwrap();
        assert_eq!(updated.interval_days, 1);
        let due_after = s.due_cards(Some("hash1"), 50).unwrap();
        assert_eq!(due_after.len(), 1);
    }

    #[test]
    fn review_persists_state_and_logs_event() {
        let mut s = store();
        s.insert_deck("hash1", &[card(1, "Q1", "A1")]).unwrap();
        let due = s.due_cards(None, 5).unwrap();
        let id = due[0].id;
        let updated = s.review(id, Ease::Easy).unwrap();
        assert_eq!(updated.interval_days, 4);
        assert!(updated.ease_factor > DEFAULT_EF); // Easy bumps EF
        let stats = s.stats(None).unwrap();
        assert_eq!(stats.reviewed_last_24h, 1);
    }

    #[test]
    fn stats_count_total_due_and_reviews() {
        let mut s = store();
        s.insert_deck(
            "a",
            &[card(1, "Q1", "A"), card(2, "Q2", "A"), card(3, "Q3", "A")],
        )
        .unwrap();
        s.insert_deck("b", &[card(1, "Other", "A")]).unwrap();
        let all = s.stats(None).unwrap();
        assert_eq!(all.total_cards, 4);
        assert_eq!(all.due_now, 4);

        let just_a = s.stats(Some("a")).unwrap();
        assert_eq!(just_a.total_cards, 3);
        assert_eq!(just_a.due_now, 3);
    }

    #[test]
    fn review_unknown_card_errors() {
        let mut s = store();
        let err = s.review(9999, Ease::Good).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
