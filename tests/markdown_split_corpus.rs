//! FCB-022 / fcb-9vx.23 qualification: the Markdown route through the
//! shared resumable lexical engine, adversarially split.
//!
//! Findings (this suite is both the qualification and the characterization):
//!
//! - **Line-boundary splits are split-equivalent** on every fixture: a
//!   chunk boundary immediately after a newline produces byte-identical
//!   coalesced spans to whole-input classification.
//! - **Mid-line splits are also split-equivalent** on the probed heading
//!   fixture across EVERY byte position: the engine re-lexes its entire
//!   pending buffer wholesale on each feed, so the resumed classification
//!   re-derives the whole-run result even though `lex_markdown_into`
//!   restarts its internal `line_start` flag. (Empirically verified: the
//!   earlier predicted mid-line divergence does not manifest.)
//! - The registry capability row stays `Provisional` until the V-layer's
//!   production verification runs; per-line constructs (headings, list
//!   markers, blockquotes, inline code, emphasis, HTML comments, setext
//!   underlines) are already truthfully classified.
//! - Post-finish feeds are refused (`AlreadyFinished`) — the resumable
//!   seam does not silently accept late chunks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use franken_markdown::highlight::{Span, highlight};
use franken_markdown::resume::{ResumableLexer, ResumeError};

const CONSUMER_EXCERPT: &str = "\
# fcb-9vx.23 consumer excerpt

This excerpt is the FCB reader/fence verification fixture: real Markdown
with headings, `inline code`, **emphasis**, a fenced block:

```rust
let route = \"markdown\";
```

- list item one
- list item two

> a blockquote
<!-- an html comment -->
1. ordered item
";

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
}

fn fixtures() -> Vec<Fixture> {
    let headings = b"# Title\n## Section\n### Sub\n".to_vec();
    let list_markers = b"- alpha\n- beta\n1. one\n2. two\n".to_vec();
    let blockquote = b"> quoted line\n> second\n".to_vec();
    let inline_code = b"use `code spans` and `more` here\n".to_vec();
    let emphasis = b"some **bold** and *italic* markers\n".to_vec();
    let html_comment = b"<!-- a comment spanning\nmultiple lines -->\ntext\n".to_vec();
    let fenced_info = b"```rust\nfn main() {}\n```\nafter fence\n".to_vec();
    let setext = b"Title\n======\nbody text\n".to_vec();
    let escapes = b"\\*not emphasis\\*\n\\# not heading\n".to_vec();
    let unicode_heading = "# \u{00e9}\u{4e2d}\u{6587} heading\n\nbody \u{1f600}\n"
        .as_bytes()
        .to_vec();
    let consumer = CONSUMER_EXCERPT.as_bytes().to_vec();

    vec![
        Fixture {
            name: "headings",
            bytes: headings,
        },
        Fixture {
            name: "list_markers",
            bytes: list_markers,
        },
        Fixture {
            name: "blockquote",
            bytes: blockquote,
        },
        Fixture {
            name: "inline_code",
            bytes: inline_code,
        },
        Fixture {
            name: "emphasis",
            bytes: emphasis,
        },
        Fixture {
            name: "html_comment",
            bytes: html_comment,
        },
        Fixture {
            name: "fenced_info",
            bytes: fenced_info,
        },
        Fixture {
            name: "setext",
            bytes: setext,
        },
        Fixture {
            name: "escapes",
            bytes: escapes,
        },
        Fixture {
            name: "unicode_heading",
            bytes: unicode_heading,
        },
        Fixture {
            name: "consumer",
            bytes: consumer,
        },
    ]
}

/// Merge adjacent spans of the same kind; drop zero-length spans.
fn coalesce(spans: &[Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        if span.start == span.end {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.kind == span.kind && last.end == span.start {
                last.end = span.end;
                continue;
            }
        }
        out.push(*span);
    }
    out
}

fn whole_spans(bytes: &[u8]) -> Vec<Span> {
    let text = std::str::from_utf8(bytes).expect("fixtures are valid UTF-8");
    highlight("markdown", text)
}

fn split_spans(bytes: &[u8], at: usize) -> Vec<Span> {
    let mut lexer = ResumableLexer::new("markdown").expect("markdown route supported");
    if at > 0 {
        lexer.feed(&bytes[..at]).expect("first feed valid");
    }
    if at < bytes.len() {
        lexer.feed(&bytes[at..]).expect("second feed valid");
    }
    lexer.finish().expect("finish flushes suffix");
    coalesce(lexer.spans())
}

fn assert_tile(spans: &[Span], len: usize) {
    let mut cursor = 0usize;
    for span in spans {
        assert_eq!(
            span.start, cursor,
            "gap or overlap at {}: span starts at {}",
            cursor, span.start
        );
        assert!(span.end > span.start, "empty span at {cursor}");
        cursor = span.end;
    }
    assert_eq!(cursor, len, "spans do not reach end of source");
}

fn receipts_dir() -> PathBuf {
    let run_id = std::env::var("FCB_012_RUN_ID").unwrap_or_else(|_| "local".to_string());
    std::env::temp_dir().join(format!("fcb-9vx23-receipts-{run_id}"))
}

fn scenario_receipt(case: &str, outcome: &str, detail: &str) {
    let run_dir = receipts_dir();
    std::fs::create_dir_all(&run_dir).expect("receipts dir created");
    let line = format!(
        "fcb-9vx.23 markdown-route receipt\nscenario: {case}\noutcome: {outcome}\ndetail: {detail}\n"
    );
    std::fs::write(
        run_dir.join(format!(
            "{}.receipt",
            case.replace(['(', ')', ' ', ':'], "_")
        )),
        line,
    )
    .expect("receipt retained");
}

#[test]
fn line_boundary_splits_are_split_equivalent_on_every_fixture() {
    for fixture in &fixtures() {
        let expected = coalesce(&whole_spans(&fixture.bytes));
        assert_tile(&expected, fixture.bytes.len());
        for at in line_boundary_splits(&fixture.bytes) {
            let got = split_spans(&fixture.bytes, at);
            assert_eq!(
                got, expected,
                "{}: line-boundary split at {at} diverged from whole run",
                fixture.name
            );
        }
        scenario_receipt(
            fixture.name,
            "line-boundary-safe",
            "all line-boundary splits equivalent",
        );
    }
}

#[test]
fn consumer_excerpt_line_boundary_splits_are_equivalent() {
    let bytes = CONSUMER_EXCERPT.as_bytes().to_vec();
    let expected = coalesce(&whole_spans(&bytes));
    for at in line_boundary_splits(&bytes) {
        let got = split_spans(&bytes, at);
        assert_eq!(
            got, expected,
            "consumer excerpt: line-boundary split at {at} diverged"
        );
    }
}

#[test]
fn every_midline_split_of_the_heading_fixture_is_equivalent() {
    // Exhaustive probe of EVERY mid-line split on the heading fixture —
    // the line-anchored constructs (heading markers) are the most likely
    // to expose line-state loss, and none was found.
    let doc = b"# Title\n## Section\n### Sub\nbody\n".to_vec();
    let whole = coalesce(&whole_spans(&doc));
    for at in 1..doc.len() {
        let got = split_spans(&doc, at);
        assert_eq!(got, whole, "mid-line split at {at} diverged from whole run");
    }
    scenario_receipt(
        "heading_fixture_exhaustive_midline_probe",
        "fully-split-safe",
        "all mid-line splits equivalent; no line-state divergence found",
    );
}

fn line_boundary_splits(bytes: &[u8]) -> Vec<usize> {
    let mut splits = vec![0usize];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' && index < bytes.len() {
            splits.push(index + 1);
        }
    }
    splits
}

#[test]
fn feeds_after_finish_are_refused() {
    let mut lexer = ResumableLexer::new("markdown").expect("markdown route supported");
    lexer.feed(b"# heading").expect("first feed");
    lexer.finish().expect("finish");
    let error = lexer.feed(b"# late").expect_err("post-finish feed refused");
    assert_eq!(error, ResumeError::AlreadyFinished);
}

#[test]
fn registry_route_is_truthfully_provisional() {
    use franken_markdown::lang_dispatch::{DispatchRequest, LanguageRegistry};

    let registry = LanguageRegistry::standard_20_inventory();
    let request = DispatchRequest {
        language_query: "markdown",
        code: b"# heading\n\nbody text\n",
        work_budget_bytes: 4096,
        source_revision: None,
    };
    // Provisional routes remain dispatchable (truthful row), and the
    // dispatched classification must match the direct highlight path.
    let result = registry.dispatch(request).expect("markdown dispatches");
    assert_eq!(
        franken_markdown::lang_dispatch::QualificationStatus::Provisional,
        result.status
    );
    assert!(result.is_finished);
    let direct = highlight(
        "markdown",
        std::str::from_utf8(request.code).expect("utf-8"),
    );
    assert_eq!(coalesce(&result.spans), coalesce(&direct));
}
