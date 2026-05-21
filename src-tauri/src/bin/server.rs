// slab-server — HTTP daemon exposing Slab's PDF operations over REST.
//
// v2.1.0 "Server" 🐳 — same Rust core as the Tauri desktop app, but
// headless. Pair with the Dockerfile at the repo root for a one-step
// homelab / NAS / Compose deployment:
//
//     docker run --rm -p 7300:7300 ghcr.io/sanjays2402/slab:2.1.0
//
// Endpoints are documented in `docs/server.md` (and at `/` in the
// bundled UI). All operations are stateless: upload PDF → server runs
// op on a per-request temp dir → server streams the result back and
// wipes the temp dir. No PDFs ever persist on the server unless the
// operator configures `SLAB_DATA_DIR` for the embedding cache.

#![cfg(feature = "server")]

use std::env;
use std::io::{Cursor, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Instant;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Multipart},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use slab_lib::pdf;

// ───────────────────────────────────────────────────────────────────
// embedded frontend
// ───────────────────────────────────────────────────────────────────

const INDEX_HTML: &str = include_str!("../../resources/server/index.html");

// ───────────────────────────────────────────────────────────────────
// types
// ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
    op: Option<String>,
}

struct ApiError {
    status: StatusCode,
    body: ErrorBody,
}

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ErrorBody {
                error: msg.into(),
                op: None,
            },
        }
    }

    fn internal(op: &str, msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorBody {
                error: msg.into(),
                op: Some(op.to_string()),
            },
        }
    }

    fn payload_too_large(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            body: ErrorBody {
                error: msg.into(),
                op: None,
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

// Shared limits — env-overridable for hosts with bigger PDFs to serve.
fn body_limit_bytes() -> usize {
    env::var("SLAB_MAX_UPLOAD_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200)
        * 1024
        * 1024
}

// ───────────────────────────────────────────────────────────────────
// multipart helpers
// ───────────────────────────────────────────────────────────────────

struct UploadedPdf {
    bytes: Bytes,
    filename: String,
}

struct ParsedUpload {
    pdfs: Vec<UploadedPdf>,
    fields: std::collections::HashMap<String, String>,
}

async fn parse_multipart(mut mp: Multipart) -> Result<ParsedUpload, ApiError> {
    let mut pdfs = Vec::new();
    let mut fields = std::collections::HashMap::new();

    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("multipart parse: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().map(|s| s.to_string());

        if let Some(fname) = filename {
            // File field
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("read file '{name}': {e}")))?;
            if data.len() > body_limit_bytes() {
                return Err(ApiError::payload_too_large(format!(
                    "file '{name}' exceeds SLAB_MAX_UPLOAD_MB"
                )));
            }
            pdfs.push(UploadedPdf {
                bytes: data,
                filename: fname,
            });
        } else {
            // Text field
            let v = field
                .text()
                .await
                .map_err(|e| ApiError::bad_request(format!("read field '{name}': {e}")))?;
            fields.insert(name, v);
        }
    }
    Ok(ParsedUpload { pdfs, fields })
}

fn ensure_pdf(pdfs: &[UploadedPdf]) -> Result<&UploadedPdf, ApiError> {
    pdfs.first()
        .ok_or_else(|| ApiError::bad_request("missing 'file' field with a PDF upload"))
}

fn write_temp(pdf: &UploadedPdf) -> Result<(tempfile::TempDir, PathBuf), ApiError> {
    let dir = tempfile::tempdir()
        .map_err(|e| ApiError::internal("temp", format!("tempdir: {e}")))?;
    let in_path = dir.path().join(&pdf.filename);
    std::fs::write(&in_path, &pdf.bytes)
        .map_err(|e| ApiError::internal("temp", format!("write upload: {e}")))?;
    Ok((dir, in_path))
}

fn pdf_filename(original: &str, suffix: &str) -> String {
    let stem = Path::new(original)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "slab".to_string());
    format!("{stem}-{suffix}.pdf")
}

fn pdf_response(path: &Path, download_name: &str) -> Result<Response, ApiError> {
    let bytes = std::fs::read(path)
        .map_err(|e| ApiError::internal("io", format!("read result: {e}")))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{download_name}\""))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}

// ───────────────────────────────────────────────────────────────────
// routes
// ───────────────────────────────────────────────────────────────────

async fn index() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], INDEX_HTML)
}

async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "slab-server",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[derive(Debug, Serialize)]
struct OpsDescriptor {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    description: &'static str,
    fields: &'static [&'static str],
}

const OPS: &[OpsDescriptor] = &[
    OpsDescriptor {
        name: "merge",
        method: "POST",
        path: "/api/v1/merge",
        description: "Merge multiple PDFs in upload order into a single PDF.",
        fields: &["file (repeatable, at least 2)"],
    },
    OpsDescriptor {
        name: "split-every",
        method: "POST",
        path: "/api/v1/split-every",
        description: "Split a PDF into chunks of N pages each. Returns a ZIP of chunks.",
        fields: &["file", "chunk_size (int, default 1)"],
    },
    OpsDescriptor {
        name: "split-ranges",
        method: "POST",
        path: "/api/v1/split-ranges",
        description: "Split a PDF by 1-based page ranges (e.g. '1-3,5,7-9'). Returns a ZIP.",
        fields: &["file", "ranges (string)"],
    },
    OpsDescriptor {
        name: "rotate",
        method: "POST",
        path: "/api/v1/rotate",
        description: "Rotate selected pages by 90/180/270 degrees.",
        fields: &["file", "pages (csv 1-based, e.g. '1,3,5')", "degrees (90|180|270|-90)"],
    },
    OpsDescriptor {
        name: "delete-pages",
        method: "POST",
        path: "/api/v1/delete-pages",
        description: "Delete listed pages from the PDF (1-based, csv).",
        fields: &["file", "pages (csv)"],
    },
    OpsDescriptor {
        name: "reorder-pages",
        method: "POST",
        path: "/api/v1/reorder-pages",
        description: "Reorder pages to match the given 1-based csv order. The list must cover every page exactly once.",
        fields: &["file", "order (csv)"],
    },
    OpsDescriptor {
        name: "compress",
        method: "POST",
        path: "/api/v1/compress",
        description: "Recompress object streams to shrink the PDF without re-rendering.",
        fields: &["file"],
    },
    OpsDescriptor {
        name: "encrypt",
        method: "POST",
        path: "/api/v1/encrypt",
        description: "Encrypt the PDF with the given password (40-bit RC4 fallback for Acrobat compat).",
        fields: &["file", "password"],
    },
    OpsDescriptor {
        name: "decrypt",
        method: "POST",
        path: "/api/v1/decrypt",
        description: "Remove encryption from a password-protected PDF.",
        fields: &["file", "password"],
    },
    OpsDescriptor {
        name: "watermark",
        method: "POST",
        path: "/api/v1/watermark",
        description: "Stamp a diagonal text watermark on every page.",
        fields: &["file", "text", "opacity (0.0-1.0, default 0.3)"],
    },
    OpsDescriptor {
        name: "extract-text",
        method: "POST",
        path: "/api/v1/extract-text",
        description: "Extract plain text from every page; returns JSON {pages: [...]}.",
        fields: &["file"],
    },
    OpsDescriptor {
        name: "info",
        method: "POST",
        path: "/api/v1/info",
        description: "Return PDF metadata: title, author, page count, size, encryption flag.",
        fields: &["file"],
    },
    OpsDescriptor {
        name: "page-count",
        method: "POST",
        path: "/api/v1/page-count",
        description: "Return the page count of a PDF as JSON {pages: N}.",
        fields: &["file"],
    },
    OpsDescriptor {
        name: "strip-metadata",
        method: "POST",
        path: "/api/v1/strip-metadata",
        description: "Strip all author/title/producer metadata from the PDF.",
        fields: &["file"],
    },
];

async fn ops_index() -> impl IntoResponse {
    Json(json!({
        "service": "slab-server",
        "version": env!("CARGO_PKG_VERSION"),
        "operations": OPS,
    }))
}

// ───────────────────────────────────────────────────────────────────
// PDF op handlers
// ───────────────────────────────────────────────────────────────────

fn parse_csv_u32(s: &str) -> Result<Vec<u32>, ApiError> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|t| t.parse::<u32>().map_err(|e| ApiError::bad_request(format!("invalid number '{t}': {e}"))))
        .collect()
}

async fn h_merge(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    if up.pdfs.len() < 2 {
        return Err(ApiError::bad_request(
            "merge requires at least 2 'file' uploads",
        ));
    }
    let dir = tempfile::tempdir()
        .map_err(|e| ApiError::internal("temp", format!("tempdir: {e}")))?;
    let mut paths = Vec::with_capacity(up.pdfs.len());
    for (i, p) in up.pdfs.iter().enumerate() {
        let dest = dir.path().join(format!("in-{i:03}-{}", p.filename));
        std::fs::write(&dest, &p.bytes)
            .map_err(|e| ApiError::internal("temp", format!("write input {i}: {e}")))?;
        paths.push(dest);
    }
    let out = dir.path().join("merged.pdf");
    pdf::merge::merge_pdfs(&paths, out.clone())
        .map_err(|e| ApiError::internal("merge", format!("{e:?}")))?;
    pdf_response(&out, "slab-merged.pdf")
}

async fn h_split_every(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let chunk = up
        .fields
        .get("chunk_size")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    if chunk == 0 {
        return Err(ApiError::bad_request("chunk_size must be >= 1"));
    }
    let (dir, in_path) = write_temp(pdf_in)?;
    let out_dir = dir.path().join("out");
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| ApiError::internal("split", format!("mkdir: {e}")))?;
    let chunks = pdf::split::split_every(&in_path, chunk, &out_dir)
        .map_err(|e| ApiError::internal("split-every", format!("{e:?}")))?;
    zip_files(&chunks, &pdf_filename(&pdf_in.filename, "split"))
}

async fn h_split_ranges(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let ranges = up
        .fields
        .get("ranges")
        .ok_or_else(|| ApiError::bad_request("missing 'ranges' field"))?;
    let parsed = parse_ranges(ranges)?;
    let page_ranges: Vec<pdf::split::PageRange> = parsed
        .into_iter()
        .map(|(s, e)| pdf::split::PageRange::new(s, e))
        .collect::<Result<_, _>>()
        .map_err(|e| ApiError::bad_request(format!("range parse: {e:?}")))?;
    let (dir, in_path) = write_temp(pdf_in)?;
    let out_dir = dir.path().join("out");
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| ApiError::internal("split", format!("mkdir: {e}")))?;
    let chunks = pdf::split::split_by_ranges(&in_path, &page_ranges, &out_dir)
        .map_err(|e| ApiError::internal("split-ranges", format!("{e:?}")))?;
    zip_files(&chunks, &pdf_filename(&pdf_in.filename, "ranges"))
}

fn parse_ranges(s: &str) -> Result<Vec<(u32, u32)>, ApiError> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|part| {
            if let Some((a, b)) = part.split_once('-') {
                let a = a
                    .trim()
                    .parse::<u32>()
                    .map_err(|e| ApiError::bad_request(format!("range '{part}': {e}")))?;
                let b = b
                    .trim()
                    .parse::<u32>()
                    .map_err(|e| ApiError::bad_request(format!("range '{part}': {e}")))?;
                if a == 0 || b < a {
                    return Err(ApiError::bad_request(format!("invalid range '{part}'")));
                }
                Ok((a, b))
            } else {
                let p = part
                    .parse::<u32>()
                    .map_err(|e| ApiError::bad_request(format!("page '{part}': {e}")))?;
                if p == 0 {
                    return Err(ApiError::bad_request("pages are 1-based"));
                }
                Ok((p, p))
            }
        })
        .collect()
}

fn zip_files(files: &[PathBuf], download_name: &str) -> Result<Response, ApiError> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opts: zip::write::SimpleFileOptions =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for f in files {
            let name = f
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("part.pdf");
            zw.start_file(name, opts)
                .map_err(|e| ApiError::internal("zip", format!("start_file: {e}")))?;
            let bytes = std::fs::read(f)
                .map_err(|e| ApiError::internal("zip", format!("read part: {e}")))?;
            zw.write_all(&bytes)
                .map_err(|e| ApiError::internal("zip", format!("write part: {e}")))?;
        }
        zw.finish()
            .map_err(|e| ApiError::internal("zip", format!("finish: {e}")))?;
    }
    let bytes = buf.into_inner();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{download_name}.zip\""))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}

async fn h_rotate(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let pages_csv = up
        .fields
        .get("pages")
        .ok_or_else(|| ApiError::bad_request("missing 'pages' field"))?;
    let pages = parse_csv_u32(pages_csv)?;
    let degrees: i64 = up
        .fields
        .get("degrees")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiError::bad_request("missing or invalid 'degrees' field"))?;
    let rot = pdf::pages::Rotation::from_int(degrees)
        .map_err(|e| ApiError::bad_request(format!("degrees must be 90, 180, 270, or -90: {e:?}")))?;
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    pdf::pages::rotate_pages(&in_path, &pages, rot, &out)
        .map_err(|e| ApiError::internal("rotate", format!("{e:?}")))?;
    pdf_response(&out, &pdf_filename(&pdf_in.filename, "rotated"))
}

async fn h_delete_pages(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let pages_csv = up
        .fields
        .get("pages")
        .ok_or_else(|| ApiError::bad_request("missing 'pages' field"))?;
    let pages = parse_csv_u32(pages_csv)?;
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    pdf::pages::delete_pages(&in_path, &pages, &out)
        .map_err(|e| ApiError::internal("delete-pages", format!("{e:?}")))?;
    pdf_response(&out, &pdf_filename(&pdf_in.filename, "trimmed"))
}

async fn h_reorder_pages(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let order_csv = up
        .fields
        .get("order")
        .ok_or_else(|| ApiError::bad_request("missing 'order' field"))?;
    let order = parse_csv_u32(order_csv)?;
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    pdf::pages::reorder_pages(&in_path, &order, &out)
        .map_err(|e| ApiError::internal("reorder-pages", format!("{e:?}")))?;
    pdf_response(&out, &pdf_filename(&pdf_in.filename, "reordered"))
}

async fn h_compress(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    let report = pdf::compress::compress(&in_path, &out)
        .map_err(|e| ApiError::internal("compress", format!("{e:?}")))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            pdf_filename(&pdf_in.filename, "compressed")
        ))
        .unwrap_or(HeaderValue::from_static("attachment")),
    );
    headers.insert(
        "x-slab-bytes-before",
        HeaderValue::from_str(&report.original_bytes.to_string()).unwrap(),
    );
    headers.insert(
        "x-slab-bytes-after",
        HeaderValue::from_str(&report.new_bytes.to_string()).unwrap(),
    );
    let bytes = std::fs::read(&out)
        .map_err(|e| ApiError::internal("io", format!("read result: {e}")))?;
    Ok((StatusCode::OK, headers, bytes).into_response())
}

async fn h_encrypt(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let password = up
        .fields
        .get("password")
        .cloned()
        .ok_or_else(|| ApiError::bad_request("missing 'password' field"))?;
    if password.is_empty() {
        return Err(ApiError::bad_request("password must not be empty"));
    }
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    pdf::encrypt::encrypt(&in_path, &out, &password)
        .map_err(|e| ApiError::internal("encrypt", format!("{e:?}")))?;
    pdf_response(&out, &pdf_filename(&pdf_in.filename, "encrypted"))
}

async fn h_decrypt(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let password = up
        .fields
        .get("password")
        .cloned()
        .ok_or_else(|| ApiError::bad_request("missing 'password' field"))?;
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    pdf::encrypt::decrypt(&in_path, &out, &password)
        .map_err(|e| ApiError::internal("decrypt", format!("{e:?}")))?;
    pdf_response(&out, &pdf_filename(&pdf_in.filename, "decrypted"))
}

async fn h_watermark(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let text = up
        .fields
        .get("text")
        .cloned()
        .ok_or_else(|| ApiError::bad_request("missing 'text' field"))?;
    let opacity: f32 = up
        .fields
        .get("opacity")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.3);
    if !(0.0..=1.0).contains(&opacity) {
        return Err(ApiError::bad_request("opacity must be 0.0..=1.0"));
    }
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    // Empty pages slice = stamp every page (matches the desktop UX).
    let pages: Vec<u32> = Vec::new();
    let opts = pdf::watermark::WatermarkOpts {
        text: &text,
        opacity,
        font_size: 48.0,
        rotation_deg: -45.0,
        gray: 0.2,
    };
    pdf::watermark::watermark(&in_path, &out, opts, &pages)
        .map_err(|e| ApiError::internal("watermark", format!("{e:?}")))?;
    pdf_response(&out, &pdf_filename(&pdf_in.filename, "watermarked"))
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractTextOut {
    pages: Vec<String>,
}

async fn h_extract_text(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let (_dir, in_path) = write_temp(pdf_in)?;
    let pages = pdf::extract::extract_text(&in_path)
        .map_err(|e| ApiError::internal("extract-text", format!("{e:?}")))?;
    Ok(Json(ExtractTextOut { pages }).into_response())
}

async fn h_info(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let (_dir, in_path) = write_temp(pdf_in)?;
    let info = pdf::info::info(&in_path)
        .map_err(|e| ApiError::internal("info", format!("{e:?}")))?;
    Ok(Json(info).into_response())
}

async fn h_page_count(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let (_dir, in_path) = write_temp(pdf_in)?;
    let pages = pdf::split::page_count(&in_path)
        .map_err(|e| ApiError::internal("page-count", format!("{e:?}")))?;
    Ok(Json(json!({ "pages": pages })).into_response())
}

async fn h_strip_metadata(mp: Multipart) -> Result<Response, ApiError> {
    let up = parse_multipart(mp).await?;
    let pdf_in = ensure_pdf(&up.pdfs)?;
    let (dir, in_path) = write_temp(pdf_in)?;
    let out = dir.path().join("out.pdf");
    pdf::metadata::strip_metadata(&in_path, &out)
        .map_err(|e| ApiError::internal("strip-metadata", format!("{e:?}")))?;
    pdf_response(&out, &pdf_filename(&pdf_in.filename, "stripped"))
}

// ───────────────────────────────────────────────────────────────────
// app construction
// ───────────────────────────────────────────────────────────────────

fn build_app() -> Router {
    let body_cap = body_limit_bytes();
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_origin(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", get(index))
        .route("/healthz", get(health))
        .route("/api/v1/ops", get(ops_index))
        .route("/api/v1/merge", post(h_merge))
        .route("/api/v1/split-every", post(h_split_every))
        .route("/api/v1/split-ranges", post(h_split_ranges))
        .route("/api/v1/rotate", post(h_rotate))
        .route("/api/v1/delete-pages", post(h_delete_pages))
        .route("/api/v1/reorder-pages", post(h_reorder_pages))
        .route("/api/v1/compress", post(h_compress))
        .route("/api/v1/encrypt", post(h_encrypt))
        .route("/api/v1/decrypt", post(h_decrypt))
        .route("/api/v1/watermark", post(h_watermark))
        .route("/api/v1/extract-text", post(h_extract_text))
        .route("/api/v1/info", post(h_info))
        .route("/api/v1/page-count", post(h_page_count))
        .route("/api/v1/strip-metadata", post(h_strip_metadata))
        .layer(DefaultBodyLimit::max(body_cap))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing: respects RUST_LOG, defaults to info.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("slab_server=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let port: u16 = env::var("SLAB_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7300);
    let host: String = env::var("SLAB_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    let app = build_app();
    let started = Instant::now();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        addr = %addr,
        max_upload_mb = body_limit_bytes() / 1024 / 1024,
        "slab-server starting"
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "slab-server listening"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

// ───────────────────────────────────────────────────────────────────
// tests
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_ranges() {
        let r = parse_ranges("1-3,5,7-9").unwrap();
        assert_eq!(r, vec![(1, 3), (5, 5), (7, 9)]);
    }

    #[test]
    fn rejects_zero_page() {
        assert!(parse_ranges("0").is_err());
        assert!(parse_ranges("0-2").is_err());
    }

    #[test]
    fn rejects_reversed_range() {
        assert!(parse_ranges("5-1").is_err());
    }

    #[test]
    fn parses_csv_u32() {
        assert_eq!(parse_csv_u32("1, 2 ,3").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_csv_u32("").unwrap(), Vec::<u32>::new());
    }

    #[test]
    fn pdf_filename_stems() {
        assert_eq!(pdf_filename("report.pdf", "merged"), "report-merged.pdf");
        assert_eq!(pdf_filename("no-extension", "x"), "no-extension-x.pdf");
    }
}
