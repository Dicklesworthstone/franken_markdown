//! Public-API behavior pins for coverage gap-fill across `src/lib.rs` and
//! `src/html.rs` (heading-id collisions, table cell/alignment mismatch, and the
//! safe-URL policy edges), plus the glyf-outline font-asset guard.
//!
//! Real inputs, no mocks: the font test mutates the REAL bundled TrueType font's
//! table directory, and every HTML assertion pins exact emitted markup.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Align, Block, Document, Inline, Table};
use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::{
    FontAssetSlot, FontAssets, FontFamily, HtmlOptions, RenderError, render_html,
    render_html_document,
};

fn text(t: &str) -> Inline {
    Inline::Text(t.to_string())
}

fn link(label: &str, dest: &str) -> Inline {
    Inline::Link {
        dest: dest.to_string(),
        title: None,
        content: vec![text(label)],
    }
}

#[test]
fn font_asset_without_glyf_outlines_is_rejected_with_a_named_slot_error() {
    // Take the real bundled serif font and rename its `glyf` table directory
    // tag. The font still parses (outline tables are optional at parse time,
    // as for CFF/OpenType), but it can no longer be deterministically subset,
    // so the slot validator must reject it.
    let mut bytes = fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec();
    let num_tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    let mut renamed = false;
    for i in 0..num_tables {
        let off = 12 + 16 * i;
        if &bytes[off..off + 4] == b"glyf" {
            bytes[off..off + 4].copy_from_slice(b"gXyf");
            renamed = true;
        }
    }
    assert!(renamed, "bundled serif font must carry a glyf table");

    let err = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, bytes)
        .expect_err("a glyf-less font must be rejected");
    assert!(matches!(err, RenderError::InvalidInput(_)), "{err:?}");
    let msg = err.to_string();
    assert!(msg.contains("body-regular"), "error names the slot: {msg}");
    assert!(
        msg.contains("glyf outlines"),
        "error names the missing capability: {msg}"
    );
}

#[test]
fn duplicate_heading_ids_get_deterministic_numeric_suffixes() {
    // The third "A" collides with the explicit "A-2" heading, so the suffix
    // probe must skip the taken candidate and emit "a-3".
    let html = render_html("# A\n\n# A-2\n\n# A\n\n# A\n", &HtmlOptions::default())
        .expect("render succeeds");
    assert!(html.contains("<h1 id=\"a\">A</h1>"), "{html}");
    assert!(html.contains("<h1 id=\"a-2\">A-2</h1>"), "{html}");
    assert!(html.contains("<h1 id=\"a-3\">A</h1>"), "{html}");
    assert!(html.contains("<h1 id=\"a-4\">A</h1>"), "{html}");
}

#[test]
fn heading_id_suffixes_reach_two_digits_without_padding() {
    let source = "## T\n\n".repeat(12);
    let html = render_html(&source, &HtmlOptions::default()).expect("render succeeds");
    assert!(html.contains("<h2 id=\"t\">T</h2>"), "{html}");
    assert!(html.contains("<h2 id=\"t-9\">T</h2>"), "{html}");
    assert!(html.contains("<h2 id=\"t-10\">T</h2>"), "{html}");
    assert!(html.contains("<h2 id=\"t-12\">T</h2>"), "{html}");
    assert!(!html.contains("id=\"t-13\""), "{html}");
}

#[test]
fn table_cells_beyond_the_alignment_row_render_without_style_attributes() {
    // A directly-constructed AST can carry more row cells than alignment
    // columns; the extras must still render, just without a style attribute.
    let doc = Document {
        blocks: vec![Block::Table(Table {
            align: vec![Align::Center],
            head: vec![vec![text("H")]],
            rows: vec![vec![vec![text("A")], vec![text("B")]]],
        })],
    };
    let html = render_html_document(&doc, &HtmlOptions::default()).expect("render succeeds");
    assert!(
        html.contains("<th style=\"text-align:center\">H</th>"),
        "{html}"
    );
    assert!(
        html.contains("<tr><td style=\"text-align:center\">A</td><td>B</td></tr>"),
        "overflow cell must render unstyled: {html}"
    );
}

#[test]
fn safe_url_policy_keeps_schemeless_urls_and_drops_malformed_schemes() {
    let doc = Document {
        blocks: vec![Block::Paragraph(vec![
            // '/', '?', and '#' before ':' mean "no scheme": kept verbatim.
            link("slash-link", "a/b:c"),
            link("query-link", "q?x:y"),
            link("frag-link", "s#f:g"),
            // Whitespace or control bytes before ':' are suspicious: dropped.
            link("ctl-link", "ja\u{7}vascript:alert(0xc1)"),
            link("gap-link", "ja vascript:alert(0xc2)"),
            // Empty / non-alphabetic-first / invalid-byte schemes: dropped.
            link("colon-link", ":odd-dest-c3"),
            link("digit-link", "1a:odd-dest-c4"),
            link("tilde-link", "a~b:odd-dest-c5"),
        ])],
    };
    let html = render_html_document(&doc, &HtmlOptions::default()).expect("render succeeds");

    assert!(html.contains("<a href=\"a/b:c\">slash-link</a>"), "{html}");
    assert!(html.contains("<a href=\"q?x:y\">query-link</a>"), "{html}");
    assert!(html.contains("<a href=\"s#f:g\">frag-link</a>"), "{html}");

    for (label, fragment) in [
        ("ctl-link", "alert(0xc1)"),
        ("gap-link", "alert(0xc2)"),
        ("colon-link", "odd-dest-c3"),
        ("digit-link", "odd-dest-c4"),
        ("tilde-link", "odd-dest-c5"),
    ] {
        assert!(
            html.contains(label),
            "dropped link keeps its visible content {label:?}: {html}"
        );
        assert!(
            !html.contains(fragment),
            "unsafe destination {fragment:?} must not reach the output: {html}"
        );
    }
}

#[test]
fn image_destinations_are_trimmed_of_control_bytes_before_use() {
    // Both ASCII whitespace and control bytes around the destination trim away.
    let doc = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Image {
            dest: "\u{1} https://e/i.png \u{1}".to_string(),
            title: None,
            alt: "trimmed".to_string(),
        }])],
    };
    let html = render_html_document(&doc, &HtmlOptions::default()).expect("render succeeds");
    assert!(
        html.contains("<img src=\"https://e/i.png\" alt=\"trimmed\">"),
        "control bytes around the destination must be trimmed: {html}"
    );
}
