//! Structural tests for the clean-room PDF MVP. These are intentionally
//! byte-level: they pin deterministic writer invariants without depending on a
//! third-party PDF parser in the clean-room project.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::{
    PageMargins, PageSize, PdfOptions, Theme, parse_markdown, render_pdf, render_pdf_document,
    render_pdf_document_profiled,
};

fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn text_streams(bytes: &[u8]) -> Vec<String> {
    let text = as_text(bytes);
    let mut streams = Vec::new();
    let mut search = text.as_str();
    while let Some(pos) = search.find("stream\n") {
        let body_start = pos + "stream\n".len();
        let Some(body_end_rel) = search[body_start..].find("endstream") else {
            break;
        };
        let body = &search[body_start..body_start + body_end_rel];
        if body.contains("BT /F") {
            streams.push(body.to_string());
        }
        search = &search[body_start + body_end_rel + "endstream".len()..];
    }
    streams
}

fn small_page_opts(width_pt: f32, height_pt: f32) -> PdfOptions {
    let mut theme = Theme::default();
    theme.page.size = PageSize {
        name: "test-small",
        width_pt,
        height_pt,
    };
    theme.page.margins = PageMargins {
        top_pt: 20.0,
        right_pt: 20.0,
        bottom_pt: 20.0,
        left_pt: 20.0,
    };
    PdfOptions {
        theme,
        ..PdfOptions::default()
    }
}

fn text_x_positions(bytes: &[u8], font_size: &str) -> Vec<f32> {
    let needle = format!("{font_size} Tf 1 0 0 1 ");
    text_streams(bytes)
        .join("\n")
        .lines()
        .filter_map(|line| {
            let pos = line.find(&needle)? + needle.len();
            let end = line[pos..].find(' ')? + pos;
            line[pos..end].parse::<f32>().ok()
        })
        .collect()
}

fn compressed_stream_ledgers(text: &str) -> Vec<(usize, usize)> {
    let mut ledgers = Vec::new();
    let mut offset = 0usize;
    let marker = "/Filter /FlateDecode /DL ";
    while let Some(rel) = text[offset..].find(marker) {
        let filter_pos = offset + rel;
        let Some(length_pos) = text[..filter_pos].rfind("/Length ") else {
            offset = filter_pos + marker.len();
            continue;
        };
        let length_start = length_pos + "/Length ".len();
        let length_end = text[length_start..]
            .find(|c: char| !c.is_ascii_digit())
            .map_or(text.len(), |end| length_start + end);
        let dl_start = filter_pos + marker.len();
        let dl_end = text[dl_start..]
            .find(|c: char| !c.is_ascii_digit())
            .map_or(text.len(), |end| dl_start + end);
        if let (Ok(length), Ok(decoded)) = (
            text[length_start..length_end].parse::<usize>(),
            text[dl_start..dl_end].parse::<usize>(),
        ) {
            ledgers.push((length, decoded));
        }
        offset = dl_end;
    }
    ledgers
}

#[test]
fn pdf_has_valid_header_xref_and_eof_marker() {
    let pdf = render_pdf(
        "# PDF\n\nA paragraph with **strong** text.\n\n- one\n- two\n",
        &PdfOptions::default(),
    )
    .unwrap();

    assert!(pdf.starts_with(b"%PDF-1.7\n"));
    assert!(pdf.ends_with(b"%%EOF\n"));

    let text = as_text(&pdf);
    let startxref_pos = text.rfind("startxref\n").unwrap();
    let number_start = startxref_pos + "startxref\n".len();
    let number_end = text[number_start..].find('\n').unwrap() + number_start;
    let xref_offset: usize = text[number_start..number_end].parse().unwrap();

    assert_eq!(&pdf[xref_offset..xref_offset + 4], b"xref");
    assert!(text.contains("/Type /Catalog"));
    assert!(text.contains("/Type /Pages"));
    // Text is set in embedded subset faces (Type0/Identity-H + CIDFontType2 with
    // a FontFile2 program), not base-14 fonts.
    assert!(text.contains("/Subtype /Type0"), "composite Type0 font");
    assert!(
        text.contains("/Encoding /Identity-H"),
        "identity glyph encoding"
    );
    assert!(
        text.contains("/Subtype /CIDFontType2"),
        "CID descendant font"
    );
    assert!(text.contains("/FontFile2"), "embedded subset font program");
    assert!(text.contains("/ToUnicode"), "selectable text mapping");
}

#[test]
fn pdf_large_page_content_streams_are_flate_compressed() {
    let mut theme = Theme::default();
    theme.page.size = PageSize {
        name: "tall-compression-test",
        width_pt: 612.0,
        height_pt: 3600.0,
    };
    theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    let opts = PdfOptions {
        theme,
        ..PdfOptions::default()
    };
    let mut md = String::from("# Compression\n\n");
    for _ in 0..180 {
        md.push_str(
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron  \n",
        );
    }

    let pdf = render_pdf(&md, &opts).unwrap();
    let text = as_text(&pdf);
    let ledgers = compressed_stream_ledgers(&text);

    assert!(
        !ledgers.is_empty(),
        "large page content streams should use FlateDecode with a decoded-length ledger"
    );
    assert!(
        ledgers
            .iter()
            .any(|(compressed, decoded)| *compressed * 100 < *decoded * 70),
        "repetitive content streams should shrink materially: {ledgers:?}"
    );
}

#[test]
fn pdf_title_metadata_is_indirect_when_title_is_set() {
    let opts = PdfOptions {
        title: Some("Quarterly Memo".to_string()),
        author: Some("Research Desk".to_string()),
        ..PdfOptions::default()
    };
    let pdf = render_pdf("# PDF\n\nBody.", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/Info "));
    assert!(text.contains(" 0 R"));
    assert!(text.contains("/Title (Quarterly Memo)"));
    assert!(text.contains("/Author (Research Desk)"));
}

#[test]
fn pdf_metadata_is_deterministic_even_without_title() {
    let pdf = render_pdf("Body.", &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/Info "));
    assert!(text.contains("/Producer (franken_markdown)"));
    assert!(text.contains("/Creator (fmd)"));
    assert!(text.contains("/CreationDate (D:19700101000000Z)"));
    assert!(text.contains("/ModDate (D:19700101000000Z)"));
}

#[test]
fn pdf_metadata_honors_explicit_epoch_seconds() {
    let opts = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let pdf = render_pdf("Body.", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/CreationDate (D:20231114221320Z)"));
    assert!(text.contains("/ModDate (D:20231114221320Z)"));
}

#[test]
fn profiled_pdf_render_matches_normal_bytes_and_reports_required_stages() {
    let doc = parse_markdown(
        "# Profiled PDF\n\n\
         A paragraph with **bold** text, `code`, and [a link](https://example.com).\n\n\
         | Stage | Count |\n|---|---:|\n| layout | 1 |\n\n\
         ```rust\nfn main() { println!(\"hi\"); }\n```\n",
    );
    let opts = PdfOptions::default();

    let normal = render_pdf_document(&doc, &opts).unwrap();
    let profiled = render_pdf_document_profiled(&doc, &opts).unwrap();

    assert_eq!(
        profiled.bytes, normal,
        "profiling must not alter the rendered PDF byte stream"
    );

    let stages: BTreeSet<&str> = profiled.stages.iter().map(|stage| stage.stage).collect();
    for required in [
        "font_load",
        "layout",
        "used_slot_scan",
        "glyph_collection_and_shaping",
        "font_subsetting",
        "pagination",
        "heading_metadata",
        "page_content_stream_generation",
        "page_stream_compression",
        "widths_array_generation",
        "font_stream_compression",
        "tounicode_generation",
        "xref_trailer_writing",
        "pdf_object_serialization_total",
        "serialize_total",
    ] {
        assert!(stages.contains(required), "missing PDF stage {required}");
    }

    assert!(
        profiled
            .stages
            .iter()
            .any(|stage| stage.bytes == normal.len()),
        "one total stage should report the final output byte count"
    );
}

#[test]
fn pdf_external_links_emit_safe_uri_annotations() {
    let pdf = render_pdf(
        "[site](https://example.com?q=1) [mail](mailto:hello@example.com) \
         [bad](javascript:alert(1)) [gap](<java\tscript:alert(2)>)",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Annots ["),
        "page should reference link annotations"
    );
    assert!(text.contains("/Subtype /Link"));
    assert!(text.contains("/S /URI"));
    assert!(text.contains("/URI (https://example.com?q=1)"));
    assert!(text.contains("/URI (mailto:hello@example.com)"));
    assert!(
        !text.contains("javascript:alert"),
        "unsafe markdown URL schemes must never become PDF annotations"
    );
}

#[test]
fn pdf_headings_create_outlines_and_internal_destinations() {
    let pdf = render_pdf(
        "# Alpha\n\nJump [to alpha](#alpha).\n\n## Beta\n\n## Alpha\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/Outlines "));
    assert!(text.contains("/PageMode /UseOutlines"));
    assert!(text.contains("/Type /Outlines"));
    assert!(text.contains("/Count 3"));
    assert!(text.contains("/Title (Alpha)"));
    assert!(text.contains("/Title (Beta)"));
    assert!(text.contains("/Dest ["));
    assert!(
        !text.contains("/URI (#alpha)"),
        "fragment links should become internal destinations, not external URI actions"
    );
}

#[test]
fn pdf_emits_tagged_structure_tree_for_core_blocks() {
    let pdf = render_pdf(
        "# Title\n\nA plain paragraph.\n\nVisit [site](https://example.com).\n\n```rust\nfn main() {}\n```\n\n| Name | Value |\n|---|---:|\n| alpha | 1 |\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/MarkInfo << /Marked true >>"));
    assert!(text.contains("/StructTreeRoot"));
    assert!(text.contains("/ParentTree"));
    assert!(text.contains("/StructParents 0"));
    assert!(text.contains("/Tabs /S"));
    assert!(text.contains("/Type /MCR"));
    assert!(text.contains("/MCID 0"));
    assert!(text.contains("/S /H1"));
    assert!(text.contains("/S /P"));
    assert!(text.contains("/S /Link"));
    assert!(text.contains("/S /Code"));
    assert!(text.contains("/S /TR"));
    assert!(
        text.contains("/ToUnicode"),
        "tagged PDF still needs ToUnicode maps for copy/search"
    );
}

#[test]
fn pdf_preserves_raw_html_source_text_instead_of_dropping_it() {
    let md = "before <i>raw</i> after\n\n<section>block</section>\n";
    for (label, opts) in [
        ("default", PdfOptions::default()),
        (
            "allow_raw_html",
            PdfOptions {
                allow_raw_html: true,
                ..PdfOptions::default()
            },
        ),
    ] {
        let pdf = render_pdf(md, &opts).unwrap();
        let text = as_text(&pdf);

        assert!(
            text.contains("<003C>"),
            "{label}: less-than from raw HTML source must remain selectable"
        );
        assert!(
            text.contains("<003E>"),
            "{label}: greater-than from raw HTML source must remain selectable"
        );
        assert!(
            text.contains("<002F>"),
            "{label}: closing-tag slash from raw HTML source must remain selectable"
        );
        assert!(
            text_streams(&pdf).join("\n").matches("BT /F").count() >= 2,
            "{label}: inline and block raw HTML source should both produce PDF text"
        );
    }
}

#[test]
fn pdf_table_cells_preserve_raw_html_source_text() {
    let pdf = render_pdf("| <i>raw</i> |\n|---|\n", &PdfOptions::default()).unwrap();
    let streams = text_streams(&pdf).join("\n");

    assert!(
        streams.contains("BT /F"),
        "raw HTML in the only table cell must not render as an empty table"
    );
}

#[test]
fn pdf_hard_breaks_force_distinct_text_lines() {
    let pdf = render_pdf("first  \nsecond", &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches("BT /F").count(),
        2,
        "Markdown hard breaks must become distinct PDF text lines"
    );

    let trailing = render_pdf("first  \n", &PdfOptions::default()).unwrap();
    let trailing_text = as_text(&trailing);
    assert_eq!(
        trailing_text.matches("BT /F").count(),
        1,
        "a trailing hard break must not synthesize an empty final PDF text line"
    );
}

#[test]
fn pdf_page_builder_keeps_headings_with_following_content() {
    let opts = small_page_opts(260.0, 150.0);
    let pdf = render_pdf(
        "one  \ntwo  \nthree  \nfour\n\n# Kept Heading\n\nafter\n",
        &opts,
    )
    .unwrap();
    let streams = text_streams(&pdf);

    assert!(
        streams.len() >= 2,
        "small page should force at least two text content streams"
    );
    assert_eq!(
        streams[0].matches("BT /F").count(),
        4,
        "first page should end before the heading instead of stranding it"
    );
    assert!(
        streams[1].matches("BT /F").count() >= 2,
        "heading and its following paragraph should start together on page two"
    );
}

#[test]
fn pdf_page_builder_avoids_single_line_widows() {
    let opts = small_page_opts(260.0, 100.0);
    let pdf = render_pdf("alpha  \nbeta  \ngamma  \ndelta  \nepsilon\n", &opts).unwrap();
    let streams = text_streams(&pdf);

    assert!(
        streams.len() >= 2,
        "small page should split the paragraph across pages"
    );
    assert_eq!(
        streams[0].matches("BT /F").count(),
        3,
        "page builder should choose 3/2 rather than a 4/1 widow split"
    );
    assert_eq!(
        streams[1].matches("BT /F").count(),
        2,
        "second page should not contain a single-line widow"
    );
}

#[test]
fn pdf_table_cells_wrap_within_measured_columns() {
    let opts = small_page_opts(240.0, 260.0);
    let pdf = render_pdf(
        "| Name | Notes |\n\
         |---|---|\n\
         | alpha | one two three four five six seven eight nine ten eleven twelve |\n",
        &opts,
    )
    .unwrap();
    let streams = text_streams(&pdf).join("\n");

    assert!(
        streams.matches("/F1 10.00 Tf").count() >= 2,
        "long body cell should wrap into multiple table text lines"
    );
}

#[test]
fn pdf_table_headers_repeat_on_continuation_pages() {
    let opts = small_page_opts(260.0, 120.0);
    let mut md = String::from("| Name | Value |\n|---|---:|\n");
    for idx in 1..=10 {
        md.push_str(&format!("| row {idx} | {idx} |\n"));
    }
    let pdf = render_pdf(&md, &opts).unwrap();
    let streams = text_streams(&pdf);

    assert!(
        streams.len() >= 2,
        "small page should force the table across multiple content streams"
    );
    assert!(
        streams[1].contains("/F2 10.00 Tf"),
        "continuation page should repeat the bold table header"
    );
    assert!(
        streams[1].contains("/F1 10.00 Tf"),
        "continuation page should still contain body rows after the repeated header"
    );
}

#[test]
fn pdf_blockquotes_have_subtle_background_and_gutter_bar() {
    let pdf = render_pdf("> quoted text\n>\n> second line\n", &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.985 0.987 0.991 rg"),
        "blockquote background should use the subtle quote panel fill"
    );
    assert!(
        text.contains("0.75 0.75 0.80 RG 2.50 w"),
        "blockquote should retain the gutter bar stroke"
    );
}

#[test]
fn pdf_lists_use_hanging_marker_gutters_for_wrapped_items() {
    let opts = small_page_opts(210.0, 260.0);
    let pdf = render_pdf(
        "- alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi\n",
        &opts,
    )
    .unwrap();
    let xs = text_x_positions(&pdf, "/F1 11.00");
    let mut counts = BTreeMap::new();
    for x in xs {
        *counts.entry((x * 100.0).round() as i32).or_insert(0usize) += 1;
    }
    assert!(!counts.is_empty(), "expected list text positions");
    let marker_x = counts.keys().next().copied().unwrap_or_default();
    let repeated_content_x = counts
        .iter()
        .find_map(|(&x, &count)| (x > marker_x + 500 && count >= 2).then_some(x));

    assert!(
        repeated_content_x.is_some(),
        "wrapped list lines should share a content column to the right of the marker gutter: {counts:?}"
    );
}

#[test]
fn pdf_code_blocks_use_shared_syntax_highlight_colors() {
    let pdf = render_pdf(
        "```rust\nfn main() { let n = 1; }\n```\n\n\
         ```html\n<section class=\"hero\">x</section>\n```\n\n\
         ```css\n.hero { color: #0a3069; }\n```\n\n\
         ```markdown\n# Title with `code`\n```\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/F4 9.50 Tf"), "code uses the mono face");
    assert!(
        text.contains("0.812 0.133 0.180 rg"),
        "keyword color emitted from shared token stream"
    );
    assert!(
        text.contains("0.584 0.220 0.000 rg"),
        "type/selector/attribute color emitted"
    );
    assert!(
        text.contains("0.039 0.188 0.412 rg"),
        "string color emitted"
    );
    assert!(
        text.contains("0.020 0.314 0.682 rg"),
        "number/operator color emitted"
    );
    assert!(
        pdf.len() < 80_000,
        "syntax-highlighted embedded-font PDF stays compact ({} bytes)",
        pdf.len()
    );

    let unknown = render_pdf(
        "```not-a-language\nfn main() { let n = 1; }\n```\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let unknown_text = as_text(&unknown);
    assert!(
        unknown_text.contains("/F4 9.50 Tf"),
        "unknown code still renders in mono"
    );
    for syntax_color in [
        "0.812 0.133 0.180 rg",
        "0.584 0.220 0.000 rg",
        "0.039 0.188 0.412 rg",
        "0.020 0.314 0.682 rg",
    ] {
        assert!(
            !unknown_text.contains(syntax_color),
            "unknown language must remain monochrome, found {syntax_color}"
        );
    }
}

#[test]
fn pdf_code_blocks_wrap_long_highlighted_lines_without_clipping() {
    let opts = small_page_opts(210.0, 260.0);
    let pdf = render_pdf(
        "```rust\nlet suffix = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaZ\";\n```\n",
        &opts,
    )
    .unwrap();
    let text = as_text(&pdf);
    let streams = text_streams(&pdf).join("\n");

    assert!(
        streams.matches("/F4 9.50 Tf").count() >= 3,
        "one long code source line should become multiple wrapped PDF text rows"
    );
    assert!(
        text.contains("<005A>"),
        "suffix character Z must survive subsetting; clipping would drop it"
    );
    assert!(
        streams.contains("0.812 0.133 0.180 rg"),
        "wrapped code should preserve syntax token colors"
    );
}

#[test]
fn pdf_code_blocks_can_emit_muted_line_numbers() {
    let opts = PdfOptions {
        code_line_numbers: true,
        ..PdfOptions::default()
    };
    let pdf = render_pdf("```text\nalpha\nbeta\n```\n", &opts).unwrap();
    let streams = text_streams(&pdf).join("\n");

    assert!(
        streams.matches("/F4 9.50 Tf").count() >= 4,
        "two source rows should render two line-number runs plus two code runs"
    );
    assert!(
        streams.contains("0.431 0.467 0.506 rg"),
        "line numbers should use the muted syntax/comment color"
    );
    assert!(
        as_text(&pdf).contains("<0032>"),
        "line number 2 should be embedded/selectable in the font subset"
    );
}

#[test]
fn pdf_honors_theme_page_size_and_margins() {
    let mut theme = Theme::default();
    theme.page.size = PageSize {
        name: "small-test",
        width_pt: 300.0,
        height_pt: 420.0,
    };
    theme.page.margins = PageMargins {
        top_pt: 30.0,
        right_pt: 40.0,
        bottom_pt: 50.0,
        left_pt: 20.0,
    };
    let opts = PdfOptions {
        theme,
        ..PdfOptions::default()
    };

    let pdf = render_pdf("Hello", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/MediaBox [0 0 300 420]"),
        "PDF page size should come from Theme.page"
    );
    assert!(
        text.contains("1 0 0 1 20.00 375.48 Tm"),
        "first text baseline should honor left/top margins"
    );
}

#[test]
fn pdf_sanitizes_pathological_theme_page_geometry() {
    let mut theme = Theme::default();
    theme.page.size = PageSize {
        name: "bad-test",
        width_pt: f32::NAN,
        height_pt: 20.0,
    };
    theme.page.margins = PageMargins {
        top_pt: 1000.0,
        right_pt: -1.0,
        bottom_pt: f32::INFINITY,
        left_pt: f32::INFINITY,
    };
    let opts = PdfOptions {
        theme,
        ..PdfOptions::default()
    };

    let pdf = render_pdf("Hello", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/MediaBox [0 0 612 80]"),
        "invalid dimensions should fall back or clamp to a printable page"
    );
    assert!(
        text.contains("1 0 0 1 72.00 25.48 Tm"),
        "invalid margins should fall back or clamp without negative content geometry"
    );
    assert!(!text.contains("NaN"));
    assert!(!text.contains("inf"));
}
