// Slab — fast, free, offline PDF tool.
// All operations run locally; nothing is ever uploaded.

pub mod ai;
pub mod first_launch;
pub mod keymap;
pub mod marketplace;
pub mod pdf;
pub mod plugins;
pub mod theater;
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
    IndexReport, IndexStats, IndexedPdfRecord, ModelBucket, SearchHit,
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
use ai::stt::{capabilities as stt_capabilities, SttCapabilities, SttEngine, Transcript};
use ai::stt_session::SttSession;
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
use pdf::bates::{apply_bates as do_apply_bates, BatesOpts, BatesReport};
use pdf::bates_batch::{
    apply_bates_batch as do_apply_bates_batch, BatchInput as BatesBatchInput,
    BatchReport as BatesBatchReport,
};
use pdf::compactor::{
    compact as do_compact, estimate as do_compactor_estimate, CompactOptions, CompactPreset,
    CompactReport, EstimateReport,
};
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
use pdf::legal_stamp::{
    apply_legal_stamp as do_apply_legal_stamp, LegalStampOpts, LegalStampReport,
};
use pdf::library::{
    auto_tag_run_many as do_auto_tag_run_many, auto_tag_run_one as do_auto_tag_run_one,
    default_db_path as library_default_db_path, ocr_queue_list_failed as do_ocr_queue_list_failed,
    ocr_queue_list_pending as do_ocr_queue_list,
    ocr_queue_requeue_all_failed as do_ocr_queue_requeue_all_failed,
    ocr_queue_requeue_doc as do_ocr_queue_requeue_doc, ocr_queue_run_all as do_ocr_queue_run_all,
    ocr_queue_run_one as do_ocr_queue_run_one, ocr_queue_stats as do_ocr_queue_stats,
    query_documents as do_query_documents, scan_folder as do_scan_folder, AutoTagRunResult,
    DocumentRecord, FolderRecord, LibraryDb, LibraryError, LibraryFilter, OcrQueueResult,
    OcrQueueStats, ScanReport, TagRecord,
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
use pdf::pages::{delete_pages, reorder_pages, rotate_pages, rotate_pages_permanent, Rotation};
use pdf::pages_build::{pages_build as do_pages_build, PagesBuildOpts};
use pdf::pdfa::{
    convert::{
        convert_to_pdfa as do_pdfa_convert, ConvertOpts as PdfAConvertOpts,
        ConvertReport as PdfAConvertReport,
    },
    font_audit::{audit_fonts as do_pdfa_font_audit, FontAuditReport as PdfAFontAuditReport},
    inspect::{inspect_pdfa as do_pdfa_inspect, InspectionReport as PdfAInspectionReport},
    validate::{validate_pdfa as do_pdfa_validate, ValidationReport as PdfAValidationReport},
    ConformanceLevel as PdfAConformanceLevel,
};
use pdf::polyglot::{polyglot_to_pdf as do_polyglot, PolyglotOpts, PolyglotReport};
use pdf::redact::{redact as do_redact, RedactOpts};
use pdf::redact_true::{redact_true as do_redact_true, TrueRedactReport};
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

// ───── First-launch self-install (issue #25, v2.0.3) ──────────────

/// Snapshot of first-launch state surfaced to the UI.
#[derive(serde::Serialize)]
struct FirstLaunchProbe {
    should_prompt: bool,
    decision: first_launch::LaunchDecision,
    /// True if the running exe is in a "looks temporary" location
    /// (Downloads, Desktop, mounted DMG, /tmp, AppImage mount).
    /// Used by the modal to escalate the "Install" CTA.
    looks_temporary: bool,
    /// Where Slab *would* live post-install (display-only).
    canonical_install_dir: Option<PathBuf>,
}

#[tauri::command]
fn slab_first_launch_probe() -> CmdResult<FirstLaunchProbe> {
    let probe = first_launch::OsProbe;
    use first_launch::Probe;
    let state = probe
        .state_path()
        .and_then(|p| first_launch::state::load(&p).ok())
        .unwrap_or_default();
    let exe = probe.current_exe().ok();
    let looks_temporary = match exe.as_deref() {
        #[cfg(target_os = "macos")]
        Some(p) => first_launch::macos::looks_like_temporary_location(p),
        #[cfg(target_os = "windows")]
        Some(p) => first_launch::windows::looks_like_temporary_location(p),
        #[cfg(target_os = "linux")]
        Some(p) => first_launch::linux::looks_like_temporary_location(p),
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        Some(_) => false,
        None => false,
    };
    CmdResult::Ok {
        value: FirstLaunchProbe {
            should_prompt: first_launch::should_prompt(&probe),
            decision: state.decision,
            looks_temporary,
            canonical_install_dir: probe.canonical_install_dir(),
        },
    }
}

#[tauri::command]
fn slab_first_launch_skip() -> CmdResult<()> {
    let probe = first_launch::OsProbe;
    use first_launch::Probe;
    let Some(state_path) = probe.state_path() else {
        return CmdResult::Err {
            message: "no HOME directory — cannot persist decision".into(),
        };
    };
    let mut st = first_launch::state::load(&state_path).unwrap_or_default();
    st.decision = first_launch::LaunchDecision::RunFromHere;
    match first_launch::state::save(&state_path, &st) {
        Ok(()) => CmdResult::Ok { value: () },
        Err(e) => CmdResult::Err {
            message: format!("save launch state: {e}"),
        },
    }
}

#[tauri::command]
fn slab_first_launch_install() -> CmdResult<PathBuf> {
    let probe = first_launch::OsProbe;
    use first_launch::Probe;
    let Some(state_path) = probe.state_path() else {
        return CmdResult::Err {
            message: "no HOME directory — cannot persist decision".into(),
        };
    };
    let installed: std::io::Result<PathBuf> = {
        #[cfg(target_os = "macos")]
        {
            first_launch::macos::install()
        }
        #[cfg(target_os = "windows")]
        {
            first_launch::windows::install()
        }
        #[cfg(target_os = "linux")]
        {
            first_launch::linux::install()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "self-install not supported on this OS",
            ))
        }
    };
    match installed {
        Ok(path) => {
            let st = first_launch::LaunchState {
                decision: first_launch::LaunchDecision::Installed,
                installed_at: Some(rfc3339_now()),
                installed_path: Some(path.clone()),
                ..Default::default()
            };
            if let Err(e) = first_launch::state::save(&state_path, &st) {
                return CmdResult::Err {
                    message: format!("install ok but state save failed: {e}"),
                };
            }
            CmdResult::Ok { value: path }
        }
        Err(e) => CmdResult::Err {
            message: format!("install failed: {e}"),
        },
    }
}

/// Lightweight RFC 3339 timestamp using SystemTime → seconds since
/// epoch, formatted as `YYYY-MM-DDTHH:MM:SSZ`. Avoids pulling chrono.
fn rfc3339_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day, hour, min, sec) = epoch_to_ymdhms(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Pure-arithmetic UTC breakdown — good enough for a one-off timestamp.
fn epoch_to_ymdhms(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let hour = (sod / 3600) as u32;
    let min = ((sod % 3600) / 60) as u32;
    let sec = (sod % 60) as u32;
    // Civil-from-days algorithm (Howard Hinnant, public domain).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (year, m, d, hour, min, sec)
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

/// Stamp Bates numbers (prefix + zero-padded counter) onto every page of
/// one PDF. The buyer-magnet litigation discovery feature.
#[tauri::command]
fn slab_bates_apply(input: PathBuf, output: PathBuf, opts: BatesOpts) -> CmdResult<BatesReport> {
    do_apply_bates(&input, &output, &opts).into()
}

/// Apply Bates numbering across a whole production set — ordered list of
/// PDFs, monotonic counter chained across files, optional CSV/JSON load
/// file for Relativity / Concordance / Everlaw ingest.
#[tauri::command]
fn slab_bates_batch(input: BatesBatchInput) -> CmdResult<BatesBatchReport> {
    do_apply_bates_batch(&input).into()
}

/// Apply a diagonal legal stamp (CONFIDENTIAL / ATTORNEY EYES ONLY /
/// PRIVILEGED & CONFIDENTIAL / DRAFT, or custom text) to the document.
#[tauri::command]
fn slab_legal_stamp_apply(
    input: PathBuf,
    output: PathBuf,
    opts: LegalStampOpts,
) -> CmdResult<LegalStampReport> {
    do_apply_legal_stamp(&input, &output, &opts).into()
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

/// Lightweight summary used by the Loom panel so it can render counts
/// without shipping every individual run + image bbox over the IPC bridge.
#[derive(Serialize)]
pub struct LoomLayoutSummary {
    pub pages: Vec<LoomPageSummary>,
    pub total_runs: usize,
    pub total_images: usize,
}

#[derive(Serialize)]
pub struct LoomPageSummary {
    pub page_number: u32,
    pub width: f32,
    pub height: f32,
    pub run_count: usize,
    pub image_count: usize,
    /// Approximate distinct font sizes used on this page, rounded to 1pt,
    /// sorted descending. Useful for heading-detection diagnostics.
    pub distinct_font_sizes: Vec<u32>,
}

#[tauri::command]
fn slab_loom_layout_summary(input: PathBuf) -> CmdResult<LoomLayoutSummary> {
    use crate::pdf::loom::extract_layout;
    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            return CmdResult::Err {
                message: format!("read {}: {}", input.display(), e),
            };
        }
    };
    let tree = match extract_layout(&bytes) {
        Ok(t) => t,
        Err(e) => return CmdResult::Err { message: e },
    };
    let pages = tree
        .pages
        .iter()
        .map(|p| {
            let mut sizes: Vec<u32> = p.runs.iter().map(|r| r.font_size.round() as u32).collect();
            sizes.sort_unstable();
            sizes.dedup();
            sizes.reverse();
            sizes.truncate(8);
            LoomPageSummary {
                page_number: p.page_number,
                width: p.width,
                height: p.height,
                run_count: p.runs.len(),
                image_count: p.images.len(),
                distinct_font_sizes: sizes,
            }
        })
        .collect();
    let total_runs = tree.total_runs();
    let total_images = tree.total_images();
    CmdResult::Ok {
        value: LoomLayoutSummary {
            pages,
            total_runs,
            total_images,
        },
    }
}

/// Matterhorn registry digest for the Loom panel's Conformance tab.
#[derive(Serialize)]
pub struct LoomMatterhornDigest {
    pub protocol_version: &'static str,
    pub applies_to: &'static str,
    pub registry_total: usize,
    pub full_protocol_total: usize,
    pub auto: usize,
    pub human: usize,
    pub out_of_scope: usize,
    pub auto_share_of_full_protocol: f64,
}

#[tauri::command]
fn slab_loom_matterhorn_digest() -> CmdResult<LoomMatterhornDigest> {
    use crate::pdf::loom::{CoverageSnapshot, APPLIES_TO, PROTOCOL_VERSION};
    let snap = CoverageSnapshot::from_registry();
    CmdResult::Ok {
        value: LoomMatterhornDigest {
            protocol_version: PROTOCOL_VERSION,
            applies_to: APPLIES_TO,
            registry_total: snap.registry_total,
            full_protocol_total: snap.full_protocol_total,
            auto: snap.auto,
            human: snap.human,
            out_of_scope: snap.out_of_scope,
            auto_share_of_full_protocol: snap.auto_share_of_full_protocol,
        },
    }
}

/// Slice 2 result surfaced to the LoomPanel: per-page node counts plus a
/// few sample headings so the UI can show what the classifier actually
/// thinks the document's outline looks like.
#[derive(Serialize)]
pub struct LoomClassifySummary {
    pub total_pages: usize,
    pub total_nodes: usize,
    pub heading_count: usize,
    pub paragraph_count: usize,
    pub list_count: usize,
    pub list_item_count: usize,
    pub figure_count: usize,
    pub artifact_count: usize,
    /// Up to 20 detected headings as `{level, text}`, in document order.
    pub headings: Vec<LoomClassifyHeading>,
    pub pages: Vec<LoomClassifyPage>,
}

#[derive(Serialize)]
pub struct LoomClassifyHeading {
    pub page: u32,
    pub level: u8,
    pub text: String,
}

#[derive(Serialize)]
pub struct LoomClassifyPage {
    pub page_number: u32,
    pub headings: usize,
    pub paragraphs: usize,
    pub list_items: usize,
    pub figures: usize,
    pub artifacts: usize,
}

#[tauri::command]
fn slab_loom_classify_summary(input: PathBuf) -> CmdResult<LoomClassifySummary> {
    use crate::pdf::loom::{classify, extract_layout, NodeKind, StructNode};
    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            return CmdResult::Err {
                message: format!("read {}: {}", input.display(), e),
            };
        }
    };
    let layout = match extract_layout(&bytes) {
        Ok(t) => t,
        Err(e) => return CmdResult::Err { message: e },
    };
    let tree = classify(&layout);

    fn walk_collect(
        nodes: &[StructNode],
        page: u32,
        per_page: &mut LoomClassifyPage,
        headings: &mut Vec<LoomClassifyHeading>,
        totals: &mut (usize, usize, usize, usize, usize, usize, usize),
    ) {
        for n in nodes {
            totals.0 += 1; // total
            match n.kind {
                NodeKind::Heading(level) => {
                    totals.1 += 1;
                    per_page.headings += 1;
                    if headings.len() < 20 {
                        let mut text = n.text.trim().to_string();
                        if text.len() > 120 {
                            text.truncate(117);
                            text.push_str("...");
                        }
                        headings.push(LoomClassifyHeading { page, level, text });
                    }
                }
                NodeKind::Paragraph => {
                    totals.2 += 1;
                    per_page.paragraphs += 1;
                }
                NodeKind::List => {
                    totals.3 += 1;
                }
                NodeKind::ListItem => {
                    totals.4 += 1;
                    per_page.list_items += 1;
                }
                NodeKind::Figure => {
                    totals.5 += 1;
                    per_page.figures += 1;
                }
                NodeKind::Artifact => {
                    totals.6 += 1;
                    per_page.artifacts += 1;
                }
                _ => {}
            }
            walk_collect(&n.children, page, per_page, headings, totals);
        }
    }

    let mut totals = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut headings = Vec::new();
    let mut pages = Vec::with_capacity(tree.pages.len());
    for p in &tree.pages {
        let mut pp = LoomClassifyPage {
            page_number: p.page_number,
            headings: 0,
            paragraphs: 0,
            list_items: 0,
            figures: 0,
            artifacts: 0,
        };
        walk_collect(&p.nodes, p.page_number, &mut pp, &mut headings, &mut totals);
        pages.push(pp);
    }
    CmdResult::Ok {
        value: LoomClassifySummary {
            total_pages: tree.pages.len(),
            total_nodes: totals.0,
            heading_count: totals.1,
            paragraph_count: totals.2,
            list_count: totals.3,
            list_item_count: totals.4,
            figure_count: totals.5,
            artifact_count: totals.6,
            headings,
            pages,
        },
    }
}

/// Slice 3 result for the LoomPanel Outline tab: per-page column count
/// + sample of the first N reading-order labels so users can see Loom's
///   re-ordering live.
#[derive(Serialize)]
pub struct LoomReadingOrderSummary {
    pub total_pages: usize,
    /// Pages on which Loom detected ≥ 2 narrow column bands.
    pub multi_column_pages: usize,
    /// Total reading-flow nodes (excludes artifacts).
    pub total_reading_nodes: usize,
    /// Total page-spanning nodes (figures, headings that cross columns,
    /// full-width banners).
    pub total_spanners: usize,
    pub pages: Vec<LoomReadingOrderPage>,
    /// First up to 40 reading-flow node labels in correct order across
    /// the document. Useful preview for the LoomPanel.
    pub flow_preview: Vec<LoomReadingOrderFlowEntry>,
}

#[derive(Serialize)]
pub struct LoomReadingOrderPage {
    pub page_number: u32,
    pub column_count: usize,
    pub spanner_count: usize,
    pub artifact_count: usize,
    pub reading_node_count: usize,
}

#[derive(Serialize)]
pub struct LoomReadingOrderFlowEntry {
    pub page: u32,
    /// PDF tag (P, H1..H6, L, LI, Figure, Caption, Artifact).
    pub tag: &'static str,
    /// First 80 chars of the node text.
    pub text: String,
}

#[tauri::command]
fn slab_loom_reading_order_summary(input: PathBuf) -> CmdResult<LoomReadingOrderSummary> {
    use crate::pdf::loom::{classify, extract_layout, order_reading, NodeKind, StructNode};
    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            return CmdResult::Err {
                message: format!("read {}: {}", input.display(), e),
            };
        }
    };
    let layout = match extract_layout(&bytes) {
        Ok(t) => t,
        Err(e) => return CmdResult::Err { message: e },
    };
    let tree = classify(&layout);
    let geometry: Vec<(f32, f32)> = layout.pages.iter().map(|p| (p.width, p.height)).collect();
    let order = order_reading(&tree, &geometry);

    fn trunc(s: &str) -> String {
        let t = s.trim();
        if t.chars().count() > 80 {
            let mut out: String = t.chars().take(77).collect();
            out.push_str("...");
            out
        } else {
            t.to_string()
        }
    }

    fn flatten<'a>(out: &mut Vec<&'a StructNode>, nodes: &'a [StructNode]) {
        for n in nodes {
            out.push(n);
            flatten(out, &n.children);
        }
    }

    let mut total_reading_nodes = 0usize;
    let mut total_spanners = 0usize;
    let mut pages = Vec::with_capacity(order.pages.len());
    let mut flow_preview: Vec<LoomReadingOrderFlowEntry> = Vec::new();
    for p in &order.pages {
        let mut flat: Vec<&StructNode> = Vec::new();
        flatten(&mut flat, &p.nodes);
        let reading_node_count = flat
            .iter()
            .filter(|n| !matches!(n.kind, NodeKind::Artifact))
            .count();
        total_reading_nodes += reading_node_count;
        total_spanners += p.spanner_count;
        pages.push(LoomReadingOrderPage {
            page_number: p.page_number,
            column_count: p.column_count,
            spanner_count: p.spanner_count,
            artifact_count: p.artifact_count,
            reading_node_count,
        });
        for n in flat.into_iter() {
            if flow_preview.len() >= 40 {
                break;
            }
            // Skip empty figure placeholders + artifacts in the preview —
            // they'd add noise without showing the reading order.
            let text = trunc(&n.text);
            if text.is_empty() && !matches!(n.kind, NodeKind::Figure) {
                continue;
            }
            flow_preview.push(LoomReadingOrderFlowEntry {
                page: p.page_number,
                tag: n.kind.tag(),
                text,
            });
        }
    }
    CmdResult::Ok {
        value: LoomReadingOrderSummary {
            total_pages: order.pages.len(),
            multi_column_pages: order.multi_column_pages(),
            total_reading_nodes,
            total_spanners,
            pages,
            flow_preview,
        },
    }
}

/// Slice 4 result for the LoomPanel "Alt-text" tab: per-figure
/// alt-text plus cache + error stats.
#[derive(Serialize)]
pub struct LoomAltTextSummary {
    pub figures_total: usize,
    pub generated: usize,
    pub cache_hits: usize,
    pub skipped_tiny: usize,
    pub skipped_preexisting: usize,
    pub errors: usize,
    pub elapsed_ms: u64,
    /// First up to 20 figures with their generated/cached alt-text.
    pub samples: Vec<LoomAltTextSample>,
}

#[derive(Serialize)]
pub struct LoomAltTextSample {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub alt_text: String,
}

#[tauri::command]
async fn slab_loom_alt_text_summary(input: PathBuf) -> CmdResult<LoomAltTextSummary> {
    use crate::pdf::loom::{
        classify, default_alt_text_cache_dir, enrich_with_alt_text, extract_layout, AltTextOptions,
        NodeKind, StructNode,
    };

    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            return CmdResult::Err {
                message: format!("read {}: {}", input.display(), e),
            };
        }
    };
    let layout = match extract_layout(&bytes) {
        Ok(t) => t,
        Err(e) => return CmdResult::Err { message: e },
    };
    let mut tree = classify(&layout);

    let cfg = match do_load_beacon_config() {
        Ok(c) => c,
        Err(e) => {
            return CmdResult::Err {
                message: format!("load Beacon config: {e}"),
            }
        }
    };
    let provider = match ai::config::make_provider(&cfg.beacon) {
        Ok(p) => p,
        Err(e) => {
            return CmdResult::Err {
                message: format!("build provider: {e}"),
            }
        }
    };

    let cache_dir = default_alt_text_cache_dir();
    let opts = AltTextOptions::default();
    let started = std::time::Instant::now();
    let stats = match enrich_with_alt_text(&input, &mut tree, provider, &opts, &cache_dir).await {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: format!("alt-text enrich: {e}"),
            }
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;

    // Collect up to 20 samples.
    fn collect_samples(
        nodes: &[StructNode],
        page: u32,
        out: &mut Vec<LoomAltTextSample>,
        cap: usize,
    ) {
        for n in nodes {
            if out.len() >= cap {
                return;
            }
            if matches!(n.kind, NodeKind::Figure) {
                if let Some(alt) = &n.alt_text {
                    out.push(LoomAltTextSample {
                        page,
                        x: n.bbox.x0,
                        y: n.bbox.y0,
                        width: n.bbox.x1 - n.bbox.x0,
                        height: n.bbox.y1 - n.bbox.y0,
                        alt_text: alt.clone(),
                    });
                }
            }
            collect_samples(&n.children, page, out, cap);
        }
    }
    let mut samples = Vec::new();
    for p in &tree.pages {
        if samples.len() >= 20 {
            break;
        }
        collect_samples(&p.nodes, p.page_number, &mut samples, 20);
    }

    CmdResult::Ok {
        value: LoomAltTextSummary {
            figures_total: stats.figures_total,
            generated: stats.generated,
            cache_hits: stats.cache_hits,
            skipped_tiny: stats.skipped_tiny,
            skipped_preexisting: stats.skipped_preexisting,
            errors: stats.errors,
            elapsed_ms,
            samples,
        },
    }
}

#[derive(Debug, Serialize)]
pub struct LoomTagResult {
    pub output_path: String,
    pub elapsed_ms: u64,
    pub pages_processed: usize,
    pub pages_skipped: usize,
    pub bdc_pairs_injected: usize,
    pub struct_elems_created: usize,
    pub figures_with_alt_text: usize,
    /// Slice 6: post-tag validator report. Auto-run on the in-memory tagged
    /// doc so the UI can render a "Validated ✓ ISO 14289-1" sub-badge.
    pub validation: crate::pdf::loom::validate::ValidateReport,
    /// Slice 6: metadata stats (title applied? lang set? xmp size).
    pub metadata: crate::pdf::loom::metadata::MetadataStats,
}

/// Slice 5: tag a PDF for PDF/UA-1 conformance and write `<name>.tagged.pdf`.
///
/// Runs the full Loom pipeline: layout → classify → reading_order →
/// (best-effort) alt-text → structure_tree::weave. Best-effort alt-text means
/// if Beacon is unavailable the tagging still ships — figures just get
/// generic alt placeholders left to the human-review pass.
#[tauri::command]
async fn slab_loom_tag_document(input: PathBuf) -> CmdResult<LoomTagResult> {
    use crate::pdf::loom::{
        classify, default_alt_text_cache_dir, enrich_with_alt_text, extract_layout,
        metadata::{apply_pdfua_metadata, MetadataOptions},
        order_reading,
        structure_tree::{weave, WeaveOptions},
        validate::validate as run_validate,
        AltTextOptions,
    };
    use lopdf::Document;

    let started = std::time::Instant::now();
    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            return CmdResult::Err {
                message: format!("read {}: {}", input.display(), e),
            };
        }
    };
    let layout = match extract_layout(&bytes) {
        Ok(l) => l,
        Err(e) => return CmdResult::Err { message: e },
    };
    let mut tree = classify(&layout);
    let page_geom: Vec<(f32, f32)> = layout.pages.iter().map(|p| (p.width, p.height)).collect();
    let order = order_reading(&tree, &page_geom);

    // Best-effort alt-text — non-fatal on Beacon unavailability.
    if let Ok(cfg) = do_load_beacon_config() {
        if let Ok(provider) = ai::config::make_provider(&cfg.beacon) {
            let cache_dir = default_alt_text_cache_dir();
            let _ = enrich_with_alt_text(
                &input,
                &mut tree,
                provider,
                &AltTextOptions::default(),
                &cache_dir,
            )
            .await;
        }
    }

    let mut doc = match Document::load_mem(&bytes) {
        Ok(d) => d,
        Err(e) => {
            return CmdResult::Err {
                message: format!("load PDF: {}", e),
            };
        }
    };
    let opts = WeaveOptions {
        fallback_lang: Some("en-US".into()),
    };
    let stats = match weave(&mut doc, &tree, &order, &opts) {
        Ok(s) => s,
        Err(e) => {
            return CmdResult::Err {
                message: format!("weave: {}", e),
            };
        }
    };

    // Slice 6: apply PDF/UA-1 metadata (XMP packet + ViewerPreferences).
    let title = input
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    let meta_stats = match apply_pdfua_metadata(
        &mut doc,
        &MetadataOptions {
            title,
            fallback_lang: Some("en-US".into()),
            ..Default::default()
        },
    ) {
        Ok(m) => m,
        Err(e) => {
            return CmdResult::Err {
                message: format!("metadata: {}", e),
            };
        }
    };

    // Slice 6: validate the in-memory tagged doc so the UI can render a
    // "Validated ✓ ISO 14289-1" sub-badge with per-condition checkmarks.
    let validation = run_validate(&doc);

    let out_path = {
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");
        let dir = input.parent().unwrap_or(std::path::Path::new("."));
        dir.join(format!("{}.tagged.pdf", stem))
    };
    if let Err(e) = doc.save(&out_path) {
        return CmdResult::Err {
            message: format!("save {}: {}", out_path.display(), e),
        };
    }

    CmdResult::Ok {
        value: LoomTagResult {
            output_path: out_path.display().to_string(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            pages_processed: stats.pages_processed,
            pages_skipped: stats.pages_skipped,
            bdc_pairs_injected: stats.bdc_pairs_injected,
            struct_elems_created: stats.struct_elems_created,
            figures_with_alt_text: stats.figures_with_alt_text,
            validation,
            metadata: meta_stats,
        },
    }
}

/// Slice 6: validate a PDF against ISO 14289-1 (PDF/UA-1).
///
/// Runs the 8 auto-decidable Matterhorn conditions against any PDF — the
/// freshly-tagged Slab output, an Acrobat-tagged file, or an arbitrary PDF
/// the user dropped onto the panel. Returns a `ValidateReport` the UI
/// renders as a green/red conformance card with per-condition checkmarks.
#[tauri::command]
fn slab_loom_validate(input: PathBuf) -> CmdResult<crate::pdf::loom::validate::ValidateReport> {
    use crate::pdf::loom::validate::validate as run_validate;
    use lopdf::Document;

    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            return CmdResult::Err {
                message: format!("read {}: {}", input.display(), e),
            };
        }
    };
    let doc = match Document::load_mem(&bytes) {
        Ok(d) => d,
        Err(e) => {
            return CmdResult::Err {
                message: format!("load PDF: {}", e),
            };
        }
    };
    CmdResult::Ok {
        value: run_validate(&doc),
    }
}

/// v3.8.0 Press: one-click PDF/X-4 conversion (ISO 15930-7).
///
/// Takes any PDF and produces a fully PDF/X-4 compliant document offline:
/// strips JavaScript, embeds Standard-14 fonts, installs ICC default
/// colour spaces, synthesizes TrimBox + optional 3mm bleed, writes the
/// PDF/X-4 XMP metadata packet (pdfxid namespace), and adds the
/// `/Catalog /OutputIntents` entry with /S /GTS_PDFX backed by the
/// vendored FOGRA51 or GRACoL2013 ICC profile.
///
/// `intent` accepts `"fogra51"` or `"gracol2013"`.
#[tauri::command]
fn slab_press_convert(
    input: PathBuf,
    output: PathBuf,
    intent: String,
    add_bleed: bool,
    title: Option<String>,
) -> CmdResult<PressConvertReportDto> {
    use crate::pdf::press::{convert_to_pdfx4, ConvertOptions, OutputIntent};

    let intent_enum = match OutputIntent::from_wire(&intent) {
        Some(i) => i,
        None => {
            return CmdResult::Err {
                message: format!(
                    "unknown intent {:?}; expected \"fogra51\" or \"gracol2013\"",
                    intent
                ),
            };
        }
    };

    let opts = ConvertOptions {
        output_intent: intent_enum,
        add_bleed,
        title,
        creator_tool: None,
    };
    match convert_to_pdfx4(&input, &output, &opts) {
        Ok(r) => CmdResult::Ok {
            value: PressConvertReportDto::from(r),
        },
        Err(e) => CmdResult::Err { message: e },
    }
}

/// Frontend-friendly view of `pdf::press::ConvertReport`. ObjectIds in
/// the internal report aren't serde-friendly, so this DTO flattens the
/// stats the UI actually wants.
#[derive(Debug, serde::Serialize)]
pub struct PressConvertReportDto {
    output_path: String,
    elapsed_ms: u128,
    fonts_embedded: usize,
    javascript_stripped: usize,
    annotations_sanitized: usize,
    color_pages_touched: usize,
    color_default_entries_added: usize,
    trimbox_synthesized: usize,
    trimbox_preserved: usize,
    bleed_added: usize,
    intent_label: String,
}

impl From<crate::pdf::press::ConvertReport> for PressConvertReportDto {
    fn from(r: crate::pdf::press::ConvertReport) -> Self {
        Self {
            output_path: r.output_path.display().to_string(),
            elapsed_ms: r.elapsed_ms,
            fonts_embedded: r.fonts_embedded,
            javascript_stripped: r.javascript_stripped,
            annotations_sanitized: r.annotations_sanitized,
            color_pages_touched: r.color_pages_touched,
            color_default_entries_added: r.color_default_entries_added,
            trimbox_synthesized: r.trimbox_synthesized,
            trimbox_preserved: r.trimbox_preserved,
            bleed_added: r.bleed_added,
            intent_label: r.intent_label,
        }
    }
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
fn slab_rotate_permanent(
    input: PathBuf,
    pages: Vec<u32>,
    degrees: i64,
    output: PathBuf,
) -> CmdResult<u32> {
    match Rotation::from_int(degrees) {
        Ok(rot) => rotate_pages_permanent(&input, &pages, rot, &output).into(),
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
fn slab_apply_page_ops(
    input: PathBuf,
    ops: Vec<crate::pdf::pages_undo::PageOp>,
    output: PathBuf,
) -> CmdResult<()> {
    crate::pdf::pages_undo::apply_ops(&input, &ops, &output).into()
}

#[tauri::command]
fn slab_pages_build(input: PathBuf, opts: PagesBuildOpts, output: PathBuf) -> CmdResult<u32> {
    do_pages_build(&input, &opts, &output).into()
}

// --- v3.9.0 "Quill" — AcroForm inspect + fill ------------------------------

#[tauri::command]
fn slab_forms_inspect(input: PathBuf) -> CmdResult<crate::pdf::forms::FormsReport> {
    crate::pdf::forms::inspect(&input).into()
}

#[tauri::command]
fn slab_forms_fill(
    input: PathBuf,
    values: std::collections::HashMap<String, String>,
    output: PathBuf,
) -> CmdResult<crate::pdf::forms::FillReport> {
    crate::pdf::forms::fill(&input, &values, &output).into()
}

// --- v3.25.0 "Quill Pro" — batch CSV form-fill (mail-merge) ----------------

#[tauri::command]
async fn slab_forms_batch_fill(
    spec: crate::pdf::forms_batch::BatchSpec,
) -> Result<crate::pdf::forms_batch::BatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || crate::pdf::forms_batch::run_batch(&spec))
        .await
        .map_err(|e| e.to_string())?
}

// --- v3.26.0 "Quill Designer" — author AcroForm fields ---------------------

#[tauri::command]
fn slab_forms_design_add(
    input: PathBuf,
    drafts: Vec<crate::pdf::forms_design::FieldDraft>,
    output: PathBuf,
) -> CmdResult<crate::pdf::forms_design::DesignReport> {
    crate::pdf::forms_design::add_fields(&input, &drafts, &output).into()
}

#[tauri::command]
fn slab_forms_design_edit(
    input: PathBuf,
    edits: Vec<crate::pdf::forms_design::FieldEdit>,
    output: PathBuf,
) -> CmdResult<crate::pdf::forms_design::DesignReport> {
    crate::pdf::forms_design::edit_fields(&input, &edits, &output).into()
}

#[tauri::command]
fn slab_forms_design_delete(
    input: PathBuf,
    names: Vec<String>,
    output: PathBuf,
) -> CmdResult<crate::pdf::forms_design::DesignReport> {
    crate::pdf::forms_design::delete_fields(&input, &names, &output).into()
}

// --- v3.27.0 "Quill Auto-Detect" — find candidate fields on flat PDFs ------

#[tauri::command]
fn slab_forms_autodetect(input: PathBuf) -> CmdResult<crate::pdf::forms_detect::DetectionReport> {
    crate::pdf::forms_detect::detect(&input).into()
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

/// v3.23.0 "Stack" — export a shareable **redline PDF** that bakes the
/// word-level diff (green inserts / red strikethrough deletes) into a
/// single document any PDF viewer can open. The buyer-magnet feature:
/// recipients don't need Slab to read the redline.
#[derive(Debug, Clone, Copy, serde::Serialize)]
struct StackRedlineSummary {
    pages: u32,
    inserts: u32,
    deletes: u32,
}

#[tauri::command]
fn slab_stack_export_redline(
    old: PathBuf,
    new: PathBuf,
    output: PathBuf,
) -> CmdResult<StackRedlineSummary> {
    match do_diff_pdfs(&old, &new) {
        Ok(d) => crate::pdf::stack_redline::export_redline(&d, &output)
            .map(|r| StackRedlineSummary {
                pages: r.pages,
                inserts: r.inserts,
                deletes: r.deletes,
            })
            .into(),
        Err(e) => Err(e).into(),
    }
}

/// v3.24.0 "Stack Pro" — three-way PDF compare. Given a common ancestor
/// `base` plus two divergent revisions (`mine`, `theirs`), classifies every
/// base line as unchanged / mine-only / theirs-only / both-agree / conflict.
/// Returns a `ThreeWayDiff` the Svelte panel renders as a 3-column view
/// with a conflict ribbon — the canonical legal/dev-team feature Litera
/// Compare charges $400/seat/yr for. Acrobat doesn't ship it.
#[tauri::command]
fn slab_diff3_pdfs(
    base: PathBuf,
    mine: PathBuf,
    theirs: PathBuf,
) -> CmdResult<crate::pdf::diff3::ThreeWayDiff> {
    crate::pdf::diff3::three_way_diff(&base, &mine, &theirs).into()
}

/// v3.24.0 "Stack Pro" — materialise the merged text per page given a fresh
/// three-way diff (re-run on the backend to avoid shipping the whole DTO
/// back over IPC) plus the user's conflict-resolution choices. Returns the
/// per-page line vec the frontend can preview and the Task 5 PDF exporter
/// will turn into a PDF.
#[tauri::command]
fn slab_diff3_materialize(
    base: PathBuf,
    mine: PathBuf,
    theirs: PathBuf,
    resolutions: Vec<crate::pdf::diff3::ResolutionEntry>,
) -> CmdResult<crate::pdf::diff3::MergedText> {
    crate::pdf::diff3::three_way_diff(&base, &mine, &theirs)
        .map(|d| crate::pdf::diff3::materialize_merged_text(&d, &resolutions))
        .into()
}

/// v3.24.0 "Stack Pro" — bake the three-way diff into a shareable PDF.
///
/// The output PDF is a self-contained colour-coded three-column redline
/// (Base / Mine / Theirs) that any PDF viewer can open — no Slab required
/// on the recipient's machine. This is the Litera Compare killer feature
/// ($400/seat/yr) given away free + offline.
#[tauri::command]
fn slab_diff3_export_pdf(
    base: PathBuf,
    mine: PathBuf,
    theirs: PathBuf,
    output: PathBuf,
) -> CmdResult<crate::pdf::stack_diff3_export::Diff3ExportResult> {
    match crate::pdf::diff3::three_way_diff(&base, &mine, &theirs) {
        Ok(d) => crate::pdf::stack_diff3_export::export_diff3_pdf(&d, &output).into(),
        Err(e) => Err(e).into(),
    }
}

/// v2.4.0 "Stack" — visual (pixel-level) PDF diff. Renders both sides at
/// `dpi` via Poppler, masks per-pixel luma delta, and returns axis-aligned
/// change boxes alongside the existing line-level diff. Defaults are tuned
/// for legible-text PDFs: 150 DPI, luma threshold 20, min mass 8.
#[tauri::command]
fn slab_visual_diff_pdfs(
    old: PathBuf,
    new: PathBuf,
    dpi: Option<u32>,
    threshold: Option<u8>,
    min_mass: Option<u32>,
) -> CmdResult<crate::pdf::visual_diff::VisualDiff> {
    let dpi = dpi.unwrap_or(150).clamp(36, 300);
    let threshold = threshold.unwrap_or(20);
    let min_mass = min_mass.unwrap_or(8);
    crate::pdf::visual_diff::visual_diff_pdfs(&old, &new, dpi, threshold, min_mass).into()
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

// ---- v2.3.0 Theater — presenter mode commands ----

use std::sync::Arc;
use theater::TheaterManager;

/// Event name broadcast on every Theater state mutation. The audience
/// window (`/theater`) and the presenter control window
/// (`/theater-control`) both subscribe; main app windows can also
/// subscribe to mirror live presenter state into the sidebar panel.
///
/// Payload is the full `TheaterState` so subscribers can render without
/// a follow-up `slab_theater_snapshot` round-trip — cuts perceived
/// latency on slide flips from ~80ms (round-trip) to ~5ms (one emit).
const THEATER_STATE_EVENT: &str = "slab:theater-state";

/// Broadcast `state` to every Tauri window. Best-effort: a failed emit
/// is logged but never propagated, because a presenter pressing
/// PageDown shouldn't see "command failed" if a stale child window
/// happens to have disconnected.
fn emit_theater_state(app: &tauri::AppHandle, state: &theater::TheaterState) {
    use tauri::Emitter;
    if let Err(e) = app.emit(THEATER_STATE_EVENT, state) {
        eprintln!("[theater] emit {THEATER_STATE_EVENT} failed: {e}");
    }
}

/// Map a TheaterManager session result into a CmdResult for serialisation
/// to the frontend, and broadcast `slab:theater-state` on success so
/// every attached window picks up the new state without polling.
fn theater_result(
    app: &tauri::AppHandle,
    r: theater::session::SessionResult<theater::TheaterState>,
) -> CmdResult<theater::TheaterState> {
    match r {
        Ok(value) => {
            emit_theater_state(app, &value);
            CmdResult::Ok { value }
        }
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_theater_start(
    app: tauri::AppHandle,
    path: PathBuf,
    total_pages: u32,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> theater::TheaterState {
    let st = manager.start(path, total_pages);
    emit_theater_state(&app, &st);
    st
}

#[tauri::command]
fn slab_theater_end(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> Option<theater::TheaterState> {
    let res = manager.end();
    if let Some(s) = &res {
        emit_theater_state(&app, s);
    }
    res
}

#[tauri::command]
fn slab_theater_snapshot(
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> Option<theater::TheaterState> {
    manager.snapshot()
}

#[tauri::command]
fn slab_theater_next(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.next_page())
}

#[tauri::command]
fn slab_theater_prev(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.prev_page())
}

#[tauri::command]
fn slab_theater_jump(
    app: tauri::AppHandle,
    page: u32,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.jump(page))
}

#[tauri::command]
fn slab_theater_toggle_blackout(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.toggle_blackout())
}

#[tauri::command]
fn slab_theater_toggle_whiteout(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.toggle_whiteout())
}

#[tauri::command]
fn slab_theater_toggle_laser(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.toggle_laser())
}

#[tauri::command]
fn slab_theater_toggle_ink(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.toggle_ink())
}

#[tauri::command]
fn slab_theater_toggle_spotlight(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.toggle_spotlight())
}

#[tauri::command]
fn slab_theater_push_stroke(
    app: tauri::AppHandle,
    stroke: theater::InkStroke,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.push_stroke(stroke))
}

#[tauri::command]
fn slab_theater_undo_stroke(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.undo_stroke())
}

#[tauri::command]
fn slab_theater_clear_strokes(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<TheaterManager>>,
) -> CmdResult<theater::TheaterState> {
    theater_result(&app, manager.clear_strokes())
}

/// Open the audience (fullscreen) and presenter-control windows for an
/// active Theater session. Idempotent: if either window already exists
/// (e.g. operator closed only one), the missing window is re-spawned.
///
/// Returns the labels of the two windows so the frontend can target
/// them for follow-up actions (focus / close).
#[tauri::command]
fn slab_theater_open_windows(
    app: tauri::AppHandle,
    state: tauri::State<'_, windows::WindowRegistry>,
    target_doc: Option<String>,
) -> Result<TheaterWindowLabels, String> {
    let audience = windows::ensure_panel_window(&app, &state, "theater", target_doc.clone())?;
    let control = windows::ensure_panel_window(&app, &state, "theater_control", target_doc)?;
    Ok(TheaterWindowLabels { audience, control })
}

/// Close the audience and presenter-control windows. Safe to call when
/// they don't exist (no-op per missing window). Sidebar panel keeps the
/// session alive — only `slab_theater_end` ends the session itself.
#[tauri::command]
fn slab_theater_close_windows(
    app: tauri::AppHandle,
    state: tauri::State<'_, windows::WindowRegistry>,
) -> Result<u32, String> {
    let mut closed = 0u32;
    for s in state.list() {
        if (s.panel_id == "theater" || s.panel_id == "theater_control")
            && windows::close_label(&app, &state, &s.label).is_ok()
        {
            closed += 1;
        }
    }
    Ok(closed)
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TheaterWindowLabels {
    pub audience: String,
    pub control: String,
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

fn resolve_compact_opts(preset: &str, custom: Option<CompactOptions>) -> CompactOptions {
    match (preset, custom) {
        ("custom", Some(c)) => c,
        ("screen", _) => CompactOptions::from_preset(CompactPreset::Screen),
        ("ebook", _) => CompactOptions::from_preset(CompactPreset::Ebook),
        ("printer", _) => CompactOptions::from_preset(CompactPreset::Printer),
        ("prepress", _) => CompactOptions::from_preset(CompactPreset::Prepress),
        _ => CompactOptions::from_preset(CompactPreset::Ebook),
    }
}

#[tauri::command]
fn slab_compactor_estimate(
    input: PathBuf,
    preset: String,
    custom: Option<CompactOptions>,
) -> CmdResult<EstimateReport> {
    let opts = resolve_compact_opts(&preset, custom);
    do_compactor_estimate(&input, opts).into()
}

#[tauri::command]
fn slab_compactor_compact(
    input: PathBuf,
    output: PathBuf,
    preset: String,
    custom: Option<CompactOptions>,
) -> CmdResult<CompactReport> {
    let opts = resolve_compact_opts(&preset, custom);
    do_compact(&input, &output, opts).into()
}

#[tauri::command]
fn slab_streamline_inspect(
    input: PathBuf,
) -> CmdResult<crate::pdf::streamline::dto::LinearizeReport> {
    use crate::pdf::streamline::{dto::LinearizeReport, is_linearized};
    match is_linearized(&input) {
        Ok((status, stats)) => CmdResult::Ok {
            value: LinearizeReport {
                input_path: input.to_string_lossy().into_owned(),
                output_path: None,
                before: stats.clone(),
                after: None,
                status,
                warnings: Vec::new(),
            },
        },
        Err(e) => CmdResult::Err {
            message: format!("{e}"),
        },
    }
}

#[tauri::command]
fn slab_streamline_linearize(
    input: PathBuf,
    output: PathBuf,
) -> CmdResult<crate::pdf::streamline::dto::LinearizeReport> {
    crate::pdf::streamline::linearize_pdf(&input, &output).into()
}

#[tauri::command]
fn slab_streamline_audit(
    folder: PathBuf,
    recursive: bool,
    max_files: Option<usize>,
) -> CmdResult<crate::pdf::streamline::AuditReport> {
    crate::pdf::streamline::audit_folder(&folder, recursive, max_files).into()
}

#[tauri::command]
fn slab_reflow_to_docx(
    input: PathBuf,
    output: PathBuf,
) -> CmdResult<crate::pdf::reflow::ReflowReport> {
    use crate::pdf::reflow::{convert_to_docx, ReflowOptions};
    match convert_to_docx(&input, &output, &ReflowOptions::default()) {
        Ok(r) => CmdResult::Ok { value: r },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_markdown_to_md(
    input: PathBuf,
    output: PathBuf,
    detect_tables: Option<bool>,
    detect_lists: Option<bool>,
    flavour_gfm: Option<bool>,
) -> CmdResult<crate::pdf::markdown::MarkdownReport> {
    use crate::pdf::markdown::{convert_to_markdown, MarkdownFlavour, MarkdownOptions};
    let mut opts = MarkdownOptions::default();
    if let Some(v) = detect_tables {
        opts.detect_tables = v;
    }
    if let Some(v) = detect_lists {
        opts.detect_lists = v;
    }
    if let Some(false) = flavour_gfm {
        opts.flavour = MarkdownFlavour::CommonMark;
    }
    match convert_to_markdown(&input, &output, &opts) {
        Ok(r) => CmdResult::Ok { value: r },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_markdown_to_html(
    input: PathBuf,
    output: PathBuf,
    detect_tables: Option<bool>,
    detect_lists: Option<bool>,
    semantic_tags: Option<bool>,
    embed_css: Option<bool>,
) -> CmdResult<crate::pdf::markdown::HtmlReport> {
    use crate::pdf::markdown::{convert_to_html, HtmlOptions};
    let mut opts = HtmlOptions::default();
    if let Some(v) = detect_tables {
        opts.detect_tables = v;
    }
    if let Some(v) = detect_lists {
        opts.detect_lists = v;
    }
    if let Some(v) = semantic_tags {
        opts.semantic_tags = v;
    }
    if let Some(v) = embed_css {
        opts.embed_css = v;
    }
    match convert_to_html(&input, &output, &opts) {
        Ok(r) => CmdResult::Ok { value: r },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn slab_bind_to_epub(
    input: PathBuf,
    output: PathBuf,
    detect_tables: Option<bool>,
    detect_lists: Option<bool>,
    split_on_h1: Option<bool>,
    language: Option<String>,
    title: Option<String>,
    author: Option<String>,
) -> CmdResult<crate::pdf::epub::EpubReport> {
    use crate::pdf::epub::{convert_to_epub, EpubOptions};
    let defaults = EpubOptions::default();
    let opts = EpubOptions {
        detect_tables: detect_tables.unwrap_or(defaults.detect_tables),
        detect_lists: detect_lists.unwrap_or(defaults.detect_lists),
        split_on_h1: split_on_h1.unwrap_or(defaults.split_on_h1),
        language: language.unwrap_or(defaults.language),
        title,
        author,
        ..defaults
    };
    match convert_to_epub(&input, &output, &opts) {
        Ok(r) => CmdResult::Ok { value: r },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_tabulate_to_xlsx(
    input: PathBuf,
    output: PathBuf,
    type_numbers: Option<bool>,
    type_dates: Option<bool>,
    include_non_table_text: Option<bool>,
    sheet_name_pattern: Option<String>,
) -> CmdResult<crate::pdf::tabulate::TabulateReport> {
    use crate::pdf::tabulate::{convert_to_xlsx, TabulateOptions};
    let defaults = TabulateOptions::default();
    let opts = TabulateOptions {
        type_numbers: type_numbers.unwrap_or(defaults.type_numbers),
        type_dates: type_dates.unwrap_or(defaults.type_dates),
        include_non_table_text: include_non_table_text.unwrap_or(defaults.include_non_table_text),
        sheet_name_pattern: sheet_name_pattern.unwrap_or(defaults.sheet_name_pattern),
    };
    match convert_to_xlsx(&input, &output, &opts) {
        Ok(r) => CmdResult::Ok { value: r },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
fn slab_slide_to_pptx(
    input: PathBuf,
    output: PathBuf,
    include_speaker_notes: Option<bool>,
    detect_titles: Option<bool>,
) -> CmdResult<crate::pdf::slide::SlideReport> {
    use crate::pdf::slide::{convert_to_pptx, SlideOptions};
    let defaults = SlideOptions::default();
    let opts = SlideOptions {
        include_speaker_notes: include_speaker_notes.unwrap_or(defaults.include_speaker_notes),
        detect_titles: detect_titles.unwrap_or(defaults.detect_titles),
        embed_page_images: false,
    };
    match convert_to_pptx(&input, &output, &opts) {
        Ok(r) => CmdResult::Ok { value: r },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
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
fn slab_redact_true(
    input: PathBuf,
    output: PathBuf,
    opts: RedactOpts,
) -> CmdResult<TrueRedactReport> {
    do_redact_true(&input, &output, opts).into()
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
fn slab_pdfa_validate(
    input: PathBuf,
    level: Option<PdfAConformanceLevel>,
) -> CmdResult<PdfAValidationReport> {
    let level = level.unwrap_or_default();
    do_pdfa_validate(&input, level).into()
}

#[tauri::command]
fn slab_pdfa_font_audit(input: PathBuf) -> CmdResult<PdfAFontAuditReport> {
    let result: Result<PdfAFontAuditReport, PdfError> = lopdf::Document::load(&input)
        .map(|doc| do_pdfa_font_audit(&doc))
        .map_err(PdfError::from);
    result.into()
}

#[tauri::command]
fn slab_pdfa_convert(
    input: PathBuf,
    output: PathBuf,
    opts: PdfAConvertOpts,
) -> CmdResult<PdfAConvertReport> {
    do_pdfa_convert(&input, &output, opts).into()
}

#[tauri::command]
fn slab_pdfa_inspect(input: PathBuf) -> CmdResult<PdfAInspectionReport> {
    do_pdfa_inspect(&input).into()
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

/// CmdResult <- SmartFillError glue for the Tauri command surface.
impl<T: Serialize> From<Result<T, crate::pdf::forms_smart_fill::SmartFillError>> for CmdResult<T> {
    fn from(r: Result<T, crate::pdf::forms_smart_fill::SmartFillError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

/// Quill Smart Fill — propose-only.
///
/// Given a target AcroForm PDF and a source document (resume, prior-year
/// tax form, CSV row, contact-card markdown, …), runs the local AI
/// provider configured in the Beacon settings and returns a
/// [`SmartFillProposal`] for the UI to render line-by-line.
///
/// This command is **propose-only** — it never writes to the target PDF.
/// The accepted values are passed back through the existing
/// `slab_forms_fill` command in Slice 2 once the user clicks "Apply".
#[tauri::command]
async fn slab_quill_smart_fill_propose(
    target_pdf: PathBuf,
    source_doc: PathBuf,
) -> CmdResult<crate::pdf::forms_smart_fill::SmartFillProposal> {
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
    crate::pdf::forms_smart_fill::propose_smart_fill(&target_pdf, &source_doc, provider)
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

// ---------- Beacon Cache Inspector (v3.54.0 round-7) ----------

/// List every PDF currently in the embedding index, newest first, with
/// per-row chunk count joined in. Powers the inspector's main table.
#[tauri::command]
fn slab_beacon_index_list() -> CmdResult<Vec<IndexedPdfRecord>> {
    let index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    index.list_indexed().into()
}

/// Bulk-forget every PDF whose hash is in `pdf_hashes`. Returns the
/// count actually removed; unknown hashes silently skip (tolerant wire
/// contract for the inspector's multi-select).
#[tauri::command]
fn slab_beacon_index_forget_many(pdf_hashes: Vec<String>) -> CmdResult<usize> {
    let mut index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    index.forget_many(&pdf_hashes).into()
}

/// Per-embed-model bucket counts in one round-trip. The inspector
/// renders one tile per bucket and surfaces a "mixed model" warning
/// when len() > 1.
#[tauri::command]
fn slab_beacon_index_stats_by_model() -> CmdResult<Vec<ModelBucket>> {
    let index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    index.stats_by_model().into()
}

/// Every indexed PDF whose `path` no longer points at a readable file.
/// The inspector turns this into a "Forget all N stale" affordance.
#[tauri::command]
fn slab_beacon_index_find_stale() -> CmdResult<Vec<IndexedPdfRecord>> {
    let index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    index.find_stale().into()
}

/// Bulk-forget every stale (missing-on-disk) PDF in one transaction.
/// Returns the count actually removed.
#[tauri::command]
fn slab_beacon_index_forget_stale() -> CmdResult<usize> {
    let mut index = match open_default_index() {
        Ok(i) => i,
        Err(e) => {
            return CmdResult::Err {
                message: e.to_string(),
            }
        }
    };
    index.forget_stale().into()
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

/// Atlas (v2.2.0) — cross-document FTS5 search.
///
/// `query` is raw user input; we sanitise it into a safe MATCH
/// expression. `limit` is clamped to `1..=500`. `folder_id` optionally
/// restricts to one folder.
#[tauri::command]
fn slab_library_search(
    query: String,
    limit: Option<u32>,
    folder_id: Option<i64>,
) -> CmdResult<Vec<pdf::library::search::SearchHit>> {
    let result = (|| -> Result<Vec<pdf::library::search::SearchHit>, LibraryError> {
        let db = open_library_db()?;
        pdf::library::search::search(db.conn(), &query, limit.unwrap_or(50), folder_id)
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

/// The most recently *applied* tags, newest first (each listed once by its
/// newest application time). Powers the "Recently used" quick-chips when
/// tagging a document so the common tags are one click away. `limit` defaults
/// to 8 when omitted. v3.44.0 Atlas Recent-Tags.
#[tauri::command]
fn slab_library_recently_used_tags(limit: Option<u32>) -> CmdResult<Vec<TagRecord>> {
    let result = (|| -> Result<Vec<TagRecord>, LibraryError> {
        let db = open_library_db()?;
        db.recently_used_tags(limit.unwrap_or(8) as usize)
    })();
    result.into()
}

/// Document count per tag as `(tag_id, count)` pairs (every tag once; unused
/// tags report 0). Powers the muted usage count beside each tag in the rail and
/// the "sort by most used" ordering. One GROUP BY round-trip, not N queries.
/// v3.46.0 Atlas Tag-Usage-Counts.
#[tauri::command]
fn slab_library_tag_usage_counts() -> CmdResult<Vec<(i64, i64)>> {
    let result = (|| -> Result<Vec<(i64, i64)>, LibraryError> {
        let db = open_library_db()?;
        db.tag_usage_counts()
    })();
    result.into()
}

/// Delete every tag attached to zero documents, returning the number removed.
/// Reclaims the residue merges and bulk-removes leave behind (a tag whose last
/// document link was detached lingers in the rail at count 0). Tags carrying
/// even one document are untouched. Emits `library-changed` on a non-empty
/// cleanup so the rail self-heals. v3.47.0 Atlas Tag-Cleanup.
#[tauri::command]
fn slab_library_delete_unused_tags(app: tauri::AppHandle) -> CmdResult<usize> {
    let result = (|| -> Result<usize, LibraryError> {
        let mut db = open_library_db()?;
        db.delete_unused_tags()
    })();
    if matches!(&result, Ok(removed) if *removed > 0) {
        emit_library_changed(&app);
    }
    result.into()
}

// -----------------------------------------------------------------
// v3.50.0 "Atlas Saved Views" — one-click restorable rail filters.
// Distinct from personal_presets (which materialize into a smart
// collection) and from smart_collections (which own a doc list) —
// a saved view RESTORES the rail's LibraryFilter and re-runs it live.
// -----------------------------------------------------------------

#[tauri::command]
fn slab_library_saved_view_save(
    app: tauri::AppHandle,
    spec: pdf::library::saved_views::NewSavedView,
) -> CmdResult<pdf::library::saved_views::SavedViewRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::saved_views::save_view(&mut db, &spec)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_saved_view_list() -> CmdResult<Vec<pdf::library::saved_views::SavedViewRecord>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::saved_views::list_views(&db)
    })();
    result.into()
}

#[tauri::command]
fn slab_library_saved_view_delete(app: tauri::AppHandle, id: i64) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::saved_views::delete_view(&mut db, id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_saved_view_rename(
    app: tauri::AppHandle,
    id: i64,
    new_name: String,
) -> CmdResult<pdf::library::saved_views::SavedViewRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::saved_views::rename_view(&mut db, id, &new_name)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_saved_view_update_filter(
    app: tauri::AppHandle,
    id: i64,
    filter: pdf::library::query::LibraryFilter,
) -> CmdResult<pdf::library::saved_views::SavedViewRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::saved_views::update_view_filter(&mut db, id, &filter)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_saved_view_duplicate(
    app: tauri::AppHandle,
    id: i64,
) -> CmdResult<pdf::library::saved_views::SavedViewRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::saved_views::duplicate_view(&mut db, id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_saved_view_set_pinned(
    app: tauri::AppHandle,
    id: i64,
    pinned: bool,
) -> CmdResult<pdf::library::saved_views::SavedViewRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::saved_views::set_view_pinned(&mut db, id, pinned)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_library_saved_view_reorder(app: tauri::AppHandle, order: Vec<i64>) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::saved_views::reorder_views(&mut db, &order)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

// ---------------------------------------------------------------
// v3.32.0 "Atlas" — Collections + Smart Collections
// ---------------------------------------------------------------

#[tauri::command]
fn slab_collection_create(
    app: tauri::AppHandle,
    name: String,
    icon: Option<String>,
    color: Option<String>,
) -> CmdResult<pdf::library::collections::CollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::create_collection(
            &mut db,
            &name,
            icon.as_deref(),
            color.as_deref(),
        )
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_list() -> CmdResult<Vec<pdf::library::collections::CollectionRecord>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::collections::list_collections(&db)
    })();
    result.into()
}

#[tauri::command]
fn slab_collection_rename(
    app: tauri::AppHandle,
    id: i64,
    name: String,
) -> CmdResult<pdf::library::collections::CollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::rename_collection(&mut db, id, &name)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_delete(app: tauri::AppHandle, id: i64) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::delete_collection(&mut db, id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_set_color(
    app: tauri::AppHandle,
    id: i64,
    color: Option<String>,
) -> CmdResult<pdf::library::collections::CollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::set_collection_color(&mut db, id, color.as_deref())
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_reorder(app: tauri::AppHandle, ordered_ids: Vec<i64>) -> CmdResult<usize> {
    let result = (|| -> Result<usize, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::reorder_collections(&mut db, &ordered_ids)
    })();
    // Only emit if something actually moved; spamming library-changed for
    // a no-op reorder would refresh every subscriber for nothing.
    let did_move = matches!(&result, Ok(n) if *n > 0);
    if did_move {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_duplicate(
    app: tauri::AppHandle,
    source_id: i64,
) -> CmdResult<pdf::library::collections::CollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::duplicate_collection(&mut db, source_id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_add_docs(
    app: tauri::AppHandle,
    collection_id: i64,
    doc_ids: Vec<i64>,
) -> CmdResult<usize> {
    let result = (|| -> Result<usize, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::add_docs(&mut db, collection_id, &doc_ids)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_remove_docs(
    app: tauri::AppHandle,
    collection_id: i64,
    doc_ids: Vec<i64>,
) -> CmdResult<usize> {
    let result = (|| -> Result<usize, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::remove_docs(&mut db, collection_id, &doc_ids)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_collection_list_docs(collection_id: i64) -> CmdResult<Vec<DocumentRecord>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::collections::list_collection_docs(&db, collection_id)
    })();
    result.into()
}

#[tauri::command]
fn slab_smart_collection_create(
    app: tauri::AppHandle,
    spec: pdf::library::collections::NewSmartCollection,
) -> CmdResult<pdf::library::collections::SmartCollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::create_smart_collection(&mut db, &spec)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_smart_collection_list() -> CmdResult<Vec<pdf::library::collections::SmartCollectionRecord>>
{
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::collections::seed_defaults(&mut open_library_db()?)?;
        pdf::library::collections::list_smart_collections(&db)
    })();
    result.into()
}

#[tauri::command]
fn slab_smart_collection_delete(app: tauri::AppHandle, id: i64) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::delete_smart_collection(&mut db, id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_smart_collection_expand(id: i64) -> CmdResult<Vec<DocumentRecord>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::collections::expand_smart_collection(&db, id)
    })();
    result.into()
}

// -----------------------------------------------------------------
// v3.35.0 "Atlas Presets" — built-in smart-collection templates
// -----------------------------------------------------------------

/// List every built-in preset. Static data, no DB hit, but we still
/// return through CmdResult to keep the IPC shape consistent across
/// library commands.
#[tauri::command]
fn slab_preset_list() -> CmdResult<Vec<pdf::library::presets::PresetInfo>> {
    let result: Result<_, LibraryError> = Ok(pdf::library::presets::list_presets());
    result.into()
}

/// Materialize the preset with `preset_id` into a real smart
/// collection row. Auto-creates any tags the preset references.
/// Emits library-changed so sidebars refresh.
#[tauri::command]
fn slab_preset_apply(
    app: tauri::AppHandle,
    preset_id: String,
) -> CmdResult<pdf::library::collections::SmartCollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::presets::apply_preset(&mut db, &preset_id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Return the preset ids that are already materialized as smart
/// collections (matched by name) — frontend uses this to grey out
/// "Add" buttons in the picker.
#[tauri::command]
fn slab_preset_already_applied() -> CmdResult<Vec<String>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::presets::presets_already_applied(&db)
    })();
    result.into()
}

// -----------------------------------------------------------------
// v3.36.0 "Atlas Personal Presets" — user-saved recipes + .slabpresets
// pack import/export.
// -----------------------------------------------------------------

#[tauri::command]
fn slab_personal_preset_save(
    app: tauri::AppHandle,
    spec: pdf::library::personal_presets::NewPersonalPreset,
) -> CmdResult<pdf::library::personal_presets::PersonalPresetRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::personal_presets::save_personal_preset(&mut db, &spec)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_personal_preset_list(
) -> CmdResult<Vec<pdf::library::personal_presets::PersonalPresetRecord>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::personal_presets::list_personal_presets(&db)
    })();
    result.into()
}

#[tauri::command]
fn slab_personal_preset_delete(app: tauri::AppHandle, id: i64) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::personal_presets::delete_personal_preset(&mut db, id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

#[tauri::command]
fn slab_personal_preset_apply(
    app: tauri::AppHandle,
    id: i64,
) -> CmdResult<pdf::library::collections::SmartCollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::personal_presets::apply_personal_preset(&mut db, id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Rename a personal preset in place. Trims the new name; empty
/// rejected; collision with another preset rejected by the UNIQUE
/// constraint. Returns the renamed record. v3.40 Slice 76.
#[tauri::command]
fn slab_personal_preset_rename(
    app: tauri::AppHandle,
    id: i64,
    new_name: String,
) -> CmdResult<pdf::library::personal_presets::PersonalPresetRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::personal_presets::rename_personal_preset(&mut db, id, &new_name)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Duplicate a personal preset. The copy gets a fresh sort_order at
/// the bottom of the list and a derived unique name ("<src> (copy)"
/// or "<src> (copy N)"). The duplicate is INDEPENDENT — editing it
/// later doesn't affect the source. v3.40 Slice 76.
#[tauri::command]
fn slab_personal_preset_duplicate(
    app: tauri::AppHandle,
    id: i64,
) -> CmdResult<pdf::library::personal_presets::PersonalPresetRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::personal_presets::duplicate_personal_preset(&mut db, id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Export the given personal preset ids (empty = all) to a JSON string.
/// The frontend handles the file save dialog itself.
#[tauri::command]
fn slab_personal_presets_export(ids: Vec<i64>) -> CmdResult<String> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::personal_presets::export_pack(&db, &ids)
    })();
    result.into()
}

/// Import a `.slabpresets` JSON pack. Frontend reads the file and passes
/// the text; we return the report (counts + per-preset errors).
#[tauri::command]
fn slab_personal_presets_import(
    app: tauri::AppHandle,
    pack_json: String,
    rename_on_conflict: bool,
) -> CmdResult<pdf::library::personal_presets::ImportReport> {
    let policy = if rename_on_conflict {
        pdf::library::personal_presets::ImportConflictPolicy::Rename
    } else {
        pdf::library::personal_presets::ImportConflictPolicy::Skip
    };
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::personal_presets::import_pack_from_str(&mut db, &pack_json, policy)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Helper: distinguish "field omitted" (None) from "field explicitly null"
/// (Some(None)) when deserializing JSON. Apply with
/// `#[serde(default, deserialize_with = "deserialize_some_option")]` on
/// fields of type `Option<Option<T>>`.
fn deserialize_some_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

// -----------------------------------------------------------------
// v3.37.0 "Atlas Smart Folders Hub" — merged built-in + personal preset
// list with persisted order and pin flags.
// -----------------------------------------------------------------

/// Return every smart folder (built-in + personal) sorted by
/// pin-then-display-order. Backs the new SmartFoldersHubPanel.
#[tauri::command]
fn slab_smart_folders_list() -> CmdResult<Vec<pdf::library::smart_folders::SmartFolderEntry>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::smart_folders::list_smart_folders(&db)
    })();
    result.into()
}

/// Persist a new visible order. Caller passes the FULL list; each item's
/// `sort_order` is its zero-based position in the UI.
#[tauri::command]
fn slab_smart_folders_reorder(
    app: tauri::AppHandle,
    items: Vec<pdf::library::smart_folders::OrderItem>,
) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::smart_folders::set_order(&mut db, &items)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Toggle the pin flag on a single smart folder entry.
#[tauri::command]
fn slab_smart_folders_pin(
    app: tauri::AppHandle,
    kind: String,
    id: String,
    pinned: bool,
) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::smart_folders::set_pinned(&mut db, &kind, &id, pinned)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

// -----------------------------------------------------------------
// v3.38.0 "Atlas Suggest" — Beacon-style heuristic suggestions for
// personal Smart Folders, sourced from recent library searches.
// -----------------------------------------------------------------

/// Return up to 3 suggested Smart Folders based on recent search history.
/// Returns `[]` if the user hasn't done enough searches yet.
#[tauri::command]
fn slab_library_suggestions_list() -> CmdResult<Vec<pdf::library::folder_suggest::Suggestion>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::folder_suggest::suggest(&db)
    })();
    result.into()
}

/// Dismiss a suggestion by its cluster_hash so we don't re-suggest it.
#[tauri::command]
fn slab_library_suggestions_dismiss(app: tauri::AppHandle, cluster_hash: String) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let db = open_library_db()?;
        pdf::library::search_log::dismiss(&db, &cluster_hash)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Accept a suggestion: create a personal preset from it AND dismiss the
/// cluster so it doesn't keep appearing.
#[tauri::command]
fn slab_library_suggestions_accept(
    app: tauri::AppHandle,
    suggestion: pdf::library::folder_suggest::Suggestion,
) -> CmdResult<pdf::library::personal_presets::PersonalPresetRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        // Build a LibraryFilter that searches title for the dominant token.
        let filter = pdf::library::query::LibraryFilter {
            title_substring: Some(suggestion.query_template.clone()),
            ..Default::default()
        };
        let spec = pdf::library::personal_presets::NewPersonalPreset {
            name: suggestion.name.clone(),
            icon: Some(suggestion.icon.clone()),
            color: Some(suggestion.color.clone()),
            description: Some(suggestion.reason.clone()),
            filter,
        };
        let saved = pdf::library::personal_presets::save_personal_preset(&mut db, &spec)?;
        // Best-effort dismiss so the same cluster doesn't reappear.
        let _ = pdf::library::search_log::dismiss(&db, &suggestion.cluster_hash);
        Ok(saved)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Total rows currently in the search log. The UI uses this to decide
/// whether to show a "search more to unlock suggestions" empty-state
/// vs hide the section entirely.
#[tauri::command]
fn slab_library_search_log_count() -> CmdResult<i64> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::search_log::count(&db)
    })();
    result.into()
}

/// Most-recent N library search rows from the rolling log, newest first.
/// Powers the LibrarySearchPanel's "Recent searches" chip strip — one click
/// to re-run a prior query. `limit` clamps to `1..=50`; default 8.
/// v3.52.0 Atlas Recent-Searches.
#[tauri::command]
fn slab_library_recent_searches(
    limit: Option<u32>,
) -> CmdResult<Vec<pdf::library::search_log::QueryRow>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        let cap = limit.unwrap_or(8).clamp(1, 50) as usize;
        pdf::library::search_log::recent_queries(&db, cap)
    })();
    result.into()
}

/// Snapshot of the FTS5 index size: distinct indexed docs + total pages.
/// Powers the LibrarySearchPanel's status footer so the user can see at
/// a glance how much of their library is actually searchable. Two cheap
/// COUNTs; safe to call on every panel mount + after every scan.
/// v3.55.0 Atlas Index-Status.
#[tauri::command]
fn slab_library_index_stats() -> CmdResult<pdf::library::search::IndexStats> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::search::index_stats(db.conn())
    })();
    result.into()
}

/// Wipe every row from `library_search_log`. Returns the count removed so
/// the UI can decide whether to toast or stay quiet. Suggestion-cluster
/// dismissals live in a sibling table and are NOT touched.
/// v3.52.0 Atlas Recent-Searches.
#[tauri::command]
fn slab_library_clear_search_history(app: tauri::AppHandle) -> CmdResult<usize> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        let n = pdf::library::search_log::clear(&db)?;
        if n > 0 {
            emit_library_changed(&app);
        }
        Ok(n)
    })();
    result.into()
}

// ---------------------------------------------------------------------
// v3.39.0 "Atlas Tag-Suggest" — per-document heuristic tag suggestions.
// ---------------------------------------------------------------------

/// Suggest up to 5 tags for a single document, computed locally from its
/// title/filename, the existing tag vocabulary, co-occurrence stats, and a
/// built-in domain dictionary. Returns `[]` if nothing plausible.
#[tauri::command]
fn slab_library_tag_suggestions_for_doc(
    doc_id: i64,
) -> CmdResult<Vec<pdf::library::tag_suggest::TagSuggestion>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::suggest_tags_for_doc(&db, doc_id)
    })();
    result.into()
}

/// Suggest tags for every untagged document (bulk). Skips docs that yield
/// no suggestions. `limit` caps how many untagged docs are scanned.
#[tauri::command]
fn slab_library_tag_suggestions_bulk_for_untagged(
    limit: Option<usize>,
) -> CmdResult<Vec<pdf::library::tag_suggest::BulkTagSuggestion>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::suggest_for_untagged(&db, limit.unwrap_or(50))
    })();
    result.into()
}

/// Accept a suggested tag: find-or-create it (auto-colored) and attach it
/// to the document, unioned with its existing tags.
#[tauri::command]
fn slab_library_tag_suggestion_accept(
    app: tauri::AppHandle,
    doc_id: i64,
    tag_name: String,
) -> CmdResult<pdf::library::TagRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::tag_suggest::accept_tag_suggestion(&mut db, doc_id, &tag_name)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Dismiss a suggested tag for a document so it never resurfaces there.
#[tauri::command]
fn slab_library_tag_suggestion_dismiss(doc_id: i64, tag_name: String) -> CmdResult<()> {
    let result = (|| -> Result<(), LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::dismiss_tag_suggestion(&db, doc_id, &tag_name)
    })();
    result.into()
}

/// Clear all dismissed tag suggestions for a document (settings escape
/// hatch — "show me suggestions again").
#[tauri::command]
fn slab_library_tag_suggestion_undismiss_all(doc_id: i64) -> CmdResult<usize> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::undismiss_all_for_doc(&db, doc_id)
    })();
    result.into()
}

/// Bulk-accept N (doc_id, tag_name) pairs in one round-trip. Per-item
/// failure semantics — a malformed name in item 12 fails item 12 alone,
/// the rest still attach. Emits a single `library-changed` event after
/// the batch so the UI refreshes once instead of N times.
#[tauri::command]
fn slab_library_tag_suggestions_accept_bulk(
    app: tauri::AppHandle,
    items: Vec<pdf::library::tag_suggest::AcceptItem>,
) -> CmdResult<pdf::library::tag_suggest::BulkAcceptResult> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::tag_suggest::accept_tag_suggestions_bulk(&mut db, &items)
    })();
    if let Ok(r) = &result {
        if !r.attached.is_empty() {
            emit_library_changed(&app);
        }
    }
    result.into()
}

/// List every dismissed tag suggestion for a doc, newest first. Powers
/// the inspector's "Hidden suggestions" disclosure so the user can
/// review what they've explicitly hidden and undo a single dismissal
/// without wiping the rest.
#[tauri::command]
fn slab_library_tag_suggestions_list_dismissed(
    doc_id: i64,
) -> CmdResult<Vec<pdf::library::tag_suggest::DismissedSuggestion>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::list_dismissed_for_doc(&db, doc_id)
    })();
    result.into()
}

/// Clear ONE dismissal for a (doc, tag) pair. Returns `true` if a row
/// was deleted; `false` if no such dismissal existed. The next call to
/// `suggest_tags_for_doc` will re-include that tag in the candidate set.
#[tauri::command]
fn slab_library_tag_suggestion_undismiss_one(doc_id: i64, tag_name: String) -> CmdResult<bool> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::undismiss_one_for_doc(&db, doc_id, &tag_name)
    })();
    result.into()
}

/// Bulk suggester over any [`LibraryFilter`]. Generalises
/// `slab_library_tag_suggestions_bulk_for_untagged` so the review surface
/// can aim at e.g. "Starred + tagged `discovery-2026`" rather than only
/// the untagged shortcut. `limit` overrides any filter-embedded limit so
/// the scan stays bounded.
#[tauri::command]
fn slab_library_tag_suggestions_bulk_for_filter(
    filter: pdf::library::query::LibraryFilter,
    limit: Option<usize>,
) -> CmdResult<Vec<pdf::library::tag_suggest::BulkTagSuggestion>> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::suggest_for_filter(&db, &filter, limit.unwrap_or(50))
    })();
    result.into()
}

/// Compact stats for the tag-suggest review badge — `untagged_docs_with_
/// suggestions` powers the toolbar count; `dismissed_total` shows up in
/// the settings escape hatch. `sample_cap` defaults to 200 so the badge
/// renders e.g. "200+" upstream when the working set is huge.
#[tauri::command]
fn slab_library_tag_suggestion_stats(
    sample_cap: Option<usize>,
) -> CmdResult<pdf::library::tag_suggest::TagSuggestionStats> {
    let result = (|| -> Result<_, LibraryError> {
        let db = open_library_db()?;
        pdf::library::tag_suggest::suggestion_stats(&db, sample_cap.unwrap_or(200))
    })();
    result.into()
}

#[derive(serde::Deserialize, Default)]
pub struct SmartCollectionPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some_option")]
    pub icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some_option")]
    pub color: Option<Option<String>>,
    #[serde(default)]
    pub filter: Option<pdf::library::query::LibraryFilter>,
}

#[tauri::command]
fn slab_smart_collection_update(
    app: tauri::AppHandle,
    id: i64,
    patch: SmartCollectionPatch,
) -> CmdResult<pdf::library::collections::SmartCollectionRecord> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::collections::update_smart_collection(
            &mut db,
            id,
            patch.name.as_deref(),
            patch.icon.as_ref().map(|o| o.as_deref()),
            patch.color.as_ref().map(|o| o.as_deref()),
            patch.filter.as_ref(),
        )
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
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

/// Update an existing tag's color (or clear it with `null`). Returns the
/// updated tag row so the UI can swap it in without a full refetch. Rejects
/// colors that aren't `#hex` / `hsl()` / `rgb()` shapes. v3.42.0 Atlas
/// Tag-Color editing.
#[tauri::command]
fn slab_library_set_tag_color(
    app: tauri::AppHandle,
    tag_id: i64,
    color: Option<String>,
) -> CmdResult<TagRecord> {
    let result = (|| -> Result<TagRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.set_tag_color(tag_id, color.as_deref())
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Rename a tag everywhere it is used. Returns the updated tag row so the UI
/// can swap it in without a full refetch. Rejects an empty name or a name
/// already taken by a different tag. v3.43.0 Atlas Tag-Rename.
#[tauri::command]
fn slab_library_rename_tag(
    app: tauri::AppHandle,
    tag_id: i64,
    new_name: String,
) -> CmdResult<TagRecord> {
    let result = (|| -> Result<TagRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.rename_tag(tag_id, &new_name)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Set (or clear) the freeform description on a tag. Pass `null` — or any
/// string that trims to empty — to clear it back to NULL. Returns the updated
/// tag row so the UI can swap it in without a full refetch. The backend trims
/// the input and rejects oversized text (cap is `MAX_TAG_DESCRIPTION_LEN`
/// Unicode scalars). v3.51.0 Atlas Tag-Descriptions.
#[tauri::command]
fn slab_library_set_tag_description(
    app: tauri::AppHandle,
    tag_id: i64,
    description: Option<String>,
) -> CmdResult<TagRecord> {
    let result = (|| -> Result<TagRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.set_tag_description(tag_id, description.as_deref())
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Override a library document's displayed `title`. Pass `null` — or any string
/// that trims to empty — to clear it back to NULL so the LibraryPanel falls
/// back to the file's basename. Returns the refreshed [`DocumentRecord`] (with
/// tags eager-loaded) so the UI can splice the card without a full
/// list_documents round-trip. The backend trims input and rejects oversized
/// text (cap is `MAX_DOC_TITLE_LEN` Unicode scalars). v3.55.0 Atlas Doc-Inspector.
#[tauri::command]
fn slab_library_set_doc_title(
    app: tauri::AppHandle,
    doc_id: i64,
    title: Option<String>,
) -> CmdResult<DocumentRecord> {
    let result = (|| -> Result<DocumentRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.set_doc_title(doc_id, title.as_deref())
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Set (or clear) the freeform `notes` on a library document. Pass `null` — or
/// any string that trims to empty — to clear the column back to NULL so the
/// inspector falls back to the empty-state placeholder. Returns the refreshed
/// [`DocumentRecord`] (with tags eager-loaded) so the Doc-Inspector drawer can
/// repaint without an extra `listDocuments` round-trip. The backend trims input
/// and rejects oversized text (cap is `MAX_DOC_NOTES_LEN` Unicode scalars).
/// v3.55.0 Atlas Doc-Inspector.
#[tauri::command]
fn slab_library_set_doc_notes(
    app: tauri::AppHandle,
    doc_id: i64,
    notes: Option<String>,
) -> CmdResult<DocumentRecord> {
    let result = (|| -> Result<DocumentRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.set_doc_notes(doc_id, notes.as_deref())
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Toggle the `starred` flag on a library document. Idempotent. Returns the
/// refreshed [`DocumentRecord`] (with tags eager-loaded) so the UI can splice
/// the card without an extra `listDocuments` round-trip. v3.55.0 Atlas
/// Doc-Inspector.
#[tauri::command]
fn slab_library_set_doc_starred(
    app: tauri::AppHandle,
    doc_id: i64,
    starred: bool,
) -> CmdResult<DocumentRecord> {
    let result = (|| -> Result<DocumentRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.set_doc_starred(doc_id, starred)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Fold `source_id` into `target_id`: re-point every document link from the
/// source tag to the target, coalescing duplicates by the newer `applied_at`,
/// then delete the source tag. Returns the surviving target row. This is the
/// deliberate "make these the same tag" path that `rename_tag` rejects.
/// v3.45.0 Atlas Tag-Merge.
#[tauri::command]
fn slab_library_merge_tags(
    app: tauri::AppHandle,
    source_id: i64,
    target_id: i64,
) -> CmdResult<TagRecord> {
    let result = (|| -> Result<TagRecord, LibraryError> {
        let mut db = open_library_db()?;
        db.merge_tags(source_id, target_id)
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

/// Bulk apply a tag (by name, find-or-created) across many documents in one
/// atomic action. Returns the resolved tag plus affected/total counts so the
/// UI can report "Applied to N of M". v3.41.0 Atlas Bulk Tag-Apply.
#[tauri::command]
fn slab_library_bulk_apply_tag(
    app: tauri::AppHandle,
    tag_name: String,
    doc_ids: Vec<i64>,
) -> CmdResult<pdf::library::bulk_tag::BulkTagResult> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::bulk_tag::apply_tag_to_docs(&mut db, &tag_name, &doc_ids)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Bulk remove a tag (by id) from many documents in one atomic action. The
/// tag row itself is preserved — only the named doc links are detached.
/// v3.41.0 Atlas Bulk Tag-Apply.
#[tauri::command]
fn slab_library_bulk_remove_tag(
    app: tauri::AppHandle,
    tag_id: i64,
    doc_ids: Vec<i64>,
) -> CmdResult<pdf::library::bulk_tag::BulkTagResult> {
    let result = (|| -> Result<_, LibraryError> {
        let mut db = open_library_db()?;
        pdf::library::bulk_tag::remove_tag_from_docs(&mut db, tag_id, &doc_ids)
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

/// Per-`ocr_state` count snapshot for the OCR Queue Panel's dashboard.
/// Pure read; safe to poll. v3.52.0 Atlas OCR-Queue Slice 3.
#[tauri::command]
fn slab_library_ocr_queue_stats() -> CmdResult<OcrQueueStats> {
    let result = (|| -> Result<OcrQueueStats, LibraryError> {
        let db = open_library_db()?;
        do_ocr_queue_stats(&db)
    })();
    result.into()
}

/// Every `ocr_failed` document, newest first, with `ocr_error`
/// populated so the failure inbox can render a reason per row.
/// v3.52.0 Atlas OCR-Queue Slice 4.
#[tauri::command]
fn slab_library_ocr_queue_list_failed() -> CmdResult<Vec<DocumentRecord>> {
    let result = (|| -> Result<Vec<DocumentRecord>, LibraryError> {
        let db = open_library_db()?;
        do_ocr_queue_list_failed(&db)
    })();
    result.into()
}

/// Re-queue one document — flip `ocr_done` / `ocr_failed` / `ocr_pending`
/// back to `scanned` and clear `ocr_error` + `ocr_output_path` so the
/// next `run_one` picks it up fresh. Returns the updated row.
/// v3.52.0 Atlas OCR-Queue Slice 2.
#[tauri::command]
fn slab_library_ocr_queue_requeue(app: tauri::AppHandle, doc_id: i64) -> CmdResult<DocumentRecord> {
    let result = (|| -> Result<DocumentRecord, LibraryError> {
        let mut db = open_library_db()?;
        do_ocr_queue_requeue_doc(&mut db, doc_id)
    })();
    if result.is_ok() {
        emit_library_changed(&app);
    }
    result.into()
}

/// Re-queue every `ocr_failed` document in one shot. Returns the number
/// of rows that flipped. v3.52.0 Atlas OCR-Queue Slice 2 companion.
#[tauri::command]
fn slab_library_ocr_queue_requeue_all_failed(app: tauri::AppHandle) -> CmdResult<usize> {
    let result = (|| -> Result<usize, LibraryError> {
        let mut db = open_library_db()?;
        do_ocr_queue_requeue_all_failed(&mut db)
    })();
    if let Ok(n) = &result {
        if *n > 0 {
            emit_library_changed(&app);
        }
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
///
/// **Workshop Slice 6.7 wiring:** when the flag flips to `true` for a
/// plugin whose manifest declares a `[runtime]` section AND whose
/// script bytes verified at discover time, we spawn a long-lived
/// [`PluginActor`](plugins::runtime::actor::PluginActor) into the
/// process-wide [`PluginRuntimeRegistry`]. Flipping back to `false`
/// removes the entry, which drops the [`WorkerHandle`] and Shutdowns
/// the worker thread. Pure declarative plugins (no `[runtime]`) take
/// the legacy path — the flag flip is the entire effect.
///
/// Spawn failures are surfaced as errors but the enabled flag is still
/// persisted (the user expressed intent to enable; failing silently
/// would be worse than a stale flag). Diagnostics live in the
/// registry's emptiness check.
#[tauri::command]
fn slab_plugins_set_enabled(
    id: String,
    enabled: bool,
    reg: tauri::State<'_, plugins::PluginRegistry>,
    runtime_reg: tauri::State<'_, plugins::PluginRuntimeRegistry>,
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

    // Workshop Slice 6.7: actor lifecycle follows the enabled flag.
    if enabled {
        // Only spawn for plugins with a verified `[runtime]` section.
        // Everything else (declarative-only, parse-broken, runtime
        // hash mismatch) is a no-op here.
        let plugin = reg.get(&id);
        let to_spawn = plugin.as_ref().and_then(|p| {
            let manifest = p.manifest.as_ref()?;
            let runtime_manifest = manifest.runtime.as_ref()?;
            let bytes = p.script_bytes.as_ref()?;
            let source = String::from_utf8(bytes.clone()).ok()?;
            Some((runtime_manifest.capabilities.clone(), source))
        });
        if let Some((declared, source)) = to_spawn {
            // Pull current grants from the on-disk store; default
            // (deny-all) when the user hasn't decided yet. The consent
            // modal already persists a decision before flipping
            // enabled — see Slice 5c — so deny-all here means either
            // the user pressed Deny or the modal was bypassed.
            let granted = plugins::default_grants_path()
                .map(|p| plugins::read_grants(&p).get(&id))
                .unwrap_or_default();
            match plugins::runtime::actor::PluginActor::spawn(id.clone(), declared, granted, source)
            {
                Ok(handle) => {
                    runtime_reg.insert(id.clone(), plugins::LiveEntry::new(handle));
                }
                Err(e) => {
                    // Worker has already torn itself down; bubble the
                    // failure up so the UI can surface it.
                    return Err(format!("could not spawn plugin runtime: {e}"));
                }
            }
        }
    } else {
        // Disabling: drop the live handle if we had one. `remove`
        // returns the entry; dropping it triggers Shutdown + join on
        // the worker thread. Safe to call even when no entry exists.
        let _ = runtime_reg.remove(&id);
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

// ---------- Workshop (v2.0.0) — plugin grant Tauri commands ----------

/// Fetch the user's grant decision for `plugin_id`. Returns the
/// persisted [`PluginGrants`] when one exists, otherwise the empty
/// "deny-all" default — callers treat default + `has_decision == false`
/// as "show the consent prompt".
///
/// We bundle the explicit-decision flag alongside the grants so the
/// frontend can distinguish "user pressed Deny" from "first run, never
/// asked". Both serialise as the same default `PluginGrants`, so we
/// need a discriminator.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PluginGrantsResponse {
    /// Has the user ever made an explicit grant decision for this
    /// plugin? `false` ⇒ prompt the user.
    has_decision: bool,
    /// Current grants (default = deny-all when `has_decision` is false).
    grants: plugins::PluginGrants,
}

/// Read the grant store from `~/.slab/plugin-grants.toml` and return
/// the entry for `plugin_id`. Missing file or missing plugin both
/// surface as `has_decision = false` with default grants.
///
/// Cabinet wires this to the consent modal: on first plugin enable the
/// modal calls `plugin_grants_get` and only shows up when
/// `has_decision == false`. Re-running an already-decided plugin skips
/// the modal.
#[tauri::command]
fn plugin_grants_get(plugin_id: String) -> Result<PluginGrantsResponse, String> {
    let path = plugins::default_grants_path().ok_or_else(|| {
        "HOME env var not set; cannot locate ~/.slab/plugin-grants.toml".to_string()
    })?;
    let store = plugins::read_grants(&path);
    Ok(PluginGrantsResponse {
        has_decision: store.has_decision(&plugin_id),
        grants: store.get(&plugin_id),
    })
}

/// Persist a user grant decision for `plugin_id`. Overwrites any
/// previous decision. Returns `()` on success; serialise/IO errors
/// bubble up as a string so the Cabinet modal can surface them.
///
/// We re-read the on-disk store, mutate the one entry, then write the
/// whole file back. The store is small (one row per plugin) so the
/// extra read is cheap compared to keeping a long-lived in-memory copy
/// behind a Mutex.
#[tauri::command]
fn plugin_grants_set(plugin_id: String, grants: plugins::PluginGrants) -> Result<(), String> {
    let path = plugins::default_grants_path().ok_or_else(|| {
        "HOME env var not set; cannot locate ~/.slab/plugin-grants.toml".to_string()
    })?;
    let mut store = plugins::read_grants(&path);
    store.set(plugin_id, grants);
    plugins::write_grants(&path, &store)
        .map_err(|e| format!("could not persist plugin grants: {e}"))
}

/// Forget a plugin's grant decision. Used by the uninstall path and by
/// the "Reset permissions" button in the plugin detail panel. After
/// reset, the next enable triggers the consent modal again.
///
/// No-op when the plugin has no entry (still returns Ok).
#[tauri::command]
fn plugin_grants_reset(plugin_id: String) -> Result<(), String> {
    let path = plugins::default_grants_path().ok_or_else(|| {
        "HOME env var not set; cannot locate ~/.slab/plugin-grants.toml".to_string()
    })?;
    let mut store = plugins::read_grants(&path);
    store.remove(&plugin_id);
    plugins::write_grants(&path, &store)
        .map_err(|e| format!("could not persist plugin grants: {e}"))
}

// ---------- Workshop (v2.0.0 Slice 6.7) — document lifecycle broadcast ----------
//
// The viewer calls these two commands whenever a PDF enters or leaves
// the active tab. Each call fan-outs the corresponding `RuntimeCmd`
// to every live plugin actor managed by `PluginRuntimeRegistry`.
//
// Lifecycle dispatch is best-effort: send failures are swallowed by
// the registry (means the worker thread already exited), and bad
// paths never throw — `DocumentEvent::from_path` accepts any string.
// The frontend treats both calls as fire-and-forget.

/// Notify all live plugin actors that a PDF was loaded into the viewer.
///
/// `path` is the absolute filesystem path of the freshly loaded PDF.
/// We derive a display `name` from the file stem; both fields land in
/// the `slab.document.onOpen` callback payload plugins observe.
///
/// Returns immediately — actual JS callback dispatch happens on each
/// plugin's worker thread asynchronously.
#[tauri::command]
fn slab_plugins_document_opened(
    path: String,
    registry: tauri::State<'_, plugins::PluginRuntimeRegistry>,
) {
    let ev = plugins::runtime::actor::DocumentEvent::from_path(path);
    registry.broadcast(plugins::runtime::actor::RuntimeCmd::DocumentOpened(ev));
}

/// Notify all live plugin actors that a previously open PDF was
/// closed or replaced. Mirror of [`slab_plugins_document_opened`];
/// invokes `slab.document.onClose` callbacks on every live plugin.
///
/// The viewer should call this on tab close, on document replacement
/// inside a tab (the symmetric `_opened` then fires for the new doc),
/// and on application shutdown.
#[tauri::command]
fn slab_plugins_document_closed(
    path: String,
    registry: tauri::State<'_, plugins::PluginRuntimeRegistry>,
) {
    let ev = plugins::runtime::actor::DocumentEvent::from_path(path);
    registry.broadcast(plugins::runtime::actor::RuntimeCmd::DocumentClosed(ev));
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
    /// Network + cache both unavailable; binary's embedded seed index
    /// is being shown. UI surfaces a "connect to see more" banner.
    /// (v2.0.2 Workshop Marketplace.)
    is_embedded_seed: bool,
    /// The index itself when present (Fresh, Stale, or EmbeddedSeed).
    index: Option<marketplace::Index>,
    /// Last-error string when present (Stale.network_error,
    /// EmbeddedSeed.network_error, or Failed).
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
            is_embedded_seed: false,
            index: Some(index),
            error: None,
        },
        marketplace::FetchOutcome::Stale {
            index,
            network_error,
        } => MarketplaceFetchResult {
            is_fresh: false,
            is_stale: true,
            is_embedded_seed: false,
            index: Some(index),
            error: Some(network_error.to_string()),
        },
        marketplace::FetchOutcome::EmbeddedSeed {
            index,
            network_error,
        } => MarketplaceFetchResult {
            is_fresh: false,
            is_stale: false,
            is_embedded_seed: true,
            index: Some(index),
            error: Some(network_error.to_string()),
        },
        marketplace::FetchOutcome::Failed(e) => MarketplaceFetchResult {
            is_fresh: false,
            is_stale: false,
            is_embedded_seed: false,
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
///
/// Side effect (v3.39 Slice 54): every outcome (signature failure,
/// download / extract failure, success / update success) is recorded
/// as one row in [`marketplace::InstallLog`] at
/// `~/.slab/marketplace-history.sqlite`. Log open failures are
/// swallowed (warned to stderr) — losing audit history must never
/// block an install the user just clicked.
#[tauri::command]
async fn slab_marketplace_install(
    entry: marketplace::IndexEntry,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<marketplace::InstallReport, String> {
    // Capture prior version BEFORE the install (the install pipeline
    // doesn't read manifests; the registry does). This is the value
    // we'll log as `prior_version` on an update row.
    let prior_version = reg
        .get(&entry.id)
        .and_then(|p| p.manifest.map(|m| m.version));

    // 1) Signature check — never trust unsigned input.
    if let Err(e) = marketplace::verify_with_maintainer_key(&entry) {
        let msg = format!("signature check failed: {e}");
        record_install_failure(&entry.id, &entry.version, &msg);
        return Err(msg);
    }

    // 2) Resolve plugins root (HOME-rooted).
    let plugins_root = match plugins::default_plugins_root() {
        Some(p) => p,
        None => {
            let msg = "HOME env var not set; cannot locate ~/.slab/plugins".to_string();
            record_install_failure(&entry.id, &entry.version, &msg);
            return Err(msg);
        }
    };
    if let Err(e) = std::fs::create_dir_all(&plugins_root) {
        let msg = format!("could not create plugins dir: {e}");
        record_install_failure(&entry.id, &entry.version, &msg);
        return Err(msg);
    }

    // 3) Download + extract via the install pipeline.
    let client = marketplace::default_client();
    let report = match marketplace::install_from_entry(&client, &entry, &plugins_root).await {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            record_install_failure(&entry.id, &entry.version, &msg);
            return Err(msg);
        }
    };

    // 4) Refresh the registry so the new plugin appears in the UI.
    let enabled = plugins::default_state_path()
        .as_deref()
        .map(plugins::read_enabled_state)
        .unwrap_or_default();
    reg.discover(&plugins_root, &enabled);

    // 5) Append a success row. The install pipeline's
    // `replaced_existing` flag is the source of truth for whether we
    // overwrote a prior copy on disk; the registry-derived
    // `prior_version` is what the UI displays. Both can drift if the
    // user mutated `~/.slab/plugins/` out of band — we trust the
    // pipeline flag for the action discriminant.
    let logged_prior = if report.replaced_existing {
        prior_version.as_deref()
    } else {
        None
    };
    if let Err(e) = open_install_log_and(|log| {
        log.record_install(
            &report.id,
            &report.version,
            "marketplace",
            report.bytes_written,
            report.files_extracted,
            logged_prior,
        )
        .map(|_| ())
    }) {
        eprintln!("[slab] install log write failed: {e}");
    }

    Ok(report)
}

/// Uninstall a marketplace-installed plugin by id. Synchronous because
/// the install module's uninstall is pure-filesystem. After removal,
/// re-discover so the registry drops the entry. Returns `false` if the
/// plugin wasn't installed in the first place.
///
/// Side effect (v3.39 Slice 54): on successful removal we append one
/// `uninstall` row to [`marketplace::InstallLog`] carrying the version
/// that was just deleted (resolved from the registry BEFORE
/// removal). Log open failures are swallowed.
#[tauri::command]
fn slab_marketplace_uninstall(
    id: String,
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<bool, String> {
    // Capture prior version BEFORE removing — once the dir is gone
    // the registry can't tell us what version we just deleted.
    let prior_version = reg.get(&id).and_then(|p| p.manifest.map(|m| m.version));

    let plugins_root = plugins::default_plugins_root()
        .ok_or_else(|| "HOME env var not set; cannot locate ~/.slab/plugins".to_string())?;
    let removed = marketplace::uninstall_plugin(&plugins_root, &id).map_err(|e| e.to_string())?;
    if removed {
        let enabled = plugins::default_state_path()
            .as_deref()
            .map(plugins::read_enabled_state)
            .unwrap_or_default();
        reg.discover(&plugins_root, &enabled);

        // Log the uninstall. If the plugin had no readable manifest
        // (registry returned None for version), use the literal
        // "unknown" so the row is still queryable by id.
        let ver = prior_version.as_deref().unwrap_or("unknown");
        if let Err(e) = open_install_log_and(|log| log.record_uninstall(&id, ver).map(|_| ())) {
            eprintln!("[slab] install log write failed: {e}");
        }
    }
    Ok(removed)
}

/// Open the default install log, run `f` against it, return the
/// result. Centralises the open + path-resolve boilerplate so the
/// install / uninstall commands stay tight. Errors propagate as
/// [`marketplace::InstallLogError`] so the caller can decide whether
/// to surface or swallow.
fn open_install_log_and<F>(f: F) -> Result<(), marketplace::InstallLogError>
where
    F: FnOnce(&mut marketplace::InstallLog) -> Result<(), marketplace::InstallLogError>,
{
    let path = marketplace::default_log_path();
    let mut log = marketplace::InstallLog::open(&path)?;
    f(&mut log)
}

/// Record an install/update failure to the install log. Best-effort:
/// any failure to open or write the log is logged to stderr and
/// swallowed — we never want a logging failure to mask the install
/// failure being reported back to the user.
fn record_install_failure(plugin_id: &str, version: &str, error_msg: &str) {
    if let Err(e) = open_install_log_and(|log| {
        log.record_failure(plugin_id, version, error_msg)
            .map(|_| ())
    }) {
        eprintln!("[slab] install log failure write failed: {e}");
    }
}

// ─── Install log read surface (v3.39 Slice 55) ──────────────────────

/// Per-plugin install/uninstall/failure timeline, newest first,
/// capped at `limit`. Returns an empty Vec for unknown plugin ids.
/// Used by PluginDetailDrawer's Activity section.
#[tauri::command]
fn slab_marketplace_install_events(
    plugin_id: String,
    limit: Option<i64>,
) -> Result<Vec<marketplace::InstallEvent>, String> {
    let path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    log.list_events(&plugin_id, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

/// Corpus-wide recent install events, newest first, capped at `limit`.
/// Used by the PluginsPanel toolbar "Recent installs" drawer.
#[tauri::command]
fn slab_marketplace_install_history_recent(
    limit: Option<i64>,
) -> Result<Vec<marketplace::InstallEvent>, String> {
    let path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    log.list_recent(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

/// Per-plugin counts of each install action kind. Powers the slim
/// header pill on PluginDetailDrawer's Activity section
/// ("Installed 3 · 1 update · 0 failures") in one round-trip.
#[tauri::command]
fn slab_marketplace_plugin_install_stats(
    plugin_id: String,
) -> Result<marketplace::InstallStats, String> {
    let path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    log.install_stats(&plugin_id).map_err(|e| e.to_string())
}

/// Slim summary of the install log as a whole. Used by the Recent
/// installs drawer's header to render "N events across X days"
/// without paging the timeline.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallLogSummary {
    pub total_events: i64,
    pub distinct_plugins: i64,
    /// Unix seconds of the oldest row, or `None` if the log is
    /// empty.
    pub oldest_occurred_at: Option<i64>,
}

/// Summary of the install log — total event count, distinct plugin
/// count, oldest event timestamp. Cheap (three small queries) and
/// safe to call on every drawer open.
#[tauri::command]
fn slab_marketplace_install_log_summary() -> Result<InstallLogSummary, String> {
    let path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    Ok(InstallLogSummary {
        total_events: log.total_event_count().map_err(|e| e.to_string())?,
        distinct_plugins: log.distinct_plugin_count().map_err(|e| e.to_string())?,
        oldest_occurred_at: log.oldest_occurred_at().map_err(|e| e.to_string())?,
    })
}

/// Trim the install log to events newer than `retain_days` days
/// before now. Returns the number of rows pruned. Used by the
/// Recent installs drawer's "Clear older than 90d" affordance and
/// (later) by a background task on app start.
///
/// `retain_days` is clamped to a minimum of 1 so a caller can't
/// accidentally wipe the whole log via `prune(0)` — to clear it
/// entirely, the user uses the explicit "Clear all" action (not
/// shipped here; pruning is the safer default surface).
#[tauri::command]
fn slab_marketplace_install_log_prune(retain_days: i64) -> Result<usize, String> {
    let path = marketplace::default_log_path();
    let mut log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    let days = retain_days.max(1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cutoff = now - days * 86_400;
    log.prune_older_than(cutoff).map_err(|e| e.to_string())
}

// ─── Install log export surface (v3.39 Slice 61) ────────────────────

/// Write the install log to disk as RFC-4180 CSV, filtered by an
/// optional `[since_unix, until_unix]` window. The frontend gathers
/// the destination from a native save-as dialog and passes the
/// absolute path here so the Tauri layer owns the disk I/O (the
/// frontend's @tauri-apps/plugin-fs scope doesn't cover arbitrary
/// user-chosen paths). Same approach as
/// `slab_hopper_export_backfill_csv`.
///
/// `limit` caps the number of rows written (default = 100_000); the
/// install log is small in practice but a defensive cap protects
/// against a runaway log eating a user's disk on export.
///
/// Returns the byte count actually written so the UI toast can say
/// "Exported 42 events (3.1 KB)" without re-reading the file.
///
/// Idempotent — overwrites if the target file exists. The frontend's
/// save dialog handles overwrite confirmation, so we don't double-
/// confirm here.
#[tauri::command]
fn slab_marketplace_install_log_export_csv(
    path: String,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    limit: Option<i64>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let events = log
        .list_events_between(since_unix, until_unix, limit.unwrap_or(100_000))
        .map_err(|e| e.to_string())?;
    let csv = marketplace::install_log::install_log_to_csv(&events, true);
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Write the install log to disk as a pretty-printed JSON envelope,
/// filtered by an optional `[since_unix, until_unix]` window.
/// Mirrors [`slab_marketplace_install_log_export_csv`] but emits the
/// [`marketplace::install_log::InstallLogExportEnvelope`] shape (with
/// schema_version + generated_at_iso + window-bounds + events array).
///
/// `limit` caps the number of rows written (same default as CSV).
/// Returns the byte count actually written.
///
/// Idempotent — overwrites if the target file exists.
#[tauri::command]
fn slab_marketplace_install_log_export_json(
    path: String,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    limit: Option<i64>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let events = log
        .list_events_between(since_unix, until_unix, limit.unwrap_or(100_000))
        .map_err(|e| e.to_string())?;
    let envelope = marketplace::install_log::install_log_to_json(&events, since_unix, until_unix);
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─── Install log filtered reader (v3.39 Slice 74) ───────────────────

/// Wire payload returned by [`slab_marketplace_install_log_list_filtered`].
/// `total_returned` is the row count actually delivered (post-limit) so
/// the UI can render "Showing N of M (limit reached)" copy when the
/// query truncated; `limit_used` echoes the effective limit so the
/// caller doesn't have to remember which default ran when it left
/// `limit` unset.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallEventFilteredResult {
    pub events: Vec<marketplace::install_log::InstallEvent>,
    pub total_returned: i64,
    pub limit_used: i64,
}

/// Read the install log with the four-axis filter exposed by
/// [`marketplace::InstallLog::list_events_filtered`]: optional
/// time-window (`since_unix` / `until_unix`), optional `actions` set
/// (case-sensitive lowercase tokens: "install" / "update" /
/// "uninstall" / "failed"), and optional `plugin_id_substr`
/// (case-insensitive). Default limit is 500 — large enough for the
/// drawer's scroll surface, small enough that an absurdly long
/// filter doesn't accidentally drag a 50k-row table into IPC.
///
/// Unknown action tokens are silently dropped — the storage layer's
/// `InstallAction::parse` treats unknown strings as `Failed` but we
/// don't want a TS typo to widen the result set; the explicit drop
/// here makes "asked for nothing valid" yield "no filter" rather
/// than "secret extra filter for failures".
#[tauri::command]
fn slab_marketplace_install_log_list_filtered(
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    actions: Option<Vec<String>>,
    plugin_id_substr: Option<String>,
    limit: Option<i64>,
) -> Result<InstallEventFilteredResult, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;

    // Parse the action tokens. Drop anything that doesn't round-trip
    // exactly so a TS typo can't widen the result.
    let parsed_actions: Option<Vec<marketplace::install_log::InstallAction>> =
        actions.map(|toks| {
            toks.iter()
                .filter_map(|t| match t.as_str() {
                    "install" => Some(marketplace::install_log::InstallAction::Install),
                    "update" => Some(marketplace::install_log::InstallAction::Update),
                    "uninstall" => Some(marketplace::install_log::InstallAction::Uninstall),
                    "failed" => Some(marketplace::install_log::InstallAction::Failed),
                    _ => None,
                })
                .collect()
        });

    let limit = limit.unwrap_or(500);
    let events = log
        .list_events_filtered(
            since_unix,
            until_unix,
            parsed_actions.as_deref(),
            plugin_id_substr.as_deref(),
            limit,
        )
        .map_err(|e| e.to_string())?;
    Ok(InstallEventFilteredResult {
        total_returned: events.len() as i64,
        limit_used: limit,
        events,
    })
}

/// Return the most-recently-active distinct plugin_ids in the install
/// log, newest activity first. Powers the filter bar's plugin
/// autocomplete in the Recent installs drawer. Default `limit` = 25,
/// which covers the typical paralegal install footprint without
/// dragging a giant id list across IPC; the drawer caches the result
/// for the lifetime of the open session.
#[tauri::command]
fn slab_marketplace_install_log_recent_plugin_ids(
    limit: Option<i64>,
) -> Result<Vec<String>, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    log.recent_plugin_ids(limit.unwrap_or(25))
        .map_err(|e| e.to_string())
}

// ─── Per-plugin histogram aggregate (v3.40 Slice 87) ────────────────

/// Wire payload returned by [`slab_marketplace_install_log_plugin_histogram`].
/// Carries the rows plus the effective window + limit used, so the UI
/// can render "Showing top 25 plugins in the last 30d" without remembering
/// which defaults ran when the caller left the args unset.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginHistogramResult {
    pub rows: Vec<marketplace::PluginHistogramRow>,
    pub since_unix: Option<i64>,
    pub until_unix: Option<i64>,
    pub limit_used: i64,
    /// Sum of every row's `total` — the grand total of events within
    /// the window across the returned plugins. Lets the UI render
    /// "12 events across 3 plugins" without re-summing client-side.
    pub grand_total: i64,
}

/// Aggregate the install log by plugin_id within an optional time
/// window. Returns the per-plugin counts (installs / updates /
/// uninstalls / failures / total / last_occurred_at) ordered by
/// total activity descending, capped at `limit` (default 25).
///
/// Powers the Recent installs drawer's "Top plugins" panel — the
/// answer to "which plugins did I install the most this month?".
/// Cheap (one indexed GROUP BY scan + a small in-memory sort).
#[tauri::command]
fn slab_marketplace_install_log_plugin_histogram(
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    limit: Option<i64>,
) -> Result<PluginHistogramResult, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(25);
    let rows = log
        .plugin_histogram(since_unix, until_unix, limit)
        .map_err(|e| e.to_string())?;
    let grand_total = rows.iter().map(|r| r.total).sum();
    Ok(PluginHistogramResult {
        rows,
        since_unix,
        until_unix,
        limit_used: limit,
        grand_total,
    })
}

// ─── Top plugins histogram export (v3.40 round-21 Slice 100) ─────────

/// Write the top-plugins histogram to disk as RFC-4180 CSV, filtered
/// by an optional `[since_unix, until_unix]` window. Mirrors
/// [`slab_marketplace_install_log_export_csv`] but emits the
/// per-plugin aggregate shape (one row per distinct plugin) rather
/// than the raw event log.
///
/// `path` is an absolute filesystem path the frontend obtains from
/// `@tauri-apps/plugin-dialog` save() so the write bypasses the
/// default plugin-fs scope (which doesn't cover arbitrary
/// user-chosen paths). Same approach as
/// `slab_marketplace_install_log_export_csv`.
///
/// `limit` caps the number of rows written (default = server's
/// histogram default of 25). The histogram itself is small in
/// practice (a typical paralegal install footprint fits well under
/// 100 plugins), but the cap also clamps the aggregate query at
/// the storage boundary so a runaway log doesn't drag a giant grid
/// through IPC just to be filtered down at write time.
///
/// Returns the byte count actually written so the UI toast can say
/// "Exported 12 plugins (1.8 KB)" without re-reading the file.
///
/// Idempotent — overwrites if the target file exists. The frontend's
/// save dialog handles overwrite confirmation, so we don't double-
/// confirm here.
#[tauri::command]
fn slab_marketplace_install_log_export_histogram_csv(
    path: String,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    limit: Option<i64>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let rows = log
        .plugin_histogram(since_unix, until_unix, limit.unwrap_or(25))
        .map_err(|e| e.to_string())?;
    let csv = marketplace::install_log::plugin_histogram_to_csv(&rows, true);
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Write the top-plugins histogram to disk as a pretty-printed JSON
/// envelope, filtered by an optional `[since_unix, until_unix]`
/// window. Mirrors [`slab_marketplace_install_log_export_histogram_csv`]
/// but emits the [`marketplace::install_log::PluginHistogramExportEnvelope`]
/// shape (schema_version + generated_at_iso + window-bounds +
/// row_count + grand_total + rows array).
///
/// `limit` caps the number of rows written (same default as the
/// CSV command — 25, matching the server's histogram default).
/// Returns the byte count actually written.
///
/// Idempotent — overwrites if the target file exists.
#[tauri::command]
fn slab_marketplace_install_log_export_histogram_json(
    path: String,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    limit: Option<i64>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let rows = log
        .plugin_histogram(since_unix, until_unix, limit.unwrap_or(25))
        .map_err(|e| e.to_string())?;
    let grand_total: i64 = rows.iter().map(|r| r.total).sum();
    let envelope = marketplace::install_log::plugin_histogram_to_json(
        &rows,
        since_unix,
        until_unix,
        grand_total,
    );
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─── Activity timeline aggregate + export surface (Slice 106) ────────

/// Wire payload returned by [`slab_marketplace_install_log_activity_timeline`].
/// Carries the buckets plus the effective window + granularity + the
/// grand_total so the UI can render "Showing 14 daily buckets, 87
/// events total" without re-summing client-side.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivityTimelineResult {
    pub buckets: Vec<marketplace::ActivityBucket>,
    pub since_unix: Option<i64>,
    pub until_unix: Option<i64>,
    pub granularity: marketplace::TimeBucketGranularity,
    /// Sum of every bucket's `total` — the corpus-wide event count
    /// within the window. Lets the UI render "87 events across 14
    /// buckets" without re-summing client-side.
    pub grand_total: i64,
}

/// Aggregate the install log by calendar bucket (day / week / month)
/// within an optional time window. Returns one
/// [`marketplace::ActivityBucket`] per non-empty bucket, ordered
/// ASCENDING by `bucket_start_unix` so the UI renders the timeline
/// left-to-right.
///
/// Powers the Recent installs drawer's "Activity over time" panel
/// — the answer to "WHEN was install activity happening?" (a
/// complementary axis to the slice-87 "Top plugins" histogram which
/// answers "WHICH plugins were active?").
///
/// `granularity` defaults to [`marketplace::TimeBucketGranularity::Day`]
/// when omitted — the most common UI default for the typical "last
/// 30 days" view. Cheap (one indexed scan + a small in-memory bucket
/// + sort).
#[tauri::command]
fn slab_marketplace_install_log_activity_timeline(
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    granularity: Option<marketplace::TimeBucketGranularity>,
) -> Result<ActivityTimelineResult, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let granularity = granularity.unwrap_or(marketplace::TimeBucketGranularity::Day);
    let buckets = log
        .activity_timeline(since_unix, until_unix, granularity)
        .map_err(|e| e.to_string())?;
    let grand_total: i64 = buckets.iter().map(|b| b.total).sum();
    Ok(ActivityTimelineResult {
        buckets,
        since_unix,
        until_unix,
        granularity,
        grand_total,
    })
}

/// Write the activity timeline to disk as RFC-4180 CSV, filtered by
/// an optional `[since_unix, until_unix]` window at the requested
/// granularity. Mirrors the histogram CSV export shape (slice 100)
/// but emits the per-bucket activity grid instead of the per-plugin
/// aggregate.
///
/// `path` is an absolute filesystem path the frontend obtains from
/// `@tauri-apps/plugin-dialog` save() so the write bypasses the
/// default plugin-fs scope.
///
/// `granularity` defaults to Day when omitted — same default as the
/// read endpoint above so "export the timeline I'm looking at" is
/// the natural reading without remembering a custom export
/// granularity.
///
/// Returns the byte count actually written. Idempotent — overwrites
/// if the target file exists. Creates parent dirs if missing.
#[tauri::command]
fn slab_marketplace_install_log_export_activity_timeline_csv(
    path: String,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    granularity: Option<marketplace::TimeBucketGranularity>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let granularity = granularity.unwrap_or(marketplace::TimeBucketGranularity::Day);
    let buckets = log
        .activity_timeline(since_unix, until_unix, granularity)
        .map_err(|e| e.to_string())?;
    let csv = marketplace::install_log::activity_timeline_to_csv(&buckets, granularity, true);
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Write the activity timeline to disk as a pretty-printed JSON
/// envelope, filtered by an optional `[since_unix, until_unix]`
/// window at the requested granularity. Mirrors the histogram JSON
/// export (slice 100) but emits the
/// [`marketplace::install_log::ActivityTimelineExportEnvelope`] shape
/// (schema_version + generated_at_iso + granularity + window-bounds +
/// bucket_count + grand_total + buckets array).
///
/// Same defaults as the CSV variant; returns the byte count actually
/// written; idempotent — overwrites if the target file exists.
#[tauri::command]
fn slab_marketplace_install_log_export_activity_timeline_json(
    path: String,
    since_unix: Option<i64>,
    until_unix: Option<i64>,
    granularity: Option<marketplace::TimeBucketGranularity>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let granularity = granularity.unwrap_or(marketplace::TimeBucketGranularity::Day);
    let buckets = log
        .activity_timeline(since_unix, until_unix, granularity)
        .map_err(|e| e.to_string())?;
    let grand_total: i64 = buckets.iter().map(|b| b.total).sum();
    let envelope = marketplace::install_log::activity_timeline_to_json(
        &buckets,
        granularity,
        since_unix,
        until_unix,
        grand_total,
    );
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─── Activity bucket drilldown surface (Slice 112) ───────────────────

/// Wire payload returned by [`slab_marketplace_install_log_bucket_drilldown`].
/// Carries the per-plugin rows plus the bucket coordinates + the
/// `grand_total` so the UI can render "12 plugins, 87 events total
/// in this day" without re-summing client-side.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BucketDrilldownResult {
    pub rows: Vec<marketplace::PluginHistogramRow>,
    pub bucket_start_unix: i64,
    pub granularity: marketplace::TimeBucketGranularity,
    /// Sum of every row's `total` — the corpus-wide event count
    /// within the bucket. Lets the UI render "87 events across 12
    /// plugins" without re-summing client-side.
    pub grand_total: i64,
}

/// Per-plugin breakdown of install-log activity within a single
/// activity-timeline bucket. Composes
/// [`marketplace::InstallLog::bucket_drilldown`] (which composes
/// `bucket_window_unix` with `plugin_histogram`) with the wire-layer
/// log-open boilerplate.
///
/// `bucket_start_unix` must come from an `activity_timeline` bucket
/// (i.e. already calendar-floored to the granularity's boundary).
/// `granularity` defaults to Day when omitted — same default as the
/// activity-timeline read endpoint so "drill into the timeline I'm
/// looking at" is the natural reading.
///
/// `limit` defaults to 25 — same default as the histogram + drilldown
/// already use across the install-log read surface; large enough that
/// the typical bucket fits, small enough that a pathologically-busy
/// day doesn't drag a giant grid into IPC.
#[tauri::command]
fn slab_marketplace_install_log_bucket_drilldown(
    bucket_start_unix: i64,
    granularity: Option<marketplace::TimeBucketGranularity>,
    limit: Option<i64>,
) -> Result<BucketDrilldownResult, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let granularity = granularity.unwrap_or(marketplace::TimeBucketGranularity::Day);
    let limit = limit.unwrap_or(25);
    let rows = log
        .bucket_drilldown(bucket_start_unix, granularity, limit)
        .map_err(|e| e.to_string())?;
    let grand_total: i64 = rows.iter().map(|r| r.total).sum();
    Ok(BucketDrilldownResult {
        rows,
        bucket_start_unix,
        granularity,
        grand_total,
    })
}

/// Write the bucket drilldown to disk as RFC-4180 CSV. Mirrors the
/// activity-timeline CSV export (slice 106) but scopes to a SINGLE
/// bucket and ships the per-plugin breakdown.
///
/// Same defaults as the read endpoint above (granularity defaults to
/// Day, limit to 25). Returns the byte count actually written;
/// idempotent — overwrites if the target file exists; creates parent
/// dirs if missing.
#[tauri::command]
fn slab_marketplace_install_log_export_bucket_drilldown_csv(
    path: String,
    bucket_start_unix: i64,
    granularity: Option<marketplace::TimeBucketGranularity>,
    limit: Option<i64>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let granularity = granularity.unwrap_or(marketplace::TimeBucketGranularity::Day);
    let limit = limit.unwrap_or(25);
    let rows = log
        .bucket_drilldown(bucket_start_unix, granularity, limit)
        .map_err(|e| e.to_string())?;
    let csv = marketplace::install_log::bucket_drilldown_to_csv(
        &rows,
        bucket_start_unix,
        granularity,
        true,
    );
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Write the bucket drilldown to disk as a pretty-printed JSON
/// envelope. Mirrors the activity-timeline JSON export (slice 106)
/// but scopes to a SINGLE bucket and ships the
/// [`marketplace::install_log::BucketDrilldownExportEnvelope`] shape.
///
/// Same defaults as the CSV variant; returns the byte count actually
/// written; idempotent.
#[tauri::command]
fn slab_marketplace_install_log_export_bucket_drilldown_json(
    path: String,
    bucket_start_unix: i64,
    granularity: Option<marketplace::TimeBucketGranularity>,
    limit: Option<i64>,
) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let granularity = granularity.unwrap_or(marketplace::TimeBucketGranularity::Day);
    let limit = limit.unwrap_or(25);
    let rows = log
        .bucket_drilldown(bucket_start_unix, granularity, limit)
        .map_err(|e| e.to_string())?;
    let grand_total: i64 = rows.iter().map(|r| r.total).sum();
    let envelope = marketplace::install_log::bucket_drilldown_to_json(
        &rows,
        bucket_start_unix,
        granularity,
        grand_total,
    );
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Wire payload returned by [`slab_marketplace_install_log_retention_policy`].
/// Carries the user-visible retention window plus the floor + interval
/// constants so the UI doesn't have to hard-code them on the TS side.
/// The `last_auto_prune_at` slot reads `None` until the first auto-prune
/// runs; the UI uses it to render a "Last auto-prune: <relative>" line
/// in the Retention section.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallLogRetentionPolicy {
    pub retain_days: i64,
    pub last_auto_prune_at: Option<i64>,
    pub default_retain_days: i64,
    pub min_retain_days: i64,
    pub auto_prune_interval_secs: i64,
}

/// Read the current install-log retention policy. Cheap (two key-value
/// queries) and idempotent. The UI calls this on mount of the Retention
/// section in RecentInstallsDrawer; the startup wiring uses
/// [`slab_marketplace_install_log_auto_prune`] which has its own
/// effective-policy logic.
#[tauri::command]
fn slab_marketplace_install_log_retention_policy() -> Result<InstallLogRetentionPolicy, String> {
    let path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    Ok(InstallLogRetentionPolicy {
        retain_days: log.retain_days().map_err(|e| e.to_string())?,
        last_auto_prune_at: log.last_auto_prune_at().map_err(|e| e.to_string())?,
        default_retain_days: marketplace::DEFAULT_RETAIN_DAYS,
        min_retain_days: marketplace::MIN_RETAIN_DAYS,
        auto_prune_interval_secs: marketplace::AUTO_PRUNE_INTERVAL_SECS,
    })
}

/// Persist a new retention window in days. The storage layer clamps
/// `days` to [`marketplace::MIN_RETAIN_DAYS`]; the command returns the
/// stored value so the UI can surface the corrected value if the user
/// typed something the floor rejects (e.g. 0 → clamps to 1).
///
/// Does NOT immediately run a prune — that's the user's separate
/// action via [`slab_marketplace_install_log_auto_prune`] (or the
/// existing `slab_marketplace_install_log_prune` for one-shot windows).
/// Splitting the two responsibilities keeps "change the policy"
/// reversible without altering the log.
#[tauri::command]
fn slab_marketplace_install_log_set_retention_days(days: i64) -> Result<i64, String> {
    let path = marketplace::default_log_path();
    let mut log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    log.set_retain_days(days).map_err(|e| e.to_string())
}

/// Run the retention policy if the debounce window has elapsed.
/// Returns the [`marketplace::AutoPruneOutcome`] (snake_case tagged
/// "pruned" with rows_removed/retain_days/cutoff_unix, or "skipped"
/// with next_due_unix). The startup wiring (Slice 66) calls this on
/// app boot; the UI's "Run auto-prune now" affordance calls it on
/// user click — both paths share the debounce so a recently-pruned
/// log skips both the boot pass AND the manual click.
///
/// `force = Some(true)` bypasses the debounce — used by the UI's
/// "Run now" button when the user explicitly wants to overwrite the
/// debounce stamp. `None` or `Some(false)` preserves the debounce.
#[tauri::command]
fn slab_marketplace_install_log_auto_prune(
    force: Option<bool>,
) -> Result<marketplace::AutoPruneOutcome, String> {
    let path = marketplace::default_log_path();
    let mut log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    if force.unwrap_or(false) {
        // Force path: clear the debounce stamp so the next call
        // executes the prune regardless of when the last one ran.
        // The stamp gets re-written by the prune itself, so the next
        // *unforced* call still honours the 24h window from this run.
        log.set_last_auto_prune_at(0).map_err(|e| e.to_string())?;
    }
    log.auto_prune_if_due_now().map_err(|e| e.to_string())
}

// ─────────────────────────────────────────────────────────────────────
// Per-plugin retention overrides surface (v3.39 round-24 — slice 117).
// ─────────────────────────────────────────────────────────────────────

/// Wire payload returned by
/// [`slab_marketplace_install_log_plugin_retention_overrides`].
/// Carries every persisted override row plus the global window in
/// force plus the floor constant so the UI can render "Override 7d
/// (default 365d, min 1d)" without round-tripping for the constants.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginRetentionOverridesResult {
    pub rows: Vec<marketplace::PluginRetentionOverride>,
    /// Global window in force at read time — same value
    /// [`slab_marketplace_install_log_retention_policy`] returns;
    /// duplicated here so the Retention section UI doesn't have to
    /// pair the two reads.
    pub default_retain_days: i64,
    /// Floor constant — same value as
    /// [`marketplace::MIN_RETAIN_DAYS`]; the UI uses it to render
    /// the input field's `min` attribute.
    pub min_retain_days: i64,
}

/// Read every persisted per-plugin retention override. Cheap O(N)
/// on the overrides table; in practice N is small (a handful of
/// audit-critical or diagnostic plugins per workstation). The UI's
/// Retention section calls this on mount + after every write.
#[tauri::command]
fn slab_marketplace_install_log_plugin_retention_overrides(
) -> Result<PluginRetentionOverridesResult, String> {
    let path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    let rows = log
        .plugin_retention_overrides()
        .map_err(|e| e.to_string())?;
    let default_retain_days = log.retain_days().map_err(|e| e.to_string())?;
    Ok(PluginRetentionOverridesResult {
        rows,
        default_retain_days,
        min_retain_days: marketplace::MIN_RETAIN_DAYS,
    })
}

/// Persist a per-plugin retention override. Returns the value
/// actually stored after the storage layer's
/// [`marketplace::MIN_RETAIN_DAYS`] clamp so the UI can surface the
/// corrected value if the user typed something the floor rejects
/// (e.g. 0 → clamps to 1). Validates `plugin_id` is non-empty
/// before touching the log — defensive bookkeeping on the wire
/// boundary.
///
/// Does NOT immediately run a prune — that's a separate user
/// action via [`slab_marketplace_install_log_auto_prune`]. Splitting
/// the two responsibilities keeps "change the policy" reversible
/// without altering the log.
#[tauri::command]
fn slab_marketplace_install_log_set_plugin_retention(
    plugin_id: String,
    days: i64,
) -> Result<i64, String> {
    if plugin_id.trim().is_empty() {
        return Err("plugin_id must not be empty".into());
    }
    let path = marketplace::default_log_path();
    let mut log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    log.set_plugin_retention_days(plugin_id.trim(), days)
        .map_err(|e| e.to_string())
}

/// Clear a per-plugin retention override. Returns `true` if a row
/// was actually removed, `false` if no override existed (idempotent).
/// The plugin falls back to the global window on its next
/// auto-prune evaluation.
#[tauri::command]
fn slab_marketplace_install_log_clear_plugin_retention(plugin_id: String) -> Result<bool, String> {
    if plugin_id.trim().is_empty() {
        return Err("plugin_id must not be empty".into());
    }
    let path = marketplace::default_log_path();
    let mut log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    log.clear_plugin_retention(plugin_id.trim())
        .map_err(|e| e.to_string())
}

/// Write the per-plugin retention overrides list to disk as RFC-4180
/// CSV. Mirrors the four sibling export commands (install-log,
/// histogram, activity-timeline, bucket-drilldown). Returns the byte
/// count actually written; idempotent — overwrites if the target file
/// exists; creates parent dirs if missing.
#[tauri::command]
fn slab_marketplace_install_log_export_plugin_retention_csv(path: String) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let rows = log
        .plugin_retention_overrides()
        .map_err(|e| e.to_string())?;
    let default_retain_days = log.retain_days().map_err(|e| e.to_string())?;
    let csv = marketplace::install_log::plugin_retention_overrides_to_csv(
        &rows,
        default_retain_days,
        true,
    );
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Write the per-plugin retention overrides list to disk as a
/// pretty-printed JSON envelope. Ships the
/// [`marketplace::install_log::PluginRetentionExportEnvelope`] shape
/// (schema_version + generated_at_iso + default_retain_days +
/// min_retain_days + row_count + rows). Returns the byte count
/// actually written; idempotent.
#[tauri::command]
fn slab_marketplace_install_log_export_plugin_retention_json(path: String) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let rows = log
        .plugin_retention_overrides()
        .map_err(|e| e.to_string())?;
    let default_retain_days = log.retain_days().map_err(|e| e.to_string())?;
    let envelope = marketplace::install_log::plugin_retention_overrides_to_json(
        &rows,
        default_retain_days,
        marketplace::MIN_RETAIN_DAYS,
    );
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─────────────────────────────────────────────────────────────────────
// Auto-prune run history surface (v3.39 round-25 — slice 122).
// ─────────────────────────────────────────────────────────────────────

/// Wire payload returned by
/// [`slab_marketplace_install_log_auto_prune_runs`]. Carries the
/// most-recent N history rows plus the total count plus the
/// envelope-level attribution totals so the UI can render
/// "Showing N of M total prunes · Cleared X events (Y from
/// per-plugin overrides)" without re-querying or re-summing.
///
/// `total_count` is the underlying row count in the
/// `install_log_auto_prune_runs` table; `rows` is capped at the
/// `limit` the command was called with. When `total_count >
/// rows.len()` the UI surfaces "Showing N of M" copy; otherwise
/// "Showing all N".
///
/// `total_rows_removed` + `total_overrides_rows_removed` are summed
/// across the RETURNED rows (NOT the underlying table) — the
/// attribution split for what the user is currently looking at. A
/// future "Show all" affordance would re-fetch with limit = -1 …
/// in practice 25 is enough for the standard view; the
/// envelope-level totals on the JSON export answer the corpus-wide
/// version of the question.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AutoPruneRunsResult {
    pub rows: Vec<marketplace::AutoPruneRun>,
    /// Total number of rows in the underlying history table —
    /// always `>= rows.len()`. The UI compares against `rows.len()`
    /// to render "Showing N of M" vs "Showing all".
    pub total_count: i64,
    /// Sum of `rows[i].rows_removed` pre-computed across the
    /// returned rows. Saves a UI re-sum on every render.
    pub total_rows_removed: i64,
    /// Sum of `rows[i].overrides_rows_removed` pre-computed across
    /// the returned rows. Pairs with `total_rows_removed` for the
    /// attribution split surfaced in the section header.
    pub total_overrides_rows_removed: i64,
}

/// Read the auto-prune run history, newest first, capped at `limit`
/// (default 25). The UI calls this on mount of the Retention
/// section and after every prune (so the new entry appears
/// immediately on the next render).
///
/// Cheap O(N) on a small indexed table; the default cap keeps the
/// table view bounded for a workstation that has been running for
/// years. A future "Show all" affordance can call with limit = -1
/// to surface a synthetic "no cap" view (the storage layer treats
/// negative limits as zero so the caller chooses; today the
/// command floors negatives at 25 to keep the UI's expectations
/// stable).
#[tauri::command]
fn slab_marketplace_install_log_auto_prune_runs(
    limit: Option<i64>,
) -> Result<AutoPruneRunsResult, String> {
    let path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    let effective_limit = limit.unwrap_or(25).max(1);
    let rows = log
        .auto_prune_runs(effective_limit)
        .map_err(|e| e.to_string())?;
    let total_count = log.auto_prune_runs_total().map_err(|e| e.to_string())?;
    let total_rows_removed: i64 = rows.iter().map(|r| r.rows_removed).sum();
    let total_overrides_rows_removed: i64 = rows.iter().map(|r| r.overrides_rows_removed).sum();
    Ok(AutoPruneRunsResult {
        rows,
        total_count,
        total_rows_removed,
        total_overrides_rows_removed,
    })
}

/// Remove every row from the auto-prune run history. Idempotent —
/// returns the number of rows actually deleted (zero on an
/// already-empty table). The corresponding `install_events` table
/// is NOT touched; this command clears the audit trail only, not
/// the events themselves. The UI's "Clear history" affordance
/// behind a confirm dialog calls this.
#[tauri::command]
fn slab_marketplace_install_log_clear_auto_prune_runs() -> Result<u64, String> {
    let path = marketplace::default_log_path();
    let mut log = marketplace::InstallLog::open(&path).map_err(|e| e.to_string())?;
    log.clear_auto_prune_runs()
        .map(|n| n as u64)
        .map_err(|e| e.to_string())
}

/// Write the auto-prune run history to disk as RFC-4180 CSV.
/// Mirrors the five sibling export commands (install-log,
/// histogram, activity-timeline, bucket-drilldown, plugin-
/// retention). Returns the byte count actually written; idempotent
/// — overwrites if the target file exists; creates parent dirs if
/// missing.
///
/// Reads ALL rows (no limit) — the UI's table view is capped at
/// 25 for readability but an export should be comprehensive. The
/// `auto_prune_runs(i64::MAX)` call costs one indexed scan on a
/// table that grows by one row per day at most.
#[tauri::command]
fn slab_marketplace_install_log_export_auto_prune_runs_csv(path: String) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let rows = log.auto_prune_runs(i64::MAX).map_err(|e| e.to_string())?;
    let csv = marketplace::install_log::auto_prune_runs_to_csv(&rows, true);
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Write the auto-prune run history to disk as a pretty-printed
/// JSON envelope. Ships the
/// [`marketplace::install_log::AutoPruneRunsExportEnvelope`] shape
/// (schema_version + generated_at_iso + row_count +
/// total_rows_removed + total_overrides_rows_removed + rows).
/// Returns the byte count actually written; idempotent.
///
/// Like the CSV companion, exports ALL rows — no limit applied at
/// the export boundary.
#[tauri::command]
fn slab_marketplace_install_log_export_auto_prune_runs_json(path: String) -> Result<u64, String> {
    let log_path = marketplace::default_log_path();
    let log = marketplace::InstallLog::open(&log_path).map_err(|e| e.to_string())?;
    let rows = log.auto_prune_runs(i64::MAX).map_err(|e| e.to_string())?;
    let envelope = marketplace::install_log::auto_prune_runs_to_json(&rows);
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─────────────────────────────────────────────────────────────────────
// Bulk updates (v3.39 round-15 — slices 69-70).
// ─────────────────────────────────────────────────────────────────────

/// Tauri event channel for per-step bulk-update progress. The frontend
/// subscribes to this while a `slab_marketplace_update_all` call is
/// in flight to render a per-plugin progress overlay.
const MARKETPLACE_UPDATE_PROGRESS_EVENT: &str = "marketplace://update-progress";

/// One step of progress emitted on the `marketplace://update-progress`
/// channel as the batch updater works through its target list. The
/// frontend renders this as "Updating 2/5 · Acme PDF Tools…".
///
/// Three phases: `starting` (about to call install pipeline),
/// `done` (install pipeline returned Ok, log row appended), `error`
/// (install pipeline returned Err — the batch continues onto the next
/// id, the failed id surfaces on the final report).
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateProgress {
    /// Monotonic batch id (typically `Date.now()`) tying this stream
    /// of events to the matching `slab_marketplace_update_all` call.
    /// Lets multiple in-flight batches stay distinct in the event bus
    /// even though we don't currently fire more than one at a time.
    pub batch_id: i64,
    /// 1-indexed position of the current target in the batch list.
    pub index: usize,
    /// Total number of targets in the batch.
    pub total: usize,
    /// Plugin id of the target this event refers to.
    pub plugin_id: String,
    /// `starting` | `done` | `error`.
    pub phase: String,
    /// Human-readable error message, only populated when `phase == "error"`.
    pub error: Option<String>,
}

/// Outcome of one target in a `slab_marketplace_update_all` batch.
/// `kind` is `succeeded` (full install pipeline returned Ok and the
/// log row landed) or `failed` (signature / download / sha256 / extract
/// returned Err; the failure row also landed in the install log).
///
/// The full prior + new version strings are carried for both kinds so
/// the UI can render "Updated Acme PDF Tools 1.4.0 → 1.5.0" or
/// "Failed to update Acme PDF Tools 1.4.0 → 1.5.0 (signature check
/// failed)" without re-resolving from the registry.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateOutcome {
    Succeeded {
        plugin_id: String,
        prior_version: String,
        new_version: String,
        bytes_written: u64,
    },
    Failed {
        plugin_id: String,
        prior_version: String,
        new_version: String,
        error: String,
    },
}

impl UpdateOutcome {
    /// Returns the plugin id regardless of outcome kind — used by the
    /// frontend reducer to key its state map.
    pub fn plugin_id(&self) -> &str {
        match self {
            Self::Succeeded { plugin_id, .. } | Self::Failed { plugin_id, .. } => plugin_id,
        }
    }

    /// True iff the outcome is `Succeeded`. Convenience for the
    /// summary calculation.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }
}

/// Final report from a `slab_marketplace_update_all` call. The wire
/// shape is self-describing: succeeded + failed are derived counts so
/// callers don't need to fold the outcomes list themselves.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchUpdateReport {
    /// Echo of the batch_id the caller passed in (so a frontend that
    /// fires multiple batches in close succession can correlate the
    /// returned report with its in-flight tracking state).
    pub batch_id: i64,
    /// Per-target outcomes in the SAME order the caller passed in.
    pub outcomes: Vec<UpdateOutcome>,
    /// Count of `Succeeded` outcomes.
    pub succeeded: usize,
    /// Count of `Failed` outcomes.
    pub failed: usize,
    /// Sum of `bytes_written` across successful outcomes.
    pub bytes_written: u64,
}

impl BatchUpdateReport {
    fn from_outcomes(batch_id: i64, outcomes: Vec<UpdateOutcome>) -> Self {
        let succeeded = outcomes.iter().filter(|o| o.is_success()).count();
        let failed = outcomes.len() - succeeded;
        let bytes_written = outcomes
            .iter()
            .filter_map(|o| match o {
                UpdateOutcome::Succeeded { bytes_written, .. } => Some(*bytes_written),
                _ => None,
            })
            .sum();
        Self {
            batch_id,
            outcomes,
            succeeded,
            failed,
            bytes_written,
        }
    }
}

/// `slab_marketplace_list_update_targets` — compute the deterministic
/// set of installed plugins that have a newer version available in
/// the freshly-fetched marketplace index. The frontend's "Updates
/// available" banner renders directly from this plan.
///
/// Re-fetches the index every call so the user gets an up-to-date
/// answer (the cache + offline-seed fallback in `fetch_index_with_cache`
/// keeps the call cheap when the network is happy or absent). The
/// returned plan is sorted by id ascending and carries the full
/// IndexEntry per target so the banner can render without a second
/// lookup, and the bulk-update call can feed `entry` straight into
/// the install pipeline.
///
/// Errors surface as `Err(String)` only when the index can't be
/// loaded at all (no network + no cache + no embedded seed) — every
/// other code path returns `Ok(UpdatePlan)` with possibly an empty
/// targets list.
#[tauri::command]
async fn slab_marketplace_list_update_targets(
    reg: tauri::State<'_, plugins::PluginRegistry>,
) -> Result<marketplace::UpdatePlan, String> {
    // 1) Resolve installed plugins (slim subset of registry).
    let installed: Vec<marketplace::InstalledPlugin> = reg
        .list()
        .into_iter()
        .filter_map(|p| {
            let version = p.manifest.as_ref().map(|m| m.version.clone())?;
            Some(marketplace::InstalledPlugin {
                id: p.id.clone(),
                version,
            })
        })
        .collect();

    // 2) Fetch the index. Use the same cache-aware path as the
    //    Browse-tab refresh so we don't re-burn the network — the
    //    user's banner reflects what the cache says when offline.
    let client = marketplace::default_client();
    let cache_path = marketplace::default_cache_path();
    let outcome = marketplace::fetch_index_with_cache(
        &client,
        marketplace::DEFAULT_INDEX_URL,
        cache_path.as_deref(),
    )
    .await;
    let index = match outcome.index() {
        Some(idx) => idx.clone(),
        None => {
            // Nothing we can show. Surface the underlying error so
            // the banner UI can pin the cause to the user.
            return Err(format!(
                "marketplace index unavailable: {}",
                match outcome {
                    marketplace::FetchOutcome::Failed(e) => e.to_string(),
                    _ => "no index, no cache, no embedded seed".to_string(),
                }
            ));
        }
    };

    Ok(marketplace::plan_updates(&installed, &index.plugins))
}

/// `slab_marketplace_update_all` — bulk-update one or more plugins
/// sequentially. The caller passes the list of ids (typically derived
/// from a prior `slab_marketplace_list_update_targets` plan) plus a
/// monotonic `batch_id` for correlating the progress event stream.
///
/// For EACH id in order:
///   1. Re-resolve the matching IndexEntry from a fresh index call
///      (defensive — the index might have moved between the planning
///      call and this call). If the id is no longer in the index, the
///      target lands as `Failed` with `error: "no longer in
///      marketplace index"`.
///   2. Emit `phase: "starting"` on the progress channel.
///   3. Run the same verify→install pipeline `slab_marketplace_install`
///      uses (signature, plugins-root, install_from_entry,
///      reg.discover, install_log).
///   4. Emit `phase: "done"` or `phase: "error"` accordingly.
///
/// The batch ALWAYS runs to completion — a failed update on id N
/// does NOT stop ids N+1 onward. This matches every other batch
/// updater UX (browser extensions, system package managers): the user
/// expects "as many as can succeed will succeed".
///
/// Returns the structured [`BatchUpdateReport`] with per-id outcomes,
/// counts of succeeded + failed, and total bytes written.
#[tauri::command]
async fn slab_marketplace_update_all(
    app: tauri::AppHandle,
    reg: tauri::State<'_, plugins::PluginRegistry>,
    batch_id: i64,
    plugin_ids: Vec<String>,
) -> Result<BatchUpdateReport, String> {
    use tauri::Emitter;

    // Fetch the index ONCE up front; bulk update inside the loop
    // resolves entries from this snapshot. If the index moves
    // mid-batch we don't pick it up — that's the deliberate trade
    // (avoid network races inside the loop).
    let client = marketplace::default_client();
    let cache_path = marketplace::default_cache_path();
    let index_outcome = marketplace::fetch_index_with_cache(
        &client,
        marketplace::DEFAULT_INDEX_URL,
        cache_path.as_deref(),
    )
    .await;
    let index = match index_outcome.index() {
        Some(idx) => idx.clone(),
        None => {
            return Err(format!(
                "marketplace index unavailable: {}",
                match index_outcome {
                    marketplace::FetchOutcome::Failed(e) => e.to_string(),
                    _ => "no index, no cache, no embedded seed".to_string(),
                }
            ))
        }
    };

    let plugins_root = plugins::default_plugins_root()
        .ok_or_else(|| "HOME env var not set; cannot locate ~/.slab/plugins".to_string())?;
    if let Err(e) = std::fs::create_dir_all(&plugins_root) {
        return Err(format!("could not create plugins dir: {e}"));
    }

    let total = plugin_ids.len();
    let mut outcomes: Vec<UpdateOutcome> = Vec::with_capacity(total);

    for (i, plugin_id) in plugin_ids.iter().enumerate() {
        let index_pos = i + 1;

        // Capture prior version BEFORE the install (registry-derived).
        let prior_version = reg
            .get(plugin_id)
            .and_then(|p| p.manifest.map(|m| m.version))
            .unwrap_or_default();

        // Resolve the matching index entry from the snapshot.
        let Some(entry) = index.plugins.iter().find(|e| &e.id == plugin_id) else {
            // Index moved or the caller passed a stale id. Surface as
            // failure without writing an install_log row — there's no
            // versioned identity to log against here.
            let _ = app.emit(
                MARKETPLACE_UPDATE_PROGRESS_EVENT,
                UpdateProgress {
                    batch_id,
                    index: index_pos,
                    total,
                    plugin_id: plugin_id.clone(),
                    phase: "error".into(),
                    error: Some("no longer in marketplace index".into()),
                },
            );
            outcomes.push(UpdateOutcome::Failed {
                plugin_id: plugin_id.clone(),
                prior_version: prior_version.clone(),
                new_version: String::new(),
                error: "no longer in marketplace index".into(),
            });
            continue;
        };

        let new_version = entry.version.clone();

        // Starting event.
        let _ = app.emit(
            MARKETPLACE_UPDATE_PROGRESS_EVENT,
            UpdateProgress {
                batch_id,
                index: index_pos,
                total,
                plugin_id: plugin_id.clone(),
                phase: "starting".into(),
                error: None,
            },
        );

        // Signature check.
        if let Err(e) = marketplace::verify_with_maintainer_key(entry) {
            let msg = format!("signature check failed: {e}");
            record_install_failure(&entry.id, &entry.version, &msg);
            let _ = app.emit(
                MARKETPLACE_UPDATE_PROGRESS_EVENT,
                UpdateProgress {
                    batch_id,
                    index: index_pos,
                    total,
                    plugin_id: plugin_id.clone(),
                    phase: "error".into(),
                    error: Some(msg.clone()),
                },
            );
            outcomes.push(UpdateOutcome::Failed {
                plugin_id: plugin_id.clone(),
                prior_version,
                new_version,
                error: msg,
            });
            continue;
        }

        // Install pipeline.
        let report = match marketplace::install_from_entry(&client, entry, &plugins_root).await {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                record_install_failure(&entry.id, &entry.version, &msg);
                let _ = app.emit(
                    MARKETPLACE_UPDATE_PROGRESS_EVENT,
                    UpdateProgress {
                        batch_id,
                        index: index_pos,
                        total,
                        plugin_id: plugin_id.clone(),
                        phase: "error".into(),
                        error: Some(msg.clone()),
                    },
                );
                outcomes.push(UpdateOutcome::Failed {
                    plugin_id: plugin_id.clone(),
                    prior_version,
                    new_version,
                    error: msg,
                });
                continue;
            }
        };

        // Refresh registry so the UI sees the bumped version.
        let enabled = plugins::default_state_path()
            .as_deref()
            .map(plugins::read_enabled_state)
            .unwrap_or_default();
        reg.discover(&plugins_root, &enabled);

        // Append install_log row (best-effort, same policy as the
        // single-install command — losing audit never blocks an
        // install the user just clicked).
        let logged_prior = if report.replaced_existing && !prior_version.is_empty() {
            Some(prior_version.as_str())
        } else {
            None
        };
        if let Err(e) = open_install_log_and(|log| {
            log.record_install(
                &report.id,
                &report.version,
                "marketplace",
                report.bytes_written,
                report.files_extracted,
                logged_prior,
            )
            .map(|_| ())
        }) {
            eprintln!("[slab] install log write failed during update: {e}");
        }

        let _ = app.emit(
            MARKETPLACE_UPDATE_PROGRESS_EVENT,
            UpdateProgress {
                batch_id,
                index: index_pos,
                total,
                plugin_id: plugin_id.clone(),
                phase: "done".into(),
                error: None,
            },
        );

        outcomes.push(UpdateOutcome::Succeeded {
            plugin_id: report.id.clone(),
            prior_version,
            new_version: report.version.clone(),
            bytes_written: report.bytes_written,
        });
    }

    Ok(BatchUpdateReport::from_outcomes(batch_id, outcomes))
}

#[cfg(test)]
mod marketplace_bulk_update_tests {
    use super::*;

    fn ok(id: &str, prior: &str, new: &str, bytes: u64) -> UpdateOutcome {
        UpdateOutcome::Succeeded {
            plugin_id: id.into(),
            prior_version: prior.into(),
            new_version: new.into(),
            bytes_written: bytes,
        }
    }

    fn fail(id: &str, prior: &str, new: &str, err: &str) -> UpdateOutcome {
        UpdateOutcome::Failed {
            plugin_id: id.into(),
            prior_version: prior.into(),
            new_version: new.into(),
            error: err.into(),
        }
    }

    #[test]
    fn outcome_plugin_id_accessor_works_for_both_kinds() {
        assert_eq!(ok("a", "1", "2", 0).plugin_id(), "a");
        assert_eq!(fail("b", "1", "2", "boom").plugin_id(), "b");
    }

    #[test]
    fn outcome_is_success_distinguishes_kinds() {
        assert!(ok("a", "1", "2", 0).is_success());
        assert!(!fail("b", "1", "2", "boom").is_success());
    }

    #[test]
    fn batch_report_counts_succeed_and_fail_correctly() {
        let outcomes = vec![
            ok("a", "1.0.0", "1.0.1", 100),
            fail("b", "2.0.0", "2.0.1", "sig"),
            ok("c", "3.0.0", "3.1.0", 250),
        ];
        let report = BatchUpdateReport::from_outcomes(42, outcomes);
        assert_eq!(report.batch_id, 42);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 1);
        assert_eq!(report.bytes_written, 350);
        assert_eq!(report.outcomes.len(), 3);
    }

    #[test]
    fn batch_report_handles_empty_outcomes() {
        let report = BatchUpdateReport::from_outcomes(1, vec![]);
        assert_eq!(report.succeeded, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.bytes_written, 0);
        assert!(report.outcomes.is_empty());
    }

    #[test]
    fn batch_report_serializes_with_snake_case_tags() {
        let report = BatchUpdateReport::from_outcomes(
            7,
            vec![
                ok("a", "1.0.0", "1.0.1", 100),
                fail("b", "1.0.0", "1.0.1", "download"),
            ],
        );
        let json = serde_json::to_string(&report).expect("serialize");
        // Snake_case tag is mandatory — the TS reducer narrows on this.
        assert!(json.contains("\"kind\":\"succeeded\""), "got {json}");
        assert!(json.contains("\"kind\":\"failed\""), "got {json}");
        assert!(json.contains("\"batch_id\":7"));
        assert!(json.contains("\"succeeded\":1"));
        assert!(json.contains("\"failed\":1"));
        assert!(json.contains("\"bytes_written\":100"));
    }

    #[test]
    fn update_progress_serializes_with_snake_case_fields() {
        let p = UpdateProgress {
            batch_id: 9,
            index: 2,
            total: 5,
            plugin_id: "com.example.hi".into(),
            phase: "done".into(),
            error: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"batch_id\":9"));
        assert!(json.contains("\"plugin_id\":\"com.example.hi\""));
        assert!(json.contains("\"phase\":\"done\""));
        // error: None skipped under default serde (no skip_if), so the
        // key is present as null — the TS reducer treats null + missing
        // identically.
        assert!(json.contains("\"error\":null"));
    }

    #[test]
    fn update_progress_serializes_error_phase_with_message() {
        let p = UpdateProgress {
            batch_id: 1,
            index: 1,
            total: 1,
            plugin_id: "a".into(),
            phase: "error".into(),
            error: Some("network down".into()),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"error\":\"network down\""));
    }
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

// ─────────────────────────────────────────────────────────────────────
// Beacon Voice Mode: Listen (v1.9.1 STT) — Tauri command surface.
// ─────────────────────────────────────────────────────────────────────

/// Probe what STT engines + recorders are available on this host. The
/// frontend uses this to (a) populate the engine selector and (b) show
/// an "install whisper-cpp" hint when the binary is missing. Cheap —
/// re-call on every settings panel render is fine.
#[tauri::command]
fn slab_beacon_voice_stt_capabilities() -> SttCapabilities {
    stt_capabilities()
}

/// Start a fresh mic recording. `engine` is optional — pass `None` to
/// use the platform default (currently always `whisper-cpp`). Cancels
/// any in-flight recording first (single-slot policy, mirroring the
/// TTS side).
///
/// `model` (v1.9.2) — optional explicit whisper.cpp model override
/// (e.g. `"base.en"` or absolute `.bin` path). When omitted, falls
/// back to `BeaconConfig.voice.stt_model`, then `$WHISPER_MODEL`,
/// then whisper.cpp's compiled-in default.
///
/// Returns `Ok(())` on success. On failure (no recorder, unknown
/// engine, spawn error) the frontend gets a user-grade error message.
#[tauri::command]
fn slab_beacon_voice_stt_start(
    engine: Option<String>,
    model: Option<String>,
    session: tauri::State<'_, std::sync::Arc<SttSession>>,
) -> CmdResult<()> {
    let eng = match engine.as_deref() {
        Some(id) => match SttEngine::from_id(id) {
            Some(e) => e,
            None => {
                return CmdResult::Err {
                    message: format!("unknown STT engine: {id}"),
                };
            }
        },
        None => match SttEngine::platform_default() {
            Some(e) => e,
            None => {
                return CmdResult::Err {
                    message: "no STT engine available on this platform".into(),
                };
            }
        },
    };
    // Precedence: explicit `model` arg → BeaconConfig.voice.stt_model
    // → (env / whisper default — handled inside build_whisper_cmd).
    let resolved_model = model.filter(|s| !s.is_empty()).or_else(|| {
        crate::ai::config::load()
            .ok()
            .and_then(|c| c.beacon.voice.stt_model)
            .filter(|s| !s.is_empty())
    });
    match session.start(eng, resolved_model) {
        Ok(()) => CmdResult::Ok { value: () },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// Stop the in-flight recording and transcribe. The WAV file is
/// **always** unlinked before this returns, even on transcription
/// failure — Slab never persists audio bytes.
///
/// Returns the transcript text + detected language + recording
/// duration. Errors carry a user-grade message ("whisper-cli not
/// installed", "transcription produced no text", etc.).
#[tauri::command]
fn slab_beacon_voice_stt_stop(
    session: tauri::State<'_, std::sync::Arc<SttSession>>,
) -> CmdResult<Transcript> {
    match session.stop() {
        Ok(t) => CmdResult::Ok { value: t },
        Err(e) => CmdResult::Err {
            message: e.to_string(),
        },
    }
}

/// True iff a recording slot is currently held. Cheap (a single mutex
/// lock with no syscall). The UI polls this to decide whether to show
/// the "recording…" indicator on focus return.
#[tauri::command]
fn slab_beacon_voice_stt_is_recording(
    session: tauri::State<'_, std::sync::Arc<SttSession>>,
) -> bool {
    session.is_recording()
}

/// Discard the in-flight recording without transcribing. Kills the
/// recorder, deletes the WAV, drops the slot. No-op if not recording.
/// Returns `()` on success — there's nothing to report.
///
/// Frontend invokes this on ESC while recording, or via right-click
/// → "Cancel recording" on the mic button (v1.9.2). Mirrors the privacy
/// guarantee of `stop()`: the WAV is unlinked before this returns,
/// audio bytes never persist or leave the device.
#[tauri::command]
fn slab_beacon_voice_stt_cancel(
    session: tauri::State<'_, std::sync::Arc<SttSession>>,
) -> CmdResult<()> {
    session.cancel();
    CmdResult::Ok { value: () }
}

/// Enumerate installed + suggested whisper.cpp models for the
/// Settings-panel picker (v1.9.2). Cheap (single readdir on
/// `$SLAB_MODELS_DIR` / `~/.slab/models/`); safe to call on every
/// settings render. Always returns the built-in suggestions even if
/// the directory doesn't exist.
#[tauri::command]
fn slab_beacon_voice_stt_list_models() -> Vec<crate::ai::stt::WhisperModelInfo> {
    crate::ai::stt::list_whisper_models()
}

// ─── Signet (v3.10.0): PDF digital signatures ──────────────────────────────
//
// Three commands the renderer needs:
//   - signet_load_identity → preview a PEM cert+key for the signer UI
//   - signet_sign          → produce a signed copy of a PDF
//   - signet_verify        → list every signature in a PDF + their status

#[derive(serde::Serialize)]
pub struct SignetIdentityPreviewDto {
    pub subject_cn: String,
    pub algorithm: String,
    pub not_before_unix: i64,
    pub not_after_unix: i64,
    pub chain_len: usize,
}

#[derive(serde::Deserialize)]
pub struct SignetSignArgs {
    pub input_path: String,
    pub output_path: String,
    pub cert_pem_path: String,
    pub key_pem_path: String,
    pub key_password: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub contact_info: Option<String>,
    pub field_name: Option<String>,
    /// Optional visible signature appearance. When `None`, the signature is
    /// invisible (legacy v3.10.0 behaviour). When `Some`, a Form XObject
    /// stamp is rendered on the specified page + rect.
    pub appearance: Option<SignetAppearanceArgs>,
    /// Optional RFC 3161 timestamp authority URL. When set, the signature
    /// is upgraded to CAdES-T (BES + embedded timestamp token). Network
    /// call. Default: `None` (offline / CAdES-BES).
    pub tsa_url: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct SignetAppearanceArgs {
    /// 1-indexed page number.
    pub page: u32,
    /// `[llx, lly, urx, ury]` in PDF user-space units.
    pub rect: [f32; 4],
    pub show_name: Option<bool>,
    pub show_date: Option<bool>,
    pub show_reason: Option<bool>,
    pub show_location: Option<bool>,
    pub font_size: Option<f32>,
}

#[derive(serde::Serialize)]
pub struct SignetSignResultDto {
    pub output_bytes: u64,
    pub byte_range: [u64; 4],
    pub field_name: String,
    pub signature_hex_used: usize,
    pub elapsed_ms: u64,
}

#[derive(serde::Serialize)]
pub struct SignetVerifiedDto {
    pub field_name: String,
    pub signer_cn: String,
    pub signed_at_unix: i64,
    pub byte_range: [u64; 4],
    pub coverage: crate::pdf::signet::Coverage,
    pub digest_status: crate::pdf::signet::DigestStatus,
    pub crypto_status: crate::pdf::signet::CryptoStatus,
    pub chain_status: crate::pdf::signet::ChainStatus,
    pub cert_subject: String,
    pub cert_issuer: String,
    pub cert_not_before: i64,
    pub cert_not_after: i64,
}

#[tauri::command]
async fn signet_load_identity(
    cert_pem_path: String,
    key_pem_path: String,
    key_password: Option<String>,
) -> Result<SignetIdentityPreviewDto, String> {
    let id = crate::pdf::signet::SigningIdentity::load_pem_pair(
        std::path::Path::new(&cert_pem_path),
        std::path::Path::new(&key_pem_path),
        key_password.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    Ok(SignetIdentityPreviewDto {
        subject_cn: id.subject_cn,
        algorithm: id.algorithm.label().to_string(),
        not_before_unix: id.not_before_unix,
        not_after_unix: id.not_after_unix,
        chain_len: id.chain_der.len(),
    })
}

/// Format a Unix timestamp (seconds since epoch) as "YYYY-MM-DD HH:MM UTC".
/// Used to seed the visible-signature appearance date line. No chrono dep.
fn format_utc_human(secs: i64) -> String {
    const SECS_PER_DAY: i64 = 86_400;
    let mut days = secs.div_euclid(SECS_PER_DAY);
    let t = secs.rem_euclid(SECS_PER_DAY) as u32;
    let h = t / 3600;
    let mi = (t % 3600) / 60;
    let mut y: i64 = 1970;
    loop {
        let dy = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 1u32;
    let mut d = days as u32;
    for (i, &dm) in dim.iter().enumerate() {
        if d < dm {
            mo = i as u32 + 1;
            break;
        }
        d -= dm;
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", y, mo, d + 1, h, mi)
}

#[tauri::command]
async fn signet_sign(args: SignetSignArgs) -> Result<SignetSignResultDto, String> {
    let id = crate::pdf::signet::SigningIdentity::load_pem_pair(
        std::path::Path::new(&args.cert_pem_path),
        std::path::Path::new(&args.key_pem_path),
        args.key_password.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    let opts = crate::pdf::signet::SignOptions {
        reason: args.reason.clone(),
        location: args.location.clone(),
        contact_info: args.contact_info,
        field_name: args.field_name,
        appearance: args.appearance.map(|a| {
            use crate::pdf::signet_pro::appearance::AppearanceSpec;
            // Best-effort human signing-time string (UTC). Format: "YYYY-MM-DD HH:MM UTC".
            let when = {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                format_utc_human(secs)
            };
            AppearanceSpec {
                page: a.page.max(1),
                rect: a.rect,
                font_size: a.font_size.unwrap_or(9.0),
                show_name: a.show_name.unwrap_or(true),
                show_date: a.show_date.unwrap_or(true),
                show_reason: a.show_reason.unwrap_or(args.reason.is_some()),
                show_location: a.show_location.unwrap_or(args.location.is_some()),
                image: None,
                reason: args.reason.clone(),
                location: args.location.clone(),
                signing_time: Some(when),
            }
        }),
        tsa_url: args.tsa_url.filter(|s| !s.is_empty()),
    };
    let report = crate::pdf::signet::sign_pdf(
        std::path::Path::new(&args.input_path),
        std::path::Path::new(&args.output_path),
        &id,
        &opts,
    )
    .map_err(|e| e.to_string())?;
    Ok(SignetSignResultDto {
        output_bytes: report.output_bytes,
        byte_range: report.byte_range,
        field_name: report.field_name,
        signature_hex_used: report.signature_hex_used,
        elapsed_ms: report.elapsed_ms,
    })
}

#[tauri::command]
async fn signet_verify(input_path: String) -> Result<Vec<SignetVerifiedDto>, String> {
    // Best-effort: load the user's trust store from the default location.
    // If the directory doesn't exist, fall back to an empty store — chain
    // checks will report SelfSigned/Untrusted as appropriate.
    let store = crate::pdf::signet::TrustStore::load_default()
        .unwrap_or_else(|_| crate::pdf::signet::TrustStore::new());
    let results = crate::pdf::signet::verify(std::path::Path::new(&input_path), &store)
        .map_err(|e| e.to_string())?;
    Ok(results
        .into_iter()
        .map(|v| SignetVerifiedDto {
            field_name: v.field_name,
            signer_cn: v.signer_cn,
            signed_at_unix: v.signed_at_unix,
            byte_range: v.byte_range,
            coverage: v.coverage,
            digest_status: v.digest_status,
            crypto_status: v.crypto_status,
            chain_status: v.chain_status,
            cert_subject: v.cert_subject,
            cert_issuer: v.cert_issuer,
            cert_not_before: v.cert_not_before,
            cert_not_after: v.cert_not_after,
        })
        .collect())
}

#[derive(serde::Deserialize)]
pub struct SignetProBatchArgs {
    pub input_dir: String,
    pub output_dir: String,
    pub cert_pem_path: String,
    pub key_pem_path: String,
    pub key_password: Option<String>,
    /// Recurse into subdirectories. Default false.
    pub recursive: Option<bool>,
    /// "suffix" (default — appends -signed) or "mirror" (preserves filename).
    pub naming: Option<String>,
    /// Skip files whose output already exists. Default false.
    pub skip_existing: Option<bool>,
    /// Optional human-readable reason embedded in every signature.
    pub reason: Option<String>,
    /// Optional location embedded in every signature.
    pub location: Option<String>,
    /// Optional RFC 3161 TSA URL — when set, every signed PDF is upgraded
    /// to CAdES-T with an embedded timestamp token.
    pub tsa_url: Option<String>,
}

#[derive(serde::Serialize)]
pub struct SignetProBatchEntryDto {
    pub input: String,
    pub output: String,
    pub ok: bool,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(serde::Serialize)]
pub struct SignetProBatchReportDto {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub elapsed_ms: u64,
    pub success_rate: f64,
    pub entries: Vec<SignetProBatchEntryDto>,
}

#[derive(serde::Serialize, Clone)]
pub struct SignetProBatchProgressDto {
    pub done: usize,
    pub total: usize,
    /// 0.0..=1.0
    pub fraction: f64,
}

#[derive(serde::Serialize)]
pub struct SignetProBatchPlannedDto {
    pub jobs: Vec<SignetProBatchEntryDto>,
}

/// Marketing-grade copy: this is the command every law firm + IT admin
/// will call. Drop a folder of contracts in, get signed PDFs out.
///
/// Emits `signet-pro/batch-progress` events of [`SignetProBatchProgressDto`]
/// after every file completes. Frontend wires these to a progress bar.
#[tauri::command]
async fn signet_pro_batch_sign(
    app: tauri::AppHandle,
    args: SignetProBatchArgs,
) -> Result<SignetProBatchReportDto, String> {
    use crate::pdf::signet_pro::batch::{sign_folder, BatchOptions, NameStrategy};
    let id = crate::pdf::signet::SigningIdentity::load_pem_pair(
        std::path::Path::new(&args.cert_pem_path),
        std::path::Path::new(&args.key_pem_path),
        args.key_password.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    let id = std::sync::Arc::new(id);

    let naming = match args.naming.as_deref() {
        Some("mirror") => NameStrategy::Mirror,
        _ => NameStrategy::SuffixSigned,
    };
    let opts = BatchOptions {
        recursive: args.recursive.unwrap_or(false),
        naming,
        skip_if_output_exists: args.skip_existing.unwrap_or(false),
    };

    let reason = args.reason.clone();
    let location = args.location.clone();
    let tsa_url = args.tsa_url.clone().filter(|s| !s.is_empty());
    let id_for_signer = std::sync::Arc::clone(&id);

    let app_for_progress = app.clone();
    let in_dir = std::path::PathBuf::from(&args.input_dir);
    let out_dir = std::path::PathBuf::from(&args.output_dir);

    // Run the rayon-parallel batch in a blocking-task slot so we don't
    // hog the tauri async runtime. The signer closure is Send+Sync (Arc).
    let report = tauri::async_runtime::spawn_blocking(move || {
        sign_folder(
            &in_dir,
            &out_dir,
            &opts,
            move |job| {
                let sign_opts = crate::pdf::signet::SignOptions {
                    reason: reason.clone(),
                    location: location.clone(),
                    contact_info: None,
                    field_name: None,
                    appearance: None,
                    tsa_url: tsa_url.clone(),
                };
                // Ensure output dir exists for each parent — cheap, idempotent.
                if let Some(p) = job.output.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                crate::pdf::signet::sign_pdf(&job.input, &job.output, &id_for_signer, &sign_opts)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            move |done, total| {
                use tauri::Emitter;
                let fraction = if total == 0 {
                    0.0
                } else {
                    done as f64 / total as f64
                };
                let _ = app_for_progress.emit(
                    "signet-pro/batch-progress",
                    SignetProBatchProgressDto {
                        done,
                        total,
                        fraction,
                    },
                );
            },
        )
    })
    .await
    .map_err(|e| format!("batch task join: {e}"))?
    .map_err(|e| format!("batch IO: {e}"))?;

    Ok(SignetProBatchReportDto {
        total: report.total,
        succeeded: report.succeeded,
        failed: report.failed,
        elapsed_ms: report.elapsed.as_millis() as u64,
        success_rate: report.success_rate(),
        entries: report
            .entries
            .into_iter()
            .map(|e| SignetProBatchEntryDto {
                input: e.input.display().to_string(),
                output: e.output.display().to_string(),
                ok: e.ok,
                error: e.error,
                elapsed_ms: e.elapsed.as_millis() as u64,
            })
            .collect(),
    })
}

/// Dry-run: walk the input folder, return the planned (input → output) pairs
/// without signing anything. Powers the panel's "preview before sign" row.
#[tauri::command]
async fn signet_pro_batch_plan(
    input_dir: String,
    output_dir: String,
    recursive: Option<bool>,
    naming: Option<String>,
    skip_existing: Option<bool>,
) -> Result<SignetProBatchPlannedDto, String> {
    use crate::pdf::signet_pro::batch::{plan_batch, BatchOptions, NameStrategy};
    let opts = BatchOptions {
        recursive: recursive.unwrap_or(false),
        naming: match naming.as_deref() {
            Some("mirror") => NameStrategy::Mirror,
            _ => NameStrategy::SuffixSigned,
        },
        skip_if_output_exists: skip_existing.unwrap_or(false),
    };
    let jobs = plan_batch(
        std::path::Path::new(&input_dir),
        std::path::Path::new(&output_dir),
        &opts,
    )
    .map_err(|e| e.to_string())?;
    Ok(SignetProBatchPlannedDto {
        jobs: jobs
            .into_iter()
            .map(|j| SignetProBatchEntryDto {
                input: j.input.display().to_string(),
                output: j.output.display().to_string(),
                ok: true,
                error: None,
                elapsed_ms: 0,
            })
            .collect(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(windows::WindowRegistry::new())
        .manage(plugins::PluginRegistry::new())
        // Workshop (v2.0.0 Slice 6.6): process-global registry of live
        // plugin actor handles. Slice 6.7 wires Tauri commands that
        // broadcast document-open/close events to every live actor.
        .manage(plugins::PluginRuntimeRegistry::default())
        .manage(std::sync::Arc::new(VoiceSession::new()))
        .manage(std::sync::Arc::new(SttSession::new()))
        // Theater (v2.3.0): single-active presenter session shared between
        // the audience fullscreen window and the presenter control window.
        .manage(std::sync::Arc::new(theater::TheaterManager::new()))
        .setup(|app| {
            // Cabinet (v1.1.0): restore last session's detached windows
            // from ~/.slab/windows.json. Quiet on error.
            let handle = tauri::Manager::app_handle(app).clone();
            windows::restore_windows(&handle);

            // Foundry (v1.3.0): discover plugins under ~/.slab/plugins
            // at boot. Quiet on error — if HOME is unset or the dir is
            // missing, registry stays empty and the UI shows an empty
            // panel rather than crashing.
            //
            // Workshop (v2.0.1 — Slice 11): before discovery, seed any
            // bundled plugins so first boot has a working example
            // installed. seed_bundled_plugins is idempotent: it only
            // writes when the destination is missing OR when the
            // shipped version differs from the on-disk one, so
            // repeated boots are no-ops and user-uninstalled bundled
            // plugins stay uninstalled for the lifetime of the
            // current install.
            if let Some(root) = plugins::default_plugins_root() {
                let _seeded = plugins::seed_bundled_plugins(&root);
                let enabled = plugins::default_state_path()
                    .as_deref()
                    .map(plugins::read_enabled_state)
                    .unwrap_or_default();
                if let Some(reg) = tauri::Manager::try_state::<plugins::PluginRegistry>(app) {
                    reg.discover(&root, &enabled);
                }
            }

            // Hopper (v3.20.0): boot the watched-folder PDF automation
            // service. Reads watches + run-log from sqlite, starts the
            // notify-backed watcher, wires an Ollama-backed AI title
            // provider. Best-effort: if any sub-init fails, Slab still
            // launches without Hopper rather than panicking.
            match crate::pdf::hopper::cmds::build_default_service(&handle) {
                Ok(svc) => {
                    use tauri::Manager as _;
                    app.manage(svc);
                }
                Err(e) => {
                    eprintln!("hopper: bootstrap failed, panel will be empty: {e}");
                }
            }

            // Marketplace install-log retention (v3.40 Slice 66): if the
            // 24h debounce window has elapsed since the last auto-prune
            // (or no prune has ever run), trim install-log rows older
            // than the user's configured `retain_days` (default 365).
            // Best-effort + non-fatal — if the log can't be opened
            // (HOME unset, disk full, schema corruption) we log to
            // stderr and Slab boots normally. The user's manual
            // "Clear older than 90d" affordance keeps working
            // regardless.
            match marketplace::InstallLog::open(marketplace::default_log_path()) {
                Ok(mut log) => match log.auto_prune_if_due_now() {
                    Ok(marketplace::AutoPruneOutcome::Pruned {
                        rows_removed,
                        retain_days,
                        ..
                    }) => {
                        if rows_removed > 0 {
                            eprintln!(
                                "marketplace install-log: auto-pruned {rows_removed} \
                                 event(s) older than {retain_days} day(s)"
                            );
                        }
                    }
                    Ok(marketplace::AutoPruneOutcome::Skipped { .. }) => {
                        // Debounce window not yet elapsed — silent skip.
                    }
                    Err(e) => {
                        eprintln!("marketplace install-log: auto-prune failed: {e}");
                    }
                },
                Err(e) => {
                    eprintln!("marketplace install-log: could not open for auto-prune: {e}");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            slab_first_launch_probe,
            slab_first_launch_install,
            slab_first_launch_skip,
            slab_merge,
            slab_split_ranges,
            slab_bates_apply,
            slab_bates_batch,
            slab_legal_stamp_apply,
            slab_split_every,
            slab_split_by_pattern,
            slab_find_matching_pages,
            slab_outline_starts,
            slab_page_count,
            slab_loom_layout_summary,
            slab_loom_classify_summary,
            slab_loom_reading_order_summary,
            slab_loom_alt_text_summary,
            slab_loom_tag_document,
            slab_loom_validate,
            slab_loom_matterhorn_digest,
            slab_press_convert,
            slab_rotate,
            slab_rotate_permanent,
            slab_delete_pages,
            slab_duplicate_pages,
            slab_reorder_pages,
            slab_apply_page_ops,
            slab_pages_build,
            slab_forms_inspect,
            slab_forms_fill,
            slab_forms_batch_fill,
            slab_forms_design_add,
            slab_forms_design_edit,
            slab_forms_design_delete,
            slab_forms_autodetect,
            slab_find_text_spans,
            slab_replace_text_span,
            slab_diff_pdfs,
            slab_diff_export_report,
            slab_diff3_pdfs,
            slab_diff3_materialize,
            slab_diff3_export_pdf,
            slab_visual_diff_pdfs,
            slab_stack_export_redline,
            slab_slides_analyze,
            slab_theater_export_annotated,
            slab_theater_start,
            slab_theater_end,
            slab_theater_snapshot,
            slab_theater_next,
            slab_theater_prev,
            slab_theater_jump,
            slab_theater_toggle_blackout,
            slab_theater_toggle_whiteout,
            slab_theater_toggle_laser,
            slab_theater_toggle_ink,
            slab_theater_toggle_spotlight,
            slab_theater_push_stroke,
            slab_theater_undo_stroke,
            slab_theater_clear_strokes,
            slab_theater_open_windows,
            slab_theater_close_windows,
            slab_extract_text,
            slab_extract_text_save,
            slab_info,
            slab_compress,
            slab_compactor_estimate,
            slab_compactor_compact,
            slab_streamline_inspect,
            slab_streamline_linearize,
            slab_streamline_audit,
            slab_reflow_to_docx,
            slab_markdown_to_md,
            slab_markdown_to_html,
            slab_bind_to_epub,
            slab_tabulate_to_xlsx,
            slab_slide_to_pptx,
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
            slab_redact_true,
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
            slab_pdfa_validate,
            slab_pdfa_font_audit,
            slab_pdfa_convert,
            slab_pdfa_inspect,
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
            slab_quill_smart_fill_propose,
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
            slab_beacon_index_list,
            slab_beacon_index_forget_many,
            slab_beacon_index_stats_by_model,
            slab_beacon_index_find_stale,
            slab_beacon_index_forget_stale,
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
            slab_library_search,
            slab_library_list_tags,
            slab_library_recently_used_tags,
            slab_library_tag_usage_counts,
            slab_library_delete_unused_tags,
            slab_library_saved_view_save,
            slab_library_saved_view_list,
            slab_library_saved_view_delete,
            slab_library_saved_view_rename,
            slab_library_saved_view_update_filter,
            slab_library_saved_view_duplicate,
            slab_library_saved_view_set_pinned,
            slab_library_saved_view_reorder,
            slab_collection_create,
            slab_collection_list,
            slab_collection_rename,
            slab_collection_delete,
            slab_collection_set_color,
            slab_collection_reorder,
            slab_collection_duplicate,
            slab_collection_add_docs,
            slab_collection_remove_docs,
            slab_collection_list_docs,
            slab_smart_collection_create,
            slab_smart_collection_list,
            slab_smart_collection_delete,
            slab_smart_collection_expand,
            slab_preset_list,
            slab_preset_apply,
            slab_preset_already_applied,
            slab_personal_preset_save,
            slab_personal_preset_list,
            slab_personal_preset_delete,
            slab_personal_preset_apply,
            slab_personal_preset_rename,
            slab_personal_preset_duplicate,
            slab_personal_presets_export,
            slab_personal_presets_import,
            slab_smart_folders_list,
            slab_smart_folders_reorder,
            slab_smart_folders_pin,
            slab_library_suggestions_list,
            slab_library_suggestions_dismiss,
            slab_library_suggestions_accept,
            slab_library_search_log_count,
            slab_library_recent_searches,
            slab_library_clear_search_history,
            slab_library_index_stats,
            slab_library_tag_suggestions_for_doc,
            slab_library_tag_suggestions_bulk_for_untagged,
            slab_library_tag_suggestion_accept,
            slab_library_tag_suggestion_dismiss,
            slab_library_tag_suggestion_undismiss_all,
            slab_library_tag_suggestions_accept_bulk,
            slab_library_tag_suggestions_list_dismissed,
            slab_library_tag_suggestion_undismiss_one,
            slab_library_tag_suggestions_bulk_for_filter,
            slab_library_tag_suggestion_stats,
            slab_smart_collection_update,
            slab_library_add_tag,
            slab_library_set_tag_color,
            slab_library_rename_tag,
            slab_library_set_tag_description,
            slab_library_set_doc_title,
            slab_library_set_doc_notes,
            slab_library_set_doc_starred,
            slab_library_merge_tags,
            slab_library_set_doc_tags,
            slab_library_bulk_apply_tag,
            slab_library_bulk_remove_tag,
            slab_library_remove_document,
            slab_library_remove_tag,
            slab_library_rescan_all,
            slab_library_ocr_queue_list_pending,
            slab_library_ocr_queue_run_one,
            slab_library_ocr_queue_run_all,
            slab_library_ocr_queue_stats,
            slab_library_ocr_queue_list_failed,
            slab_library_ocr_queue_requeue,
            slab_library_ocr_queue_requeue_all_failed,
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
            plugin_grants_get,
            plugin_grants_set,
            plugin_grants_reset,
            slab_plugins_document_opened,
            slab_plugins_document_closed,
            slab_marketplace_index,
            slab_marketplace_install,
            slab_marketplace_uninstall,
            slab_marketplace_install_events,
            slab_marketplace_install_history_recent,
            slab_marketplace_plugin_install_stats,
            slab_marketplace_install_log_summary,
            slab_marketplace_install_log_prune,
            slab_marketplace_install_log_export_csv,
            slab_marketplace_install_log_export_json,
            slab_marketplace_install_log_list_filtered,
            slab_marketplace_install_log_recent_plugin_ids,
            slab_marketplace_install_log_plugin_histogram,
            slab_marketplace_install_log_export_histogram_csv,
            slab_marketplace_install_log_export_histogram_json,
            slab_marketplace_install_log_activity_timeline,
            slab_marketplace_install_log_export_activity_timeline_csv,
            slab_marketplace_install_log_export_activity_timeline_json,
            slab_marketplace_install_log_bucket_drilldown,
            slab_marketplace_install_log_export_bucket_drilldown_csv,
            slab_marketplace_install_log_export_bucket_drilldown_json,
            slab_marketplace_install_log_retention_policy,
            slab_marketplace_install_log_set_retention_days,
            slab_marketplace_install_log_auto_prune,
            slab_marketplace_install_log_plugin_retention_overrides,
            slab_marketplace_install_log_set_plugin_retention,
            slab_marketplace_install_log_clear_plugin_retention,
            slab_marketplace_install_log_export_plugin_retention_csv,
            slab_marketplace_install_log_export_plugin_retention_json,
            slab_marketplace_install_log_auto_prune_runs,
            slab_marketplace_install_log_clear_auto_prune_runs,
            slab_marketplace_install_log_export_auto_prune_runs_csv,
            slab_marketplace_install_log_export_auto_prune_runs_json,
            slab_marketplace_list_update_targets,
            slab_marketplace_update_all,
            slab_beacon_voice_capabilities,
            slab_beacon_voice_list_voices,
            slab_beacon_voice_speak,
            slab_beacon_voice_stop,
            slab_beacon_voice_is_speaking,
            slab_beacon_voice_test,
            slab_beacon_voice_stt_capabilities,
            slab_beacon_voice_stt_start,
            slab_beacon_voice_stt_stop,
            slab_beacon_voice_stt_is_recording,
            slab_beacon_voice_stt_cancel,
            slab_beacon_voice_stt_list_models,
            signet_load_identity,
            signet_sign,
            signet_verify,
            signet_pro_batch_sign,
            signet_pro_batch_plan,
            crate::pdf::atelier::cmds::atelier_save_recipe,
            crate::pdf::atelier::cmds::atelier_load_recipes,
            crate::pdf::atelier::cmds::atelier_delete_recipe,
            crate::pdf::atelier::cmds::atelier_run_batch,
            crate::pdf::hopper::cmds::slab_hopper_list_watches,
            crate::pdf::hopper::cmds::slab_hopper_add_watch,
            crate::pdf::hopper::cmds::slab_hopper_remove_watch,
            crate::pdf::hopper::cmds::slab_hopper_set_enabled,
            crate::pdf::hopper::cmds::slab_hopper_list_runs,
            crate::pdf::hopper::cmds::slab_hopper_run_now,
            crate::pdf::hopper::cmds::slab_hopper_describe,
            crate::pdf::hopper::cmds::slab_hopper_get_rules,
            crate::pdf::hopper::cmds::slab_hopper_set_rules,
            crate::pdf::hopper::cmds::slab_hopper_test_rules,
            crate::pdf::hopper::cmds::slab_hopper_rule_coverage,
            crate::pdf::hopper::cmds::slab_hopper_sample_drilldown,
            crate::pdf::hopper::cmds::slab_hopper_plan_backfill,
            crate::pdf::hopper::cmds::slab_hopper_execute_backfill,
            crate::pdf::hopper::cmds::slab_hopper_execute_backfill_async,
            crate::pdf::hopper::cmds::slab_hopper_cancel_backfill,
            crate::pdf::hopper::cmds::slab_hopper_list_backfill_runs,
            crate::pdf::hopper::cmds::slab_hopper_export_backfill_csv,
            crate::pdf::hopper::cmds::slab_hopper_export_drilldown_csv,
            crate::pdf::hopper::cmds::slab_hopper_export_drilldown_json,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Slab");
}
