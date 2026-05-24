// Word-level diff for use by `pdf::diff`.
//
// Splits each input on whitespace boundaries while keeping the trailing
// whitespace attached to each token, so re-joining the resulting segments
// yields the original string byte-for-byte. This is what powers Stack's
// inline `<ins>`/`<del>` redline rendering.

use crate::pdf::diff::{WordDiff, WordOp};
use similar::{ChangeTag, TextDiff};

/// Tokenise `s` into whitespace-suffixed words.
///
/// `"foo  bar"` → `["foo  ", "bar"]`. Re-joining the slice reconstructs `s`
/// exactly. Operates on byte indices but only treats ASCII whitespace as a
/// boundary; multi-byte UTF-8 runs are kept intact inside a token.
pub fn tokenize(s: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    if s.is_empty() {
        return out;
    }
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        // Advance through a non-whitespace run.
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Then through any trailing whitespace.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        out.push(&s[start..i]);
        start = i;
    }
    out
}

/// Word-level diff between `old` and `new`. Adjacent same-op segments are
/// coalesced so the result is the minimum number of `WordDiff` entries.
pub fn diff_words(old: &str, new: &str) -> Vec<WordDiff> {
    let a = tokenize(old);
    let b = tokenize(new);
    let diff = TextDiff::from_slices(&a, &b);
    let mut out: Vec<WordDiff> = Vec::new();
    for change in diff.iter_all_changes() {
        let op = match change.tag() {
            ChangeTag::Equal => WordOp::Equal,
            ChangeTag::Insert => WordOp::Insert,
            ChangeTag::Delete => WordOp::Delete,
        };
        let text = change.value().to_string();
        if let Some(last) = out.last_mut() {
            if last.op == op {
                last.text.push_str(&text);
                continue;
            }
        }
        out.push(WordDiff { op, text });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_roundtrips_through_join() {
        let s = "The  quick brown\tfox";
        let toks = tokenize(s);
        assert_eq!(toks.concat(), s);
    }

    #[test]
    fn tokenize_empty_string_yields_empty_vec() {
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn tokenize_only_whitespace_yields_one_token() {
        let toks = tokenize("   ");
        assert_eq!(toks, vec!["   "]);
    }

    #[test]
    fn pure_insertion_marks_only_inserts() {
        let d = diff_words("hello world", "hello brave new world");
        let inserts: String = d
            .iter()
            .filter(|w| w.op == WordOp::Insert)
            .map(|w| w.text.as_str())
            .collect();
        assert!(inserts.contains("brave"));
        assert!(inserts.contains("new"));
        assert!(!d.iter().any(|w| w.op == WordOp::Delete));
    }

    #[test]
    fn pure_deletion_marks_only_deletes() {
        let d = diff_words("alpha beta gamma", "alpha gamma");
        assert!(d
            .iter()
            .any(|w| w.op == WordOp::Delete && w.text.contains("beta")));
        assert!(!d.iter().any(|w| w.op == WordOp::Insert));
    }

    #[test]
    fn rejoin_equal_plus_insert_equals_new() {
        let d = diff_words("a b", "a x b");
        let rebuilt: String = d
            .iter()
            .filter(|w| matches!(w.op, WordOp::Equal | WordOp::Insert))
            .map(|w| w.text.as_str())
            .collect();
        assert_eq!(rebuilt, "a x b");
    }

    #[test]
    fn rejoin_equal_plus_delete_equals_old() {
        let d = diff_words("a x b", "a b");
        let rebuilt: String = d
            .iter()
            .filter(|w| matches!(w.op, WordOp::Equal | WordOp::Delete))
            .map(|w| w.text.as_str())
            .collect();
        assert_eq!(rebuilt, "a x b");
    }

    #[test]
    fn substitution_produces_paired_delete_then_insert() {
        let d = diff_words("the quick brown fox", "the quick red fox");
        let has_brown_del = d
            .iter()
            .any(|w| w.op == WordOp::Delete && w.text.contains("brown"));
        let has_red_ins = d
            .iter()
            .any(|w| w.op == WordOp::Insert && w.text.contains("red"));
        assert!(has_brown_del && has_red_ins);
    }

    #[test]
    fn identical_strings_produce_only_equal() {
        let d = diff_words("same string", "same string");
        assert!(d.iter().all(|w| w.op == WordOp::Equal));
    }

    #[test]
    fn adjacent_same_op_segments_coalesce() {
        let d = diff_words("a", "a b c d");
        // After "a " equal, we expect ONE insert containing "b c d", not three.
        let inserts: Vec<&WordDiff> = d.iter().filter(|w| w.op == WordOp::Insert).collect();
        assert_eq!(inserts.len(), 1);
        assert!(inserts[0].text.contains("b"));
        assert!(inserts[0].text.contains("c"));
        assert!(inserts[0].text.contains("d"));
    }
}
