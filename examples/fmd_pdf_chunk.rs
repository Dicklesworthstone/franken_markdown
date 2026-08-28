//! Synthetic N-page Markdown -> PDF for bead u9jt.2 (chunked vs monolithic).
//!
//! Stdout is one JSON object. Diagnostics go to stderr. `--max-heap-mb` is the
//! writer-owned retained-byte ceiling; RSS is measured by the wrapping script.

use franken_markdown::{
    PdfEmitOptions, PdfOptions, PdfPageEmission, parse_markdown, render_pdf_document_emitted,
};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const BEAD_ID: &str = "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-u9jt.2";
const FILLER: &str = "deterministic typography optimization representation hyphenation pagination markdown rendering ligature kerning paragraph document ";

struct Args {
    pages: usize,
    lines: usize,
    width: usize,
    emission: PdfPageEmission,
    max_heap_mb: Option<usize>,
    dump: Option<PathBuf>,
    out: Option<PathBuf>,
    verbose: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(io::stderr(), "fmd_pdf_chunk: {err}");
            ExitCode::from(70)
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    log_phase(
        "generate",
        args.pages,
        &format!(
            "lines={} width={} emission={:?}",
            args.lines, args.width, args.emission
        ),
    );
    let source = generate_pages(args.pages, args.lines, args.width);
    if let Some(path) = &args.dump {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        fs::write(path, source.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
        log_phase("dump", source.len(), &format!("path={}", path.display()));
    }

    let opts = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        title: Some("u9jt.2".into()),
        author: Some("FMD".into()),
        ..PdfOptions::default()
    };
    let emit = PdfEmitOptions {
        emission: args.emission,
        max_retained_bytes: args.max_heap_mb.map(|mb| mb.saturating_mul(1024 * 1024)),
        verbose: args.verbose,
    };
    let started = Instant::now();
    log_phase("parse+pdf", source.len(), "render_pdf_document_emitted");
    let doc = parse_markdown(&source);
    let bytes = render_pdf_document_emitted(&doc, &opts, emit).map_err(|e| e.to_string())?;
    let wall_ns = started.elapsed().as_nanos();
    if let Some(path) = &args.out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        fs::write(path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
        log_phase("write", bytes.len(), &format!("path={}", path.display()));
    }

    emit_line(&json_object(&[
        ("type", json_str("pdf_chunk_sample")),
        ("bead_id", json_str(BEAD_ID)),
        ("requested_pages", json_u128(args.pages as u128)),
        ("lines_per_section", json_u128(args.lines as u128)),
        ("width", json_u128(args.width as u128)),
        (
            "emission",
            json_str(match args.emission {
                PdfPageEmission::Chunked => "chunked",
                PdfPageEmission::Monolithic => "monolithic",
            }),
        ),
        (
            "max_heap_mb",
            args.max_heap_mb
                .map_or_else(|| "null".to_string(), |n| json_u128(n as u128)),
        ),
        ("source_bytes", json_u128(source.len() as u128)),
        ("pdf_bytes", json_u128(bytes.len() as u128)),
        ("wall_ns", json_u128(wall_ns)),
    ]))?;
    log_phase(
        "done",
        bytes.len(),
        &format!("wall_ms={}", wall_ns / 1_000_000),
    );
    Ok(())
}

fn generate_pages(pages: usize, lines: usize, width: usize) -> String {
    let line = padded_line(width);
    let mut para = String::with_capacity(lines.saturating_mul(width.saturating_add(1)));
    for i in 0..lines {
        para.push_str(&line);
        if i + 1 < lines {
            para.push(' ');
        }
    }
    let mut out = String::with_capacity(pages.saturating_mul(para.len().saturating_add(24)));
    for i in 0..pages {
        out.push_str("## S");
        out.push_str(&i.to_string());
        out.push_str("\n\n");
        out.push_str(&para);
        out.push_str("\n\n");
    }
    out
}

fn padded_line(width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut line = String::with_capacity(width);
    while line.len() < width {
        line.push_str(FILLER);
    }
    line.truncate(width);
    line
}

fn parse_args<I>(mut args: I) -> Result<Args, String>
where
    I: Iterator<Item = String>,
{
    let mut pages = 1000usize;
    let mut lines = 48usize;
    let mut width = 72usize;
    let mut emission = PdfPageEmission::Chunked;
    let mut max_heap_mb = None;
    let mut dump = None;
    let mut out = None;
    let mut verbose = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pages" => pages = parse_positive(&next_val(&mut args, "--pages")?)?,
            "--lines" => lines = parse_positive(&next_val(&mut args, "--lines")?)?,
            "--width" => width = parse_positive(&next_val(&mut args, "--width")?)?,
            "--emission" => {
                emission = match next_val(&mut args, "--emission")?.as_str() {
                    "chunked" => PdfPageEmission::Chunked,
                    "monolithic" => PdfPageEmission::Monolithic,
                    other => {
                        return Err(format!(
                            "--emission must be chunked|monolithic, got {other}"
                        ));
                    }
                };
            }
            "--max-heap-mb" => {
                max_heap_mb = Some(parse_positive(&next_val(&mut args, "--max-heap-mb")?)?)
            }
            "--dump" => dump = Some(PathBuf::from(next_val(&mut args, "--dump")?)),
            "--out" => out = Some(PathBuf::from(next_val(&mut args, "--out")?)),
            "--verbose" => verbose = true,
            "--help" | "-h" => {
                println!(
                    "Usage: fmd_pdf_chunk [--pages N] [--lines N] [--width N] [--emission chunked|monolithic] [--max-heap-mb N] [--dump PATH] [--out PATH] [--verbose]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    Ok(Args {
        pages,
        lines,
        width,
        emission,
        max_heap_mb,
        dump,
        out,
        verbose,
    })
}

fn next_val<I: Iterator<Item = String>>(args: &mut I, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive(raw: &str) -> Result<usize, String> {
    let n: usize = raw
        .parse()
        .map_err(|_| format!("expected positive integer, got '{raw}'"))?;
    if n == 0 {
        Err(String::from("counts must be greater than zero"))
    } else {
        Ok(n)
    }
}

fn emit_line(line: &str) -> Result<(), String> {
    let mut out = io::stdout().lock();
    out.write_all(line.as_bytes())
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
        .map_err(|e| format!("stdout write failed: {e}"))
}

fn log_phase(phase: &str, items: usize, detail: &str) {
    let _ = writeln!(
        io::stderr(),
        "fmd_pdf_chunk: phase={phase} items={items} {detail}"
    );
}

fn json_str(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_u128(value: u128) -> String {
    value.to_string()
}

fn json_object(fields: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_str(k));
        out.push(':');
        out.push_str(v);
    }
    out.push('}');
    out
}
