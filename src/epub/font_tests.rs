//! Execute with cargo test --lib epub::embedded_fonts in the Rust environment.
//! Archive assertions check actual compressed payloads, sizes and CRCs; no
//! source-string matching is used as a substitute for exercising the renderer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{self as epub, Document};
use super::*;
use franken_markdown::{FontAssets, crc32, parse_markdown, zlib_decompress};

const SOURCE_SANS: &[u8] =
    include_bytes!("../../tests/fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");

fn options() -> HtmlOptions {
    HtmlOptions {
        title: Some("Résumé".into()),
        font_assets: FontAssets::default()
            .with_slot(
                FontAssetSlot::BodyRegular,
                fonts::body_bytes(FontFamily::Sans, FontStyle::Regular).to_vec(),
            )
            .unwrap(),
        ..HtmlOptions::default()
    }
}

fn package(opts: &HtmlOptions, text: &str) -> Package {
    let mut repertoire = Repertoire::new(enabled(opts).unwrap());
    repertoire.add(text).unwrap();
    repertoire.finish(opts).unwrap()
}

fn for_document(doc: &Document, opts: &HtmlOptions) -> Package {
    let mut html_opts = opts.clone();
    html_opts.custom_css = Some(String::new());
    html_opts.allow_raw_html = false;
    let html = franken_markdown::html::render(doc, &html_opts);
    let body = epub::html_fragment_to_xhtml(epub::extract_main_body(&html).unwrap());
    let body = epub::resources::prepare(&body).unwrap();
    let mut repertoire = Repertoire::new(enabled(opts).unwrap());
    repertoire.add(opts.title.as_deref().unwrap()).unwrap();
    repertoire.add(&epub::theme::stylesheet(opts)).unwrap();
    repertoire.add(&body.body).unwrap();
    repertoire.finish(opts).unwrap()
}

#[test]
fn no_explicit_fonts_keeps_the_archive_font_free_with_themed_css() {
    let opts = HtmlOptions {
        title: Some("Legacy".into()),
        ..HtmlOptions::default()
    };
    let doc = parse_markdown("# Legacy\n\nUnchanged text.\n");
    let mut html_opts = opts.clone();
    html_opts.custom_css = Some(String::new());
    let html = franken_markdown::html::render(&doc, &html_opts);
    let body = epub::html_fragment_to_xhtml(epub::extract_main_body(&html).unwrap());
    let prepared = epub::resources::prepare(&body).unwrap();
    let id = epub::content_identifier("Legacy", "en", &body);
    let css = epub::theme::stylesheet(&opts);
    let id = epub::content_identifier(&id, "epub-stylesheet-v1", &css);
    let mut old = ZipWriter::new();
    old.add_stored("mimetype", epub::MIMETYPE);
    old.add_deflated("META-INF/container.xml", epub::CONTAINER_XML.as_bytes());
    old.add_deflated(
        "OEBPS/content.opf",
        epub::content_opf("Legacy", "en", &id, &prepared).as_bytes(),
    );
    old.add_deflated(
        "OEBPS/nav.xhtml",
        epub::nav_xhtml("Legacy", "en", &doc).as_bytes(),
    );
    old.add_deflated(
        "OEBPS/chapter-1.xhtml",
        epub::chapter_xhtml("Legacy", "en", &prepared.body).as_bytes(),
    );
    old.add_deflated("OEBPS/style.css", css.as_bytes());
    assert_eq!(epub::render_epub(&doc, &opts).unwrap(), old.finish());
    assert!(!enabled(&opts).unwrap());
}

#[test]
fn all_semantic_faces_are_subset_and_have_valid_unicode_cmaps() {
    let opts = options();
    let result = package(&opts, "Résumé bold italic code");
    assert_eq!(result.css.matches("@font-face").count(), 6);
    assert!(
        result
            .css
            .contains("font-family:\"FmdEpubBody\";font-style:italic;font-weight:700")
    );
    assert!(
        result
            .css
            .contains("font-family:\"FmdEpubMono\";font-style:normal;font-weight:400")
    );
    assert!(result.css.contains("FmdEpubSymbols"));
    assert!(result.resources.len() <= 6);
    let body = Font::parse(result.resources[0].bytes.clone()).unwrap();
    for ch in ['R', 'é', ' ', '0', '9', '&', '<'] {
        assert_ne!(body.glyph_index(ch), 0, "{ch}");
    }
    assert!(
        result.resources[0].bytes.len() < opts.font_assets.body_regular.as_ref().unwrap().len()
    );
    for resource in &result.resources {
        assert!(
            Font::parse(resource.bytes.clone())
                .unwrap()
                .has_glyf_outlines()
        );
    }
}

#[test]
fn byte_identical_subsets_share_one_archive_resource_across_roles() {
    let mut opts = options();
    let bytes = opts.font_assets.body_regular.clone().unwrap();
    for slot in FontAssetSlot::ALL {
        opts.font_assets.set_slot(slot, bytes.clone()).unwrap();
    }
    let result = package(&opts, "same repertoire");
    assert_eq!(
        result.resources.len(),
        2,
        "five roles share a subset plus symbol fallback"
    );
    assert_eq!(result.css.matches("url(\"fonts/font-1.ttf\")").count(), 5);
    assert_eq!(result.css.matches("url(\"fonts/font-2.ttf\")").count(), 1);
}

#[test]
fn truetype_resources_keep_legacy_subset_bytes_and_metadata() {
    let font = fonts::load_body(FontFamily::Sans, FontStyle::Regular).unwrap();
    let keep = ['R', 'é', 's', 'u', 'm'];
    let expected = font.subset(&keep).unwrap();
    let mut package = Package::default();
    package
        .face(font, &keep, "Legacy", 400, false, FontEmbedding::Subset)
        .unwrap();
    assert_eq!(package.resources[0].bytes, expected);
    assert_eq!(package.resources[0].href, "fonts/font-1.ttf");
    assert_eq!(package.resources[0].format, EmbeddingFormat::TrueType);
    assert_eq!(
        package.css,
        "@font-face{font-family:\"Legacy\";font-style:normal;font-weight:400;src:url(\"fonts/font-1.ttf\") format(\"truetype\");}\n"
    );
    let mut opf = "<manifest></manifest>".to_owned();
    package.manifest(&mut opf).unwrap();
    assert_eq!(
        opf,
        "<manifest><item id=\"epub-font-css\" href=\"embedded-fonts.css\" media-type=\"text/css\"/>\n\
         <item id=\"epub-font-1\" href=\"fonts/font-1.ttf\" media-type=\"font/ttf\"/>\n</manifest>"
    );
}

fn cff_options() -> HtmlOptions {
    HtmlOptions {
        title: Some("Résumé".into()),
        font_assets: FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, SOURCE_SANS.to_vec())
            .unwrap(),
        ..HtmlOptions::default()
    }
}

fn assert_cff_glyphs(bytes: &[u8], text: &str) {
    assert_eq!(&bytes[..4], b"OTTO");
    let subset = Font::parse(bytes.to_vec()).unwrap();
    let source = Font::parse(SOURCE_SANS.to_vec()).unwrap();
    assert!(!subset.has_glyf_outlines());
    assert!(subset.num_glyphs < source.num_glyphs);
    for ch in text.chars() {
        let old = source.glyph_index(ch);
        let new = subset.glyph_index(ch);
        assert_ne!(old, 0, "fixture must contain {ch}");
        assert_ne!(new, 0, "subset must contain {ch}");
        assert_eq!(
            source.cff_outline(old).unwrap(),
            subset.cff_outline(new).unwrap(),
            "CFF glyph {ch} must retain its exact cubic outline and metrics"
        );
    }
}

#[test]
fn cff_semantic_faces_share_an_opentype_resource_with_truetype_symbols() {
    let mut opts = cff_options();
    for slot in FontAssetSlot::ALL {
        opts.font_assets
            .set_slot(slot, SOURCE_SANS.to_vec())
            .unwrap();
    }
    let doc = parse_markdown("# Résumé\n\nΩ **é** *Ж* ***O*** and `A`.\n");
    let package = for_document(&doc, &opts);
    assert_eq!(package.resources.len(), 2);
    assert_eq!(package.resources[0].href, "fonts/font-1.otf");
    assert_eq!(package.resources[0].format, EmbeddingFormat::OpenTypeCff);
    assert_cff_glyphs(&package.resources[0].bytes, "RésuméΩéЖOA");
    assert!(package.resources[0].bytes.len() < SOURCE_SANS.len());
    assert_eq!(package.resources[1].href, "fonts/font-2.ttf");
    assert_eq!(package.resources[1].format, EmbeddingFormat::TrueType);
    assert!(
        Font::parse(package.resources[1].bytes.clone())
            .unwrap()
            .has_glyf_outlines()
    );
    assert_eq!(package.css.matches("@font-face").count(), 6);
    assert_eq!(
        package
            .css
            .matches("url(\"fonts/font-1.otf\") format(\"opentype\")")
            .count(),
        5
    );
    assert_eq!(package.css.matches("format(\"truetype\")").count(), 1);
    let mut opf = "<manifest></manifest><spine/>".to_owned();
    package.manifest(&mut opf).unwrap();
    assert!(opf.contains("href=\"fonts/font-1.otf\" media-type=\"font/otf\""));
    assert!(opf.contains("href=\"fonts/font-2.ttf\" media-type=\"font/ttf\""));

    let output = epub::render_epub(&doc, &opts).unwrap();
    let entries = entries(&output);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.name.starts_with("OEBPS/fonts/"))
            .count(),
        2
    );
    assert_payload(&entries, "OEBPS/embedded-fonts.css", package.css.as_bytes());
    for resource in &package.resources {
        assert_payload(
            &entries,
            &format!("OEBPS/{}", resource.href),
            &resource.bytes,
        );
    }
    assert_eq!(output, epub::render_epub(&doc, &opts).unwrap());
}

#[test]
fn cff_body_and_bundled_truetype_styles_keep_their_own_resource_formats() {
    let package = package(&cff_options(), "Résumé Ω Ж bold italic code");
    assert_eq!(package.resources[0].format, EmbeddingFormat::OpenTypeCff);
    assert_cff_glyphs(&package.resources[0].bytes, "RésuméΩЖ");
    for resource in package.resources.iter().skip(1) {
        assert_eq!(resource.format, EmbeddingFormat::TrueType);
        assert!(resource.href.ends_with(".ttf"));
        assert!(
            Font::parse(resource.bytes.clone())
                .unwrap()
                .has_glyf_outlines()
        );
    }
    let mut opf = "<manifest></manifest>".to_owned();
    package.manifest(&mut opf).unwrap();
    assert_eq!(opf.matches("media-type=\"font/otf\"").count(), 1);
    assert_eq!(
        opf.matches("media-type=\"font/ttf\"").count(),
        package.resources.len() - 1
    );
    assert_eq!(package.css.matches("format(\"opentype\")").count(), 1);
    assert_eq!(package.css.matches("format(\"truetype\")").count(), 5);
}

#[test]
fn full_fonts_preserve_exact_programs_deduplication_and_archive_metadata() {
    let doc =
        parse_markdown("# Résumé\n\noffice AVATAR z\u{0301}\u{0307} **Ω** *é* ***Ж*** `A`.\n");
    for (source, format, extension, media_type) in [
        (SOURCE_SANS, EmbeddingFormat::OpenTypeCff, "otf", "font/otf"),
        (
            fonts::body_bytes(FontFamily::Sans, FontStyle::Regular),
            EmbeddingFormat::TrueType,
            "ttf",
            "font/ttf",
        ),
    ] {
        let mut opts = options();
        opts.font_embedding = FontEmbedding::Full;
        for slot in FontAssetSlot::ALL {
            opts.font_assets.set_slot(slot, source.to_vec()).unwrap();
        }
        let assets = opts.font_assets.clone();
        let package = for_document(&doc, &opts);
        assert_eq!(package.resources.len(), 2, "five full faces deduplicate");
        assert_eq!(package.resources[0].bytes, source);
        assert_eq!(package.resources[0].format, format);
        assert_eq!(
            package.resources[0].href,
            format!("fonts/font-1.{extension}")
        );
        assert_eq!(package.resources[1].bytes, fonts::symbol_bytes());
        assert_eq!(package.resources[1].format, EmbeddingFormat::TrueType);
        assert_eq!(
            package.payload_bytes,
            source.len() + fonts::symbol_bytes().len()
        );
        assert_eq!(
            package
                .css
                .matches(&format!("url(\"fonts/font-1.{extension}\")"))
                .count(),
            5
        );
        let mut opf = "<manifest></manifest><spine/>".to_owned();
        package.manifest(&mut opf).unwrap();
        assert!(opf.contains(&format!(
            "href=\"fonts/font-1.{extension}\" media-type=\"{media_type}\""
        )));
        assert!(opf.contains("href=\"fonts/font-2.ttf\" media-type=\"font/ttf\""));

        let output = epub::render_epub(&doc, &opts).unwrap();
        let entries = entries(&output);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.name.starts_with("OEBPS/fonts/"))
                .count(),
            2
        );
        for resource in &package.resources {
            assert_payload(
                &entries,
                &format!("OEBPS/{}", resource.href),
                &resource.bytes,
            );
        }
        assert_payload(&entries, "OEBPS/embedded-fonts.css", package.css.as_bytes());
        assert_eq!(output, epub::render_epub(&doc, &opts).unwrap());
        assert_eq!(opts.font_assets, assets);

        // The policy changes the payload and its content-derived identity,
        // while compact embedding remains available for the same options.
        opts.font_embedding = FontEmbedding::Subset;
        let compact = for_document(&doc, &opts);
        assert!(compact.payload_bytes < package.payload_bytes);
        assert_ne!(compact.fingerprint(), package.fingerprint());
    }
}

#[test]
fn full_missing_style_slots_keep_exact_bundled_programs() {
    let mut opts = cff_options();
    opts.font_embedding = FontEmbedding::Full;
    opts.theme.font = FontFamily::Sans;
    let result = package(&opts, "office AVATAR");
    assert_eq!(result.resources.len(), 6);
    let expected = [
        SOURCE_SANS,
        fonts::body_bytes(FontFamily::Sans, FontStyle::Bold),
        fonts::body_bytes(FontFamily::Sans, FontStyle::Italic),
        fonts::body_bytes(FontFamily::Sans, FontStyle::BoldItalic),
        fonts::mono_bytes(FontStyle::Regular),
        fonts::symbol_bytes(),
    ];
    for (resource, source) in result.resources.iter().zip(expected) {
        assert_eq!(resource.bytes, source);
    }
    assert_eq!(result.resources[0].format, EmbeddingFormat::OpenTypeCff);
    for resource in result.resources.iter().skip(1) {
        assert_eq!(resource.format, EmbeddingFormat::TrueType);
        assert!(resource.href.ends_with(".ttf"));
    }
}

#[test]
fn full_fonts_are_shared_once_across_the_entire_book_repertoire() {
    let mut opts = cff_options();
    opts.font_embedding = FontEmbedding::Full;
    for slot in FontAssetSlot::ALL {
        opts.font_assets
            .set_slot(slot, SOURCE_SANS.to_vec())
            .unwrap();
    }
    let inputs = [
        franken_markdown::BookInput {
            path: "first.md".into(),
            source: "# First\n\noffice AVATAR z\u{0301}\u{0307}\n".into(),
        },
        franken_markdown::BookInput {
            path: "second.md".into(),
            source: "# Second\n\nΩ Ж é\n".into(),
        },
    ];
    let book = franken_markdown::build_book(&inputs).unwrap();
    let expected = package(&opts, "Unrelated ASCII repertoire");
    let output = epub::render_book_epub(&book, &opts).unwrap();
    let entries = entries(&output);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.name.starts_with("OEBPS/fonts/"))
            .count(),
        2
    );
    assert_payload(&entries, "OEBPS/fonts/font-1.otf", SOURCE_SANS);
    assert_payload(&entries, "OEBPS/fonts/font-2.ttf", fonts::symbol_bytes());
    assert_payload(
        &entries,
        "OEBPS/embedded-fonts.css",
        expected.css.as_bytes(),
    );
    assert_eq!(output, epub::render_book_epub(&book, &opts).unwrap());
    let source = Font::parse(SOURCE_SANS.to_vec()).unwrap();
    for ch in ['Ω', 'Ж', 'é', 'ź'] {
        assert_ne!(source.glyph_index(ch), 0);
    }
}

#[test]
fn cff_book_shares_one_subset_with_glyphs_from_every_chapter() {
    let mut opts = cff_options();
    for slot in FontAssetSlot::ALL {
        opts.font_assets
            .set_slot(slot, SOURCE_SANS.to_vec())
            .unwrap();
    }
    let inputs = [
        franken_markdown::BookInput {
            path: "first.md".into(),
            source: "# First\n\nΩ\n".into(),
        },
        franken_markdown::BookInput {
            path: "second.md".into(),
            source: "# Second\n\nЖ é\n".into(),
        },
    ];
    let book = franken_markdown::build_book(&inputs).unwrap();
    let mut repertoire = Repertoire::new(true);
    repertoire.add(opts.title.as_deref().unwrap()).unwrap();
    repertoire.add(&epub::theme::stylesheet(&opts)).unwrap();
    for input in &inputs {
        repertoire.add(&input.source).unwrap();
    }
    let package = repertoire.finish(&opts).unwrap();
    assert_eq!(package.resources.len(), 2);
    assert_cff_glyphs(&package.resources[0].bytes, "ΩЖé");
    let output = epub::render_book_epub(&book, &opts).unwrap();
    let entries = entries(&output);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.name.ends_with(".otf"))
            .count(),
        1
    );
    for resource in &package.resources {
        assert_payload(
            &entries,
            &format!("OEBPS/{}", resource.href),
            &resource.bytes,
        );
    }
    assert_payload(&entries, "OEBPS/embedded-fonts.css", package.css.as_bytes());
    assert_eq!(output, epub::render_book_epub(&book, &opts).unwrap());
}

#[test]
fn a_used_unsupported_cff_glyph_refuses_epub_with_typed_context() {
    // FontTools locates the pinned source's Omega (GID818) Type2 program at
    // byte106951. Only that glyph is changed; structural font validation and
    // unrelated ASCII glyphs still succeed.
    const OMEGA: usize = 106_951;
    assert_eq!(&SOURCE_SANS[OMEGA..OMEGA + 4], &[0xf7, 0x34, 0x8b, 0xcf]);
    let mut bytes = SOURCE_SANS.to_vec();
    bytes[OMEGA..OMEGA + 2].copy_from_slice(&[12, 23]); // random is unsupported
    let mut opts = HtmlOptions {
        font_assets: FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, bytes)
            .unwrap(),
        ..cff_options()
    };
    assert!(epub::render_epub(&parse_markdown("Unrelated text"), &opts).is_ok());
    let error = epub::render_epub(&parse_markdown("Ω"), &opts).unwrap_err();
    let RenderError::InvalidInput(message) = error else {
        panic!("used CFF glyph failure must be an input error: {error}");
    };
    assert!(message.contains("epub_fonts:"), "{message}");
    assert!(message.contains("UnsupportedOperator"), "{message}");
    assert!(message.contains("glyph Some(818)"), "{message}");

    // Full embedding delegates the complete admitted program to the reader;
    // compact decoder restrictions must not accidentally reject that mode.
    opts.font_embedding = FontEmbedding::Full;
    let full = epub::render_epub(&parse_markdown("Ω"), &opts).unwrap();
    assert_payload(
        &entries(&full),
        "OEBPS/fonts/font-1.otf",
        opts.font_assets.body_regular.as_ref().unwrap(),
    );
}

#[test]
fn full_variable_faces_use_the_existing_instances_before_deduplication() {
    let source = franken_markdown::text::variable_triangle_fixture();
    let parsed = Font::parse(source.clone()).unwrap();
    let mut opts = options();
    opts.font_embedding = FontEmbedding::Full;
    opts.font_assets = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, source)
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, 300)
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyBold, 800)
        .unwrap();
    let assets = opts.font_assets.clone();
    let result = package(&opts, "A");
    assert_ne!(result.resources[0].bytes, result.resources[1].bytes);
    for (resource, weight) in result.resources.iter().zip([300., 800.]) {
        assert_eq!(resource.bytes, parsed.instance(weight).unwrap().as_sfnt());
        assert!(
            Font::parse(resource.bytes.clone())
                .unwrap()
                .instance_bounds(*b"wght")
                .is_none()
        );
    }
    assert_eq!(opts.font_assets, assets);
    opts.font_assets
        .set_slot_weight(FontAssetSlot::BodyBold, 300)
        .unwrap();
    let same = package(&opts, "A");
    assert_eq!(same.css.matches("url(\"fonts/font-1.ttf\")").count(), 2);
    assert_eq!(same.resources.len() + 1, result.resources.len());
    assert_ne!(same.fingerprint(), result.fingerprint());
}

#[test]
fn full_mode_embeds_bundled_faces_and_enforces_admission_and_output_limits() {
    let doc = parse_markdown("# No supplied fonts\n\nContent.");
    let mut opts = HtmlOptions::default();
    let font_free = epub::render_epub(&doc, &opts).unwrap();
    assert!(!enabled(&opts).unwrap());
    opts.font_embedding = FontEmbedding::Full;
    assert!(enabled(&opts).unwrap());
    let full = epub::render_epub(&doc, &opts).unwrap();
    assert_ne!(font_free, full);
    let result = package(&opts, "Content.");
    let expected = [
        fonts::body_bytes(opts.theme.font, FontStyle::Regular),
        fonts::body_bytes(opts.theme.font, FontStyle::Bold),
        fonts::body_bytes(opts.theme.font, FontStyle::Italic),
        fonts::body_bytes(opts.theme.font, FontStyle::BoldItalic),
        fonts::mono_bytes(FontStyle::Regular),
        fonts::symbol_bytes(),
    ];
    assert_eq!(result.resources.len(), expected.len());
    let book = franken_markdown::build_book(&[
        franken_markdown::BookInput {
            path: "first.md".into(),
            source: "# First\n\nContent.".into(),
        },
        franken_markdown::BookInput {
            path: "second.md".into(),
            source: "# Second\n\nLater chapter.".into(),
        },
    ])
    .unwrap();
    let full_book = epub::render_book_epub(&book, &opts).unwrap();
    for archive in [&full, &full_book] {
        let entries = entries(archive);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.name.starts_with("OEBPS/fonts/"))
                .count(),
            expected.len()
        );
        for (resource, source) in result.resources.iter().zip(expected) {
            assert_eq!(resource.bytes, source);
            assert_payload(&entries, &format!("OEBPS/{}", resource.href), source);
        }
    }
    assert_eq!(opts.font_assets, FontAssets::default());
    assert_eq!(full, epub::render_epub(&doc, &opts).unwrap());
    for bytes in [Vec::new(), b"not a font".to_vec()] {
        opts.font_assets.body_regular = Some(bytes);
        assert!(epub::render_epub(&doc, &opts).is_err());
    }
    opts = cff_options();
    opts.font_embedding = FontEmbedding::Full;
    let result = package(&opts, "Bounds");
    assert!(result.check_output([256 * 1024 * 1024]).is_err());
    assert!(result.check_output([usize::MAX]).is_err());
    assert!(result.check_output([1024]).is_ok());
    let mut saturated = Package {
        payload_bytes: MAX_TOTAL_FONT_BYTES,
        ..Package::default()
    };
    let error = saturated
        .face(
            Font::parse(SOURCE_SANS.to_vec()).unwrap(),
            &['A'],
            "Full",
            400,
            false,
            FontEmbedding::Full,
        )
        .unwrap_err();
    assert!(error.to_string().contains("full fonts exceed 128 MiB"));
    assert!(saturated.resources.is_empty());
}

#[test]
fn explicit_variable_weights_are_static_subsets_and_bold_shares_the_regular_source() {
    let mut opts = options();
    opts.font_assets = FontAssets::default()
        .with_slot(
            FontAssetSlot::BodyRegular,
            franken_markdown::text::variable_triangle_fixture(),
        )
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, 300)
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyBold, 800)
        .unwrap();
    let result = package(&opts, " ");
    assert_ne!(result.resources[0].bytes, result.resources[1].bytes);
    for resource in result.resources.iter().take(2) {
        assert!(
            Font::parse(resource.bytes.clone())
                .unwrap()
                .instance_bounds(*b"wght")
                .is_none()
        );
    }
    opts.font_assets
        .set_slot_weight(FontAssetSlot::BodyBold, 300)
        .unwrap();
    let same = package(&opts, " ");
    assert_eq!(same.css.matches("url(\"fonts/font-1.ttf\")").count(), 2);
    assert_ne!(result.fingerprint(), same.fingerprint());
}

#[test]
fn static_weight_pins_do_not_change_static_outline_payloads() {
    let mut opts = options();
    let normal = package(&opts, "Static");
    opts.font_assets
        .set_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
    let pinned = package(&opts, "Static");
    assert_eq!(normal.fingerprint(), pinned.fingerprint());
}

#[test]
fn repertoire_keeps_unicode_numeric_entities_and_generated_markers() {
    let mut repertoire = Repertoire::new(true);
    repertoire
        .add("<p>Δ café &#937; &#x1d400; &amp; &#xnot; &#55296;</p>")
        .unwrap();
    for ch in ['Δ', 'é', 'Ω', '𝐀', '&', '•', '↩'] {
        assert!(repertoire.characters.contains(&ch), "{ch}");
    }
    assert!(!repertoire.characters.contains(&'中'));
    let mut disabled = Repertoire::new(false);
    disabled.add("not retained").unwrap();
    assert!(disabled.characters.is_empty());
    assert_eq!(disabled.bytes, 0);
}

#[test]
fn repertoire_and_output_budgets_fail_without_a_truncated_subset() {
    let mut repertoire = Repertoire::new(true);
    repertoire.bytes = MAX_TEXT_BYTES;
    assert!(repertoire.add("x").is_err());
    let mut repertoire = Repertoire::new(true);
    repertoire.characters = (0..=0x10ffff)
        .filter_map(char::from_u32)
        .take(MAX_CHARACTERS)
        .collect();
    assert!(
        repertoire.insert('中').is_ok(),
        "already admitted character"
    );
    assert!(repertoire.insert('🦀').is_err());
    let result = package(&options(), "Bounds");
    assert!(result.check_output([256 * 1024 * 1024]).is_err());
    assert!(result.check_output([usize::MAX]).is_err());
    assert!(result.check_output([0, 1024]).is_ok());
}

#[test]
fn malformed_directly_assigned_faces_and_invalid_weights_cannot_be_ignored() {
    let doc = parse_markdown("# Validate before rendering");
    for bytes in [Vec::new(), b"not a font".to_vec()] {
        let mut opts = HtmlOptions::default();
        opts.font_assets.body_regular = Some(bytes);
        assert!(epub::render_epub(&doc, &opts).is_err());
    }
    let mut opts = options();
    opts.font_assets.body_bold_weight = Some(0);
    assert!(enabled(&opts).is_err());
    opts.font_assets.body_bold_weight = Some(400);
    opts.font_assets.body_regular = Some(vec![0; MAX_FONT_BYTES + 1]);
    assert!(enabled(&opts).is_err());
}

#[test]
fn stylesheet_order_respects_custom_replacement_in_both_document_shells() {
    let result = package(&options(), "Style");
    for custom in [false, true] {
        let mut chapter = epub::chapter_xhtml("Style", "en", "<p>Text</p>");
        result.link(&mut chapter, custom).unwrap();
        let mut nav = epub::nav_xhtml("Style", "en", &parse_markdown("# Heading"));
        result.link(&mut nav, custom).unwrap();
        for xhtml in [chapter, nav] {
            assert_eq!(xhtml.matches(FONT_LINK).count(), 1);
            assert_eq!(xhtml.matches(STYLE_LINK).count(), 1);
            assert_eq!(
                xhtml.find(FONT_LINK).unwrap() < xhtml.find(STYLE_LINK).unwrap(),
                custom
            );
            assert!(xhtml.find(FONT_LINK).unwrap() < xhtml.find("</head>").unwrap());
        }
    }
    assert!(result.link(&mut "not XHTML".into(), false).is_err());
}

#[test]
fn font_manifest_entries_are_resources_not_spine_items() {
    let result = package(&options(), "Manifest");
    let mut opf = "<manifest></manifest><spine><itemref idref=\"chapter-1\"/></spine>".to_string();
    result.manifest(&mut opf).unwrap();
    assert_eq!(
        opf.matches("media-type=\"font/ttf\"").count(),
        result.resources.len()
    );
    assert_eq!(opf.matches("id=\"epub-font-css\"").count(), 1);
    assert!(!opf.split_once("<spine>").unwrap().1.contains("epub-font"));
    assert!(result.manifest(&mut String::new()).is_err());
}

struct Entry<'a> {
    name: &'a str,
    method: u16,
    crc: u32,
    size: usize,
    payload: &'a [u8],
}
fn entries(bytes: &[u8]) -> Vec<Entry<'_>> {
    let u16_at = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut result = Vec::new();
    let mut at = 0;
    while bytes.get(at..at + 4) == Some(b"PK\x03\x04".as_slice()) {
        assert_eq!(u16_at(at + 6) & 8, 0);
        let name_end = at + 30 + usize::from(u16_at(at + 26));
        let start = name_end + usize::from(u16_at(at + 28));
        let end = start + u32_at(at + 18) as usize;
        result.push(Entry {
            name: std::str::from_utf8(&bytes[at + 30..name_end]).unwrap(),
            method: u16_at(at + 8),
            crc: u32_at(at + 14),
            size: u32_at(at + 22) as usize,
            payload: &bytes[start..end],
        });
        at = end;
    }
    assert_eq!(bytes.get(at..at + 4), Some(b"PK\x01\x02".as_slice()));
    assert_eq!(result[0].name, "mimetype");
    assert_eq!(result[0].method, 0);
    assert_eq!(result[0].payload, b"application/epub+zip");
    assert_eq!(
        result
            .iter()
            .map(|entry| entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        result.len()
    );
    result
}

fn assert_payload(entries: &[Entry<'_>], name: &str, expected: &[u8]) {
    let entry = entries
        .iter()
        .find(|entry| entry.name == name)
        .expect("archive resource exists");
    assert_eq!(entry.size, expected.len(), "{name}");
    assert_eq!(entry.crc, crc32(expected), "{name}");
    if entry.method == 0 {
        assert_eq!(entry.payload, expected);
        return;
    }
    assert_eq!(entry.method, 8);
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in expected {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    let mut zlib = vec![0x78, 0x9c];
    zlib.extend_from_slice(entry.payload);
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    assert_eq!(
        zlib_decompress(&zlib, expected.len()).as_deref(),
        Some(expected),
        "{name}"
    );
}

#[test]
fn single_document_writes_actual_subsets_css_and_manifest_into_the_archive() {
    let opts = options();
    let doc = parse_markdown("# Résumé\n\nRegular, **bold**, *italic*, ***both*** and `code`.\n");
    let package = for_document(&doc, &opts);
    let output = epub::render_epub(&doc, &opts).unwrap();
    let entries = entries(&output);
    assert_payload(&entries, "OEBPS/embedded-fonts.css", package.css.as_bytes());
    assert_payload(
        &entries,
        "OEBPS/style.css",
        epub::theme::stylesheet(&opts).as_bytes(),
    );
    for resource in &package.resources {
        assert_payload(
            &entries,
            &format!("OEBPS/{}", resource.href),
            &resource.bytes,
        );
    }
    let mut html_opts = opts.clone();
    html_opts.custom_css = Some(String::new());
    let html = franken_markdown::html::render(&doc, &html_opts);
    let original = epub::html_fragment_to_xhtml(epub::extract_main_body(&html).unwrap());
    let prepared = epub::resources::prepare(&original).unwrap();
    let base_id = epub::content_identifier("Résumé", "en", &original);
    let base_id = epub::content_identifier(
        &base_id,
        "epub-stylesheet-v1",
        &epub::theme::stylesheet(&opts),
    );
    let id = epub::content_identifier(&base_id, "epub-fonts-v1", &package.fingerprint().unwrap());
    let mut opf = epub::content_opf("Résumé", "en", &id, &prepared);
    package.manifest(&mut opf).unwrap();
    assert_payload(&entries, "OEBPS/content.opf", opf.as_bytes());
    let mut chapter = epub::chapter_xhtml("Résumé", "en", &prepared.body);
    package.link(&mut chapter, false).unwrap();
    assert_payload(&entries, "OEBPS/chapter-1.xhtml", chapter.as_bytes());
    let mut nav = epub::nav_xhtml("Résumé", "en", &doc);
    package.link(&mut nav, false).unwrap();
    assert_payload(&entries, "OEBPS/nav.xhtml", nav.as_bytes());
    assert_eq!(output, epub::render_epub(&doc, &opts).unwrap());
}

#[test]
fn custom_stylesheet_bytes_are_not_rewritten_or_combined_with_generated_css() {
    let doc = parse_markdown("# Style\n\nBody\n");
    for css in ["", "/* author CSS */\nbody{font-family:fantasy;}\n"] {
        let mut opts = options();
        opts.custom_css = Some(css.into());
        let output = epub::render_epub(&doc, &opts).unwrap();
        assert_payload(&entries(&output), "OEBPS/style.css", css.as_bytes());
    }
}

#[test]
fn different_embedded_faces_change_identity_and_never_mutate_input_assets() {
    let doc = parse_markdown("# Fonts\n\nTypography\n");
    let mut opts = options();
    let before = opts.font_assets.clone();
    let first = epub::render_epub(&doc, &opts).unwrap();
    assert_eq!(before, opts.font_assets);
    let first_package = for_document(&doc, &opts);
    opts.font_assets
        .set_slot(
            FontAssetSlot::BodyRegular,
            fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec(),
        )
        .unwrap();
    let second = epub::render_epub(&doc, &opts).unwrap();
    assert_ne!(first, second);
    assert_ne!(
        first_package.fingerprint(),
        for_document(&doc, &opts).fingerprint()
    );
}
