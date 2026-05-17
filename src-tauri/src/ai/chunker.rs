// Beacon Chunker — split per-page PDF text into embedding-sized chunks.
//
// Rationale: embedding models (Ollama's `nomic-embed-text`, OpenAI's
// `text-embedding-3-small`) work best on coherent prose blocks of
// 200-800 chars. Too short → not enough context, retrieval picks up
// stop-word matches. Too long → embedding gets averaged out, retrieval
// blurs. We aim for ~600 chars with paragraph-aware boundaries, and
// overlap the tail of each chunk into the head of the next so a
// concept that straddles a chunk boundary is still findable.
//
// Each chunk remembers its source page (1-indexed) so the UI can jump
// the reader to the right page when the user clicks a search result.
//
// This module is intentionally pure (no IO, no allocations of state
// outside the returned vec) — easy to unit-test.

use serde::{Deserialize, Serialize};

/// Target chunk size in chars. Picked to land in the embedding model's
/// sweet spot (~150 tokens for nomic-embed-text). Not a hard cap —
/// `chunk_page` may overshoot by up to ~30% if a paragraph is long.
pub const TARGET_CHUNK_CHARS: usize = 600;

/// Floor below which we stop emitting a chunk — too-short trailing
/// chunks tend to be navigation crumbs / page numbers that pollute
/// search results.
pub const MIN_CHUNK_CHARS: usize = 80;

/// Char overlap from the previous chunk's tail into the next chunk's
/// head. Lets a sentence that's split across a boundary still be
/// retrievable from either side.
pub const CHUNK_OVERLAP_CHARS: usize = 80;

/// One chunk of source text with provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chunk {
    /// 1-indexed page number this chunk came from.
    pub page: u32,
    /// 0-indexed sequence within the page. Lets us reconstruct order
    /// and dedupe re-indexing without UPDATE-WHERE acrobatics.
    pub idx_in_page: u32,
    /// The actual text content. Already whitespace-collapsed.
    pub text: String,
}

/// Split one page of text into chunks. Tries to break at paragraph
/// boundaries (double newline), falls back to sentence ends, then
/// raw char count as last resort.
pub fn chunk_page(page_text: &str, page_no: u32) -> Vec<Chunk> {
    let collapsed = collapse_whitespace(page_text);
    if collapsed.trim().len() < MIN_CHUNK_CHARS {
        // Whole page is shorter than min chunk — emit it as a single
        // chunk if it has *any* content at all, otherwise skip.
        if collapsed.trim().is_empty() {
            return Vec::new();
        }
        return vec![Chunk {
            page: page_no,
            idx_in_page: 0,
            text: collapsed.trim().to_string(),
        }];
    }

    // Split on paragraph boundaries first.
    let paragraphs: Vec<&str> = collapsed
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current = String::new();
    let mut idx: u32 = 0;

    for para in paragraphs {
        // If adding this paragraph keeps us under target, append it.
        if current.len() + para.len() < TARGET_CHUNK_CHARS {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(para);
            continue;
        }

        // Otherwise, flush current and start a new one. If `para`
        // itself is bigger than target, split it on sentence ends.
        if !current.is_empty() {
            chunks.push(Chunk {
                page: page_no,
                idx_in_page: idx,
                text: current.clone(),
            });
            idx += 1;
            current = take_overlap_tail(&current);
        }
        if para.len() > TARGET_CHUNK_CHARS {
            for piece in split_long_paragraph(para) {
                if current.len() + piece.len() < TARGET_CHUNK_CHARS {
                    if !current.is_empty() {
                        current.push(' ');
                    }
                    current.push_str(piece);
                } else {
                    if !current.is_empty() {
                        chunks.push(Chunk {
                            page: page_no,
                            idx_in_page: idx,
                            text: current.clone(),
                        });
                        idx += 1;
                    }
                    current = take_overlap_tail(&current);
                    if !current.is_empty() {
                        current.push(' ');
                    }
                    current.push_str(piece);
                }
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(para);
        }
    }
    if current.trim().len() >= MIN_CHUNK_CHARS || (chunks.is_empty() && !current.trim().is_empty())
    {
        chunks.push(Chunk {
            page: page_no,
            idx_in_page: idx,
            text: current.trim().to_string(),
        });
    }
    chunks
}

/// Split every page in `pages` (1-indexed by position) into chunks.
pub fn chunk_pages(pages: &[String]) -> Vec<Chunk> {
    let mut out = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let mut page_chunks = chunk_page(page, (i as u32) + 1);
        out.append(&mut page_chunks);
    }
    out
}

/// Collapse runs of horizontal whitespace into a single space, but
/// preserve paragraph breaks (double newlines). PDF text extraction
/// tends to spit out hard-wrapped lines; we want flowing paragraphs.
fn collapse_whitespace(s: &str) -> String {
    // Split on \n\n (paragraph), then within each para collapse
    // single newlines + runs of spaces into single spaces.
    s.split("\n\n")
        .map(|para| para.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Split a paragraph longer than TARGET_CHUNK_CHARS at sentence ends
/// (`. `, `? `, `! `). Fallback: hard split at TARGET_CHUNK_CHARS.
fn split_long_paragraph(para: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = para.as_bytes();
    while start < bytes.len() {
        let remaining = bytes.len() - start;
        if remaining <= TARGET_CHUNK_CHARS {
            out.push(&para[start..]);
            break;
        }
        let window_end = (start + TARGET_CHUNK_CHARS).min(bytes.len());
        let window = &para[start..window_end];
        // Find a sentence end inside the window, prefer the last one.
        let break_at = window
            .rfind(". ")
            .or_else(|| window.rfind("? "))
            .or_else(|| window.rfind("! "))
            .map(|p| p + 2)
            .or_else(|| window.rfind(' ').map(|p| p + 1))
            .unwrap_or(window.len());
        // Step back across `start` index boundary safely.
        let abs_end = start + break_at;
        // Find char boundary
        let abs_end = floor_char_boundary(para, abs_end);
        if abs_end <= start {
            // No progress possible; hard slice
            let hard = floor_char_boundary(para, window_end);
            if hard <= start {
                break;
            }
            out.push(&para[start..hard]);
            start = hard;
            continue;
        }
        out.push(&para[start..abs_end]);
        start = abs_end;
    }
    out
}

/// Return the largest valid UTF-8 char boundary `<= i`.
fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Take the trailing `CHUNK_OVERLAP_CHARS` chars of `prev` so the next
/// chunk starts with a small overlap. Returned string is whitespace-
/// trimmed at the left edge so we never start a chunk mid-word if we
/// can help it.
fn take_overlap_tail(prev: &str) -> String {
    if prev.len() <= CHUNK_OVERLAP_CHARS {
        return String::new();
    }
    let start = floor_char_boundary(prev, prev.len() - CHUNK_OVERLAP_CHARS);
    let tail = &prev[start..];
    // Trim leading partial word
    let trimmed = match tail.find(' ') {
        Some(sp) => &tail[sp + 1..],
        None => tail,
    };
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_page_emits_one_chunk() {
        let chunks = chunk_page("Quarterly results were strong. Revenue grew 12%.", 7);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].page, 7);
        assert_eq!(chunks[0].idx_in_page, 0);
        assert!(chunks[0].text.contains("Quarterly"));
    }

    #[test]
    fn empty_page_emits_zero_chunks() {
        let chunks = chunk_page("   \n\n  \t  \n", 1);
        assert!(chunks.is_empty());
    }

    #[test]
    fn tiny_page_still_emits_chunk_if_any_content() {
        // Below MIN_CHUNK_CHARS but non-empty — keep it, dropping it
        // would mean a 1-page summary card has nothing to retrieve.
        let chunks = chunk_page("Title", 1);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Title");
    }

    #[test]
    fn paragraph_breaks_drive_chunking() {
        let body = format!(
            "{}\n\n{}\n\n{}",
            "a".repeat(400),
            "b".repeat(400),
            "c".repeat(400),
        );
        let chunks = chunk_page(&body, 2);
        // 3 paragraphs of 400 chars; target is 600 → expect ≥2 chunks.
        assert!(
            chunks.len() >= 2,
            "expected ≥2 chunks for 1200 chars of body, got {}",
            chunks.len()
        );
        for c in &chunks {
            assert_eq!(c.page, 2);
        }
        // idx_in_page increments monotonically
        let mut idxs: Vec<u32> = chunks.iter().map(|c| c.idx_in_page).collect();
        let sorted = {
            let mut s = idxs.clone();
            s.sort();
            s
        };
        idxs.sort();
        assert_eq!(idxs, sorted);
    }

    #[test]
    fn long_paragraph_splits_at_sentence_boundary() {
        // 5 sentences of ~200 chars each = ~1000 chars in one paragraph.
        // Should be split at sentence ends.
        let para = (0..5)
            .map(|i| format!("Sentence {i} {}.", "x".repeat(180)))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunk_page(&para, 1);
        assert!(
            chunks.len() >= 2,
            "expected ≥2 chunks, got {}",
            chunks.len()
        );
        for c in &chunks {
            // No chunk should be wildly over target.
            assert!(c.text.len() < TARGET_CHUNK_CHARS + 250);
        }
    }

    #[test]
    fn unicode_doesnt_panic() {
        let body = "日本語のテスト文書 ".repeat(300);
        let chunks = chunk_page(&body, 1);
        assert!(!chunks.is_empty());
        for c in &chunks {
            assert!(c.text.is_char_boundary(0));
            assert!(c.text.is_char_boundary(c.text.len()));
        }
    }

    #[test]
    fn chunk_pages_concatenates_with_correct_page_numbers() {
        let pages = vec![
            "First page content. Lorem ipsum.".to_string(),
            "Second page content. Dolor sit.".to_string(),
            "Third page content. Amet consectetur.".to_string(),
        ];
        let chunks = chunk_pages(&pages);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].page, 1);
        assert_eq!(chunks[1].page, 2);
        assert_eq!(chunks[2].page, 3);
    }

    #[test]
    fn collapse_whitespace_keeps_paragraph_breaks() {
        let input = "Line one\nstill line one\n\nNew paragraph\n   here";
        let out = collapse_whitespace(input);
        assert_eq!(out, "Line one still line one\n\nNew paragraph here");
    }

    #[test]
    fn floor_char_boundary_handles_multibyte() {
        let s = "日本語";
        // mid-char index should snap left to the previous boundary
        let i = floor_char_boundary(s, 2);
        assert!(s.is_char_boundary(i));
    }
}
