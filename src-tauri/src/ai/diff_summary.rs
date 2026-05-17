// Beacon Diff Summary — natural-language explanation of a `pdf::diff::DocDiff`.
//
// Architecture mirrors `ai::summary`: build a deterministic message vec
// (system + user with diff context), call provider.chat, return a tagged
// response. Tested with the same `MockProvider` pattern.
//
// The user-facing flow is: user runs Compare, gets a structured line diff,
// optionally clicks "Explain Changes" to get an AI-written paragraph
// describing *what* changed — not just *that* something changed.

use super::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use crate::pdf::diff::DocDiff;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// What the front-end gets back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconDiffSummary {
    /// Plain-text natural language summary.
    pub content: String,
    /// Model that produced it.
    pub model: String,
    /// Whether we truncated the diff before sending (so the UI can warn).
    pub truncated: bool,
    /// Number of changed pages we included.
    pub pages_included: u32,
    /// Number of changed pages in the diff (for "N of M" display).
    pub pages_total_changed: u32,
}

/// Build the textual diff block sent to the model. Format is a
/// human-friendly approximation of unified diff — clear enough that any
/// chat model can reason about it.
///
/// `max_chars` is a soft budget: we stop adding pages once we'd exceed it
/// and set `truncated = true`. Equal lines are ALWAYS skipped (they're
/// noise). Public for testability.
pub fn build_diff_block(diff: &DocDiff, max_chars: usize) -> (String, bool, u32, u32) {
    let mut block = String::new();
    let mut truncated = false;
    let mut included: u32 = 0;
    let mut total_changed: u32 = 0;

    for page in &diff.pages {
        let s = &page.summary;
        if s.added == 0 && s.removed == 0 && s.changed == 0 {
            continue;
        }
        total_changed += 1;

        let heading = match (page.old_page, page.new_page) {
            (Some(o), Some(n)) if o == n => format!("=== Page {o} ===\n"),
            (Some(o), Some(n)) => format!("=== Old p.{o} ↔ New p.{n} ===\n"),
            (Some(o), None) => format!("=== Old p.{o} (removed) ===\n"),
            (None, Some(n)) => format!("=== New p.{n} (added) ===\n"),
            (None, None) => "=== (orphan page) ===\n".to_string(),
        };
        // Build this page's body first so we can decide whole-page inclusion.
        let mut body = String::new();
        for line in &page.lines {
            use crate::pdf::diff::DiffOp;
            let marker = match line.op {
                DiffOp::Equal => continue,
                DiffOp::Insert => '+',
                DiffOp::Delete => '-',
            };
            body.push(marker);
            body.push(' ');
            body.push_str(line.text.trim_end());
            body.push('\n');
        }

        if block.len() + heading.len() + body.len() > max_chars && included > 0 {
            truncated = true;
            break;
        }
        block.push_str(&heading);
        block.push_str(&body);
        block.push('\n');
        included += 1;
    }

    (block, truncated, included, total_changed)
}

fn build_messages(diff: &DocDiff, max_diff_chars: usize) -> (Vec<ChatMessage>, bool, u32, u32) {
    let (diff_block, truncated, included, total_changed) = build_diff_block(diff, max_diff_chars);

    let header = format!(
        "Old file: {}\nNew file: {}\nPage counts: {} → {}\nTotals: +{} added, -{} removed, ~{} changed\n",
        diff.old_path.display(),
        diff.new_path.display(),
        diff.old_page_count,
        diff.new_page_count,
        diff.total.added,
        diff.total.removed,
        diff.total.changed,
    );

    let body = if diff_block.is_empty() {
        "(no textual differences detected — the two PDFs may differ only in non-text content like images, fonts, or metadata)".to_string()
    } else {
        diff_block
    };

    let truncated_note = if truncated {
        "\n\n[NOTE: the diff was truncated to fit the model's context window. The summary below covers the most-changed pages first.]"
    } else {
        ""
    };

    let user_content = format!(
        "{header}\n--- DIFF START ---\n{body}--- DIFF END ---{truncated_note}\n\nWrite a clear, factual explanation of what changed between the old and new PDF. Focus on the substance of the edits — what's been added, removed, or rewritten. If there's a discernible theme to the changes (e.g. a section was reorganized, a clause was tightened, numbers were updated), call it out. Reply in 3-6 short bullet points. Do not speculate about intent."
    );

    let msgs = vec![
        ChatMessage {
            role: ChatRole::System,
            content: "You are Beacon, a PDF diff explainer. You receive a unified-diff-ish block and you describe in plain English what changed. Stay strictly within the diff content — do not invent details. Prefer specificity over generality."
                .to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: user_content,
        },
    ];
    (msgs, truncated, included, total_changed)
}

/// Run a Beacon diff-summary call. `max_diff_chars` is the soft budget for
/// the diff body inside the user message (the header is always included).
pub async fn beacon_diff_summary(
    provider: Arc<dyn AiProvider>,
    diff: &DocDiff,
    max_diff_chars: usize,
) -> Result<BeaconDiffSummary, AiError> {
    let (msgs, truncated, pages_included, pages_total_changed) =
        build_messages(diff, max_diff_chars);
    let opts = ChatOpts {
        // Diffs want precision, not creativity.
        temperature: Some(0.1),
        max_tokens: Some(600),
        ..Default::default()
    };
    let resp = provider.chat(&msgs, &opts).await?;
    Ok(BeaconDiffSummary {
        content: resp.content,
        model: resp.model,
        truncated,
        pages_included,
        pages_total_changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ChatResponse;
    use crate::pdf::diff::{DiffOp, DiffSummary, LineDiff, PageDiff};
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockProvider {
        captured: Mutex<Vec<ChatMessage>>,
        captured_opts: Mutex<Option<ChatOpts>>,
        reply: String,
    }

    #[async_trait]
    impl AiProvider for MockProvider {
        async fn chat(
            &self,
            msgs: &[ChatMessage],
            opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            *self.captured.lock().unwrap() = msgs.to_vec();
            *self.captured_opts.lock().unwrap() = Some(opts.clone());
            Ok(ChatResponse {
                content: self.reply.clone(),
                model: "mock-model".to_string(),
            })
        }
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            unimplemented!()
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    fn mk_diff_one_changed_page() -> DocDiff {
        let page = PageDiff {
            old_page: Some(1),
            new_page: Some(1),
            lines: vec![
                LineDiff {
                    op: DiffOp::Equal,
                    old_line: Some(1),
                    new_line: Some(1),
                    text: "intro".into(),
                },
                LineDiff {
                    op: DiffOp::Delete,
                    old_line: Some(2),
                    new_line: None,
                    text: "old clause".into(),
                },
                LineDiff {
                    op: DiffOp::Insert,
                    old_line: None,
                    new_line: Some(2),
                    text: "new clause".into(),
                },
            ],
            summary: DiffSummary {
                added: 1,
                removed: 1,
                changed: 1,
            },
        };
        DocDiff {
            old_path: PathBuf::from("/tmp/a.pdf"),
            new_path: PathBuf::from("/tmp/b.pdf"),
            old_page_count: 1,
            new_page_count: 1,
            pages: vec![page],
            total: DiffSummary {
                added: 1,
                removed: 1,
                changed: 1,
            },
        }
    }

    #[test]
    fn build_diff_block_skips_equal_lines() {
        let d = mk_diff_one_changed_page();
        let (block, truncated, included, total) = build_diff_block(&d, 10_000);
        assert!(block.contains("- old clause"));
        assert!(block.contains("+ new clause"));
        assert!(!block.contains("intro"), "equal lines must be excluded");
        assert!(!truncated);
        assert_eq!(included, 1);
        assert_eq!(total, 1);
    }

    #[test]
    fn build_diff_block_skips_unchanged_pages() {
        let mut d = mk_diff_one_changed_page();
        d.pages.push(PageDiff {
            old_page: Some(2),
            new_page: Some(2),
            lines: vec![LineDiff {
                op: DiffOp::Equal,
                old_line: Some(1),
                new_line: Some(1),
                text: "unchanged".into(),
            }],
            summary: DiffSummary::default(),
        });
        let (_block, _trunc, included, total) = build_diff_block(&d, 10_000);
        assert_eq!(included, 1, "only one changed page should be included");
        assert_eq!(total, 1);
    }

    #[test]
    fn build_diff_block_truncates_when_over_budget() {
        // Two changed pages, tiny budget — second should be excluded.
        let mut d = mk_diff_one_changed_page();
        let mut p2 = d.pages[0].clone();
        p2.old_page = Some(2);
        p2.new_page = Some(2);
        d.pages.push(p2);
        let (_block, truncated, included, total) = build_diff_block(&d, 60);
        assert!(truncated, "expected truncation at 60-char budget");
        assert_eq!(included, 1);
        assert_eq!(total, 2);
    }

    #[tokio::test]
    async fn beacon_diff_summary_sends_diff_to_provider() {
        let mock = Arc::new(MockProvider {
            captured: Mutex::new(Vec::new()),
            captured_opts: Mutex::new(None),
            reply: "- One clause was replaced.".to_string(),
        });
        let d = mk_diff_one_changed_page();
        let res = beacon_diff_summary(mock.clone(), &d, 10_000).await.unwrap();
        assert!(res.content.contains("One clause"));
        assert_eq!(res.model, "mock-model");
        assert!(!res.truncated);
        assert_eq!(res.pages_included, 1);
        assert_eq!(res.pages_total_changed, 1);

        // The user message must contain the actual diff body.
        let captured = mock.captured.lock().unwrap().clone();
        assert_eq!(captured.len(), 2);
        let user = &captured[1];
        assert!(user.content.contains("- old clause"));
        assert!(user.content.contains("+ new clause"));
        assert!(user.content.contains("Page counts: 1 → 1"));

        // Deterministic temperature.
        let opts = mock.captured_opts.lock().unwrap().clone().unwrap();
        assert_eq!(opts.temperature, Some(0.1));
        assert_eq!(opts.max_tokens, Some(600));
    }

    #[tokio::test]
    async fn beacon_diff_summary_empty_diff_emits_no_difference_note() {
        let mock = Arc::new(MockProvider {
            captured: Mutex::new(Vec::new()),
            captured_opts: Mutex::new(None),
            reply: "No textual changes.".to_string(),
        });
        let d = DocDiff {
            old_path: PathBuf::from("/x.pdf"),
            new_path: PathBuf::from("/y.pdf"),
            old_page_count: 1,
            new_page_count: 1,
            pages: vec![],
            total: DiffSummary::default(),
        };
        let _ = beacon_diff_summary(mock.clone(), &d, 10_000).await.unwrap();
        let captured = mock.captured.lock().unwrap().clone();
        let user = &captured[1];
        assert!(user.content.contains("no textual differences detected"));
    }
}
