//! Streaming HTML preserves document-wide state and reports sink failures
//! without retaining the whole rendered document.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{self, Write};
use std::path::Path;

use franken_markdown::{
    Block, Document, FontAssets, HtmlOptions, Inline, Profile, RenderError, parse_markdown,
    render_html_document, render_html_document_to_writer,
};

fn options() -> HtmlOptions {
    HtmlOptions {
        custom_css: Some("body { color: #222; }".into()),
        ..HtmlOptions::default()
    }
}

fn assert_parity(doc: &Document, opts: &HtmlOptions) {
    let expected = render_html_document(doc, opts).unwrap();
    let mut actual = Vec::new();
    let bytes = render_html_document_to_writer(doc, opts, &mut actual).unwrap();
    assert_eq!(bytes, expected.len());
    assert_eq!(actual, expected.as_bytes());
}

#[test]
fn cross_block_html_headings_toc_and_nested_notes_preserve_exact_bytes() {
    let source = "# Repeated <title>\n\n<div align=\"center\">\n\nFirst **paragraph**[^outer].\n\n# Repeated <title>\n\n> Quoted [link](https://example.com?a=1&b=2).\n\n</div>\n\n- [x] task\n- second\n\n| Name | Value |\n| :--- | ---: |\n| café | $2 |\n\nTerm\n: Definition\n\n```rust\nlet n = 1 < 2;\n```\n\n[^outer]: Outer note cites [^inner].\n\n[^inner]: Inner **note**.\n";
    let doc = parse_markdown(source);
    for allow_raw_html in [false, true] {
        for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
            let opts = HtmlOptions {
                toc: true,
                allow_raw_html,
                profile,
                title: Some("Streaming <>& \" title".into()),
                lang: Some("en\" &".into()),
                ..options()
            };
            assert_parity(&doc, &opts);
        }
    }
    assert_parity(&parse_markdown("[TOC]\n\n# One\n\n## Two\n"), &options());
    assert_parity(&Document::default(), &options());
    // Exercise generated font CSS as well as the smaller custom-CSS matrix.
    assert_parity(&doc, &HtmlOptions::default());
}

#[test]
fn repository_markdown_fixtures_match_the_full_string_renderer() {
    fn visit(path: &Path, count: &mut usize) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, count);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let source = std::fs::read_to_string(&path).unwrap();
                let doc = parse_markdown(&source);
                assert_parity(&doc, &options());
                *count += 1;
            }
        }
    }
    let mut count = 0;
    visit(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        &mut count,
    );
    assert!(count > 0, "fixture corpus must be exercised");
}

struct ShortWriter {
    bytes: Vec<u8>,
    remaining: Option<usize>,
    interrupted: bool,
    flushes: usize,
}

impl Write for ShortWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        if self.remaining == Some(0) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "closed test sink",
            ));
        }
        let count = bytes.len().min(7).min(self.remaining.unwrap_or(usize::MAX));
        self.bytes.extend_from_slice(&bytes[..count]);
        if let Some(remaining) = &mut self.remaining {
            *remaining -= count;
        }
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

#[test]
fn short_writes_and_interruptions_produce_exact_bytes_without_flushing() {
    let doc = parse_markdown("# αβγ\n\nOne & two.\n\n## αβγ\n\nLast paragraph.\n");
    let expected = render_html_document(&doc, &options()).unwrap();
    let mut sink = ShortWriter {
        bytes: Vec::new(),
        remaining: None,
        interrupted: false,
        flushes: 0,
    };
    let count = render_html_document_to_writer(&doc, &options(), &mut sink).unwrap();
    assert_eq!(sink.bytes, expected.as_bytes());
    assert_eq!(count, expected.len());
    assert_eq!(sink.flushes, 0);
}

#[test]
fn sink_failure_returns_original_error_after_only_a_prefix() {
    let doc = parse_markdown("# Start\n\nFirst paragraph.\n\nSecond paragraph.\n");
    let expected = render_html_document(&doc, &options()).unwrap();
    let limit = expected.len() - 10;
    let mut sink = ShortWriter {
        bytes: Vec::new(),
        remaining: Some(limit),
        interrupted: false,
        flushes: 0,
    };
    let error = render_html_document_to_writer(&doc, &options(), &mut sink).unwrap_err();
    assert!(
        matches!(error, RenderError::Io(ref error) if error.kind() == io::ErrorKind::BrokenPipe)
    );
    assert_eq!(sink.bytes, &expected.as_bytes()[..limit]);
    assert_eq!(sink.flushes, 0);
}

#[test]
fn invalid_fonts_are_rejected_before_writing_any_bytes() {
    let opts = HtmlOptions {
        font_assets: FontAssets {
            body_regular: Some(vec![0; 8]),
            ..FontAssets::default()
        },
        ..options()
    };
    let mut bytes = Vec::new();
    assert!(matches!(
        render_html_document_to_writer(&parse_markdown("text"), &opts, &mut bytes),
        Err(RenderError::InvalidInput(_))
    ));
    assert!(bytes.is_empty());
}

#[test]
fn many_blocks_are_delivered_incrementally() {
    #[derive(Default)]
    struct CountingSink {
        total: usize,
        largest: usize,
        writes: usize,
    }
    impl Write for CountingSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.total += bytes.len();
            self.largest = self.largest.max(bytes.len());
            self.writes += 1;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let doc = Document {
        blocks: (0..16_384)
            .map(|index| {
                Block::Paragraph(vec![Inline::Text(format!(
                    "Paragraph {index}: {}",
                    "<> & café ".repeat(32)
                ))])
            })
            .collect(),
    };
    let expected_len = render_html_document(&doc, &options()).unwrap().len();
    let mut sink = CountingSink::default();
    let count = render_html_document_to_writer(&doc, &options(), &mut sink).unwrap();
    assert_eq!(count, expected_len);
    assert_eq!(sink.total, expected_len);
    assert!(sink.total > 8 * 1024 * 1024);
    assert!(
        sink.largest < 64 * 1024,
        "ordinary blocks must not accumulate into a document-sized write"
    );
    assert!(sink.writes >= doc.blocks.len());
}
