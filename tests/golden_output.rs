//! Golden rendered-output regression (bead grn.5.5).
//!
//! The existing `tests/golden/render_tree/*` goldens pin the LAYOUT tree; this
//! suite pins the actual RENDERED OUTPUT so a change in the HTML emitter, theme,
//! highlighter, or PDF writer is caught byte-for-byte:
//!
//!   * HTML — a deterministic snapshot (`<name>.html.snapshot`): the full-document
//!     byte length + content fingerprint (any byte drift — theme CSS, highlighter,
//!     font subset — moves the fingerprint) followed by the normalized `<main>`
//!     body for a human-reviewable structural diff. (We snapshot rather than commit
//!     the ~30-110 KB self-contained document, most of which is base64 font data.)
//!   * PDF — a deterministic structural snapshot (`<name>.pdf.snapshot`): byte
//!     length, a content fingerprint, page count, and indirect-object count. A
//!     binary diff in the (deterministic, pinned-epoch) PDF moves the fingerprint;
//!     a structural change moves the counts.
//!
//! Regenerate after an intentional output change:
//!   UPDATE_GOLDEN=1 cargo test --test golden_output
//! The run then rewrites the goldens and passes; commit the diff after review.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use franken_markdown::{HtmlOptions, PdfOptions, render_html, render_pdf};

/// Representative documents covering the major block + inline features. Kept small
/// and self-contained so the goldens stay reviewable.
const CASES: &[(&str, &str)] = &[
    (
        "prose",
        "# Title\n\nA paragraph with **strong**, *emphasis*, `inline code`, \
         ~~strike~~, and a [link](https://example.com).\n\nSecond paragraph for measure.\n",
    ),
    (
        "lists",
        "- alpha\n- beta\n  - nested gamma\n- [x] done\n- [ ] todo\n\n1. first\n2. second\n",
    ),
    (
        "table",
        "| Name | Value |\n|:---|---:|\n| alpha | 1 |\n| beta | 22 |\n| gamma | 333 |\n",
    ),
    (
        "code",
        "```rust\nfn main() {\n    let x = 42;\n    println!(\"{x}\");\n}\n```\n",
    ),
    (
        "quote-and-rule",
        "> A blockquote\n> spanning two lines.\n\n---\n\nClosing paragraph.\n",
    ),
];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/output")
}

fn updating() -> bool {
    std::env::var_os("UPDATE_GOLDEN").is_some()
}

fn fingerprint(bytes: &[u8]) -> u64 {
    // std's DefaultHasher has a fixed initial key, so this is deterministic across
    // runs and machines for a fixed input — exactly what a snapshot needs.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Snapshot the full HTML: a fingerprint over the whole self-contained document
/// (catches any byte drift) plus the normalized `<main>` body for review (the
/// `<style>` block, dominated by base64 font data, is excluded from the review
/// text but still folded into the fingerprint).
fn html_snapshot(html: &str) -> String {
    let body = html
        .find("<main")
        .and_then(|start| {
            html[start..]
                .find("</main>")
                .map(|end| &html[start..start + end + 7])
        })
        .unwrap_or("<main>(no main element)</main>");
    format!(
        "bytes={}\nfingerprint={:016x}\n--- body ---\n{}\n",
        html.len(),
        fingerprint(html.as_bytes()),
        body,
    )
}

fn pdf_snapshot(pdf: &[u8]) -> String {
    let text = String::from_utf8_lossy(pdf);
    let pages = text.matches("/Type /Page").count() - text.matches("/Type /Pages").count();
    let objects = text.matches("endobj").count();
    format!(
        "bytes={}\nfingerprint={:016x}\npages={}\nobjects={}\n",
        pdf.len(),
        fingerprint(pdf),
        pages,
        objects,
    )
}

#[test]
fn rendered_html_and_pdf_match_committed_goldens() {
    let dir = golden_dir();
    if updating() {
        std::fs::create_dir_all(&dir).unwrap();
    }
    let pdf_opts = PdfOptions {
        // Pin the date so the PDF (and thus its fingerprint) is deterministic.
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };

    let mut mismatches = Vec::new();
    for (name, src) in CASES {
        // --- HTML: fingerprint + normalized-body snapshot ---
        let html = render_html(src, &HtmlOptions::default()).unwrap();
        let html_snap = html_snapshot(&html);
        let html_path = dir.join(format!("{name}.html.snapshot"));
        if updating() {
            std::fs::write(&html_path, &html_snap).unwrap();
        } else {
            let want = std::fs::read_to_string(&html_path).unwrap_or_else(|_| {
                panic!("missing HTML snapshot {html_path:?}; run UPDATE_GOLDEN=1 to create it")
            });
            if want != html_snap {
                mismatches.push(format!("HTML snapshot mismatch for {name} ({html_path:?})"));
            }
        }

        // --- PDF: deterministic structural snapshot ---
        let pdf = render_pdf(src, &pdf_opts).unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "{name}: PDF must be well-formed");
        let snap = pdf_snapshot(&pdf);
        let snap_path = dir.join(format!("{name}.pdf.snapshot"));
        if updating() {
            std::fs::write(&snap_path, &snap).unwrap();
        } else {
            let want = std::fs::read_to_string(&snap_path).unwrap_or_else(|_| {
                panic!("missing PDF snapshot {snap_path:?}; run UPDATE_GOLDEN=1 to create it")
            });
            if want != snap {
                mismatches.push(format!(
                    "PDF snapshot mismatch for {name} ({snap_path:?}):\n  want:\n{want}  got:\n{snap}"
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "rendered-output goldens drifted (run UPDATE_GOLDEN=1 after reviewing):\n{}",
        mismatches.join("\n"),
    );
}
