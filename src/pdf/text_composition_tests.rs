#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn canonical_latin_pdf_metrics_and_glyphs_match_precomposed_text() {
    for family in [crate::FontFamily::Sans, crate::FontFamily::Serif] {
        let mut opts = PdfOptions::default();
        opts.theme.font = family;
        let faces = Faces::load(&opts).unwrap();
        for slot in [F_BODY, F_BOLD, F_ITALIC, F_BOLDITALIC, F_MONO] {
            let face = faces.face(slot);
            let source = "officecafe\u{0301} A\u{030a}ngstro\u{0308}m";
            let composed = "officecafé Ångström";
            assert_eq!(
                face.shaped_width(source, font_size_of(11.0)),
                face.shaped_width(composed, font_size_of(11.0)),
                "family={family:?}, slot={slot}"
            );
            let actual = shape_run(&face.font, &face.lig, face.ascii_tables(), source);
            let expected = shape_run(&face.font, &face.lig, face.ascii_tables(), composed);
            assert_eq!(
                actual.glyphs, expected.glyphs,
                "family={family:?}, slot={slot}"
            );
            assert_eq!(actual.text, source);
            assert!(!actual.glyphs.contains(&0));
        }
    }
}

#[test]
fn canonical_latin_pdf_warning_does_not_report_supported_marks_missing() {
    let doc =
        crate::parse_markdown("# Cafe\u{0301}\n\n**A\u{030a}ngstro\u{0308}m** `nai\u{0308}ve`.");
    for family in [crate::FontFamily::Sans, crate::FontFamily::Serif] {
        let mut opts = PdfOptions::default();
        opts.theme.font = family;
        assert!(
            !render_warnings(&doc, &opts)
                .iter()
                .any(|warning| { matches!(warning, RenderWarning::MissingGlyphs { .. }) })
        );
    }
}

#[test]
fn canonical_latin_warnings_respect_the_actual_host_font_style() {
    let regular = fonts::body_font(crate::FontFamily::Sans, FontStyle::Regular)
        .unwrap()
        .subset(&['e', ' '])
        .unwrap();
    let opts = PdfOptions {
        font_assets: FontAssets {
            body_regular: Some(regular),
            ..FontAssets::default()
        },
        page_numbers: false,
        ..PdfOptions::default()
    };
    let regular_doc = crate::parse_markdown("e\u{0301}");
    let warnings = render_warnings(&regular_doc, &opts);
    let missing = warnings.iter().find_map(|warning| match warning {
        RenderWarning::MissingGlyphs { count, sample } => Some((*count, sample.as_str())),
        _ => None,
    });
    assert_eq!(missing, Some((1, "\u{0301}")));
    let regular_pdf = crate::render_pdf_document(&regular_doc, &opts).unwrap();
    assert!(!composition_page_content(&regular_pdf).contains("/ActualText"));

    // A static host regular face does not replace the independent bundled
    // bold face. Its composite is available, so the same source now paints
    // correctly and must not inherit the regular face's missing-mark warning.
    let bold_doc = crate::parse_markdown("**e\u{0301}**");
    assert!(
        !render_warnings(&bold_doc, &opts)
            .iter()
            .any(|warning| { matches!(warning, RenderWarning::MissingGlyphs { .. }) })
    );
    let bold_pdf = crate::render_pdf_document(&bold_doc, &opts).unwrap();
    assert!(composition_page_content(&bold_pdf).contains("/ActualText <FEFF00650301>"));
}

#[test]
fn positioned_svg_elements_do_not_suppress_an_orphan_mark_warning() {
    for separate_elements in [false, true] {
        let text = if separate_elements {
            "<text x=\"10\" y=\"25\">e</text><text x=\"20\" y=\"25\">\u{0301}</text>"
        } else {
            "<text x=\"10\" y=\"25\">e\u{0301}</text>"
        };
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"80\" height=\"40\">{text}</svg>"
        );
        let opts = PdfOptions {
            image_assets: vec![crate::PdfImageAsset::new(
                "positioned.svg",
                svg.into_bytes(),
            )],
            page_numbers: false,
            ..PdfOptions::default()
        };
        let doc = crate::parse_markdown("![Positioned label](positioned.svg)");
        let warnings = render_warnings(&doc, &opts);
        let missing = warnings.iter().find_map(|warning| match warning {
            RenderWarning::MissingGlyphs { count, sample } => Some((*count, sample.as_str())),
            _ => None,
        });
        // A separately positioned mark never composes with the preceding base,
        // so it is reported exactly when the selected face cannot draw it alone.
        let faces = Faces::load(&opts).unwrap();
        let face_lacks_mark = {
            let image = parse_pdf_image_asset("positioned.svg", &opts.image_assets[0].bytes)
                .expect("positioned svg parses");
            let mut runs = Vec::new();
            collect_svg_image_text(&image, &mut runs);
            let (slot, _) = runs
                .iter()
                .find(|(_, text)| text.contains('\u{0301}'))
                .expect("the mark is collected");
            let face = faces.get(*slot);
            let gid = face.glyph_index('\u{0301}');
            gid == 0 || gid >= face.num_glyphs
        };
        assert_eq!(
            missing,
            (separate_elements && face_lacks_mark).then_some((1, "\u{0301}"))
        );
        let pdf = crate::render_pdf_document(&doc, &opts).unwrap();
        let content = composition_page_content(&pdf);
        assert_eq!(
            content.contains("/ActualText <FEFF00650301>"),
            !separate_elements
        );
        if separate_elements {
            assert!(!content.contains("/ActualText"));
        }
    }
}

fn assert_layout_geometry_matches(actual: &[Line], expected: &[Line]) {
    assert_eq!(actual.len(), expected.len(), "physical line count");
    for (line_index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual.size, expected.size, "line {line_index}: size");
        assert_eq!(
            actual.gap_after, expected.gap_after,
            "line {line_index}: gap"
        );
        assert_eq!(
            actual.table_cols, expected.table_cols,
            "line {line_index}: cells"
        );
        assert_eq!(
            actual.segs.len(),
            expected.segs.len(),
            "line {line_index}: runs"
        );
        for (actual, expected) in actual.segs.iter().zip(&expected.segs) {
            assert_eq!(actual.slot, expected.slot, "line {line_index}: face");
            assert_eq!(actual.fill, expected.fill);
            assert_eq!(actual.link, expected.link);
            assert_eq!(actual.strike, expected.strike);
            assert_eq!(actual.expansion_permille, expected.expansion_permille);
            assert!(
                (actual.x - expected.x).abs() < 0.001
                    && (actual.width - expected.width).abs() < 0.001,
                "line {line_index}, {:?}: x/width ({}, {}) != ({}, {})",
                actual.text,
                actual.x,
                actual.width,
                expected.x,
                expected.width
            );
        }
    }
}

#[test]
fn same_style_ast_fragments_keep_the_accent_with_its_source_base() {
    let opts = PdfOptions::default();
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let suffix = Inline::Strong(vec![Inline::Text("TAIL".into())]);
    let split = [Block::Paragraph(vec![
        Inline::Text("officecaf".into()),
        Inline::Text("e".into()),
        Inline::Text("\u{0301}".into()),
        suffix.clone(),
    ])];
    let whole = [Block::Paragraph(vec![
        Inline::Text("officecafe\u{0301}".into()),
        suffix,
    ])];
    let actual = layout(&split, &opts, &faces, page);
    let expected = layout(&whole, &opts, &faces, page);
    assert_layout_geometry_matches(&actual, &expected);
    let segments: Vec<_> = actual.iter().flat_map(|line| &line.segs).collect();
    assert_eq!(segments.len(), 2, "one body run followed by one bold run");
    assert_eq!(segments[0].text, "officecafe\u{0301}");
    assert_eq!(segments[0].slot, F_BODY);
    let drawn = faces.shaped_width_points(F_BODY, "officecafé", actual[0].size);
    assert!((segments[1].x - segments[0].x - drawn).abs() < 0.001);
}

#[test]
fn marked_headings_styled_paragraphs_tables_and_code_match_nfc_geometry() {
    let canonical = "# Café\n\nCafé **Å** *ö* ***é*** [Café](https://example.com) \
        ~~Café~~ and `naïve`.\n\n| Café | Å |\n| --- | --- |\n| Café | **é** |\n\n\
        ```\ncafé\n```\n";
    let source = canonical
        .replace('é', "e\u{0301}")
        .replace('Å', "A\u{030a}")
        .replace('ö', "o\u{0308}")
        .replace('ï', "i\u{0308}");
    let actual_doc = crate::parse_markdown(&source);
    let expected_doc = crate::parse_markdown(canonical);
    for family in [crate::FontFamily::Sans, crate::FontFamily::Serif] {
        let mut opts = PdfOptions::default();
        opts.theme.font = family;
        let faces = Faces::load(&opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let actual = layout(&actual_doc.blocks, &opts, &faces, page);
        let expected = layout(&expected_doc.blocks, &opts, &faces, page);
        assert_layout_geometry_matches(&actual, &expected);
        assert!(actual.iter().any(|line| !line.table_cols.is_empty()));
        assert!(actual.iter().any(|line| line.bg != 0));
        assert!(
            actual
                .iter()
                .flat_map(|line| &line.segs)
                .any(|seg| { seg.text.contains("e\u{0301}") && seg.link.is_some() })
        );
        assert!(
            actual
                .iter()
                .flat_map(|line| &line.segs)
                .all(|seg| { seg.slot != F_SYMBOL && !seg.text.contains('é') })
        );
    }
}

#[test]
fn incremental_marked_runs_match_every_whole_prefix() {
    for family in [crate::FontFamily::Sans, crate::FontFamily::Serif] {
        let mut opts = PdfOptions::default();
        opts.theme.font = family;
        let faces = Faces::load(&opts).unwrap();
        for slot in [F_BODY, F_BOLD, F_ITALIC, F_BOLDITALIC, F_MONO] {
            let face = faces.face(slot);
            for source in [
                "officecafe\u{0301}AV",
                "A\u{030a}fi u\u{0308}\u{0301}z",
                "e\u{0301}\u{0301} office A\u{030a}",
            ] {
                let fs = font_size_of(11.0);
                let mut shaper = SegRunShaper::new(face, fs);
                let mut accumulated = String::new();
                for ch in source.chars() {
                    let mut bytes = [0; 4];
                    let chunk = ch.encode_utf8(&mut bytes);
                    accumulated.push(ch);
                    shaper.append(chunk, &accumulated);
                    assert_eq!(
                        shaper.current_width(),
                        face.shaped_width(&accumulated, fs),
                        "family={family:?}, slot={slot}, prefix={accumulated:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn dictionary_and_emergency_breaks_never_split_a_base_mark_chain() {
    let source = format!("x{}", "e\u{0301}".repeat(50));
    let chars: Vec<_> = source.chars().collect();
    let every_boundary: Vec<_> = (1..chars.len()).collect();
    for dictionary in [Vec::new(), every_boundary] {
        let points = pdf_word_break_points(&chars, &dictionary);
        assert!(
            !points.is_empty(),
            "long marked words must remain breakable"
        );
        for point in points {
            assert!(point.at > 0 && point.at < chars.len());
            assert!(
                !matches!(chars[point.at] as u32, 0x0300..=0x036f),
                "break at {} separates its following mark from the base",
                point.at
            );
        }
    }

    let opts = PdfOptions::default();
    let faces = Faces::load(&opts).unwrap();
    let mut page = PageGeom::from_theme(&opts.theme);
    page.content_w = 45.0;
    let lines = layout(
        &[Block::Paragraph(vec![Inline::Text(source.clone())])],
        &opts,
        &faces,
        page,
    );
    let rendered: Vec<_> = lines.iter().filter(|line| !line.segs.is_empty()).collect();
    assert!(rendered.len() > 2);
    let mut reconstructed = String::new();
    for line in rendered {
        let text: String = line.segs.iter().map(|seg| seg.text.as_str()).collect();
        assert!(
            !text
                .chars()
                .next()
                .is_some_and(|ch| { matches!(ch as u32, 0x0300..=0x036f) })
        );
        // This source contains no hyphens; any line-final hyphen is the
        // renderer's discretionary marker rather than source text.
        reconstructed.push_str(text.strip_suffix('-').unwrap_or(&text));
    }
    assert_eq!(reconstructed, source);
}

#[test]
fn narrow_code_wraps_clusters_and_preserves_fragment_source() {
    let opts = PdfOptions::default();
    let faces = Faces::load(&opts).unwrap();
    let source = "officecafe\u{0301}A\u{030a}".repeat(12);
    let fragments = [
        CodeFrag {
            text: "officecafe".into(),
            fill: Fill::Black,
        },
        CodeFrag {
            text: source["officecafe".len()..].into(),
            fill: Fill::Black,
        },
    ];
    let lines = wrap_code_fragments(&fragments, 33.0, 39.0, 9.5, &faces);
    assert!(lines.len() > 3);
    assert_eq!(
        lines
            .iter()
            .flatten()
            .map(|frag| frag.text.as_str())
            .collect::<String>(),
        source
    );
    let cache = RefCell::new(WidthCache::default());
    for (index, line) in lines.iter().enumerate() {
        let text: String = line.iter().map(|frag| frag.text.as_str()).collect();
        assert!(
            !text
                .chars()
                .next()
                .is_some_and(|ch| { matches!(ch as u32, 0x0300..=0x036f) })
        );
        let segments = code_frags_to_segs(line, 0.0, 9.5, &faces, &cache);
        assert!(segments.iter().all(|seg| seg.slot == F_MONO));
        let actual_width: f32 = segments.iter().map(|seg| seg.width).sum();
        let limit = if index == 0 { 33.0 } else { 39.0 };
        assert!(
            actual_width <= limit + 0.01,
            "{text:?}: {actual_width} > {limit}"
        );
    }
}

#[test]
fn narrow_styled_table_cells_hard_wrap_complete_marked_clusters() {
    let source = "e\u{0301}".repeat(36);
    let canonical = "é".repeat(36);
    let cell = |text: &str| {
        vec![Inline::Link {
            dest: "https://example.com/cell".into(),
            title: None,
            content: vec![Inline::Strong(vec![Inline::Strikethrough(vec![
                Inline::Text(text.into()),
            ])])],
        }]
    };
    let max_width = 24.0;
    for family in [crate::FontFamily::Sans, crate::FontFamily::Serif] {
        let mut opts = PdfOptions::default();
        opts.theme.font = family;
        let faces = Faces::load(&opts).unwrap();
        let cache = RefCell::new(WidthCache::default());
        let mut links = LinkIntern::default();
        let source_tokens = cell_tokens(&cell(&source), false, &faces, &mut links);
        let canonical_tokens = cell_tokens(&cell(&canonical), false, &faces, &mut links);
        let actual = wrap_cell_styled(&source_tokens, max_width, 10.0, &faces, &cache);
        let expected = wrap_cell_styled(&canonical_tokens, max_width, 10.0, &faces, &cache);
        assert!(actual.len() > 2);
        assert_eq!(actual.len(), expected.len());
        assert_eq!(
            actual
                .iter()
                .flat_map(|line| &line.runs)
                .map(|run| run.text.as_str())
                .collect::<String>(),
            source
        );
        for (actual, expected) in actual.iter().zip(&expected) {
            assert!(
                actual.width <= max_width + 0.001,
                "cell width {}",
                actual.width
            );
            assert!((actual.width - expected.width).abs() < 0.001);
            assert_eq!(actual.runs.len(), expected.runs.len());
            for (run, canonical_run) in actual.runs.iter().zip(&expected.runs) {
                assert_eq!(run.slot, F_BOLD);
                assert_eq!(run.link, Some(0));
                assert!(run.strike);
                assert!(
                    !run.text
                        .chars()
                        .next()
                        .is_some_and(|ch| { matches!(ch as u32, 0x0300..=0x036f) })
                );
                assert_eq!(run.text.replace("e\u{0301}", "é"), canonical_run.text);
                let face = faces.face(run.slot);
                let shaped = shape_run(&face.font, &face.lig, face.ascii_tables(), &run.text);
                assert!(!shaped.glyphs.contains(&0));
                assert!((run.width - face.shaped_width_points(&run.text, 10.0)).abs() < 0.001);
            }
        }
    }
}

fn composition_page_content(pdf: &[u8]) -> String {
    let text = String::from_utf8_lossy(pdf);
    let mut result = String::new();
    for reference in text.split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let header = format!("\n{number} 0 obj\n");
        let offset = pdf
            .windows(header.len())
            .position(|bytes| bytes == header.as_bytes())
            .unwrap()
            + header.len();
        let object = &pdf[offset..];
        let start = object
            .windows(8)
            .position(|bytes| bytes == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..start]).unwrap();
        let len: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let payload = &object[start + 8..start + 8 + len];
        if dictionary.contains("/Filter /FlateDecode") {
            let decoded = crate::zlib_decompress(payload, 16 * 1024 * 1024).unwrap();
            result.push_str(std::str::from_utf8(&decoded).unwrap());
        } else {
            assert!(!dictionary.contains("/Filter"));
            result.push_str(std::str::from_utf8(payload).unwrap());
        }
    }
    assert!(!result.is_empty());
    result
}

#[test]
fn emitted_pdf_preserves_nfd_occurrences_without_corrupting_nfc_cmap() {
    let opts = PdfOptions {
        page_numbers: false,
        ..PdfOptions::default()
    };
    // The NFD-only document must also seed the composite into the subset's
    // cmap; sharing the font with NFC text must never change its global map.
    for source in ["Cafe\u{0301}", "Café\n\nCafe\u{0301}"] {
        let doc = crate::parse_markdown(source);
        let pdf = crate::render_pdf_document(&doc, &opts).unwrap();
        let content = composition_page_content(&pdf);
        let actual_text = "/ActualText <FEFF00430061006600650301>";
        assert_eq!(content.matches(actual_text).count(), 1, "{content}");
        let actual = content.split_once(actual_text).unwrap().1;
        let marked = actual.split_once("EMC").unwrap().0;
        assert!(marked.contains("BDC") && marked.contains("BT") && marked.contains("TJ"));
        assert!(
            marked.contains("ET"),
            "text object must finish inside ActualText"
        );
        let raw = String::from_utf8_lossy(&pdf);
        assert!(
            raw.contains("> <00E9>\n"),
            "selected é needs a canonical ToUnicode entry"
        );
        assert!(
            !raw.contains("> <00650301>\n"),
            "NFD belongs to each occurrence, not the glyph map"
        );
    }
}

#[test]
fn embedded_svg_keeps_one_actual_text_span_for_anchored_fill_stroke_replays() {
    let render = |label: &str| {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="50" viewBox="0 0 160 50"><text x="80" y="30" text-anchor="middle" font-size="18" fill="blue" stroke="black" stroke-width="0.5" paint-order="stroke fill">{label}</text></svg>"#
        );
        let opts = PdfOptions {
            image_assets: vec![crate::PdfImageAsset::new("label.svg", svg.into_bytes())],
            page_numbers: false,
            ..PdfOptions::default()
        };
        let document = crate::parse_markdown("![Label](label.svg)");
        assert!(
            !render_warnings(&document, &opts)
                .iter()
                .any(|warning| { matches!(warning, RenderWarning::MissingGlyphs { .. }) })
        );
        crate::render_pdf_document(&document, &opts).unwrap()
    };
    let actual_pdf = render("Cafe\u{0301}");
    let canonical_pdf = render("Café");
    let actual = composition_page_content(&actual_pdf);
    let canonical = composition_page_content(&canonical_pdf);
    let start = "/Span << /ActualText <FEFF00430061006600650301> >> BDC\n";
    assert_eq!(actual.matches(start).count(), 1);
    let (before, marked) = actual.split_once(start).unwrap();
    let (drawing, after) = marked.split_once("EMC\n").unwrap();
    assert_eq!(
        drawing.matches("BT").count(),
        2,
        "stroke and fill replay text"
    );
    assert_eq!(drawing.matches("TJ").count(), 2);
    assert!(!canonical.contains("/ActualText"));
    // Removing only the occurrence's semantic wrapper leaves the same text
    // matrices and glyph operators: middle anchoring must use composed width,
    // and replaying the paint must not duplicate the extraction replacement.
    assert_eq!(format!("{before}{drawing}{after}"), canonical);
    assert!(String::from_utf8_lossy(&actual_pdf).contains("> <00E9>\n"));
}

#[test]
fn repeated_marked_text_is_deterministic_in_chunked_and_monolithic_pdf() {
    let mut blocks = Vec::new();
    for page in 0..3 {
        if page != 0 {
            blocks.push(Block::PageBreak);
        }
        blocks.push(Block::Paragraph(vec![Inline::Text("Cafe\u{0301}".into())]));
    }
    let doc = Document { blocks };
    let opts = PdfOptions {
        page_numbers: false,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let chunked =
        crate::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap();
    let monolithic =
        crate::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap();
    let again = crate::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap();
    assert_eq!(chunked, monolithic);
    assert_eq!(chunked, again);
    assert_eq!(
        String::from_utf8_lossy(&chunked)
            .matches("/Type /Page ")
            .count(),
        3
    );
    assert_eq!(
        composition_page_content(&chunked)
            .matches("/ActualText <FEFF00430061006600650301>")
            .count(),
        3
    );
}

#[test]
fn unsupported_clusters_keep_their_original_shaping_and_source_text() {
    let faces = Faces::load(&PdfOptions::default()).unwrap();
    for source in [
        "a\u{0301}\u{0307}",
        "e\u{0301}\u{0301}",
        "a\u{034f}",
        "\u{0301}a",
        "ﬃ①²",
    ] {
        let face = faces.face(F_BODY);
        let raw: Vec<_> = source.chars().map(|ch| face.font.glyph_index(ch)).collect();
        let expected: Vec<_> = face
            .lig
            .substitute_with_spans(&raw)
            .iter()
            .map(|&(gid, _)| gid)
            .collect();
        let shaped = shape_run(&face.font, &face.lig, face.ascii_tables(), source);
        assert_eq!(shaped.text, source);
        assert_eq!(shaped.glyphs, expected, "unsupported chain {source:?}");
        let mut toks = Vec::new();
        push_text_tokens(source, F_BODY, false, None, &mut toks);
        apply_symbol_fallback(&mut toks, &faces);
        assert_eq!(
            toks.iter().map(|tok| tok.text.as_str()).collect::<String>(),
            source
        );
    }
}
