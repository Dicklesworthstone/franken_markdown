//! Coverage-guided fuzz target: parse + HTML + PDF (m7fs.1).
#![no_main]

use franken_markdown::{
    HtmlOptions, PdfOptions, parse_markdown, render_html_document, render_pdf_document,
};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 4096;

fuzz_target!(|data: &[u8]| {
    let slice = if data.len() > MAX_INPUT {
        &data[..MAX_INPUT]
    } else {
        data
    };
    let src = String::from_utf8_lossy(slice);
    let doc = parse_markdown(&src);
    let _ = render_html_document(&doc, &HtmlOptions::default());
    let _ = render_pdf_document(&doc, &PdfOptions::default());
});
