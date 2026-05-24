//! Cluster `TextRun`s into a `SlideContent` (title + body bullets).

use crate::pdf::reflow::types::TextRun;
use crate::pdf::slide::types::SlideContent;

/// The largest run sitting in the top third of the page becomes the title
/// when `detect_title` is true; everything else (in descending-Y order) is a
/// body bullet. Empty or whitespace-only runs are dropped.
pub(crate) fn cluster(
    runs: &[TextRun],
    width_pt: f32,
    height_pt: f32,
    detect_title: bool,
) -> SlideContent {
    let top_band = height_pt * 0.66;
    let mut title_idx: Option<usize> = None;

    if detect_title {
        let mut best_size = 0.0f32;
        for (i, r) in runs.iter().enumerate() {
            if r.y >= top_band && r.font_size > best_size && !r.text.trim().is_empty() {
                best_size = r.font_size;
                title_idx = Some(i);
            }
        }
    }

    let title = title_idx.map(|i| runs[i].text.trim().to_string());

    let mut body: Vec<&TextRun> = runs
        .iter()
        .enumerate()
        .filter(|(i, r)| Some(*i) != title_idx && !r.text.trim().is_empty())
        .map(|(_, r)| r)
        .collect();
    body.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal));
    let body_bullets: Vec<String> = body
        .into_iter()
        .map(|r| r.text.trim().to_string())
        .collect();

    SlideContent {
        title,
        body_bullets,
        notes: None,
        width_pt,
        height_pt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, x: f32, y: f32, size: f32) -> TextRun {
        TextRun {
            page: 1,
            x,
            y,
            text: text.to_string(),
            font_name: String::new(),
            font_size: size,
            bold: false,
            italic: false,
        }
    }

    #[test]
    fn largest_top_run_is_title() {
        let runs = vec![
            run("Slab Beats Acrobat", 50.0, 700.0, 32.0),
            run("Bullet one", 50.0, 600.0, 14.0),
            run("Bullet two", 50.0, 580.0, 14.0),
        ];
        let c = cluster(&runs, 612.0, 792.0, true);
        assert_eq!(c.title.as_deref(), Some("Slab Beats Acrobat"));
        assert_eq!(c.body_bullets, vec!["Bullet one", "Bullet two"]);
    }

    #[test]
    fn no_title_when_disabled() {
        let runs = vec![run("Big", 50.0, 700.0, 32.0)];
        let c = cluster(&runs, 612.0, 792.0, false);
        assert!(c.title.is_none());
        assert_eq!(c.body_bullets, vec!["Big"]);
    }

    #[test]
    fn empty_runs_yield_empty_slide() {
        let c = cluster(&[], 612.0, 792.0, true);
        assert!(c.title.is_none());
        assert!(c.body_bullets.is_empty());
    }

    #[test]
    fn body_sorted_top_to_bottom() {
        let runs = vec![
            run("third", 50.0, 200.0, 12.0),
            run("first", 50.0, 600.0, 12.0),
            run("second", 50.0, 400.0, 12.0),
        ];
        // detect_title=false to keep them all as bullets
        let c = cluster(&runs, 612.0, 792.0, false);
        assert_eq!(c.body_bullets, vec!["first", "second", "third"]);
    }

    #[test]
    fn carries_page_dimensions() {
        let c = cluster(&[], 720.0, 540.0, true);
        assert_eq!(c.width_pt, 720.0);
        assert_eq!(c.height_pt, 540.0);
    }
}
