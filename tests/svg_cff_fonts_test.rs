//! CFF host fonts must produce native cubic ink, including through shaping,
//! and must fail closed when an actually painted glyph cannot be decoded.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use franken_markdown::ast::{Block, Document, Inline};
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics, render_svg_with_resources};
use franken_markdown::text::Font;
use franken_markdown::{FontAssets, RenderError, parse_markdown};

const SOURCE_SANS: &[u8] = include_bytes!("fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");

fn resources(bytes: &[u8]) -> FontAssets {
    FontAssets {
        body_regular: Some(bytes.to_vec()),
        ..FontAssets::default()
    }
}

fn paths(svg: &str) -> Vec<&str> {
    svg.lines()
        .filter(|line| line.starts_with("<path id=\"g"))
        .map(|line| {
            line.split(" d=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
        })
        .collect()
}

#[test]
fn cff_cubics_match_independent_outline_and_reuse_one_definition() {
    let doc = parse_markdown("OOO");
    let opts = SvgOptions::default();
    let fonts = resources(SOURCE_SANS);
    let output = render_svg_with_resources(&doc, &opts, &fonts, &[]).unwrap();
    assert_eq!(
        output,
        render_svg_with_resources(&doc, &opts, &fonts, &[]).unwrap()
    );
    let (bytes, report, warnings) = output;
    let svg = String::from_utf8(bytes).unwrap();
    // Independent FontTools RecordingPen output for the pinned upstream O
    // (GID16), scaled by 0.1 and with y flipped. These are cubic control
    // points, not an outline or expected value obtained from fmd-font.
    let expected = concat!(
        "M33.20 1.20C49.70 1.20 61.30 -12.00 61.30 -33.10",
        "C61.30 -54.00 49.70 -66.80 33.20 -66.80",
        "C16.70 -66.80 5.20 -54.10 5.20 -33.10",
        "C5.20 -12.00 16.70 1.20 33.20 1.20Z",
        "M33.20 -6.10C21.50 -6.10 13.80 -16.70 13.80 -33.10",
        "C13.80 -49.40 21.50 -59.50 33.20 -59.50",
        "C44.90 -59.50 52.60 -49.40 52.60 -33.10",
        "C52.60 -16.70 44.90 -6.10 33.20 -6.10Z"
    );
    assert_eq!(paths(&svg), [expected]);
    assert_eq!(report.paths_emitted, 1);
    assert_eq!(report.glyphs_drawn, 3);
    assert_eq!(report.glyphs_missing, 0);
    assert_eq!(svg.matches("<use href=\"#g0\"").count(), 3);
    assert!(!svg.contains("<text"));
    assert!(warnings.is_empty());
}

#[test]
fn cff_and_truetype_slots_keep_their_native_curve_types() {
    let doc = parse_markdown("O **O**");
    let (bytes, report, _) =
        render_svg_with_resources(&doc, &SvgOptions::default(), &resources(SOURCE_SANS), &[])
            .unwrap();
    let svg = String::from_utf8(bytes).unwrap();
    let definitions = paths(&svg);
    assert_eq!(report.glyphs_drawn, 2);
    assert_eq!(definitions.len(), 2);
    assert!(
        definitions
            .iter()
            .any(|path| path.contains('C') && !path.contains('Q'))
    );
    assert!(
        definitions
            .iter()
            .any(|path| path.contains('Q') && !path.contains('C'))
    );
}

#[test]
fn all_host_style_slots_support_cff_cubics() {
    let fonts = FontAssets {
        body_regular: Some(SOURCE_SANS.to_vec()),
        body_bold: Some(SOURCE_SANS.to_vec()),
        body_italic: Some(SOURCE_SANS.to_vec()),
        body_bold_italic: Some(SOURCE_SANS.to_vec()),
        mono_regular: Some(SOURCE_SANS.to_vec()),
        ..FontAssets::default()
    };
    let doc = parse_markdown("O **O** *O* ***O*** `O`");
    let (bytes, report, _) =
        render_svg_with_resources(&doc, &SvgOptions::default(), &fonts, &[]).unwrap();
    let svg = String::from_utf8(bytes).unwrap();
    assert_eq!(report.glyphs_drawn, 5);
    assert_eq!(report.paths_emitted, 5);
    assert!(
        paths(&svg)
            .iter()
            .all(|path| path.contains('C') && !path.contains('Q'))
    );
}

#[test]
fn cff_poster_retains_more_than_256_distinct_glyphs() {
    let font = Font::parse(SOURCE_SANS.to_vec()).unwrap();
    let mut gids = BTreeSet::new();
    let chars: Vec<char> = (0x21..=0x52f)
        .filter_map(char::from_u32)
        .filter(|ch| ch.is_alphabetic())
        .filter(|&ch| {
            let gid = font.glyph_index(ch);
            gid != 0 && font.advance_1000(ch) > 0 && gids.insert(gid)
        })
        .take(300)
        .collect();
    assert_eq!(chars.len(), 300);
    let text = chars
        .iter()
        .map(char::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let doc = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Text(text)])],
    };
    let (bytes, report, _) =
        render_svg_with_resources(&doc, &SvgOptions::default(), &resources(SOURCE_SANS), &[])
            .unwrap();
    assert_eq!(report.paths_emitted, 300);
    assert_eq!(report.glyphs_drawn, 300);
    assert_eq!(report.glyphs_missing, 0);
    assert_eq!(paths(std::str::from_utf8(&bytes).unwrap()).len(), 300);
}

/// FontTools locates the pinned fixture's 12-byte O (GID16) CharString at
/// byte71697. Mutate only that program: the font remains structurally valid,
/// and .notdef and all unrelated glyphs still decode normally.
fn altered_o(program: &[u8]) -> Vec<u8> {
    const START: usize = 71_697;
    assert_eq!(&SOURCE_SANS[START..START + 4], &[0x33, 0x0a, 0x01, 0xbf]);
    assert!(program.len() <= 12);
    let mut bytes = SOURCE_SANS.to_vec();
    bytes[START..START + 12].fill(14); // endchar padding after the replacement
    bytes[START..START + program.len()].copy_from_slice(program);
    bytes
}

#[test]
fn only_used_bad_cff_glyphs_block_svg_output() {
    let opts = SvgOptions::default();
    for (program, kind) in [
        (&[0][..], "UnsupportedOperator"),
        (&[21][..], "Malformed"), // rmoveto with no coordinates
        // Move to32767,0 and draw to32768,0: valid operators, out-of-range ink.
        (&[28, 0x7f, 0xff, 139, 21, 140, 139, 5, 14][..], "Capacity"),
    ] {
        let fonts = resources(&altered_o(program));
        fonts.validate().unwrap();
        let (_, report, _) =
            render_svg_with_resources(&parse_markdown("B"), &opts, &fonts, &[]).unwrap();
        assert_eq!(report.glyphs_drawn, 1, "unused O: {kind}");
        let err = render_svg_with_resources(&parse_markdown("O"), &opts, &fonts, &[])
            .expect_err("used invalid CFF ink must not become a successful blank poster");
        assert!(matches!(err, RenderError::InvalidInput(_)));
        let message = err.to_string();
        assert!(message.contains("svg_cff_font"), "{message}");
        assert!(message.contains(kind), "{message}");
    }
}

#[test]
fn empty_resources_preserve_bundled_svg_bytes_and_reports() {
    let doc = parse_markdown("# Heading\n\nPlain **bold** *italic* and `code`.\n\n$\\frac{a}{b}$");
    let opts = SvgOptions::default();
    assert_eq!(
        render_svg_with_resources(&doc, &opts, &FontAssets::default(), &[]).unwrap(),
        render_svg_with_diagnostics(&doc, &opts),
    );
}
