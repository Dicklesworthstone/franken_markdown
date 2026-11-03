//! Unicode reference keys follow the complete pinned default full case fold.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

const CASE_FOLDING: &str = include_str!("fixtures/unicode/CaseFolding-17.0.0.txt");

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn link(label: &str, destination: &str) -> Inline {
    Inline::Link {
        dest: destination.into(),
        title: Some("Title".into()),
        content: vec![text(label)],
    }
}

fn expected(destination: &str) -> Vec<Block> {
    vec![Block::Paragraph(vec![
        link("label", destination),
        text(" "),
        Inline::Image {
            dest: destination.into(),
            title: Some("Title".into()),
            alt: "picture".into(),
        },
    ])]
}

#[test]
fn every_official_full_mapping_resolves_reference_links_and_images_in_both_directions() {
    let mut checked = 0;
    let mut mismatches = Vec::new();
    for line in CASE_FOLDING.lines() {
        let fields: Vec<_> = line
            .split('#')
            .next()
            .unwrap()
            .split(';')
            .map(str::trim)
            .collect();
        if fields.len() < 3 || !matches!(fields[1], "C" | "F") {
            continue;
        }
        let cp = u32::from_str_radix(fields[0], 16).unwrap();
        let original = char::from_u32(cp).unwrap().to_string();
        let folded: String = fields[2]
            .split_whitespace()
            .map(|value| char::from_u32(u32::from_str_radix(value, 16).unwrap()).unwrap())
            .collect();
        for (definition, label) in [(&original, &folded), (&folded, &original)] {
            let source = format!(
                "[label][{label}] ![picture][{label}]\n\n[{definition}]: /target.png \"Title\"\n"
            );
            if parse_markdown(&source).blocks != expected("/target.png") {
                mismatches.push(format!("U+{cp:04X}: {definition:?} -> {label:?}"));
            }
        }
        checked += 1;
    }
    assert_eq!(
        checked, 1585,
        "the complete Unicode 17.0.0 C+F data is exercised"
    );
    assert!(
        mismatches.is_empty(),
        "{} failed resolutions, first cases: {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(12)]
    );
}

#[test]
fn full_collapsed_and_shortcut_forms_preserve_original_display_text() {
    for (label, definition) in [
        ("ﬃ", "FFI"),
        ("ſ", "S"),
        ("µ", "Μ"),
        ("ǰ", "J\u{030c}"),
        ("և", "ԵՒ"),
        ("ᾲ", "ὰΙ"),
        ("ᏸ", "Ᏸ"),
        ("İ", "i\u{0307}"),
    ] {
        let source = format!(
            "[label][{label}] [{label}][] [{label}]\n\n[{definition}]: /target \"Title\"\n"
        );
        let document = parse_markdown(&source);
        assert_eq!(
            document.blocks,
            vec![Block::Paragraph(vec![
                link("label", "/target"),
                text(" "),
                link(label, "/target"),
                text(" "),
                link(label, "/target"),
            ])],
            "{label:?}"
        );
        let html =
            franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
        assert_eq!(html.matches("href=\"/target\"").count(), 3);
        assert!(html.contains(&format!(">{label}</a>")));
    }
}

#[test]
fn folded_duplicate_definitions_keep_first_source_order_across_containers() {
    for (first, second) in [("ﬃ", "FFI"), ("FFI", "ﬃ"), ("ſ", "S"), ("S", "ſ")] {
        for prefix in ["", "> ", "- "] {
            let source = format!(
                "{prefix}[{first}]: /first \"Title\"\n\n[{second}]: /second \"Wrong\"\n\n[label][{second}] ![picture][{first}]\n"
            );
            let document = parse_markdown(&source);
            assert_eq!(
                document.blocks.last(),
                expected("/first").last(),
                "{source:?}"
            );
            let spans = destination_spans(&source);
            assert_eq!(
                spans
                    .iter()
                    .map(|span| span.dest.as_str())
                    .collect::<Vec<_>>(),
                ["/first", "/second"]
            );
        }
    }
}

#[test]
fn folding_does_not_add_locale_tailoring_normalization_or_inline_decoding() {
    for (definition, label) in [
        ("I", "ı"),
        ("İ", "i"),
        ("é", "e\u{0301}"),
        ("Ａ", "A"),
        ("&#xFB03;", "ffi"),
        (r"\*", "*"),
    ] {
        let source = format!("[{definition}]: /wrong\n\n[visible][{label}]\n");
        let document = parse_markdown(&source);
        let Block::Paragraph(inlines) = &document.blocks[0] else {
            panic!("unresolved source")
        };
        assert!(
            !inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Link { .. })),
            "{source:?}"
        );
    }
}

#[test]
fn source_character_limit_is_checked_before_expanding_the_folded_key() {
    for count in [333, 999] {
        let label = "ﬃ".repeat(count);
        let source =
            format!("[{label}]: /valid \"Title\"\n\n[label][{label}] ![picture][{label}]\n");
        assert_eq!(parse_markdown(&source).blocks, expected("/valid"));
    }
    let ligatures = "ﬃ".repeat(333);
    let expanded = "FFI".repeat(333);
    let source =
        format!("[{ligatures}]: /valid \"Title\"\n\n[label][{expanded}] ![picture][{expanded}]\n");
    assert_eq!(parse_markdown(&source).blocks, expected("/valid"));

    let astral = "𐐀".repeat(999);
    let source =
        format!("[{astral}]: /valid \"Title\"\n\n[label][{astral}] ![picture][{astral}]\n");
    assert_eq!(parse_markdown(&source).blocks, expected("/valid"));

    let padded = format!("{}id", " ".repeat(998));
    let source = format!("[id]: /invalid\n\n[visible][{padded}]\n");
    assert!(matches!(
        &parse_markdown(&source).blocks[0],
        Block::Paragraph(inlines) if inlines == &[text(&format!("[visible][{padded}]"))]
    ));

    let label = "ﬃ".repeat(1000);
    let source = format!("[{label}]: /invalid\n\n[visible][{label}]\n");
    assert!(destination_spans(&source).is_empty());
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 2);
    assert!(
        matches!(&document.blocks[0], Block::Paragraph(inlines) if inlines == &[text(&format!("[{label}]: /invalid"))])
    );
}

#[test]
fn whitespace_collapsing_and_exact_original_spans_survive_expanding_folds() {
    let source = "\u{feff}[label][ STRASSE\tFFI ] ![picture][Straße ﬃ]\r\n\r\n[Straße\r\n  ﬃ]: </café#FfI> \"Title\"\r\n";
    let document = parse_markdown(source);
    assert_eq!(document.blocks, expected("/café#FfI"));
    assert_eq!(parse_markdown_profiled(source).document, document);
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), document);
    assert_eq!(
        spanned.blocks[0].span.slice(source),
        Some("[label][ STRASSE\tFFI ] ![picture][Straße ﬃ]")
    );
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(&source[spans[0].range.clone()], "</café#FfI>");
    assert_eq!(&source[spans[0].fragment_range(source).unwrap()], "FfI");
}
