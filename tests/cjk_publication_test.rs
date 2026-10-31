//! Self-contained CJK HTML uses real document subsets; full publications and
//! styled editors retain the complete optional fallback for future characters.
#![cfg(feature = "cjk-face")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::fonts::cjk_bytes;
use franken_markdown::text::Font;
use franken_markdown::{
    FontEmbedding, HtmlFontFormat, HtmlOptions, parse_markdown, render_html,
    render_interactive_html,
};

fn decode_base64(encoded: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut value = 0u32;
    let mut bits = 0;
    for ch in encoded.bytes() {
        let digit = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => break,
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((value >> bits) as u8);
        }
    }
    bytes
}

fn cjk_payload(html: &str, mime: &str) -> Vec<u8> {
    let face = html.split_once("font-family: \"FMD CJK\";").unwrap().1;
    let encoded = face.split_once(&format!("data:{mime};base64,")).unwrap().1;
    decode_base64(encoded)
}

#[test]
fn html_snapshots_embed_one_cjk_subset_for_every_style_before_host_fallbacks() {
    let source = "中文 **日本語** *かな* ***カナ*** `代码`";
    for (format, mime) in [
        (HtmlFontFormat::Ttf, "font/ttf"),
        (HtmlFontFormat::Woff2, "font/woff2"),
    ] {
        let opts = HtmlOptions {
            html_font_format: format,
            ..HtmlOptions::default()
        };
        let html = render_html(source, &opts).unwrap();
        assert_eq!(html.matches("font-family: \"FMD CJK\";").count(), 1);
        assert!(html.contains("--fmd-font-body: \"FMD Body\", \"FMD CJK\","));
        assert!(html.contains("--fmd-font-mono: \"FMD Mono\", \"FMD CJK\","));
        let payload = cjk_payload(&html, mime);
        let bytes = if format == HtmlFontFormat::Woff2 {
            franken_markdown::woff2::decode_woff2(&payload).unwrap()
        } else {
            payload
        };
        assert!(bytes.len() < cjk_bytes().unwrap().len() / 100);
        let font = Font::parse(bytes).unwrap();
        for ch in "中文日本語かなカナ代码".chars() {
            let gid = font.glyph_index(ch);
            assert_ne!(gid, 0, "{ch:?} must be self-contained");
            assert!(font.glyph_bbox(gid).is_some());
        }
        assert_eq!(
            font.glyph_index('龍'),
            0,
            "snapshot keeps the used repertoire only"
        );
        assert_eq!(
            font.glyph_index('A'),
            0,
            "fallback must not replace Latin typography"
        );
    }
}

#[test]
fn html_entity_text_embeds_decoded_cjk_in_blocks_inline_html_and_image_descriptions() {
    use franken_markdown::ast::{Block, Document, Inline};
    use franken_markdown::render_html_document;

    let opts = HtmlOptions {
        html_font_format: HtmlFontFormat::Ttf,
        ..HtmlOptions::default()
    };
    let source = "<div>&#x4E2D;&#25991;</div>";
    let block = render_html(source, &opts).unwrap();
    assert!(block.contains("<div>中文</div>"));

    // Direct AST callers may supply a complete inline HTML fragment. Keep
    // this path covered independently of how Markdown tokenizes raw HTML.
    let doc = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Html(
            "<span>&#20013;&#x6587;</span>".into(),
        )])],
    };
    let inline = render_html_document(&doc, &opts).unwrap();
    assert!(inline.contains("<span>中文</span>"));

    // Unsafe image URLs render their decoded description as visible text.
    // Literal descriptions must also retain their existing source coverage.
    let images = render_html(
        "<div><img src='javascript:bad' alt='&#x4E2D;&#25991;'><img src='missing.png' alt='日本語'></div>",
        &opts,
    )
    .unwrap();
    assert!(images.contains("<div>中文<img"));
    assert!(images.contains("alt=\"日本語\""));

    for (html, expected) in [(&block, "中文"), (&inline, "中文"), (&images, "中文日本語")]
    {
        assert_eq!(html.matches("font-family: \"FMD CJK\";").count(), 1);
        let font = Font::parse(cjk_payload(html, "font/ttf")).unwrap();
        for ch in expected.chars() {
            let gid = font.glyph_index(ch);
            assert_ne!(gid, 0, "decoded {ch:?} must be self-contained");
            assert!(font.glyph_bbox(gid).is_some());
        }
        assert_eq!(font.glyph_index('龍'), 0, "unrelated glyph stays omitted");
    }

    // An escaped reference is visible ASCII, not a second decoding pass.
    let literal = render_html("<div>&amp;#x4E2D;&amp;#25991;</div>", &opts).unwrap();
    assert!(!literal.contains("FMD CJK"));
}

#[test]
fn full_html_keeps_complete_cjk_bytes_only_for_documents_using_cjk() {
    let opts = HtmlOptions {
        html_font_format: HtmlFontFormat::Ttf,
        font_embedding: FontEmbedding::Full,
        ..HtmlOptions::default()
    };
    let html = render_html("中文", &opts).unwrap();
    assert!(cjk_payload(&html, "font/ttf") == cjk_bytes().unwrap());
    let ascii = render_html("Latin without CJK", &opts).unwrap();
    assert!(!ascii.contains("FMD CJK"));
    let subset = render_html("Latin without CJK", &HtmlOptions::default()).unwrap();
    assert!(!subset.contains("FMD CJK"));
}

#[test]
fn styled_editor_keeps_full_cjk_for_text_absent_from_its_initial_source() {
    let opts = HtmlOptions {
        toc: true,
        html_font_format: HtmlFontFormat::Ttf,
        ..HtmlOptions::default()
    };
    let source = "Initially Latin";
    let html = render_interactive_html(&parse_markdown(source), source, &opts);
    assert!(html.contains("data-fmd-styled-preview"));
    let expected = cjk_bytes().unwrap();
    assert!(
        html.split("data:font/ttf;base64,")
            .skip(1)
            .any(|encoded| decode_base64(encoded) == expected),
        "editor must retain the complete CJK face"
    );
}

#[test]
fn custom_html_css_keeps_its_existing_replacement_contract() {
    let opts = HtmlOptions {
        custom_css: Some("body { font-family: serif; color: navy; }".into()),
        ..HtmlOptions::default()
    };
    let html = render_html("中文日本語", &opts).unwrap();
    assert!(!html.contains("@font-face"));
    assert!(html.contains(opts.custom_css.as_deref().unwrap()));
}
