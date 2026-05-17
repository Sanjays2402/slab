//! Vision Q&A — rasterize a PDF page (optionally cropped to a rect),
//! base64-encode the PNG, hand it to the AiProvider's image-aware chat
//! method, and return a buffered reply.
//!
//! Why a separate module: keeps the rasterize+crop pipeline out of the
//! provider trait. Providers stay narrow ("take images, run a prompt"),
//! and the PDF-specific knowledge (`pdftoppm`, point/pixel math, the
//! Beacon system prompt) lives next to the rest of the orchestration
//! code.
//!
//! Wire format: Ollama's `/api/chat` accepts a `messages[].images: [base64]`
//! field that attaches to the most-recent user turn. OpenAI-compatible
//! providers use a different `content: [{type: "image_url", ...}]` shape
//! that we leave for v0.13.1 — the default trait impl rejects vision so
//! users get a clean error rather than a confused stack trace.

use crate::ai::{AiError, AiProvider, ChatMessage, ChatOpts, ChatRole};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

/// User-tunable vision knobs. `Default::default()` is what 99% of the
/// UI sends — only the advanced settings panel exposes the knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionOpts {
    /// Rasterization DPI. 150 is the llava sweet spot: detailed enough
    /// for diagrams + small captions, cheap enough on tokens.
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    /// Max image edge in pixels. If the rendered page is larger we
    /// downscale (Triangle filter) before sending — keeps Ollama
    /// happy and request bodies manageable.
    #[serde(default = "default_max_edge")]
    pub max_edge_px: u32,
}

fn default_dpi() -> u32 {
    150
}
fn default_max_edge() -> u32 {
    1568
}

impl Default for VisionOpts {
    fn default() -> Self {
        Self {
            dpi: default_dpi(),
            max_edge_px: default_max_edge(),
        }
    }
}

/// A rectangle on a PDF page, in *PDF points* (`pt`), bottom-left origin.
/// Matches the convention used everywhere else in Slab (`pdf::crop`,
/// `pdf::redact`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RectPts {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Final reply surfaced to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionReply {
    pub content: String,
    pub model: String,
    pub page: u32,
    pub rect_pts: Option<RectPts>,
    /// Final image dimensions (after crop+downscale) — useful for UI
    /// debugging and a "🖼️ p3 · 1568×2048" chip in the chat history.
    pub image_width: u32,
    pub image_height: u32,
}

/// System prompt that scopes Beacon to the supplied page image.
const SYSTEM_PROMPT: &str =
    "You are Beacon, an AI assistant looking at a PDF page rendered as an \
     image. Answer the user's question grounded in what's visible. If the \
     question can't be answered from the image, say so plainly. Keep \
     responses concise unless asked for detail.";

/// Render `page` (1-indexed) of `input` to PNG bytes. If `rect_pts` is
/// provided, crop to that rectangle (in PDF points, bottom-left origin)
/// before downscaling.
///
/// Side effects: shells out to `pdftoppm`. The temp PNG is read into
/// memory and the temp dir is dropped on return.
pub fn render_page_image(
    input: &Path,
    page: u32,
    rect_pts: Option<RectPts>,
    opts: &VisionOpts,
) -> Result<Vec<u8>, AiError> {
    if page == 0 {
        return Err(AiError::InvalidResponse(
            "page must be 1-indexed (got 0)".into(),
        ));
    }
    if !input.exists() {
        return Err(AiError::InvalidResponse(format!(
            "input not found: {}",
            input.display()
        )));
    }

    let tmp = tempfile::tempdir()
        .map_err(|e| AiError::Network(format!("create temp dir: {e}")))?;
    let prefix = tmp.path().join("page");
    let status = Command::new("pdftoppm")
        .arg("-r")
        .arg(opts.dpi.to_string())
        .arg("-f")
        .arg(page.to_string())
        .arg("-l")
        .arg(page.to_string())
        .arg("-png")
        .arg(input)
        .arg(&prefix)
        .status()
        .map_err(|e| {
            AiError::ProviderUnavailable(format!(
                "pdftoppm not found on PATH ({e}). On macOS: `brew install poppler`."
            ))
        })?;
    if !status.success() {
        return Err(AiError::InvalidResponse(format!(
            "pdftoppm exited {}",
            status.code().unwrap_or(-1)
        )));
    }

    // pdftoppm names the file like `page-5.png` or `page-05.png` depending
    // on page-number padding. With -f/-l pinned we expect exactly one PNG.
    let mut pngs: Vec<_> = std::fs::read_dir(tmp.path())
        .map_err(|e| AiError::Network(format!("read temp dir: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    pngs.sort();
    let png_path = pngs.into_iter().next().ok_or_else(|| {
        AiError::InvalidResponse("pdftoppm produced no PNG (bad page index?)".into())
    })?;

    let bytes = std::fs::read(&png_path)
        .map_err(|e| AiError::Network(format!("read png: {e}")))?;
    let mut img = image::load_from_memory(&bytes)
        .map_err(|e| AiError::InvalidResponse(format!("decode png: {e}")))?;

    // Optional crop. Convert PDF pts → image px using the same DPI.
    // Image origin is top-left; PDF origin is bottom-left, so we flip y.
    if let Some(r) = rect_pts {
        let scale = opts.dpi as f32 / 72.0;
        let img_w = img.width();
        let img_h = img.height();
        let x = ((r.x * scale).max(0.0) as u32).min(img_w);
        let rect_w_px = (r.width * scale) as u32;
        let rect_h_px = (r.height * scale) as u32;
        let y_top = img_h.saturating_sub((r.y * scale) as u32 + rect_h_px);
        // Clamp to image bounds so a too-wide rect doesn't panic.
        let crop_w = rect_w_px.min(img_w.saturating_sub(x));
        let crop_h = rect_h_px.min(img_h.saturating_sub(y_top));
        if crop_w == 0 || crop_h == 0 {
            return Err(AiError::InvalidResponse(
                "rect_pts crops to zero area — check coords".into(),
            ));
        }
        img = img.crop_imm(x, y_top, crop_w, crop_h);
    }

    // Downscale if either edge exceeds the cap.
    let max_edge = img.width().max(img.height());
    if max_edge > opts.max_edge_px {
        let ratio = opts.max_edge_px as f32 / max_edge as f32;
        let new_w = ((img.width() as f32 * ratio) as u32).max(1);
        let new_h = ((img.height() as f32 * ratio) as u32).max(1);
        img = img.resize(new_w, new_h, image::imageops::FilterType::Triangle);
    }

    // Re-encode as PNG.
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| AiError::InvalidResponse(format!("encode png: {e}")))?;
    Ok(out.into_inner())
}

/// Run a vision Q&A turn against `input` page `page`.
/// `rect_pts` is optional — when supplied, the image is cropped to that
/// region before being sent.
///
/// Returns a buffered `VisionReply`. Streaming follow-up is tracked
/// for v0.13.1 once the Tauri event channel is wired uniformly across
/// all Beacon commands.
pub async fn vision_ask(
    provider: Arc<dyn AiProvider>,
    input: &Path,
    page: u32,
    rect_pts: Option<RectPts>,
    prompt: &str,
    history: &[ChatMessage],
    opts: &VisionOpts,
) -> Result<VisionReply, AiError> {
    let png = render_page_image(input, page, rect_pts, opts)?;
    // Decode again to get the post-crop+downscale dims for the reply.
    // Cheap because `image` already decoded it once; this round-trip
    // costs an extra decode but keeps the API single-purpose.
    let dims = image::load_from_memory(&png)
        .map_err(|e| AiError::InvalidResponse(format!("decode rendered png: {e}")))?;
    let (w, h) = (dims.width(), dims.height());
    let b64 = B64.encode(&png);

    let mut msgs: Vec<ChatMessage> = Vec::with_capacity(history.len() + 2);
    msgs.push(ChatMessage {
        role: ChatRole::System,
        content: SYSTEM_PROMPT.into(),
    });
    for m in history {
        msgs.push(m.clone());
    }
    msgs.push(ChatMessage {
        role: ChatRole::User,
        content: prompt.to_string(),
    });

    let chat_opts = ChatOpts {
        model: None,
        temperature: Some(0.2),
        max_tokens: Some(800),
    };
    let resp = provider.chat_with_images(&msgs, &[b64], &chat_opts).await?;
    Ok(VisionReply {
        content: resp.content,
        model: resp.model,
        page,
        rect_pts,
        image_width: w,
        image_height: h,
    })
}

/// Probe whether `pdftoppm` is available — used by tests to gate the
/// expensive rasterize integration test.
#[cfg(test)]
fn pdftoppm_available() -> bool {
    Command::new("pdftoppm")
        .arg("-v")
        .output()
        .map(|o| o.status.success() || !o.stderr.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatResponse, ChatRole};
    use async_trait::async_trait;
    use std::sync::Mutex;

    fn write_test_pdf(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        crate::pdf::test_fixtures::make_n_page_pdf(&path, 1);
        path
    }

    #[test]
    fn vision_opts_defaults_sensible() {
        let o = VisionOpts::default();
        assert_eq!(o.dpi, 150);
        assert_eq!(o.max_edge_px, 1568);
    }

    #[test]
    fn render_page_zero_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let pdf = write_test_pdf(tmp.path(), "one.pdf");
        let err = render_page_image(&pdf, 0, None, &VisionOpts::default()).unwrap_err();
        match err {
            AiError::InvalidResponse(m) => assert!(m.contains("1-indexed")),
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    #[test]
    fn render_missing_input_errors() {
        let p = std::path::Path::new("/tmp/definitely-not-here-xyz-slab.pdf");
        let err = render_page_image(p, 1, None, &VisionOpts::default()).unwrap_err();
        match err {
            AiError::InvalidResponse(m) => assert!(m.contains("not found")),
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    #[test]
    fn render_page_one_produces_png_bytes() {
        if !pdftoppm_available() {
            eprintln!("skip: pdftoppm not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let pdf = write_test_pdf(tmp.path(), "doc.pdf");
        let bytes = render_page_image(&pdf, 1, None, &VisionOpts::default())
            .expect("render must succeed when pdftoppm is present");
        // PNG magic: 89 50 4E 47 0D 0A 1A 0A
        assert!(bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]));
        // Should be > 100 bytes — a real page render, not a stub.
        assert!(bytes.len() > 100, "got {} bytes", bytes.len());
    }

    #[test]
    fn render_with_rect_crops_smaller() {
        if !pdftoppm_available() {
            eprintln!("skip: pdftoppm not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let pdf = write_test_pdf(tmp.path(), "doc.pdf");
        let full = render_page_image(&pdf, 1, None, &VisionOpts::default()).unwrap();
        let rect = RectPts {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let cropped =
            render_page_image(&pdf, 1, Some(rect), &VisionOpts::default()).unwrap();
        assert!(cropped.starts_with(&[0x89, 0x50, 0x4E, 0x47]));
        assert!(
            cropped.len() < full.len(),
            "cropped {} should be < full {}",
            cropped.len(),
            full.len()
        );
    }

    #[test]
    fn render_with_zero_area_rect_errors() {
        if !pdftoppm_available() {
            eprintln!("skip: pdftoppm not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let pdf = write_test_pdf(tmp.path(), "doc.pdf");
        // Rect way past the page's right edge → clamp to zero width.
        let rect = RectPts {
            x: 10_000.0,
            y: 10_000.0,
            width: 10.0,
            height: 10.0,
        };
        let err = render_page_image(&pdf, 1, Some(rect), &VisionOpts::default()).unwrap_err();
        match err {
            AiError::InvalidResponse(m) => assert!(m.contains("zero area")),
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
    }

    #[test]
    fn render_downscales_to_max_edge() {
        if !pdftoppm_available() {
            eprintln!("skip: pdftoppm not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let pdf = write_test_pdf(tmp.path(), "doc.pdf");
        // Constrain to a tiny max edge → image should still decode and
        // be no larger than max_edge_px on its long axis.
        let opts = VisionOpts {
            dpi: 150,
            max_edge_px: 200,
        };
        let bytes = render_page_image(&pdf, 1, None, &opts).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        assert!(img.width() <= 200 && img.height() <= 200, "got {}x{}", img.width(), img.height());
    }

    // --- Orchestrator test (mock provider, no network) -----------------

    struct CapturingProvider {
        last_images: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl AiProvider for CapturingProvider {
        async fn chat(
            &self,
            _msgs: &[ChatMessage],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            unreachable!("vision path should not hit text chat()")
        }
        async fn chat_with_images(
            &self,
            _msgs: &[ChatMessage],
            images_b64: &[String],
            _opts: &ChatOpts,
        ) -> Result<ChatResponse, AiError> {
            *self.last_images.lock().unwrap() = images_b64.to_vec();
            Ok(ChatResponse {
                content: "looks like a page".into(),
                model: "mock-vision".into(),
            })
        }
        async fn embed(&self, _t: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
            unreachable!()
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn vision_ask_passes_image_to_provider() {
        if !pdftoppm_available() {
            eprintln!("skip: pdftoppm not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let pdf = write_test_pdf(tmp.path(), "v.pdf");

        let prov_typed = Arc::new(CapturingProvider {
            last_images: Mutex::new(Vec::new()),
        });
        let prov: Arc<dyn AiProvider> = prov_typed.clone();

        let reply = vision_ask(
            prov,
            &pdf,
            1,
            None,
            "what's on this page?",
            &[],
            &VisionOpts::default(),
        )
        .await
        .unwrap();

        assert_eq!(reply.content, "looks like a page");
        assert_eq!(reply.model, "mock-vision");
        assert_eq!(reply.page, 1);
        assert!(reply.image_width > 0 && reply.image_height > 0);
        assert!(reply.rect_pts.is_none());

        let imgs = prov_typed.last_images.lock().unwrap().clone();
        assert_eq!(imgs.len(), 1, "provider should receive exactly 1 image");
        // Base64-encoded PNG always starts with `iVBOR` (PNG magic in b64).
        assert!(
            imgs[0].starts_with("iVBOR"),
            "expected b64-PNG prefix, got {}",
            &imgs[0][..30.min(imgs[0].len())]
        );
    }

    #[tokio::test]
    async fn vision_ask_threads_rect_into_reply() {
        if !pdftoppm_available() {
            eprintln!("skip: pdftoppm not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let pdf = write_test_pdf(tmp.path(), "v.pdf");
        let prov_typed = Arc::new(CapturingProvider {
            last_images: Mutex::new(Vec::new()),
        });
        let prov: Arc<dyn AiProvider> = prov_typed.clone();
        let rect = RectPts {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let reply = vision_ask(
            prov,
            &pdf,
            1,
            Some(rect),
            "what's here?",
            &[],
            &VisionOpts::default(),
        )
        .await
        .unwrap();
        let returned = reply.rect_pts.expect("rect should round-trip");
        assert!((returned.width - 100.0).abs() < f32::EPSILON);
        assert!((returned.height - 100.0).abs() < f32::EPSILON);
    }
}
