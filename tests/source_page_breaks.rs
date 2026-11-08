//! Authors can request existing page boundaries without losing comment source.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::borrow::Cow;

use franken_markdown::book::{BookInput, BookRenderer, build_book, render_book_pdf};
use franken_markdown::{
    Block, DisplayBlock, Document, HtmlOptions, Inline, PdfASettings, PdfEmitOptions, PdfOptions,
    Profile, ResumableFlowDisplay, apply_profile, epub, html, parse_markdown,
    parse_markdown_spanned, render_html_document, render_html_document_to_writer,
    render_pdf_document, render_pdf_document_emitted, render_pdf_document_pdfa,
    render_pdf_document_profiled, render_pdf_document_reported, verify, wasm,
};

const MARKER: &str = "<!-- pagebreak -->";
const SOURCE: &str = "ALPHAPROOF.\n\n<!-- pagebreak -->\n\nBETAPROOF.\n";

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.into())])
}

fn expected_document() -> Document {
    Document {
        blocks: vec![
            paragraph("ALPHAPROOF."),
            Block::PageBreak,
            paragraph("BETAPROOF."),
        ],
    }
}

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn page_breaks(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|block| match block {
            Block::PageBreak => 1,
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                page_breaks(inner)
            }
            Block::List(list) => list
                .items
                .iter()
                .map(|item| page_breaks(&item.blocks))
                .sum(),
            _ => 0,
        })
        .sum()
}

#[test]
fn complete_physical_comment_lines_activate_only_in_extended_profiles() {
    for marker in [
        MARKER.to_string(),
        format!(" {MARKER}"),
        format!("   {MARKER} \t"),
        format!("{MARKER}\n"),
        format!("{MARKER}\r\n"),
        format!("{MARKER}\r"),
        format!("  {MARKER}\t \r\n"),
    ] {
        let document = Document {
            blocks: vec![Block::HtmlBlock(marker)],
        };
        let original = document.clone();
        for profile in [None, Some(Profile::GfmPlus)] {
            assert_eq!(apply_profile(&document, profile).blocks, [Block::PageBreak]);
        }
        let commonmark = apply_profile(&document, Some(Profile::CommonMarkGfm));
        assert!(matches!(commonmark, Cow::Borrowed(_)));
        assert_eq!(commonmark.as_ref(), &original);
        assert_eq!(document, original);
    }
    let native = expected_document();
    for profile in [None, Some(Profile::GfmPlus), Some(Profile::CommonMarkGfm)] {
        assert!(matches!(apply_profile(&native, profile), Cow::Borrowed(_)));
        assert_eq!(page_breaks(&apply_profile(&native, profile).blocks), 1);
    }
}

#[test]
fn partial_multiline_or_unknown_comments_stay_ordinary_html() {
    for marker in [
        "<!--pagebreak-->",
        "<!-- PAGEBREAK -->",
        "<!-- pagebreak: next -->",
        "<!-- pagebreak --> after",
        "before <!-- pagebreak -->",
        "<!-- pagebreak --> <!-- more -->",
        "<!--\npagebreak\n-->",
        "<!-- pagebreak -->\n\n",
        "\n<!-- pagebreak -->",
        "<!-- pagebreak -->\r\n \r\n",
        "<!-- pagebreak",
        "    <!-- pagebreak -->",
        "\t<!-- pagebreak -->",
        "<!-- pagebreak -->\u{a0}",
        "<div>\n<!-- pagebreak -->\n</div>",
    ] {
        let document = Document {
            blocks: vec![Block::HtmlBlock(marker.into())],
        };
        for profile in [None, Some(Profile::GfmPlus), Some(Profile::CommonMarkGfm)] {
            let profiled = apply_profile(&document, profile);
            assert!(matches!(profiled, Cow::Borrowed(_)), "{marker:?}");
            assert_eq!(profiled.as_ref(), &document);
        }
    }
}

#[test]
fn inline_code_math_table_and_enclosing_html_contexts_are_not_directives() {
    for source in [
        "Before <!-- pagebreak --> after.",
        "`<!-- pagebreak -->`",
        "```html\n<!-- pagebreak -->\n```",
        "    <!-- pagebreak -->",
        "\\<!-- pagebreak -->",
        "<div>\n<!-- pagebreak -->\n</div>",
        "<pre>\n<!-- pagebreak -->\n</pre>",
        "<!-- unrelated\n<!-- pagebreak -->\n-->",
        "| Control |\n| --- |\n| <!-- pagebreak --> |",
        "$$\n<!-- pagebreak -->\n$$",
    ] {
        let document = parse_markdown(source);
        assert_eq!(
            page_breaks(&apply_profile(&document, None).blocks),
            0,
            "{source}"
        );
        assert!(
            !html::render_fragment(&document.blocks, &HtmlOptions::default())
                .contains("fmd-pagebreak"),
            "{source}"
        );
    }
}

#[test]
fn parser_boundaries_diagnostics_and_original_utf8_spans_are_preserved() {
    for newline in ["\n", "\r\n", "\r"] {
        // HTML comments can already interrupt a paragraph without blank lines.
        let source = format!("α before.{newline} {MARKER}\t{newline}β after.{newline}");
        let document = parse_markdown_spanned(&source);
        assert!(document.diagnostics.is_empty());
        assert_eq!(document.blocks().len(), 3);
        assert!(
            matches!(&document.blocks()[1].node, Block::HtmlBlock(comment) if comment == &format!(" {MARKER}\t"))
        );
        let span = document.source_span_for_block(1).unwrap();
        assert_eq!(&source[span.start..span.end], format!(" {MARKER}\t"));
        assert!(source.is_char_boundary(span.start) && source.is_char_boundary(span.end));
        let plain = document.to_document();
        assert_eq!(plain, parse_markdown(&source));
        let before = plain.clone();
        assert_eq!(page_breaks(&apply_profile(&plain, None).blocks), 1);
        assert_eq!(plain, before);
        assert_eq!(document.source_span_for_block(1), Some(span));
    }
}

#[test]
fn quotes_lists_and_note_bodies_keep_their_own_boundaries() {
    let source = "> Quote before.\n>\n> <!-- pagebreak -->\n>\n> Quote after.\n\n\
                  - Item before.\n\n  <!-- pagebreak -->\n\n  Item after.\n\n\
                  Claim.[^n]\n\n[^n]: Note before.\n\n    <!-- pagebreak -->\n\n    Note after.\n";
    let document = parse_markdown(source);
    let original = document.clone();
    let extended = apply_profile(&document, Some(Profile::GfmPlus));
    assert_eq!(page_breaks(&extended.blocks), 3);
    assert!(
        matches!(&extended.blocks[0], Block::BlockQuote(inner) if inner[1] == Block::PageBreak)
    );
    assert!(
        matches!(&extended.blocks[1], Block::List(list) if list.items[0].blocks[1] == Block::PageBreak)
    );
    assert!(
        matches!(&extended.blocks[3], Block::FootnoteDefinition { blocks, .. } if blocks[1] == Block::PageBreak)
    );
    assert_eq!(
        page_breaks(&apply_profile(&document, Some(Profile::CommonMarkGfm)).blocks),
        0
    );
    assert_eq!(document, original);
}

#[test]
fn html_string_writer_and_fragment_share_profile_behavior() {
    let document = parse_markdown(SOURCE);
    let native = expected_document();
    for profile in [None, Some(Profile::GfmPlus)] {
        let options = HtmlOptions {
            profile,
            custom_css: Some(String::new()),
            ..Default::default()
        };
        let actual = render_html_document(&document, &options).unwrap();
        assert_eq!(actual, render_html_document(&native, &options).unwrap());
        let mut written = Vec::new();
        assert_eq!(
            render_html_document_to_writer(&document, &options, &mut written).unwrap(),
            actual.len()
        );
        assert_eq!(written, actual.as_bytes());
        assert_eq!(
            html::render_fragment(&document.blocks, &options),
            html::render_fragment(&native.blocks, &options)
        );
        assert_eq!(actual.matches("class=\"fmd-pagebreak\"").count(), 1);
        assert!(!actual.contains(MARKER));
    }
    let mixed = parse_markdown("Term\n: Definition.\n\n<!-- pagebreak -->\n");
    for allow_raw_html in [false, true] {
        let options = HtmlOptions {
            profile: Some(Profile::CommonMarkGfm),
            allow_raw_html,
            custom_css: Some(String::new()),
            ..Default::default()
        };
        let fragment = html::render_fragment(&mixed.blocks, &options);
        assert!(!fragment.contains("<dl>"));
        assert!(!fragment.contains("fmd-pagebreak"));
        assert!(fragment.contains(": Definition."));
        assert_eq!(fragment.contains(MARKER), allow_raw_html);
        let full = render_html_document(&mixed, &options).unwrap();
        assert!(full.contains(&fragment));
    }
}

#[test]
fn svg_uses_continuous_boundaries_and_preserves_explicit_profiles_and_resources() {
    use franken_markdown::svg::{
        SvgOptions, render_svg, render_svg_with_diagnostics, render_svg_with_report,
        render_svg_with_resources,
    };

    let options = SvgOptions::default();
    let fonts = franken_markdown::FontAssets {
        body_regular: Some(
            franken_markdown::fonts::mono_bytes(franken_markdown::fonts::FontStyle::Regular)
                .to_vec(),
        ),
        ..Default::default()
    };
    let assets = [franken_markdown::PdfImageAsset::new(
        "figure.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#0a7890"/></svg>"##.to_vec(),
    )];
    let image = Block::Paragraph(vec![Inline::Image {
        dest: "figure.svg".into(),
        title: None,
        alt: "Plot".into(),
    }]);
    for nested in [false, true] {
        let source = if nested {
            "> ALPHAPROOF.\n>\n> <!-- pagebreak -->\n>\n> ![Plot](figure.svg)\n>\n> BETAPROOF.\n"
        } else {
            "ALPHAPROOF.\n\n<!-- pagebreak -->\n\n![Plot](figure.svg)\n\nBETAPROOF.\n"
        };
        let document = parse_markdown(source);
        let wrap = |blocks| Document {
            blocks: if nested {
                vec![Block::BlockQuote(blocks)]
            } else {
                blocks
            },
        };
        let native = wrap(vec![
            paragraph("ALPHAPROOF."),
            Block::PageBreak,
            image.clone(),
            paragraph("BETAPROOF."),
        ]);
        // A literal code example must remain visible. Complete HTML comments,
        // including CommonMark's ordinary marker comment, are removed by the
        // existing safe-HTML preparation used by SVG's endnote pass.
        let literal = wrap(vec![
            paragraph("ALPHAPROOF."),
            Block::CodeBlock {
                lang: None,
                code: MARKER.into(),
            },
            image.clone(),
            paragraph("BETAPROOF."),
        ]);
        let default = render_svg_with_diagnostics(&document, &options);
        assert_eq!(default, render_svg_with_diagnostics(&native, &options));
        assert_eq!(render_svg(&document, &options), default.0);
        assert_eq!(
            render_svg_with_report(&document, &options),
            (default.0, default.1)
        );
        let expected = render_svg_with_resources(&native, &options, &fonts, &assets).unwrap();
        assert_eq!(
            render_svg_with_resources(&document, &options, &fonts, &assets).unwrap(),
            expected
        );
        for profile in [None, Some(Profile::GfmPlus)] {
            let profiled = apply_profile(&document, profile);
            assert_eq!(
                render_svg_with_resources(&profiled, &options, &fonts, &assets).unwrap(),
                expected
            );
        }
        let profiled = apply_profile(&document, Some(Profile::CommonMarkGfm));
        let commonmark = render_svg_with_resources(&profiled, &options, &fonts, &assets).unwrap();
        assert!(
            commonmark == expected,
            "ordinary comments add no poster ink"
        );
        assert!(
            commonmark.0
                != render_svg_with_resources(&literal, &options, &fonts, &assets)
                    .unwrap()
                    .0,
            "literal code examples remain visible"
        );
        assert!(expected.2.is_empty());
        assert_ne!(
            expected.0,
            render_svg_with_resources(
                &native,
                &options,
                &franken_markdown::FontAssets::default(),
                &assets,
            )
            .unwrap()
            .0
        );
        assert!(String::from_utf8(expected.0).unwrap().contains("<image"));
    }
}

#[cfg(feature = "wasm-bindgen")]
#[test]
fn browser_svg_keeps_the_selected_authoring_profile() {
    let document = parse_markdown(SOURCE);
    for profile in [None, Some(Profile::GfmPlus), Some(Profile::CommonMarkGfm)] {
        let actual = franken_markdown::wasm_abi::render_svg_configured_profiled(
            SOURCE,
            None,
            None,
            None,
            Some(420.0),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            profile.map(|value| value.as_str().to_owned()),
        )
        .unwrap();
        let profiled = apply_profile(&document, profile);
        let expected = franken_markdown::svg::render_svg_with_resources(
            &profiled,
            &franken_markdown::SvgOptions {
                max_width_pt: 420.0,
                ..Default::default()
            },
            &franken_markdown::FontAssets::default(),
            &[],
        )
        .unwrap();
        assert_eq!(actual.bytes(), expected.0);
        assert_eq!(actual.source_length(), SOURCE.len());
    }
}

#[test]
fn source_controls_use_the_real_pdf_page_boundary_in_every_entrypoint() {
    let document = parse_markdown(SOURCE);
    let native = expected_document();
    let options = PdfOptions::default();
    let expected = render_pdf_document(&native, &options).unwrap();
    assert!(render_pdf_document(&document, &options).unwrap() == expected);
    for emit in [PdfEmitOptions::default(), PdfEmitOptions::monolithic()] {
        assert!(render_pdf_document_emitted(&document, &options, emit).unwrap() == expected);
        let reported = render_pdf_document_reported(&document, &options, emit).unwrap();
        assert!(reported.bytes == expected);
    }
    assert!(
        render_pdf_document_profiled(&document, &options)
            .unwrap()
            .bytes
            == expected
    );
    for settings in [PdfASettings::OFF, PdfASettings::a2b_strict()] {
        assert!(
            render_pdf_document_pdfa(&document, &options, settings).unwrap()
                == render_pdf_document_pdfa(&native, &options, settings).unwrap()
        );
    }
    let report = verify::verify_pdf_result(&document, &options).unwrap();
    assert_eq!(report.page_count, 2);
    assert!(
        report.pages[0]
            .runs
            .iter()
            .any(|run| run.text.contains("ALPHAPROOF"))
    );
    assert!(
        report.pages[1]
            .runs
            .iter()
            .any(|run| run.text.contains("BETAPROOF"))
    );
    assert!(
        report
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .all(|run| !run.text.contains("pagebreak"))
    );
    let commonmark = PdfOptions {
        profile: Some(Profile::CommonMarkGfm),
        ..Default::default()
    };
    assert_eq!(
        verify::verify_pdf_result(&document, &commonmark)
            .unwrap()
            .page_count,
        1
    );
    assert_eq!(
        verify::verify_pdf_result(&native, &commonmark)
            .unwrap()
            .page_count,
        2
    );
}

#[test]
fn leading_trailing_and_repeated_controls_do_not_manufacture_blank_pdf_pages() {
    let source =
        format!("{MARKER}\n\nALPHAPROOF.\n\n{MARKER}\n{MARKER}\n\nBETAPROOF.\n\n{MARKER}\n");
    let document = parse_markdown(&source);
    for strict_pagination in [false, true] {
        let options = PdfOptions {
            strict_pagination,
            ..Default::default()
        };
        assert!(
            render_pdf_document(&document, &options).unwrap()
                == render_pdf_document(&expected_document(), &options).unwrap()
        );
        assert_eq!(
            verify::verify_pdf_result(&document, &options)
                .unwrap()
                .page_count,
            2
        );
    }
}

#[test]
fn epub_and_browser_adapters_share_the_same_authoring_pass() {
    let document = parse_markdown(SOURCE);
    let native = expected_document();
    for profile in [None, Some(Profile::GfmPlus), Some(Profile::CommonMarkGfm)] {
        let html_options = HtmlOptions {
            profile,
            custom_css: Some(String::new()),
            ..Default::default()
        };
        let pdf_options = PdfOptions {
            profile,
            ..Default::default()
        };
        let browser = wasm::WasmRenderOptions {
            profile,
            custom_css: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(
            wasm::render_html(SOURCE, &browser).unwrap().bytes,
            render_html_document(&document, &html_options)
                .unwrap()
                .as_bytes()
        );
        assert!(
            wasm::render_pdf(SOURCE, &browser).unwrap().bytes
                == render_pdf_document(&document, &pdf_options).unwrap()
        );
        let expected = if profile == Some(Profile::CommonMarkGfm) {
            Document {
                blocks: vec![paragraph("ALPHAPROOF."), paragraph("BETAPROOF.")],
            }
        } else {
            native.clone()
        };
        assert!(
            epub::render_epub(&document, &html_options).unwrap()
                == epub::render_epub(&expected, &html_options).unwrap()
        );
    }
}

#[test]
fn chapter_scoped_navigation_and_epub_keep_authored_breaks_before_html_lowering() {
    let sources = [
        input(
            "one.md",
            "# One\n\n[Next](two.md#two)\n\n<!-- pagebreak -->\n\n## After\n\nAfter proof.\n",
        ),
        input("two.md", "# Two\n\n[Back](one.md#after)\n"),
    ];
    let book = build_book(&sources).unwrap();
    let mut native = build_book(&[
        input(
            "one.md",
            "# One\n\n[Next](two.md#two)\n\n## After\n\nAfter proof.\n",
        ),
        sources[1].clone(),
    ])
    .unwrap();
    native.chapters[0].doc.blocks.insert(2, Block::PageBreak);
    let renderer = BookRenderer::new(&sources).unwrap();
    for profile in [None, Some(Profile::GfmPlus)] {
        let pdf_options = PdfOptions {
            profile,
            ..Default::default()
        };
        let actual = renderer
            .render_pdf_with_options(&pdf_options, PdfASettings::OFF)
            .unwrap();
        assert!(actual == render_book_pdf(&native, &pdf_options).unwrap());
        assert!(
            !actual
                .windows(b"fmd-book-pdf-nav/".len())
                .any(|part| part == b"fmd-book-pdf-nav/")
        );
        let html_options = HtmlOptions {
            profile,
            custom_css: Some(String::new()),
            ..Default::default()
        };
        assert!(
            epub::render_book_epub(&book, &html_options).unwrap()
                == epub::render_book_epub(&native, &html_options).unwrap()
        );
    }
    native.chapters[0].doc.blocks.remove(2);
    let commonmark = PdfOptions {
        profile: Some(Profile::CommonMarkGfm),
        ..Default::default()
    };
    assert!(
        renderer
            .render_pdf_with_options(&commonmark, PdfASettings::OFF)
            .unwrap()
            == render_book_pdf(&native, &commonmark).unwrap()
    );
    let commonmark_html = HtmlOptions {
        profile: Some(Profile::CommonMarkGfm),
        custom_css: Some(String::new()),
        ..Default::default()
    };
    assert!(
        epub::render_book_epub(&book, &commonmark_html).unwrap()
            == epub::render_book_epub(&native, &commonmark_html).unwrap()
    );
}

#[test]
fn include_sources_can_request_a_boundary_without_becoming_chapters() {
    let chapters = [input(
        "guide.md",
        "# Guide\n\nBefore proof.\n\n{{#include parts/break.md}}\n\nAfter proof.\n",
    )];
    let mut included = BookRenderer::from_sources(
        &chapters,
        &[input("parts/break.md", "<!-- pagebreak -->\n")],
    )
    .unwrap();
    let mut inline = BookRenderer::new(&[input(
        "guide.md",
        "# Guide\n\nBefore proof.\n\n<!-- pagebreak -->\n\nAfter proof.\n",
    )])
    .unwrap();
    for profile in [None, Some(Profile::GfmPlus), Some(Profile::CommonMarkGfm)] {
        included.options_mut().profile = profile;
        inline.options_mut().profile = profile;
        included.options_mut().custom_css = Some(String::new());
        inline.options_mut().custom_css = Some(String::new());
        assert!(included.render_pdf().unwrap() == inline.render_pdf().unwrap());
        assert!(included.render_epub().unwrap() == inline.render_epub().unwrap());
        assert_eq!(included.book().chapters.len(), 1);
    }
}

#[test]
fn flow_and_reading_maps_keep_the_original_marker_range() {
    let source = "α before.\r\n\r\n<!-- pagebreak -->\r\n\r\nβ after.\r\n";
    let parsed = parse_markdown_spanned(source);
    let span = parsed.source_span_for_block(1).unwrap();
    let map = parsed.source_map(source).unwrap();
    assert!(map.rendered_text().contains("--- PAGE BREAK ---"));
    assert!(!map.rendered_text().contains(MARKER));
    assert_eq!(map.source_len(), source.len());
    let mut flow = ResumableFlowDisplay::new(source, 1);
    while flow.step().unwrap().is_some() {}
    let rule = flow
        .blocks()
        .iter()
        .position(|block| matches!(block, DisplayBlock::Rule))
        .unwrap();
    assert_eq!(flow.source_span_for_block(rule), Some(span));
    assert_eq!(
        flow.blocks()
            .iter()
            .filter(|block| matches!(block, DisplayBlock::Rule))
            .count(),
        1
    );
    assert_eq!(flow.source(), source);
    assert_eq!(&source[span.start..span.end], MARKER);
}

#[test]
fn nested_boundaries_do_not_capture_the_following_proses_source() {
    const AFTER: &str = "pagebreak";
    for source in [
        "> Literal `<!-- pagebreak -->`.\n>\n> <!-- pagebreak -->\n>\n> pagebreak\n",
        "- Literal `<!-- pagebreak -->`.\n\n  <!-- pagebreak -->\n\n  pagebreak\n",
        "Claim.[^n]\n\n[^n]: Literal `<!-- pagebreak -->`.\n\n    <!-- pagebreak -->\n\n    pagebreak\n",
        "> ![diagram](image.png \"<!-- pagebreak -->\")\n>\n> <!-- pagebreak -->\n>\n> pagebreak\n",
        "- ![diagram](image.png \"<!-- pagebreak -->\")\n\n  <!-- pagebreak -->\n\n  pagebreak\n",
        "Claim.[^n]\n\n[^n]: ![diagram](image.png \"<!-- pagebreak -->\")\n\n    <!-- pagebreak -->\n\n    pagebreak\n",
        "- [ ] <!-- pagebreak -->\n\n  pagebreak\n",
        "- [x] <!-- pagebreak -->\n\n  pagebreak\n",
        "- [X] <!-- pagebreak -->\n\n  pagebreak\n",
    ] {
        let parsed = parse_markdown_spanned(source);
        let map = parsed.source_map(source).unwrap();
        let element = map
            .elements()
            .iter()
            .find(|element| element.rendered_text == AFTER)
            .expect("the paragraph after the control remains mapped");
        let start = source.rfind(AFTER).unwrap();
        assert_eq!(
            element.source_ranges.spans(),
            &[franken_markdown::span::SourceSpan::new(
                start,
                start + AFTER.len()
            )]
        );
        assert_eq!(map.rendered_text().matches("--- PAGE BREAK ---").count(), 1);
    }
}
