//! Linux memory probe for the current full-buffer and writer HTML APIs.
//! Run each mode in a fresh process with the same Markdown input. This is a
//! comparison of output buffering, not a release-speed benchmark or a claim
//! that parsing and individual blocks use constant memory.
#![forbid(unsafe_code)]

use franken_markdown::{
    HtmlOptions, parse_markdown, render_html_document, render_html_document_to_writer,
};
use std::io::{self, Write};
use std::time::Instant;

fn status(field: &str) -> io::Result<usize> {
    std::fs::read_to_string("/proc/self/status")?
        .lines()
        .find(|line| line.starts_with(field))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| io::Error::other(format!("missing Linux process counter {field}")))
}

struct FingerprintSink {
    bytes: usize,
    hash: u64,
}

impl Write for FingerprintSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // A stable FNV-1a output fingerprint, independent of write boundaries.
        // Exact fixture byte comparisons remain the correctness test oracle.
        for byte in bytes {
            self.hash = (self.hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || !matches!(args[1].as_str(), "string" | "stream") {
        return Err("usage: fmd_html_stream_memory <string|stream> INPUT.md".into());
    }
    let source = std::fs::read_to_string(&args[2])?;
    let doc = parse_markdown(&source);
    let opts = HtmlOptions {
        custom_css: Some("body { color: #222; }".into()),
        ..HtmlOptions::default()
    };
    let rss_before = status("VmRSS:")?;
    let peak_before = status("VmHWM:")?;
    let mut sink = FingerprintSink {
        bytes: 0,
        hash: 14_695_981_039_346_656_037,
    };
    let start = Instant::now();
    let count = if args[1] == "string" {
        let html = render_html_document(&doc, &opts)?;
        sink.write_all(std::hint::black_box(html.as_bytes()))?;
        std::hint::black_box(&html);
        html.len()
    } else {
        render_html_document_to_writer(&doc, &opts, &mut sink)?
    };
    if count != sink.bytes {
        return Err("reported output length differs from delivered bytes".into());
    }
    println!(
        "{{\"mode\":\"{}\",\"input_bytes\":{},\"output_bytes\":{},\"blocks\":{},\"fnv1a64\":\"{:016x}\",\"render_us\":{},\"rss_before_kib\":{},\"peak_before_kib\":{},\"peak_after_kib\":{}}}",
        args[1],
        source.len(),
        count,
        doc.blocks.len(),
        sink.hash,
        start.elapsed().as_micros(),
        rss_before,
        peak_before,
        status("VmHWM:")?
    );
    std::hint::black_box((source, doc));
    Ok(())
}
