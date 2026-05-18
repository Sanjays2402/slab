// Slab — fast, free, offline PDF tool.
// All operations run locally; nothing is ever uploaded.

pub mod ai;
pub mod keymap;
pub mod marketplace;
pub mod pdf;
pub mod plugins;
pub mod windows;

use ai::auto_tag::AutoTagOpts;
use ai::chat::{
    beacon_chat_from_path as do_beacon_chat, BeaconChatReply, DEFAULT_MAX_CONTEXT_CHARS,
};
use ai::citations::{
    find_citations_from_path as do_beacon_find_citations, CitationOpts, CitationReport,
};
use ai::config::{
    load as do_load_beacon_config, save as do_save_beacon_config, BeaconConfig, ProviderKind,
    SlabConfig,
};
use ai::diff_summary::{beacon_diff_summary as do_beacon_diff_summary, BeaconDiffSummary};
use ai::embedding_index::{
    default_index_path, index_pdf as do_index_pdf, search_index as do_search_index, EmbeddingIndex,
    IndexReport, IndexStats, SearchHit,
};
use ai::glossary::{
    build_glossary_from_path as do_beacon_build_glossary, GlossaryOpts, GlossaryReport,
};
use ai::glossary_cache::{
    cache_dir as glossary_cache_dir, clear as glossary_cache_clear, load as glossary_cache_load,
    save as glossary_cache_save,
};
use ai::outline::{
    propose_outline_from_path as do_beacon_propose_outline, ProposedOutline,
    DEFAULT_OUTLINE_MAX_CHARS,
};
use ai::pii::{
    find_pii as do_find_pii, CustomPattern as PiiCustomPattern, PiiError, PiiHit, PiiKind, PiiOpts,
    PiiSummary,
};
use ai::selection_action::{
    run_selection_action as do_selection_action, SelectionAction, SelectionActionReply,
};
use ai::sm2::Ease;
use ai::study::{generate_deck_from_path as do_beacon_generate_deck, DeckOpts, DeckReport};
use ai::study_store::{
    default_db_path as default_study_db_path, StoredCard, StudyError, StudyStats, StudyStore,
};
use ai::summary::{beacon_summary_from_path as do_beacon_summary, BeaconSummary, SummaryLength};
use ai::voice::{
    capabilities as voice_capabilities, engine_is_installed as voice_engine_is_installed,
    list_voices as voice_list_voices, SpeakOpts, TtsEngine, Voice, VoiceCapabilities,
};
use ai::voice_session::VoiceSession;
use ai::{ChatMessage, ChatRole};

use pdf::annot_export::{
    extract as do_extract_annots, to_markdown as do_annots_to_md, ExtractedAnnotation,
};
use pdf::annotations::{append as do_append_annotations, Annotation};
use pdf::auto_redact::{auto_redact as do_auto_redact, AutoRedactOpts};
use pdf::compress::{compress as do_compress, CompressReport};
use pdf::crop::{crop as do_crop, CropOpts};
use pdf::diff::{diff_pdfs as do_diff_pdfs, export_report as do_diff_export_report, DocDiff};
use pdf::duplicate::duplicate_pages;
use pdf::edit_text::{
    find_text_spans as do_find_text_spans, replace_text_span as do_replace_text_span, PageSpans,
};
use pdf::encrypt::{decrypt as do_decrypt, encrypt as do_encrypt};
use pdf::extract::{extract_text as do_extract_text, extract_text_concat};
use pdf::flatten::{flatten as do_flatten, FlattenOpts, FlattenReport};
use pdf::grayscale::{grayscale as do_grayscale, GrayscaleOpts};
use pdf::header_footer::{apply as do_header_footer, HFOpts};
use pdf::info::{info as do_info, PdfInfo};
use pdf::insert::{insert as do_insert, InsertOpts};
use pdf::library::{
    auto_tag_run_many as do_auto_tag_run_many, auto_tag_run_one as do_auto_tag_run_one,
    default_db_path as library_default_db_path, ocr_queue_list_pending as do_ocr_queue_list,
    ocr_queue_run_all as do_ocr_queue_run_all, ocr_queue_run_one as do_ocr_queue_run_one,
    query_documents as do_query_documents, scan_folder as do_scan_folder, AutoTagRunResult,
    DocumentRecord, FolderRecord, LibraryDb, LibraryError, LibraryFilter, OcrQueueResult,
    ScanReport, TagRecord,
};
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
use pdf::pages_build::{pages_build as do_pages_build, PagesBuildOpts};
use pdf::polyglot::{polyglot_to_pdf as do_polyglot, PolyglotOpts, PolyglotReport};
use pdf::redact::{redact as do_redact, RedactOpts};
use pdf::repair::{repair as do_repair, RepairReport};
use pdf::sanitize::{sanitize as do_sanitize, SanitizeOpts, SanitizeReport};
use pdf::scan_audit::{audit as do_scan_audit, ScanAuditReport};
use pdf::slides::{analyze as do_slides_analyze, SlideReport};
use pdf::split::{page_count as do_page_count, split_by_ranges, split_every, PageRange};
use pdf::split_pattern::{
    find_matching_pages, outline_top_level_pages, split_by_pattern as do_split_by_pattern,
};
use pdf::stamp_annotations::{stamp_annotations as do_stamp_annotations, StampAnnotationsOpts};
use pdf::table_extract::{
    extract_tables as do_extract_tables, to_csv as do_table_to_csv, Table as TableDto, TableOpts,
};
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
fn slab_split_by_pattern(
    input: PathBuf,
    pattern: Option<String>,
    out_dir: PathBuf,
) -> CmdResult<Vec<PathBuf>> {
    do_split_by_pattern(&input, pattern.as_deref(), &out_dir).into()
}

#[tauri::command]
fn slab_find_matching_pages(input: PathBuf, pattern: String) -> CmdResult<Vec<u32>> {
    find_matching_pages(&input, &pattern).into()
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
fn slab_duplicate_pages(input: PathBuf, pages: Vec<u32>, output: PathBuf) -> CmdResult<u32> {
    duplicate_pages(&input, &pages, &output).into()
}

#[tauri::command]
fn slab_reorder_pages(input: PathBuf, order: Vec<u32>, output: PathBuf) -> CmdResult<()> {
    reorder_pages(&input, &order, &output).into()
}

#[tauri::command]
fn slab_pages_build(input: PathBuf, opts: PagesBuildOpts, output: PathBuf) -> CmdResult<u32> {
    do_pages_build(&input, &opts, &output).into()
}

#[tauri::command]
fn slab_find_text_spans(input: PathBuf) -> CmdResult<Vec<PageSpans>> {
    do_find_text_spans(&input).into()
}

#[tauri::command]
fn slab_replace_text_span(
    input: PathBuf,
    output: PathBuf,
    span_id: String,
    new_text: String,
) -> CmdResult<()> {
    do_replace_text_span(&input, &output, &span_id, &new_text).into()
}

#[tauri::command]
fn slab_diff_pdfs(old: PathBuf, new: PathBuf) -> CmdResult<DocDiff> {
    do_diff_pdfs(&old, &new).into()
}

#[tauri::command]
fn slab_diff_export_report(old: PathBuf, new: PathBuf, output: PathBuf) -> CmdResult<u32> {
    // We re-run the diff here rather than asking the frontend to ship the
    // (potentially huge) DocDiff payload back over the IPC wire. Cheap on
    // any sane PDF and keeps the command surface tiny.
    match do_diff_pdfs(&old, &new) {
        Ok(d) => do_diff_export_report(&d, &output).into(),
        Err(e) => Err(e).into(),
    }
}

#[tauri::command]
fn slab_slides_analyze(input: PathBuf) -> CmdResult<SlideReport> {
    do_slides_analyze(&input).into()
}

#[tauri::command]
fn slab_theater_export_annotated(
    input: PathBuf,
    output: PathBuf,
    opts: StampAnnotationsOpts,
) -> CmdResult<u32> {
    do_stamp_annotations(&input, &output, opts).into()
}

#[tauri::command]
fn slab_outline_starts(input: PathBuf) -> CmdResult<Vec<u32>> {
    outline_top_level_pages(&input).into()
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
fn slab_scan_audit(input: PathBuf) -> CmdResult<ScanAuditReport> {
    do_scan_audit(&input).into()
}

#[tauri::command]
fn slab_extract_tables(input: PathBuf, opts: TableOpts) -> CmdResult<Vec<TableDto>> {
    do_extract_tables(&input, &opts).into()
}

#[tauri::command]
fn slab_table_to_csv(table: TableDto) -> CmdResult<String> {
    CmdResult::Ok {
        value: do_table_to_csv(&table),
    }
}

#[tauri::command]
fn slab_table_save_csv(table: TableDto, output: PathBuf) -> CmdResult<PathBuf> {
    let csv = do_table_to_csv(&table);
    match std::fs::write(&output, csv) {
        Ok(_) => CmdResult::Ok { value: output },
        Err(e) => CmdResult::Err {
            message: format!("write csv: {e}"),
        },
    }
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

/// Read just the `[ui]` block — theme/accent/density. The frontend
/// calls this once on boot to set CSS variables. Splitting it from
/// `slab_beacon_config_read` means the theme bootstrap doesn't pay
/// for Beacon-provider decode work.
#[tauri::command]
fn slab_ui_config_read() -> CmdResult<ai::config::UiConfig> {
    match do_load_beacon_config() {
        Ok(cfg) => CmdResult::Ok { value: cfg.ui },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Persist UI prefs without touching `[beacon]`. Read-modify-write so a
/// theme change never wipes the user's Beacon provider config.
#[tauri::command]
fn slab_ui_config_write(ui: ai::config::UiConfig) -> CmdResult<()> {
    let mut cfg = match do_load_beacon_config() {
        Ok(c) => c,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    cfg.ui = ui;
    do_save_beacon_config(&cfg).into()
}

// ---------- Keymap commands (Glass Slice 7) ----------

/// Snapshot the user's current keymap. Frontend caches this on boot and
/// uses it to drive every `matches(event, "...")` check across the app.
#[tauri::command]
fn slab_keymap_read() -> CmdResult<keymap::commands::KeymapView> {
    match do_load_beacon_config() {
        Ok(cfg) => CmdResult::Ok {
            value: keymap::commands::build_view(&cfg.keymap),
        },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Apply a batch of `(action_id, "Binding")` overrides. Validates every
/// entry up front, rejects on first error, never persists a partial
/// state. Returns the freshly-materialised view so the frontend never
/// gets out of sync with disk.
#[tauri::command]
fn slab_keymap_write(
    args: keymap::commands::KeymapWriteArgs,
) -> CmdResult<keymap::commands::KeymapView> {
    let mut cfg = match do_load_beacon_config() {
        Ok(c) => c,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    if let Err(e) = keymap::commands::apply_overrides(&mut cfg.keymap, args.overrides) {
        return CmdResult::Err {
            message: e.to_string(),
        };
    }
    if let Err(e) = do_save_beacon_config(&cfg) {
        return CmdResult::Err {
            message: e.to_string(),
        };
    }
    CmdResult::Ok {
        value: keymap::commands::build_view(&cfg.keymap),
    }
}

/// Wipe all user overrides — restore the factory-default keymap.
#[tauri::command]
fn slab_keymap_reset() -> CmdResult<keymap::commands::KeymapView> {
    let mut cfg = match do_load_beacon_config() {
        Ok(c) => c,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    cfg.keymap.clear_all();
    if let Err(e) = do_save_beacon_config(&cfg) {
        return CmdResult::Err {
            message: e.to_string(),
        };
    }
    CmdResult::Ok {
        value: keymap::commands::build_view(&cfg.keymap),
    }
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

/// Beacon summary — one-call summary of an opened PDF at the user's
/// chosen length (TLDR / Short / Long). Reuses the same provider abstraction
/// as `slab_beacon_chat`.
#[tauri::command]
async fn slab_beacon_summary(
    pdf_path: PathBuf,
    length: SummaryLength,
    max_context_chars: Option<u32>,
) -> CmdResult<BeaconSummary> {
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
    let budget = max_context_chars
        .map(|n| n as usize)
        .unwrap_or(DEFAULT_MAX_CONTEXT_CHARS);
    do_beacon_summary(provider, &pdf_path, length, budget)
        .await
        .into()
}

/// Beacon "Smart Outline" — propose a hierarchical TOC for an opened PDF.
/// Returns a `ProposedOutline` whose `nodes` field is shaped exactly like
/// what `slab_write_outline` expects, so the frontend can pipe an accepted
/// proposal straight into the existing save path with zero translation.
#[tauri::command]
async fn slab_beacon_propose_outline(
    pdf_path: PathBuf,
    max_context_chars: Option<u32>,
) -> CmdResult<ProposedOutline> {
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
    let budget = max_context_chars
        .map(|n| n as usize)
        .unwrap_or(DEFAULT_OUTLINE_MAX_CHARS);
    do_beacon_propose_outline(provider, &pdf_path, budget)
        .await
        .into()
}

/// Beacon Citations — scan the PDF for inline citations and extract a
/// structured References list from end-matter. Returns a `CitationReport`
/// that the front-end can render as a sidebar panel with mention chips
/// and "jump to bibliography" links. v1.6.0 Beacon Bonus Slice 12.
#[tauri::command]
async fn slab_beacon_find_citations(
    pdf_path: PathBuf,
    opts: Option<CitationOpts>,
) -> CmdResult<CitationReport> {
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
    let opts = opts.unwrap_or_default();
    do_beacon_find_citations(provider, &pdf_path, &opts)
        .await
        .into()
}

/// Beacon Glossary — scan a PDF for jargon, acronyms, and italicised
/// terms, ask the AI provider for plain-English definitions, and return
/// a sorted, deduped `GlossaryReport`. Result is automatically cached
/// to `~/.slab/glossary/<pdf_hash>.json` so subsequent loads are
/// instant. v1.8.0 Beacon Bonus Slice 14.
#[tauri::command]
async fn slab_beacon_build_glossary(
    pdf_path: PathBuf,
    opts: Option<GlossaryOpts>,
) -> CmdResult<GlossaryReport> {
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
    let opts = opts.unwrap_or_default();
    let report = match do_beacon_build_glossary(provider, &pdf_path, &opts).await {
        Ok(r) => r,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    // Best-effort write to the on-disk cache; a failed write does NOT
    // fail the command (the caller already has the report in memory).
    if let Ok(hash) = hash_pdf_path(&pdf_path) {
        let dir = glossary_cache_dir();
        if let Err(e) = glossary_cache_save(&hash, &report, &dir) {
            eprintln!("[beacon/glossary] cache save failed: {e}");
        }
    }
    CmdResult::Ok { value: report }
}

/// Beacon Glossary — load a previously-built report from the on-disk
/// cache, keyed by `sha256(pdf_contents)`. Returns `Some(report)` on a
/// hit, `None` on a miss / version-mismatch / corrupted file. The UI
/// uses this to populate the panel without re-hitting the LLM.
#[tauri::command]
async fn slab_beacon_load_glossary_cache(pdf_path: PathBuf) -> CmdResult<Option<GlossaryReport>> {
    let hash = match hash_pdf_path(&pdf_path) {
        Ok(h) => h,
        Err(e) => {
            return CmdResult::Err {
                message: format!("hashing pdf: {e}"),
            }
        }
    };
    let dir = glossary_cache_dir();
    match glossary_cache_load(&hash, &dir) {
        Ok(opt) => CmdResult::Ok { value: opt },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Beacon Glossary — remove the cached report for a PDF so the next
/// `slab_beacon_build_glossary` call rebuilds from scratch. Invoked by
/// the "Rebuild" button in the glossary panel.
#[tauri::command]
async fn slab_beacon_clear_glossary_cache(pdf_path: PathBuf) -> CmdResult<()> {
    let hash = match hash_pdf_path(&pdf_path) {
        Ok(h) => h,
        Err(e) => {
            return CmdResult::Err {
                message: format!("hashing pdf: {e}"),
            }
        }
    };
    let dir = glossary_cache_dir();
    match glossary_cache_clear(&hash, &dir) {
        Ok(()) => CmdResult::Ok { value: () },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

impl<T: Serialize> From<Result<T, StudyError>> for CmdResult<T> {
    fn from(r: Result<T, StudyError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

/// SHA-256 of a PDF file's contents, used to scope Study Mode cards
/// per file (so renames/moves don't fork the deck). Re-uses the same
/// helper that the embedding index uses for its `pdf_hash` column.
fn hash_pdf_path(p: &std::path::Path) -> Result<String, std::io::Error> {
    ai::embedding_index::EmbeddingIndex::hash_file(p)
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Beacon Study — generate a deck of Q&A flashcards from a PDF and
/// persist them in `~/.slab/study.sqlite` (UNIQUE(pdf_hash, q_norm)
/// dedupes across re-runs). Returns the freshly-generated deck (NOT
/// the full stored deck — UI uses `slab_beacon_study_due` next).
/// v1.7.0 Beacon Bonus Slice 13.
#[tauri::command]
async fn slab_beacon_generate_deck(
    pdf_path: PathBuf,
    opts: Option<DeckOpts>,
) -> CmdResult<DeckReport> {
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
    let opts = opts.unwrap_or_default();
    let report = match do_beacon_generate_deck(provider, &pdf_path, &opts).await {
        Ok(r) => r,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    // Persist (best-effort path — open store, insert. Returns the report
    // even if no new cards were inserted because of dedupe.)
    let hash = match hash_pdf_path(&pdf_path) {
        Ok(h) => h,
        Err(e) => {
            return CmdResult::Err {
                message: format!("hashing pdf: {e}"),
            }
        }
    };
    match StudyStore::open(&default_study_db_path()) {
        Ok(mut store) => {
            let _ = store.insert_deck(&hash, &report.cards);
        }
        Err(e) => {
            return CmdResult::Err {
                message: format!("opening study store: {e}"),
            }
        }
    }
    CmdResult::Ok { value: report }
}

/// Beacon Study — fetch cards due for review. If `pdf_path` is given,
/// scope to that PDF; otherwise return cross-document due cards.
#[tauri::command]
async fn slab_beacon_study_due(
    pdf_path: Option<PathBuf>,
    limit: Option<u32>,
) -> CmdResult<Vec<StoredCard>> {
    let store = match StudyStore::open(&default_study_db_path()) {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let hash_owned: Option<String> = match pdf_path.as_ref() {
        Some(p) => match hash_pdf_path(p) {
            Ok(h) => Some(h),
            Err(e) => {
                return CmdResult::Err {
                    message: format!("hashing pdf: {e}"),
                }
            }
        },
        None => None,
    };
    store
        .due_cards(hash_owned.as_deref(), limit.unwrap_or(50))
        .into()
}

/// Beacon Study — record a review and return the updated card.
#[tauri::command]
async fn slab_beacon_study_review(card_id: i64, ease: Ease) -> CmdResult<StoredCard> {
    let mut store = match StudyStore::open(&default_study_db_path()) {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    store.review(card_id, ease).into()
}

/// Beacon Study — counters for the footer.
#[tauri::command]
async fn slab_beacon_study_stats(pdf_path: Option<PathBuf>) -> CmdResult<StudyStats> {
    let store = match StudyStore::open(&default_study_db_path()) {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let hash_owned: Option<String> = match pdf_path.as_ref() {
        Some(p) => match hash_pdf_path(p) {
            Ok(h) => Some(h),
            Err(e) => {
                return CmdResult::Err {
                    message: format!("hashing pdf: {e}"),
                }
            }
        },
        None => None,
    };
    store.stats(hash_owned.as_deref()).into()
}

/// Beacon "what changed?" — natural-language summary of a DocDiff. Re-runs
/// the diff server-side (cheap) so the front-end doesn't need to ship the
/// full DocDiff payload back over the IPC wire. v0.14.0 Stack Slice 5.
#[tauri::command]
async fn slab_beacon_diff_summary(
    old: PathBuf,
    new: PathBuf,
    max_diff_chars: Option<u32>,
) -> CmdResult<BeaconDiffSummary> {
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
    let diff = match do_diff_pdfs(&old, &new) {
        Ok(d) => d,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let budget = max_diff_chars
        .map(|n| n as usize)
        .unwrap_or(DEFAULT_MAX_CONTEXT_CHARS);
    do_beacon_diff_summary(provider, &diff, budget).await.into()
}

/// Beacon vision Q&A — render `page` (or a region of it) to an image
/// and ask the configured provider a question about it. Buffered for
/// v0.13.0; streaming variant arrives in v0.13.1 once the Beacon
/// channel surface is uniformly wired across `chat` / `summary` /
/// `selection_action` / vision.
///
/// Requires a vision-capable provider. Ollama is the default
/// (`llava:7b`); OpenAI-compat surfaces a clean
/// "vision unsupported" error until the multimodal endpoint is wired
/// (a v0.13.1 follow-up).
#[tauri::command]
async fn slab_beacon_vision_ask(
    pdf_path: PathBuf,
    page: u32,
    rect_pts: Option<ai::vision::RectPts>,
    prompt: String,
    history: Vec<ChatTurnDto>,
    opts: Option<ai::vision::VisionOpts>,
) -> CmdResult<ai::vision::VisionReply> {
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
    let opts = opts.unwrap_or_default();
    ai::vision::vision_ask(
        provider,
        &pdf_path,
        page,
        rect_pts,
        &prompt,
        &history_msgs,
        &opts,
    )
    .await
    .into()
}

// ---------- Beacon semantic search (Slice 6/7) ----------

/// Helper: open the shared on-disk embedding index. Each command opens
/// fresh — SQLite handle creation is microseconds and we're single-user.
fn open_default_index() -> Result<EmbeddingIndex, ai::embedding_index::IndexError> {
    EmbeddingIndex::open(&default_index_path())
}

impl<T: Serialize> From<Result<T, ai::embedding_index::IndexError>> for CmdResult<T> {
    fn from(r: Result<T, ai::embedding_index::IndexError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

/// Index (or re-index) a PDF for semantic search. Reads page text via
/// the existing extract pipeline, chunks it, embeds each chunk via the
/// configured provider, and writes rows to `~/.slab/beacon-index.sqlite`.
///
/// If the same file content (SHA-256) has already been indexed and
/// `force_reindex` is false, this is a no-op that returns
/// `was_cached: true` — letting the UI fire-and-forget on PDF open.
#[tauri::command]
async fn slab_beacon_index_pdf(
    pdf_path: PathBuf,
    force_reindex: Option<bool>,
) -> CmdResult<IndexReport> {
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
    let pages = match pdf::extract::extract_text(&pdf_path) {
        Ok(p) => p,
        Err(e) => {
            return CmdResult::Err {
                message: format!("read PDF: {e}"),
            }
        }
    };
    let index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let index = std::sync::Arc::new(std::sync::Mutex::new(index));
    let embed_model = cfg
        .beacon
        .embed_model
        .clone()
        .unwrap_or_else(|| "default".to_string());
    do_index_pdf(
        index,
        provider,
        &pdf_path,
        &pages,
        &embed_model,
        force_reindex.unwrap_or(false),
    )
    .await
    .into()
}

/// Search the embedding index. `top_k` defaults to 12; `only_pdf_hash`
/// restricts the search to a single PDF (set it to the hash returned
/// by `slab_beacon_index_pdf` for "this PDF only" mode).
#[tauri::command]
async fn slab_beacon_search(
    query: String,
    top_k: Option<u32>,
    only_pdf_hash: Option<String>,
) -> CmdResult<Vec<SearchHit>> {
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
    let index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let index = std::sync::Arc::new(std::sync::Mutex::new(index));
    let k = top_k.unwrap_or(12) as usize;
    do_search_index(index, provider, &query, k, only_pdf_hash)
        .await
        .into()
}

/// How many PDFs / chunks are in the index? Powers the "Indexed 4 PDFs
/// (1,247 chunks)" footer in the search panel.
#[tauri::command]
fn slab_beacon_index_stats() -> CmdResult<IndexStats> {
    let index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    index.stats().into()
}

/// Forget a single PDF from the index. The UI uses this when the user
/// hits the trash icon next to an indexed PDF in the panel footer.
#[tauri::command]
fn slab_beacon_index_forget(pdf_hash: String) -> CmdResult<()> {
    let index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    index.forget(&pdf_hash).into()
}

// ---------- Beacon PII Highlighter (Slice 8) ----------

impl<T: Serialize> From<Result<T, PiiError>> for CmdResult<T> {
    fn from(r: Result<T, PiiError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

/// Bundled response from `slab_beacon_pii_find` — the front-end shows
/// both the per-hit list and the per-kind summary in one go.
#[derive(Debug, Clone, Serialize)]
struct PiiFindReport {
    hits: Vec<PiiHit>,
    summary: PiiSummary,
}

/// Scan a PDF for PII. Regex pass is always on; LLM pass (names +
/// addresses) is toggled by `include_llm_pass`. When the LLM pass is
/// requested we also load the configured Beacon provider — if it's
/// down the UI gets a clear "provider unavailable" message.
#[tauri::command]
async fn slab_beacon_pii_find(
    pdf_path: PathBuf,
    include_llm_pass: Option<bool>,
    kinds: Option<Vec<PiiKind>>,
    custom_patterns: Option<Vec<PiiCustomPattern>>,
) -> CmdResult<PiiFindReport> {
    let want_llm = include_llm_pass.unwrap_or(false);
    let opts = PiiOpts {
        include_llm_pass: want_llm,
        custom_patterns: custom_patterns.unwrap_or_default(),
        kinds: kinds.unwrap_or_default(),
    };
    // Lazily build provider only if the user asked for the LLM pass —
    // a pure regex scan should work even when Ollama isn't installed.
    let provider_box;
    let provider_ref: Option<&dyn ai::AiProvider> = if want_llm {
        let cfg = match do_load_beacon_config() {
            Ok(c) => c,
            Err(e) => {
                return CmdResult::Err {
                    message: e.to_string(),
                }
            }
        };
        provider_box = match ai::config::make_provider(&cfg.beacon) {
            Ok(p) => p,
            Err(e) => {
                return CmdResult::Err {
                    message: e.to_string(),
                }
            }
        };
        Some(provider_box.as_ref())
    } else {
        None
    };
    match do_find_pii(&pdf_path, provider_ref, opts).await {
        Ok(hits) => {
            let summary = PiiSummary::from_hits(&hits);
            CmdResult::Ok {
                value: PiiFindReport { hits, summary },
            }
        }
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Apply auto-redaction for a chosen set of PII kinds + extra custom
/// patterns. We translate the regex-friendly kinds (email/ssn/phone/cc)
/// into existing `auto_redact` presets, and let the caller pass through
/// arbitrary regex strings for everything else (Names/Addresses get
/// turned into literal-match patterns by the UI before calling this).
///
/// Returns the number of regex matches the redactor blacked out.
#[tauri::command]
fn slab_beacon_pii_redact(
    input: PathBuf,
    output: PathBuf,
    presets: Vec<String>,
    patterns: Vec<String>,
    gray: Option<f32>,
) -> CmdResult<u32> {
    let opts = pdf::auto_redact::AutoRedactOpts {
        presets,
        patterns,
        gray: gray.unwrap_or(0.0),
    };
    let result = pdf::auto_redact::auto_redact(&input, &output, opts);
    match result {
        Ok(n) => CmdResult::Ok { value: n },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Beacon Selection Action — run one of five quick LLM transforms
/// (Translate / Explain / Define / Rewrite / Summarize) on a snippet
/// the user highlighted in the PDF reader.
///
/// Lightweight by design: no PDF context loading, just the snippet.
/// Uses the same provider as chat/summary (resolved from `~/.slab/config.toml`).
/// `target_lang` is only consulted for the Translate action; ignored otherwise.
#[tauri::command]
async fn slab_beacon_selection_action(
    text: String,
    action: SelectionAction,
    target_lang: Option<String>,
) -> CmdResult<SelectionActionReply> {
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
    do_selection_action(provider, &text, action, target_lang)
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

// ---------- Library Mode (v0.12.0 Atlas) ----------

impl<T: Serialize> From<Result<T, LibraryError>> for CmdResult<T> {
    fn from(r: Result<T, LibraryError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

/// Open the default library DB at `~/.slab/library.sqlite`.
fn open_library_db() -> Result<LibraryDb, LibraryError> {
    LibraryDb::open(&library_default_db_path())
}

// ---------- Cabinet (v1.1.0) Slice 6 — cross-window events ----------
//
// When the library mutates (folder added, doc OCR'd, tags changed, …) any
// detached LibraryPanel instance in another window won't notice unless we
// proactively tell it. We emit a global `slab://library-changed` event
// after every successful library-mutation command; LibraryPanel listens
// and refetches.
//
// We deliberately do NOT pass a payload — the event is a *poke*, not a
// patch. Each listener decides how much state to refresh. This keeps
// schema-coupling between backend and any number of frontend listeners
// at exactly zero.

/// Emit `slab://library-changed` to every Tauri window. Swallows errors —
/// a transient event-bus failure should never escalate to a command Err
/// the user sees on what was an otherwise-successful write.
fn emit_library_changed(app: &tauri::AppHandle) {
    use tauri::Emitter;
    if let Err(e) = app.emit("slab://library-changed", ()) {
        eprintln!("[cabinet] failed to emit slab://library-changed: {e}");
    }
}

/// Ask the main Slab window to open `path` in a new Reader tab. Invoked
/// from detached panel windows (typically a detached Library) so the user
/// can still drive the main reader from a satellite window. Emits
/// `slab://open-doc` to the main window only (NOT a broadcast — we don't
/// want every detached Reader to also open the doc).
///
/// Returns Err if the main window isn't currently alive — that's an
/// invariant violation worth surfacing, since the main window outlives
/// every detached child.
#[tauri::command]
fn slab_request_open_in_main(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    main.emit("slab://open-doc", path)
        .map_err(|e| format!("emit slab://open-doc: {e}"))
}

#[tauri::command]
fn slab_library_add_folder(app: tauri::AppHandle, path: String) -> CmdResult<FolderRecord> {
    let result = (|| -> Result<FolderRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.add_folder(&path)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_remove_folder(app: tauri::AppHandle, id: i64) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        db.remove_folder(id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_list_folders() -> CmdResult<Vec<FolderRecord>> {
    let result = (|| -> Result<Vec<FolderRecord>, LibraryError> {
        let db = open_library_db()?;
        db.list_folders()
    })();
    result.into()
}

#[tauri::command]
fn slab_library_scan(app: tauri::AppHandle, folder_id: i64) -> CmdResult<ScanReport> {
    let result = (|| -> Result<ScanReport, LibraryError> {
        let mut db = open_library_db()?;
        let folders = db.list_folders()?;
        let folder = folders
            .into_iter()
            .find(|f| f.id == folder_id)
            .ok_or_else(|| LibraryError::Other(format!("folder id {folder_id} not found")))?;
        do_scan_folder(&mut db, &folder)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_list_docs(filter: Option<LibraryFilter>) -> CmdResult<Vec<DocumentRecord>> {
    let result = (|| -> Result<Vec<DocumentRecord>, LibraryError> {
        let db = open_library_db()?;
        do_query_documents(&db, &filter.unwrap_or_default())
    })();
    result.into()
}

#[tauri::command]
fn slab_library_list_tags() -> CmdResult<Vec<TagRecord>> {
    let result = (|| -> Result<Vec<TagRecord>, LibraryError> {
        let db = open_library_db()?;
        db.list_tags()
    })();
    result.into()
}

#[tauri::command]
fn slab_library_add_tag(
    app: tauri::AppHandle,
    name: String,
    color: Option<String>,
) -> CmdResult<TagRecord> {
    let result = (|| -> Result<TagRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.add_tag(&name, color.as_deref())
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_set_doc_tags(
    app: tauri::AppHandle,
    doc_id: i64,
    tag_ids: Vec<i64>,
) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        db.set_doc_tags(doc_id, &tag_ids)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_remove_document(app: tauri::AppHandle, doc_id: i64) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        db.remove_document(doc_id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_remove_tag(app: tauri::AppHandle, tag_id: i64) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        db.remove_tag(tag_id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Rescan every registered folder in one call. Returns one ScanReport
/// per folder, in folder-insertion order. Folders that fail to scan
/// (permission denied, etc.) emit a zero-counts report rather than
/// aborting the whole sweep — the UI surfaces partial progress.
#[tauri::command]
fn slab_library_rescan_all(app: tauri::AppHandle) -> CmdResult<Vec<ScanReport>> {
    let result = (|| -> Result<Vec<ScanReport>, LibraryError> {
        let mut db = open_library_db()?;
        let folders = db.list_folders()?;
        let mut reports = Vec::with_capacity(folders.len());
        for folder in folders {
            match do_scan_folder(&mut db, &folder) {
                Ok(r) => reports.push(r),
                Err(_) => reports.push(ScanReport {
                    folder_id: folder.id,
                    ..Default::default()
                }),
            }
        }
        Ok(reports)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// List documents whose ocr_state is `scanned` or `mixed` (i.e. OCR candidates
/// not yet queued/processed). Ordered by added_at ASC.
#[tauri::command]
fn slab_library_ocr_queue_list_pending() -> CmdResult<Vec<DocumentRecord>> {
    let result = (|| -> Result<Vec<DocumentRecord>, LibraryError> {
        let db = open_library_db()?;
        do_ocr_queue_list(&db)
    })();
    result.into()
}

/// Run OCR on a single document by id. Returns the queue result (state_after,
/// output_path, error). `opts` is optional — defaults to eng @ 300dpi.
#[tauri::command]
fn slab_library_ocr_queue_run_one(
    app: tauri::AppHandle,
    doc_id: i64,
    opts: Option<pdf::ocr::OcrOpts>,
) -> CmdResult<OcrQueueResult> {
    let result = (|| -> Result<OcrQueueResult, LibraryError> {
        let mut db = open_library_db()?;
        Ok(do_ocr_queue_run_one(
            &mut db,
            doc_id,
            &opts.unwrap_or_default(),
        ))
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Run OCR on every pending document. Returns one result per document
/// attempted, in queue order. `opts` is optional.
#[tauri::command]
fn slab_library_ocr_queue_run_all(
    app: tauri::AppHandle,
    opts: Option<pdf::ocr::OcrOpts>,
) -> CmdResult<Vec<OcrQueueResult>> {
    let result = (|| -> Result<Vec<OcrQueueResult>, LibraryError> {
        let mut db = open_library_db()?;
        do_ocr_queue_run_all(&mut db, &opts.unwrap_or_default())
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

// ---------- Library auto-tag (Lens Slice 6) ----------

/// Run auto-tag on one library document. Extracts page text, asks the
/// configured Beacon provider for 3–5 topical tags, materialises them as
/// `library_tags` rows and attaches them to the doc — unioning with any
/// tags the user previously set by hand. Returns `AutoTagRunResult` whose
/// `error: Some(...)` field signals per-doc failure (the command itself
/// only Errs on backend wiring problems like a missing provider config).
#[tauri::command]
async fn slab_library_auto_tag_one(
    app: tauri::AppHandle,
    doc_id: i64,
    opts: Option<AutoTagOpts>,
) -> CmdResult<AutoTagRunResult> {
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
    let mut db = match open_library_db() {
        Ok(d) => d,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let res = do_auto_tag_run_one(&mut db, provider, doc_id, &opts.unwrap_or_default()).await;
    emit_library_changed(&app);
    CmdResult::Ok { value: res }
}

/// Run auto-tag on many library documents sequentially. Continues past
/// per-doc failures — inspect each result's `error` field.
#[tauri::command]
async fn slab_library_auto_tag_many(
    app: tauri::AppHandle,
    doc_ids: Vec<i64>,
    opts: Option<AutoTagOpts>,
) -> CmdResult<Vec<AutoTagRunResult>> {
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
    let mut db = match open_library_db() {
        Ok(d) => d,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    let res = do_auto_tag_run_many(&mut db, provider, &doc_ids, &opts.unwrap_or_default()).await;
    emit_library_changed(&app);
    CmdResult::Ok { value: res }
}

// ---------- Foundry (v1.3.0) — plugin Tauri commands ----------

/// List all known plugins (active + broken + disabled). UI calls this
/// to populate the plugin panel.
#[tauri::command]
fn slab_plugins_list(reg: tauri::State<'_, plugins::PluginRegistry>) -> Vec<plugins::Plugin> {
    reg.list()
}

/// Flip a plugin's enabled flag and persist the new state to
/// `~/.slab/plugin-state.toml`. Returns `false` if the ID is unknown.
#[tauri::command]
fn slab_plugins_set_enabled(
    id: String,
    enabled: bool,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<bool, String> {
    if !reg.set_enabled(&id, enabled) {
        return Ok(false);
    }
    if let Some(p) = plugins::default_state_path() {
        let snap = reg.enabled_state();
        if let Err(e) = plugins::write_enabled_state(&p, &snap) {
            return Err(format!("could not persist plugin state: {e}"));
        }
    }
    Ok(true)
}

/// Re-scan the plugins directory (`~/.slab/plugins`). UI calls this
/// after the user drops a new plugin in. Returns the fresh list.
#[tauri::command]
fn slab_plugins_reload(
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<Vec<plugins::Plugin>, String> {
    let state_path = plugins::default_state_path();
    let enabled = state_path
        .as_deref()
        .map(plugins::read_enabled_state)
        .unwrap_or_default();
    let root = plugins::default_plugins_root()
        .ok_or_else(|| "HOME env var not set; cannot locate ~/.slab/plugins".to_string())?;
    reg.discover(&root, &enabled);
    Ok(reg.list())
}

/// Return the on-disk path of `~/.slab/plugins` (creating it if it
/// doesn't exist). The frontend uses this with `tauri-plugin-opener` to
/// reveal the directory in Finder/Explorer/Files.
#[tauri::command]
fn slab_plugins_dir() -> Result<String, String> {
    let root = plugins::default_plugins_root()
        .ok_or_else(|| "HOME env var not set; cannot locate ~/.slab/plugins".to_string())?;
    if let Err(e) = std::fs::create_dir_all(&root) {
        return Err(format!("could not create plugins dir: {e}"));
    }
    Ok(root.to_string_lossy().to_string())
}

/// List themes contributed by enabled plugins. Frontend calls this on
/// boot to populate the theme picker. Each entry carries the plugin's
/// dir so the frontend can `slab_plugins_read_asset` the CSS file.
#[tauri::command]
fn slab_plugins_active_themes(
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Vec<plugins::ActiveTheme> {
    plugins::active_themes(&reg)
}

/// List locale bundles contributed by enabled plugins.
#[tauri::command]
fn slab_plugins_active_locales(
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Vec<plugins::ActiveLocale> {
    plugins::active_locales(&reg)
}

/// List custom commands (palette entries) contributed by enabled plugins.
#[tauri::command]
fn slab_plugins_active_commands(
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Vec<plugins::ActiveCommand> {
    plugins::active_commands(&reg)
}

/// List AI providers contributed by enabled plugins.
#[tauri::command]
fn slab_plugins_active_ai_providers(
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Vec<plugins::ActiveAiProvider> {
    plugins::active_ai_providers(&reg)
}

/// List PDF actions contributed by enabled plugins. (CLI runner lands
/// in Slice 6; this just exposes the catalog.)
#[tauri::command]
fn slab_plugins_active_pdf_actions(
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Vec<plugins::ActivePdfAction> {
    plugins::active_pdf_actions(&reg)
}

/// Read a relative asset file (theme CSS, locale JSON, icon) from a
/// plugin's directory. Path-traversal is rejected at the Rust layer —
/// the resolved path must stay inside the plugin's directory.
#[tauri::command]
fn slab_plugins_read_asset(
    plugin_id: String,
    relative: String,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<String, String> {
    let p = reg
        .get(&plugin_id)
        .ok_or_else(|| format!("unknown plugin {plugin_id:?}"))?;
    plugins::read_asset(&p.dir, &relative)
}

/// Run a plugin PDF action against `input` and write the result to
/// `output`. Returns an [`ActionReport`] with status + stdout/stderr;
/// the frontend surfaces this so users can see what the external CLI
/// said. Errors here are reserved for setup failures (missing input,
/// tempfile failures) — CLI exit codes go in the report status.
#[tauri::command]
fn slab_plugins_run_pdf_action(
    plugin_id: String,
    action_id: String,
    input: String,
    output: String,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<plugins::ActionReport, String> {
    let actions = plugins::active_pdf_actions(&reg);
    let action = actions
        .into_iter()
        .find(|a| a.plugin_id == plugin_id && a.action.id == action_id)
        .ok_or_else(|| format!("no active pdf_action {action_id:?} on plugin {plugin_id:?}"))?;
    plugins::run_pdf_action(
        &action,
        std::path::Path::new(&input),
        std::path::Path::new(&output),
    )
    .map_err(|e| e.to_string())
}

/// Load a plugin-contributed locale bundle as a flat `key -> translation`
/// map. The frontend i18n layer merges these into its in-memory bundles
/// at boot (and on plugin enable/disable) without going through a file
/// read on the JS side.
///
/// Errors when:
/// - the plugin or locale is not active,
/// - the JSON is malformed,
/// - any value is not a string.
#[tauri::command]
fn slab_plugins_load_locale_bundle(
    plugin_id: String,
    locale: String,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let locales = plugins::active_locales(&reg);
    let entry = locales
        .into_iter()
        .find(|l| l.plugin_id == plugin_id && l.locale.locale == locale)
        .ok_or_else(|| format!("no active locale {locale:?} on plugin {plugin_id:?}"))?;
    plugins::load_locale_bundle(&entry.plugin_dir, &entry.locale.bundle)
}

/// Run a plugin-contributed command. Shell commands spawn `/bin/sh -c`
/// (Windows: `cmd /C`) under a 30s default timeout and return captured
/// stdout/stderr. URL commands return an outcome carrying the URL so
/// the frontend can dispatch through `tauri_plugin_opener`.
#[tauri::command]
fn slab_plugins_run_command(
    plugin_id: String,
    command_id: String,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<plugins::CommandOutcome, String> {
    let cmds = plugins::active_commands(&reg);
    let entry = cmds
        .into_iter()
        .find(|c| c.plugin_id == plugin_id && c.command.id == command_id)
        .ok_or_else(|| format!("no active command {command_id:?} on plugin {plugin_id:?}"))?;
    plugins::run_command(&entry).map_err(|e| e.to_string())
}

/// Validate a plugin-contributed AI provider by running the
/// materialiser (Foundry Slice 8). On success returns `Ok(())` — this
/// only checks that the contribution's `kind` is supported and that
/// the constructor accepts the manifest fields. It does **not** make
/// an HTTP call: header `$VAR` expansion is deferred to request time.
///
/// Used by the Settings → Plugins UI to surface a "misconfigured"
/// chip next to providers whose manifest is rejected.
#[tauri::command]
fn slab_plugins_validate_ai_provider(
    plugin_id: String,
    provider_id: String,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<(), String> {
    let providers = plugins::active_ai_providers(&reg);
    let entry = providers
        .into_iter()
        .find(|p| p.plugin_id == plugin_id && p.provider.id == provider_id)
        .ok_or_else(|| format!("no active ai_provider {provider_id:?} on plugin {plugin_id:?}"))?;
    plugins::materialize_active(&entry)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---------- Bench (v1.4.0) — marketplace Tauri commands ----------

/// Outcome shape for [`slab_marketplace_index`]. Mirrors the Rust
/// [`marketplace::FetchOutcome`] but as a flat struct that's easier
/// for the TS layer to consume (no tagged union).
#[derive(Debug, Clone, serde::Serialize)]
struct MarketplaceFetchResult {
    /// Index was fetched from the network this call.
    is_fresh: bool,
    /// Network failed but a cached copy was returned.
    is_stale: bool,
    /// The index itself when present (Fresh or Stale).
    index: Option<marketplace::Index>,
    /// Last-error string when present (Stale.network_error or Failed).
    error: Option<String>,
}

/// Fetch the curated plugin index from the maintainer repo with offline
/// cache fallback. Never throws — failures land in the `error` field
/// so the UI can render a friendly "showing cached results" banner.
///
/// The frontend `marketplaceStore` calls this on demand (Browse tab
/// mount + manual refresh button). We do **not** auto-refresh on boot
/// because the index is only relevant when the user opens the panel.
#[tauri::command]
async fn slab_marketplace_index() -> MarketplaceFetchResult {
    let client = marketplace::default_client();
    let cache_path = marketplace::default_cache_path();
    let outcome = marketplace::fetch_index_with_cache(
        &client,
        marketplace::DEFAULT_INDEX_URL,
        cache_path.as_deref(),
    )
    .await;
    match outcome {
        marketplace::FetchOutcome::Fresh(index) => MarketplaceFetchResult {
            is_fresh: true,
            is_stale: false,
            index: Some(index),
            error: None,
        },
        marketplace::FetchOutcome::Stale {
            index,
            network_error,
        } => MarketplaceFetchResult {
            is_fresh: false,
            is_stale: true,
            index: Some(index),
            error: Some(network_error.to_string()),
        },
        marketplace::FetchOutcome::Failed(e) => MarketplaceFetchResult {
            is_fresh: false,
            is_stale: false,
            index: None,
            error: Some(e.to_string()),
        },
    }
}

/// Install a plugin from a marketplace [`IndexEntry`]. We verify the
/// Ed25519 signature against the baked-in maintainer key **before**
/// touching the network — a bad sig fails fast and saves bandwidth.
/// After the install pipeline succeeds, we trigger a registry re-scan
/// so the new plugin shows up in the Installed list immediately.
///
/// The frontend is expected to have already filtered out untrusted
/// entries; we re-verify here as defence-in-depth.
#[tauri::command]
async fn slab_marketplace_install(
    entry: marketplace::IndexEntry,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<marketplace::InstallReport, String> {
    // 1) Signature check — never trust unsigned input.
    marketplace::verify_with_maintainer_key(&entry)
        .map_err(|e| format!("signature check failed: {e}"))?;

    // 2) Resolve plugins root (HOME-rooted).
    let plugins_root = plugins::default_plugins_root()
        .ok_or_else(|| "HOME env var not set; cannot locate ~/.slab/plugins".to_string())?;
    if let Err(e) = std::fs::create_dir_all(&plugins_root) {
        return Err(format!("could not create plugins dir: {e}"));
    }

    // 3) Download + extract via the install pipeline.
    let client = marketplace::default_client();
    let report = marketplace::install_from_entry(&client, &entry, &plugins_root)
        .await
        .map_err(|e| e.to_string())?;

    // 4) Refresh the registry so the new plugin appears in the UI.
    let enabled = plugins::default_state_path()
        .as_deref()
        .map(plugins::read_enabled_state)
        .unwrap_or_default();
    reg.discover(&plugins_root, &enabled);

    Ok(report)
}

/// Uninstall a marketplace-installed plugin by id. Synchronous because
/// the install module's uninstall is pure-filesystem. After removal,
/// re-discover so the registry drops the entry. Returns `false` if the
/// plugin wasn't installed in the first place.
#[tauri::command]
fn slab_marketplace_uninstall(
    id: String,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<bool, String> {
    let plugins_root = plugins::default_plugins_root()
        .ok_or_else(|| "HOME env var not set; cannot locate ~/.slab/plugins".to_string())?;
    let removed = marketplace::uninstall_plugin(&plugins_root, &id).map_err(|e| e.to_string())?;
    if removed {
        let enabled = plugins::default_state_path()
            .as_deref()
            .map(plugins::read_enabled_state)
            .unwrap_or_default();
        reg.discover(&plugins_root, &enabled);
    }
    Ok(removed)
}

// ─────────────────────────────────────────────────────────────────────
// Beacon Voice Mode (v1.9.0 Slice 15) — Tauri command surface.
// ─────────────────────────────────────────────────────────────────────

/// Probe what TTS engines are available on this host. Returns the
/// recommended engine for the platform plus the list of engines that
/// responded to a version probe. STT is always `false` in v1.9.0.
#[tauri::command]
fn slab_beacon_voice_capabilities() -> VoiceCapabilities {
    voice_capabilities()
}

/// List the voices the named engine knows about. The frontend uses
/// this to populate the voice picker in settings. Returns an empty Vec
/// if the engine isn't installed (the UI surfaces an "install hint"
/// based on `slab_beacon_voice_capabilities`).
#[tauri::command]
fn slab_beacon_voice_list_voices(engine: String) -> CmdResult<Vec<Voice>> {
    let eng = match TtsEngine::from_id(&engine) {
        Some(e) => e,
        None => {
            return CmdResult::Err {
                message: format!("unknown TTS engine: {engine}"),
            }
        }
    };
    if !voice_engine_is_installed(eng) {
        return CmdResult::Err {
            message: format!("{engine} is not installed on this host"),
        };
    }
    match voice_list_voices(eng) {
        Ok(v) => CmdResult::Ok { value: v },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Speak `text` through the chosen engine. Cancels any in-flight
/// utterance first (single-slot policy). Returns the new child PID
/// so the UI can correlate the "speaking now" badge if it wants.
///
/// `voice` and `rate_wpm` are optional — pass `None` to use the
/// engine's defaults.
#[tauri::command]
fn slab_beacon_voice_speak(
    engine: String,
    text: String,
    voice: Option<String>,
    rate_wpm: Option<u32>,
    session: tauri::State<'_, std::sync::Arc<VoiceSession>>,
) -> CmdResult<u32> {
    let eng = match TtsEngine::from_id(&engine) {
        Some(e) => e,
        None => {
            return CmdResult::Err {
                message: format!("unknown TTS engine: {engine}"),
            }
        }
    };
    if text.trim().is_empty() {
        return CmdResult::Err {
            message: "text is empty".into(),
        };
    }
    let opts = SpeakOpts { voice, rate_wpm };
    match session.speak(eng, &text, &opts) {
        Ok(pid) => CmdResult::Ok { value: pid },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Stop any in-flight utterance. Returns `true` if a process was
/// killed, `false` if nothing was speaking.
#[tauri::command]
fn slab_beacon_voice_stop(
    session: tauri::State<'_, std::sync::Arc<VoiceSession>>,
) -> CmdResult<bool> {
    CmdResult::Ok {
        value: session.stop(),
    }
}

/// Returns whether a speaker is currently active. The UI polls this
/// only when needed (on button-press) so the cost is bounded.
#[tauri::command]
fn slab_beacon_voice_is_speaking(
    session: tauri::State<'_, std::sync::Arc<VoiceSession>>,
) -> CmdResult<bool> {
    CmdResult::Ok {
        value: session.is_speaking(),
    }
}

/// Speak a short fixed test phrase so the user can sanity-check the
/// engine + voice + rate combination they just picked. Returns the
/// child PID for symmetry with `voice_speak`.
#[tauri::command]
fn slab_beacon_voice_test(
    engine: String,
    voice: Option<String>,
    rate_wpm: Option<u32>,
    session: tauri::State<'_, std::sync::Arc<VoiceSession>>,
) -> CmdResult<u32> {
    slab_beacon_voice_speak(
        engine,
        "Slab Beacon voice test. The quick brown fox jumps over the lazy dog.".into(),
        voice,
        rate_wpm,
        session,
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(windows::WindowRegistry::new())
        .manage(plugins::PluginRegistry::new())
        .manage(std::sync::Arc::new(VoiceSession::new()))
        .setup(|app| {
            // Cabinet (v1.1.0): restore last session's detached windows
            // from ~/.slab/windows.json. Quiet on error.
            let handle = tauri::Manager::app_handle(app).clone();
            windows::restore_windows(&handle);

            // Foundry (v1.3.0): discover plugins under ~/.slab/plugins
            // at boot. Quiet on error — if HOME is unset or the dir is
            // missing, registry stays empty and the UI shows an empty
            // panel rather than crashing.
            if let Some(root) = plugins::default_plugins_root() {
                let enabled = plugins::default_state_path()
                    .as_deref()
                    .map(plugins::read_enabled_state)
                    .unwrap_or_default();
                if let Some(reg) = tauri::Manager::try_state::<plugins::PluginRegistry>(app) {
                    reg.discover(&root, &enabled);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            slab_merge,
            slab_split_ranges,
            slab_split_every,
            slab_split_by_pattern,
            slab_find_matching_pages,
            slab_outline_starts,
            slab_page_count,
            slab_rotate,
            slab_delete_pages,
            slab_duplicate_pages,
            slab_reorder_pages,
            slab_pages_build,
            slab_find_text_spans,
            slab_replace_text_span,
            slab_diff_pdfs,
            slab_diff_export_report,
            slab_slides_analyze,
            slab_theater_export_annotated,
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
            slab_scan_audit,
            slab_extract_tables,
            slab_table_to_csv,
            slab_table_save_csv,
            slab_polyglot,
            slab_flatten,
            slab_sanitize,
            slab_repair,
            slab_beacon_config_read,
            slab_beacon_config_write,
            slab_ui_config_read,
            slab_ui_config_write,
            slab_keymap_read,
            slab_keymap_write,
            slab_keymap_reset,
            slab_beacon_provider_test,
            slab_beacon_provider_kinds,
            slab_beacon_chat,
            slab_beacon_summary,
            slab_beacon_propose_outline,
            slab_beacon_find_citations,
            slab_beacon_build_glossary,
            slab_beacon_load_glossary_cache,
            slab_beacon_clear_glossary_cache,
            slab_beacon_generate_deck,
            slab_beacon_study_due,
            slab_beacon_study_review,
            slab_beacon_study_stats,
            slab_beacon_diff_summary,
            slab_beacon_index_pdf,
            slab_beacon_search,
            slab_beacon_index_stats,
            slab_beacon_index_forget,
            slab_beacon_pii_find,
            slab_beacon_pii_redact,
            slab_beacon_selection_action,
            slab_beacon_vision_ask,
            slab_export_annotations_md,
            slab_library_add_folder,
            slab_library_remove_folder,
            slab_library_list_folders,
            slab_library_scan,
            slab_library_list_docs,
            slab_library_list_tags,
            slab_library_add_tag,
            slab_library_set_doc_tags,
            slab_library_remove_document,
            slab_library_remove_tag,
            slab_library_rescan_all,
            slab_library_ocr_queue_list_pending,
            slab_library_ocr_queue_run_one,
            slab_library_ocr_queue_run_all,
            slab_library_auto_tag_one,
            slab_library_auto_tag_many,
            windows::slab_window_open,
            windows::slab_window_close,
            windows::slab_window_list,
            slab_request_open_in_main,
            slab_plugins_list,
            slab_plugins_set_enabled,
            slab_plugins_reload,
            slab_plugins_dir,
            slab_plugins_active_themes,
            slab_plugins_active_locales,
            slab_plugins_active_commands,
            slab_plugins_active_ai_providers,
            slab_plugins_active_pdf_actions,
            slab_plugins_read_asset,
            slab_plugins_run_pdf_action,
            slab_plugins_load_locale_bundle,
            slab_plugins_run_command,
            slab_plugins_validate_ai_provider,
            slab_marketplace_index,
            slab_marketplace_install,
            slab_marketplace_uninstall,
            slab_beacon_voice_capabilities,
            slab_beacon_voice_list_voices,
            slab_beacon_voice_speak,
            slab_beacon_voice_stop,
            slab_beacon_voice_is_speaking,
            slab_beacon_voice_test,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Slab");
}
