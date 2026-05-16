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

use slab_lib::pdf::auto_redact::{auto_redact, AutoRedactOpts};
use slab_lib::pdf::compress::compress;
use slab_lib::pdf::encrypt::{decrypt, encrypt};
use slab_lib::pdf::extract::{extract_text, extract_text_concat};
use slab_lib::pdf::grayscale::{grayscale, GrayscaleOpts};
use slab_lib::pdf::info::info;
use slab_lib::pdf::md2pdf::{render as md2pdf_render, Md2PdfOpts};
use slab_lib::pdf::merge::merge_pdfs;
use slab_lib::pdf::metadata::{read_metadata, strip_metadata};
use slab_lib::pdf::ocr::{ocr, OcrOpts};
use slab_lib::pdf::outline::{read_outline, write_outline, OutlineNode};
use slab_lib::pdf::pages::{delete_pages, rotate_pages, Rotation};
use slab_lib::pdf::split::{page_count, split_by_ranges, split_every, PageRange};
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
