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

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        out.push(TABLE[(chunk[0] >> 2) as usize] as char);
        out.push(TABLE[(((chunk[0] & 0x03) << 4) | (chunk[1] >> 4)) as usize] as char);
        out.push(TABLE[(((chunk[1] & 0x0f) << 2) | (chunk[2] >> 6)) as usize] as char);
        out.push(TABLE[(chunk[2] & 0x3f) as usize] as char);
    }
    match chunks.remainder() {
        [a] => {
            out.push(TABLE[(a >> 2) as usize] as char);
            out.push(TABLE[((a & 0x03) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            out.push(TABLE[(a >> 2) as usize] as char);
            out.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            out.push(TABLE[((b & 0x0f) << 2) as usize] as char);
            out.push('=');
        }
        [] => {}
        _ => unreachable!(),
    }
    out
}

fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn utf16be_pdf_hex_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        out.push_str(&format!("{unit:04X}"));
    }
    out.push('>');
    out
}

fn first_text_matrix_xy_after(pdf_text: &str, marker: &str) -> (f32, f32) {
    let tail = pdf_text
        .split(marker)
        .nth(1)
        .unwrap_or_else(|| panic!("marker not found in PDF text: {marker}\n{pdf_text}"));
    let matrix_tail = tail
        .split(" Tf ")
        .nth(1)
        .unwrap_or_else(|| panic!("text font operator not found after marker {marker}: {tail}"));
    let mut parts = matrix_tail.split_whitespace();
    let _a = parts.next();
    let _b = parts.next();
    let _c = parts.next();
    let _d = parts.next();
    let x = parts
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or_else(|| panic!("text matrix x not found after marker {marker}: {matrix_tail}"));
    let y = parts
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or_else(|| panic!("text matrix y not found after marker {marker}: {matrix_tail}"));
    (x, y)
}

fn first_text_font_size_after(pdf_text: &str, marker: &str) -> f32 {
    let tail = pdf_text
        .split(marker)
        .nth(1)
        .unwrap_or_else(|| panic!("marker not found in PDF text: {marker}\n{pdf_text}"));
    let font_tail = tail
        .split(" Tf ")
        .next()
        .unwrap_or_else(|| panic!("text font operator not found after marker {marker}: {tail}"));
    font_tail
        .split_whitespace()
        .last()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or_else(|| panic!("text font size not found after marker {marker}: {font_tail}"))
}

fn first_text_object_after(pdf_text: &str, marker: &str) -> String {
    let tail = pdf_text
        .split(marker)
        .nth(1)
        .unwrap_or_else(|| panic!("marker not found in PDF text: {marker}\n{pdf_text}"));
    let end = tail
        .find(" ET")
        .unwrap_or_else(|| panic!("text object end not found after marker {marker}: {tail}"));
    tail[..end].to_string()
}

fn first_graphics_state_after<'a>(pdf_text: &'a str, marker: &str) -> &'a str {
    let tail = pdf_text
        .split(marker)
        .nth(1)
        .unwrap_or_else(|| panic!("marker not found in PDF text: {marker}\n{pdf_text}"));
    let end = tail
        .find("\nQ\n")
        .unwrap_or_else(|| panic!("graphics state end not found after marker {marker}: {tail}"));
    &tail[..end]
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

fn trailer_file_id(text: &str) -> (&str, &str) {
    let marker = "/ID [<";
    let start = text.find(marker).expect("PDF trailer should include /ID") + marker.len();
    let first = &text[start..start + 32];
    let rest = &text[start + 32..];
    assert!(rest.starts_with("> <"), "unexpected /ID separator: {rest}");
    let second = &rest[3..35];
    assert!(
        rest[35..].starts_with(">]"),
        "unexpected /ID terminator: {}",
        &rest[35..]
    );
    (first, second)
}

#[test]
fn pdf_trailer_has_deterministic_content_sensitive_file_id() {
    let opts = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let first_pdf = render_pdf("Body.", &opts).unwrap();
    let second_pdf = render_pdf("Body.", &opts).unwrap();
    let changed_pdf = render_pdf("Different body.", &opts).unwrap();

    assert_eq!(first_pdf, second_pdf, "same input should stay byte-stable");

    let text = as_text(&first_pdf);
    let (first, second) = trailer_file_id(&text);
    assert_eq!(
        first, second,
        "generated PDFs use one stable file identifier"
    );
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(
        first, "00000000000000000000000000000000",
        "file ID should not be the all-zero sentinel"
    );

    let changed_text = as_text(&changed_pdf);
    let (changed, _) = trailer_file_id(&changed_text);
    assert_ne!(first, changed, "file ID should reflect PDF content");
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
fn pdf_renders_supplied_svg_image_as_vector_content() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 80" role="img">
  <defs>
    <marker id="arrow-end"><path d="M0 0 L8 3.5 L0 7 Z" fill="#94a3b8"/></marker>
  </defs>
  <path d="M64 40 C78 40 82 40 96 40" fill="none" stroke="#94a3b8" stroke-width="2" marker-end="url(#arrow-end)"/>
  <rect x="12" y="18" width="52" height="44" rx="6" fill="#ffffff" stroke="#e2e8f0" stroke-width="1.5"/>
  <rect x="96" y="18" width="52" height="44" fill="#ffffff" stroke="#e2e8f0" stroke-width="1.5"/>
  <text x="38" y="45" text-anchor="middle" font-size="14" fill="#1a1a2e">Parse</text>
  <text x="122" y="45" text-anchor="middle" font-size="14" fill="#1a1a2e">PDF</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("diagrams/flow.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Flow chart](diagrams/flow.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("/Subtype /Image"),
        "SVG assets should render as page-stream vector content, not raster XObjects"
    );
    assert!(
        !text.contains("/XObject << /Im"),
        "a pure-SVG document should not allocate image resources"
    );
    assert!(
        text.contains("0.75 0 0 -0.75"),
        "SVG user units should be mapped from 96dpi CSS pixels into PDF points"
    );
    assert!(
        text.contains(" re "),
        "rectangles should become PDF path operators"
    );
    assert!(
        text.contains(" c "),
        "curves and rounded corners should emit cubic paths"
    );
    assert!(
        text.contains("BT /F1"),
        "SVG labels should remain real selectable PDF text"
    );
    assert!(
        text.contains("/S /Figure"),
        "tagged structure marks SVG images as figures"
    );
    assert!(
        text.contains("/Alt (Flow chart)"),
        "SVG figure alt text should be carried into the structure element"
    );
    assert!(
        text.contains("/O /Layout /BBox ["),
        "SVG figures should carry a layout bounding box"
    );
}

#[test]
fn pdf_svg_line_and_poly_markers_render_as_vector_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 64">
  <defs>
    <marker id="arrow" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto" markerUnits="strokeWidth">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <line x1="8" y1="8" x2="64" y2="8" stroke="#0000ff" stroke-width="2" marker-start="url(#arrow)" marker-end="url(#arrow)"/>
  <polyline points="8,24 32,36 64,24" fill="none" stroke="#00aa00" stroke-width="2" marker-mid="url(#arrow)" marker-end="url(#arrow)"/>
  <polygon points="8,48 32,58 64,48" fill="none" stroke="#00aa00" stroke-width="2" marker="url(#arrow)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("markers.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Markers](markers.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 0.000 1.000 RG 2 w 0 J 0 j 4 M 8 8 m 64 8 l S"),
        "the line itself should still render with the ordinary vector stroke: {text}"
    );
    let marker_paints = text
        .matches("1.000 0.000 0.000 rg 0 0 m 8 4 l 0 8 l h f")
        .count();
    assert!(
        marker_paints >= 7,
        "line start/end, polyline mid/end, and polygon marker shorthand should all paint vector marker shapes; saw {marker_paints}\n{text}"
    );
}

#[test]
fn pdf_svg_marker_orient_auto_start_reverse_only_reverses_starts() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <defs>
    <marker id="auto" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto" markerUnits="strokeWidth">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000"/>
    </marker>
    <marker id="reverse" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto-start-reverse" markerUnits="strokeWidth">
      <path d="M0 0 L8 4 L0 8 Z" fill="#00ff00"/>
    </marker>
  </defs>
  <line x1="8" y1="8" x2="64" y2="8" stroke="#0000ff" stroke-width="2" marker-start="url(#auto)" marker-end="url(#auto)"/>
  <line x1="8" y1="24" x2="64" y2="24" stroke="#0000ff" stroke-width="2" marker-start="url(#reverse)" marker-end="url(#reverse)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("marker-orient.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Marker orientation](marker-orient.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 8 8 cm 2 0 0 2 0 0 cm 1 0 0 1 -8 -4 cm"),
        "plain orient=auto marker-start should follow the outgoing path tangent: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 64 8 cm 2 0 0 2 0 0 cm 1 0 0 1 -8 -4 cm"),
        "plain orient=auto marker-end should follow the incoming path tangent: {text}"
    );
    assert!(
        text.contains("q -1 0 0 -1 8 24 cm 2 0 0 2 0 0 cm 1 0 0 1 -8 -4 cm"),
        "orient=auto-start-reverse should flip only marker-start: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 64 24 cm 2 0 0 2 0 0 cm 1 0 0 1 -8 -4 cm"),
        "orient=auto-start-reverse should behave like auto for marker-end: {text}"
    );
}

#[test]
fn pdf_svg_marker_orient_default_and_angle_are_fixed_user_space_angles() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 64">
  <defs>
    <marker id="default" markerWidth="8" markerHeight="8" refX="4" refY="4" markerUnits="userSpaceOnUse">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000"/>
    </marker>
    <marker id="ninety" markerWidth="8" markerHeight="8" refX="4" refY="4" orient="90deg" markerUnits="userSpaceOnUse">
      <path d="M0 0 L8 4 L0 8 Z" fill="#00ff00"/>
    </marker>
  </defs>
  <line x1="8" y1="8" x2="8" y2="56" stroke="#0000ff" stroke-width="2" marker-end="url(#default)"/>
  <line x1="24" y1="8" x2="24" y2="56" stroke="#0000ff" stroke-width="2" marker-end="url(#ninety)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("marker-fixed-orient.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf(
        "![Fixed marker orientation](marker-fixed-orient.svg)",
        &opts,
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 8 56 cm 1 0 0 1 0 0 cm 1 0 0 1 -4 -4 cm"),
        "missing orient should use the SVG default fixed 0-degree marker angle, not auto tangent rotation: {text}"
    );
    assert!(
        text.contains("q 0 1 -1 0 24 56 cm 1 0 0 1 0 0 cm 1 0 0 1 -4 -4 cm"),
        "orient=90deg should rotate the marker in user space: {text}"
    );
}

#[test]
fn pdf_svg_marker_viewbox_scales_marker_body_and_reference_point() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <defs>
    <marker id="scaled" viewBox="0 0 10 10" markerWidth="5" markerHeight="5" refX="10" refY="5" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L10 5 L0 10 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <line x1="4" y1="10" x2="24" y2="10" stroke="#0000ff" stroke-width="2" marker-end="url(#scaled)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("marker-viewbox.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Marker viewBox](marker-viewbox.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 24 10 cm 1 0 0 1 0 0 cm 1 0 0 1 -5 -2.5 cm 0.5 0 0 0.5 0 0 cm"),
        "marker viewBox should fit into markerWidth/markerHeight and align the mapped ref point: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg 0 0 m 10 5 l 0 10 l h f"),
        "marker body should remain vector path content under the marker-local viewBox transform: {text}"
    );
}

#[test]
fn pdf_svg_marker_viewbox_honors_preserve_aspect_ratio() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 32">
  <defs>
    <marker id="meet" viewBox="0 0 20 10" markerWidth="10" markerHeight="10" refX="20" refY="5" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L20 5 L0 10 Z" fill="#ff0000"/>
    </marker>
    <marker id="none" viewBox="0 0 20 10" markerWidth="10" markerHeight="20" refX="20" refY="5" orient="auto" markerUnits="userSpaceOnUse" preserveAspectRatio="none">
      <path d="M0 0 L20 5 L0 10 Z" fill="#00ff00"/>
    </marker>
  </defs>
  <line x1="4" y1="8" x2="24" y2="8" stroke="#0000ff" stroke-width="2" marker-end="url(#meet)"/>
  <line x1="4" y1="24" x2="24" y2="24" stroke="#0000ff" stroke-width="2" marker-end="url(#none)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("marker-aspect.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Marker aspect](marker-aspect.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 24 8 cm 1 0 0 1 0 0 cm 1 0 0 1 -10 -5 cm 0.5 0 0 0.5 0 2.5 cm"),
        "default marker preserveAspectRatio should meet-scale and center a wide viewBox vertically: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 24 24 cm 1 0 0 1 0 0 cm 1 0 0 1 -10 -10 cm 0.5 0 0 2 0 0 cm"),
        "marker preserveAspectRatio=none should use non-uniform viewBox scaling and map refY through that scale: {text}"
    );
}

#[test]
fn pdf_svg_marker_context_paint_inherits_referencing_shape_fill_and_stroke() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 30">
  <defs>
    <marker id="ctx" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L8 4 L0 8 Z" fill="context-fill" stroke="context-stroke"/>
    </marker>
  </defs>
  <polygon points="4,4 28,15 4,26" fill="#00ff00" stroke="#0000ff" stroke-width="2" marker-end="url(#ctx)"/>
  <line x1="36" y1="15" x2="56" y2="15" fill="#ff00ff" stroke="#ff0000" stroke-width="2" marker-end="url(#ctx)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("marker-context.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Marker context paint](marker-context.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "0.000 1.000 0.000 rg 0.000 0.000 1.000 RG 1 w 0 J 0 j 4 M 0 0 m 8 4 l 0 8 l h B"
        ),
        "marker child fill=context-fill and stroke=context-stroke should inherit the referencing polygon paint: {text}"
    );
    assert!(
        text.contains(
            "1.000 0.000 1.000 rg 1.000 0.000 0.000 RG 1 w 0 J 0 j 4 M 0 0 m 8 4 l 0 8 l h B"
        ),
        "marker context-fill should see an explicit fill on a referencing line even though the line itself is stroke-only: {text}"
    );
}

#[test]
fn pdf_svg_markers_render_on_unstroked_referencing_shapes_when_marker_has_own_paint() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 72 42">
  <defs>
    <marker id="own" markerWidth="6" markerHeight="6" refX="6" refY="3" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L6 3 L0 6 Z" fill="#00ff00"/>
    </marker>
    <marker id="ctx-fill" markerWidth="6" markerHeight="6" refX="6" refY="3" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L6 3 L0 6 Z" fill="context-fill"/>
    </marker>
    <marker id="ctx-stroke" markerWidth="6" markerHeight="6" refX="6" refY="3" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L6 3 L0 6 Z" fill="context-stroke"/>
    </marker>
  </defs>
  <line x1="4" y1="6" x2="24" y2="6" stroke="none" marker-end="url(#own)"/>
  <path d="M4 16 L24 16" fill="none" stroke="none" marker-end="url(#own)"/>
  <polyline points="4,30 14,36 24,30" fill="none" stroke="none" marker-mid="url(#own)" marker-end="url(#own)"/>
  <polygon points="34,4 58,14 34,24" fill="#ff00ff" stroke="none" marker-end="url(#ctx-fill)"/>
  <line x1="34" y1="34" x2="58" y2="34" stroke="none" marker-end="url(#ctx-stroke)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("unstroked-marker.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Unstroked markers](unstroked-marker.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let explicit_green_markers = text
        .matches("0.000 1.000 0.000 rg 0 0 m 6 3 l 0 6 l h f")
        .count();
    assert_eq!(
        explicit_green_markers, 4,
        "unstroked line/path/polyline references should still place explicitly painted marker shapes; saw {explicit_green_markers}\n{text}"
    );
    assert!(
        text.contains("1.000 0.000 1.000 rg 0 0 m 6 3 l 0 6 l h f"),
        "fill-only referencing shapes should provide context-fill to marker children: {text}"
    );
    assert!(
        !text.contains("0.000 0.000 0.000 rg 0 0 m 6 3 l 0 6 l h f"),
        "context-stroke on an unstroked referencing shape must not invent a black marker fill: {text}"
    );
}

#[test]
fn pdf_svg_context_paint_without_marker_context_does_not_paint() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 12">
  <path d="M2 6 L30 6" fill="none" stroke="context-stroke" stroke-width="4"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("context-no-marker.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![No marker context](context-no-marker.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("2 6 m 30 6 l S"),
        "context-stroke has no context element outside marker/use rendering, so the path must not be painted: {text}"
    );
    assert!(
        !text.contains("0.000 0.000 0.000 RG"),
        "context-stroke must not silently fall back to black stroke outside a context element: {text}"
    );
}

#[test]
fn pdf_svg_path_marker_mid_and_shorthand_render_as_vector_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 64">
  <defs>
    <marker id="arrow" markerWidth="8" markerHeight="8" refX="8" refY="4" markerUnits="strokeWidth">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <path d="M8 8 L32 24 L64 8" fill="none" stroke="#0000ff" stroke-width="2" marker-mid="url(#arrow)" marker-end="url(#arrow)"/>
  <path d="M8 42 L32 54 L64 42" fill="none" stroke="#00aa00" stroke-width="2" marker="url(#arrow)" marker-start="none"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("path-markers.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Path markers](path-markers.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 0.000 1.000 RG 2 w 0 J 0 j 4 M 8 8 m 32 24 l 64 8 l S"),
        "the marked path itself should still render as ordinary vector strokes: {text}"
    );
    let marker_paints = text
        .matches("1.000 0.000 0.000 rg 0 0 m 8 4 l 0 8 l h f")
        .count();
    assert!(
        marker_paints >= 4,
        "path marker-mid plus marker shorthand fallback should paint vector marker shapes at interior/end vertices; saw {marker_paints}\n{text}"
    );
}

#[test]
fn pdf_svg_css_and_inline_marker_properties_render_vector_markers() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 64">
  <style>
    .edge { fill: none; stroke: #0000ff; stroke-width: 2; marker-end: url(#arrow); }
    .chain { fill: none; stroke: #00aa00; stroke-width: 2; marker: url(#arrow); }
  </style>
  <defs>
    <marker id="arrow" markerWidth="8" markerHeight="8" refX="8" refY="4" markerUnits="strokeWidth">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <line class="edge" x1="8" y1="10" x2="48" y2="10"/>
  <path class="chain" d="M8 42 L32 54 L64 42" style="marker-start: none; marker-mid: url(#arrow); marker-end: url(#arrow)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("css-marker-props.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![CSS marker properties](css-marker-props.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 0.000 1.000 RG 2 w 0 J 0 j 4 M 8 10 m 48 10 l S"),
        "the stylesheet-styled line should still render as a blue vector stroke: {text}"
    );
    assert!(
        text.contains("0.000 0.667 0.000 RG 2 w 0 J 0 j 4 M 8 42 m 32 54 l 64 42 l S"),
        "the inline-style marked path should still render as a green vector stroke: {text}"
    );
    let marker_paints = text
        .matches("1.000 0.000 0.000 rg 0 0 m 8 4 l 0 8 l h f")
        .count();
    assert!(
        marker_paints >= 3,
        "CSS marker-end plus inline marker-mid/marker-end should paint vector marker shapes; saw {marker_paints}\n{text}"
    );
}

#[test]
fn pdf_svg_closed_path_marker_end_uses_close_segment_tangent() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <defs>
    <marker id="arrow" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto" markerUnits="strokeWidth">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000"/>
    </marker>
  </defs>
  <path d="M8 8 L32 24 L64 8 Z" fill="none" stroke="#0000ff" stroke-width="2" marker-end="url(#arrow)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("closed-path-marker.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Closed marker](closed-path-marker.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("8 8 m 32 24 l 64 8 l h S"),
        "the closed path should still render as a closed vector path: {text}"
    );
    assert!(
        text.contains("q -1 0 0 -1 8 8 cm 2 0 0 2 0 0 cm 1 0 0 1 -8 -4 cm"),
        "marker-end for a closed subpath must use the close segment endpoint and tangent: {text}"
    );
}

#[test]
fn pdf_svg_quadratic_path_after_close_uses_closed_subpath_current_point() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40">
  <path d="M10 10 L20 10 Z Q10 20 20 20" fill="none" stroke="#000000"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("quad-after-close.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Quadratic after close](quad-after-close.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("10 10 m 20 10 l h 10 16.67 13.33 20 20 20 c"),
        "a quadratic segment after Z must be converted from the closed subpath start point: {text}"
    );
    assert!(
        !text.contains("20 10 l h 13.33 16.67 13.33 20 20 20 c"),
        "the post-close quadratic must not use the pre-close endpoint as its current point: {text}"
    );
}

#[test]
fn pdf_svg_text_clip_quadratic_after_close_uses_closed_subpath_current_point() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40">
  <defs>
    <clipPath id="clip">
      <path d="M10 10 L20 10 Z Q10 20 20 20"/>
    </clipPath>
  </defs>
  <text x="6" y="30" font-size="8" fill="#000000" clip-path="url(#clip)">Clip</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "text-clip-after-close.svg",
            svg.to_vec(),
        )],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Text clip after close](text-clip-after-close.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("79.5 712.5 m 87 712.5 l h 79.5 707.5 82 705 87 705 c W n"),
        "SVG text clip paths are emitted in page space and must convert post-Z quadratics from the closed subpath start: {text}"
    );
    assert!(
        !text.contains("87 712.5 l h 82 707.5 82 705 87 705 c W n"),
        "the mapped text clip path must not use the pre-close endpoint as its current point: {text}"
    );
}

#[test]
fn pdf_svg_root_title_and_desc_backfill_empty_markdown_alt() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 80" role="img">
  <title>Pipeline diagram</title>
  <desc>Parse &amp; PDF flow.</desc>
  <g>
    <title>Nested tooltip should not name the outer figure</title>
    <rect x="12" y="18" width="52" height="44" rx="6" fill="#ffffff" stroke="#e2e8f0" stroke-width="1.5"/>
  </g>
  <rect x="96" y="18" width="52" height="44" fill="#ffffff" stroke="#e2e8f0" stroke-width="1.5"/>
  <text x="38" y="45" text-anchor="middle" font-size="14" fill="#1a1a2e">Parse</text>
  <text x="122" y="45" text-anchor="middle" font-size="14" fill="#1a1a2e">PDF</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("diagrams/accessible.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf(
        "![](diagrams/accessible.svg)\n\n![Manual alt](diagrams/accessible.svg)",
        &opts,
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Alt (Pipeline diagram - Parse & PDF flow.)"),
        "empty Markdown alt text should fall back to root SVG title/desc metadata: {text}"
    );
    assert!(
        text.contains("/Alt (Manual alt)"),
        "author-supplied Markdown alt text should remain authoritative: {text}"
    );
    assert!(
        !text.contains("/Alt (Nested tooltip"),
        "nested element titles should not name the outer figure: {text}"
    );
}

#[test]
fn pdf_svg_accessible_text_replaces_invalid_numeric_entities_without_dropping_text() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" role="img">
  <title>Zero &#0;</title>
  <desc>Bad &#xD800; Huge &#99999999; Malformed &#x;</desc>
  <rect x="2" y="2" width="16" height="16" fill="#ffffff" stroke="#000000"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("invalid-entities.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![](invalid-entities.svg)", &opts).unwrap();
    let text = as_text(&pdf);
    let expected_alt =
        utf16be_pdf_hex_string("Zero \u{FFFD} - Bad \u{FFFD} Huge \u{FFFD} Malformed &#x;");

    assert!(
        text.contains(&format!("/Alt {expected_alt}")),
        "invalid SVG numeric XML references should become U+FFFD, while malformed references stay literal: {text}"
    );
    assert!(
        !text.contains("/Alt (Zero "),
        "invalid numeric XML references must not be dropped into an ASCII literal alt string: {text}"
    );
}

#[test]
fn pdf_svg_vector_effect_non_scaling_stroke_preserves_device_width() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40" viewBox="0 0 100 20">
  <style>
    .fixed { vector-effect: non-scaling-stroke; }
  </style>
  <line class="fixed" x1="5" y1="6" x2="95" y2="6" stroke="#2563eb" stroke-width="6" stroke-linecap="butt"/>
  <line x1="5" y1="14" x2="95" y2="14" stroke="#64748b" stroke-width="6" stroke-linecap="butt"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "diagrams/vector-effect.svg",
            svg.to_vec(),
        )],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Vector effect](diagrams/vector-effect.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("1.5 0 0 -1.5"),
        "fixture should exercise a uniform 1.5x SVG viewport scale: {text}"
    );
    assert!(
        text.contains("0.145 0.388 0.922 RG 4 w 0 J"),
        "non-scaling stroke should divide the authored 6-unit width by the 1.5x viewport scale: {text}"
    );
    assert!(
        text.contains("0.392 0.455 0.545 RG 6 w 0 J"),
        "ordinary strokes should keep the authored width and continue scaling with the viewport: {text}"
    );
}

#[test]
fn pdf_svg_root_viewport_preserves_viewbox_aspect_ratio() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 100">
  <rect x="0" y="0" width="100" height="100" fill="#22c55e"/>
  <text x="0" y="20" font-size="12" fill="#000000">Left</text>
</svg>
"##;
    let mut opts = small_page_opts(220.0, 160.0);
    opts.image_assets = vec![PdfImageAsset::new("wide.svg", svg.to_vec())];

    let pdf = render_pdf("![Wide](wide.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/BBox [ 20 65 170 140 ]"),
        "root SVG width=200 should produce a 150pt PDF viewport instead of the 75pt square viewBox size: {text}"
    );
    assert!(
        text.contains("q 0.75 0 0 -0.75 57.5 "),
        "default preserveAspectRatio=xMidYMid meet should use uniform scale and horizontally center the square viewBox in the wide viewport: {text}"
    );
    assert!(
        !text.contains("q 1.5 0 0 -0.75 "),
        "default preserveAspectRatio must not stretch the viewBox non-uniformly: {text}"
    );
    assert!(
        text.contains("BT /F1 9.00 Tf 1 0 0 1 57.50 "),
        "selectable SVG text should use the same centered viewport transform as vector shapes: {text}"
    );
}

#[test]
fn pdf_svg_preserve_aspect_ratio_none_allows_non_uniform_viewport_scale() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 100" preserveAspectRatio="none">
  <rect x="0" y="0" width="100" height="100" fill="#22c55e"/>
  <text x="0" y="20" font-size="12" fill="#000000">Left</text>
</svg>
"##;
    let mut opts = small_page_opts(220.0, 160.0);
    opts.image_assets = vec![PdfImageAsset::new("wide-none.svg", svg.to_vec())];

    let pdf = render_pdf("![Wide](wide-none.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1.5 0 0 -0.75 20 "),
        "preserveAspectRatio=none should map the square viewBox to the full wide viewport: {text}"
    );
    assert!(
        text.contains("BT /F1 13.50 Tf 1.33 0 0 0.67 20.00 "),
        "selectable SVG text should reflect the same non-uniform viewport scale as vector shapes: {text}"
    );
}

#[test]
fn pdf_svg_percentage_root_viewport_falls_back_to_viewbox_size() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 160 80">
  <rect x="0" y="0" width="160" height="80" fill="#22c55e"/>
</svg>
"##;
    let mut opts = small_page_opts(220.0, 160.0);
    opts.image_assets = vec![PdfImageAsset::new("responsive.svg", svg.to_vec())];

    let pdf = render_pdf("![Responsive](responsive.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/BBox [ 20 80 140 140 ]"),
        "unresolved percentage root dimensions should preserve the viewBox intrinsic aspect and size: {text}"
    );
    assert!(
        text.contains("q 0.75 0 0 -0.75 20 140 cm"),
        "percentage root dimensions should not be treated as a literal 100-by-100 viewport: {text}"
    );
}

#[test]
fn pdf_svg_root_background_color_paints_viewport_before_children() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 100">
  <style>
    :root { --root-bg: #f8fafc; }
    svg {
      background: var(--root-bg);
      background-image: radial-gradient(circle at 20% 20%, #ffffff 0%, transparent 45%);
    }
  </style>
  <rect x="25" y="25" width="50" height="50" fill="#ff0000"/>
</svg>
"##;
    let mut opts = small_page_opts(220.0, 160.0);
    opts.image_assets = vec![PdfImageAsset::new("root-bg.svg", svg.to_vec())];

    let pdf = render_pdf("![Root background](root-bg.svg)", &opts).unwrap();
    let text = as_text(&pdf);
    let background = "q 0.973 0.980 0.988 rg 20 65 150 75 re f\nQ\n";
    let child = "1.000 0.000 0.000 rg 25 25 50 50 re f";
    let background_pos = text
        .find(background)
        .expect("root background should paint the full 200x100 viewport in page coordinates");
    let child_pos = text
        .find(child)
        .expect("child SVG rect should still render after the synthetic root background");

    assert!(
        background_pos < child_pos,
        "root background must paint before SVG children so child geometry remains visible: {text}"
    );
    assert!(
        text.contains("q 0.75 0 0 -0.75 57.5 140 cm\n1.000 0.000 0.000 rg"),
        "child geometry should keep preserveAspectRatio centering while the root background covers the wide viewport: {text}"
    );
}

#[test]
fn pdf_svg_root_background_image_gradients_paint_viewport_before_children() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 100">
  <style>
    :root {
      --root-bg: #f8fafc;
      --accent: #2563eb;
      --node-stroke: #cbd5e1;
    }
    svg {
      background: var(--root-bg);
      background-image:
        radial-gradient(ellipse at 20% 0%, color-mix(in srgb, var(--accent) 10%, transparent) 0%, transparent 50%),
        linear-gradient(180deg, var(--root-bg) 0%, color-mix(in srgb, var(--root-bg) 96%, var(--node-stroke) 4%) 100%);
    }
  </style>
  <rect x="25" y="25" width="50" height="50" fill="#ff0000"/>
</svg>
"##;
    let mut opts = small_page_opts(220.0, 160.0);
    opts.image_assets = vec![PdfImageAsset::new("root-bg-gradient.svg", svg.to_vec())];

    let pdf = render_pdf("![Root background](root-bg-gradient.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Shading << /SG1 << /ShadingType 2"),
        "root linear-gradient background layer should register a native axial shading: {text}"
    );
    assert!(
        text.contains("/SG2 << /ShadingType 3"),
        "root radial-gradient background layer should register a native radial shading: {text}"
    );
    assert!(
        text.contains("/Coords [95 140 95 65]"),
        "linear-gradient(180deg, ...) should run from viewport top to bottom in page coordinates: {text}"
    );
    assert!(
        text.contains("/Coords [50 140 0 50 140"),
        "radial-gradient(... at 20% 0%) should anchor near the viewport top-left, not the centered viewBox content: {text}"
    );

    let color_pos = text
        .find("q 0.973 0.980 0.988 rg 20 65 150 75 re f\nQ\n")
        .expect("root background color should still paint the full viewport first");
    let linear_pos = text
        .find("q 20 65 150 75 re W n /SG1 sh\nQ\n")
        .expect("bottom background-image layer should clip to the full viewport");
    let radial_pos = text
        .find("q 20 65 150 75 re W n /SG2 sh\nQ\n")
        .expect("top background-image layer should clip to the full viewport");
    let child_pos = text
        .find("1.000 0.000 0.000 rg 25 25 50 50 re f")
        .expect("child SVG geometry should still render after root background layers");
    assert!(
        color_pos < linear_pos && linear_pos < radial_pos && radial_pos < child_pos,
        "root background color, linear layer, radial layer, then child content should preserve CSS background stacking order: {text}"
    );
}

#[test]
fn pdf_svg_root_background_shorthand_extracts_mixed_color_and_gradient() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 100">
  <style>
    svg {
      background-color: #00ff00;
      background: #f8fafc linear-gradient(90deg, #f8fafc 0%, #cbd5e1 100%);
    }
  </style>
  <rect x="25" y="25" width="50" height="50" fill="#ff0000"/>
</svg>
"##;
    let mut opts = small_page_opts(220.0, 160.0);
    opts.image_assets = vec![PdfImageAsset::new("root-bg-shorthand.svg", svg.to_vec())];

    let pdf = render_pdf("![Root background](root-bg-shorthand.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Shading << /SG1 << /ShadingType 2"),
        "background shorthand should extract a drawable gradient layer even when it shares a layer with a color token: {text}"
    );
    let color_pos = text
        .find("q 0.973 0.980 0.988 rg 20 65 150 75 re f\nQ\n")
        .expect(
            "background shorthand color should replace the earlier background-color declaration",
        );
    let gradient_pos = text
        .find("q 20 65 150 75 re W n /SG1 sh\nQ\n")
        .expect("background shorthand gradient should clip to the full viewport");
    let child_pos = text
        .find("1.000 0.000 0.000 rg 25 25 50 50 re f")
        .expect("child SVG geometry should still render after shorthand background");
    assert!(
        color_pos < gradient_pos && gradient_pos < child_pos,
        "background shorthand should paint the parsed color, then gradient, then child content: {text}"
    );
}

#[test]
fn pdf_svg_root_background_shorthand_without_color_resets_prior_color() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 100">
  <style>
    svg {
      background-color: #00ff00;
      background: linear-gradient(90deg, #f8fafc 0%, #cbd5e1 100%);
    }
  </style>
  <rect x="25" y="25" width="50" height="50" fill="#ff0000"/>
</svg>
"##;
    let mut opts = small_page_opts(220.0, 160.0);
    opts.image_assets = vec![PdfImageAsset::new(
        "root-bg-shorthand-reset.svg",
        svg.to_vec(),
    )];

    let pdf = render_pdf("![Root background](root-bg-shorthand-reset.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("q 0.000 1.000 0.000 rg 20 65 150 75 re f\nQ\n"),
        "background shorthand without a color should reset the earlier background-color instead of leaving a stale flat fill: {text}"
    );
    assert!(
        text.contains("q 20 65 150 75 re W n /SG1 sh\nQ\n"),
        "gradient-only background shorthand should still emit the drawable gradient layer: {text}"
    );
}

#[test]
fn pdf_svg_root_background_only_still_renders_as_vector_content() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40" viewBox="0 0 80 40" style="--bg: rgba(15, 23, 42, 0.5); background-color: var(--bg)">
</svg>
"##;
    let mut opts = small_page_opts(120.0, 100.0);
    opts.image_assets = vec![PdfImageAsset::new("empty-root-bg.svg", svg.to_vec())];

    let pdf = render_pdf("![Background only](empty-root-bg.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/GSa05000500 gs 0.059 0.090 0.165 rg"),
        "a root background should count as vector SVG content with its own alpha even without child elements: {text}"
    );
}

#[test]
fn pdf_svg_embedded_png_image_renders_as_nested_xobject() {
    let png_data = base64_encode(&tiny_rgb_png(&[[0x0B, 0x61, 0xA4], [0xF5, 0x9E, 0x0B]]));
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 18" role="img">
  <rect x="1" y="1" width="22" height="16" fill="#ffffff" stroke="#111827"/>
  <image x="4" y="5" width="10" height="8" opacity="0.5" preserveAspectRatio="none" href="data:image/png;base64,{png_data}"/>
</svg>
"##
    );
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("diagrams/raster.svg", svg.into_bytes())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Raster diagram](diagrams/raster.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Subtype /Image"),
        "embedded SVG PNG should allocate a raster image XObject"
    );
    assert!(
        text.contains("/XObject << /Im1 "),
        "nested image resource should be exposed on the page"
    );
    assert!(
        text.contains("10 0 0 -8 4 13 cm /Im1 Do"),
        "nested PNG should be drawn in SVG coordinates with upright orientation"
    );
    assert!(
        text.contains("/GSa05000500 gs"),
        "SVG image opacity should become a PDF ExtGState"
    );
    assert!(
        text.contains("/Alt (Raster diagram)"),
        "outer SVG figure alt text should remain intact"
    );
}

#[test]
fn pdf_svg_embedded_png_image_preserve_aspect_ratio_is_respected() {
    let png_data = base64_encode(&tiny_rgb_png(&[[0x0B, 0x61, 0xA4], [0xF5, 0x9E, 0x0B]]));
    let svg = format!(
        r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 42 16">
  <image x="4" y="2" width="10" height="8" href="data:image/png;base64,{png_data}"/>
  <image x="18" y="2" width="10" height="8" preserveAspectRatio="none" href="data:image/png;base64,{png_data}"/>
  <image x="32" y="2" width="6" height="8" preserveAspectRatio="xMidYMid slice" href="data:image/png;base64,{png_data}"/>
</svg>
"##
    );
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("aspect.svg", svg.into_bytes())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Aspect](aspect.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("10 0 0 -5 4 8.5 cm /Im1 Do"),
        "default SVG image preserveAspectRatio should meet and center the wide raster vertically: {text}"
    );
    assert!(
        text.contains("10 0 0 -8 18 10 cm /Im1 Do"),
        "preserveAspectRatio=none should retain the explicit stretch behavior: {text}"
    );
    assert!(
        text.contains("32 2 6 8 re W n 16 0 0 -8 27 10 cm /Im1 Do"),
        "slice mode should clip to the declared image viewport while painting the over-wide raster: {text}"
    );
}

#[test]
fn pdf_svg_text_does_not_leak_graphics_state() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 36">
  <text x="10" y="22" font-size="16" fill="#ff0000">Red SVG label</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("red.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf(
        "![Diagram](red.svg)\n\n```text\nplain code after the SVG\n```",
        &opts,
    )
    .unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q\n1.000 0.000 0.000 rg\nBT /F1"),
        "SVG labels should draw in an isolated graphics state: {text}"
    );
    assert!(
        text.contains("TJ ET\nQ\n"),
        "SVG label text must restore the previous PDF graphics state before later content: {text}"
    );
}

#[test]
fn pdf_svg_text_transform_uses_pdf_text_matrix() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
  <text x="20" y="30" font-size="12" fill="#000000" transform="rotate(90 20 30)">Rotated</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("rotated-text.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Rotated text](rotated-text.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("BT /F1 9.00 Tf 0 -1 1 0 "),
        "rotated SVG text should remain selectable text with a rotated PDF text matrix: {text}"
    );
    assert!(
        !text.contains("BT /F1 9.00 Tf 1 0 0 1 "),
        "rotated SVG text must not be flattened to an unrotated baseline: {text}"
    );
    assert!(
        text.contains("TJ ET\nQ\n"),
        "the transformed SVG text run should still close and restore its graphics state: {text}"
    );
}

#[test]
fn pdf_svg_text_with_fill_none_does_not_render_as_black() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 48">
  <text x="10" y="20" font-size="14" fill="none">Hidden</text>
  <text x="10" y="40" font-size="14" fill="#0000ff">Visible</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("text.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Diagram](text.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("0.000 0.000 0.000 rg\nBT /F1"),
        "fill=none SVG text should be skipped, not painted with the fallback black fill: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg\nBT /F1"),
        "visible sibling SVG text should still render: {text}"
    );
}

#[test]
fn pdf_svg_tspan_children_render_as_separate_text_runs() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 52">
  <text x="10" y="12" font-size="10" fill="none">
    <tspan x="10" y="12" fill="#0000ff">First line</tspan>
    <tspan x="10" dy="1.5em" style="fill: #ff0000; font-size: 8">Second line</tspan>
  </text>
  <text x="10" y="38" font-size="10">
    <tspan fill="#123456">Left</tspan><tspan fill="#00ff00">Right</tspan>
  </text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("tspan.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Tspan](tspan.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.matches("BT /F1").count() >= 2,
        "tspan children should become separate selectable PDF text runs, not one collapsed label: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg\nBT /F1"),
        "a child tspan should override parent fill=none with its own blue fill: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg\nBT /F1"),
        "a later tspan should render with its own red fill instead of inheriting the first tspan: {text}"
    );

    let left_tail = text
        .split("0.071 0.204 0.337 rg\nBT /F1")
        .nth(1)
        .expect("left inline tspan should render with its own color");
    let right_tail = text
        .split("0.000 1.000 0.000 rg\nBT /F1")
        .nth(1)
        .expect("right inline tspan should render with its own color");
    let matrix_prefix = " 1 0 0 1 ";
    let left_x: f32 = left_tail
        .split(matrix_prefix)
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|x| x.parse().ok())
        .expect("left inline tspan should emit a text matrix");
    let right_x: f32 = right_tail
        .split(matrix_prefix)
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|x| x.parse().ok())
        .expect("right inline tspan should emit a text matrix");
    assert!(
        right_x > left_x,
        "adjacent tspans without explicit x should advance instead of overlapping: {text}"
    );
}

#[test]
fn pdf_svg_text_font_weight_and_style_select_embedded_font_slots() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 72">
  <style>
    :root { --svg-heavy: 700; }
    .bold { font-weight: 700; }
    .italic { font-style: italic; }
    .both { font-weight: var(--svg-heavy, 700); font-style: oblique 10deg; }
    .medium { font-weight: 500; font-style: normal; }
  </style>
  <text x="4" y="12" class="bold" font-size="10" fill="#000000">Bold</text>
  <text x="4" y="28" class="italic" font-size="10" fill="#000000">Italic</text>
  <text x="4" y="44" class="both" font-size="10" fill="#000000">Both</text>
  <text x="4" y="60" class="medium" font-size="10" fill="#000000">Medium</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("styled-text.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Styled text](styled-text.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("BT /F2 "),
        "font-weight >= 600 should render SVG text with the bundled bold face: {text}"
    );
    assert!(
        text.contains("BT /F3 "),
        "font-style=italic should render SVG text with the bundled italic face: {text}"
    );
    assert!(
        text.contains("BT /F5 "),
        "combined bold + italic SVG text should render with the bold-italic face: {text}"
    );
    assert!(
        text.contains("BT /F1 "),
        "font-weight=500 should remain on the regular face when no medium face is bundled: {text}"
    );
}

#[test]
fn pdf_svg_text_font_family_monospace_selects_mono_slot() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 220 88">
  <style>
    :root { --code-family: ui-monospace, "SFMono-Regular", Menlo, monospace; }
    .code { font-family: var(--code-family); }
    .body { font-family: Inter, sans-serif; }
  </style>
  <text x="4" y="14" font-size="10" fill="#000000" font-family="ui-monospace, monospace">Attr mono</text>
  <text x="4" y="30" font-size="10" fill="#0000ff" class="code">CSS mono</text>
  <text x="4" y="46" font-size="10" fill="#00ff00" class="body">CSS body</text>
  <text x="4" y="62" font-size="10" fill="#ff0000" class="code">
    <tspan>Inherited mono</tspan>
    <tspan x="120" y="62" style="font-family: Inter, sans-serif">Body override</tspan>
  </text>
  <text x="4" y="78" font-size="10" fill="#111111" font-family="Inter, monospace">Primary body</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("font-family.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Font family](font-family.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert_eq!(
        text.matches("BT /F4 ").count(),
        3,
        "only direct, stylesheet-variable, and inherited monospace SVG text should render with the bundled mono face; a recognized primary body family must not fall through to a later monospace fallback: {text}"
    );
    assert!(
        text.matches("BT /F1 ").count() >= 3,
        "non-monospace SVG font-family and tspan overrides should stay on the regular body face: {text}"
    );
}

#[test]
fn pdf_svg_tspan_inherits_and_overrides_font_weight_and_style() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 40">
  <text x="4" y="16" font-size="10" font-weight="bold" font-style="italic" fill="#000000">
    <tspan>Both</tspan>
    <tspan x="4" y="32" style="font-style: normal !important">Bold only</tspan>
  </text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("styled-tspan.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Styled tspan](styled-tspan.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("BT /F5 "),
        "tspan children should inherit parent bold+italic SVG text styling: {text}"
    );
    assert!(
        text.contains("BT /F2 "),
        "a tspan should be able to override inherited italic style while retaining bold: {text}"
    );
}

#[test]
fn pdf_svg_dominant_baseline_adjusts_selectable_text_position() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 48">
  <style>
    :root { --label-baseline: middle; }
    .middle { dominant-baseline: var(--label-baseline, middle); }
  </style>
  <text x="12" y="20" font-size="10" fill="#ff0000">Auto</text>
  <text x="12" y="20" font-size="10" dominant-baseline="central" fill="#0000ff">Central</text>
  <text x="12" y="36" font-size="10" class="middle" fill="#00ff00">Middle</text>
  <text x="80" y="36" font-size="10" dominant-baseline="central" fill="#123456">
    <tspan style="dominant-baseline: auto !important">Tspan auto</tspan>
  </text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("baseline.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Baseline](baseline.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let (_, auto_y) = first_text_matrix_xy_after(&text, "1.000 0.000 0.000 rg\nBT /F1");
    let (_, central_y) = first_text_matrix_xy_after(&text, "0.000 0.000 1.000 rg\nBT /F1");
    let (_, middle_y) = first_text_matrix_xy_after(&text, "0.000 1.000 0.000 rg\nBT /F1");
    let (_, tspan_auto_y) = first_text_matrix_xy_after(&text, "0.071 0.204 0.337 rg\nBT /F1");

    assert!(
        auto_y > central_y,
        "dominant-baseline=central should move the emitted PDF baseline below the default SVG alphabetic baseline at the same y coordinate: {text}"
    );
    assert!(
        auto_y - central_y > 0.5,
        "central baseline shift should be visible after SVG image scaling, not rounded away: {text}"
    );
    assert!(
        middle_y < tspan_auto_y,
        "stylesheet dominant-baseline=middle should lower the PDF baseline, while an inline tspan override back to auto should not inherit that shift: {text}"
    );
}

#[test]
fn pdf_svg_letter_spacing_adjusts_selectable_text_and_anchor_width() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 180 54">
  <style>
    :root { --wide-spacing: 0.1em; }
    .wide { letter-spacing: var(--wide-spacing, 0.1em); }
  </style>
  <text x="80" y="16" font-size="10" text-anchor="middle" fill="#ff0000">HH</text>
  <text x="80" y="32" font-size="10" text-anchor="middle" class="wide" fill="#0000ff">HH</text>
  <text x="12" y="46" font-size="10" letter-spacing="-0.05em" fill="#00ff00">NN</text>
  <text x="96" y="46" font-size="10" class="wide" fill="#123456">
    <tspan style="letter-spacing: normal !important">OO</tspan>
  </text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("letter-spacing.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Letter spacing](letter-spacing.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let (plain_x, _) = first_text_matrix_xy_after(&text, "1.000 0.000 0.000 rg\nBT /F1");
    let (wide_x, _) = first_text_matrix_xy_after(&text, "0.000 0.000 1.000 rg\nBT /F1");
    assert!(
        wide_x < plain_x,
        "text-anchor=middle should account for the extra letter-spacing width: {text}"
    );

    let wide_object = first_text_object_after(&text, "0.000 0.000 1.000 rg\nBT /F1");
    assert!(
        wide_object.contains(">-100<"),
        "positive 0.1em SVG letter-spacing should become a negative PDF TJ adjustment: {text}"
    );

    let tight_object = first_text_object_after(&text, "0.000 1.000 0.000 rg\nBT /F1");
    assert!(
        tight_object.contains(">50<"),
        "negative SVG letter-spacing should become a positive PDF TJ adjustment: {text}"
    );

    let reset_object = first_text_object_after(&text, "0.071 0.204 0.337 rg\nBT /F1");
    assert!(
        !reset_object.contains(">-100<") && !reset_object.contains(">50<"),
        "a tspan should be able to reset inherited letter-spacing back to normal: {text}"
    );
}

#[test]
fn pdf_svg_text_stylesheet_font_size_and_anchor_affect_selectable_text() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 180 88">
  <style>
    :root { --label-size: 20px; --middle-anchor: middle; --half-size: 0.5em; }
    .css-label { font-size: var(--label-size); text-anchor: var(--middle-anchor); }
    .child-small { font-size: var(--half-size); text-anchor: end; }
  </style>
  <text x="80" y="16" font-size="10" fill="#ff0000">Attr</text>
  <text x="80" y="36" font-size="10" text-anchor="start" class="css-label" fill="#0000ff">CSS</text>
  <text x="80" y="56" font-size="20" fill="#00ff00"><tspan class="child-small">Half</tspan></text>
  <text x="80" y="76" class="css-label" style="font-size: 12px; text-anchor: end" fill="#123456">Inline</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("css-text.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![CSS text](css-text.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let attr_marker = "1.000 0.000 0.000 rg\nBT /F1";
    let css_marker = "0.000 0.000 1.000 rg\nBT /F1";
    let child_marker = "0.000 1.000 0.000 rg\nBT /F1";
    let inline_marker = "0.071 0.204 0.337 rg\nBT /F1";
    let attr_size = first_text_font_size_after(&text, attr_marker);
    let css_size = first_text_font_size_after(&text, css_marker);
    let child_size = first_text_font_size_after(&text, child_marker);
    let inline_size = first_text_font_size_after(&text, inline_marker);
    let (attr_x, _) = first_text_matrix_xy_after(&text, attr_marker);
    let (css_x, _) = first_text_matrix_xy_after(&text, css_marker);

    assert!(
        css_size > attr_size * 1.5,
        "stylesheet font-size should override the smaller presentation attribute: {text}"
    );
    assert!(
        (child_size - attr_size).abs() < 0.1,
        "tspan CSS font-size in em units should resolve relative to the parent text size: {text}"
    );
    assert!(
        inline_size < css_size,
        "inline font-size should retain final priority over matched stylesheet rules: {text}"
    );
    assert!(
        css_x < attr_x,
        "stylesheet text-anchor=middle should shift the emitted text matrix left of start-anchored text: {text}"
    );
}

#[test]
fn pdf_svg_text_length_adjusts_selectable_text_width() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 180 72">
  <text x="80" y="14" font-size="10" text-anchor="middle" fill="#ff0000">Wide</text>
  <text x="80" y="30" font-size="10" text-anchor="middle" fill="#0000ff" textLength="80">Wide</text>
  <text x="10" y="46" font-size="10" fill="#00ff00" textLength="34" lengthAdjust="spacingAndGlyphs">Scale</text>
  <text x="10" y="62" font-size="10" fill="#123456">
    <tspan textLength="40">AA</tspan><tspan fill="#ff00ff">BB</tspan>
  </text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("text-length.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Text length](text-length.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let (plain_x, _) = first_text_matrix_xy_after(&text, "1.000 0.000 0.000 rg\nBT /F1");
    let (length_x, _) = first_text_matrix_xy_after(&text, "0.000 0.000 1.000 rg\nBT /F1");
    assert!(
        length_x < plain_x - 10.0,
        "text-anchor=middle should use textLength rather than natural text width: {text}"
    );

    let spacing_object = first_text_object_after(&text, "0.000 0.000 1.000 rg\nBT /F1");
    assert!(
        spacing_object.contains(">-"),
        "default lengthAdjust=spacing should stretch text by increasing PDF TJ spacing: {text}"
    );

    let scaled_object = first_text_object_after(&text, "0.000 1.000 0.000 rg\nBT /F1");
    assert!(
        !scaled_object.contains(" Tf 1 0 0 1 "),
        "lengthAdjust=spacingAndGlyphs should scale the selectable PDF text x axis: {text}"
    );

    let (left_x, _) = first_text_matrix_xy_after(&text, "0.071 0.204 0.337 rg\nBT /F1");
    let (right_x, _) = first_text_matrix_xy_after(&text, "1.000 0.000 1.000 rg\nBT /F1");
    assert!(
        right_x - left_x > 20.0,
        "adjacent tspans should advance by textLength, not the natural glyph width: {text}"
    );
}

#[test]
fn pdf_svg_parent_text_length_adjusts_contiguous_tspan_runs() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 180 40">
  <text x="10" y="14" font-size="10" fill="#123456"><tspan>AA</tspan><tspan fill="#ff00ff">BB</tspan></text>
  <text x="10" y="30" font-size="10" fill="#0000ff" textLength="90"><tspan>AA</tspan><tspan fill="#00ff00">BB</tspan></text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("parent-text-length.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Parent text length](parent-text-length.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let (plain_second_x, _) = first_text_matrix_xy_after(&text, "1.000 0.000 1.000 rg\nBT /F1");
    let (length_second_x, _) = first_text_matrix_xy_after(&text, "0.000 1.000 0.000 rg\nBT /F1");
    assert!(
        length_second_x > plain_second_x + 20.0,
        "parent textLength should re-space a contiguous tspan run instead of being ignored: {text}"
    );

    let first_stretched_object = first_text_object_after(&text, "0.000 0.000 1.000 rg\nBT /F1");
    assert!(
        first_stretched_object.contains(">-"),
        "parent textLength should be allocated to child tspans as selectable PDF spacing: {text}"
    );
}

#[test]
fn pdf_svg_text_decoration_draws_vector_lines_without_flattening_text() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 220 84">
  <style>
    :root { --decor: underline; }
    .under { text-decoration: var(--decor); }
    .combo { text-decoration-line: overline line-through; }
  </style>
  <text x="10" y="16" font-size="10" class="under" fill="#0000ff">Under</text>
  <text x="10" y="36" font-size="10" class="combo" fill="#ff0000">Combo</text>
  <text x="10" y="56" font-size="10" text-decoration="underline" fill="#00ff00">A<tspan text-decoration="none" fill="#123456">B</tspan><tspan style="text-decoration: line-through" fill="#ff00ff">C</tspan></text>
  <text x="10" y="76" font-size="10" text-decoration="wavy" text-decoration-line="underline" fill="#00ffff">LineAttr</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("decor.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Decor](decor.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let under = first_graphics_state_after(&text, "0.000 0.000 1.000 rg\nBT /F1");
    let combo = first_graphics_state_after(&text, "1.000 0.000 0.000 rg\nBT /F1");
    let inherited = first_graphics_state_after(&text, "0.000 1.000 0.000 rg\nBT /F1");
    let reset = first_graphics_state_after(&text, "0.071 0.204 0.337 rg\nBT /F1");
    let tspan = first_graphics_state_after(&text, "1.000 0.000 1.000 rg\nBT /F1");
    let line_attr = first_graphics_state_after(&text, "0.000 1.000 1.000 rg\nBT /F1");

    assert!(
        under.contains("0.000 0.000 1.000 RG") && under.matches(" l S").count() == 1,
        "CSS var text-decoration underline should paint one blue vector line while text remains selectable: {text}"
    );
    assert!(
        combo.contains("1.000 0.000 0.000 RG") && combo.matches(" l S").count() == 2,
        "text-decoration-line should paint overline and line-through for the same text run: {text}"
    );
    assert!(
        inherited.contains("0.000 1.000 0.000 RG") && inherited.matches(" l S").count() == 1,
        "presentation-attribute underline should apply to the parent text node: {text}"
    );
    assert!(
        reset.matches(" l S").count() == 0 && !reset.contains("0.071 0.204 0.337 RG"),
        "text-decoration=none on a tspan should reset the inherited parent underline: {text}"
    );
    assert!(
        tspan.contains("1.000 0.000 1.000 RG") && tspan.matches(" l S").count() == 1,
        "inline tspan style should be able to re-enable line-through independently: {text}"
    );
    assert!(
        line_attr.contains("0.000 1.000 1.000 RG") && line_attr.matches(" l S").count() == 1,
        "text-decoration-line should still apply when an unsupported text-decoration shorthand is present: {text}"
    );
    assert!(
        text.contains("<0041>") && text.contains("<0042>") && text.contains("<0043>"),
        "decorated SVG labels must remain real selectable PDF text: {text}"
    );
}

#[test]
fn pdf_svg_numeric_attributes_accept_e_prefixed_unit_suffixes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 40">
  <text x="10" y="25" font-size="20em" fill="#000000">Sized</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("unit.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Unit suffix](unit.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("BT /F1 15.00 Tf"),
        "SVG numeric attributes with em/ex-style suffixes should use the leading number instead of falling back to the default font size: {text}"
    );
}

#[test]
fn pdf_svg_treats_fully_transparent_rgba_paint_as_none() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <rect x="0" y="0" width="40" height="20" fill="rgba(255,0,0,0)" stroke="none"/>
  <rect x="4" y="4" width="32" height="12" fill="#0000ff" stroke="none"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("transparent.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Transparent](transparent.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("1.000 0.000 0.000 rg"),
        "fully transparent rgba fills should not paint opaque red content: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg"),
        "the visible sibling shape should still render"
    );
}

#[test]
fn pdf_svg_partial_opacity_uses_native_pdf_extgstate_resources() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 92 36">
  <style>
    .fade { opacity: 50%; stroke-opacity: 25%; }
    .rgba-fade { fill: rgba(255,0,255,0.6); }
  </style>
  <rect class="fade" x="2" y="2" width="12" height="10" fill="#ff0000" stroke="#0000ff" stroke-width="2"/>
  <circle cx="28" cy="7" r="5" fill="#00ff00" fill-opacity="0.4"/>
  <rect x="40" y="2" width="12" height="10" fill="rgba(0,0,255,0.25)"/>
  <rect class="rgba-fade" x="54" y="2" width="10" height="10"/>
  <text x="2" y="28" font-size="10" fill="#000000" opacity="0.3">AlphaText</text>
  <rect x="72" y="2" width="12" height="10" fill="#123456" opacity="0"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("opacity.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Opacity](opacity.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/ExtGState <<"),
        "pages with partially transparent SVG content should advertise native PDF ExtGState resources: {text}"
    );
    assert!(
        text.contains("/GSa05000125 << /ca 0.500 /CA 0.125 >>"),
        "class opacity and stroke-opacity should combine into distinct fill/stroke alpha: {text}"
    );
    assert!(
        text.contains("/GSa04001000 << /ca 0.400 /CA 1.000 >>"),
        "fill-opacity should create a non-stroking alpha state without weakening absent strokes: {text}"
    );
    assert!(
        text.contains("/GSa02501000 << /ca 0.250 /CA 1.000 >>"),
        "rgba fill alpha should use the same native PDF alpha path: {text}"
    );
    assert!(
        text.contains("/GSa03000300 << /ca 0.300 /CA 0.300 >>"),
        "selectable SVG text opacity should also be backed by an ExtGState resource: {text}"
    );
    assert!(
        text.contains("/GSa06001000 << /ca 0.600 /CA 1.000 >>"),
        "CSS class rgba fill alpha should be preserved in the parsed style patch: {text}"
    );
    assert!(
        text.contains("q /GSa05000125 gs 1.000 0.000 0.000 rg 0.000 0.000 1.000 RG 2 w"),
        "the faded stroked rect should scope its alpha before painting: {text}"
    );
    assert!(
        text.contains("/GSa03000300 gs\n0.000 0.000 0.000 rg\nBT /F1"),
        "transparent SVG labels should remain real selectable PDF text: {text}"
    );
    assert!(
        !text.contains("0.071 0.204 0.337 rg 72 2 12 10 re f"),
        "opacity=0 shapes should still be skipped rather than emitted with an alpha state: {text}"
    );
}

#[test]
fn pdf_svg_hidden_or_fully_transparent_elements_do_not_paint() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 52">
  <g opacity="0">
    <rect x="0" y="0" width="80" height="18" fill="#ff0000" stroke="none"/>
    <text x="4" y="14" font-size="12" fill="#ff0000">OpacityHidden</text>
  </g>
  <g style="display: none">
    <rect x="0" y="18" width="80" height="16" fill="#00ff00" stroke="none"/>
    <text x="4" y="30" font-size="12" fill="#00ff00">DisplayHidden</text>
  </g>
  <g visibility="hidden">
    <rect x="0" y="34" width="80" height="16" fill="#ffff00" stroke="none"/>
  </g>
  <rect x="4" y="4" width="24" height="12" fill="#0000ff" stroke="none"/>
  <text x="4" y="45" font-size="12" fill="#0000ff">Visible</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("hidden.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Diagram](hidden.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("1.000 0.000 0.000 rg"),
        "opacity=0 SVG content must not paint opaque red: {text}"
    );
    assert!(
        !text.contains("0.000 1.000 0.000 rg"),
        "display:none SVG content must not paint opaque green: {text}"
    );
    assert!(
        !text.contains("1.000 1.000 0.000 rg"),
        "visibility:hidden SVG content must not paint opaque yellow: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg"),
        "visible sibling content should still render: {text}"
    );
    assert!(
        text.contains("BT /F1"),
        "visible sibling text should still remain selectable"
    );
}

#[test]
fn pdf_svg_inherits_group_paint_for_child_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <g fill="#00ff00" stroke="#0000ff" stroke-width="2">
    <rect x="4" y="4" width="32" height="24"/>
    <line x1="44" y1="8" x2="72" y2="28"/>
  </g>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("group.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Group](group.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 1.000 0.000 rg"),
        "child rect should inherit fill from its group: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 RG 2 w"),
        "child shapes should inherit stroke color and width from their group: {text}"
    );
    assert!(
        text.contains("44 8 m 72 28 l S"),
        "the inherited-stroke child line should render instead of disappearing: {text}"
    );
}

#[test]
fn pdf_svg_transform_attributes_apply_to_vector_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 50">
  <rect x="5" y="6" width="20" height="10" fill="#ff0000" transform="translate(10 20) scale(2)"/>
  <g transform="translate(3 4)">
    <line x1="1" y1="1" x2="6" y2="1" stroke="#0000ff" transform="scale(2)"/>
  </g>
  <rect x="1" y="2" width="3" height="4" style="transform: translate(7 8); fill: #00ff00"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("transform.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Transformed](transform.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 2 0 0 2 10 20 cm 1.000 0.000 0.000 rg 5 6 20 10 re f\nQ"),
        "direct SVG transform lists should wrap shape drawing in a PDF matrix: {text}"
    );
    assert!(
        text.contains("q 2 0 0 2 3 4 cm 0.000 0.000 1.000 RG 1 w 0 J 0 j 4 M 1 1 m 6 1 l S\nQ"),
        "group and child SVG transforms should compose in source order: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 7 8 cm 0.000 1.000 0.000 rg 1 2 3 4 re f\nQ"),
        "CSS-style SVG transform declarations should also be honored: {text}"
    );
}

#[test]
fn pdf_svg_malformed_non_ascii_hex_colors_do_not_panic() {
    let svg = r##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 24">
  <rect x="2" y="2" width="12" height="10" fill="#éx"/>
  <path d="M2 18 L44 18" fill="none" stroke="#aébcd" stroke-width="2"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "bad-non-ascii-color.svg",
            svg.as_bytes().to_vec(),
        )],
        ..PdfOptions::default()
    };

    let rendered =
        std::panic::catch_unwind(|| render_pdf("![Bad color](bad-non-ascii-color.svg)", &opts));
    assert!(
        rendered.is_ok(),
        "malformed non-ASCII SVG hex colors must be ignored, not panic"
    );
    let pdf = rendered.unwrap().unwrap();
    assert!(pdf.starts_with(b"%PDF-1.7\n") && pdf.ends_with(b"%%EOF\n"));
}

#[test]
fn pdf_svg_stroke_defaults_match_svg_initial_values() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <path d="M2 10 L12 2 L22 10" fill="none" stroke="#123456" stroke-width="2"/>
  <path d="M2 16 L22 16" fill="none" stroke="#abcdef" stroke-width="3" stroke-linecap="round" stroke-linejoin="bevel"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("stroke-defaults.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Stroke defaults](stroke-defaults.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.071 0.204 0.337 RG 2 w 0 J 0 j 4 M 2 10 m 12 2 l 22 10 l S"),
        "omitted stroke-linecap/join/miterlimit should use SVG initial butt/miter/4 values: {text}"
    );
    assert!(
        text.contains("0.671 0.804 0.937 RG 3 w 1 J 2 j 2 16 m 22 16 l S"),
        "explicit stroke cap/join declarations should still override SVG defaults: {text}"
    );
}

#[test]
fn pdf_svg_css_class_stroke_styles_apply_to_vector_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 32">
  <style>
    @import url('ignored.css');
    .dash { stroke-dasharray: 6 4; stroke-linecap: square; stroke-linejoin: bevel; }
    .miter { stroke-linejoin: miter; stroke-miterlimit: 2.5; }
    .solid { stroke-dasharray: none; }
    @media print {
      .dash { stroke-dasharray: 1 1; stroke-linecap: butt; stroke-linejoin: miter; }
    }
    @supports (stroke-dasharray: 9 9) {
      .solid { stroke-dasharray: 9 9; }
    }
  </style>
  <path class="dash" d="M4 4 L60 4" fill="none" stroke="#0000ff" stroke-width="2"/>
  <path class="dash solid" d="M4 12 L60 12" fill="none" stroke="#ff0000" stroke-width="1"/>
  <path d="M4 20 L60 20" fill="none" stroke="#00ff00" stroke-width="1.5" stroke-linecap="butt" stroke-linejoin="miter" stroke-miterlimit="3" stroke-dasharray="2,3" stroke-dashoffset="1"/>
  <path class="miter" d="M4 28 L60 28" fill="none" stroke="#123456" stroke-width="2"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("dash.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Dashed](dash.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 0.000 0.000 1.000 RG 2 w 2 J 2 j [6 4] 0 d 4 4 m 60 4 l S\nQ"),
        "simple CSS class stroke rules should apply dash, cap, and join: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 RG 1 w 2 J 2 j 4 12 m 60 12 l S"),
        "a later class should reset stroke-dasharray without losing cap/join declarations: {text}"
    );
    assert!(
        text.contains("q 0.000 1.000 0.000 RG 1.5 w 0 J 0 j 3 M [2 3] 1 d 4 20 m 60 20 l S\nQ"),
        "presentation attributes should support dash offset and explicit cap/join/miter-limit values: {text}"
    );
    assert!(
        text.contains("0.071 0.204 0.337 RG 2 w 0 J 0 j 2.5 M 4 28 m 60 28 l S"),
        "CSS stroke-miterlimit should emit a PDF miter-limit operator with explicit miter joins: {text}"
    );
}

#[test]
fn pdf_svg_css_cascade_overrides_presentation_attrs_without_multiplying_local_opacity() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 20">
  <style>
    .present { fill: #0000ff; opacity: 0.5; }
    rect.present.later { opacity: 0.8; }
  </style>
  <g opacity="0.5">
    <rect class="present later" x="2" y="2" width="12" height="10" fill="#ff0000"/>
  </g>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("cascade.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Cascade](cascade.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 0.000 1.000 rg"),
        "author CSS fill should override the lower-priority presentation fill attribute: {text}"
    );
    assert!(
        !text.contains("1.000 0.000 0.000 rg"),
        "the presentation red fill must not win over stylesheet fill: {text}"
    );
    assert!(
        text.contains("/GSa04001000 << /ca 0.400 /CA 1.000 >>"),
        "ancestor opacity 0.5 should multiply only the winning local opacity 0.8: {text}"
    );
    assert!(
        !text.contains("/GSa02001000"),
        "same-element opacity declarations must cascade, not multiply 0.5 * 0.5 * 0.8: {text}"
    );
}

#[test]
fn pdf_svg_css_rect_rx_ry_rounds_rectangle_geometry() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 72 20">
  <style>
    :root { --corner: 4px; }
    .css-rx { rx: 5px; }
    .css-ry { ry: var(--corner); }
    .inline { rx: 0; }
    .invalid { rx: var(--missing); }
  </style>
  <rect class="css-rx" x="2" y="2" width="12" height="10" rx="0" fill="#000001"/>
  <rect class="css-ry" x="20" y="2" width="12" height="10" fill="#000002"/>
  <rect class="inline" x="38" y="2" width="12" height="10" style="rx: 4px; fill: #000003"/>
  <rect class="invalid" x="56" y="2" width="12" height="10" rx="3" fill="#000004"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("css-rect-radii.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![CSS rect radii](css-rect-radii.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    for sharp_rect in [
        "2 2 12 10 re f",
        "20 2 12 10 re f",
        "38 2 12 10 re f",
        "56 2 12 10 re f",
    ] {
        assert!(
            !text.contains(sharp_rect),
            "CSS/attribute rect radii should emit rounded paths, not sharp re rectangles: {text}"
        );
    }
    let cubic_segments = text.matches(" c").count();
    assert!(
        cubic_segments >= 16,
        "four rounded rectangles should emit at least sixteen cubic corner segments; found {cubic_segments}: {text}"
    );
}

#[test]
fn pdf_svg_rect_rx_ry_emit_asymmetric_elliptical_corners() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 28 34">
  <defs>
    <clipPath id="asymmetric-clip">
      <rect x="2" y="18" width="20" height="12" rx="6" ry="2"/>
    </clipPath>
  </defs>
  <rect x="2" y="2" width="20" height="12" rx="6" ry="2" fill="#000001"/>
  <rect x="2" y="18" width="20" height="12" fill="#000002" clip-path="url(#asymmetric-clip)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("asymmetric-radii.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Asymmetric radii](asymmetric-radii.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("8 2 m 16 2 l 19.31 2 22 2.9 22 4 c 22 12 l"),
        "rx=6 and ry=2 should keep distinct horizontal and vertical corner radii: {text}"
    );
    assert!(
        !text.contains("8 2 m 16 2 l 19.31 2 22 4.69 22 8 c 22 8 l"),
        "asymmetric SVG rect radii must not collapse to the old circular rx-only path: {text}"
    );
    assert!(
        text.contains("8 18 m 16 18 l 19.31 18 22 18.9 22 20 c 22 28 l") && text.contains("W n"),
        "clipPath rect radii should use the same asymmetric corner geometry as painted rects: {text}"
    );
}

#[test]
fn pdf_svg_css_compound_selectors_apply_with_specificity() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 32">
  <style>
    * { stroke-linecap: square; stroke-linejoin: bevel; }
    path.edge { stroke: #0000ff; stroke-width: 2; }
    #priority { stroke: #ff0000; }
    .edge { stroke: #00ff00; }
    .edge.dashed { stroke-dasharray: 3 2; }
    rect#box.warning { fill: #123456; }
    line { stroke: #00ff00; }
    g.scope path.child { stroke: #000000; stroke-width: 2; }
    .scope > path.direct { stroke: #ff00ff; stroke-width: 1.5; }
    .scope g path.nested { stroke: #00ffff; stroke-width: 3; }
    .scope > path.too-deep { stroke: #ff8800; stroke-width: 4; }
  </style>
  <path id="priority" class="edge dashed" d="M2 4 L30 4" fill="none"/>
  <rect id="box" class="warning" x="2" y="10" width="8" height="8"/>
  <line x1="2" y1="24" x2="20" y2="24"/>
  <g class="scope">
    <path class="child" d="M32 8 L58 8" fill="none"/>
    <path class="direct" d="M32 14 L58 14" fill="none"/>
    <g>
      <path class="nested" d="M32 20 L58 20" fill="none"/>
      <path class="too-deep" d="M32 28 L58 28" fill="none"/>
    </g>
  </g>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("css-selectors.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Selectors](css-selectors.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1.000 0.000 0.000 RG 2 w 2 J 2 j [3 2] 0 d 2 4 m 30 4 l S\nQ"),
        "ID specificity should beat a later class rule, while compound class and universal selectors still contribute stroke options: {text}"
    );
    assert!(
        !text.contains("0.000 0.000 0.000 RG 2 w 2 J 2 j [3 2] 0 d 2 4 m 30 4 l S"),
        "descendant selectors must not leak onto unrelated path elements outside the matching group: {text}"
    );
    assert!(
        text.contains("0.071 0.204 0.337 rg 2 10 8 8 re f\n"),
        "compound rect#id.class selectors should style matching shapes: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 RG 1 w 2 J 2 j 2 24 m 20 24 l S\n"),
        "type selectors and universal stroke options should apply to line elements: {text}"
    );
    assert!(
        text.contains("0.000 0.000 0.000 RG 2 w 2 J 2 j 32 8 m 58 8 l S\n"),
        "descendant selectors should style matching paths under matching groups: {text}"
    );
    assert!(
        text.contains("1.000 0.000 1.000 RG 1.5 w 2 J 2 j 32 14 m 58 14 l S\n"),
        "direct-child selectors should style immediate child paths: {text}"
    );
    assert!(
        text.contains("0.000 1.000 1.000 RG 3 w 2 J 2 j 32 20 m 58 20 l S\n"),
        "multi-hop descendant selectors should match nested paths: {text}"
    );
    assert!(
        !text.contains("1.000 0.533 0.000 RG 4 w"),
        "direct-child selectors must not match paths nested below an intermediate group: {text}"
    );
}

#[test]
fn pdf_svg_marker_child_css_selectors_style_marker_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 20">
  <style>
    path.edge { stroke: #0000ff; stroke-width: 2; fill: none; }
    marker#css-arrow path { fill: #00ff00; stroke: none; }
  </style>
  <defs>
    <marker id="css-arrow" markerWidth="6" markerHeight="6" refX="6" refY="3" orient="auto" markerUnits="strokeWidth">
      <path d="M0 0 L6 3 L0 6 Z"/>
    </marker>
  </defs>
  <path class="edge" d="M4 10 L32 10" marker-end="url(#css-arrow)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("marker-css.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Marker](marker-css.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 0.000 1.000 RG 2 w 0 J 0 j 4 M 4 10 m 32 10 l S\n"),
        "the edge path should still receive its simple path.edge rule: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg"),
        "marker#id path descendant selectors should style marker child fills: {text}"
    );
}

#[test]
fn pdf_svg_current_color_resolves_through_cascade_and_inheritance() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 32">
  <style>
    .theme { color: #123456; }
    .icon { fill: currentColor; stroke: currentColor; stroke-width: 2; }
    .later-color { color: #00ff00; }
  </style>
  <g class="theme">
    <path class="icon" d="M2 4 L18 4 L18 14 Z"/>
    <path class="icon later-color" d="M24 4 L40 4 L40 14 Z"/>
    <path d="M46 4 L62 4 L62 14 Z" style="fill: currentColor; color: #0000ff"/>
    <g color="#ff0000">
      <path d="M68 4 L84 4 L84 14 Z" fill="currentColor"/>
      <text x="68" y="24" font-size="8" fill="currentColor">Hi</text>
    </g>
  </g>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("current-color.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Current color](current-color.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.071 0.204 0.337 rg 0.071 0.204 0.337 RG 2 w"),
        "fill/stroke currentColor should resolve from inherited class color: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg 0.000 1.000 0.000 RG 2 w"),
        "a later matching color rule should update currentColor-derived fill and stroke: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg 46 4 m 62 4 l 62 14 l h f"),
        "inline color declarations after fill:currentColor should still update the used fill color: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg 68 4 m 84 4 l 84 14 l h f"),
        "presentation color attributes should feed fill=\"currentColor\": {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg\nBT /F1"),
        "selectable SVG text should also use currentColor-derived fill: {text}"
    );
}

#[test]
fn pdf_svg_stylesheets_apply_document_wide_from_defs_and_late_positions() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 24">
  <rect class="late" x="2" y="2" width="10" height="8"/>
  <defs>
    <style>
      .from-defs { fill: #123456; }
    </style>
  </defs>
  <rect class="from-defs" x="18" y="2" width="10" height="8"/>
  <style>
    .late { fill: #00ff00; }
  </style>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("document-wide-css.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![CSS](document-wide-css.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 1.000 0.000 rg 2 2 10 8 re f\n"),
        "stylesheet rules should apply document-wide even when declared after the target element: {text}"
    );
    assert!(
        text.contains("0.071 0.204 0.337 rg 18 2 10 8 re f\n"),
        "stylesheet rules inside <defs> should apply to normal rendered elements: {text}"
    );
}

#[test]
fn pdf_svg_evenodd_fill_rule_uses_pdf_even_odd_paint_operators() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 32">
  <style>
    .hole { fill-rule: evenodd; }
  </style>
  <path class="hole" d="M2 2 H30 V30 H2 Z M10 10 H22 V22 H10 Z" fill="#0000ff"/>
  <path d="M34 2 H62 V30 H34 Z M42 10 H54 V22 H42 Z" fill="#ff0000" stroke="#000000" fill-rule="evenodd"/>
  <path d="M64 2 H78 V30 H64 Z" fill="#00ff00" style="fill-rule: nonzero"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("fill-rule.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Fill rule](fill-rule.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 0.000 1.000 rg 2 2 m 30 2 l 30 30 l 2 30 l h 10 10 m 22 10 l 22 22 l 10 22 l h f*\n"),
        "CSS fill-rule: evenodd should emit PDF f* so subpaths cut holes: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 rg 0.000 0.000 0.000 RG 1 w 0 J 0 j 4 M 34 2 m 62 2 l 62 30 l 34 30 l h 42 10 m 54 10 l 54 22 l 42 22 l h B*\n"),
        "presentation fill-rule=evenodd should emit PDF B* for fill-and-stroke shapes: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg 64 2 m 78 2 l 78 30 l 64 30 l h f\n"),
        "explicit nonzero fill rule should keep the default PDF fill operator: {text}"
    );
}

#[test]
fn pdf_svg_paint_order_reorders_fill_and_stroke_layers() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 90 44">
  <style>
    :root { --edge-first: stroke fill; }
    .css { paint-order: stroke fill markers; }
    .var { paint-order: var(--edge-first); }
  </style>
  <path class="css" d="M2 2 H28 V18 H2 Z" fill="#0000ff" stroke="#ff0000" stroke-width="3"/>
  <path d="M32 2 H58 V18 H32 Z" fill="#00ff00" stroke="#000000" stroke-width="2" paint-order="stroke"/>
  <path class="var" d="M62 2 H88 V18 H62 Z" fill="#ff00ff" stroke="#123456" stroke-width="1"/>
  <path d="M2 24 H28 V40 H2 Z" fill="#00ffff" stroke="#000000" stroke-width="2" paint-order="normal"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("paint-order.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Paint order](paint-order.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("1.000 0.000 0.000 RG 3 w 0 J 0 j 4 M 2 2 m 28 2 l 28 18 l 2 18 l h S\n0.000 0.000 1.000 rg 2 2 m 28 2 l 28 18 l 2 18 l h f\n"),
        "CSS paint-order: stroke fill should stroke before filling the same vector path: {text}"
    );
    assert!(
        text.contains("0.000 0.000 0.000 RG 2 w 0 J 0 j 4 M 32 2 m 58 2 l 58 18 l 32 18 l h S\n0.000 1.000 0.000 rg 32 2 m 58 2 l 58 18 l 32 18 l h f\n"),
        "presentation paint-order=stroke should append omitted fill/markers after the explicit stroke layer: {text}"
    );
    assert!(
        text.contains("0.071 0.204 0.337 RG 1 w 0 J 0 j 4 M 62 2 m 88 2 l 88 18 l 62 18 l h S\n1.000 0.000 1.000 rg 62 2 m 88 2 l 88 18 l 62 18 l h f\n"),
        "paint-order should resolve CSS variables before applying the ordered paint layers: {text}"
    );
    assert!(
        text.contains("0.000 1.000 1.000 rg 0.000 0.000 0.000 RG 2 w 0 J 0 j 4 M 2 24 m 28 24 l 28 40 l 2 40 l h B\n"),
        "paint-order=normal should keep the compact combined fill+stroke operator: {text}"
    );
}

#[test]
fn pdf_svg_paint_order_reorders_marker_layers() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 24">
  <defs>
    <marker id="green-tip" markerWidth="6" markerHeight="6" refX="6" refY="3" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L6 3 L0 6 Z" fill="#00ff00"/>
    </marker>
    <marker id="magenta-tip" markerWidth="6" markerHeight="6" refX="6" refY="3" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L6 3 L0 6 Z" fill="#ff00ff"/>
    </marker>
  </defs>
  <line x1="4" y1="6" x2="36" y2="6" stroke="#ff0000" stroke-width="2" marker-end="url(#green-tip)" paint-order="markers stroke"/>
  <line x1="4" y1="18" x2="36" y2="18" stroke="#0000ff" stroke-width="2" marker-end="url(#magenta-tip)" paint-order="stroke markers"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("paint-marker-order.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Paint marker order](paint-marker-order.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    let green_marker = text
        .find("0.000 1.000 0.000 rg")
        .unwrap_or_else(|| panic!("green marker fill not found: {text}"));
    let red_stroke = text
        .find("1.000 0.000 0.000 RG 2 w")
        .unwrap_or_else(|| panic!("red line stroke not found: {text}"));
    assert!(
        green_marker < red_stroke,
        "paint-order=markers stroke should paint the marker before the line stroke: {text}"
    );

    let blue_stroke = text
        .find("0.000 0.000 1.000 RG 2 w")
        .unwrap_or_else(|| panic!("blue line stroke not found: {text}"));
    let magenta_marker = text
        .find("1.000 0.000 1.000 rg")
        .unwrap_or_else(|| panic!("magenta marker fill not found: {text}"));
    assert!(
        blue_stroke < magenta_marker,
        "paint-order=stroke markers should paint the marker after the line stroke: {text}"
    );
}

#[test]
fn pdf_svg_paint_order_applies_inside_marker_child_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 16">
  <defs>
    <marker id="ordered" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L8 4 L0 8 Z" fill="#ff0000" stroke="#0000ff" stroke-width="1.5" paint-order="stroke fill"/>
    </marker>
    <marker id="normal" markerWidth="8" markerHeight="8" refX="8" refY="4" orient="auto" markerUnits="userSpaceOnUse">
      <path d="M0 0 L8 4 L0 8 Z" fill="#00ff00" stroke="#000000" stroke-width="1.5"/>
    </marker>
  </defs>
  <line x1="4" y1="5" x2="24" y2="5" stroke="#64748b" stroke-width="1" marker-end="url(#ordered)"/>
  <line x1="4" y1="12" x2="24" y2="12" stroke="#64748b" stroke-width="1" marker-end="url(#normal)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "marker-child-paint-order.svg",
            svg.to_vec(),
        )],
        ..PdfOptions::default()
    };
    let pdf = render_pdf(
        "![Marker child paint order](marker-child-paint-order.svg)",
        &opts,
    )
    .unwrap();
    let text = as_text(&pdf);

    let marker_stroke = text
        .find("0.000 0.000 1.000 RG 1.5 w 0 J 0 j 4 M 0 0 m 8 4 l 0 8 l h S\n")
        .unwrap_or_else(|| panic!("ordered marker child stroke layer not found: {text}"));
    let marker_fill = text
        .find("1.000 0.000 0.000 rg 0 0 m 8 4 l 0 8 l h f\n")
        .unwrap_or_else(|| panic!("ordered marker child fill layer not found: {text}"));
    assert!(
        marker_stroke < marker_fill,
        "marker child paint-order=stroke fill should stroke before filling the marker path: {text}"
    );
    assert!(
        text.contains(
            "0.000 1.000 0.000 rg 0.000 0.000 0.000 RG 1.5 w 0 J 0 j 4 M 0 0 m 8 4 l 0 8 l h B\n"
        ),
        "marker children without custom paint-order should keep the compact fill+stroke operator: {text}"
    );
}

#[test]
fn pdf_svg_gradient_paints_use_native_linear_shading_and_fallback_colors() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 32">
  <defs>
    <linearGradient id="warm">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <radialGradient id="cool">
      <stop offset="0" style="stop-color: #00ff00"/>
      <stop offset="1" style="stop-color: #0000ff; stop-opacity: 0.5"/>
    </radialGradient>
    <linearGradient id="line-warm" gradientUnits="userSpaceOnUse" x1="2" y1="24" x2="28" y2="24">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <style>
    .gradient-stroke { stroke: url(#cool); }
  </style>
  <rect x="2" y="2" width="10" height="10" fill="url(#warm)"/>
  <path class="gradient-stroke" d="M2 16 L28 16" fill="none" stroke-width="2"/>
  <line x1="2" y1="24" x2="28" y2="24" stroke="url(#line-warm)" stroke-width="2" stroke-linecap="butt"/>
  <rect x="34" y="2" width="10" height="10" fill="url(#missing) #00ff00"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("gradient.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Gradient](gradient.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "/Shading << /SG1 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [2 2 12 2]"
        ),
        "simple linearGradient fills should be exposed as native PDF axial shadings: {text}"
    );
    assert!(
        text.contains(
            "/Function << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> /Extend [true true] >>"
        ),
        "native PDF shading should preserve the linearGradient endpoint colors: {text}"
    );
    assert!(
        text.contains("q 2 2 10 10 re W n /SG1 sh\nQ\n"),
        "linearGradient fills should clip the shape and paint the registered shading: {text}"
    );
    assert!(
        text.contains("0.250 0.750 0.500 RG 2 w 0 J 0 j 4 M 2 16 m 28 16 l S\n"),
        "unsupported gradient strokes should retain deterministic representative vector colors: {text}"
    );
    assert!(
        text.contains("/ShadingType 2 /ColorSpace /DeviceRGB /Coords [2 24 28 24]")
            && text.contains("q 2 25 m 28 25 l 28 23 l 2 23 l h W n /SG"),
        "straight line gradient strokes should clip the stroked outline and paint a native PDF shading: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg 34 2 10 10 re f\n"),
        "url(#missing) fallback colors should keep shapes visible instead of dropping paint: {text}"
    );
}

#[test]
fn pdf_svg_multistop_linear_gradients_use_pdf_stitching_functions() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <defs>
    <linearGradient id="traffic">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="50%" stop-color="#00ff00"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <rect x="2" y="2" width="30" height="12" fill="url(#traffic)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("multistop.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Multi](multistop.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "/Shading << /SG1 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [2 2 32 2]"
        ),
        "multi-stop linearGradient fills should still register a native axial shading: {text}"
    );
    assert!(
        text.contains("/Function << /FunctionType 3 /Domain [0 1] /Functions [ << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 1.000 0.000] /N 1 >> << /FunctionType 2 /Domain [0 1] /C0 [0.000 1.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> ] /Bounds [ 0.5 ] /Encode [ 0 1 0 1 ] >>"),
        "multi-stop linearGradient colors should be preserved with a PDF stitching function: {text}"
    );
    assert!(
        text.contains("q 2 2 30 12 re W n /SG1 sh\nQ\n"),
        "multi-stop linearGradient fills should clip the shape and paint the stitched shading: {text}"
    );
}

#[test]
fn pdf_svg_linear_gradients_inherit_stops_and_attrs_via_href() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 48 24">
  <defs>
    <linearGradient id="base" gradientUnits="userSpaceOnUse" x1="0" y1="1" x2="10" y2="1">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="50%" stop-color="#00ff00"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <linearGradient id="derived" xlink:href="#base" x1="4" y1="5" x2="24" y2="5"/>
  </defs>
  <rect x="2" y="2" width="30" height="12" fill="url(#derived)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("gradient-href.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Gradient href](gradient-href.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "/Shading << /SG1 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [4 5 24 5]"
        ),
        "derived gradients should inherit gradientUnits from the referenced gradient while allowing local coordinate overrides: {text}"
    );
    assert!(
        text.contains("/Function << /FunctionType 3 /Domain [0 1] /Functions [ << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 1.000 0.000] /N 1 >> << /FunctionType 2 /Domain [0 1] /C0 [0.000 1.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> ] /Bounds [ 0.5 ] /Encode [ 0 1 0 1 ] >>"),
        "derived gradients with no local stops should inherit the referenced stop list: {text}"
    );
    assert!(
        text.contains("q 2 2 30 12 re W n /SG1 sh\nQ\n"),
        "the href-derived gradient should still paint through the native clipped shading path: {text}"
    );
}

#[test]
fn pdf_svg_radial_gradients_use_native_pdf_shading_when_circular() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40">
  <defs>
    <radialGradient id="glow" cx="50%" cy="50%" r="50%" fx="50%" fy="50%" fr="10%">
      <stop offset="0%" stop-color="#ffffff"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
    <radialGradient id="wide" cx="50%" cy="50%" r="50%">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
  </defs>
  <circle cx="20" cy="20" r="16" fill="url(#glow)"/>
  <rect x="2" y="2" width="30" height="12" fill="url(#wide)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("radial.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Radial](radial.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "/Shading << /SG1 << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [20 20 3.2 20 20 16]"
        ),
        "circular radialGradient fills should register native PDF radial shadings: {text}"
    );
    assert!(
        text.contains("/Function << /FunctionType 2 /Domain [0 1] /C0 [1.000 1.000 1.000] /C1 [0.000 0.000 1.000] /N 1 >> /Extend [true true] >>"),
        "radialGradient stop colors should feed the PDF shading function: {text}"
    );
    assert!(
        text.contains("q 4 20 m 4 11.16 11.16 4 20 4 c 28.84 4 36 11.16 36 20 c 36 28.84 28.84 36 20 36 c 11.16 36 4 28.84 4 20 c h W n /SG1 sh\nQ\n"),
        "native radial shadings should paint through the same clipped shape path as axial shadings: {text}"
    );
    assert!(
        text.contains("0.500 0.000 0.500 rg 2 2 30 12 re f\n"),
        "non-circular objectBoundingBox radial gradients should keep the deterministic fallback color instead of emitting an incorrect circular shading: {text}"
    );
    assert!(
        !text.contains("/SG2 << /ShadingType 3"),
        "non-circular radial gradients must not register a native radial shading with visibly wrong geometry: {text}"
    );
}

#[test]
fn pdf_svg_radial_gradient_repeat_and_reflect_emit_bounded_native_rings() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 20">
  <defs>
    <radialGradient id="repeat" spreadMethod="repeat">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
    <radialGradient id="reflect" spreadMethod="reflect">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </radialGradient>
  </defs>
  <rect x="2" y="2" width="16" height="16" fill="url(#repeat)"/>
  <rect x="24" y="2" width="16" height="16" fill="url(#reflect)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("radial-spread.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Radial spread](radial-spread.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/SG1 << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [10 10 0 10 10 8] /Function << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> /Extend [false false] >>"),
        "repeat spreadMethod should emit a native radial shading for the first finite ring: {text}"
    );
    assert!(
        text.contains("/SG2 << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [10 10 8 10 10 16] /Function << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> /Extend [false false] >>"),
        "repeat spreadMethod should emit the second ring instead of padding the first: {text}"
    );
    assert!(
        text.contains("q 2 2 16 16 re W n /SG1 sh\n/SG2 sh\nQ\n"),
        "repeat radial rings should paint under one clipped shape path: {text}"
    );
    assert!(
        text.contains("/SG3 << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [32 10 0 32 10 8] /Function << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> /Extend [false false] >>"),
        "reflect spreadMethod should paint the first radial ring in forward stop order: {text}"
    );
    assert!(
        text.contains("/SG4 << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [32 10 8 32 10 16] /Function << /FunctionType 2 /Domain [0 1] /C0 [0.000 0.000 1.000] /C1 [1.000 0.000 0.000] /N 1 >> /Extend [false false] >>"),
        "reflect spreadMethod should reverse odd radial rings instead of repeating a hard reset: {text}"
    );
    assert!(
        text.contains("q 24 2 16 16 re W n /SG3 sh\n/SG4 sh\nQ\n"),
        "reflect radial rings should paint under one clipped shape path: {text}"
    );
}

#[test]
fn pdf_svg_gradient_stops_honor_stylesheet_selectors() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <style>
    .start { stop-color: #00ff00; }
    stop.end { stop-color: #0000ff; stop-opacity: 50%; }
    #override { stop-color: #ff0000; }
  </style>
  <defs>
    <linearGradient id="styled">
      <stop class="start" offset="0%"/>
      <stop class="end" offset="50%"/>
      <stop id="override" class="end" offset="100%"/>
    </linearGradient>
  </defs>
  <rect x="2" y="2" width="30" height="12" fill="url(#styled)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("styled-stops.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Styled stops](styled-stops.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Function << /FunctionType 3 /Domain [0 1] /Functions [ << /FunctionType 2 /Domain [0 1] /C0 [0.000 1.000 0.000] /C1 [0.500 0.500 1.000] /N 1 >> << /FunctionType 2 /Domain [0 1] /C0 [0.500 0.500 1.000] /C1 [1.000 0.500 0.500] /N 1 >> ] /Bounds [ 0.5 ] /Encode [ 0 1 0 1 ] >>"),
        "stylesheet selectors on <stop> elements should drive the native PDF gradient colors, including specificity and opacity blending: {text}"
    );
}

#[test]
fn pdf_svg_linear_gradient_transform_and_spread_method_are_respected() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 32">
  <defs>
    <linearGradient id="shifted" gradientUnits="userSpaceOnUse" x1="2" y1="3" x2="12" y2="3" gradientTransform="translate(3 4)">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <linearGradient id="bbox-rotated" x1="0%" y1="0%" x2="100%" y2="0%" gradientTransform="rotate(90)">
      <stop offset="0%" stop-color="#00ff00"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <linearGradient id="repeating" x2="50%" spreadMethod="repeat">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
    <linearGradient id="reflecting" x2="50%" spreadMethod="reflect">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <rect x="2" y="2" width="10" height="10" fill="url(#shifted)"/>
  <rect x="20" y="2" width="10" height="20" fill="url(#bbox-rotated)"/>
  <rect x="40" y="2" width="10" height="10" fill="url(#repeating)"/>
  <rect x="54" y="2" width="10" height="10" fill="url(#reflecting)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("gradient-transform.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Gradient transforms](gradient-transform.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/SG1 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [5 7 15 7]"),
        "userSpaceOnUse gradientTransform should transform native shading endpoints: {text}"
    );
    assert!(
        text.contains("/SG2 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 2 20 22]"),
        "objectBoundingBox gradientTransform should transform normalized gradient coordinates before bbox mapping: {text}"
    );
    assert!(
        text.contains("q 2 2 10 10 re W n /SG1 sh\nQ\n"),
        "the shifted gradient should paint through a native clipped shading: {text}"
    );
    assert!(
        text.contains("q 20 2 10 20 re W n /SG2 sh\nQ\n"),
        "the rotated bbox gradient should paint through a native clipped shading: {text}"
    );
    assert!(
        text.contains("/SG3 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [40 2 45 2]"),
        "repeat spreadMethod should emit a native axial shading for the first covered period: {text}"
    );
    assert!(
        text.contains("/SG4 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [45 2 50 2]"),
        "repeat spreadMethod should emit a native axial shading for the second covered period: {text}"
    );
    assert!(
        text.contains("/SG3 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [40 2 45 2] /Function << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> /Extend [false false] >>"),
        "repeat spreadMethod shadings should not extend beyond their period band: {text}"
    );
    assert!(
        text.contains("q 40 2 10 10 re W n /SG3 sh\n/SG4 sh\nQ\n"),
        "repeat spreadMethod should paint all finite period bands under one shape clip: {text}"
    );
    assert!(
        text.contains("/SG5 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [54 2 59 2] /Function << /FunctionType 2 /Domain [0 1] /C0 [1.000 0.000 0.000] /C1 [0.000 0.000 1.000] /N 1 >> /Extend [false false] >>"),
        "reflect spreadMethod should paint even periods in the forward stop order: {text}"
    );
    assert!(
        text.contains("/SG6 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [59 2 64 2] /Function << /FunctionType 2 /Domain [0 1] /C0 [0.000 0.000 1.000] /C1 [1.000 0.000 0.000] /N 1 >> /Extend [false false] >>"),
        "reflect spreadMethod should reverse odd-period stop order instead of repeating a hard reset: {text}"
    );
    assert!(
        text.contains("q 54 2 10 10 re W n /SG5 sh\n/SG6 sh\nQ\n"),
        "reflect spreadMethod should paint all finite period bands under one shape clip: {text}"
    );
}

#[test]
fn pdf_svg_user_space_patterns_tile_vector_children_under_shape_clip() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 16">
  <style>
    .pattern-fill { fill: url(#stripe); }
  </style>
  <defs>
    <pattern id="stripe" patternUnits="userSpaceOnUse" x="2" y="2" width="4" height="8">
      <rect x="0" y="0" width="2" height="8" fill="#ff0000"/>
      <path d="M0 0 L4 8" fill="none" stroke="#0000ff" stroke-width="0.5"/>
    </pattern>
  </defs>
  <rect class="pattern-fill" x="2" y="2" width="8" height="8" stroke="#00ff00" stroke-width="1"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("pattern.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Pattern](pattern.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 2 2 8 8 re W n q 1 0 0 1 2 2 cm"),
        "pattern fills should clip to the painted shape before tiling vector children: {text}"
    );
    assert!(
        text.contains(
            "1.000 0.000 0.000 rg 0 0 2 8 re f\n0.000 0.000 1.000 RG 0.5 w 0 J 0 j 4 M 0 0 m 4 8 l S\nQ\nq 1 0 0 1 6 2 cm"
        ),
        "userSpaceOnUse pattern tiles should repeat at the declared x/y/width/height cadence: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 RG 0.5 w 0 J 0 j 4 M 0 0 m 4 8 l S"),
        "pattern children should remain vector paths with their own stroke styles: {text}"
    );
    assert!(
        text.contains("Q\n0.000 1.000 0.000 RG 1 w 0 J 0 j 4 M 2 2 8 8 re S\n"),
        "outer strokes should still paint after the pattern fill: {text}"
    );
    assert!(
        !text.contains("0.000 0.000 0.000 rg 2 2 8 8 re f"),
        "known pattern paint servers should not fall back to a solid default fill: {text}"
    );
}

#[test]
fn pdf_svg_patterns_inherit_attrs_and_body_via_href() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 28 14">
  <defs>
    <pattern id="base" patternUnits="userSpaceOnUse" x="1" y="2" width="5" height="6">
      <rect x="0" y="0" width="5" height="6" fill="#112233"/>
    </pattern>
    <pattern id="derived" xlink:href="#base" x="3">
      <!-- Alias-only pattern still inherits the referenced body. -->
    </pattern>
    <pattern id="override" href="#base" x="16" y="2" width="4" height="4">
      <path d="M0 4 L4 0" fill="none" stroke="#336699" stroke-width="0.75"/>
    </pattern>
  </defs>
  <rect x="3" y="2" width="7" height="6" fill="url(#derived)"/>
  <rect x="16" y="2" width="4" height="4" fill="url(#override)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("pattern-href.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Pattern href](pattern-href.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 3 2 7 6 re W n q 1 0 0 1 3 2 cm"),
        "href-derived pattern should inherit base geometry and override x: {text}"
    );
    assert!(
        text.contains("0.067 0.133 0.200 rg 0 0 5 6 re f"),
        "href-derived pattern should inherit base vector children: {text}"
    );
    assert!(
        text.contains("q 16 2 4 4 re W n q 1 0 0 1 16 2 cm"),
        "patterns with their own body should keep inherited geometry overrides: {text}"
    );
    assert!(
        text.contains("0.200 0.400 0.600 RG 0.75 w 0 J 0 j 4 M 0 4 m 4 0 l S"),
        "patterns with their own body should replace inherited body content: {text}"
    );
}

#[test]
fn pdf_svg_css_variables_resolve_paint_values() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 36">
  <style>
    :root {
      --node-fill: #123456;
      --edge-color: var(--node-fill);
      --transparent-fill: rgba(255, 0, 0, 0);
    }
    .edge { stroke: var(--edge-color); }
  </style>
  <defs>
    <linearGradient id="var-gradient">
      <stop offset="0%" stop-color="var(--node-fill)"/>
      <stop offset="100%" stop-color="#ffffff"/>
    </linearGradient>
  </defs>
  <path class="edge" d="M2 4 L40 4" fill="none" stroke-width="2"/>
  <rect x="2" y="10" width="10" height="10" fill="var(--missing-fill, #00ff00)"/>
  <rect x="18" y="10" width="10" height="10" fill="var(--transparent-fill)"/>
  <rect x="34" y="10" width="10" height="10" fill="url(#var-gradient)"/>
  <rect x="50" y="10" width="10" height="10" fill="var(--missing-rgba, rgba(0, 0, 255, 1))"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("css-vars.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Vars](css-vars.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.071 0.204 0.337 RG 2 w 0 J 0 j 4 M 2 4 m 40 4 l S\n"),
        "CSS class paint should resolve var() chains before PDF stroke emission: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg 2 10 10 10 re f\n"),
        "var() fallback colors should keep SVG shapes visible: {text}"
    );
    assert!(
        !text.contains("1.000 0.000 0.000 rg 18 10 10 10 re f"),
        "transparent custom-property fills should not become opaque red paint: {text}"
    );
    assert!(
        text.contains(
            "/Shading << /SG1 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [34 10 44 10]"
        ),
        "custom-property linearGradient fills should be emitted as native PDF shadings: {text}"
    );
    assert!(
        text.contains(
            "/C0 [0.071 0.204 0.337] /C1 [1.000 1.000 1.000] /N 1 >> /Extend [true true] >>"
        ),
        "gradient stops should resolve custom-property colors before native shading emission: {text}"
    );
    assert!(
        text.contains("q 34 10 10 10 re W n /SG1 sh\nQ\n"),
        "custom-property linearGradient fills should clip the shape and paint the shading: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg 50 10 10 10 re f\n"),
        "var() fallback parsing should handle nested rgba() parentheses: {text}"
    );
}

#[test]
fn pdf_svg_root_css_variables_cascade_in_source_order() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12">
  <style>
    :root { --accent: #ff0000; }
    :root { --accent: #0000ff; }
  </style>
  <rect x="2" y="2" width="10" height="8" fill="var(--accent)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("root-var-cascade.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Root var cascade](root-var-cascade.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.000 0.000 1.000 rg 2 2 10 8 re f\n"),
        "later :root custom-property declarations should override earlier ones: {text}"
    );
    assert!(
        !text.contains("1.000 0.000 0.000 rg 2 2 10 8 re f\n"),
        "stale first-defined :root custom-property values must not survive the cascade: {text}"
    );
}

#[test]
fn pdf_svg_color_mix_resolves_srgb_paints_and_keeps_fallbacks() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 88 42">
  <style>
    :root {
      --mix-a: #ff0000;
      --mix-b: #0000ff;
    }
    .edge {
      --edge-mix: color-mix(in srgb, var(--mix-a) 25%, var(--mix-b));
      stroke: var(--edge-mix);
      stroke-width: 2;
      fill: none;
    }
  </style>
  <rect x="2" y="2" width="10" height="10" fill="color-mix(in srgb, var(--mix-a), var(--mix-b))"/>
  <rect x="18" y="2" width="10" height="10" fill="color-mix(in srgb, #ff0000 25%, #0000ff)"/>
  <rect x="34" y="2" width="10" height="10" fill="color-mix(in srgb, #ff0000 10%, #0000ff 30%)"/>
  <rect x="50" y="2" width="10" height="10" style="fill: #123456; fill: color-mix(in srgb, #ff0000 50%, transparent)"/>
  <path class="edge" d="M2 26 L40 26"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("color-mix.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Color mix](color-mix.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("0.500 0.000 0.500 rg 2 2 10 10 re f\n"),
        "color-mix should default omitted srgb percentages to an even mix: {text}"
    );
    assert!(
        text.contains("0.250 0.000 0.750 rg 18 2 10 10 re f\n"),
        "one omitted color-mix percentage should use the remaining weight: {text}"
    );
    assert!(
        text.contains("0.250 0.000 0.750 rg 34 2 10 10 re f\n"),
        "two explicit color-mix percentages should normalize by their positive sum: {text}"
    );
    assert!(
        text.contains("0.071 0.204 0.337 rg 50 2 10 10 re f\n"),
        "transparent color-mix components should leave earlier fallback declarations active: {text}"
    );
    assert!(
        text.contains("0.250 0.000 0.750 RG 2 w 0 J 0 j 4 M 2 26 m 40 26 l S\n"),
        "selector-scoped custom properties should feed color-mix stroke paints: {text}"
    );
}

#[test]
fn pdf_svg_css_variables_are_scoped_by_matching_selectors() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 84 20">
  <style>
    :root { --accent: #111111; }
    .scope-a { --accent: #ff0000; }
    .scope-b { --accent: #0000ff; }
    .unmatched { --accent: #00ff00; }
    .scope-a rect, .scope-b rect, .direct { fill: var(--accent); }
    .scope-a .child { stroke: var(--accent); stroke-width: 2; fill: none; }
  </style>
  <g class="scope-a">
    <rect x="2" y="2" width="8" height="8"/>
    <path class="child" d="M2 14 L14 14"/>
  </g>
  <g class="scope-b">
    <rect x="18" y="2" width="8" height="8"/>
  </g>
  <rect class="direct" x="34" y="2" width="8" height="8"/>
  <rect x="50" y="2" width="8" height="8" style="--accent: #00ff00; fill: var(--accent)"/>
  <rect x="66" y="2" width="8" height="8" style="--accent: #00ffff" fill="var(--accent)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("scoped-vars.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Scoped Vars](scoped-vars.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("1.000 0.000 0.000 rg 2 2 8 8 re f\n"),
        "ancestor-scoped custom properties should feed descendant fills: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 RG 2 w 0 J 0 j 4 M 2 14 m 14 14 l S\n"),
        "ancestor-scoped custom properties should feed descendant strokes: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg 18 2 8 8 re f\n"),
        "sibling scopes should keep independent custom property values: {text}"
    );
    assert!(
        text.contains("0.067 0.067 0.067 rg 34 2 8 8 re f\n"),
        "unmatched class custom properties must not leak over the root fallback: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg 50 2 8 8 re f\n"),
        "inline custom properties should resolve inline style paints on the same element: {text}"
    );
    assert!(
        text.contains("0.000 1.000 1.000 rg 66 2 8 8 re f\n"),
        "inline custom properties should resolve presentation paint attributes on the same element: {text}"
    );
}

#[test]
fn pdf_svg_marker_definitions_render_referenced_marker_shapes() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <defs>
    <marker id="open" markerWidth="8" markerHeight="7" refX="7" refY="3.5" markerUnits="userSpaceOnUse" orient="auto">
      <path d="M0 0.5 L7 3.5 L0 6.5" fill="none" stroke="#ff0000" stroke-width="1.2"/>
    </marker>
    <marker id="diamond" markerWidth="8" markerHeight="8" refX="8" refY="4" markerUnits="userSpaceOnUse" orient="auto">
      <path d="M4 0 L8 4 L4 8 L0 4 Z" fill="#00ff00"/>
    </marker>
  </defs>
  <path d="M2 4 L22 4" fill="none" stroke="#0000ff" stroke-width="2" marker-end="url(#open)"/>
  <path d="M2 14 L22 14" fill="none" stroke="#0000ff" stroke-width="2" marker-end="url(#diamond)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("markers.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Markers](markers.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 22 4 cm 1 0 0 1 0 0 cm 1 0 0 1 -7 -3.5 cm"),
        "marker-end should place the referenced marker at the path endpoint with the marker ref point aligned: {text}"
    );
    assert!(
        text.contains("1.000 0.000 0.000 RG 1.2 w 0 J 0 j 4 M 0 0.5 m 7 3.5 l 0 6.5 l S\nQ"),
        "open marker definitions should render their declared stroked, unfilled path instead of a generic filled triangle: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 rg 4 0 m 8 4 l 4 8 l 0 4 l h f\nQ"),
        "filled marker definitions should render their declared marker body, such as Mermaid diamond markers: {text}"
    );
}

#[test]
fn pdf_svg_use_elements_expand_referenced_shapes_and_groups() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 72 24">
  <defs>
    <path id="chevron" d="M0 0 L8 4 L0 8" fill="none" stroke-width="1.5"/>
    <g id="badge" fill="#00ff00">
      <g transform="translate(2 0)">
        <rect x="0" y="0" width="8" height="8"/>
      </g>
      <path d="M1 7 L7 1" fill="none" stroke="#0000ff"/>
    </g>
  </defs>
  <use href="#chevron" x="10" y="4" stroke="#ff0000"/>
  <use xlink:href="#badge" x="28" y="4"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("use.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Use refs](use.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "q 1 0 0 1 10 4 cm 1.000 0.000 0.000 RG 1.5 w 0 J 0 j 4 M 0 0 m 8 4 l 0 8 l S\nQ"
        ),
        "href-based <use> should expand a referenced path with the use element's inherited stroke and x/y placement: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 30 4 cm 0.000 1.000 0.000 rg 0 0 8 8 re f\nQ"),
        "xlink:href <use> should expand nested grouped definitions and preserve inherited group fill: {text}"
    );
    assert!(
        text.contains("q 1 0 0 1 28 4 cm 0.000 0.000 1.000 RG 1 w 0 J 0 j 4 M 1 7 m 7 1 l S\nQ"),
        "group children with explicit stroke should render alongside inherited-fill siblings: {text}"
    );
}

#[test]
fn pdf_svg_use_alias_definitions_expand_recursively() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 20">
  <defs>
    <rect id="tile" x="0" y="0" width="4" height="4" fill="#00ff00"/>
    <use id="tile-alias" href="#tile" x="2" y="3"/>
  </defs>
  <use href="#tile-alias" x="10" y="5"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("use-alias.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Use alias](use-alias.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 12 8 cm 0.000 1.000 0.000 rg 0 0 4 4 re f\nQ"),
        "a reusable <use id=...> definition should recursively expand its target and compose alias/document x-y translations: {text}"
    );
    assert!(
        !text.contains("10 5 cm 0.000 1.000 0.000 rg 0 0 4 4 re f"),
        "the alias <use> x/y translation must not be dropped: {text}"
    );
}

#[test]
fn pdf_svg_clip_paths_apply_native_pdf_clipping() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 90 36">
  <defs>
    <clipPath id="scene">
      <rect x="1" y="2" width="30" height="18"/>
    </clipPath>
    <clipPath id="hole" clip-rule="evenodd">
      <path d="M50 0 H80 V30 H50 Z M58 8 H72 V22 H58 Z"/>
    </clipPath>
  </defs>
  <style>
    .hole-clip { clip-path: url(#hole); }
  </style>
  <g transform="translate(12 8)" clip-path="url(#scene)">
    <path d="M0 0 H40 V20 H0 Z" fill="#ddeeff"/>
  </g>
  <rect class="hole-clip" x="50" y="0" width="30" height="30" fill="#0000ff"/>
  <text class="hole-clip" x="60" y="16" font-size="10" fill="#0000ff">ClipText</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("clip.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Clipped](clip.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 12 8 cm 1 2 m 31 2 l 31 20 l 1 20 l h W n 0.867 0.933 1.000 rg 0 0 m 40 0 l 40 20 l 0 20 l h f\nQ"),
        "group clip-path should emit a scoped PDF clipping path before drawing transformed child shapes: {text}"
    );
    assert!(
        text.contains("q 50 0 m 80 0 l 80 30 l 50 30 l h 58 8 m 72 8 l 72 22 l 58 22 l h W* n 0.000 0.000 1.000 rg 50 0 30 30 re f\nQ"),
        "CSS clip-path with clip-rule=evenodd should emit PDF W* and stay scoped to the clipped element: {text}"
    );
    assert!(
        text.contains("W* n\n0.000 0.000 1.000 rg\nBT /F1"),
        "clipped SVG labels should keep real PDF text while applying the inherited clip path: {text}"
    );
}

#[test]
fn pdf_svg_masks_apply_supported_hard_mask_geometry_as_pdf_clipping() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 90 36">
  <defs>
    <mask id="reveal">
      <rect x="5" y="4" width="18" height="10" fill="white"/>
      <rect x="0" y="0" width="4" height="4" fill="black"/>
    </mask>
    <mask id="hole">
      <path fill="white" fill-rule="evenodd" d="M50 0 H80 V30 H50 Z M58 8 H72 V22 H58 Z"/>
    </mask>
  </defs>
  <style>
    .masked-label { mask: url(#reveal); fill: #0000ff; }
  </style>
  <rect x="0" y="0" width="32" height="18" fill="#ff0000" mask="url(#reveal)"/>
  <rect x="50" y="0" width="30" height="30" fill="#00ff00" mask="url(#hole)"/>
  <text class="masked-label" x="6" y="16" font-size="10">MaskText</text>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("mask.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Masked](mask.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 5 4 m 23 4 l 23 14 l 5 14 l h W n 1.000 0.000 0.000 rg 0 0 32 18 re f\nQ"),
        "opaque white mask geometry should become a scoped PDF clipping path for filled shapes: {text}"
    );
    assert!(
        !text.contains("0 0 m 4 0 l 4 4 l 0 4 l h W n"),
        "black mask geometry should not reveal content in the hard-mask subset: {text}"
    );
    assert!(
        text.contains("q 50 0 m 80 0 l 80 30 l 50 30 l h 58 8 m 72 8 l 72 22 l 58 22 l h W* n 0.000 1.000 0.000 rg 50 0 30 30 re f\nQ"),
        "evenodd white mask paths should preserve hole-cutting through PDF W*: {text}"
    );
    assert!(
        text.contains("W n\n0.000 0.000 1.000 rg\nBT /F1"),
        "CSS-applied masks should also clip selectable SVG text: {text}"
    );
}

#[test]
fn pdf_svg_object_bounding_box_clip_and_mask_units_scale_to_target_geometry() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 36">
  <defs>
    <clipPath id="middle-half" clipPathUnits="objectBoundingBox">
      <rect x="0.25" y="0" width="0.5" height="1"/>
    </clipPath>
    <mask id="center-window" maskContentUnits="objectBoundingBox">
      <rect x="0.2" y="0.25" width="0.6" height="0.5" fill="white"/>
    </mask>
  </defs>
  <rect x="10" y="4" width="40" height="20" fill="#ff0000" clip-path="url(#middle-half)"/>
  <rect x="60" y="4" width="30" height="20" fill="#00ff00" mask="url(#center-window)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("object-bbox-clip.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Object bbox clip](object-bbox-clip.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "q 20 4 m 40 4 l 40 24 l 20 24 l h W n 1.000 0.000 0.000 rg 10 4 40 20 re f\nQ"
        ),
        "objectBoundingBox clipPath geometry should be scaled into the clipped element bbox, not emitted in 0..1 coordinates: {text}"
    );
    assert!(
        text.contains(
            "q 66 9 m 84 9 l 84 19 l 66 19 l h W n 0.000 1.000 0.000 rg 60 4 30 20 re f\nQ"
        ),
        "objectBoundingBox maskContentUnits geometry should be scaled into the masked element bbox, not emitted in 0..1 coordinates: {text}"
    );
}

#[test]
fn pdf_svg_clip_and_mask_child_transforms_affect_clip_geometry() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 70 24">
  <defs>
    <clipPath id="moved-clip" transform="translate(3 1)">
      <rect x="1" y="2" width="8" height="6" transform="translate(10 4)"/>
    </clipPath>
    <mask id="moved-mask" style="transform: translate(1 2)">
      <rect x="2" y="1" width="8" height="5" fill="white" transform="translate(40 3)"/>
    </mask>
  </defs>
  <rect x="0" y="0" width="30" height="18" fill="#ff0000" clip-path="url(#moved-clip)"/>
  <rect x="36" y="0" width="30" height="18" fill="#0000ff" mask="url(#moved-mask)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("transformed-clip.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Transformed clip](transformed-clip.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains(
            "q 14 7 m 22 7 l 22 13 l 14 13 l h W n 1.000 0.000 0.000 rg 0 0 30 18 re f\nQ"
        ),
        "clipPath element and child transforms must move the emitted PDF clipping geometry: {text}"
    );
    assert!(
        text.contains(
            "q 43 6 m 51 6 l 51 11 l 43 11 l h W n 0.000 0.000 1.000 rg 36 0 30 18 re f\nQ"
        ),
        "supported hard-mask element and child transforms must move the emitted PDF clipping geometry: {text}"
    );
}

#[test]
fn pdf_svg_filter_drop_shadow_gets_vector_shadow_fallback() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 28">
  <defs>
    <filter id="drop-shadow"><feDropShadow dx="4" dy="-3" stdDeviation="6" flood-color="#0f172a" flood-opacity="0.15"/></filter>
  </defs>
  <g filter="url(#drop-shadow)">
    <rect x="4" y="4" width="32" height="16" rx="3" fill="#0000ff" stroke="#00ff00" stroke-width="1"/>
  </g>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("shadow.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Shadow](shadow.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 4 -3 cm /GSa01500150 gs 0.059 0.090 0.165 rg"),
        "feDropShadow parameters should survive into the deterministic vector fallback: {text}"
    );
    assert!(
        text.contains("f Q\n0.000 0.000 1.000 rg"),
        "the real SVG fill should still paint after the shadow fallback: {text}"
    );
    assert!(
        text.contains("0.000 1.000 0.000 RG 1 w"),
        "the real SVG stroke should still paint after the shadow fallback: {text}"
    );
}

#[test]
fn pdf_svg_css_drop_shadow_uses_declared_shadow_values() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 28">
  <style>
    .soft { filter: drop-shadow(5px -1px 6px rgba(12, 34, 56, 0.25)); }
  </style>
  <rect class="soft" x="4" y="4" width="32" height="16" fill="#0000ff" fill-opacity="0.5"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("css-shadow.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Shadow](css-shadow.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("q 1 0 0 1 5 -1 cm /GSa01250125 gs 0.047 0.133 0.220 rg"),
        "CSS drop-shadow() offsets, rgba color, and composed paint alpha should survive into PDF vector fallback: {text}"
    );
}

#[test]
fn pdf_svg_missing_or_unsupported_url_filters_do_not_invent_shadows() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 28">
  <defs>
    <filter id="blur"><feGaussianBlur stdDeviation="3"/></filter>
  </defs>
  <rect x="4" y="4" width="20" height="16" fill="#0000ff" filter="url(#missing)"/>
  <rect x="36" y="4" width="20" height="16" fill="#00ff00" filter="url(#blur)"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("no-shadow.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![No accidental shadow](no-shadow.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("q 1 0 0 1 2 2 cm"),
        "missing or unsupported url() filters must not synthesize the fallback shadow transform: {text}"
    );
    assert!(
        !text.contains("0.890 0.900 0.920 rg"),
        "missing or unsupported url() filters must not synthesize the fallback shadow paint: {text}"
    );
    assert!(
        text.contains("0.000 0.000 1.000 rg") && text.contains("0.000 1.000 0.000 rg"),
        "the actual filtered shapes should still paint normally: {text}"
    );
}

#[test]
fn pdf_svg_smooth_path_commands_continue_the_path() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
  <path d="M10 10 C20 0 30 0 40 10 S60 20 70 10 s20 -10 30 0" fill="none" stroke="#ff0000" stroke-width="2"/>
  <path d="M10 40 Q20 20 30 40 T50 40 t20 0" fill="none" stroke="#0000ff" stroke-width="2"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("smooth.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Smooth paths](smooth.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("50 20 60 20 70 10 c"),
        "absolute S should reflect the previous cubic control point instead of truncating the path: {text}"
    );
    assert!(
        text.contains("80 0 90 0 100 10 c"),
        "relative s should use reflected absolute control points and continue the path: {text}"
    );
    assert!(
        text.contains("36.67 53.33 43.33 53.33 50 40 c"),
        "absolute T should lower to a cubic with the reflected quadratic control point: {text}"
    );
    assert!(
        text.contains("56.67 26.67 63.33 26.67 70 40 c"),
        "relative t should continue from the previous smooth quadratic control point: {text}"
    );
}

#[test]
fn pdf_svg_arc_path_commands_become_cubic_curves() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 80">
  <path d="M20 40 A20 20 0 0 1 60 40 a20 20 0 0 1 40 0" fill="none" stroke="#ff0000" stroke-width="2"/>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("arc.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Arc paths](arc.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        !text.contains("20 40 m 60 40 l 100 40 l"),
        "SVG arcs should not collapse to straight line segments: {text}"
    );
    assert!(
        text.contains("20 28.95 28.95 20 40 20 c"),
        "absolute A should lower the first quarter arc to a cubic segment: {text}"
    );
    assert!(
        text.contains("51.05 20 60 28.95 60 40 c"),
        "absolute A should lower the second quarter arc to a cubic segment: {text}"
    );
    assert!(
        text.contains("60 28.95 68.95 20 80 20 c"),
        "relative a should continue from the current point and lower to cubic segments: {text}"
    );
    assert!(
        text.contains("91.05 20 100 28.95 100 40 c"),
        "relative a should end at the relative target using a cubic segment: {text}"
    );
}

#[test]
fn pdf_svg_oversized_path_data_degrades_to_alt_text() {
    let mut path = String::from("M0 0");
    for idx in 0..4096 {
        path.push_str(&format!(" L{} {}", idx % 128, idx / 128));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 40">
  <path d="{path}" fill="none" stroke="#ff0000" stroke-width="1"/>
</svg>
"##
    );
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "too-many-path-ops.svg",
            svg.into_bytes(),
        )],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Huge vector fallback](too-many-path-ops.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("BT /F"),
        "oversized single-path SVG should degrade to visible alt text instead of allocating unbounded path ops: {text}"
    );
    assert!(
        !text.contains("0 0 m"),
        "oversized SVG path should not be emitted as vector path content: {text}"
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

    // A genuinely-malformed envelope (a chunk before IHDR, or a non-empty IEND)
    // still degrades to alt text.
    for (dest, bytes) in [
        ("images/prefix.png", tiny_rgb_png_with_prefix_chunk()),
        ("images/bad-iend.png", tiny_rgb_png_with_nonempty_iend()),
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

    // A VALID PNG that merely carries trailing bytes after IEND is not malformed;
    // every real decoder renders it, so it embeds as an image (the trailing bytes
    // are discarded during decode and never enter the PDF).
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "images/trailing.png",
            tiny_rgb_png_with_trailing_bytes(),
        )],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Trailing bytes](images/trailing.png)", &opts).unwrap();
    assert!(
        as_text(&pdf).contains("/Subtype /Image"),
        "a valid PNG with trailing bytes after IEND should still embed as an image"
    );
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
        "[site](https://example.com?q=1) [site_upper](HTTPS://EXAMPLE.com/Path) \
         [mail](mailto:hello@example.com) [mail_upper](MAILTO:hello@example.com) \
         [phone_upper](TEL:+15550000000) \
         [bad](javascript:alert(1)) [bad_case](JaVaScRiPt:alert(3)) \
         [gap](<java\tscript:alert(2)>)",
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
    assert!(text.contains("/URI (HTTPS://EXAMPLE.com/Path)"));
    assert!(text.contains("/URI (mailto:hello@example.com)"));
    assert!(text.contains("/URI (MAILTO:hello@example.com)"));
    assert!(text.contains("/URI (TEL:+15550000000)"));
    assert!(
        !text.contains("javascript:alert"),
        "unsafe markdown URL schemes must never become PDF annotations"
    );
    assert!(
        !text.contains("JaVaScRiPt:alert"),
        "mixed-case unsafe markdown URL schemes must never become PDF annotations"
    );
}

#[test]
fn pdf_svg_anchor_links_emit_safe_uri_annotations() {
    let svg = br##"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40">
  <a xlink:href="https://example.com/diagram">
    <rect x="4" y="4" width="24" height="14" fill="#22c55e"/>
  </a>
  <a href="HTTPS://EXAMPLE.com/diagram2">
    <rect x="4" y="24" width="24" height="10" fill="#3b82f6"/>
  </a>
  <a href="javascript:alert(1)">
    <rect x="42" y="4" width="24" height="14" fill="#ef4444"/>
  </a>
  <a href="JaVaScRiPt:alert(2)">
    <rect x="42" y="24" width="24" height="10" fill="#f97316"/>
  </a>
  <a href="https://example.com/invisible">
    <rect x="4" y="34" width="24" height="4" fill="none" stroke="#111111" stroke-width="0"/>
  </a>
</svg>
"##;
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("linked.svg", svg.to_vec())],
        ..PdfOptions::default()
    };
    let pdf = render_pdf("![Linked diagram](linked.svg)", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(
        text.contains("/Annots ["),
        "page should reference SVG link annotations: {text}"
    );
    assert_eq!(
        text.matches("/Subtype /Link").count(),
        2,
        "only the safe visible SVG anchors should become PDF link annotations: {text}"
    );
    assert!(text.contains("/URI (https://example.com/diagram)"));
    assert!(text.contains("/URI (HTTPS://EXAMPLE.com/diagram2)"));
    assert!(
        !text.contains("https://example.com/invisible"),
        "safe but invisible SVG anchors should not create phantom PDF hitboxes"
    );
    assert!(
        !text.contains("javascript:alert"),
        "unsafe SVG anchor schemes must never become PDF annotations"
    );
    assert!(
        !text.contains("JaVaScRiPt:alert"),
        "mixed-case unsafe SVG anchor schemes must never become PDF annotations"
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

#[test]
fn pdf_wrapped_table_cells_are_not_duplicated_in_structure() {
    // Both body cells wrap to multiple visual lines in a 2-column table. Each
    // row's /TR must reference EXACTLY 2 cell children — a wrapped cell extends
    // its existing /TD (or /TH), never spawns a duplicate. A duplicate would make
    // a screen reader read each logical cell torn in half and interleaved.
    let md = "| Col A wide header | Col B wide header |\n|---|---|\n\
              | cell one has quite a lot of text so it wraps to two lines here | cell two also has plenty making both columns narrow |\n\
              | a | b |";
    let pdf = render_pdf(md, &PdfOptions::default()).unwrap();
    let raw = as_text(&pdf);
    let mut checked = 0;
    let mut rest = raw.as_str();
    while let Some(p) = rest.find("/S /TR") {
        let after = &rest[p..];
        let k = after.find("/K [").expect("a /TR must have a /K array");
        let arr = &after[k + 4..];
        let end = arr.find(']').expect("the /K array must close");
        let cells = arr[..end].matches(" 0 R").count();
        assert_eq!(
            cells, 2,
            "each /TR must reference exactly 2 cells, got {cells}"
        );
        checked += 1;
        rest = &after[k + 4..];
    }
    assert!(
        checked >= 2,
        "expected header + body rows, checked {checked}"
    );
}
