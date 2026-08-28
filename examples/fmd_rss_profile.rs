//! Synthetic N-page Markdown -> PDF for the streaming RSS gate (bead u9jt.1).
//!
//! Stdout is one JSON object. Diagnostics go to stderr. Peak RSS is measured
//! by the wrapping script via `/usr/bin/time` so this example stays std-only.

use franken_markdown::{PdfOptions, parse_markdown, render_pdf_document_profiled};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const BEAD_ID: &str = "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-u9jt.1";
const FILLER: &str = "deterministic typography optimization representation hyphenation pagination markdown rendering ligature kerning paragraph document ";

struct Args {
    pages: usize,
    lines: usize,
    width: usize,
    dump: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(io::stderr(), "fmd_rss_profile: {err}");
            ExitCode::from(70)
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    log_phase(
        "generate",
        args.pages,
        &format!("lines={} width={}", args.lines, args.width),
    );
    let source = generate_pages(args.pages, args.lines, args.width);
    if let Some(path) = &args.dump {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        fs::write(path, source.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
        log_phase("dump", source.len(), &format!("path={}", path.display()));
    }

    let opts = PdfOptions::default();
    let started = Instant::now();
    log_phase("parse+pdf", source.len(), "render_pdf_document_profiled");
    let doc = parse_markdown(&source);
    let profile = render_pdf_document_profiled(&doc, &opts).map_err(|e| e.to_string())?;
    let wall_ns = started.elapsed().as_nanos();
    let page_count = stage_count(&profile.stages, "page_content_stream_generation")
        .or_else(|| stage_count(&profile.stages, "pagination"));
    let layout_ns = stage_ns(&profile.stages, "layout");
    let pagination_ns = stage_ns(&profile.stages, "pagination");

    emit_line(&json_object(&[
        ("type", json_str("rss_render_sample")),
        ("bead_id", json_str(BEAD_ID)),
        ("requested_pages", json_u128(args.pages as u128)),
        ("lines_per_section", json_u128(args.lines as u128)),
        ("width", json_u128(args.width as u128)),
        ("source_bytes", json_u128(source.len() as u128)),
        ("pdf_bytes", json_u128(profile.bytes.len() as u128)),
        (
            "observed_pages",
            page_count.map_or_else(|| "null".to_string(), json_u128),
        ),
        ("wall_ns", json_u128(wall_ns)),
        (
            "layout_ns",
            layout_ns.map_or_else(|| "null".to_string(), json_u128),
        ),
        (
            "pagination_ns",
            pagination_ns.map_or_else(|| "null".to_string(), json_u128),
        ),
    ]))?;
    log_phase(
        "done",
        profile.bytes.len(),
        &format!(
            "observed_pages={} wall_ms={}",
            page_count.unwrap_or(0),
            wall_ns / 1_000_000
        ),
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

fn stage_count(stages: &[franken_markdown::PdfStageSummary], name: &str) -> Option<u128> {
    stages
        .iter()
        .find(|s| s.stage == name)
        .map(|s| s.count as u128)
}

fn stage_ns(stages: &[franken_markdown::PdfStageSummary], name: &str) -> Option<u128> {
    stages
        .iter()
        .find(|s| s.stage == name)
        .map(|s| s.elapsed_ns)
}

fn parse_args<I>(mut args: I) -> Result<Args, String>
where
    I: Iterator<Item = String>,
{
    let mut pages = 1000usize;
    let mut lines = 48usize;
    let mut width = 72usize;
    let mut dump = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pages" => pages = parse_positive(&next_val(&mut args, "--pages")?)?,
            "--lines" => lines = parse_positive(&next_val(&mut args, "--lines")?)?,
            "--width" => width = parse_positive(&next_val(&mut args, "--width")?)?,
            "--dump" => dump = Some(PathBuf::from(next_val(&mut args, "--dump")?)),
            "--help" | "-h" => {
                println!(
                    "Usage: fmd_rss_profile [--pages N] [--lines N] [--width N] [--dump PATH]"
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
        dump,
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
        "fmd_rss_profile: phase={phase} items={items} {detail}"
    );
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn json_str(value: &str) -> String {
    let mut out = String::from("\"");
    out.push_str(&json_escape(value));
    out.push('"');
    out
}

fn json_u128(value: u128) -> String {
    value.to_string()
}

fn json_object(fields: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(key);
        out.push_str("\":");
        out.push_str(value);
    }
    out.push('}');
    out
}
