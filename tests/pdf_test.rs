//! Structural tests for the clean-room PDF MVP. These are intentionally
//! byte-level: they pin deterministic writer invariants without depending on a
//! third-party PDF parser in the clean-room project.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::{
    HtmlOptions, PageMargins, PageSize, PdfImageAsset, PdfOptions, Theme, parse_markdown,
    render_html, render_pdf, render_pdf_document, render_pdf_document_profiled,
};

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    // The renderer does not trust or decode CRCs for this first PDF embedding
    // slice; the chunk envelope is enough for deterministic XObject emission.
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn tiny_rgb_png(dest_pixels: &[[u8; 3]]) -> Vec<u8> {
    let width = dest_pixels.len() as u32;
    let height = 1u32;
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB, deflate, PNG filters, no interlace.

    let mut rows = Vec::with_capacity(1 + dest_pixels.len() * 3);
    rows.push(0); // filter type 0 for the single row.
    for pixel in dest_pixels {
        rows.extend_from_slice(pixel);
    }
    let idat = franken_markdown::compress::zlib_compress(&rows);

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

fn tiny_rgb_png_with_prefix_chunk() -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"tEXt", b"before ihdr"));
    png.extend_from_slice(&tiny_rgb_png(&[[0x24, 0x91, 0xB8]])[8..]);
    png
}

fn tiny_rgb_png_with_nonempty_iend() -> Vec<u8> {
    let mut png = tiny_rgb_png(&[[0x24, 0x91, 0xB8]]);
    let iend = png_chunk(b"IEND", b"bad");
    if let Some(pos) = png.windows(12).position(|chunk| &chunk[4..8] == b"IEND") {
        png.truncate(pos);
        png.extend_from_slice(&iend);
    }
    png
}

fn tiny_rgb_png_with_trailing_bytes() -> Vec<u8> {
    let mut png = tiny_rgb_png(&[[0x24, 0x91, 0xB8]]);
    png.extend_from_slice(b"trailing bytes after IEND");
    png
}

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

fn text_matrices(bytes: &[u8], font_size: &str) -> Vec<(f32, f32)> {
    let needle = format!("{font_size} Tf 1 0 0 1 ");
    text_streams(bytes)
        .join("\n")
        .lines()
        .filter_map(|line| {
            let pos = line.find(&needle)? + needle.len();
            let mut parts = line[pos..].split_whitespace();
            let x = parts.next()?.parse::<f32>().ok()?;
            let y = parts.next()?.parse::<f32>().ok()?;
            Some((x, y))
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
fn pdf_uses_discretionary_hyphen_only_for_chosen_hyphen_breaks() {
    let narrow = render_pdf("hyphenation", &small_page_opts(80.0, 220.0)).unwrap();
    let narrow_text = as_text(&narrow);

    assert!(
        narrow_text.contains("<002D>"),
        "narrow PDF line should emit a selectable discretionary hyphen"
    );
    assert!(
        text_streams(&narrow).join("\n").matches("BT /F").count() >= 2,
        "hyphenated word should split across multiple physical PDF text rows"
    );

    let wide = render_pdf("hyphenation", &small_page_opts(260.0, 220.0)).unwrap();
    let wide_text = as_text(&wide);
    assert!(
        !wide_text.contains("<002D>"),
        "wide unbroken word must not synthesize an unused discretionary hyphen"
    );
}

#[test]
fn pdf_headings_stay_ragged_and_do_not_discretionary_hyphenate() {
    let pdf = render_pdf("# hyphenation", &small_page_opts(80.0, 220.0)).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("<002D>"),
        "headings use a ragged policy and must not synthesize discretionary hyphens"
    );
}

#[test]
fn pdf_lists_and_blockquotes_use_paragraph_hyphenation_policy() {
    let list_pdf = render_pdf("- representation\n", &small_page_opts(80.0, 220.0)).unwrap();
    let quote_pdf = render_pdf("> representation\n", &small_page_opts(80.0, 220.0)).unwrap();
    let list_count = as_text(&list_pdf).matches("<002D>").count();
    let quote_count = as_text(&quote_pdf).matches("<002D>").count();

    assert!(
        list_count >= 1,
        "list paragraph flow should be eligible for discretionary hyphenation; found {list_count}"
    );
    assert!(
        quote_count >= 1,
        "blockquote paragraph flow should be eligible for discretionary hyphenation; found {quote_count}"
    );
}

#[test]
fn pdf_table_cells_and_code_blocks_stay_ragged_without_discretionary_hyphens() {
    let table_pdf =
        render_pdf("| representation |\n|---|\n", &small_page_opts(90.0, 220.0)).unwrap();
    let code_pdf = render_pdf(
        "```text\nrepresentation\n```\n",
        &small_page_opts(90.0, 220.0),
    )
    .unwrap();

    assert!(
        !as_text(&table_pdf).contains("<002D>"),
        "table cell wrapping is currently a ragged measured-column policy, not discretionary hyphenation"
    );
    assert!(
        !as_text(&code_pdf).contains("<002D>"),
        "code block wrapping must not synthesize discretionary prose hyphens"
    );
}

#[test]
fn pdf_justifies_non_final_paragraph_lines_with_adjusted_glue() {
    let pdf = render_pdf(
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda",
        &small_page_opts(170.0, 260.0),
    )
    .unwrap();
    let matrices = text_matrices(&pdf, "11.00");

    let mut by_baseline: BTreeMap<i32, usize> = BTreeMap::new();
    for (_, y) in matrices {
        *by_baseline.entry((y * 100.0).round() as i32).or_default() += 1;
    }

    assert!(
        by_baseline
            .values()
            .any(|segments_on_line| *segments_on_line >= 2),
        "at least one non-final plain paragraph line should split into multiple positioned text segments after glue justification"
    );
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
fn pdf_renders_supplied_png_image_as_xobject() {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "images/tiny.png",
            tiny_rgb_png(&[[0xD0, 0x22, 0x40], [0x20, 0x64, 0xC8]]),
        )],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Tiny chart](images/tiny.png)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Subtype /Image"),
        "supported PNG should become an image XObject"
    );
    assert!(text.contains("/ColorSpace /DeviceRGB"));
    assert!(text.contains("/Filter /FlateDecode"));
    assert!(text.contains("/Predictor 15"));
    assert!(text.contains("/Colors 3"));
    assert!(text.contains("/Columns 2"));
    assert!(text.contains("/XObject << /Im1 "));
    assert!(
        text.contains("/Im1 Do"),
        "page content should draw the image"
    );
    assert!(
        text.contains("/S /Figure"),
        "tagged structure marks a figure"
    );
    assert!(
        text.contains("/Alt (Tiny chart)"),
        "figure alt text should be carried into the structure element"
    );
}

#[test]
fn pdf_images_fall_back_to_alt_text_when_asset_missing_or_unsupported() {
    let missing = render_pdf(
        "![Missing image](images/missing.png)",
        &PdfOptions::default(),
    )
    .unwrap();
    let missing_text = as_text(&missing);
    assert!(!missing_text.contains("/Subtype /Image"));
    assert!(
        missing_text.contains("BT /F"),
        "missing image asset should render visible alt text"
    );

    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("images/bad.png", b"not a png".to_vec())],
        ..PdfOptions::default()
    };
    let unsupported = render_pdf("![Bad image](images/bad.png)", &opts).unwrap();
    let unsupported_text = as_text(&unsupported);
    assert!(!unsupported_text.contains("/Subtype /Image"));
    assert!(
        unsupported_text.contains("BT /F"),
        "unsupported image asset should render visible alt text"
    );

    for (dest, bytes) in [
        ("images/prefix.png", tiny_rgb_png_with_prefix_chunk()),
        ("images/bad-iend.png", tiny_rgb_png_with_nonempty_iend()),
        ("images/trailing.png", tiny_rgb_png_with_trailing_bytes()),
    ] {
        let opts = PdfOptions {
            image_assets: vec![PdfImageAsset::new(dest, bytes)],
            ..PdfOptions::default()
        };
        let pdf = render_pdf(&format!("![Malformed envelope]({dest})"), &opts).unwrap();
        let pdf_text = as_text(&pdf);
        assert!(!pdf_text.contains("/Subtype /Image"));
        assert!(
            pdf_text.contains("BT /F"),
            "malformed PNG envelope should render visible alt text"
        );
    }
}

#[test]
fn pdf_image_object_order_is_deterministic_across_asset_order() {
    let md = "![Second](images/b.png)\n\n![First](images/a.png)";
    let first = PdfImageAsset::new("images/a.png", tiny_rgb_png(&[[0x24, 0x91, 0xB8]]));
    let second = PdfImageAsset::new("images/b.png", tiny_rgb_png(&[[0xE8, 0x44, 0x44]]));

    let opts_ab = PdfOptions {
        image_assets: vec![first.clone(), second.clone()],
        ..PdfOptions::default()
    };
    let opts_ba = PdfOptions {
        image_assets: vec![second, first],
        ..PdfOptions::default()
    };

    let pdf_ab = render_pdf(md, &opts_ab).unwrap();
    let pdf_ba = render_pdf(md, &opts_ba).unwrap();

    assert_eq!(
        pdf_ab, pdf_ba,
        "host asset order must not affect deterministic PDF bytes"
    );
    let pdf_text = as_text(&pdf_ab);
    assert!(pdf_text.contains("/XObject << /Im1 "));
    assert!(pdf_text.contains("/Im2 "));
    assert!(pdf_text.contains("/Im2 Do"));
    assert!(pdf_text.contains("/Im1 Do"));
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

    // Colors now derive from the shared theme tokens (one-theme-model doctrine):
    // the blockquote tint is `bg_subtle` (#f6f8fa) and the gutter bar is the
    // `quote_bar` token (#d1d9e0), matching the HTML stylesheet.
    assert!(
        text.contains("0.965 0.973 0.980 rg"),
        "blockquote background should use the theme `bg_subtle` tint"
    );
    assert!(
        text.contains("0.820 0.851 0.878 RG 2.50 w"),
        "blockquote gutter bar should use the theme `quote_bar` stroke"
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

// ---------------------------------------------------------------------------
// mwm.6.1 — cross-surface theme-token invariant.
//
// PDF colors must derive from the SAME shared theme tokens the HTML stylesheet
// uses (the "one theme model" doctrine). These tests render both surfaces and
// assert each visual element's color matches its theme token on both, and that
// changing a token moves both surfaces together. They log a full token table so
// any future divergence is obvious without a debugger.
// ---------------------------------------------------------------------------

/// Format a `#rrggbb` theme token the way the PDF writer does: device-RGB with
/// three decimals. Mirrors `pdf::hex_rgb` + the `{:.3}` content-stream writer.
fn token_pdf_rgb(hex: &str) -> String {
    let s = hex.trim_start_matches('#');
    let comp = |i: usize| f32::from(u8::from_str_radix(&s[i..i + 2], 16).unwrap()) / 255.0;
    format!("{:.3} {:.3} {:.3}", comp(0), comp(2), comp(4))
}

/// A small document touching every theme-driven element, kept under the page
/// stream compression threshold so the content stream stays inspectable.
const THEME_PROBE_MD: &str = "# Heading One\n\n> quoted text\n>\n> more quote\n\nBody with a \
     [link](https://example.com) and `inline code`.\n\n| A | B |\n|---|--:|\n| 1 | 2 |\n| 3 | 4 |\n\n---\n";

/// For each page content stream, the ordered list of structural tags (P, TR,
/// H1/H2, Code, Figure) as emitted by the `/<TAG> <</MCID n>> BDC` marks.
fn page_tag_sequences(bytes: &[u8]) -> Vec<Vec<String>> {
    text_streams(bytes)
        .iter()
        .map(|stream| {
            let mut tags = Vec::new();
            let mut rest = stream.as_str();
            while let Some(pos) = rest.find(" <</MCID ") {
                let before = &rest[..pos];
                if let Some(slash) = before.rfind('/') {
                    tags.push(before[slash + 1..].to_string());
                }
                rest = &rest[pos + 1..];
            }
            tags
        })
        .collect()
}

#[test]
fn pdf_keeps_a_short_caption_with_its_following_table() {
    // mwm.7 keep-with-next: a short intro/caption paragraph must not strand at the
    // foot of a page while the table it introduces starts the next page.
    let mut filler = String::new();
    for i in 1..=9 {
        filler.push_str(&format!("Filler line number {i} here.\n\n"));
    }
    let opts = small_page_opts(240.0, 150.0);

    // Control: without a table, the caption fits as the last paragraph on page 1.
    let no_table = format!("{filler}Caption for the table\n");
    let control = page_tag_sequences(&render_pdf(&no_table, &opts).unwrap());
    let caption_on_last_page = control
        .last()
        .unwrap()
        .iter()
        .filter(|t| t.as_str() == "P")
        .count();
    assert!(
        control.len() == 2 && caption_on_last_page == 5,
        "control: the caption should sit with filler on page 1; got {control:?}"
    );

    // With a table, the caption is pulled onto the table's page (keep-with-next).
    let with_table = format!(
        "{filler}Caption for the table\n\n| Col A | Col B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n"
    );
    let pages = page_tag_sequences(&render_pdf(&with_table, &opts).unwrap());
    let table_page = pages
        .iter()
        .position(|tags| tags.iter().any(|t| t == "TR"))
        .expect("the table should render onto some page");
    assert!(
        table_page >= 2,
        "the table should paginate onto page 2+, got {pages:?}"
    );
    assert_eq!(
        pages[table_page][0], "P",
        "the caption (P) must head the table's page (kept with the table); got {:?}",
        pages[table_page]
    );
    assert!(
        !pages[table_page - 1].iter().any(|t| t == "TR"),
        "the page before the table must not contain table rows; got {pages:?}"
    );
    // The caption was pulled off the prior page: 4 filler P remain, not 5.
    assert_eq!(
        pages[table_page - 1]
            .iter()
            .filter(|t| t.as_str() == "P")
            .count(),
        4,
        "the caption should have been removed from the page before the table; got {pages:?}"
    );
}

#[test]
fn pdf_colors_derive_from_shared_theme_tokens() {
    let theme = Theme::default();
    let colors = &theme.colors;
    let pdf = render_pdf(THEME_PROBE_MD, &PdfOptions::default()).unwrap();
    let pdf_text = as_text(&pdf);
    let html = render_html(
        THEME_PROBE_MD,
        &HtmlOptions {
            theme: theme.clone(),
            ..HtmlOptions::default()
        },
    )
    .unwrap();

    // (element, theme-token hex): each must appear as device-RGB in the PDF and
    // as its hex token in the HTML stylesheet.
    let ledger: [(&str, &str); 6] = [
        ("link/accent", colors.accent.as_str()),
        ("body text (fg)", colors.fg.as_str()),
        ("code/quote bg (bg_subtle)", colors.bg_subtle.as_str()),
        ("blockquote bar (quote_bar)", colors.quote_bar.as_str()),
        ("table stripe (stripe)", colors.stripe.as_str()),
        (
            "heading/table rule (border_muted)",
            colors.border_muted.as_str(),
        ),
    ];

    let mut failures = Vec::new();
    for (element, hex) in ledger {
        let pdf_rgb = token_pdf_rgb(hex);
        let in_pdf = pdf_text.contains(&pdf_rgb);
        let in_html = html.contains(hex);
        eprintln!(
            "theme-token | {element:34} | token {hex} | pdf `{pdf_rgb}` present={in_pdf} | html present={in_html}"
        );
        if !in_pdf {
            failures.push(format!(
                "PDF missing {element} color `{pdf_rgb}` (token {hex})"
            ));
        }
        if !in_html {
            failures.push(format!("HTML missing {element} token {hex}"));
        }
    }
    assert!(
        failures.is_empty(),
        "PDF/HTML diverged from shared theme tokens:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn theme_color_token_changes_propagate_to_both_html_and_pdf() {
    // Changing one token must move BOTH surfaces — the core of the doctrine.
    let mut theme = Theme::default();
    theme.colors.accent = "#ff0000".to_string();
    let md = "A [link](https://example.com).\n";

    let html = render_html(
        md,
        &HtmlOptions {
            theme: theme.clone(),
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    let pdf = render_pdf(
        md,
        &PdfOptions {
            theme,
            ..PdfOptions::default()
        },
    )
    .unwrap();
    let pdf_text = as_text(&pdf);

    let red_rgb = token_pdf_rgb("#ff0000");
    eprintln!("custom accent #ff0000 -> pdf `{red_rgb}`");
    assert!(
        html.contains("#ff0000"),
        "HTML link color should follow the custom accent token"
    );
    assert!(
        pdf_text.contains(&red_rgb),
        "PDF link color should follow the custom accent token (`{red_rgb}`)"
    );
    assert!(
        !pdf_text.contains(&token_pdf_rgb("#0969da")),
        "overridden accent must not leave the default accent in the PDF"
    );
}

#[test]
fn theme_serif_keeps_shared_color_tokens() {
    // Switching the body font to serif must not change colors: they remain
    // theme-token driven, proving font and color are independent.
    let theme = Theme::serif();
    let pdf = render_pdf(
        THEME_PROBE_MD,
        &PdfOptions {
            theme: theme.clone(),
            ..PdfOptions::default()
        },
    )
    .unwrap();
    let pdf_text = as_text(&pdf);
    for hex in [
        &theme.colors.accent,
        &theme.colors.fg,
        &theme.colors.bg_subtle,
    ] {
        let rgb = token_pdf_rgb(hex);
        assert!(
            pdf_text.contains(&rgb),
            "serif PDF should still use theme token {hex} (`{rgb}`)"
        );
    }
}
