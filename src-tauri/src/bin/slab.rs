// Slab CLI — invoke any PDF op from the terminal.
//
// Usage:
//   slab merge a.pdf b.pdf c.pdf -o out.pdf
//   slab split-every input.pdf 5 ./out/
//   slab rotate input.pdf 1,3 90 -o out.pdf
//   slab info input.pdf
//   slab page-count input.pdf
//   slab compress input.pdf -o out.pdf
//   slab encrypt input.pdf -o out.pdf --password hunter2
//   slab grayscale input.pdf -o out.pdf
//   slab md2pdf README.md -o out.pdf
//   slab extract-text input.pdf
//   slab auto-redact input.pdf -o out.pdf --presets email,ssn
//
// Run `slab help` for the full list.

use slab_lib::ai::auto_tag::AutoTagOpts;
use slab_lib::ai::config::{load as load_ai_config, make_provider};
use slab_lib::pdf::annot_export::{extract as extract_annots, to_markdown as annots_to_md};
use slab_lib::pdf::auto_redact::{auto_redact, AutoRedactOpts};
use slab_lib::pdf::bates::{apply_bates, BatesOpts, BatesPosition};
use slab_lib::pdf::booklet::{impose_booklet, BookletOpts};
use slab_lib::pdf::compress::compress;
use slab_lib::pdf::encrypt::{decrypt, encrypt};
use slab_lib::pdf::extract::{extract_text, extract_text_concat};
use slab_lib::pdf::flatten::{flatten as do_flatten, FlattenMode, FlattenOpts};
use slab_lib::pdf::grayscale::{grayscale, GrayscaleOpts};
use slab_lib::pdf::info::info;
use slab_lib::pdf::invert::{invert_colors, InvertOpts};
use slab_lib::pdf::library::{
    auto_tag_run_one, default_db_path as library_db_path, ocr_queue_list_pending,
    ocr_queue_run_one, query_documents, LibraryDb, LibraryFilter,
};
use slab_lib::pdf::md2pdf::{render as md2pdf_render, Md2PdfOpts};
use slab_lib::pdf::merge::merge_pdfs;
use slab_lib::pdf::metadata::{read_metadata, strip_metadata};
use slab_lib::pdf::ocr::{ocr, OcrOpts};
use slab_lib::pdf::outline::{read_outline, write_outline, OutlineNode};
use slab_lib::pdf::pages::{delete_pages, rotate_pages, Rotation};
use slab_lib::pdf::polyglot::{polyglot_to_pdf, PolyglotOpts};
use slab_lib::pdf::preflight::{preflight, PreflightOpts, Status as PreflightStatus};
use slab_lib::pdf::repair::repair as do_repair;
use slab_lib::pdf::reverse::reverse_pages;
use slab_lib::pdf::sanitize::{sanitize as do_sanitize, SanitizeOpts};
use slab_lib::pdf::scan_audit::{audit as scan_audit, PageClassification, Recommendation};
use slab_lib::pdf::split::{page_count, split_by_ranges, split_every, PageRange};
use slab_lib::pdf::table_extract::{extract_tables, to_csv as table_to_csv, TableOpts};
use slab_lib::pdf::PdfError;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        eprintln!("{}", help_text());
        return ExitCode::from(2);
    }
    let cmd = argv[1].as_str();
    let rest = &argv[2..];

    let result: Result<(), CliError> = match cmd {
        "help" | "--help" | "-h" => {
            println!("{}", help_text());
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("slab {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "info" => cmd_info(rest),
        "page-count" => cmd_page_count(rest),
        "merge" => cmd_merge(rest),
        "split-every" => cmd_split_every(rest),
        "split-ranges" => cmd_split_ranges(rest),
        "rotate" => cmd_rotate(rest),
        "delete-pages" => cmd_delete_pages(rest),
        "compress" => cmd_compress(rest),
        "encrypt" => cmd_encrypt(rest),
        "decrypt" => cmd_decrypt(rest),
        "grayscale" => cmd_grayscale(rest),
        "md2pdf" => cmd_md2pdf(rest),
        "extract-text" => cmd_extract_text(rest),
        "auto-redact" => cmd_auto_redact(rest),
        "read-metadata" => cmd_read_metadata(rest),
        "strip-metadata" => cmd_strip_metadata(rest),
        "ocr" => cmd_ocr(rest),
        "outline" => cmd_outline(rest),
        "polyglot" => cmd_polyglot(rest),
        "flatten" => cmd_flatten(rest),
        "sanitize" => cmd_sanitize(rest),
        "repair" => cmd_repair(rest),
        "export-annots" => cmd_export_annots(rest),
        "bates" => cmd_bates(rest),
        "invert" => cmd_invert(rest),
        "reverse" => cmd_reverse(rest),
        "booklet" => cmd_booklet(rest),
        "lens" => cmd_lens(rest),
        other => Err(CliError::Usage(format!(
            "Unknown command: {other}\n\nRun `slab help`."
        ))),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(2)
        }
        Err(CliError::Op(e)) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    Op(PdfError),
}

impl From<PdfError> for CliError {
    fn from(e: PdfError) -> Self {
        CliError::Op(e)
    }
}

fn help_text() -> String {
    format!(
        "slab {} — free, offline PDF tool

Usage: slab <command> [args]

Commands:
  info <file>                        Print page count, version, encryption status
  page-count <file>                  Print page count only
  merge <in1> <in2> ... -o <out>     Concatenate PDFs
  split-every <file> <n> <out-dir>   Split into N-page chunks
  split-ranges <file> <r1,r2..> <dir>   e.g. 1-3,5,7-9
  rotate <file> <pages> <deg> -o <out>  pages comma-list (1-based), deg ∈ 90/180/270
  delete-pages <file> <pages> -o <out>
  compress <file> -o <out>           Re-compress streams
  encrypt <file> -o <out> --password <pwd>
  decrypt <file> -o <out> --password <pwd>
  grayscale <file> -o <out> [--pages 1,2,3]
  md2pdf <markdown-file> -o <out> [--page-size A4|Letter|Legal]
  extract-text <file> [-o <out.txt>]
  auto-redact <file> -o <out> [--presets email,ssn,phone,cc] [--patterns 'regex' ...]
  read-metadata <file>
  strip-metadata <file> -o <out>
  ocr <file> -o <out> [--lang eng] [--dpi 300]
                                     Rasterize + run Tesseract → searchable PDF.
                                     Requires `pdftoppm` and `tesseract` on PATH.
  outline read <file>                Print outline as JSON
  outline write <file> -o <out> --json <outline.json>
                                     Replace the /Outlines tree from JSON
  polyglot <file> -o <out> [--page-size A4|Letter|Legal]
                                     Convert .docx/.xlsx/.pptx/.html/.epub/csv/
                                     json/xml/img/audio → PDF. Requires
                                     `markitdown` on PATH
                                     (`pipx install 'markitdown[all]'`).
  flatten <file> -o <out> [--no-widgets] [--raster] [--dpi N]
                                     Bake annotations into the page content
                                     stream and remove /AcroForm. The result
                                     is a static PDF with no editable fields.
  sanitize <file> -o <out> [--keep-links]
                                     Strip JavaScript, embedded files, launch
                                     actions, /OpenAction, /AA, /XFA, and (by
                                     default) external URI links. Visual
                                     appearance unchanged.
  repair <file> -o <out>             Rebuild the xref table and drop
                                     unreachable indirect objects. Fixes most
                                     'this PDF won't open' files and shrinks
                                     PDFs bloated by incremental edits.
  export-annots <file> -o <out.md>   Extract highlights & notes as Markdown

Lens commands (v0.13.0 — OCR / Vision / Tables / AI):
  lens audit <file>                  Classify each page as text/image/mixed/empty
                                     and print the recommended action.
  lens tables <file> <page>          Extract tables from a page → CSV.
                                     Optional: --min-rows N, --min-cols N,
                                     -o <out.csv> (multiple tables → suffixed).
                                     Requires `pdftotext` on PATH.
  lens ocr-queue list                List library docs queued for OCR
                                     (state ∈ scanned, mixed).
  lens ocr-queue run <doc-id>        OCR a single library doc.
                                     [--lang eng] [--dpi 300]
  lens ocr-queue run-all             Drain the queue sequentially.
                                     [--lang eng] [--dpi 300]
  lens auto-tag <doc-id>             AI auto-tag a library doc via Beacon
                                     provider (~/.slab/config.toml).
                                     [--max-tags 5]
  lens auto-tag --all                Auto-tag every library doc.
                                     [--max-tags 5]
  lens preflight                     Probe every external dep used by
                                     Lens features (Poppler pdftoppm/
                                     pdftotext, Tesseract, Ollama) and
                                     print a readiness report. Exits
                                     non-zero if any check fails.
                                     [--json] [--ollama <url>]

  help, --help                       This help
  version, --version                 Print version

Examples:
  slab merge a.pdf b.pdf -o combined.pdf
  slab split-every report.pdf 10 ./chunks/
  slab grayscale color.pdf -o gray.pdf
  slab auto-redact contract.pdf -o redacted.pdf --presets email,ssn
  echo '# Hello' > t.md && slab md2pdf t.md -o t.pdf
",
        env!("CARGO_PKG_VERSION")
    )
}

// ---- helpers ----

fn require_arg(args: &[String], n: usize, label: &str) -> Result<PathBuf, CliError> {
    args.get(n)
        .map(PathBuf::from)
        .ok_or_else(|| CliError::Usage(format!("missing {label}")))
}

fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut iter = args.iter().enumerate();
    while let Some((_, a)) = iter.next() {
        if a == flag {
            return iter.next().map(|(_, v)| v.as_str());
        }
    }
    None
}

fn require_flag(args: &[String], flag: &str) -> Result<String, CliError> {
    find_flag(args, flag)
        .map(String::from)
        .ok_or_else(|| CliError::Usage(format!("missing {flag} <value>")))
}

fn output_path(args: &[String]) -> Result<PathBuf, CliError> {
    let s = find_flag(args, "-o")
        .or_else(|| find_flag(args, "--output"))
        .ok_or_else(|| CliError::Usage("missing -o <output>".into()))?;
    Ok(PathBuf::from(s))
}

fn parse_pages(s: &str) -> Result<Vec<u32>, CliError> {
    s.split(',')
        .map(|p| {
            p.trim()
                .parse::<u32>()
                .map_err(|_| CliError::Usage(format!("invalid page number: {p:?}")))
        })
        .collect()
}

fn parse_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

// ---- commands ----

fn cmd_info(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let i = info(&input)?;
    println!(
        "pages: {}\nversion: {}\nencrypted: {}\nfile_size: {} bytes",
        i.page_count, i.version, i.encrypted, i.size_bytes
    );
    Ok(())
}

fn cmd_page_count(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let n = page_count(&input)?;
    println!("{n}");
    Ok(())
}

fn cmd_merge(args: &[String]) -> Result<(), CliError> {
    if args.is_empty() {
        return Err(CliError::Usage(
            "usage: merge <in1> <in2> ... -o <out>".into(),
        ));
    }
    let output = output_path(args)?;
    // All positional args before -o / --output are inputs.
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-o" || a == "--output" {
            i += 2;
            continue;
        }
        inputs.push(PathBuf::from(a));
        i += 1;
    }
    if inputs.is_empty() {
        return Err(CliError::Usage("need at least one input file".into()));
    }
    merge_pdfs(&inputs, output.clone())?;
    println!("✓ wrote {}", output.display());
    Ok(())
}

fn cmd_split_every(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let n: u32 = args
        .get(1)
        .ok_or_else(|| CliError::Usage("missing <chunk-size>".into()))?
        .parse()
        .map_err(|_| CliError::Usage("chunk-size must be a number".into()))?;
    let out_dir = require_arg(args, 2, "<out-dir>")?;
    let files = split_every(&input, n, &out_dir)?;
    for f in &files {
        println!("{}", f.display());
    }
    Ok(())
}

fn cmd_split_ranges(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let spec = args
        .get(1)
        .ok_or_else(|| CliError::Usage("missing <ranges>".into()))?;
    let out_dir = require_arg(args, 2, "<out-dir>")?;
    let ranges: Result<Vec<PageRange>, CliError> = spec
        .split(',')
        .map(|piece| {
            let piece = piece.trim();
            let (s, e) = if let Some((a, b)) = piece.split_once('-') {
                (
                    a.parse::<u32>()
                        .map_err(|_| CliError::Usage(format!("bad range start: {a:?}")))?,
                    b.parse::<u32>()
                        .map_err(|_| CliError::Usage(format!("bad range end: {b:?}")))?,
                )
            } else {
                let v = piece
                    .parse::<u32>()
                    .map_err(|_| CliError::Usage(format!("bad page: {piece:?}")))?;
                (v, v)
            };
            PageRange::new(s, e).map_err(CliError::Op)
        })
        .collect();
    let ranges = ranges?;
    let files = split_by_ranges(&input, &ranges, &out_dir)?;
    for f in &files {
        println!("{}", f.display());
    }
    Ok(())
}

fn cmd_rotate(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let pages_csv = args
        .get(1)
        .ok_or_else(|| CliError::Usage("missing <pages>".into()))?;
    let deg: i64 = args
        .get(2)
        .ok_or_else(|| CliError::Usage("missing <degrees>".into()))?
        .parse()
        .map_err(|_| CliError::Usage("degrees must be a number".into()))?;
    let output = output_path(args)?;
    let pages = parse_pages(pages_csv)?;
    let rot = Rotation::from_int(deg)?;
    let n = rotate_pages(&input, &pages, rot, &output)?;
    println!("✓ rotated {n} page(s) → {}", output.display());
    Ok(())
}

fn cmd_delete_pages(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let pages_csv = args
        .get(1)
        .ok_or_else(|| CliError::Usage("missing <pages>".into()))?;
    let output = output_path(args)?;
    let pages = parse_pages(pages_csv)?;
    let n = delete_pages(&input, &pages, &output)?;
    println!("✓ deleted {n} page(s) → {}", output.display());
    Ok(())
}

fn cmd_compress(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let report = compress(&input, &output)?;
    println!(
        "✓ {} → {} bytes (ratio {:.2})",
        report.original_bytes, report.new_bytes, report.ratio
    );
    Ok(())
}

fn cmd_encrypt(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let pwd = require_flag(args, "--password")?;
    encrypt(&input, &output, &pwd)?;
    println!("✓ encrypted → {}", output.display());
    Ok(())
}

fn cmd_decrypt(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let pwd = require_flag(args, "--password")?;
    decrypt(&input, &output, &pwd)?;
    println!("✓ decrypted → {}", output.display());
    Ok(())
}

fn cmd_grayscale(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let pages = match find_flag(args, "--pages") {
        Some(s) => parse_pages(s)?,
        None => vec![],
    };
    let n = grayscale(&input, &output, GrayscaleOpts { pages })?;
    println!("✓ rewrote {n} stream(s) → {}", output.display());
    Ok(())
}

fn cmd_md2pdf(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<markdown-file>")?;
    let output = output_path(args)?;
    let page_size = find_flag(args, "--page-size").unwrap_or("A4").to_string();
    let md = std::fs::read_to_string(&input)
        .map_err(|e| CliError::Op(PdfError::Other(format!("read {}: {e}", input.display()))))?;
    let n = md2pdf_render(
        &md,
        &output,
        Md2PdfOpts {
            markdown: md.clone(),
            page_size,
        },
    )?;
    println!("✓ {n} page(s) → {}", output.display());
    Ok(())
}

fn cmd_polyglot(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let page_size = find_flag(args, "--page-size").unwrap_or("A4").to_string();
    let report = polyglot_to_pdf(&input, &output, PolyglotOpts { page_size })?;
    println!(
        "✓ {} ({} bytes md) → {} page(s) → {}",
        report.source_kind,
        report.markdown_bytes,
        report.pages,
        output.display()
    );
    Ok(())
}

fn cmd_flatten(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let include_widgets = !args.iter().any(|a| a == "--no-widgets");
    let raster = args.iter().any(|a| a == "--raster");
    let dpi: u32 = find_flag(args, "--dpi")
        .and_then(|s| s.parse().ok())
        .unwrap_or(150);
    let mode = if raster {
        FlattenMode::Raster { dpi }
    } else {
        FlattenMode::Annotations
    };
    let opts = FlattenOpts {
        include_widgets,
        mode,
    };
    let report = do_flatten(&input, &output, opts)?;
    let raster_note = if report.pages_rasterized > 0 {
        format!(
            ", {} page(s) rasterized @ {} DPI",
            report.pages_rasterized, report.dpi
        )
    } else {
        String::new()
    };
    println!(
        "✓ flattened {}/{} annotation(s) ({} dropped) across {} page(s){}{} → {}",
        report.annotations_flattened,
        report.annotations_in,
        report.annotations_dropped,
        report.pages_with_annotations,
        if report.had_acroform {
            ", AcroForm removed"
        } else {
            ""
        },
        raster_note,
        output.display()
    );
    Ok(())
}

fn cmd_sanitize(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let keep_links = args.iter().any(|a| a == "--keep-links");
    let opts = SanitizeOpts { keep_links };
    let report = do_sanitize(&input, &output, opts)?;
    println!(
        "✓ stripped: js={} launch={} uri={} embeds={} open-action={} aa-catalog={} aa-pages={} xfa={} → {}",
        report.js_removed,
        report.launch_removed,
        report.uri_removed,
        report.embedded_files_removed,
        report.open_action_removed,
        report.catalog_aa_removed,
        report.pages_aa_removed,
        report.xfa_removed,
        output.display()
    );
    Ok(())
}

fn cmd_repair(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let report = do_repair(&input, &output)?;
    let delta_bytes = report.bytes_before as i64 - report.bytes_after as i64;
    let pct = if report.bytes_before > 0 {
        (delta_bytes as f64 / report.bytes_before as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "✓ repaired: objects {} → {} ({} pruned), size {} → {} bytes ({:+.1}%) → {}",
        report.objects_before,
        report.objects_after,
        report.objects_pruned,
        report.bytes_before,
        report.bytes_after,
        -pct,
        output.display()
    );
    Ok(())
}

fn cmd_extract_text(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    if let Some(out) = find_flag(args, "-o").or_else(|| find_flag(args, "--output")) {
        let text = extract_text_concat(&input)?;
        std::fs::write(out, text)
            .map_err(|e| CliError::Op(PdfError::Other(format!("write {out}: {e}"))))?;
        println!("✓ saved → {out}");
    } else {
        let pages = extract_text(&input)?;
        for (i, p) in pages.iter().enumerate() {
            println!("--- page {} ---\n{}", i + 1, p);
        }
    }
    Ok(())
}

fn cmd_auto_redact(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let presets = find_flag(args, "--presets")
        .map(parse_csv)
        .unwrap_or_default();
    // Allow repeated --patterns 'r1' --patterns 'r2'.
    let mut patterns: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--patterns" {
            if let Some(v) = args.get(i + 1) {
                patterns.push(v.clone());
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    let opts = AutoRedactOpts {
        patterns,
        presets,
        gray: 0.0,
    };
    let n = auto_redact(&input, &output, opts)?;
    println!("✓ {n} match(es) redacted → {}", output.display());
    Ok(())
}

fn cmd_read_metadata(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let m = read_metadata(&input)?;
    print_meta_field("Title", &m.title);
    print_meta_field("Author", &m.author);
    print_meta_field("Subject", &m.subject);
    print_meta_field("Keywords", &m.keywords);
    print_meta_field("Creator", &m.creator);
    print_meta_field("Producer", &m.producer);
    Ok(())
}

fn print_meta_field(label: &str, value: &Option<String>) {
    match value {
        Some(v) => println!("{label}: {v}"),
        None => println!("{label}: <unset>"),
    }
}

fn cmd_strip_metadata(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    strip_metadata(&input, &output)?;
    println!("✓ stripped → {}", output.display());
    Ok(())
}

fn cmd_ocr(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let lang = find_flag(args, "--lang").unwrap_or("eng").to_string();
    let dpi: u32 = find_flag(args, "--dpi")
        .map(|s| s.parse().unwrap_or(300))
        .unwrap_or(300);
    let report = ocr(&input, &output, &OcrOpts { lang, dpi })?;
    println!(
        "✓ OCR'd {} page(s) [{} @ {} DPI] → {}",
        report.pages,
        report.lang,
        report.dpi,
        output.display()
    );
    Ok(())
}

fn cmd_outline(args: &[String]) -> Result<(), CliError> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match sub {
        "read" => {
            let input = require_arg(rest, 0, "<file>")?;
            let tree = read_outline(&input)?;
            let json = serde_json::to_string_pretty(&tree)
                .map_err(|e| CliError::Op(PdfError::Other(format!("json: {e}"))))?;
            println!("{json}");
            Ok(())
        }
        "write" => {
            let input = require_arg(rest, 0, "<file>")?;
            let output = output_path(rest)?;
            let json_path = find_flag(rest, "--json").ok_or_else(|| {
                CliError::Usage("outline write needs --json <outline.json>".into())
            })?;
            let json = std::fs::read_to_string(json_path)
                .map_err(|e| CliError::Op(PdfError::Other(format!("read json: {e}"))))?;
            let nodes: Vec<OutlineNode> = serde_json::from_str(&json)
                .map_err(|e| CliError::Op(PdfError::Other(format!("parse json: {e}"))))?;
            write_outline(&input, &output, &nodes)?;
            println!("✓ outline written → {}", output.display());
            Ok(())
        }
        _ => Err(CliError::Usage(
            "outline subcommand: `read <file>` or `write <file> -o <out> --json <outline.json>`"
                .into(),
        )),
    }
}

// Suppress unused warnings on Path.
#[allow(dead_code)]
fn _force_path_use(_p: &Path) {}

fn cmd_export_annots(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let annots = extract_annots(&input)?;
    let label = input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document.pdf")
        .to_string();
    let md = annots_to_md(&label, &annots);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::Op(PdfError::Other(format!("create output dir: {e}"))))?;
    }
    std::fs::write(&output, md)
        .map_err(|e| CliError::Op(PdfError::Other(format!("write markdown: {e}"))))?;
    println!(
        "✓ {} annotation(s) exported → {}",
        annots.len(),
        output.display()
    );
    Ok(())
}

// ---- v2.2 batch: bates / invert / reverse / booklet ----

fn cmd_bates(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let prefix = find_flag(args, "--prefix").unwrap_or("").to_string();
    let start_at: u64 = find_flag(args, "--start")
        .unwrap_or("1")
        .parse()
        .map_err(|e| CliError::Usage(format!("--start must be a positive integer: {e}")))?;
    let digits: u8 = find_flag(args, "--digits")
        .unwrap_or("6")
        .parse()
        .map_err(|e| CliError::Usage(format!("--digits must be 1..=12: {e}")))?;
    let font_size: f32 = find_flag(args, "--font-size")
        .unwrap_or("10")
        .parse()
        .map_err(|e| CliError::Usage(format!("--font-size must be a number: {e}")))?;
    let position = match find_flag(args, "--position").unwrap_or("bottom-right") {
        "top-left" => BatesPosition::TopLeft,
        "top-center" => BatesPosition::TopCenter,
        "top-right" => BatesPosition::TopRight,
        "bottom-left" => BatesPosition::BottomLeft,
        "bottom-center" => BatesPosition::BottomCenter,
        "bottom-right" => BatesPosition::BottomRight,
        other => {
            return Err(CliError::Usage(format!(
                "unknown --position {other:?} (use top-left, top-center, top-right, bottom-left, bottom-center, bottom-right)"
            )))
        }
    };
    let opts = BatesOpts {
        prefix,
        start_at,
        digits,
        position,
        font_size,
        gray: 0.0,
    };
    let report = apply_bates(&input, &output, &opts)?;
    println!(
        "✓ stamped {} page(s): {} … {} (next: {}) → {}",
        report.pages_stamped,
        report.first_label,
        report.last_label,
        report.next_start,
        output.display()
    );
    Ok(())
}

fn cmd_invert(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let pages = match find_flag(args, "--pages") {
        Some(s) => parse_pages(s)?,
        None => vec![],
    };
    let n = invert_colors(&input, &output, InvertOpts { pages })?;
    println!("✓ inverted {n} stream(s) → {}", output.display());
    Ok(())
}

fn cmd_reverse(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let n = reverse_pages(&input, &output)?;
    println!("✓ reversed {n} page(s) → {}", output.display());
    Ok(())
}

fn cmd_booklet(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let output = output_path(args)?;
    let margin: f32 = find_flag(args, "--margin")
        .unwrap_or("18")
        .parse()
        .map_err(|e| CliError::Usage(format!("--margin must be a number: {e}")))?;
    let gap: f32 = find_flag(args, "--gap")
        .unwrap_or("12")
        .parse()
        .map_err(|e| CliError::Usage(format!("--gap must be a number: {e}")))?;
    let opts = BookletOpts { margin, gap };
    let r = impose_booklet(&input, &output, opts)?;
    println!(
        "✓ booklet: {} source page(s) → {} signature page(s) ({} sheet(s), {} output page(s)) → {}",
        r.source_pages,
        r.signature_pages,
        r.sheets,
        r.output_pages,
        output.display()
    );
    Ok(())
}

// ---- lens subcommands (v0.13.0) ----

fn cmd_lens(args: &[String]) -> Result<(), CliError> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match sub {
        "audit" => cmd_lens_audit(rest),
        "tables" => cmd_lens_tables(rest),
        "ocr-queue" => cmd_lens_ocr_queue(rest),
        "auto-tag" => cmd_lens_auto_tag(rest),
        "preflight" => cmd_lens_preflight(rest),
        "" => Err(CliError::Usage(
            "lens needs a subcommand: audit | tables | ocr-queue | auto-tag | preflight".into(),
        )),
        other => Err(CliError::Usage(format!(
            "unknown lens subcommand: {other}\n\
             try: audit | tables | ocr-queue | auto-tag | preflight"
        ))),
    }
}

fn classification_label(c: PageClassification) -> &'static str {
    match c {
        PageClassification::Text => "text",
        PageClassification::Image => "image",
        PageClassification::Mixed => "mixed",
        PageClassification::Empty => "empty",
    }
}

fn recommendation_label(r: Recommendation) -> (&'static str, &'static str) {
    match r {
        Recommendation::OcrAll => (
            "ocr_all",
            "run `slab ocr <file> -o <out>` — every page is scanned",
        ),
        Recommendation::OcrSome => ("ocr_some", "consider OCR — some pages are scanned"),
        Recommendation::None => ("none", "nothing to do — fully text-native"),
    }
}

fn cmd_lens_audit(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let report = scan_audit(&input).map_err(CliError::Op)?;
    println!("pages: {}", report.total());
    for (i, p) in report.pages.iter().enumerate() {
        println!("  {}: {}", i + 1, classification_label(*p));
    }
    println!(
        "text={} image={} mixed={} empty={}",
        report.text_pages, report.image_pages, report.mixed_pages, report.empty_pages,
    );
    let (label, hint) = recommendation_label(report.recommended_action);
    println!("recommended: {label} — {hint}");
    Ok(())
}

fn parse_u32_flag(args: &[String], flag: &str, default: u32) -> Result<u32, CliError> {
    match find_flag(args, flag) {
        Some(s) => s
            .parse::<u32>()
            .map_err(|_| CliError::Usage(format!("{flag} must be a positive integer"))),
        None => Ok(default),
    }
}

fn cmd_lens_tables(args: &[String]) -> Result<(), CliError> {
    let input = require_arg(args, 0, "<file>")?;
    let page = args
        .get(1)
        .ok_or_else(|| CliError::Usage("missing <page>".into()))?
        .parse::<u32>()
        .map_err(|_| CliError::Usage("page must be a positive integer".into()))?;
    let min_rows = parse_u32_flag(args, "--min-rows", 2)?;
    let min_cols = parse_u32_flag(args, "--min-cols", 2)?;
    let opts = TableOpts {
        page,
        min_rows,
        min_cols,
    };
    let tables = extract_tables(&input, &opts).map_err(CliError::Op)?;
    if tables.is_empty() {
        println!("no tables found on page {page} (min_rows={min_rows}, min_cols={min_cols})");
        return Ok(());
    }
    let out_flag = find_flag(args, "-o")
        .or_else(|| find_flag(args, "--output"))
        .map(PathBuf::from);
    match out_flag {
        None => {
            for (i, t) in tables.iter().enumerate() {
                println!("--- table {} ({}×{}) ---", i + 1, t.row_count(), t.columns);
                print!("{}", table_to_csv(t));
            }
        }
        Some(out) if tables.len() == 1 => {
            std::fs::write(&out, table_to_csv(&tables[0]))
                .map_err(|e| CliError::Op(PdfError::Other(format!("write csv: {e}"))))?;
            println!(
                "✓ 1 table ({}×{}) → {}",
                tables[0].row_count(),
                tables[0].columns,
                out.display()
            );
        }
        Some(out) => {
            // ≥2 tables — write <stem>_t<N>.csv siblings.
            let stem = out
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("table")
                .to_string();
            let parent = out
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            for (i, t) in tables.iter().enumerate() {
                let p = parent.join(format!("{stem}_t{}.csv", i + 1));
                std::fs::write(&p, table_to_csv(t))
                    .map_err(|e| CliError::Op(PdfError::Other(format!("write csv: {e}"))))?;
                println!(
                    "✓ table {} ({}×{}) → {}",
                    i + 1,
                    t.row_count(),
                    t.columns,
                    p.display()
                );
            }
        }
    }
    Ok(())
}

fn open_default_library() -> Result<LibraryDb, CliError> {
    LibraryDb::open(&library_db_path()).map_err(|e| {
        CliError::Op(PdfError::Other(format!(
            "open library DB at {}: {e}",
            library_db_path().display()
        )))
    })
}

fn cmd_lens_ocr_queue(args: &[String]) -> Result<(), CliError> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match sub {
        "list" => cmd_lens_ocr_queue_list(rest),
        "run" => cmd_lens_ocr_queue_run(rest),
        "run-all" => cmd_lens_ocr_queue_run_all(rest),
        "" => Err(CliError::Usage(
            "ocr-queue needs a subcommand: list | run <id> | run-all".into(),
        )),
        other => Err(CliError::Usage(format!(
            "unknown ocr-queue subcommand: {other}\n\
             try: list | run <id> | run-all"
        ))),
    }
}

fn cmd_lens_ocr_queue_list(_args: &[String]) -> Result<(), CliError> {
    let db = open_default_library()?;
    let pending = ocr_queue_list_pending(&db)
        .map_err(|e| CliError::Op(PdfError::Other(format!("list ocr queue: {e}"))))?;
    if pending.is_empty() {
        println!("no docs pending OCR.");
        return Ok(());
    }
    println!("{} doc(s) pending OCR:", pending.len());
    for d in &pending {
        println!(
            "  [{}] {} ({}, {} page(s))",
            d.id,
            d.path,
            d.ocr_state,
            d.pages.unwrap_or(0),
        );
    }
    Ok(())
}

fn parse_ocr_opts(args: &[String]) -> OcrOpts {
    let lang = find_flag(args, "--lang").unwrap_or("eng").to_string();
    let dpi: u32 = find_flag(args, "--dpi")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    OcrOpts { lang, dpi }
}

fn cmd_lens_ocr_queue_run(args: &[String]) -> Result<(), CliError> {
    let doc_id: i64 = args
        .first()
        .ok_or_else(|| CliError::Usage("missing <doc-id>".into()))?
        .parse()
        .map_err(|_| CliError::Usage("<doc-id> must be an integer".into()))?;
    let opts = parse_ocr_opts(args);
    let mut db = open_default_library()?;
    let r = ocr_queue_run_one(&mut db, doc_id, &opts);
    if let Some(err) = &r.error {
        eprintln!("✗ doc {doc_id}: {err}");
        return Err(CliError::Op(PdfError::Other(err.clone())));
    }
    println!(
        "✓ doc {} → {} (state: {})",
        r.doc_id,
        r.output_path.as_deref().unwrap_or("?"),
        r.state_after,
    );
    Ok(())
}

fn cmd_lens_ocr_queue_run_all(args: &[String]) -> Result<(), CliError> {
    let opts = parse_ocr_opts(args);
    let mut db = open_default_library()?;
    let pending = ocr_queue_list_pending(&db)
        .map_err(|e| CliError::Op(PdfError::Other(format!("list ocr queue: {e}"))))?;
    if pending.is_empty() {
        println!("no docs pending OCR.");
        return Ok(());
    }
    println!("running OCR on {} doc(s)…", pending.len());
    let mut ok = 0usize;
    let mut fail = 0usize;
    for d in pending {
        let r = ocr_queue_run_one(&mut db, d.id, &opts);
        if let Some(err) = &r.error {
            eprintln!("✗ doc {} ({}): {err}", d.id, d.path);
            fail += 1;
        } else {
            println!(
                "✓ doc {} ({}) → {}",
                r.doc_id,
                d.path,
                r.output_path.as_deref().unwrap_or("?"),
            );
            ok += 1;
        }
    }
    println!("{ok} succeeded, {fail} failed");
    if fail > 0 && ok == 0 {
        return Err(CliError::Op(PdfError::Other("all OCR jobs failed".into())));
    }
    Ok(())
}

fn cmd_lens_auto_tag(args: &[String]) -> Result<(), CliError> {
    let max_tags = parse_u32_flag(args, "--max-tags", 5)?;
    let opts = AutoTagOpts {
        max_tags,
        ..Default::default()
    };

    let all = args.iter().any(|a| a == "--all");
    let cfg = load_ai_config()
        .map_err(|e| CliError::Op(PdfError::Other(format!("load ai config: {e}"))))?;
    let provider = make_provider(&cfg.beacon)
        .map_err(|e| CliError::Op(PdfError::Other(format!("build ai provider: {e}"))))?;

    let mut db = open_default_library()?;

    // Resolve target doc id(s).
    let doc_ids: Vec<i64> = if all {
        let docs = query_documents(&db, &LibraryFilter::default())
            .map_err(|e| CliError::Op(PdfError::Other(format!("list docs: {e}"))))?;
        if docs.is_empty() {
            println!("library is empty — nothing to tag.");
            return Ok(());
        }
        docs.iter().map(|d| d.id).collect()
    } else {
        let first = args
            .iter()
            .find(|a| !a.starts_with("--"))
            .ok_or_else(|| CliError::Usage("missing <doc-id> (or pass --all)".into()))?;
        let id: i64 = first
            .parse()
            .map_err(|_| CliError::Usage("<doc-id> must be an integer".into()))?;
        vec![id]
    };

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| CliError::Op(PdfError::Other(format!("start tokio runtime: {e}"))))?;

    let mut ok = 0usize;
    let mut fail = 0usize;
    for id in &doc_ids {
        let provider = provider.clone();
        let r = rt.block_on(auto_tag_run_one(&mut db, provider, *id, &opts));
        if let Some(err) = &r.error {
            eprintln!("✗ doc {id}: {err}");
            fail += 1;
        } else {
            let tags = if r.tags_assigned.is_empty() {
                "<none>".to_string()
            } else {
                r.tags_assigned.join(", ")
            };
            println!("✓ doc {id}: {tags}");
            ok += 1;
        }
    }
    if doc_ids.len() > 1 {
        println!("{ok} succeeded, {fail} failed");
    }
    if fail > 0 && ok == 0 {
        return Err(CliError::Op(PdfError::Other(
            "all auto-tag jobs failed".into(),
        )));
    }
    Ok(())
}

fn cmd_lens_preflight(args: &[String]) -> Result<(), CliError> {
    // Flags:
    //   --json           emit JSON instead of human-readable
    //   --ollama <url>   override the ollama URL (default
    //                    http://localhost:11434 — pass empty to skip)
    let want_json = args.iter().any(|a| a == "--json");
    let mut opts = PreflightOpts::default();
    if let Some(pos) = args.iter().position(|a| a == "--ollama") {
        let val = args
            .get(pos + 1)
            .ok_or_else(|| CliError::Usage("--ollama needs a value".into()))?;
        opts.ollama_url = if val.is_empty() {
            None
        } else {
            Some(val.clone())
        };
    }
    let report = preflight(&opts);
    if want_json {
        let s = serde_json::to_string_pretty(&report)
            .map_err(|e| CliError::Op(PdfError::Other(format!("serialize report: {e}"))))?;
        println!("{s}");
    } else {
        println!("Slab Lens dependency preflight\n");
        for c in &report.checks {
            let (icon, body) = match &c.status {
                PreflightStatus::Ok { detail } => ("✓", detail.as_str()),
                PreflightStatus::Wrong { detail } => ("✗", detail.as_str()),
                PreflightStatus::Missing { hint } => ("·", hint.as_str()),
            };
            println!("  {icon} {label}", label = c.label);
            println!("    features: {f}", f = c.features);
            println!("    {body}");
            println!();
        }
        println!("{} / {} checks OK", report.ok, report.total);
    }
    if !report.all_ok() {
        // Non-zero exit so this is scriptable.
        return Err(CliError::Op(PdfError::Other(format!(
            "{} / {} preflight checks failed",
            report.total - report.ok,
            report.total
        ))));
    }
    Ok(())
}
