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
