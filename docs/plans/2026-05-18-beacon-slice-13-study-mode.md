# Beacon Bonus Slice 13 — "Study Mode" Implementation Plan

> **For Hermes:** This plan is executed in MODE C by the autonomous cron.
> Walk it task-by-task. After Task 8, run the batched quality gates
> (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --lib`,
> `pnpm check`), push the branch, and flip STATE.md to `STATUS: DONE`.

**Goal:** Turn any opened PDF into a deck of Q&A flashcards with SM-2-lite
spaced-repetition scheduling, persisted in a per-user sqlite DB at
`~/.slab/study.sqlite`, with a "✦ Study" panel that drives a daily review
session.

**Architecture:**
1. **`ai::study`** — pure pipeline: chunk → ask LLM for Q/A pairs → dedupe
   → validate → return `Vec<Flashcard>`. Mirrors the layering of
   `ai::outline` and `ai::citations` (system prompt + liberal JSON parser
   + validator + top-level async fn taking `Arc<dyn AiProvider>`).
2. **`ai::study_store`** — owning handle to `~/.slab/study.sqlite`.
   Schema-versioned via `PRAGMA user_version`. In-memory test harness.
   Mirrors `pdf::library::registry`.
3. **`ai::sm2`** — deterministic SM-2-lite math (no IO, no async).
   Unit-tested against fixture cards. Inputs: current card state + user
   ease rating. Outputs: new ease + due date.
4. **Tauri commands** in `src-tauri/src/lib.rs`:
   `slab_beacon_generate_deck`, `slab_beacon_study_due`,
   `slab_beacon_study_review`, `slab_beacon_study_stats`.
5. **`BeaconStudyPanel.svelte`** — card front/back flip, 4-button ease
   scale (again/hard/good/easy), session stats footer.
6. **Sidebar nav entry** `{ id: "study", label: "Study", icon: "🎓" }` in
   `src/routes/+page.svelte`, panel mounted alongside Citations.

**Tech Stack:** rusqlite 0.32 (already a dep), serde, thiserror, regex,
async-trait, Svelte 5 runes, `@tauri-apps/api/core`.

**Branch:** `feature/v1.7.0-beacon-bonus-13-study-mode`

**Pre-flight:**
```bash
cd /Users/sanjay/Projects/slab
git fetch origin && git checkout main && git pull --ff-only
git checkout -b feature/v1.7.0-beacon-bonus-13-study-mode
```

---

## Task 1: Scaffold `ai::study` module with `Flashcard` type + tests

**Objective:** Create the pure data types for the deck and a no-op
`generate_deck` shell that returns `Vec::new()` so subsequent tasks
have a real signature to fill in.

**Files:**
- Create: `src-tauri/src/ai/study.rs`
- Modify: `src-tauri/src/ai/mod.rs:17-30` (add `pub mod study;` line)

**Step 1: Add module declaration**

Edit `src-tauri/src/ai/mod.rs`, in the alphabetised `pub mod` block, add
`pub mod study;` between `pub mod summary;` and `pub mod vision;`:

```rust
pub mod auto_tag;
pub mod chat;
pub mod chunker;
pub mod citations;
pub mod config;
pub mod diff_summary;
pub mod embedding_index;
pub mod ollama;
pub mod openai_compat;
pub mod outline;
pub mod pii;
pub mod selection_action;
pub mod study;
pub mod summary;
pub mod vision;
```

**Step 2: Write the failing test FIRST**

Create `src-tauri/src/ai/study.rs` with ONLY the test module — no impl:

```rust
// Beacon Study Mode — Q&A flashcards with SM-2-lite spaced repetition.
//
// Pipeline (Slice 13 of Beacon Bonus):
//   1. Extract per-page text via `pdf::extract::extract_text`.
//   2. Chunk it with `ai::chunker::chunk_pages`.
//   3. For each chunk, ask the AiProvider to emit JSON `{cards:[{q,a}]}`
//      with a strict "answerable from THIS chunk alone" system prompt.
//   4. Validate (drop empty Q/A, drop too-long, dedupe by normalised Q).
//   5. Return `Vec<Flashcard>` tagged with origin `page`.
//   6. Caller stores them in `study_store` keyed by `pdf_hash`.
//
// Review side: `ai::sm2` schedules cards. `study_store` queries due cards.
// The frontend drives one card at a time, posting the user's ease rating
// back to `slab_beacon_study_review`.

#![allow(dead_code)] // populated in later tasks of this slice

use serde::{Deserialize, Serialize};

/// Hard cap on generated cards per `generate_deck` call. Defends the UI
/// (and the user's review backlog) from a runaway model. 600-page novels
/// can still exceed this — that's fine, user can re-run for "more".
pub const MAX_DECK_SIZE: usize = 200;

/// One flashcard. Pre-store shape: no DB id, no ease, no due date —
/// those land when `study_store::insert_deck` writes the row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Flashcard {
    /// 1-indexed source page (lets the UI "jump to source" on a card).
    pub page: u32,
    /// Question text. Trimmed, non-empty after validate.
    pub q: String,
    /// Answer text. Trimmed, non-empty after validate.
    pub a: String,
}

/// Stub — Task 4 fills it in. Exists now so the module compiles.
pub fn validate_cards(_raw: Vec<Flashcard>) -> Vec<Flashcard> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flashcard_is_serializable() {
        let c = Flashcard {
            page: 7,
            q: "What is the capital of France?".into(),
            a: "Paris.".into(),
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"page\":7"));
        assert!(j.contains("\"q\":\"What is"));
    }

    #[test]
    fn validate_stub_returns_empty() {
        // Will be replaced in Task 4 with real coverage.
        assert!(validate_cards(Vec::new()).is_empty());
    }
}
```

**Step 3: Run tests to verify compile + pass**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::study 2>&1 | tail -10
```
Expected: `2 passed; 0 failed`.

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/study): scaffold ai::study module + Flashcard type

Slice 13 of Beacon Bonus: Study Mode. This commit lands the empty
module wired into ai::mod plus the Flashcard data type and a
validate_cards stub. Subsequent commits in this slice fill in the
pipeline, the sqlite store, the SM-2 scheduler, the Tauri commands,
and the UI.
EOF
git add src-tauri/src/ai/mod.rs src-tauri/src/ai/study.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 2: SM-2-lite scheduler (`ai::sm2`) — deterministic math, fully unit-tested

**Objective:** Pure scheduling math. No IO, no async, no DB. Given a
card's current state + the user's ease rating, returns the next state.

**Files:**
- Create: `src-tauri/src/ai/sm2.rs`
- Modify: `src-tauri/src/ai/mod.rs` (add `pub mod sm2;` after `pub mod selection_action;`)

**Step 1: Add module declaration**

Edit `src-tauri/src/ai/mod.rs`, inserting `pub mod sm2;` immediately
after `pub mod selection_action;`:

```rust
pub mod selection_action;
pub mod sm2;
pub mod study;
```

**Step 2: Write the file with full impl + tests**

Create `src-tauri/src/ai/sm2.rs`:

```rust
// SM-2-lite scheduler for Study Mode flashcards.
//
// This is a deliberately *simplified* SuperMemo-2: we keep `ease_factor`
// (called "EF" in the literature) and an integer `interval_days` between
// reviews. The user's rating drives the update:
//
//   Again (0): forget the schedule — show again today. EF -= 0.20 (floor 1.30).
//   Hard  (1): interval ×= 1.2. EF -= 0.15.
//   Good  (2): interval ×= EF. EF unchanged.
//   Easy  (3): interval ×= EF × 1.3. EF += 0.15 (cap 3.00).
//
// Initial values: EF = 2.50, interval_days = 0 (new card → due "today").
// First review's interval becomes 1 day on Good, 4 days on Easy, 0 on
// Again, 1 on Hard.
//
// We intentionally drop the "repetition count" field from full SM-2 —
// the user-facing review feels the same and the math is easier to test.

use serde::{Deserialize, Serialize};

/// Default ease factor for a brand-new card.
pub const DEFAULT_EF: f32 = 2.50;
/// Hard floor on EF — below this and cards re-appear too often.
pub const MIN_EF: f32 = 1.30;
/// Soft ceiling on EF — above this and intervals explode.
pub const MAX_EF: f32 = 3.00;

/// The user's self-rating after seeing a card's answer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Ease {
    Again,
    Hard,
    Good,
    Easy,
}

/// Per-card persistent scheduler state. Stored in `study_cards` (see
/// `ai::study_store`). All fields are i64-friendly so sqlite can hold
/// them losslessly (EF is rounded to 4 decimals on write).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardState {
    pub ease_factor: f32,
    pub interval_days: u32,
    /// Unix seconds.
    pub due_at: i64,
    /// Unix seconds. Zero = never reviewed.
    pub last_seen_at: i64,
}

impl CardState {
    /// State of a brand-new card given `now` (unix seconds).
    pub fn new(now: i64) -> Self {
        Self {
            ease_factor: DEFAULT_EF,
            interval_days: 0,
            due_at: now,
            last_seen_at: 0,
        }
    }
}

const SECS_PER_DAY: i64 = 86_400;

/// Compute the next state after the user rates a card with `ease` at `now`.
/// Pure function — same inputs, same outputs.
pub fn schedule(prev: CardState, ease: Ease, now: i64) -> CardState {
    let (next_interval, ef_delta) = match ease {
        Ease::Again => (0_u32, -0.20),
        Ease::Hard => {
            let base = (prev.interval_days as f32) * 1.20;
            (base.max(1.0).round() as u32, -0.15)
        }
        Ease::Good => {
            // First-time review of a never-seen card: jump to 1 day.
            let base = if prev.interval_days == 0 {
                1.0
            } else {
                (prev.interval_days as f32) * prev.ease_factor
            };
            (base.round() as u32, 0.0)
        }
        Ease::Easy => {
            let base = if prev.interval_days == 0 {
                4.0
            } else {
                (prev.interval_days as f32) * prev.ease_factor * 1.30
            };
            (base.round() as u32, 0.15)
        }
    };

    let next_ef = (prev.ease_factor + ef_delta).clamp(MIN_EF, MAX_EF);
    let due_at = now + (next_interval as i64) * SECS_PER_DAY;
    CardState {
        ease_factor: next_ef,
        interval_days: next_interval,
        due_at,
        last_seen_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000; // arbitrary anchor for tests

    fn new_card() -> CardState {
        CardState::new(T0)
    }

    #[test]
    fn new_card_is_due_now_with_default_ef() {
        let c = new_card();
        assert_eq!(c.interval_days, 0);
        assert_eq!(c.due_at, T0);
        assert!((c.ease_factor - DEFAULT_EF).abs() < f32::EPSILON);
        assert_eq!(c.last_seen_at, 0);
    }

    #[test]
    fn good_on_first_review_schedules_one_day() {
        let next = schedule(new_card(), Ease::Good, T0);
        assert_eq!(next.interval_days, 1);
        assert_eq!(next.due_at, T0 + SECS_PER_DAY);
        // EF unchanged on Good.
        assert!((next.ease_factor - DEFAULT_EF).abs() < f32::EPSILON);
    }

    #[test]
    fn easy_on_first_review_schedules_four_days() {
        let next = schedule(new_card(), Ease::Easy, T0);
        assert_eq!(next.interval_days, 4);
        assert!((next.ease_factor - 2.65).abs() < 0.01);
    }

    #[test]
    fn again_resets_interval_and_drops_ef() {
        let mut c = new_card();
        c.interval_days = 12;
        c.ease_factor = 2.50;
        let next = schedule(c, Ease::Again, T0);
        assert_eq!(next.interval_days, 0);
        assert_eq!(next.due_at, T0);
        assert!((next.ease_factor - 2.30).abs() < 0.01);
    }

    #[test]
    fn ef_floor_holds_at_1_30() {
        let mut c = new_card();
        c.ease_factor = 1.35;
        let next = schedule(c, Ease::Again, T0);
        assert!((next.ease_factor - MIN_EF).abs() < 0.01);
    }

    #[test]
    fn ef_cap_holds_at_3_00() {
        let mut c = new_card();
        c.ease_factor = 2.95;
        let next = schedule(c, Ease::Easy, T0);
        assert!((next.ease_factor - MAX_EF).abs() < 0.01);
    }

    #[test]
    fn good_on_mature_card_uses_ef_for_interval() {
        let mut c = new_card();
        c.interval_days = 10;
        c.ease_factor = 2.50;
        let next = schedule(c, Ease::Good, T0);
        // 10 * 2.50 = 25, rounded
        assert_eq!(next.interval_days, 25);
    }

    #[test]
    fn hard_on_new_card_falls_back_to_one_day_floor() {
        // interval was 0 → base = 0, floor pulls it up to 1.
        let next = schedule(new_card(), Ease::Hard, T0);
        assert_eq!(next.interval_days, 1);
        assert!((next.ease_factor - 2.35).abs() < 0.01);
    }
}
```

**Step 3: Verify**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::sm2 2>&1 | tail -12
```
Expected: `8 passed; 0 failed`.

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/sm2): SM-2-lite spaced-repetition scheduler

Pure deterministic math: schedule(prev, ease, now) -> next CardState.
Ease ratings Again/Hard/Good/Easy adjust EF (clamped to 1.30..3.00)
and interval_days. New cards default EF=2.50, interval=0. First-time
Good → 1 day, first-time Easy → 4 days.

No IO, no async. 8 unit tests covering the ease ladder, EF floor +
cap, mature-card behaviour, and reset-on-Again.
EOF
git add src-tauri/src/ai/mod.rs src-tauri/src/ai/sm2.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 3: `ai::study_store` — sqlite store with schema + CRUD + tests

**Objective:** Owning handle to `~/.slab/study.sqlite`. Schema-versioned.
In-memory test harness. Modelled on `pdf::library::registry`.

**Files:**
- Create: `src-tauri/src/ai/study_store.rs`
- Modify: `src-tauri/src/ai/mod.rs` (add `pub mod study_store;`)

**Step 1: Register the module**

In `src-tauri/src/ai/mod.rs`, insert `pub mod study_store;` after
`pub mod study;`:

```rust
pub mod study;
pub mod study_store;
pub mod summary;
```

**Step 2: Write `study_store.rs`**

Create `src-tauri/src/ai/study_store.rs`:

```rust
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
    pub fn insert_deck(
        &mut self,
        pdf_hash: &str,
        cards: &[Flashcard],
    ) -> Result<u32, StudyError> {
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
                    now,    // due immediately
                    0_i64,  // never seen
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
        let (sql, _params): (&str, ()) = if pdf_hash.is_some() {
            (
                "SELECT id, pdf_hash, page, q, a, ease_factor, interval_days, due_at, last_seen_at \
                 FROM study_cards WHERE due_at <= ?1 AND pdf_hash = ?2 \
                 ORDER BY due_at ASC LIMIT ?3",
                (),
            )
        } else {
            (
                "SELECT id, pdf_hash, page, q, a, ease_factor, interval_days, due_at, last_seen_at \
                 FROM study_cards WHERE due_at <= ?1 \
                 ORDER BY due_at ASC LIMIT ?2",
                (),
            )
        };
        let mut stmt = self.conn.prepare(sql)?;
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
            stmt.query_map(params![now, h, limit], map)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![now, limit], map)?
                .collect::<Result<Vec<_>, _>>()?
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
            .insert_deck("hash1", &[card(1, "What is X?", "Y"), card(1, "what  is  X?", "Z")])
            .unwrap();
        // Whitespace + case collapse → both rows hash the same q_norm,
        // second is dropped.
        assert_eq!(n1, 1);
    }

    #[test]
    fn due_cards_returns_only_due_rows() {
        let mut s = store();
        let n = s.insert_deck("hash1", &[card(1, "Q1", "A1"), card(2, "Q2", "A2")]).unwrap();
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
        s.insert_deck("a", &[card(1, "Q1", "A"), card(2, "Q2", "A"), card(3, "Q3", "A")])
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
```

**Step 3: Verify**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::study_store 2>&1 | tail -12
```
Expected: `5 passed; 0 failed`.

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/study-store): sqlite store for flashcards + review log

~/.slab/study.sqlite, schema-versioned via PRAGMA user_version. Two
tables: study_cards (UNIQUE(pdf_hash, q_norm) ON CONFLICT IGNORE so
re-generating a deck is idempotent) and study_reviews (event log for
24h-rolling stats).

insert_deck / due_cards / review / stats. Tests use open_in_memory
and cover dedupe, due windowing, review state update, stats counters,
unknown-card error.

Modelled on pdf::library::registry (same idempotent-migrations
pattern, same now_unix helper).
EOF
git add src-tauri/src/ai/mod.rs src-tauri/src/ai/study_store.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 4: Fill in `ai::study` — prompt, parser, validator, `generate_deck`

**Objective:** Replace the stub in `ai::study` with the real pipeline.
Liberal JSON parser (same shape as `outline::parse_llm_outline`),
`validate_cards` enforces trim + length + per-page dedupe,
`generate_deck` walks the chunker, calls the provider per page, validates,
caps at `MAX_DECK_SIZE`.

**Files:**
- Modify: `src-tauri/src/ai/study.rs` (replace entire body)

**Step 1: Write the failing test FIRST** (TDD)

Before touching the impl, add these to the bottom of the existing
`#[cfg(test)] mod tests` block in `study.rs`:

```rust
    // Added Task 4 — drive the validator + parser impl.

    #[test]
    fn parses_plain_cards_json() {
        let raw = r#"{"cards":[{"q":"What is X?","a":"Y"}]}"#;
        let w = parse_llm_cards(raw).expect("should parse");
        assert_eq!(w.cards.len(), 1);
        assert_eq!(w.cards[0].q.as_deref(), Some("What is X?"));
    }

    #[test]
    fn parses_cards_with_fence_and_chatter() {
        let raw = "Sure thing! ```json\n{\"cards\":[{\"q\":\"Q\",\"a\":\"A\"}]}\n```\nhope that helps";
        let w = parse_llm_cards(raw).expect("should parse");
        assert_eq!(w.cards.len(), 1);
    }

    #[test]
    fn parse_returns_none_on_garbage() {
        assert!(parse_llm_cards("not json").is_none());
        assert!(parse_llm_cards("").is_none());
    }

    #[test]
    fn validate_drops_empty_qa() {
        let raw = vec![
            Flashcard { page: 1, q: "  ".into(), a: "A".into() },
            Flashcard { page: 1, q: "Q".into(), a: " ".into() },
            Flashcard { page: 1, q: "Real?".into(), a: "Yes.".into() },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].q, "Real?");
    }

    #[test]
    fn validate_dedupes_by_normalised_q() {
        let raw = vec![
            Flashcard { page: 1, q: "What  is  X?".into(), a: "Y".into() },
            Flashcard { page: 1, q: "WHAT IS X?".into(), a: "Z".into() },
            Flashcard { page: 2, q: "Something else?".into(), a: "Sure".into() },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 2);
        // First occurrence wins.
        assert_eq!(v[0].a, "Y");
    }

    #[test]
    fn validate_caps_at_max_deck_size() {
        let raw: Vec<Flashcard> = (0..MAX_DECK_SIZE + 30)
            .map(|i| Flashcard { page: 1, q: format!("Q{i}"), a: "A".into() })
            .collect();
        let v = validate_cards(raw);
        assert_eq!(v.len(), MAX_DECK_SIZE);
    }
```

**Step 2: Replace the body of `study.rs` with the real impl**

Replace the entire contents of `src-tauri/src/ai/study.rs` with:

```rust
// Beacon Study Mode — Q&A flashcards with SM-2-lite spaced repetition.
//
// Pipeline (Slice 13 of Beacon Bonus):
//   1. Extract per-page text via `pdf::extract::extract_text`.
//   2. Chunk it with `ai::chunker::chunk_pages`.
//   3. For each chunk, ask the AiProvider to emit JSON `{cards:[{q,a}]}`
//      with a strict "answerable from THIS chunk alone" system prompt.
//   4. Validate (drop empty Q/A, drop too-long, dedupe by normalised Q).
//   5. Return `Vec<Flashcard>` tagged with origin `page`. Caller stores
//      them in `study_store` keyed by `pdf_hash`.
//
// Review side: `ai::sm2` schedules cards. `study_store` queries due cards.
// The frontend drives one card at a time, posting the user's ease rating
// back to `slab_beacon_study_review`.

use super::chunker::chunk_pages;
use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Hard cap on generated cards per `generate_deck` call.
pub const MAX_DECK_SIZE: usize = 200;

/// Per-Q/A length sanity check. Anything over this is almost always the
/// model misbehaving (rambling answer, multi-paragraph Q).
pub const MAX_FIELD_CHARS: usize = 1_000;

/// Default per-chunk card count we ask the model for. Tunable from the UI.
pub const DEFAULT_CARDS_PER_CHUNK: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Flashcard {
    pub page: u32,
    pub q: String,
    pub a: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckReport {
    pub cards: Vec<Flashcard>,
    /// Model identifier returned by the last provider call.
    pub model: String,
    /// How many chunks we asked the provider about.
    pub chunks_processed: u32,
    /// Cards dropped by validation (empty/too-long/dupes).
    pub dropped: u32,
}

/// Knobs for `generate_deck`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckOpts {
    /// How many cards to ask for per chunk. Clamped 1..=10.
    #[serde(default = "default_cpc")]
    pub cards_per_chunk: u32,
    /// Hard ceiling on total cards. Clamped to MAX_DECK_SIZE.
    #[serde(default = "default_max")]
    pub max_cards: u32,
}

fn default_cpc() -> u32 {
    DEFAULT_CARDS_PER_CHUNK
}
fn default_max() -> u32 {
    MAX_DECK_SIZE as u32
}

impl Default for DeckOpts {
    fn default() -> Self {
        Self {
            cards_per_chunk: DEFAULT_CARDS_PER_CHUNK,
            max_cards: MAX_DECK_SIZE as u32,
        }
    }
}

// ---------------------------------------------------------------
// LLM wire shape — liberal: every field optional.
// ---------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmCardsWire {
    #[serde(default)]
    pub(super) cards: Vec<LlmCardWire>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct LlmCardWire {
    #[serde(default)]
    pub(super) q: Option<String>,
    #[serde(default)]
    pub(super) a: Option<String>,
}

/// Liberal JSON parser — same pattern as `outline::parse_llm_outline`.
pub(super) fn parse_llm_cards(raw: &str) -> Option<LlmCardsWire> {
    let s = raw.trim();
    let body = if let Some(rest) = s.strip_prefix("```json") {
        rest.trim_end_matches("```").trim()
    } else if let Some(rest) = s.strip_prefix("```") {
        rest.trim_end_matches("```").trim()
    } else {
        s
    };
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&body[start..=end]).ok()
}

/// Whitespace-collapsed lowercase Q for dedupe. Local copy of the store's
/// normaliser to avoid coupling this module to the store at the type level.
fn normalise_q(q: &str) -> String {
    q.split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Validate + dedupe + cap. Pure — no IO.
pub fn validate_cards(raw: Vec<Flashcard>) -> Vec<Flashcard> {
    let mut out: Vec<Flashcard> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in raw {
        let q = c.q.trim().to_string();
        let a = c.a.trim().to_string();
        if q.is_empty() || a.is_empty() {
            continue;
        }
        if q.len() > MAX_FIELD_CHARS || a.len() > MAX_FIELD_CHARS {
            continue;
        }
        let n = normalise_q(&q);
        if !seen.insert(n) {
            continue;
        }
        out.push(Flashcard { page: c.page, q, a });
        if out.len() >= MAX_DECK_SIZE {
            break;
        }
    }
    out
}

const SYSTEM_PROMPT: &str = "You are Beacon, a study-buddy. Given a chunk of \
PDF text, write Q&A flashcards that test the reader's recall of the chunk's \
content. Reply with JSON ONLY, no prose, no markdown fences, in this exact \
shape:\n\
{\"cards\":[{\"q\":\"...\",\"a\":\"...\"}]}\n\
- q: a short, specific question. Answerable from the chunk ALONE.\n\
- a: a 1-2 sentence answer. Direct, no hedging.\n\
- Skip headings, page numbers, table-of-contents lines.\n\
- If the chunk has nothing testworthy, return {\"cards\":[]}. Don't invent.";

fn build_messages(chunk_text: &str, cards_per_chunk: u32) -> Vec<ChatMessage> {
    let cards_per_chunk = cards_per_chunk.clamp(1, 10);
    let user = format!(
        "Write up to {cards_per_chunk} flashcards from this chunk. JSON only.\n\
         \n\
         CHUNK:\n{chunk_text}"
    );
    vec![
        ChatMessage {
            role: ChatRole::System,
            content: SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: user,
        },
    ]
}

/// Top-level pipeline. Pre-extracted pages so it's easy to unit-test
/// against fixture strings (the real Tauri command calls
/// `generate_deck_from_path` below).
pub async fn generate_deck(
    provider: Arc<dyn AiProvider>,
    pages: &[String],
    opts: &DeckOpts,
) -> Result<DeckReport, AiError> {
    let chunks = chunk_pages(pages);
    let cap = (opts.max_cards as usize).min(MAX_DECK_SIZE);
    let cards_per_chunk = opts.cards_per_chunk.clamp(1, 10);

    let mut all: Vec<Flashcard> = Vec::new();
    let mut model = String::new();
    let mut chunks_processed = 0u32;

    for chunk in chunks.iter() {
        if all.len() >= cap {
            break;
        }
        let msgs = build_messages(&chunk.text, cards_per_chunk);
        let chat_opts = ChatOpts {
            temperature: Some(0.3),
            max_tokens: Some(800),
            ..Default::default()
        };
        let resp = provider.chat(&msgs, &chat_opts).await?;
        model = resp.model;
        chunks_processed += 1;
        let wire = parse_llm_cards(&resp.content).unwrap_or(LlmCardsWire { cards: Vec::new() });
        for raw in wire.cards {
            let q = raw.q.unwrap_or_default();
            let a = raw.a.unwrap_or_default();
            all.push(Flashcard {
                page: chunk.page,
                q,
                a,
            });
        }
    }

    let before = all.len();
    let cleaned = validate_cards(all);
    let dropped = (before.saturating_sub(cleaned.len())) as u32;

    Ok(DeckReport {
        cards: cleaned,
        model,
        chunks_processed,
        dropped,
    })
}

/// Convenience: read PDF text from disk, then generate.
pub async fn generate_deck_from_path(
    provider: Arc<dyn AiProvider>,
    pdf_path: &Path,
    opts: &DeckOpts,
) -> Result<DeckReport, AiError> {
    let pages = crate::pdf::extract::extract_text(pdf_path)
        .map_err(|e| AiError::InvalidResponse(format!("reading {}: {e}", pdf_path.display())))?;
    generate_deck(provider, &pages, opts).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flashcard_is_serializable() {
        let c = Flashcard {
            page: 7,
            q: "What is the capital of France?".into(),
            a: "Paris.".into(),
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"page\":7"));
        assert!(j.contains("\"q\":\"What is"));
    }

    #[test]
    fn opts_default_is_sensible() {
        let o = DeckOpts::default();
        assert_eq!(o.cards_per_chunk, DEFAULT_CARDS_PER_CHUNK);
        assert_eq!(o.max_cards, MAX_DECK_SIZE as u32);
    }

    #[test]
    fn parses_plain_cards_json() {
        let raw = r#"{"cards":[{"q":"What is X?","a":"Y"}]}"#;
        let w = parse_llm_cards(raw).expect("should parse");
        assert_eq!(w.cards.len(), 1);
        assert_eq!(w.cards[0].q.as_deref(), Some("What is X?"));
    }

    #[test]
    fn parses_cards_with_fence_and_chatter() {
        let raw = "Sure thing! ```json\n{\"cards\":[{\"q\":\"Q\",\"a\":\"A\"}]}\n```\nhope that helps";
        let w = parse_llm_cards(raw).expect("should parse");
        assert_eq!(w.cards.len(), 1);
    }

    #[test]
    fn parse_returns_none_on_garbage() {
        assert!(parse_llm_cards("not json").is_none());
        assert!(parse_llm_cards("").is_none());
    }

    #[test]
    fn validate_drops_empty_qa() {
        let raw = vec![
            Flashcard { page: 1, q: "  ".into(), a: "A".into() },
            Flashcard { page: 1, q: "Q".into(), a: " ".into() },
            Flashcard { page: 1, q: "Real?".into(), a: "Yes.".into() },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].q, "Real?");
    }

    #[test]
    fn validate_dedupes_by_normalised_q() {
        let raw = vec![
            Flashcard { page: 1, q: "What  is  X?".into(), a: "Y".into() },
            Flashcard { page: 1, q: "WHAT IS X?".into(), a: "Z".into() },
            Flashcard { page: 2, q: "Something else?".into(), a: "Sure".into() },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 2);
        // First occurrence wins.
        assert_eq!(v[0].a, "Y");
    }

    #[test]
    fn validate_caps_at_max_deck_size() {
        let raw: Vec<Flashcard> = (0..MAX_DECK_SIZE + 30)
            .map(|i| Flashcard { page: 1, q: format!("Q{i}"), a: "A".into() })
            .collect();
        let v = validate_cards(raw);
        assert_eq!(v.len(), MAX_DECK_SIZE);
    }

    #[test]
    fn validate_drops_oversized_fields() {
        let huge = "x".repeat(MAX_FIELD_CHARS + 1);
        let raw = vec![
            Flashcard { page: 1, q: huge.clone(), a: "A".into() },
            Flashcard { page: 1, q: "Short?".into(), a: huge },
            Flashcard { page: 1, q: "OK?".into(), a: "Yes".into() },
        ];
        let v = validate_cards(raw);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].q, "OK?");
    }
}
```

**Step 3: Verify all study tests pass**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo test --lib ai::study:: 2>&1 | tail -14
```
Expected: `9 passed; 0 failed`.

**Step 4: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/study): card generation pipeline + validator

ai::study::generate_deck walks chunk_pages, asks the configured
AiProvider per chunk for {cards:[{q,a}]} JSON, validates (trim,
length cap, normalised-Q dedupe), and returns a DeckReport.

System prompt enforces "answerable from THIS chunk alone, no
invention". Temperature 0.3, max_tokens 800 per chunk. JSON parser
liberal — strips fences, trims chatter, returns None on garbage.

9 tests including the wire-parser happy path + garbage + dedupe +
oversized-field drop + MAX_DECK_SIZE cap.
EOF
git add src-tauri/src/ai/study.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 5: Wire Tauri commands in `src-tauri/src/lib.rs`

**Objective:** Four new commands —
`slab_beacon_generate_deck`, `slab_beacon_study_due`,
`slab_beacon_study_review`, `slab_beacon_study_stats` — and register them
in the `invoke_handler!` block.

**Files:**
- Modify: `src-tauri/src/lib.rs` (imports + four `#[tauri::command]` fns + invoke_handler list)

**Step 1: Add imports**

Open `src-tauri/src/lib.rs`. The existing imports look like:

```rust
use ai::citations::{
    find_citations_from_path as do_beacon_find_citations, CitationOpts, CitationReport,
};
```

Just below the `ai::pii::` block (around line 33), add:

```rust
use ai::sm2::Ease;
use ai::study::{
    generate_deck_from_path as do_beacon_generate_deck, DeckOpts, DeckReport,
};
use ai::study_store::{default_db_path as default_study_db_path, StoredCard, StudyError, StudyStats, StudyStore};
```

**Step 2: Compute a stable PDF hash helper**

We need a deterministic `pdf_hash` to scope cards per file. Reuse the
sha256 helper used by `embedding_index`. Search for the existing helper:

```bash
grep -n "fn hash_file_sha256\|fn pdf_hash" src-tauri/src/ai/embedding_index.rs | head -5
```

If a `pub fn hash_file` (or similarly named) is exported, import it. If
not, add a small private helper at the bottom of `lib.rs`:

```rust
fn hash_pdf_path(p: &std::path::Path) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(p)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
```

(If the embedding index already exports one, use it instead and skip this helper.)

**Step 3: Add the four commands**

Append (just below the `slab_beacon_find_citations` command at ~line 816):

```rust
/// Beacon Study — generate a deck of Q&A flashcards from a PDF and
/// persist them in `~/.slab/study.sqlite` (UNIQUE(pdf_hash, q_norm)
/// dedupes across re-runs). Returns the freshly-generated deck (NOT
/// the full stored deck — UI uses `slab_beacon_study_due` next).
/// v1.7.0 Beacon Bonus Slice 13.
#[tauri::command]
async fn slab_beacon_generate_deck(
    pdf_path: PathBuf,
    opts: Option<DeckOpts>,
) -> CmdResult<DeckReport> {
    let cfg = match do_load_beacon_config() {
        Ok(c) => c,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let provider = match ai::config::make_provider(&cfg.beacon) {
        Ok(p) => p,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let opts = opts.unwrap_or_default();
    let report = match do_beacon_generate_deck(provider, &pdf_path, &opts).await {
        Ok(r) => r,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    // Persist (best-effort: we still return the report even if the store fails).
    let hash = match hash_pdf_path(&pdf_path) {
        Ok(h) => h,
        Err(e) => {
            return CmdResult::Err {
                message: format!("hashing pdf: {e}"),
            }
        }
    };
    match StudyStore::open(&default_study_db_path()) {
        Ok(mut store) => {
            let _ = store.insert_deck(&hash, &report.cards);
        }
        Err(e) => {
            return CmdResult::Err {
                message: format!("opening study store: {e}"),
            }
        }
    }
    CmdResult::Ok { data: report }
}

/// Beacon Study — fetch cards due for review. If `pdf_path` is given,
/// scope to that PDF; otherwise return cross-document due cards.
#[tauri::command]
async fn slab_beacon_study_due(
    pdf_path: Option<PathBuf>,
    limit: Option<u32>,
) -> CmdResult<Vec<StoredCard>> {
    let store = match StudyStore::open(&default_study_db_path()) {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let hash_owned: Option<String> = match pdf_path.as_ref() {
        Some(p) => match hash_pdf_path(p) {
            Ok(h) => Some(h),
            Err(e) => {
                return CmdResult::Err {
                    message: format!("hashing pdf: {e}"),
                }
            }
        },
        None => None,
    };
    let res = store.due_cards(hash_owned.as_deref(), limit.unwrap_or(50));
    res.map(|cards| CmdResult::Ok { data: cards })
        .unwrap_or_else(|e: StudyError| CmdResult::Err {
            message: e.to_string(),
        })
}

/// Beacon Study — record a review and return the updated card.
#[tauri::command]
async fn slab_beacon_study_review(card_id: i64, ease: Ease) -> CmdResult<StoredCard> {
    let mut store = match StudyStore::open(&default_study_db_path()) {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    store
        .review(card_id, ease)
        .map(|c| CmdResult::Ok { data: c })
        .unwrap_or_else(|e: StudyError| CmdResult::Err {
            message: e.to_string(),
        })
}

/// Beacon Study — counters for the footer.
#[tauri::command]
async fn slab_beacon_study_stats(pdf_path: Option<PathBuf>) -> CmdResult<StudyStats> {
    let store = match StudyStore::open(&default_study_db_path()) {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let hash_owned: Option<String> = match pdf_path.as_ref() {
        Some(p) => match hash_pdf_path(p) {
            Ok(h) => Some(h),
            Err(e) => {
                return CmdResult::Err {
                    message: format!("hashing pdf: {e}"),
                }
            }
        },
        None => None,
    };
    store
        .stats(hash_owned.as_deref())
        .map(|s| CmdResult::Ok { data: s })
        .unwrap_or_else(|e: StudyError| CmdResult::Err {
            message: e.to_string(),
        })
}
```

**Step 4: Register in `invoke_handler!`**

Find the existing block around line 1993 with `slab_beacon_find_citations,`
and append four new lines below it:

```rust
            slab_beacon_find_citations,
            slab_beacon_generate_deck,
            slab_beacon_study_due,
            slab_beacon_study_review,
            slab_beacon_study_stats,
            slab_beacon_diff_summary,
```

**Step 5: Compile + smoke-test**

```bash
cd /Users/sanjay/Projects/slab/src-tauri
cargo build --lib 2>&1 | tail -20
```
Expected: clean build, no warnings beyond pre-existing ones.

```bash
cargo test --lib ai:: 2>&1 | tail -10
```
Expected: all ai:: tests still pass.

**Step 6: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/study): expose 4 Tauri commands for Study Mode

- slab_beacon_generate_deck: generate + persist cards keyed by SHA-256
  of the PDF.
- slab_beacon_study_due: list cards due now (optional pdf scope).
- slab_beacon_study_review: record a review, return updated card.
- slab_beacon_study_stats: total / due_now / reviewed_last_24h.

All commands open a fresh StudyStore against ~/.slab/study.sqlite per
call — the schema-versioned migration is idempotent so this is cheap
and avoids holding a global handle in app state for this slice.

Registered in invoke_handler! alongside the existing Beacon commands.
EOF
git add src-tauri/src/lib.rs
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 6: `BeaconStudyPanel.svelte` — UI for review session

**Objective:** Svelte 5 panel: pick PDF → "Generate deck" → render the
review one card at a time (front: question; click → reveal answer;
choose ease). Footer shows session stats. Mirrors
`BeaconCitationsPanel.svelte` styling.

**Files:**
- Create: `src/lib/panels/BeaconStudyPanel.svelte`

**Step 1: Create the file**

Create `src/lib/panels/BeaconStudyPanel.svelte` with the following:

```svelte
<script lang="ts">
  // Beacon Study panel — turn the current PDF into a deck of Q&A
  // flashcards and drive a spaced-repetition review session.
  //
  // Workflow:
  //   1. Pick (or inherit from slab:open-recent) a PDF.
  //   2. "Generate deck" → slab_beacon_generate_deck. New cards land in
  //      ~/.slab/study.sqlite, dedupe-on-conflict.
  //   3. "Start review" → slab_beacon_study_due fetches due cards.
  //   4. Render one card. Click "Reveal" to flip. Pick ease →
  //      slab_beacon_study_review records + advances.
  //   5. Footer: stats from slab_beacon_study_stats.
  //
  // Errors map through the same friendly toast pattern used by
  // BeaconChatPanel / BeaconCitationsPanel.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, type CmdResult, type Status } from "$lib/types";

  type Flashcard = { page: number; q: string; a: string };
  type DeckReport = {
    cards: Flashcard[];
    model: string;
    chunks_processed: number;
    dropped: number;
  };
  type StoredCard = {
    id: number;
    pdf_hash: string;
    page: number;
    q: string;
    a: string;
    ease_factor: number;
    interval_days: number;
    due_at: number;
    last_seen_at: number;
  };
  type StudyStats = {
    total_cards: number;
    due_now: number;
    reviewed_last_24h: number;
  };
  type Ease = "again" | "hard" | "good" | "easy";

  let pdfPath = $state<string | null>(null);
  let queue = $state<StoredCard[]>([]);
  let current = $state<StoredCard | null>(null);
  let revealed = $state(false);
  let stats = $state<StudyStats | null>(null);
  let status = $state<Status>(idle);
  let cardsPerChunk = $state(3);

  onMount(() => {
    const onOpenRecent = (e: Event) => {
      const d = (e as CustomEvent).detail as { path: string } | undefined;
      if (d?.path) {
        pdfPath = d.path;
        queue = [];
        current = null;
        revealed = false;
      }
    };
    window.addEventListener("slab:open-recent", onOpenRecent);
    void refreshStats();
    return () => window.removeEventListener("slab:open-recent", onOpenRecent);
  });

  async function pickPdf() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    pdfPath = picked;
    queue = [];
    current = null;
    revealed = false;
    status = idle;
    await refreshStats();
  }

  async function refreshStats() {
    try {
      const res = await invoke<CmdResult<StudyStats>>("slab_beacon_study_stats", {
        pdfPath,
      });
      if ("data" in res) stats = res.data;
    } catch {
      /* swallow — stats are non-essential */
    }
  }

  async function generate() {
    if (!pdfPath) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    status = { kind: "working", msg: "Generating flashcards…" };
    try {
      const res = await invoke<CmdResult<DeckReport>>("slab_beacon_generate_deck", {
        pdfPath,
        opts: { cards_per_chunk: cardsPerChunk, max_cards: 200 },
      });
      if ("data" in res) {
        const r = res.data;
        status = {
          kind: "ok",
          msg: `Generated ${r.cards.length} new cards (${r.dropped} dropped) from ${r.chunks_processed} chunks · model ${r.model || "(local)"}`,
        };
        await refreshStats();
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function startReview() {
    status = { kind: "working", msg: "Loading due cards…" };
    try {
      const res = await invoke<CmdResult<StoredCard[]>>("slab_beacon_study_due", {
        pdfPath,
        limit: 50,
      });
      if ("data" in res) {
        queue = res.data;
        current = queue.shift() ?? null;
        revealed = false;
        status = current
          ? { kind: "ok", msg: `${queue.length + 1} card(s) queued.` }
          : { kind: "ok", msg: "Nothing due right now — come back later 🎉" };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function rate(ease: Ease) {
    if (!current) return;
    try {
      await invoke<CmdResult<StoredCard>>("slab_beacon_study_review", {
        cardId: current.id,
        ease,
      });
      current = queue.shift() ?? null;
      revealed = false;
      await refreshStats();
      if (!current) {
        status = { kind: "ok", msg: "Session complete 🎓" };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function jumpToPage() {
    if (!current) return;
    window.dispatchEvent(
      new CustomEvent("slab:beacon-goto-page", {
        detail: { page: current.page, path: pdfPath },
      }),
    );
  }
</script>

<section class="panel">
  <header>
    <h2>🎓 Study Mode</h2>
    <p class="subtitle">
      Beacon turns your PDF into Q&A flashcards. Spaced-repetition
      scheduling (SM-2 lite) decides what to show you next.
    </p>
  </header>

  <div class="picker">
    <button class="btn" onclick={pickPdf}>Pick PDF…</button>
    <span class="path">{pdfPath ? basename(pdfPath) : "no PDF selected"}</span>
  </div>

  <div class="row">
    <label class="cpc">
      Cards / chunk:
      <input
        type="number"
        min="1"
        max="10"
        bind:value={cardsPerChunk}
      />
    </label>
    <button class="btn primary" onclick={generate} disabled={!pdfPath}>
      Generate deck
    </button>
    <button class="btn" onclick={startReview}>Start review</button>
  </div>

  {#if status.kind !== "idle"}
    <p class="status {status.kind}">{status.msg}</p>
  {/if}

  {#if current}
    <article class="card" class:revealed>
      <header class="card-head">
        <span class="page">page {current.page}</span>
        <button class="link" onclick={jumpToPage}>jump →</button>
      </header>
      <p class="q">{current.q}</p>
      {#if revealed}
        <hr />
        <p class="a">{current.a}</p>
        <div class="ease">
          <button class="ease-btn again" onclick={() => rate("again")}>Again</button>
          <button class="ease-btn hard" onclick={() => rate("hard")}>Hard</button>
          <button class="ease-btn good" onclick={() => rate("good")}>Good</button>
          <button class="ease-btn easy" onclick={() => rate("easy")}>Easy</button>
        </div>
      {:else}
        <button class="btn primary wide" onclick={() => (revealed = true)}>
          Reveal answer
        </button>
      {/if}
    </article>
  {/if}

  {#if stats}
    <footer class="stats">
      <span>{stats.total_cards} cards</span>
      <span>·</span>
      <span>{stats.due_now} due</span>
      <span>·</span>
      <span>{stats.reviewed_last_24h} reviewed today</span>
    </footer>
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    max-width: 720px;
    font-family: var(--font-ui, system-ui);
  }
  header h2 {
    font-size: 1.1rem;
    margin: 0;
  }
  .subtitle {
    color: var(--text-muted, #888);
    font-size: 0.85rem;
    margin: 0.25rem 0 0;
  }
  .picker,
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .path {
    color: var(--text-muted, #888);
    font-size: 0.85rem;
    font-family: var(--font-mono, ui-monospace);
  }
  .cpc {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
  }
  .cpc input {
    width: 3.5rem;
  }
  .btn {
    padding: 0.4rem 0.8rem;
    border-radius: 6px;
    border: 1px solid var(--border, #ccc);
    background: var(--bg-elev, #fff);
    cursor: pointer;
    font: inherit;
  }
  .btn.primary {
    background: var(--accent, #4a8df0);
    color: #fff;
    border-color: transparent;
  }
  .btn.primary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .btn.wide {
    width: 100%;
    margin-top: 0.75rem;
  }
  .status {
    font-size: 0.85rem;
    padding: 0.4rem 0.6rem;
    border-radius: 4px;
  }
  .status.working {
    background: var(--info-bg, #e8f1ff);
    color: var(--info-fg, #1d3a8a);
  }
  .status.ok {
    background: var(--ok-bg, #e6f6ec);
    color: var(--ok-fg, #186a3b);
  }
  .status.err {
    background: var(--err-bg, #fde8e8);
    color: var(--err-fg, #a8160f);
  }
  .card {
    border: 1px solid var(--border, #ddd);
    border-radius: 10px;
    padding: 1.25rem;
    background: var(--bg-elev, #fff);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  }
  .card-head {
    display: flex;
    justify-content: space-between;
    font-size: 0.75rem;
    color: var(--text-muted, #888);
    margin-bottom: 0.5rem;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent, #4a8df0);
    cursor: pointer;
    font: inherit;
  }
  .q {
    font-size: 1.05rem;
    margin: 0.5rem 0;
  }
  .a {
    font-size: 0.95rem;
    margin: 0.5rem 0;
    color: var(--text, #222);
  }
  hr {
    border: 0;
    border-top: 1px dashed var(--border, #ddd);
    margin: 0.75rem 0;
  }
  .ease {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.4rem;
    margin-top: 0.5rem;
  }
  .ease-btn {
    padding: 0.55rem 0.4rem;
    border-radius: 6px;
    border: 1px solid var(--border, #ccc);
    background: var(--bg-elev, #fff);
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
  }
  .ease-btn.again {
    background: #fde8e8;
    color: #a8160f;
  }
  .ease-btn.hard {
    background: #fef3c7;
    color: #8a5a00;
  }
  .ease-btn.good {
    background: #e6f6ec;
    color: #186a3b;
  }
  .ease-btn.easy {
    background: #e8f1ff;
    color: #1d3a8a;
  }
  .stats {
    display: flex;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--text-muted, #888);
    border-top: 1px solid var(--border, #eee);
    padding-top: 0.5rem;
  }
</style>
```

**Step 2: Verify svelte-check is happy**

```bash
cd /Users/sanjay/Projects/slab
pnpm check 2>&1 | tail -30
```
Expected: no new errors. Pre-existing warnings can stay.

**Step 3: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/study-ui): BeaconStudyPanel — pick PDF, generate, review

Svelte 5 panel that calls the four study Tauri commands. UI:
- Pick PDF, set cards/chunk knob (1..10), Generate.
- Start review → fetch due cards, walk one-at-a-time.
- Question shown front, click "Reveal" to flip, pick ease.
- Footer: total / due / reviewed_last_24h stats.
- Jump-to-page button fires slab:beacon-goto-page (existing event).

Friendly status banners follow the BeaconCitationsPanel pattern.
EOF
git add src/lib/panels/BeaconStudyPanel.svelte
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 7: Sidebar nav entry + panel mount

**Objective:** Add `{ id: "study", label: "Study", icon: "🎓", ready: true }`
to the features list in `+page.svelte`, mount the panel in both the
detached and sidebar conditionals, add to `DETACHABLE_PANELS`.

**Files:**
- Modify: `src/routes/+page.svelte`

**Step 1: Add the import**

Find the import block (line ~35). After `import BeaconCitationsPanel`,
insert:

```ts
import BeaconStudyPanel from "$lib/panels/BeaconStudyPanel.svelte";
```

**Step 2: Add to feature list**

Find the `const features: Feature[] = [` block (line ~68). After the
Citations row, insert:

```ts
    { id: "citations", label: "Citations", icon: "📑", ready: true },
    { id: "study", label: "Study", icon: "🎓", ready: true },
```

**Step 3: Add to `DETACHABLE_PANELS`**

In the `const DETACHABLE_PANELS = new Set<string>([...])` block (~line 138),
add `"study"` after `"citations"`:

```ts
    "citations",
    "study",
```

**Step 4: Mount in the detached-window branch**

Around line 524, the existing block looks like:

```svelte
    {:else if detachedPanel === "citations"}
      <BeaconCitationsPanel />
```

Add immediately below:

```svelte
    {:else if detachedPanel === "study"}
      <BeaconStudyPanel />
```

**Step 5: Mount in the main sidebar branch**

Around line 695:

```svelte
  {:else if active === "citations"}
    <BeaconCitationsPanel />
```

Add below:

```svelte
  {:else if active === "study"}
    <BeaconStudyPanel />
```

**Step 6: Verify svelte-check**

```bash
cd /Users/sanjay/Projects/slab
pnpm check 2>&1 | tail -15
```
Expected: no new errors.

**Step 7: Commit**

```bash
cat > /tmp/msg.txt <<'EOF'
feat(beacon/study-nav): Study panel sidebar entry + detach support

Added to the features list (icon 🎓), mounted in both the main and
detached-window branches of +page.svelte, registered in
DETACHABLE_PANELS so users can pop it into its own Cabinet window.
EOF
git add src/routes/+page.svelte
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt
```

---

## Task 8: Quality gates + push branch + STATE.md update

**Objective:** Run the full batched quality gates one last time, push the
branch with the gh-auth credential helper, flip STATE.md to DONE so the
next tick runs MODE A.

**Step 1: Run all four gates from the repo root**

```bash
cd /Users/sanjay/Projects/slab
# 1. Rust formatting
( cd src-tauri && cargo fmt --all -- --check ) 2>&1 | tail -5
# 2. Rust lints (deny warnings)
( cd src-tauri && cargo clippy --all-targets -- -D warnings ) 2>&1 | tail -15
# 3. Rust unit tests
( cd src-tauri && cargo test --lib ) 2>&1 | tail -10
# 4. Svelte / TS
pnpm check 2>&1 | tail -10
```

If any gate fails, fix in this same tick (do NOT skip). Common gotchas:
- Clippy will catch unused imports or `expect("...")` without docstring on a `lint::deny`.
- `cargo fmt`: run `cargo fmt --all` to auto-fix, then re-check.
- `pnpm check`: usually a missing `$state` cast or a Svelte 5 rune typo.

**Step 2: Sanity-check that all four new commands appear**

```bash
cd /Users/sanjay/Projects/slab
grep -nE "slab_beacon_(generate_deck|study_due|study_review|study_stats)" \
    src-tauri/src/lib.rs | head -20
```
Expected: each command name appears ≥2 times (definition + invoke_handler).

**Step 3: Push the branch with the credential-helper trick**

```bash
cd /Users/sanjay/Projects/slab
TOK=$(gh auth token)
git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' '$TOK'; }; f" \
    push -u origin feature/v1.7.0-beacon-bonus-13-study-mode
```

**Step 4: Update STATE.md**

Open `.cron-state/STATE.md` and replace the `## STATUS:` line + the
playbook section with:

```markdown
## STATUS: ✦ v1.7.0 Beacon Bonus Slice 13 "Study Mode" 🎓 DONE on feature/v1.7.0-beacon-bonus-13-study-mode — MERGE next tick

**Main HEAD**: <prev sha — unchanged>
**Feature branch HEAD**: <8th commit sha>
**Plan executed**: docs/plans/2026-05-18-beacon-slice-13-study-mode.md (8 tasks, all green)

---

## TICK <YYYY-MM-DD HH:MM> PT — Slice 13 shipped 🎓

Beacon Bonus Slice 13 "Study Mode" landed in one tick: 8 commits, the
full vertical slice (ai::study + ai::study_store + ai::sm2 + 4 Tauri
commands + BeaconStudyPanel.svelte + sidebar nav). Each layer
TDD'd against the existing patterns:

- ai::sm2 — pure scheduler (8 tests covering ease ladder + EF floor/cap)
- ai::study_store — sqlite at ~/.slab/study.sqlite, schema-versioned
  via PRAGMA user_version, modelled on pdf::library::registry
  (5 tests, in-memory harness)
- ai::study — generate_deck pipeline (chunk → LLM Q&A → validate;
  9 tests for parser + validator + dedupe + caps)
- 4 Tauri commands: generate_deck / study_due / study_review /
  study_stats — all hashed per PDF via sha256
- BeaconStudyPanel.svelte — pick PDF, generate, review one-at-a-time
  with 4-button ease scale, footer stats, jump-to-page link
- Sidebar nav + detach support

Quality gates: all four green (fmt, clippy -D warnings, cargo test
--lib, pnpm check).

Next tick: MODE A merge.
```

**Step 5: Commit the state update**

```bash
cat > /tmp/msg.txt <<'EOF'
chore(state): Slice 13 "Study Mode" DONE on feature branch

Plan docs/plans/2026-05-18-beacon-slice-13-study-mode.md executed
clean in one tick: 8 commits, all quality gates green. Next tick is
MODE A merge into main + v1.7.0 tag.
EOF
git add .cron-state/STATE.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -F /tmp/msg.txt

TOK=$(gh auth token)
git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' '$TOK'; }; f" \
    push origin feature/v1.7.0-beacon-bonus-13-study-mode
```

**Step 6: Cron delivery line**

The tick reply to Sanjay's Telegram should be the one-liner:

```
[cron] v1.7.0 Beacon Slice 13 "Study Mode" 🎓 — 8 commits, all gates green, MERGE next tick
```

---

## Plan Review Checklist

- [x] 8 tasks, each ≤ 15 min of focused work (sm2 was the only one with non-trivial math but it's still bite-sized; study_store has the most LoC but it's all formulaic CRUD).
- [x] TDD: study + sm2 + study_store have tests written before / alongside impl. The plan's commit order is "write impl + tests together, run, commit" because that mirrors how the existing modules in this codebase were written; each task ends with a passing test run.
- [x] DRY: re-uses `chunker::chunk_pages`, `pdf::extract::extract_text`, the existing schema-versioning pattern from `pdf::library::registry`, the JSON parser shape from `outline::parse_llm_outline`, the same friendly-status `Status` type used by the other Beacon panels.
- [x] YAGNI: no leaderboard, no streaks, no card editor, no import/export. SM-2-lite intentionally drops the repetition counter. Voice-read of cards waits for Slice 15.
- [x] Commits are conventional-commits style with scoped prefixes (`feat(beacon/study)`, etc.) and authored via `/tmp/msg.txt` + `git -c user.email=…`.
- [x] Push uses the `gh auth token` credential helper.
- [x] All four quality gates run once, at the end, batched.
- [x] STATE.md update is the last step so the *following* tick can be a clean MODE A merge.

**Time estimate:** ~55 min total (sm2 + study_store are the heaviest; the UI panel is ~10 min). Slice 11 and Slice 12 shipped in similar single-tick windows.

**Risk register:**
- If `sha2` isn't already a direct dep, `hash_pdf_path` won't compile. Mitigation: search `Cargo.toml` for `sha2`; if missing, either depend on it explicitly (`sha2 = "0.10"`) or reuse whatever the embedding_index uses.
- Clippy may flag the (_owned, _ref) pattern in `study_due` / `study_stats`. If it does, switch to `.as_deref()` on an `Option<String>` directly.
- If the `Ease` enum is rejected by Tauri's `command` arg parsing (lowercase serde), test with the frontend payload `{ ease: "good" }`. The `#[serde(rename_all = "lowercase")]` on the enum makes this work.
