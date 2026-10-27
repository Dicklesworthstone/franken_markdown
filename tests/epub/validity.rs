//! Publication behavior, checked through the public APIs and decoded ZIP members.
use super::*;
use franken_markdown::book::BookRenderer;
use franken_markdown::{Block, BookInput, Document, Inline, PdfImageAsset, RenderError};

#[test]
fn missing_images_remain_readable_without_unresolved_resource_references() {
    let source = "# Pictures\n\n[![A & B](https://example.invalid/image.png \"original title\")](https://example.com/details)\n\n![Local](missing.svg)\n\n<img src='also-missing.svg' alt='Raw &amp; safe'>\n\n![Present](present.svg)\n\n`![Code](not-an-image.svg)`\n";
    let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"6\"><rect width=\"12\" height=\"6\"/></svg>";
    let options = HtmlOptions {
        image_assets: vec![PdfImageAsset::new("present.svg", bytes.to_vec())],
        ..HtmlOptions::default()
    };
    let doc = parse_markdown(source);
    let original = doc.clone();
    let single = epub::render_epub(&doc, &options).unwrap();
    assert_eq!(doc, original, "export must not mutate source AST");
    let mut renderer = BookRenderer::new(&[BookInput {
        path: "guide/pictures.md".into(),
        source: source.into(),
    }])
    .unwrap();
    renderer
        .set_image("guide/present.svg", bytes.to_vec())
        .unwrap();
    let book = renderer.render_epub().unwrap();
    for archive in [&single, &book] {
        let body = entry_text(archive, "OEBPS/chapter-1.xhtml");
        assert_well_formed_xml(&body);
        assert_eq!(body.matches("class=\"fmd-image-unavailable\"").count(), 3);
        for alt in ["A &amp; B", "Local", "Raw &amp; safe"] {
            assert!(
                body.contains(&format!("[Image unavailable: {alt}]")),
                "{body}"
            );
        }
        assert!(body.contains("data-image-source=\"https://example.invalid/image.png\""));
        assert!(body.contains("title=\"original title\""));
        assert!(body.contains("href=\"https://example.com/details\"><span"));
        assert!(body.contains("<code>![Code](not-an-image.svg)</code>"));
        assert_eq!(body.matches("<img ").count(), 1);
        assert!(!body.contains("src=\"https://"));
        assert!(!body.contains("src=\"missing.svg\""));
        let paths: Vec<_> = parse_central_directory(archive)
            .into_iter()
            .filter(|entry| entry.name.starts_with("OEBPS/assets/"))
            .map(|entry| entry.name)
            .collect();
        assert_eq!(paths.len(), 1, "resolved illustration still packaged");
        let manifest = entry_text(archive, "OEBPS/content.opf");
        assert!(manifest.contains(paths[0].strip_prefix("OEBPS/").unwrap()));
    }
    assert_eq!(single, epub::render_epub(&doc, &options).unwrap());
    assert_eq!(book, renderer.render_epub().unwrap());
}

#[test]
fn supplied_remote_image_keys_are_packaged_instead_of_falling_back() {
    let url = "https://example.invalid/image?one=1&two=2";
    let source = format!("# Illustration\n\n![Supplied]({url})\n");
    let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"6\"/>";
    let options = HtmlOptions {
        image_assets: vec![PdfImageAsset::new(url, bytes.to_vec())],
        ..HtmlOptions::default()
    };
    let single = epub::render_epub(&parse_markdown(&source), &options).unwrap();
    let mut renderer = BookRenderer::new(&[BookInput {
        path: "picture.md".into(),
        source,
    }])
    .unwrap();
    renderer.set_image(url, bytes.to_vec()).unwrap();
    for archive in [single, renderer.render_epub().unwrap()] {
        let body = entry_text(&archive, "OEBPS/chapter-1.xhtml");
        assert_well_formed_xml(&body);
        assert!(body.contains("<img src=\"assets/"));
        assert!(!body.contains("fmd-image-unavailable"));
        assert!(!body.contains("https://example.invalid"));
    }
}

#[test]
fn xml_forbidden_scalars_are_rejected_without_mutating_the_document() {
    // XML 1.0 section 2.2: TAB/LF/CR are the only legal C0 controls; U+FFFE
    // and U+FFFF are also excluded. An entity escape cannot legalize them.
    let forbidden = (0u32..32)
        .filter(|value| ![9, 10, 13].contains(value))
        .chain([0xfffe, 0xffff]);
    for scalar in forbidden {
        let ch = char::from_u32(scalar).unwrap();
        let doc = Document {
            blocks: vec![Block::Paragraph(vec![Inline::Text(format!(
                "before{ch}after"
            ))])],
        };
        let original = doc.clone();
        let error = epub::render_epub(&doc, &HtmlOptions::default()).unwrap_err();
        assert!(matches!(error, RenderError::InvalidInput(_)));
        let message = error.to_string();
        assert!(message.contains("XML 1.0-forbidden character"), "{message}");
        assert!(message.contains(&format!("U+{scalar:04X}")), "{message}");
        assert!(message.contains("chapter-1.xhtml"), "{message}");
        assert_eq!(doc, original);
    }
}

#[test]
fn invalid_metadata_is_rejected_and_book_errors_identify_the_chapter() {
    let doc = parse_markdown("# Valid\n\nBody.\n");
    for options in [
        HtmlOptions {
            title: Some("Title\u{1f}".into()),
            ..HtmlOptions::default()
        },
        HtmlOptions {
            lang: Some("en\u{ffff}".into()),
            ..HtmlOptions::default()
        },
    ] {
        let error = epub::render_epub(&doc, &options).unwrap_err();
        assert!(error.to_string().contains("XML 1.0-forbidden character"));
        let book = franken_markdown::build_book(&[BookInput {
            path: "valid.md".into(),
            source: "# Valid\n\nBody.\n".into(),
        }])
        .unwrap();
        let error = epub::render_book_epub(&book, &options).unwrap_err();
        assert!(error.to_string().contains("XML 1.0-forbidden character"));
    }
    let book = franken_markdown::build_book(&[
        BookInput {
            path: "one.md".into(),
            source: "# First\n\nValid text.".into(),
        },
        BookInput {
            path: "two.md".into(),
            source: "# Second\n\n```text\ninvalid\u{1f}code\n```\n".into(),
        },
    ])
    .unwrap();
    let error = epub::render_book_epub(&book, &HtmlOptions::default()).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("chapter-2.xhtml"), "{message}");
    assert!(message.contains("U+001F"), "{message}");
}

#[test]
fn legal_xml_whitespace_unicode_and_escaped_source_text_are_preserved() {
    let source = "# Unicode\n\n```text\nA\tB\n\u{7f}\u{85}\u{a0}\u{e000}\u{fffd}\u{10000}\u{1fffe}\u{10ffff}\n```\n\n`&#x1F;` and &amp;.\n";
    let archive = epub::render_epub(&parse_markdown(source), &HtmlOptions::default()).unwrap();
    let body = entry_text(&archive, "OEBPS/chapter-1.xhtml");
    assert_well_formed_xml(&body);
    assert!(
        body.contains("&amp;#x1F;"),
        "escaped source must stay literal"
    );
    for scalar in [
        '\u{7f}',
        '\u{85}',
        '\u{a0}',
        '\u{e000}',
        '\u{fffd}',
        '\u{10000}',
        '\u{1fffe}',
        '\u{10ffff}',
    ] {
        assert!(
            body.contains(scalar),
            "legal scalar U+{:04X} lost",
            u32::from(scalar)
        );
    }
}
