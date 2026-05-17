// Slab — fast, free, offline PDF tool.
// All operations run locally; nothing is ever uploaded.

pub mod ai;
pub mod pdf;

use ai::chat::{
    beacon_chat_from_path as do_beacon_chat, BeaconChatReply, DEFAULT_MAX_CONTEXT_CHARS,
};
use ai::config::{
    load as do_load_beacon_config, save as do_save_beacon_config, BeaconConfig, ProviderKind,
    SlabConfig,
};
use ai::{ChatMessage, ChatRole};

use pdf::annot_export::{
    extract as do_extract_annots, to_markdown as do_annots_to_md, ExtractedAnnotation,
};
use pdf::annotations::{append as do_append_annotations, Annotation};
use pdf::auto_redact::{auto_redact as do_auto_redact, AutoRedactOpts};
use pdf::compress::{compress as do_compress, CompressReport};
use pdf::crop::{crop as do_crop, CropOpts};
use pdf::encrypt::{decrypt as do_decrypt, encrypt as do_encrypt};
use pdf::extract::{extract_text as do_extract_text, extract_text_concat};
use pdf::flatten::{flatten as do_flatten, FlattenOpts, FlattenReport};
use pdf::grayscale::{grayscale as do_grayscale, GrayscaleOpts};
use pdf::header_footer::{apply as do_header_footer, HFOpts};
use pdf::info::{info as do_info, PdfInfo};
use pdf::insert::{insert as do_insert, InsertOpts};
use pdf::md2pdf::{render as do_md2pdf, Md2PdfOpts};
use pdf::merge::merge_pdfs;
use pdf::metadata::{
    read_metadata as do_read_metadata, strip_metadata as do_strip_metadata,
    write_metadata as do_write_metadata, Metadata,
};
use pdf::nup::{nup as do_nup, NupOpts};
use pdf::ocr::{ocr as do_ocr, OcrOpts, OcrReport};
use pdf::outline::{
    read_outline as do_read_outline, write_outline as do_write_outline, OutlineNode,
};
use pdf::page_labels::{apply as do_page_labels, PageLabelsOpts};
use pdf::page_numbers::{add_page_numbers as do_page_numbers, PageNumbersOpts};
use pdf::pages::{delete_pages, reorder_pages, rotate_pages, Rotation};
use pdf::polyglot::{polyglot_to_pdf as do_polyglot, PolyglotOpts, PolyglotReport};
use pdf::redact::{redact as do_redact, RedactOpts};
use pdf::repair::{repair as do_repair, RepairReport};
use pdf::sanitize::{sanitize as do_sanitize, SanitizeOpts, SanitizeReport};
use pdf::split::{page_count as do_page_count, split_by_ranges, split_every, PageRange};
use pdf::watermark::{watermark as do_watermark, WatermarkOpts};
use pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result returned from any Slab command. JSON-friendly for the Svelte side.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CmdResult<T: Serialize> {
    Ok { value: T },
    Err { message: String },
}

impl<T: Serialize> From<Result<T, PdfError>> for CmdResult<T> {
    fn from(r: Result<T, PdfError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Slab",
        version: env!("CARGO_PKG_VERSION"),
        commit: option_env!("SLAB_COMMIT").unwrap_or("dev"),
    }
}

#[tauri::command]
fn slab_merge(inputs: Vec<PathBuf>, output: PathBuf) -> CmdResult<PathBuf> {
    let out = output.clone();
    merge_pdfs(&inputs, output).map(|_| out).into()
}

#[derive(Deserialize)]
pub struct PageRangeDto {
    pub start: u32,
    pub end: u32,
}

#[tauri::command]
fn slab_split_ranges(
    input: PathBuf,
    ranges: Vec<PageRangeDto>,
    out_dir: PathBuf,
) -> CmdResult<Vec<PathBuf>> {
    let parsed: Result<Vec<PageRange>, PdfError> = ranges
        .iter()
        .map(|r| PageRange::new(r.start, r.end))
        .collect();
    match parsed {
        Ok(rs) => split_by_ranges(&input, &rs, &out_dir).into(),
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_split_every(input: PathBuf, chunk_size: u32, out_dir: PathBuf) -> CmdResult<Vec<PathBuf>> {
    split_every(&input, chunk_size, &out_dir).into()
}

#[tauri::command]
fn slab_page_count(input: PathBuf) -> CmdResult<u32> {
    do_page_count(&input).into()
}

#[tauri::command]
fn slab_rotate(input: PathBuf, pages: Vec<u32>, degrees: i64, output: PathBuf) -> CmdResult<u32> {
    match Rotation::from_int(degrees) {
        Ok(rot) => rotate_pages(&input, &pages, rot, &output).into(),
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_delete_pages(input: PathBuf, pages: Vec<u32>, output: PathBuf) -> CmdResult<u32> {
    delete_pages(&input, &pages, &output).into()
}

#[tauri::command]
fn slab_reorder_pages(input: PathBuf, order: Vec<u32>, output: PathBuf) -> CmdResult<()> {
    reorder_pages(&input, &order, &output).into()
}

#[tauri::command]
fn slab_extract_text(input: PathBuf) -> CmdResult<Vec<String>> {
    do_extract_text(&input).into()
}

#[tauri::command]
fn slab_extract_text_save(input: PathBuf, output: PathBuf) -> CmdResult<PathBuf> {
    match extract_text_concat(&input) {
        Ok(text) => match std::fs::write(&output, text) {
            Ok(()) => CmdResult::Ok { value: output },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_info(input: PathBuf) -> CmdResult<PdfInfo> {
    do_info(&input).into()
}

#[tauri::command]
fn slab_compress(input: PathBuf, output: PathBuf) -> CmdResult<CompressReport> {
    do_compress(&input, &output).into()
}

#[tauri::command]
fn slab_encrypt(input: PathBuf, output: PathBuf, password: String) -> CmdResult<()> {
    do_encrypt(&input, &output, &password).into()
}

#[tauri::command]
fn slab_decrypt(input: PathBuf, output: PathBuf, password: String) -> CmdResult<()> {
    do_decrypt(&input, &output, &password).into()
}

#[derive(Deserialize)]
pub struct WatermarkDto {
    pub text: String,
    pub opacity: f32,
    pub font_size: f32,
    pub rotation_deg: f32,
    pub gray: f32,
}

#[tauri::command]
fn slab_watermark(
    input: PathBuf,
    output: PathBuf,
    opts: WatermarkDto,
    pages: Vec<u32>,
) -> CmdResult<u32> {
    let opts = WatermarkOpts {
        text: &opts.text,
        opacity: opts.opacity,
        font_size: opts.font_size,
        rotation_deg: opts.rotation_deg,
        gray: opts.gray,
    };
    do_watermark(&input, &output, opts, &pages).into()
}

#[tauri::command]
fn slab_read_metadata(input: PathBuf) -> CmdResult<Metadata> {
    do_read_metadata(&input).into()
}

#[tauri::command]
fn slab_write_metadata(input: PathBuf, output: PathBuf, meta: Metadata) -> CmdResult<()> {
    do_write_metadata(&input, &output, &meta).into()
}

#[tauri::command]
fn slab_strip_metadata(input: PathBuf, output: PathBuf) -> CmdResult<()> {
    do_strip_metadata(&input, &output).into()
}

#[tauri::command]
fn slab_page_numbers(input: PathBuf, output: PathBuf, opts: PageNumbersOpts) -> CmdResult<u32> {
    do_page_numbers(&input, &output, &opts).into()
}

#[tauri::command]
fn slab_crop(input: PathBuf, output: PathBuf, opts: CropOpts, pages: Vec<u32>) -> CmdResult<u32> {
    do_crop(&input, &output, opts, &pages).into()
}

#[tauri::command]
fn slab_insert(input: PathBuf, output: PathBuf, opts: InsertOpts) -> CmdResult<u32> {
    do_insert(&input, &output, opts).into()
}

#[tauri::command]
fn slab_header_footer(input: PathBuf, output: PathBuf, opts: HFOpts) -> CmdResult<u32> {
    do_header_footer(&input, &output, opts).into()
}

#[tauri::command]
fn slab_redact(input: PathBuf, output: PathBuf, opts: RedactOpts) -> CmdResult<u32> {
    do_redact(&input, &output, opts).into()
}

#[tauri::command]
fn slab_nup(input: PathBuf, output: PathBuf, opts: NupOpts) -> CmdResult<u32> {
    do_nup(&input, &output, opts).into()
}

#[tauri::command]
fn slab_md2pdf(output: PathBuf, opts: Md2PdfOpts) -> CmdResult<u32> {
    let md = opts.markdown.clone();
    do_md2pdf(&md, &output, opts).into()
}

#[tauri::command]
fn slab_grayscale(input: PathBuf, output: PathBuf, opts: GrayscaleOpts) -> CmdResult<u32> {
    do_grayscale(&input, &output, opts).into()
}

#[tauri::command]
fn slab_page_labels(input: PathBuf, output: PathBuf, opts: PageLabelsOpts) -> CmdResult<u32> {
    do_page_labels(&input, &output, opts).into()
}

#[tauri::command]
fn slab_auto_redact(input: PathBuf, output: PathBuf, opts: AutoRedactOpts) -> CmdResult<u32> {
    do_auto_redact(&input, &output, opts).into()
}

#[tauri::command]
fn slab_read_outline(input: PathBuf) -> CmdResult<Vec<OutlineNode>> {
    do_read_outline(&input).into()
}

#[tauri::command]
fn slab_write_outline(input: PathBuf, output: PathBuf, nodes: Vec<OutlineNode>) -> CmdResult<u32> {
    do_write_outline(&input, &output, &nodes).into()
}

#[tauri::command]
fn slab_append_annotations(
    input: PathBuf,
    output: PathBuf,
    annotations: Vec<Annotation>,
) -> CmdResult<u32> {
    do_append_annotations(&input, &output, &annotations).into()
}

#[tauri::command]
fn slab_ocr(input: PathBuf, output: PathBuf, opts: OcrOpts) -> CmdResult<OcrReport> {
    do_ocr(&input, &output, &opts).into()
}

#[tauri::command]
fn slab_polyglot(input: PathBuf, output: PathBuf, opts: PolyglotOpts) -> CmdResult<PolyglotReport> {
    do_polyglot(&input, &output, opts).into()
}

#[tauri::command]
fn slab_flatten(input: PathBuf, output: PathBuf, opts: FlattenOpts) -> CmdResult<FlattenReport> {
    do_flatten(&input, &output, opts).into()
}

#[tauri::command]
fn slab_sanitize(input: PathBuf, output: PathBuf, opts: SanitizeOpts) -> CmdResult<SanitizeReport> {
    do_sanitize(&input, &output, opts).into()
}

#[tauri::command]
fn slab_repair(input: PathBuf, output: PathBuf) -> CmdResult<RepairReport> {
    do_repair(&input, &output).into()
}

// ---------- Beacon (AI) commands ----------

/// Helper: turn an `AiError` into a `CmdResult` so the Tauri shell
/// surfaces a user-readable string instead of a panic.
impl<T: Serialize> From<Result<T, ai::AiError>> for CmdResult<T> {
    fn from(r: Result<T, ai::AiError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

/// Read `~/.slab/config.toml`. If the file is missing, returns the
/// default config (local Ollama). Lets the settings panel populate
/// without a flicker.
#[tauri::command]
fn slab_beacon_config_read() -> CmdResult<SlabConfig> {
    do_load_beacon_config().into()
}

/// Persist `~/.slab/config.toml`. Caller hands the full `SlabConfig`
/// back — the Svelte side keeps the source of truth in component
/// state and writes the whole thing on every Save click.
#[tauri::command]
fn slab_beacon_config_write(config: SlabConfig) -> CmdResult<()> {
    do_save_beacon_config(&config).into()
}

/// Smoke-test the configured provider. Currently issues a trivial chat
/// call ("Reply with the single word READY"). Returns the model name
/// on success so the UI can show "Connected to llama3.2:3b ✓".
///
/// The settings panel uses this to give users a "Test connection"
/// button before they hit Save.
#[tauri::command]
async fn slab_beacon_provider_test(config: BeaconConfig) -> CmdResult<String> {
    use ai::{ChatMessage, ChatOpts, ChatRole};
    let provider = match ai::config::make_provider(&config) {
        Ok(p) => p,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let msgs = vec![ChatMessage {
        role: ChatRole::User,
        content: "Reply with the single word READY".into(),
    }];
    let opts = ChatOpts {
        max_tokens: Some(8),
        temperature: Some(0.0),
        ..Default::default()
    };
    match provider.chat(&msgs, &opts).await {
        Ok(resp) => CmdResult::Ok { value: resp.model },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Enumerate available provider kinds for the settings dropdown. Stays
/// in lockstep with the `ProviderKind` enum without hand-syncing on
/// the Svelte side.
#[tauri::command]
fn slab_beacon_provider_kinds() -> CmdResult<Vec<String>> {
    let kinds = [ProviderKind::Ollama, ProviderKind::Openai]
        .iter()
        .map(|k| match k {
            ProviderKind::Ollama => "ollama".to_string(),
            ProviderKind::Openai => "openai".to_string(),
        })
        .collect();
    CmdResult::Ok { value: kinds }
}

/// DTO for prior chat turns. Mirrors `ChatMessage` but uses lowercase
/// `role` strings so the Svelte side can stay agnostic of Rust enums.
#[derive(Deserialize)]
pub struct ChatTurnDto {
    pub role: String,
    pub content: String,
}

fn parse_role(s: &str) -> Option<ChatRole> {
    match s {
        "system" => Some(ChatRole::System),
        "user" => Some(ChatRole::User),
        "assistant" => Some(ChatRole::Assistant),
        _ => None,
    }
}

/// Beacon chat — Q&A against an opened PDF. The front-end hands us the
/// PDF path, the new question, and the prior conversation history.
/// We extract the PDF text, build a context-rich prompt, call the
/// configured provider (Ollama or OpenAI-compatible), and return the
/// assistant's reply along with cited page numbers.
///
/// `max_context_chars` is optional — defaults to ~30K. Front-end can
/// pass a smaller value for tiny local models or larger for hosted ones.
#[tauri::command]
async fn slab_beacon_chat(
    pdf_path: PathBuf,
    question: String,
    history: Vec<ChatTurnDto>,
    max_context_chars: Option<u32>,
) -> CmdResult<BeaconChatReply> {
    // Load the user's saved config so we honour the provider they picked
    // in the settings panel.
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
    let history_msgs: Vec<ChatMessage> = history
        .into_iter()
        .filter_map(|t| {
            parse_role(&t.role).map(|role| ChatMessage {
                role,
                content: t.content,
            })
        })
        .collect();
    let budget = max_context_chars
        .map(|n| n as usize)
        .unwrap_or(DEFAULT_MAX_CONTEXT_CHARS);
    do_beacon_chat(provider, &pdf_path, &question, &history_msgs, budget)
        .await
        .into()
}

#[tauri::command]
fn slab_export_annotations_md(
    input: PathBuf,
    output: PathBuf,
    label: Option<String>,
) -> CmdResult<u32> {
    let result: Result<u32, PdfError> = (|| {
        let annots: Vec<ExtractedAnnotation> = do_extract_annots(&input)?;
        let label = label.unwrap_or_else(|| {
            input
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("document.pdf")
                .to_string()
        });
        let md = do_annots_to_md(&label, &annots);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PdfError::Other(format!("create output dir: {e}")))?;
        }
        std::fs::write(&output, md).map_err(|e| PdfError::Other(format!("write markdown: {e}")))?;
        Ok(annots.len() as u32)
    })();
    result.into()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            app_info,
            slab_merge,
            slab_split_ranges,
            slab_split_every,
            slab_page_count,
            slab_rotate,
            slab_delete_pages,
            slab_reorder_pages,
            slab_extract_text,
            slab_extract_text_save,
            slab_info,
            slab_compress,
            slab_encrypt,
            slab_decrypt,
            slab_watermark,
            slab_read_metadata,
            slab_write_metadata,
            slab_strip_metadata,
            slab_page_numbers,
            slab_crop,
            slab_insert,
            slab_header_footer,
            slab_redact,
            slab_nup,
            slab_md2pdf,
            slab_grayscale,
            slab_page_labels,
            slab_auto_redact,
            slab_read_outline,
            slab_write_outline,
            slab_append_annotations,
            slab_ocr,
            slab_polyglot,
            slab_flatten,
            slab_sanitize,
            slab_repair,
            slab_beacon_config_read,
            slab_beacon_config_write,
            slab_beacon_provider_test,
            slab_beacon_provider_kinds,
            slab_beacon_chat,
            slab_export_annotations_md,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Slab");
}
