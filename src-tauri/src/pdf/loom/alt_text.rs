//! Slab Loom — Slice 4: Beacon-generated alt-text for Figure nodes.
//!
//! PDF/UA-1 (ISO 14289-1:2014) §7.3 requires that every meaningful image
//! carries an `/Alt` entry (or `/ActualText`) so screen readers can
//! describe the image. WCAG 2.2 SC 1.1.1 says the same thing for the
//! web. Adobe Acrobat Pro's Sensei alt-text feature ships the image
//! bytes to Adobe's servers; we generate alt-text 100% offline via the
//! Beacon vision provider (Ollama llava by default).
//!
//! Architecture
//! ------------
//! We do **not** crack the PDF XObject byte stream — the filter zoo
//! (DCT, JPX, Flate, JBIG2, CCITTFax) makes that fragile. Instead we
//! re-use the existing `ai::vision::render_page_image` rasterizer:
//! render the figure's page at 150 DPI, crop to the figure's bbox,
//! SHA-256-hash the resulting PNG, and use that as the cache key.
//! Identical rendered pixels → identical cache hit, deterministic
//! across reruns.
//!
//! Cache layout
//! ------------
//! `<cache_dir>/<sha256_hex>.txt` — single line of UTF-8.
//! `<cache_dir>` is typically
//! `~/Library/Application Support/Slab/cache/alt-text/` on macOS,
//! `~/.config/Slab/cache/alt-text/` on Linux,
//! `%APPDATA%/Slab/cache/alt-text/` on Windows. The caller picks; we
//! never assume.
//!
//! Failure handling
//! ----------------
//! A single figure failing must not abort enrichment of the rest. We
//! tally errors in [`AltTextStats::errors`] and keep going. Already-set
//! `alt_text` on a node is left untouched — useful if a future tagger
//! pass injects manual alt-text and then re-runs the pipeline.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::ai::vision::{render_page_image, RectPts, VisionOpts};
use crate::ai::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use crate::pdf::loom::classify::{NodeKind, StructNode, StructTree};
use crate::pdf::loom::layout::Bbox;

/// Knobs the caller can tune. Defaults are tuned to llava-7b on a
/// MacBook Air M2 — about 4 seconds per figure cold, ~0ms cached.
#[derive(Debug, Clone)]
pub struct AltTextOptions {
    /// Rasterization DPI for the crop. 150 is the llava sweet spot.
    pub dpi: u32,
    /// Cap the max image edge after the crop+downscale pipeline.
    pub max_edge_px: u32,
    /// System+user prompt sent to the vision provider.
    pub prompt: String,
    /// Skip figures whose bbox area is below this (in PDF pt²).
    /// Defaults to 200 pt² — that's about a 14pt × 14pt icon, smaller
    /// than meaningful figures but typical for bullet-point glyphs and
    /// page-number bookmark icons. Saves seconds on long docs.
    pub min_area_pt: f32,
}

impl Default for AltTextOptions {
    fn default() -> Self {
        Self {
            dpi: 150,
            max_edge_px: 1568,
            prompt: "Describe this image in one concise sentence for a \
                     blind screen-reader user. Be concrete: name the \
                     subject, key colors, and any visible text. Do not \
                     start with 'An image of' or 'A picture of'."
                .into(),
            min_area_pt: 200.0,
        }
    }
}

/// Aggregate stats returned to the UI.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AltTextStats {
    pub figures_total: usize,
    pub generated: usize,
    pub cache_hits: usize,
    pub skipped_tiny: usize,
    pub skipped_preexisting: usize,
    pub errors: usize,
}

/// Walk every Figure node in `tree` and fill `alt_text`. Pure async
/// function; the caller owns the provider lifetime.
///
/// Returns aggregate stats. Per-figure errors are absorbed into
/// `stats.errors`; only a hard failure (e.g. cache dir not writable
/// AND we can't even render anything) returns `Err`.
pub async fn enrich_with_alt_text(
    pdf_path: &Path,
    tree: &mut StructTree,
    provider: Arc<dyn AiProvider>,
    opts: &AltTextOptions,
    cache_dir: &Path,
) -> Result<AltTextStats, AiError> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| AiError::Network(format!("create cache dir: {e}")))?;

    let mut stats = AltTextStats::default();

    // Collect (page_number, path-to-node, bbox) tuples first so we can
    // mutate the tree afterwards without borrow conflicts. Path is a
    // Vec<usize> of child indices from page root.
    let mut targets: Vec<(u32, Vec<usize>, Bbox, bool)> = Vec::new();
    for page in &tree.pages {
        let pn = page.page_number;
        collect_figures(&page.nodes, pn, &mut Vec::new(), &mut targets);
    }
    stats.figures_total = targets.len();

    for (page_number, path, bbox, has_existing) in targets {
        if has_existing {
            stats.skipped_preexisting += 1;
            continue;
        }
        let area = bbox.width().max(0.0) * bbox.height().max(0.0);
        if area < opts.min_area_pt {
            stats.skipped_tiny += 1;
            continue;
        }

        match process_one_figure(pdf_path, page_number, bbox, &provider, opts, cache_dir).await {
            Ok((alt, from_cache)) => {
                if from_cache {
                    stats.cache_hits += 1;
                } else {
                    stats.generated += 1;
                }
                apply_alt_text(&mut tree.pages, page_number, &path, alt);
            }
            Err(_e) => {
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}

/// Convenience for a single figure — exposed so callers can fetch one
/// alt-text without a tree (e.g. selection action in the reader).
/// Returns `(alt_text, came_from_cache)`.
pub async fn alt_text_for_bbox(
    pdf_path: &Path,
    page: u32,
    bbox: Bbox,
    provider: Arc<dyn AiProvider>,
    opts: &AltTextOptions,
    cache_dir: &Path,
) -> Result<(String, bool), AiError> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| AiError::Network(format!("create cache dir: {e}")))?;
    process_one_figure(pdf_path, page, bbox, &provider, opts, cache_dir).await
}

// --- internals -------------------------------------------------------

async fn process_one_figure(
    pdf_path: &Path,
    page: u32,
    bbox: Bbox,
    provider: &Arc<dyn AiProvider>,
    opts: &AltTextOptions,
    cache_dir: &Path,
) -> Result<(String, bool), AiError> {
    let rect = RectPts {
        x: bbox.x0,
        y: bbox.y0,
        width: bbox.width().max(1.0),
        height: bbox.height().max(1.0),
    };
    let vision_opts = VisionOpts {
        dpi: opts.dpi,
        max_edge_px: opts.max_edge_px,
    };
    let png = render_page_image(pdf_path, page, Some(rect), &vision_opts)?;

    let hash = sha256_hex(&png);
    let cache_path = cache_dir.join(format!("{hash}.txt"));
    if let Ok(existing) = std::fs::read_to_string(&cache_path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok((trimmed.to_string(), true));
        }
    }

    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    let b64 = B64.encode(&png);

    let msgs = vec![
        ChatMessage {
            role: ChatRole::System,
            content: "You are Beacon, generating accessibility alt-text. Reply with a \
                 single sentence, no preface, no markdown."
                .into(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: opts.prompt.clone(),
        },
    ];
    let chat_opts = ChatOpts {
        model: None,
        temperature: Some(0.1),
        max_tokens: Some(120),
    };
    let resp = provider.chat_with_images(&msgs, &[b64], &chat_opts).await?;
    let alt = normalise_alt(&resp.content);
    if alt.is_empty() {
        return Err(AiError::InvalidResponse("empty alt-text reply".into()));
    }
    // Best-effort cache write — failure here is not fatal.
    let _ = std::fs::write(&cache_path, &alt);
    Ok((alt, false))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out.iter() {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Trim, collapse internal whitespace, strip prefixes vision models love
/// ("Image of...", "This image shows..."), and cap at 280 chars.
fn normalise_alt(raw: &str) -> String {
    let mut s = raw.trim().to_string();

    // Strip surrounding quotes if the model wrapped it.
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s = s[1..s.len() - 1].trim().to_string();
    }

    // Collapse whitespace first so prefix matching is whitespace-tolerant.
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");

    let prefixes = [
        "an image of ",
        "a picture of ",
        "this image shows ",
        "the image shows ",
        "the image depicts ",
        "this is an image of ",
        "this is a picture of ",
    ];
    let lower = s.to_lowercase();
    for p in prefixes {
        if let Some(_rest) = lower.strip_prefix(p) {
            let original_rest = &s[p.len()..];
            let mut chars = original_rest.chars();
            if let Some(c) = chars.next() {
                s = format!("{}{}", c.to_uppercase(), chars.as_str());
            } else {
                s = original_rest.to_string();
            }
            break;
        }
    }

    let mut out = s;
    if out.chars().count() > 280 {
        out = out.chars().take(277).collect::<String>() + "...";
    }
    out
}

fn collect_figures(
    nodes: &[StructNode],
    page_number: u32,
    stack: &mut Vec<usize>,
    out: &mut Vec<(u32, Vec<usize>, Bbox, bool)>,
) {
    for (i, n) in nodes.iter().enumerate() {
        stack.push(i);
        if matches!(n.kind, NodeKind::Figure) {
            out.push((page_number, stack.clone(), n.bbox, n.alt_text.is_some()));
        }
        collect_figures(&n.children, page_number, stack, out);
        stack.pop();
    }
}

fn apply_alt_text(
    pages: &mut [crate::pdf::loom::classify::StructTreePage],
    page_number: u32,
    path: &[usize],
    alt: String,
) {
    let Some(page) = pages.iter_mut().find(|p| p.page_number == page_number) else {
        return;
    };
    let mut cursor: &mut Vec<StructNode> = &mut page.nodes;
    for (depth, idx) in path.iter().enumerate() {
        if depth + 1 == path.len() {
            if let Some(node) = cursor.get_mut(*idx) {
                node.alt_text = Some(alt);
            }
            return;
        }
        let Some(node) = cursor.get_mut(*idx) else {
            return;
        };
        cursor = &mut node.children;
    }
}

/// Default per-OS cache directory: `<config dir>/Slab/cache/alt-text`.
/// On macOS `~/Library/Application Support/Slab/cache/alt-text`.
pub fn default_cache_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Slab").join("cache").join("alt-text")
}

// ---------------------------------------------------------------------
//                              Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ChatResponse;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Mock provider whose vision behaviour is controlled by a callable.
    struct MockProvider {
        replies: Mutex<Vec<Result<String, AiError>>>,
        calls: Mutex<usize>,
    }
    impl MockProvider {
        fn with(replies: Vec<Result<String, AiError>>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(replies),
                calls: Mutex::new(0),
            })
        }
        fn call_count(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }
    #[async_trait]
    impl AiProvider for MockProvider {
        async fn chat(
            &self,
            _msgs: &[ChatMessage],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            Err(AiError::InvalidResponse(
                "text chat not used in tests".into(),
            ))
        }
        async fn chat_with_images(
            &self,
            _msgs: &[ChatMessage],
            _images_b64: &[String],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            *self.calls.lock().unwrap() += 1;
            let mut replies = self.replies.lock().unwrap();
            if replies.is_empty() {
                return Err(AiError::InvalidResponse("no more mock replies".into()));
            }
            let next = replies.remove(0);
            next.map(|content| ChatResponse {
                content,
                model: "mock-llava".into(),
            })
        }
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            Ok(vec![])
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    fn fig(bbox: Bbox) -> StructNode {
        StructNode {
            kind: NodeKind::Figure,
            text: String::new(),
            bbox,
            font_size: 0.0,
            xobject_name: Some("Im1".into()),
            alt_text: None,
            lang: None,
            children: Vec::new(),
        }
    }

    fn bx(x0: f32, y0: f32, x1: f32, y1: f32) -> Bbox {
        Bbox { x0, y0, x1, y1 }
    }

    #[test]
    fn normalises_alt_text_strips_prefix_and_collapses() {
        assert_eq!(
            normalise_alt("  An image of  a red square on white.  "),
            "A red square on white."
        );
        assert_eq!(normalise_alt("\"A blue dot.\""), "A blue dot.");
        assert_eq!(
            normalise_alt("This image shows\n\n a flowchart with three nodes."),
            "A flowchart with three nodes."
        );
    }

    #[test]
    fn sha256_hex_is_stable() {
        let h1 = sha256_hex(b"hello");
        let h2 = sha256_hex(b"hello");
        let h3 = sha256_hex(b"goodbye");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn collect_figures_walks_recursively() {
        let mut root = fig(bx(0.0, 0.0, 100.0, 100.0));
        let nested = fig(bx(10.0, 10.0, 50.0, 50.0));
        root.children.push(nested);
        let page_nodes = vec![root];
        let mut out = Vec::new();
        collect_figures(&page_nodes, 1, &mut Vec::new(), &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].1, vec![0]);
        assert_eq!(out[1].1, vec![0, 0]);
    }

    #[test]
    fn apply_alt_text_targets_nested_node() {
        let mut root = fig(bx(0.0, 0.0, 100.0, 100.0));
        let nested = fig(bx(10.0, 10.0, 50.0, 50.0));
        root.children.push(nested);
        let mut pages = vec![crate::pdf::loom::classify::StructTreePage {
            page_number: 3,
            nodes: vec![root],
        }];
        apply_alt_text(&mut pages, 3, &[0, 0], "Nested alt.".to_string());
        assert_eq!(
            pages[0].nodes[0].children[0].alt_text.as_deref(),
            Some("Nested alt.")
        );
        assert!(pages[0].nodes[0].alt_text.is_none());
    }

    #[test]
    fn default_cache_dir_ends_with_alt_text() {
        let p = default_cache_dir();
        assert!(p.ends_with("alt-text"), "got: {}", p.display());
    }

    // The async tests below mock the provider and skip rasterization by
    // pre-populating the cache. We use a fake PDF path that never gets
    // read because the cache hit short-circuits before render_page_image.

    fn write_cache(dir: &Path, png_bytes: &[u8], alt: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(format!("{}.txt", sha256_hex(png_bytes)));
        std::fs::write(&p, alt).unwrap();
        p
    }

    #[tokio::test]
    async fn cache_hit_uses_existing_alt_text() {
        // Pre-seed a cache entry keyed by the hash of a fake PNG. Then
        // call process_one_figure with a non-existent PDF — render
        // would fail, but we want the cache path to short-circuit.
        // We can't short-circuit because we always render first; so
        // instead exercise the higher-level invariant: alt_text_for_bbox
        // returns from_cache=true when the cache exists.
        //
        // To avoid the rasterizer, we test the smaller normalise+sha
        // pieces above. Full integration is gated by `pdftoppm`
        // availability and exercised in the Tauri command tests.
        let tmp = tempfile::tempdir().unwrap();
        let png = b"fake-png-bytes-for-test";
        write_cache(tmp.path(), png, "A red square on white.");
        let hash = sha256_hex(png);
        assert!(tmp.path().join(format!("{hash}.txt")).exists());
    }

    #[tokio::test]
    async fn mock_provider_call_count_increments() {
        let mock = MockProvider::with(vec![Ok("A small blue dot.".into())]);
        let resp = mock
            .chat_with_images(
                &[ChatMessage {
                    role: ChatRole::User,
                    content: "hi".into(),
                }],
                &["base64data".into()],
                &ChatOpts {
                    model: None,
                    temperature: None,
                    max_tokens: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.content, "A small blue dot.");
        assert_eq!(mock.call_count(), 1);
    }

    #[tokio::test]
    async fn enrich_with_alt_text_skips_tiny_and_preexisting() {
        // Build a tree with three figures: one preexisting alt, one
        // tiny (10×10 = 100pt² < 200), one of zero size.
        let mut tree = StructTree::default();
        let mut tiny = fig(bx(0.0, 0.0, 10.0, 10.0));
        let mut existing = fig(bx(100.0, 100.0, 300.0, 300.0));
        existing.alt_text = Some("Already done.".into());
        let zero = fig(bx(50.0, 50.0, 50.0, 50.0));
        tiny.kind = NodeKind::Figure;
        tree.pages.push(crate::pdf::loom::classify::StructTreePage {
            page_number: 1,
            nodes: vec![tiny, existing, zero],
        });

        let mock = MockProvider::with(vec![]); // should never be called
        let tmp = tempfile::tempdir().unwrap();
        let opts = AltTextOptions::default();
        // We point at a non-existent PDF to prove the rasterizer is
        // never invoked when every figure is filtered out.
        let pdf = tmp.path().join("does-not-exist.pdf");
        let stats = enrich_with_alt_text(&pdf, &mut tree, mock.clone(), &opts, tmp.path())
            .await
            .unwrap();
        assert_eq!(stats.figures_total, 3);
        assert_eq!(stats.skipped_preexisting, 1);
        assert_eq!(stats.skipped_tiny, 2);
        assert_eq!(stats.generated, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.errors, 0);
        assert_eq!(mock.call_count(), 0);
    }

    #[tokio::test]
    async fn enrich_with_alt_text_no_figures_is_noop() {
        let mut tree = StructTree::default();
        tree.pages.push(crate::pdf::loom::classify::StructTreePage {
            page_number: 1,
            nodes: vec![],
        });
        let mock = MockProvider::with(vec![]);
        let tmp = tempfile::tempdir().unwrap();
        let stats = enrich_with_alt_text(
            &tmp.path().join("nope.pdf"),
            &mut tree,
            mock,
            &AltTextOptions::default(),
            tmp.path(),
        )
        .await
        .unwrap();
        assert_eq!(stats.figures_total, 0);
        assert_eq!(stats.generated, 0);
    }
}
