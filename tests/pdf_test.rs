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
    assert!(
        text.contains("/O /Layout /BBox ["),
        "figure should carry a layout bounding box so AT can locate the image"
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
    // Single /Document root under the StructTreeRoot, then semantic elements.
    assert!(text.contains("/S /Document"));
    assert!(text.contains("/S /H1"));
    assert!(text.contains("/S /P"));
    assert!(text.contains("/S /Link"));
    assert!(text.contains("/S /Code"));
    // Tables now nest properly: a /Table holding /TR rows holding /TH//TD cells.
    assert!(text.contains("/S /Table"));
    assert!(text.contains("/S /TR"));
    assert!(text.contains("/S /TH"));
    assert!(text.contains("/S /TD"));
    assert!(
        text.contains("/ToUnicode"),
        "tagged PDF still needs ToUnicode maps for copy/search"
    );
}

/// Parse every `/StructElem` into `(object, tag, parent_object)`. The structure
/// elements are plain (non-stream) objects, so a forward scan over the bytes is
/// exact.
fn struct_elements(bytes: &[u8]) -> Vec<(usize, String, usize)> {
    let text = as_text(bytes);
    let needle = " 0 obj\n<< /Type /StructElem /S /";
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(pos) = rest.find(needle) {
        let obj = rest[..pos]
            .rsplit(|c: char| !c.is_ascii_digit())
            .find(|s| !s.is_empty())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        let after = &rest[pos + needle.len()..];
        let tag: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        let parent = after
            .find(" /P ")
            .and_then(|p| after[p + 4..].split(" 0 R").next())
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);
        out.push((obj, tag, parent));
        rest = after;
    }
    out
}

#[test]
fn pdf_blockquote_inside_list_item_nests_under_the_list() {
    // A blockquote nested inside a list item must tag as BlockQuote *inside* the
    // item's LBody — not the reverse. Containers are ordered by where they open,
    // so this and the `> - item` (list-in-quote) case both nest correctly.
    let pdf = render_pdf("- alpha\n\n  > quoted\n", &PdfOptions::default()).unwrap();
    let elems = struct_elements(&pdf);

    let lbody = elems
        .iter()
        .find(|(_, tag, _)| tag == "LBody")
        .map(|(obj, _, _)| *obj)
        .expect("a list item should have an LBody");
    let blockquote = elems
        .iter()
        .find(|(_, tag, _)| tag == "BlockQuote")
        .expect("the nested blockquote should be tagged");
    assert_eq!(
        blockquote.2, lbody,
        "the blockquote must nest inside the list item's LBody; elems={elems:?}"
    );
}

#[test]
fn pdf_list_inside_blockquote_nests_under_the_quote() {
    // The mirror case: `> - item` must nest the list inside the blockquote.
    let pdf = render_pdf("> - alpha\n> - beta\n", &PdfOptions::default()).unwrap();
    let elems = struct_elements(&pdf);

    let quote = elems
        .iter()
        .find(|(_, tag, _)| tag == "BlockQuote")
        .map(|(obj, _, _)| *obj)
        .expect("the blockquote should be tagged");
    let list = elems
        .iter()
        .find(|(_, tag, _)| tag == "L")
        .expect("the list inside the quote should be tagged");
    assert_eq!(
        list.2, quote,
        "the list must nest inside the blockquote; elems={elems:?}"
    );
}

/// Sum of marked-content openers (`BDC` for structure, `BMC` for artifacts) and
/// closers (`EMC`) across the (uncompressed) page content streams. For a
/// conforming tagged PDF these must balance: nothing may be left unmarked. Only
/// `text_streams` (streams holding `BT /F` text) are inspected, so binary font
/// programs cannot contribute false matches.
fn marked_content_balance(bytes: &[u8]) -> (usize, usize) {
    let mut openers = 0usize;
    let mut closers = 0usize;
    for body in text_streams(bytes) {
        openers += body.matches("BDC").count() + body.matches("BMC").count();
        closers += body.matches("EMC").count();
    }
    (openers, closers)
}

#[test]
fn pdf_structure_tree_is_hierarchical_and_accessible() {
    // A document exercising every container the writer can tag: nested lists, a
    // blockquote, an aligned table, a fenced code block, and an inline link.
    // Kept small so the page content stream stays uncompressed and its marked
    // content is inspectable as plain bytes.
    let md = "# Heading\n\n\
        Intro with a [link](https://example.com/docs).\n\n\
        - first item\n- second item\n  - nested item\n- third item\n\n\
        > A quoted paragraph.\n\n\
        | Name | Value |\n|:---|---:|\n| alpha | 1 |\n| beta | 2 |\n\n\
        ```rust\nfn main() {}\n```\n";
    let pdf = render_pdf(md, &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);
    assert!(
        text_streams(&pdf).iter().any(|s| s.contains("BDC")),
        "test relies on an uncompressed content stream; shrink the doc if this fails"
    );

    // Catalog wires the tree as a tagged PDF.
    assert!(text.contains("/MarkInfo << /Marked true >>"));
    assert!(text.contains("/Lang (en-US)"));

    // Exactly one /Document root, and it parents the StructTreeRoot. Recover the
    // StructTreeRoot object number from the catalog and confirm the chain.
    assert_eq!(
        text.matches("/S /Document").count(),
        1,
        "there must be a single /Document structure root"
    );
    let root_obj = text
        .split("/StructTreeRoot ")
        .nth(1)
        .and_then(|tail| tail.split(" 0 R").next())
        .and_then(|n| n.trim().parse::<usize>().ok())
        .expect("catalog references the StructTreeRoot object");
    assert!(
        text.contains(&format!("/S /Document /P {root_obj} 0 R")),
        "the /Document element must be parented by the StructTreeRoot ({root_obj})"
    );

    // Semantic containers and leaves.
    for tag in [
        "/S /H1",
        "/S /P",
        "/S /L",
        "/S /LI",
        "/S /LBody",
        "/S /BlockQuote",
        "/S /Link",
        "/S /Table",
        "/S /TR",
        "/S /TH",
        "/S /TD",
        "/S /Code",
    ] {
        assert!(text.contains(tag), "missing structure tag {tag}");
    }

    // The nested list produces a second /L nested inside the outer list. The
    // `/S /L ` form (trailing space before `/P`) matches only `/L`, never the
    // `/LI` or `/LBody` elements.
    assert!(
        text.matches("/S /L ").count() >= 2,
        "a nested list must emit an inner /L element"
    );

    // Header cells advertise a column scope; the link annotation is referenced
    // back from its /Link element with an /OBJR.
    assert!(
        text.contains("/A << /O /Table /Scope /Column >>"),
        "table header cells need a column scope"
    );
    assert_eq!(
        text.matches("/Scope /Column").count(),
        2,
        "both header cells (Name, Value) should carry a column scope"
    );
    assert!(
        text.contains("/Type /OBJR"),
        "a link annotation must be referenced from the structure tree"
    );
    // The reverse of the /OBJR: the link annotation carries a /StructParent, the
    // StructTreeRoot advertises /ParentTreeNextKey, and the parent tree maps the
    // annotation's key to its owning element (PDF/UA bidirectional link).
    assert!(
        text.contains("/StructParent "),
        "a tagged link annotation needs a /StructParent back-reference"
    );
    assert!(
        text.contains("/ParentTreeNextKey"),
        "the StructTreeRoot must advertise /ParentTreeNextKey"
    );

    // Decoration is wrapped as /Artifact, and all marked content is balanced —
    // nothing is left unmarked in the tagged page (PDF/UA 7.1).
    assert!(
        text.contains("/Artifact BMC"),
        "rules, panels, stripes, and quote bars must be artifacts"
    );
    let (openers, closers) = marked_content_balance(&pdf);
    assert!(openers > 0, "the page should contain marked content");
    assert_eq!(
        openers, closers,
        "every BDC/BMC must be closed by an EMC (balanced marked content)"
    );

    // Every content-stream MCID has exactly one MCR in the structure tree.
    let content_mcids: usize = text_streams(&pdf)
        .iter()
        .map(|s| s.matches("<</MCID ").count())
        .sum();
    assert_eq!(
        content_mcids,
        text.matches("/Type /MCR").count(),
        "each content-stream MCID must be referenced by one structure MCR"
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
    // Table content is now tagged per cell (`TH` for header, `TD` for body), so
    // a table's presence on a page is the presence of any cell tag.
    let is_cell = |t: &String| t == "TH" || t == "TD";
    let table_page = pages
        .iter()
        .position(|tags| tags.iter().any(is_cell))
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
        !pages[table_page - 1].iter().any(is_cell),
        "the page before the table must not contain table cells; got {pages:?}"
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
fn pdf_keeps_a_short_intro_with_its_following_list() {
    // mwm.10: a short intro paragraph must not strand at the foot of a page while
    // the list it introduces starts the next page (list-start keep-with-next).
    let mut filler = String::new();
    for i in 1..=9 {
        filler.push_str(&format!("Filler line number {i} here.\n\n"));
    }
    let opts = small_page_opts(240.0, 150.0);

    // Control: without a list, the intro fits as the last paragraph on page 1.
    let no_list = format!("{filler}Intro for the list\n");
    let control = page_tag_sequences(&render_pdf(&no_list, &opts).unwrap());
    assert!(
        control.len() == 2 && control[1].iter().filter(|t| t.as_str() == "P").count() == 5,
        "control: the intro should sit with filler on page 1; got {control:?}"
    );

    // With a list, the intro is pulled off page 1 (4 filler P remain) to join the
    // list on a later page.
    let with_list =
        format!("{filler}Intro for the list\n\n- first item\n- second item\n- third item\n");
    let pages = page_tag_sequences(&render_pdf(&with_list, &opts).unwrap());
    assert!(
        pages.len() >= 3,
        "intro + list should paginate onto page 2+, got {pages:?}"
    );
    assert_eq!(
        pages[1].iter().filter(|t| t.as_str() == "P").count(),
        4,
        "the intro should have been pulled off the page before the list; got {pages:?}"
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

// ===========================================================================
// grn.2.3.1 — pagination: real multi-page renders that exercise the page
// builder, repeated table headers + body-row continuation, list/blockquote
// splitting, widow/orphan control, and the page-fill/break estimator.
// ===========================================================================

/// `/Type /Pages /Count N` — the number of physical pages in the document.
fn pages_count(bytes: &[u8]) -> usize {
    as_text(bytes)
        .split("/Type /Pages /Count ")
        .nth(1)
        .and_then(|tail| {
            tail.split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
        })
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0)
}

/// Distinct text baselines (physical lines) in one page content stream, recovered
/// from the `Tf 1 0 0 1 X Y Tm` text matrices. Justification can split one visual
/// line into several positioned segments, so segment count overstates lines;
/// distinct Y values are the true physical-line count.
fn baseline_count(stream: &str) -> usize {
    let mut ys = BTreeSet::new();
    for part in stream.split(" Tf 1 0 0 1 ").skip(1) {
        let mut fields = part.split_whitespace();
        let _x = fields.next();
        if let Some(y) = fields.next().and_then(|y| y.parse::<f32>().ok()) {
            ys.insert((y * 100.0).round() as i64);
        }
    }
    ys.len()
}

#[test]
fn pdf_paginates_many_section_document_without_stranding_headings() {
    // The pagination-proof.sh document in miniature: many heading + prose +
    // captioned-table sections sized so structures land at varied page
    // positions. Proves the multi-page builder, the keep-with-next heading
    // rule across many breaks, and byte determinism.
    let mut md = String::new();
    for s in 1..=12 {
        md.push_str(&format!("## Section {s}\n\n"));
        for p in 1..=3 {
            md.push_str(&format!(
                "Paragraph {p} of section {s} with enough words to wrap across a couple of \
                 lines on this narrow page and exercise the vertical page builder.\n\n"
            ));
        }
        md.push_str(&format!("Caption for table {s}\n\n"));
        md.push_str(&format!(
            "| Key | Value |\n|---|---:|\n| a | {s} |\n| b | {s}{s} |\n\n"
        ));
    }
    let mut opts = small_page_opts(260.0, 170.0);
    opts.metadata_epoch_seconds = Some(1_700_000_000);

    let pdf = render_pdf(&md, &opts).unwrap();
    assert!(
        pages_count(&pdf) > 4,
        "a 12-section narrow-page document should span several pages, got {}",
        pages_count(&pdf)
    );

    // Keep-with-next: a heading (its `/H2 <</MCID..` leaf) must never be the last
    // tagged block on a page when content follows on a later page.
    let pages = page_tag_sequences(&pdf);
    let headings = ["H", "H1", "H2", "H3", "H4", "H5", "H6"];
    for (i, tags) in pages.iter().enumerate() {
        if i + 1 < pages.len() {
            if let Some(last) = tags.last() {
                assert!(
                    !headings.contains(&last.as_str()),
                    "page {i} ends with a stranded heading {last:?}; pages={pages:?}"
                );
            }
        }
    }

    // Determinism across runs (pinned metadata epoch).
    let again = render_pdf(&md, &opts).unwrap();
    assert_eq!(pdf, again, "multi-page render must be byte-deterministic");
}

#[test]
fn pdf_splits_long_list_across_pages_keeping_list_tags() {
    let opts = small_page_opts(240.0, 120.0);
    let mut md = String::from("Intro paragraph before the list.\n\n");
    for i in 1..=14 {
        md.push_str(&format!(
            "- list item number {i} with a few trailing words\n"
        ));
    }
    let pdf = render_pdf(&md, &opts).unwrap();
    let text = as_text(&pdf);
    let streams = text_streams(&pdf);

    assert!(
        pages_count(&pdf) > 1,
        "a 14-item list on a short page should paginate, got {}",
        pages_count(&pdf)
    );
    assert!(
        streams.len() >= 2 && streams.iter().all(|s| s.contains("BT /F")),
        "every list page should carry continuation list text"
    );
    for tag in ["/S /L", "/S /LI", "/S /LBody"] {
        assert!(
            text.contains(tag),
            "split list must keep its {tag} structure"
        );
    }
    let (openers, closers) = marked_content_balance(&pdf);
    assert!(openers > 0 && openers == closers, "balanced marked content");
}

#[test]
fn pdf_splits_long_blockquote_across_pages_repeating_gutter_bar() {
    let opts = small_page_opts(240.0, 110.0);
    let mut md = String::new();
    for i in 1..=14 {
        md.push_str(&format!(
            "> quoted line {i} carrying enough words to fill the measure\n>\n"
        ));
    }
    let pdf = render_pdf(&md, &opts).unwrap();
    let text = as_text(&pdf);
    let streams = text_streams(&pdf);

    assert!(
        pages_count(&pdf) > 1,
        "a long blockquote on a short page should paginate, got {}",
        pages_count(&pdf)
    );
    assert!(
        text.contains("/S /BlockQuote"),
        "split quote keeps BlockQuote tag"
    );
    // The decorative gutter bar is stroked per page the quote occupies: a 2.50 w
    // stroke wrapped as an /Artifact. It must appear on more than one page stream.
    let pages_with_bar = streams
        .iter()
        .filter(|s| s.contains("0.820 0.851 0.878 RG 2.50 w"))
        .count();
    assert!(
        pages_with_bar >= 2,
        "the blockquote gutter bar should repeat on each continuation page, got {pages_with_bar}"
    );
    let (openers, closers) = marked_content_balance(&pdf);
    assert!(openers > 0 && openers == closers, "balanced marked content");
}

#[test]
fn pdf_table_body_row_wraps_across_page_break_and_repeats_header() {
    // A single body row with a long wrapping cell, on a very short page, forces a
    // mid-row page break. The continuation page must (a) repeat the bold header
    // and (b) resume the body row's wrapped lines — exercising the orphan
    // body-row-wrap continuation path in the per-cell structure writer.
    let opts = small_page_opts(240.0, 110.0);
    let long = "wrap word ".repeat(40);
    let md = format!(
        "| Name | Notes |\n|---|---|\n| alpha | {long} |\n| beta | short note here |\n\
         | gamma | another short note |\n"
    );
    let pdf = render_pdf(&md, &opts).unwrap();
    let text = as_text(&pdf);
    let streams = text_streams(&pdf);

    assert!(
        pages_count(&pdf) > 1,
        "a wrapping body row on a short page should paginate, got {}",
        pages_count(&pdf)
    );
    assert!(
        streams.len() >= 2,
        "table should span multiple content streams"
    );
    assert!(
        streams[0].contains("/F2 10.00 Tf"),
        "the first page carries the bold table header"
    );
    // A continuation page repeats the bold header (F2) above resumed body-row
    // wrap lines (F1) — the orphan body-row-wrap continuation path.
    assert!(
        streams
            .iter()
            .skip(1)
            .any(|s| s.contains("/F2 10.00 Tf") && s.contains("/F1 10.00 Tf")),
        "a continuation page should repeat the header above resumed body rows"
    );
    assert!(
        text.contains("/S /TH") && text.contains("/S /TD"),
        "per-cell tags survive the split"
    );
    let (openers, closers) = marked_content_balance(&pdf);
    assert!(
        openers > 0 && openers == closers,
        "balanced marked content across the split"
    );
}

#[test]
fn pdf_avoids_widow_when_splitting_a_soft_wrapped_paragraph() {
    // One genuine soft-wrapped paragraph (a single flow group, no hard breaks)
    // that must split across short pages. The club/widow penalty must keep at
    // least two physical baselines on each page rather than stranding a lone line.
    let opts = small_page_opts(170.0, 90.0);
    let body = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi \
                omicron pi rho sigma tau upsilon phi chi psi omega kappa lambda alpha beta \
                gamma delta epsilon zeta";
    let pdf = render_pdf(body, &opts).unwrap();
    let streams = text_streams(&pdf);

    assert!(
        pages_count(&pdf) > 1 && streams.len() >= 2,
        "the paragraph should split across at least two pages, got {} page(s)",
        pages_count(&pdf)
    );
    for (i, stream) in streams.iter().enumerate() {
        let lines = baseline_count(stream);
        assert!(
            lines >= 2,
            "page {i} holds a single-line widow/orphan ({lines} baselines)"
        );
    }
}

#[test]
fn pdf_reuses_hyphenation_cache_for_repeated_words() {
    // A narrow paragraph repeating the same long word (lowercase and Capitalized)
    // exercises the per-document hyphenation cache: the first occurrence computes
    // + inserts, later occurrences hit the cache, and the capitalized form folds
    // case before the lookup. Output must still synthesize discretionary hyphens.
    let opts = small_page_opts(90.0, 320.0);
    let md = "internationalization internationalization Internationalization \
              internationalization Internationalization internationalization";
    let pdf = render_pdf(md, &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("<002D>"),
        "repeated hyphenating words should still emit discretionary hyphens"
    );
    assert!(
        text_streams(&pdf).join("\n").matches("BT /F").count() >= 4,
        "the repeated long words should wrap into several physical rows"
    );
    // Cache reuse must not perturb deterministic output.
    assert_eq!(
        pdf,
        render_pdf(md, &opts).unwrap(),
        "cache path stays deterministic"
    );
}

// ===========================================================================
// grn.2.3.2 — accessibility/tagging: heading levels H3–H6, ordered/task list
// markers, the /Nums parent tree, multi-page marked-content balance, empty
// table cells, and strikethrough runs.
// ===========================================================================

#[test]
fn pdf_tags_h3_through_h6_with_generic_heading_collapse() {
    let pdf = render_pdf(
        "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n\nBody text.\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    for tag in ["/S /H1 ", "/S /H2 ", "/S /H3 "] {
        assert!(text.contains(tag), "explicit heading level missing: {tag}");
    }
    // H4–H6 share the body measure, so the writer cannot recover the source level
    // and collapses them to the generic `/H`. There are three such headings.
    assert_eq!(
        text.matches("/S /H /P").count(),
        3,
        "H4/H5/H6 should each collapse to a generic /H structure element"
    );
    // Each heading still produces an outline destination/title.
    assert!(text.contains("/Outlines ") && text.contains("/Title (Three)"));
}

#[test]
fn pdf_tags_task_list_markers_and_keeps_them_selectable() {
    let pdf = render_pdf("- [x] done item\n- [ ] todo item\n", &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);

    for tag in ["/S /L", "/S /LI", "/S /LBody"] {
        assert!(text.contains(tag), "task list missing structure tag {tag}");
    }
    // The checkbox marker glyphs `[`, `]`, and `x` must remain copyable.
    for scalar in ["<005B>", "<005D>", "<0078>"] {
        assert!(
            text.contains(scalar),
            "task marker scalar {scalar} must be selectable"
        );
    }
}

#[test]
fn pdf_tags_ordered_list_markers_with_custom_start() {
    let pdf = render_pdf("3. gamma\n4. delta\n", &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/S /L"),
        "ordered list keeps an /L structure element"
    );
    // Markers `3.` and `4.` (start = 3) render digit glyphs not present in the
    // body words `gamma`/`delta`, so their scalars come from the numbering.
    for scalar in ["<0033>", "<0034>"] {
        assert!(
            text.contains(scalar),
            "ordered marker digit {scalar} must be selectable"
        );
    }
}

#[test]
fn pdf_parent_tree_nums_maps_page_and_link_annotation_keys() {
    let pdf = render_pdf(
        "# Linked\n\nVisit [the site](https://example.com/docs) for more.\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    // The parent tree's /Nums array maps page StructParents key 0 to its element
    // list, then the link annotation's key (>= page count) back to its /Link.
    let nums = text
        .split("/Nums [ ")
        .nth(1)
        .and_then(|tail| tail.split(" ] >>").next())
        .expect("parent tree should emit a /Nums array");
    assert!(
        nums.starts_with("0 ["),
        "page key 0 should head the parent tree: {nums:?}"
    );
    assert!(
        nums.contains("0 R"),
        "the /Nums array references structure elements"
    );
    // The annotation key advertised by /ParentTreeNextKey must exceed the page
    // count (single page here, so keys start at 1).
    let next_key = text
        .split("/ParentTreeNextKey ")
        .nth(1)
        .and_then(|tail| {
            tail.split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
        })
        .and_then(|n| n.parse::<usize>().ok())
        .expect("StructTreeRoot advertises /ParentTreeNextKey");
    assert!(
        next_key >= 2,
        "annotation key should follow the page key, got {next_key}"
    );
}

#[test]
fn pdf_marked_content_balances_across_a_multi_page_tagged_document() {
    let opts = small_page_opts(240.0, 120.0);
    let mut md = String::from("# Report\n\n");
    for i in 1..=10 {
        md.push_str(&format!(
            "Paragraph {i} with a [link](https://example.com/{i}) and `code` inline.\n\n"
        ));
    }
    let pdf = render_pdf(&md, &opts).unwrap();

    assert!(pages_count(&pdf) > 1, "report should span multiple pages");
    let streams = text_streams(&pdf);
    assert!(
        streams.len() >= 2 && streams.iter().all(|s| s.contains("BT /F")),
        "small pages should keep every page content stream uncompressed + inspectable"
    );
    let (openers, closers) = marked_content_balance(&pdf);
    assert!(openers > 0, "tagged pages contain marked content");
    assert_eq!(
        openers, closers,
        "every BDC/BMC closes with an EMC across all pages"
    );

    // Every content-stream MCID is referenced by exactly one structure MCR.
    let content_mcids: usize = streams.iter().map(|s| s.matches("<</MCID ").count()).sum();
    assert_eq!(
        content_mcids,
        as_text(&pdf).matches("/Type /MCR").count(),
        "each MCID across pages maps to one MCR"
    );
}

#[test]
fn pdf_table_with_empty_cells_stays_balanced_and_tagged() {
    // An empty body cell emits no seg (no `/TD` backfill, a documented limitation)
    // but the surrounding cells must still tag and the marked content must balance.
    let pdf = render_pdf(
        "| Name | Value |\n|---|---|\n| alpha |  |\n|  | 7 |\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/S /TH"), "header cells tag as /TH");
    assert!(text.contains("/S /TD"), "non-empty body cells tag as /TD");
    let (openers, closers) = marked_content_balance(&pdf);
    assert!(
        openers > 0 && openers == closers,
        "empty cells must not unbalance marks"
    );
}

#[test]
fn pdf_strikethrough_runs_draw_a_stroke_and_stay_selectable() {
    let plain = render_pdf("deletedword", &PdfOptions::default()).unwrap();
    let struck = render_pdf("~~deletedword~~", &PdfOptions::default()).unwrap();

    let plain_stream = text_streams(&plain).join("\n");
    let struck_stream = text_streams(&struck).join("\n");

    assert!(
        !plain_stream.contains(" l S\n"),
        "plain prose has no strike/underline stroke in its text stream"
    );
    assert!(
        struck_stream.contains(" m ") && struck_stream.contains(" l S"),
        "a strikethrough run draws a stroke through the text"
    );
    // The struck word's glyphs remain copyable (subset + ToUnicode).
    assert!(
        as_text(&struck).contains("<0064>"),
        "the letter d of the struck word should still be selectable"
    );
}

// ===========================================================================
// grn.2.3.3 — images, fonts, and content streams: grayscale image XObjects,
// PNG rejection paths, ancillary chunks, empty/blank code fences, and supplied
// font assets.
// ===========================================================================

/// A minimal 8-bit grayscale (color type 0) PNG, one row of `pixels`.
fn tiny_gray_png(pixels: &[u8]) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(pixels.len() as u32).to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]); // 8-bit grayscale, deflate, PNG filters, no interlace.

    let mut rows = Vec::with_capacity(1 + pixels.len());
    rows.push(0); // filter type 0.
    rows.extend_from_slice(pixels);
    let idat = franken_markdown::compress::zlib_compress(&rows);

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

/// 13-byte IHDR payload with explicit bit depth + color type.
fn ihdr_data(width: u32, height: u32, bit_depth: u8, color_type: u8) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&width.to_be_bytes());
    d.extend_from_slice(&height.to_be_bytes());
    d.extend_from_slice(&[bit_depth, color_type, 0, 0, 0]);
    d
}

#[test]
fn pdf_renders_grayscale_png_as_devicegray_xobject() {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "images/gray.png",
            tiny_gray_png(&[0x20, 0xC0]),
        )],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Gray ramp](images/gray.png)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Subtype /Image"),
        "grayscale PNG becomes an image XObject"
    );
    assert!(
        text.contains("/ColorSpace /DeviceGray"),
        "grayscale uses DeviceGray"
    );
    assert!(
        text.contains("/Predictor 15 /Colors 1 /BitsPerComponent 8 /Columns 2"),
        "grayscale DecodeParms carry a single color component"
    );
    assert!(
        text.contains("/Im1 Do"),
        "page content draws the grayscale image"
    );
    assert!(text.contains("/S /Figure") && text.contains("/Alt (Gray ramp)"));
}

#[test]
fn pdf_rejects_unsupported_png_color_and_geometry_variants() {
    let valid_idat = franken_markdown::compress::zlib_compress(&[0u8; 8]);

    // Palette (color type 3) — unsupported color model.
    let mut palette = Vec::new();
    palette.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    palette.extend_from_slice(&png_chunk(b"IHDR", &ihdr_data(2, 1, 8, 3)));
    palette.extend_from_slice(&png_chunk(b"IDAT", &valid_idat));
    palette.extend_from_slice(&png_chunk(b"IEND", &[]));

    // 16-bit depth — only 8-bit is accepted.
    let mut deep = Vec::new();
    deep.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    deep.extend_from_slice(&png_chunk(b"IHDR", &ihdr_data(2, 1, 16, 2)));
    deep.extend_from_slice(&png_chunk(b"IDAT", &valid_idat));
    deep.extend_from_slice(&png_chunk(b"IEND", &[]));

    // Wrong IHDR length (12 bytes, not 13).
    let mut bad_ihdr = Vec::new();
    bad_ihdr.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    bad_ihdr.extend_from_slice(&png_chunk(b"IHDR", &[0u8; 12]));
    bad_ihdr.extend_from_slice(&png_chunk(b"IEND", &[]));

    // Missing IDAT entirely.
    let mut no_idat = Vec::new();
    no_idat.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    no_idat.extend_from_slice(&png_chunk(b"IHDR", &ihdr_data(2, 1, 8, 2)));
    no_idat.extend_from_slice(&png_chunk(b"IEND", &[]));

    // Truncated chunk: a declared length that runs past the end of the buffer.
    let mut truncated = Vec::new();
    truncated.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    truncated.extend_from_slice(&png_chunk(b"IHDR", &ihdr_data(2, 1, 8, 2)));
    truncated.extend_from_slice(&0xFFFF_FFF0u32.to_be_bytes());
    truncated.extend_from_slice(b"IDAT");

    for (dest, bytes) in [
        ("images/palette.png", palette),
        ("images/deep.png", deep),
        ("images/badihdr.png", bad_ihdr),
        ("images/noidat.png", no_idat),
        ("images/truncated.png", truncated),
    ] {
        let opts = PdfOptions {
            image_assets: vec![PdfImageAsset::new(dest, bytes)],
            ..PdfOptions::default()
        };
        let pdf = render_pdf(&format!("![Rejected]({dest})"), &opts).unwrap();
        let text = as_text(&pdf);
        assert!(
            !text.contains("/Subtype /Image"),
            "{dest}: unsupported PNG must not become an image XObject"
        );
        assert!(
            text.contains("BT /F"),
            "{dest}: rejected image should render visible alt text"
        );
    }
}

#[test]
fn pdf_accepts_png_with_ancillary_chunk_after_ihdr() {
    // An ancillary chunk (here a tEXt) between IHDR and IDAT must be skipped, not
    // rejected — the image should still embed.
    let idat = {
        let rows = vec![0u8, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60]; // filter byte + 2 RGB pixels.
        franken_markdown::compress::zlib_compress(&rows)
    };
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr_data(2, 1, 8, 2)));
    png.extend_from_slice(&png_chunk(b"tEXt", b"after ihdr"));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));

    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("images/ancillary.png", png)],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Has metadata](images/ancillary.png)", &opts).unwrap();
    let text = as_text(&pdf);
    assert!(
        text.contains("/Subtype /Image"),
        "ancillary chunk after IHDR should not block embedding"
    );
    assert!(text.contains("/ColorSpace /DeviceRGB") && text.contains("/Im1 Do"));
}

#[test]
fn pdf_empty_image_destination_falls_back_to_alt_text() {
    // `![alt]()` resolves to an empty destination, which short-circuits image
    // resolution; the alt text must render as ordinary prose.
    let pdf = render_pdf("![only alt text]()", &PdfOptions::default()).unwrap();
    let text = as_text(&pdf);
    assert!(
        !text.contains("/Subtype /Image"),
        "empty destination has no image"
    );
    assert!(
        text.contains("BT /F"),
        "empty-destination image renders alt text"
    );
}

#[test]
fn pdf_empty_code_fence_emits_a_panel() {
    // An empty fenced block still gets a one-line-tall panel. With no line
    // numbers it has no visible glyphs (so no tagged content), but the decorative
    // panel tint is still drawn.
    let plain = render_pdf("```text\n```\n", &PdfOptions::default()).unwrap();
    assert!(
        as_text(&plain).contains("0.965 0.973 0.980 rg"),
        "empty fence draws the code panel background tint"
    );

    // With line numbers, the empty fence still emits a muted, selectable "1" run,
    // which is visible content and therefore tags as /Code.
    let numbered = render_pdf(
        "```text\n```\n",
        &PdfOptions {
            code_line_numbers: true,
            ..PdfOptions::default()
        },
    )
    .unwrap();
    let numbered_text = as_text(&numbered);
    assert!(
        numbered_text.contains("/S /Code"),
        "numbered empty fence tags as /Code"
    );
    assert!(
        numbered_text.contains("0.431 0.467 0.506 rg"),
        "an empty numbered fence still emits a muted line-number run"
    );
    assert!(
        numbered_text.contains("<0031>"),
        "line number 1 is selectable"
    );
}

#[test]
fn pdf_code_block_with_blank_lines_keeps_numbered_panel_rows() {
    let opts = PdfOptions {
        code_line_numbers: true,
        ..PdfOptions::default()
    };
    let pdf = render_pdf("```text\nalpha\n\nbeta\n```\n", &opts).unwrap();
    let streams = text_streams(&pdf).join("\n");
    let text = as_text(&pdf);

    assert!(
        text.contains("/S /Code"),
        "blank-line code still tags as /Code"
    );
    // Three source rows (alpha, blank, beta) each emit a line-number run; the
    // blank row produces an empty code row that still carries its number column.
    assert!(
        streams.matches("/F4 9.50 Tf").count() >= 4,
        "numbered code with a blank line should still render number + code runs"
    );
    assert!(
        text.contains("<0033>"),
        "line number 3 (beta) should be selectable"
    );
}

#[test]
fn pdf_accepts_supplied_bundled_font_assets() {
    // Supplying caller-provided font bytes exercises the supplied-asset path
    // (vs. the bundled fallback). Bundled bytes are valid, subsettable faces, so
    // the result is still a well-formed embedded-font PDF.
    use franken_markdown::FontFamily;
    use franken_markdown::fonts::{self, FontStyle};

    let assets = franken_markdown::FontAssets {
        body_regular: Some(fonts::body_bytes(FontFamily::Sans, FontStyle::Regular).to_vec()),
        body_bold: Some(fonts::body_bytes(FontFamily::Sans, FontStyle::Bold).to_vec()),
        body_italic: Some(fonts::body_bytes(FontFamily::Sans, FontStyle::Italic).to_vec()),
        body_bold_italic: Some(fonts::body_bytes(FontFamily::Sans, FontStyle::BoldItalic).to_vec()),
        mono_regular: Some(fonts::mono_bytes(FontStyle::Regular).to_vec()),
    };
    let opts = PdfOptions {
        font_assets: assets,
        ..PdfOptions::default()
    };
    let pdf = render_pdf("Plain **bold** *italic* `code` text.", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(pdf.starts_with(b"%PDF-1.7\n") && pdf.ends_with(b"%%EOF\n"));
    assert!(
        text.contains("/FontFile2"),
        "supplied fonts still embed as subset programs"
    );
    assert!(text.contains("/Subtype /Type0") && text.contains("/ToUnicode"));
}

#[test]
fn pdf_table_cells_render_inline_styling_and_links() {
    // A single body cell mixes bold/italic/code/strikethrough; another holds a
    // link. Previously every cell was flattened to one plain slot with no link.
    let md = "| Style | Link |\n|---|---|\n\
              | **bold** *ital* `code` ~~s~~ | [site](https://example.com) |";
    let pdf = render_pdf(md, &PdfOptions::default()).unwrap();

    // The decompressed page content uses distinct faces inside one row: body
    // (F1), bold (F2), italic (F3), and mono (F4) — proof the cell is no longer
    // collapsed to a single slot.
    let streams = text_streams(&pdf).join("\n");
    for (slot, what) in [("F2", "bold"), ("F3", "italic"), ("F4", "mono")] {
        assert!(
            streams.contains(&format!("/{slot} 10.00 Tf")),
            "table cell should render a {what} run (/{slot})"
        );
    }

    // The cell link is a real clickable annotation with its URI, not dead text.
    let raw = as_text(&pdf);
    assert!(
        raw.contains("/Subtype /Link"),
        "cell link must be an annotation"
    );
    assert!(
        raw.contains("https://example.com"),
        "cell link URI must be embedded"
    );
}

#[test]
fn pdf_table_cells_wrap_and_align_without_panic() {
    // A right-aligned numeric column, a centered column, an empty cell, and a
    // long cell that must wrap exercise the styled cell alignment + wrapping.
    let md = "| Left | Mid | Right |\n|:---|:--:|---:|\n\
              | a very long cell that has to wrap across several lines inside its column | m | 12345 |\n\
              |  | **b** | 9 |";
    let pdf = render_pdf(md, &PdfOptions::default()).unwrap();
    assert!(pdf.starts_with(b"%PDF-") && pdf.ends_with(b"%%EOF\n"));
    let raw = as_text(&pdf);
    assert!(raw.contains("/StructTreeRoot"), "table stays tagged");
}
