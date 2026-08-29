//! Plass-style optimal pagination — public-contract tests.
//!
//! Default (off) is byte-identical to the greedy path; opt-in is
//! deterministic, produces a valid PDF, and never changes the page count on
//! fixtures without forced-break edge cases beyond sanity bounds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfOptions, parse_markdown, render_pdf_document};

const BODY: &str = "The document-wide pagination dynamic program minimizes the total of the same void badness and keep penalties the greedy breaker applies page by page. Pagination quality shows most on documents with headings, tables, and code blocks interleaved with body text, because those carry keep-with-next constraints whose consequences span page boundaries. A greedy breaker fills each page in isolation and only sees the next page when it gets there, while the optimal breaker trades a little extra void on one page for keeping a block whole on the next.\n\n## Section\n\nMore body text follows the heading so the keep-with-next rule binds the heading to at least its first content line. Tables follow.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n\n```text\na fenced code block\nwith a few short lines\nthat prefer to stay together\n```\n\nTrailing body text rounds out the document so pagination has real choices to make across several pages of content, repeated once more below to push past a single page of material.\n";

fn doc() -> franken_markdown::Document {
    parse_markdown(&format!("# Title\n\n{BODY}{BODY}{BODY}"))
}

fn count_pages(pdf: &[u8]) -> usize {
    pdf.windows(b"/Type /Page ".len())
        .filter(|w| *w == b"/Type /Page ")
        .count()
}

#[test]
fn default_off_is_deterministic() {
    let a = render_pdf_document(&doc(), &PdfOptions::default()).expect("a");
    let b = render_pdf_document(&doc(), &PdfOptions::default()).expect("b");
    assert_eq!(a, b);
    assert!(a.starts_with(b"%PDF-"));
    assert!(count_pages(&a) >= 2, "fixture must span multiple pages");
}

#[test]
fn optimal_pagination_deterministic_and_valid() {
    let opts = || PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    let a = render_pdf_document(&doc(), &opts()).expect("a");
    let b = render_pdf_document(&doc(), &opts()).expect("b");
    assert_eq!(a, b, "opt-in render deterministic");
    assert!(a.starts_with(b"%PDF-"), "still a valid PDF");
    // The DP minimizes cost, not page count: legitimately adding pages to
    // honor keep-penalties is the feature (each dodged keep is 0.65-1.5M
    // demerits vs ~60k for an extra page). The bound here only rejects
    // runaway growth (alpha mis-calibration), not the trade itself.
    let greedy = render_pdf_document(&doc(), &PdfOptions::default()).expect("greedy");
    let (gp, op) = (count_pages(&greedy), count_pages(&a));
    assert!(
        op <= gp + (gp / 4).max(2),
        "optimal pagination page count {op} within runaway bound of greedy {gp}"
    );
}
