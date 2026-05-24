//! Splits a Block stream into Chapters for EPUB serialization.
//!
//! When `split_on_h1` is true (the default), every `Heading { level: 1 }`
//! starts a new chapter, with all subsequent non-H1 blocks belonging to it.
//! Pre-heading content is collected into a synthetic "Chapter 1". When
//! `split_on_h1` is false, the entire document is emitted as one chapter.

use crate::pdf::reflow::types::Block;

#[derive(Debug, Clone)]
pub struct Chapter {
    pub title: String,
    pub blocks: Vec<Block>,
}

pub fn split_into_chapters(blocks: &[Block], split_on_h1: bool) -> Vec<Chapter> {
    if !split_on_h1 {
        return vec![Chapter {
            title: "Document".to_string(),
            blocks: blocks.to_vec(),
        }];
    }

    let mut chapters: Vec<Chapter> = Vec::new();
    let mut current: Option<Chapter> = None;

    for blk in blocks {
        match blk {
            Block::Heading { level: 1, text } => {
                if let Some(c) = current.take() {
                    chapters.push(c);
                }
                current = Some(Chapter {
                    title: text.clone(),
                    blocks: vec![blk.clone()],
                });
            }
            _ => {
                if let Some(ref mut c) = current {
                    c.blocks.push(blk.clone());
                } else {
                    // Pre-heading content goes into a synthetic "Chapter 1".
                    current = Some(Chapter {
                        title: "Chapter 1".to_string(),
                        blocks: vec![blk.clone()],
                    });
                }
            }
        }
    }
    if let Some(c) = current {
        chapters.push(c);
    }
    if chapters.is_empty() {
        chapters.push(Chapter {
            title: "Chapter 1".to_string(),
            blocks: blocks.to_vec(),
        });
    }
    chapters
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::reflow::types::Block;

    fn blk_h1(t: &str) -> Block {
        Block::Heading {
            level: 1,
            text: t.into(),
        }
    }
    fn blk_h2(t: &str) -> Block {
        Block::Heading {
            level: 2,
            text: t.into(),
        }
    }
    fn blk_body(t: &str) -> Block {
        Block::Body { text: t.into() }
    }

    #[test]
    fn splits_on_h1_when_enabled() {
        let blocks = vec![
            blk_h1("Introduction"),
            blk_body("para A"),
            blk_h1("Methods"),
            blk_body("para B"),
            blk_body("para C"),
        ];
        let chapters = split_into_chapters(&blocks, true);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Introduction");
        assert_eq!(chapters[0].blocks.len(), 2);
        assert_eq!(chapters[1].title, "Methods");
        assert_eq!(chapters[1].blocks.len(), 3);
    }

    #[test]
    fn single_chapter_when_no_h1() {
        let blocks = vec![blk_body("only para")];
        let chapters = split_into_chapters(&blocks, true);
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "Chapter 1");
    }

    #[test]
    fn forced_single_chapter_when_disabled() {
        let blocks = vec![blk_h1("A"), blk_body("x"), blk_h1("B")];
        let chapters = split_into_chapters(&blocks, false);
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "Document");
        assert_eq!(chapters[0].blocks.len(), 3);
    }

    #[test]
    fn pre_h1_content_lands_in_synthetic_chapter_one() {
        let blocks = vec![
            blk_body("front matter"),
            blk_h1("Real Chapter"),
            blk_body("x"),
        ];
        let chapters = split_into_chapters(&blocks, true);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Chapter 1");
        assert_eq!(chapters[0].blocks.len(), 1);
        assert_eq!(chapters[1].title, "Real Chapter");
    }

    #[test]
    fn h2_does_not_split() {
        let blocks = vec![blk_h1("A"), blk_h2("A.1"), blk_body("x"), blk_h2("A.2")];
        let chapters = split_into_chapters(&blocks, true);
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "A");
        assert_eq!(chapters[0].blocks.len(), 4);
    }
}
