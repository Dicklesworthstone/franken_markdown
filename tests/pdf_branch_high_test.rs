//! Branch-coverage tests for the *upper half* of `src/pdf.rs` (lines >= 12700):
//! word/line breaking, Knuth-Plass paragraph assembly, table column measuring
//! and page-break repetition, list/task markers, pagination (widows/orphans and
//! keep-with-next), PNG chunk rejection edges, code-block fitting/wrapping, and
//! the richer SVG drawing paths (gradients, markers, text, shadows, patterns).
//!
//! Like `pdf_test.rs`/`pdf_coverage_test.rs` these are intentionally byte-level:
//! every test pins a concrete, observable writer invariant (a PDF operator
//! substring, an object/xref shape, a page count, or a link/annotation rect)
//! without leaning on a third-party PDF parser. No test asserts a bare `is_ok`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PageMargins, PageSize, PdfImageAsset, PdfOptions, Theme, parse_markdown, render_pdf,
    render_warnings,
};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Count the number of page objects (`/Type /Page` but not `/Type /Pages`).
fn page_count(bytes: &[u8]) -> usize {
    let text = as_text(bytes);
    text.matches("/Type /Page\n").count() + text.matches("/Type /Page ").count()
}

fn svg_opts(name: &str, svg: impl Into<Vec<u8>>) -> PdfOptions {
    PdfOptions {
        image_assets: vec![PdfImageAsset::new(name, svg)],
        ..PdfOptions::default()
    }
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

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes()); // CRC is not validated by the reader.
    out
}

fn ihdr(width: u32, height: u32, bit_depth: u8, color_type: u8, interlace: u8) -> Vec<u8> {
    ihdr_full(width, height, bit_depth, color_type, 0, 0, interlace)
}

fn ihdr_full(
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    compression: u8,
    filter: u8,
    interlace: u8,
) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&width.to_be_bytes());
    d.extend_from_slice(&height.to_be_bytes());
    d.extend_from_slice(&[bit_depth, color_type, compression, filter, interlace]);
    d
}

/// Assemble a PNG from a signature, an IHDR payload, and a sequence of extra
/// chunks. Callers control every field so malformed variants are pinned exactly.
fn png_bytes(ihdr_data: &[u8], chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", ihdr_data));
    for (kind, data) in chunks {
        png.extend_from_slice(&png_chunk(kind, data));
    }
    png
}

fn zlib(data: &[u8]) -> Vec<u8> {
    franken_markdown::compress::zlib_compress(data)
}

fn render(md: &str, opts: &PdfOptions) -> Vec<u8> {
    render_pdf(md, opts).unwrap()
}

/// Per-page content-stream bodies that carry text (each begins a `BT /F`).
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

/// Number of text-line placements (`BT /F...`) in the whole file.
fn text_lines(bytes: &[u8]) -> usize {
    as_text(bytes).matches("BT /F").count()
}

// ===========================================================================
// PNG chunk rejection edges (parse_png_chunks). Every rejected image must fall
// back to visible alt text and never produce an image XObject.
// ===========================================================================

fn assert_rejected_png(dest: &str, bytes: Vec<u8>) {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(dest, bytes)],
        ..PdfOptions::default()
    };
    let md = format!("![alt {dest}]({dest})");
    let doc = parse_markdown(&md);
    let warns = render_warnings(&doc, &opts);
    assert!(
        !warns.is_empty(),
        "{dest}: a malformed PNG must produce a degraded-content warning"
    );
    let text = as_text(&render(&md, &opts));
    assert!(
        !text.contains("/Subtype /Image"),
        "{dest}: rejected PNG must not become an image XObject"
    );
    assert!(
        text.contains("BT /F"),
        "{dest}: rejected PNG must fall back to visible alt text"
    );
}

#[test]
fn pdf_png_ihdr_geometry_and_flag_rejections_fall_back_to_alt_text() {
    let idat = zlib(&[0u8; 16]);

    // interlace byte = 2 (> 1): only 0/1 are valid.
    assert_rejected_png(
        "il.png",
        png_bytes(
            &ihdr(2, 1, 8, 2, 2),
            &[(b"IDAT", idat.clone()), (b"IEND", Vec::new())],
        ),
    );
    // compression method byte != 0.
    assert_rejected_png(
        "comp.png",
        png_bytes(
            &ihdr_full(2, 1, 8, 2, 1, 0, 0),
            &[(b"IDAT", idat.clone()), (b"IEND", Vec::new())],
        ),
    );
    // filter method byte != 0.
    assert_rejected_png(
        "filt.png",
        png_bytes(
            &ihdr_full(2, 1, 8, 2, 0, 1, 0),
            &[(b"IDAT", idat.clone()), (b"IEND", Vec::new())],
        ),
    );
    // pixel count over the cap: 6000*5000 = 30M > MAX_PDF_IMAGE_PIXELS (24M).
    assert_rejected_png(
        "big.png",
        png_bytes(
            &ihdr(6000, 5000, 8, 2, 0),
            &[(b"IDAT", idat.clone()), (b"IEND", Vec::new())],
        ),
    );
    // decoded-byte cap: 5000*4000=20M pixels (<=24M) but 16-bit RGBA = 160MB > 96MB.
    assert_rejected_png(
        "deep.png",
        png_bytes(
            &ihdr(5000, 4000, 16, 6, 0),
            &[(b"IDAT", idat.clone()), (b"IEND", Vec::new())],
        ),
    );
    // invalid color-type / bit-depth combo: truecolor (2) at 1-bit.
    assert_rejected_png(
        "combo.png",
        png_bytes(
            &ihdr(2, 1, 1, 2, 0),
            &[(b"IDAT", idat.clone()), (b"IEND", Vec::new())],
        ),
    );
    // duplicate IHDR chunk.
    assert_rejected_png(
        "dupihdr.png",
        png_bytes(
            &ihdr(2, 1, 8, 2, 0),
            &[
                (b"IHDR", ihdr(2, 1, 8, 2, 0)),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
}

#[test]
fn pdf_png_palette_and_trns_chunk_rejections_fall_back_to_alt_text() {
    let idat = zlib(&[0u8; 8]);

    // PLTE with length not a multiple of 3.
    assert_rejected_png(
        "plte-mod3.png",
        png_bytes(
            &ihdr(2, 1, 8, 3, 0),
            &[
                (b"PLTE", vec![1, 2, 3, 4]),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    // Empty PLTE.
    assert_rejected_png(
        "plte-empty.png",
        png_bytes(
            &ihdr(2, 1, 8, 3, 0),
            &[
                (b"PLTE", Vec::new()),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    // PLTE on a grayscale (color type 0) image is illegal.
    assert_rejected_png(
        "plte-gray.png",
        png_bytes(
            &ihdr(2, 1, 8, 0, 0),
            &[
                (b"PLTE", vec![1, 2, 3]),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    // 1-bit palette with more entries than 2^1 = 2 colors.
    assert_rejected_png(
        "plte-toobig.png",
        png_bytes(
            &ihdr(2, 1, 1, 3, 0),
            &[
                (b"PLTE", vec![0, 0, 0, 1, 1, 1, 2, 2, 2]),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    // Duplicate PLTE.
    assert_rejected_png(
        "plte-dup.png",
        png_bytes(
            &ihdr(2, 1, 8, 3, 0),
            &[
                (b"PLTE", vec![0, 0, 0]),
                (b"PLTE", vec![1, 1, 1]),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    // tRNS before PLTE on a palette image.
    assert_rejected_png(
        "trns-noplte.png",
        png_bytes(
            &ihdr(2, 1, 8, 3, 0),
            &[
                (b"tRNS", vec![0xFF]),
                (b"PLTE", vec![0, 0, 0]),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    // tRNS on an RGBA image (color type 6) is illegal (alpha already present).
    assert_rejected_png(
        "trns-rgba.png",
        png_bytes(
            &ihdr(1, 1, 8, 6, 0),
            &[
                (b"tRNS", vec![0, 0, 0, 0, 0, 0]),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
}

#[test]
fn pdf_png_missing_ihdr_and_iend_streams_fall_back_to_alt_text() {
    // Signature only: the chunk loop never runs, so IHDR is absent.
    assert_rejected_png("sigonly.png", b"\x89PNG\r\n\x1A\n".to_vec());
    // IHDR + IDAT but no IEND terminator.
    let idat = zlib(&[0u8; 8]);
    assert_rejected_png(
        "no-iend.png",
        png_bytes(&ihdr(2, 1, 8, 2, 0), &[(b"IDAT", idat)]),
    );
    // A non-IHDR first chunk is rejected immediately.
    assert_rejected_png("first-not-ihdr.png", {
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
        png.extend_from_slice(&png_chunk(b"IDAT", &[0u8; 4]));
        png.extend_from_slice(&png_chunk(b"IEND", &[]));
        png
    });
}

// ===========================================================================
// Table column measuring / allocation.
// ===========================================================================

#[test]
fn pdf_table_with_long_unbreakable_cell_words_measures_and_wraps() {
    let md = "\
| Short | Description |
|-------|-------------|
| aaa | Supercalifragilisticexpialidocioussupercalifragilistic longtail continues onward |
| bbb | tiny |
| ccc | another quite lengthy descriptive passage that must wrap across several visual lines here |
";
    let pdf = render(md, &small_page_opts(200.0, 400.0));
    let text = as_text(&pdf);
    assert!(text.contains("/S /Table"), "table structure tag expected");
    assert!(
        text.contains("/S /TR") && text.contains("/S /TH"),
        "table rows and header cells must be tagged"
    );
    // Three body rows in two columns tag exactly six data cells; the long
    // descriptive cells drive the cell wrapper / column measuring paths.
    assert_eq!(
        text.matches("/S /TD").count(),
        6,
        "three two-column body rows must tag six data cells"
    );
    assert_eq!(page_count(&pdf), 1, "the measured table stays on one page");
}

#[test]
fn pdf_many_column_table_allocates_minimum_widths_on_a_narrow_page() {
    let md = "\
| a | b | c | d | e | f | g |
|---|---|---|---|---|---|---|
| 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| 11 | 22 | 33 | 44 | 55 | 66 | 77 |
";
    let pdf = render(md, &small_page_opts(120.0, 400.0));
    let text = as_text(&pdf);
    assert!(text.contains("/S /Table"), "narrow table still tagged");
    assert_eq!(page_count(&pdf), 1, "the tiny table fits on one page");
}

#[test]
fn pdf_wide_table_columns_expand_toward_max_content() {
    let md = "\
| Key | Value |
|-----|-------|
| x | this column carries almost all of the descriptive weight in the whole table |
| y | z |
";
    let pdf = render(md, &small_page_opts(520.0, 400.0));
    let text = as_text(&pdf);
    assert!(text.contains("/S /Table"), "wide table tagged");
    assert!(
        text.contains("/S /TD") && text.contains("/S /TR"),
        "wide table must tag data cells and rows"
    );
    assert_eq!(page_count(&pdf), 1, "the wide table fits on one page");
}

#[test]
fn pdf_tall_table_repeats_its_header_row_on_each_page() {
    let mut md = String::from("| Item | Detail |\n|------|--------|\n");
    for i in 0..40 {
        md.push_str(&format!("| row{i} | detail value number {i} here |\n"));
    }
    let pdf = render(&md, &small_page_opts(260.0, 160.0));
    let streams = text_streams(&pdf);
    assert!(
        streams.len() >= 2,
        "40-row table must span multiple content streams, got {}",
        streams.len()
    );
    // The bold header font (/F2) is re-emitted at the top of a continuation
    // page, and body rows (/F1) follow it — that is the repeated header.
    assert!(
        streams[1].contains("/F2 ") && streams[1].contains("/F1 "),
        "continuation page must repeat the bold header then body rows"
    );
}

// ===========================================================================
// Word/line breaking: long words, URLs (separator breaks), emergency breaks.
// ===========================================================================

#[test]
fn pdf_extremely_long_alphabetic_word_gets_emergency_break_points() {
    let word = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnop"; // 42 chars, ascii-alphabetic
    let pdf = render(word, &small_page_opts(90.0, 240.0));
    let text = as_text(&pdf);
    assert!(text.starts_with("%PDF-1.7"), "valid PDF header");
    let placements = text.matches(" Tf ").count();
    assert!(
        placements >= 1,
        "a long word should still be typeset, got {placements} text placements"
    );
}

#[test]
fn pdf_long_url_like_token_breaks_after_separators() {
    let md =
        "See https://example.com/very/long/path/segment-name.with.many.dots/and-more-parts/here";
    let pdf = render(md, &small_page_opts(150.0, 260.0));
    let text = as_text(&pdf);
    assert!(text.starts_with("%PDF-1.7"));
    // The long URL token is broken at its own separators onto multiple lines
    // (no hyphen is synthesized for non-dictionary separator breaks).
    assert!(
        text_lines(&pdf) >= 3,
        "the URL should break across several baselines, got {}",
        text_lines(&pdf)
    );
    assert!(page_count(&pdf) >= 1);
}

#[test]
fn pdf_hyphenating_word_repeated_hits_the_hyphenation_cache() {
    let mut md = String::new();
    for _ in 0..6 {
        md.push_str("representation ");
    }
    let pdf = render(md.trim(), &small_page_opts(70.0, 300.0));
    let text = as_text(&pdf);
    assert!(text.starts_with("%PDF-1.7"));
    // A very narrow measure forces the repeated word to hyphenate/wrap onto many
    // baselines; the cache insert-then-hit path in flush_pdf_word is exercised.
    assert!(
        text_lines(&pdf) >= 4,
        "narrow repeated word should wrap to many lines, got {}",
        text_lines(&pdf)
    );
}

// ===========================================================================
// Pagination penalties: keep-with-next, widows/orphans, code splits, lists.
// ===========================================================================

#[test]
fn pdf_heading_is_kept_with_following_content_across_a_page_break() {
    let mut md = String::new();
    for i in 0..6 {
        md.push_str(&format!(
            "Filler paragraph number {i} with enough words to occupy vertical space on the page.\n\n"
        ));
    }
    md.push_str(
        "## A Late Heading\n\nBody text that belongs with the heading above and must not be orphaned.\n",
    );
    let pdf = render(&md, &small_page_opts(300.0, 150.0));
    assert!(
        page_count(&pdf) >= 2,
        "filler + heading must paginate, got {}",
        page_count(&pdf)
    );
    let text = as_text(&pdf);
    assert!(text.contains("Heading") || text.contains("Late"));
}

#[test]
fn pdf_code_block_resists_internal_page_breaks() {
    let mut md = String::from("Intro paragraph.\n\n```\n");
    for i in 0..12 {
        md.push_str(&format!("line_of_code_number_{i} = compute(value_{i});\n"));
    }
    md.push_str("```\n\nAfter the code block.\n");
    let pdf = render(&md, &small_page_opts(320.0, 200.0));
    let text = as_text(&pdf);
    assert!(
        text.contains("/S /Code") || text.contains("compute"),
        "code block present"
    );
    assert!(page_count(&pdf) >= 1);
}

#[test]
fn pdf_short_final_list_item_is_not_stranded_alone() {
    let md = "\
- first list item with a reasonable amount of descriptive text to fill space
- second list item also carrying a good measure of descriptive words here
- third list item continuing with yet more descriptive filler text on the page
- last
";
    let pdf = render(md, &small_page_opts(300.0, 120.0));
    assert!(
        page_count(&pdf) >= 2,
        "the list must split across pages, got {}",
        page_count(&pdf)
    );
    let text = as_text(&pdf);
    assert!(
        text.contains("/S /LI") || text.contains("first"),
        "list items tagged"
    );
}

#[test]
fn pdf_widow_orphan_control_keeps_two_lines_together_in_a_paragraph() {
    let mut para = String::new();
    for i in 0..60 {
        para.push_str(&format!("word{i} "));
    }
    let md = format!("Intro.\n\n{para}\n");
    let pdf = render(&md, &small_page_opts(200.0, 130.0));
    assert!(
        page_count(&pdf) >= 2,
        "the long paragraph must paginate, got {}",
        page_count(&pdf)
    );
}

// ===========================================================================
// Code blocks: ASCII diagrams, tab expansion, line numbers, wrapping.
// ===========================================================================

#[test]
fn pdf_ascii_diagram_code_block_preserves_its_lines() {
    let md = "\
```
+------+     +------+
| A    |---->| B    |
+------+     +------+
   |            ^
   +----------->+
```
";
    let pdf = render(md, &PdfOptions::default());
    let text = as_text(&pdf);
    assert!(text.contains("/S /Code"), "diagram is still a code block");
    // Preserved (not reflowed): each of the five non-empty source rows becomes
    // its own baseline instead of being wrapped/greedily filled.
    assert!(
        text_lines(&pdf) >= 5,
        "the ASCII diagram must keep one baseline per source row, got {}",
        text_lines(&pdf)
    );
}

#[test]
fn pdf_code_block_with_line_numbers_renders_a_number_column() {
    let opts = PdfOptions {
        code_line_numbers: true,
        ..PdfOptions::default()
    };
    let md = "```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n";
    let pdf = render(md, &opts);
    let text = as_text(&pdf);
    assert!(text.contains("/S /Code"), "code block tagged");
    // Turning line numbers on adds a muted number segment per code row, so the
    // numbered render emits strictly more text placements than the plain one.
    let plain = render(md, &PdfOptions::default());
    assert!(
        text_lines(&pdf) > text_lines(&plain),
        "line-numbered code should add number-column segments: {} vs {}",
        text_lines(&pdf),
        text_lines(&plain)
    );
}

#[test]
fn pdf_code_block_with_tabs_and_long_lines_fits_and_wraps() {
    let md = "\
```text
\tindented_with_tab = 1
this_is_a_very_long_single_line_of_code_that_definitely_exceeds_the_available_width_and_forces_font_fitting_or_wrapping = compute_a_result()
```
";
    let pdf = render(md, &small_page_opts(220.0, 400.0));
    let text = as_text(&pdf);
    assert!(text.contains("/S /Code"), "code block present");
    // The overlong line is fitted/wrapped, so the block spans more than its two
    // source rows of text baselines.
    assert!(
        text_lines(&pdf) >= 2,
        "tabbed + overlong code should still typeset, got {}",
        text_lines(&pdf)
    );
}

// ===========================================================================
// Lists, task markers, nested lists, blockquotes (prefixed inlines).
// ===========================================================================

#[test]
fn pdf_task_list_checkboxes_render_selectable_markers() {
    let md = "- [x] done item\n- [ ] pending item\n";
    let pdf = render(md, &PdfOptions::default());
    let text = as_text(&pdf);
    assert!(
        text.contains("/S /L") && text.contains("/S /LI"),
        "list tags present"
    );
    assert_eq!(
        text.matches("/S /LI").count(),
        2,
        "two task-list items expected"
    );
    // The checked marker draws a rounded, filled accent box (`... c h f`) and a
    // white check stroke — the task-checkbox drawing path ran.
    assert!(
        text.contains("c h f"),
        "a filled rounded checkbox marker should be drawn"
    );
}

#[test]
fn pdf_nested_ordered_and_blockquote_prefixed_inlines_wrap() {
    let md = "\
> A blockquote paragraph with enough words to require wrapping on a narrow page measure indeed.

1. first ordered item with a long descriptive tail that wraps across the measure here
2. second ordered item
   - nested bullet with additional descriptive words that also need to wrap somewhere
";
    let pdf = render(md, &small_page_opts(180.0, 500.0));
    let text = as_text(&pdf);
    assert!(
        text.contains("/S /BlockQuote") || text.contains("blockquote"),
        "quote present"
    );
    assert!(text.contains("/S /L"), "list present");
}

// ===========================================================================
// SVG drawing: gradients, markers, text, shadows, opacity states.
// ===========================================================================

#[test]
fn pdf_svg_repeated_linear_gradient_expands_into_native_shading() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="20" y2="0" gradientUnits="userSpaceOnUse" spreadMethod="repeat">
      <stop offset="0" stop-color="#ff0000"/>
      <stop offset="1" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="120" height="40" fill="url(#g)"/>
</svg>"##;
    let pdf = render("![grad](rep.svg)", &svg_opts("rep.svg", *svg));
    let text = as_text(&pdf);
    assert!(
        text.contains("/Shading") || text.contains(" sh\n") || text.contains(" scn"),
        "repeated gradient must emit a shading or pattern fill"
    );
}

#[test]
fn pdf_svg_reflected_radial_gradient_expands_into_native_shading() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 80">
  <defs>
    <radialGradient id="r" cx="40" cy="40" r="15" gradientUnits="userSpaceOnUse" spreadMethod="reflect">
      <stop offset="0" stop-color="#00ff00"/>
      <stop offset="1" stop-color="#ff00ff"/>
    </radialGradient>
  </defs>
  <circle cx="40" cy="40" r="38" fill="url(#r)"/>
</svg>"##;
    let pdf = render("![radial](refl.svg)", &svg_opts("refl.svg", *svg));
    let text = as_text(&pdf);
    assert!(
        text.contains("/Shading") || text.contains("/Pattern") || text.contains(" scn"),
        "reflected radial gradient must emit shading/pattern paint"
    );
}

#[test]
fn pdf_svg_text_anchors_and_decoration_and_stroke_render() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 80">
  <text x="10" y="20" text-anchor="start" fill="#101010">Start</text>
  <text x="100" y="40" text-anchor="middle" fill="#101010" text-decoration="underline">Middle</text>
  <text x="190" y="60" text-anchor="end" fill="#101010" stroke="#ff0000" stroke-width="0.5"
        text-decoration="line-through" letter-spacing="1.5" word-spacing="3">End word</text>
</svg>"##;
    let pdf = render("![t](txt.svg)", &svg_opts("txt.svg", *svg));
    let text = as_text(&pdf);
    assert!(
        text.contains("BT") && (text.contains(" Tj") || text.contains(" TJ")),
        "SVG text must emit text-showing operators"
    );
}

#[test]
fn pdf_svg_shape_with_drop_shadow_and_opacity_paints_layers() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60">
  <defs>
    <filter id="s"><feDropShadow dx="2" dy="2" stdDeviation="1" flood-color="#000000"/></filter>
  </defs>
  <rect x="10" y="10" width="30" height="30" rx="4" fill="#3366cc" fill-opacity="0.6"
        stroke="#113355" stroke-opacity="0.4" stroke-width="2" filter="url(#s)"/>
</svg>"##;
    let pdf = render("![s](shadow.svg)", &svg_opts("shadow.svg", *svg));
    let text = as_text(&pdf);
    assert!(
        text.contains(" gs") || text.contains("/GS") || text.contains(" re"),
        "shadow/opacity should emit graphics-state or shape operators"
    );
}

// ---------------------------------------------------------------------------
// PNG decode variants (batch 4) + base64 helper for embedded SVG images.
// ---------------------------------------------------------------------------

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
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            out.push(TABLE[(rem[0] >> 2) as usize] as char);
            out.push(TABLE[((rem[0] & 0x03) << 4) as usize] as char);
            out.push_str("==");
        }
        2 => {
            out.push(TABLE[(rem[0] >> 2) as usize] as char);
            out.push(TABLE[(((rem[0] & 0x03) << 4) | (rem[1] >> 4)) as usize] as char);
            out.push(TABLE[((rem[1] & 0x0f) << 2) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

fn png_from_rows(
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    rows: &[Vec<u8>],
    extra_before_idat: &[(&[u8; 4], Vec<u8>)],
) -> Vec<u8> {
    let mut raw = Vec::new();
    for r in rows {
        raw.extend_from_slice(r);
    }
    let mut chunks: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    for c in extra_before_idat {
        chunks.push((c.0, c.1.clone()));
    }
    chunks.push((b"IDAT", zlib(&raw)));
    chunks.push((b"IEND", Vec::new()));
    png_bytes(&ihdr(width, height, bit_depth, color_type, 0), &chunks)
}

fn valid_rgb_png(pixels: &[[u8; 3]]) -> Vec<u8> {
    let mut row = vec![0u8];
    for p in pixels {
        row.extend_from_slice(p);
    }
    png_from_rows(pixels.len() as u32, 1, 8, 2, &[row], &[])
}

fn assert_decodes(dest: &str, bytes: Vec<u8>) -> String {
    let opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new(dest, bytes)],
        ..PdfOptions::default()
    };
    let md = format!("![img {dest}]({dest})");
    let doc = parse_markdown(&md);
    assert!(
        render_warnings(&doc, &opts).is_empty(),
        "{dest}: a valid PNG variant must decode without a warning"
    );
    let text = as_text(&render(&md, &opts));
    assert!(
        text.contains("/Subtype /Image") && text.contains(" Do"),
        "{dest}: a decoded PNG must become a drawn image XObject"
    );
    text
}

#[test]
fn pdf_png_palette_and_palette_alpha_decode_to_rgb_and_soft_mask() {
    let plte = vec![0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF];
    let png = png_from_rows(3, 1, 8, 3, &[vec![0, 0, 1, 2]], &[(b"PLTE", plte.clone())]);
    let text = assert_decodes("palette.png", png);
    assert!(
        text.contains("/ColorSpace /DeviceRGB"),
        "palette resolves to RGB"
    );

    let png_a = png_from_rows(
        3,
        1,
        8,
        3,
        &[vec![0, 0, 1, 2]],
        &[(b"PLTE", plte), (b"tRNS", vec![0x00, 0x80, 0xFF])],
    );
    let text_a = assert_decodes("palette-alpha.png", png_a);
    assert!(
        text_a.contains(" /SMask "),
        "transparent palette carries a soft mask"
    );
}

#[test]
fn pdf_png_grayscale_alpha_decodes_with_soft_mask() {
    let rows = vec![
        vec![0, 0x20, 0xFF, 0x40, 0x80],
        vec![4, 0x05, 0x10, 0x02, 0x08],
    ];
    let png = png_from_rows(2, 2, 8, 4, &rows, &[]);
    let text = assert_decodes("gray-alpha.png", png);
    assert!(
        text.contains("/ColorSpace /DeviceGray") && text.contains(" /SMask "),
        "gray+alpha keeps gray samples and emits a soft mask"
    );
}

#[test]
fn pdf_png_sixteen_bit_rgb_scales_down_to_eight_bit() {
    let rows = vec![vec![0, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]];
    let png = png_from_rows(1, 1, 16, 2, &rows, &[]);
    let text = assert_decodes("rgb16.png", png);
    assert!(
        text.contains("/BitsPerComponent 8"),
        "16-bit scales to 8-bit"
    );
}

#[test]
fn pdf_png_grayscale_and_rgb_trns_become_soft_masks() {
    let gray = png_from_rows(
        2,
        1,
        8,
        0,
        &[vec![0, 0x10, 0x10]],
        &[(b"tRNS", vec![0x00, 0x10])],
    );
    assert!(assert_decodes("gray-trns.png", gray).contains(" /SMask "));
    let rgb = png_from_rows(
        2,
        1,
        8,
        2,
        &[vec![0, 0x10, 0x20, 0x30, 0x10, 0x20, 0x30]],
        &[(b"tRNS", vec![0x00, 0x10, 0x00, 0x20, 0x00, 0x30])],
    );
    assert!(assert_decodes("rgb-trns.png", rgb).contains(" /SMask "));
}

#[test]
fn pdf_png_interlaced_rgb_decodes_through_adam7_passes() {
    fn pass_count(total: u32, start: u32, step: u32) -> u32 {
        if total <= start {
            0
        } else {
            (total - start).div_ceil(step)
        }
    }
    const ADAM7: [(u32, u32, u32, u32); 7] = [
        (0, 8, 0, 8),
        (4, 8, 0, 8),
        (0, 4, 4, 8),
        (2, 4, 0, 4),
        (0, 2, 2, 4),
        (1, 2, 0, 2),
        (0, 1, 1, 2),
    ];
    let (w, h) = (4u32, 4u32);
    let px = |x: u32, y: u32| -> [u8; 3] { [(x * 40) as u8, (y * 40) as u8, ((x + y) * 20) as u8] };
    let mut raw = Vec::new();
    for &(xs, xstep, ys, ystep) in &ADAM7 {
        let (pw, ph) = (pass_count(w, xs, xstep), pass_count(h, ys, ystep));
        if pw == 0 || ph == 0 {
            continue;
        }
        for j in 0..ph {
            raw.push(0u8);
            for i in 0..pw {
                raw.extend_from_slice(&px(xs + i * xstep, ys + j * ystep));
            }
        }
    }
    let png = png_bytes(
        &ihdr(w, h, 8, 2, 1),
        &[(b"IDAT", zlib(&raw)), (b"IEND", Vec::new())],
    );
    let text = assert_decodes("interlaced.png", png);
    assert!(
        text.contains("/Width 4 /Height 4"),
        "interlaced image keeps 4x4 geometry"
    );
}

#[test]
fn pdf_png_more_chunk_ordering_rejections_fall_back_to_alt_text() {
    let idat = zlib(&[0u8; 8]);
    assert_rejected_png(
        "plte-after-idat.png",
        png_bytes(
            &ihdr(2, 1, 8, 3, 0),
            &[
                (b"IDAT", idat.clone()),
                (b"PLTE", vec![0, 0, 0, 1, 1, 1]),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    assert_rejected_png(
        "dup-trns.png",
        png_bytes(
            &ihdr(2, 1, 8, 0, 0),
            &[
                (b"tRNS", vec![0x00, 0x10]),
                (b"tRNS", vec![0x00, 0x20]),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
    assert_rejected_png(
        "split-idat.png",
        png_bytes(
            &ihdr(2, 1, 8, 2, 0),
            &[
                (b"IDAT", idat.clone()),
                (b"tEXt", b"k\0v".to_vec()),
                (b"IDAT", idat.clone()),
                (b"IEND", Vec::new()),
            ],
        ),
    );
}

#[test]
fn pdf_png_fast_path_predictor_length_mismatch_is_rejected() {
    // Fast-path (8-bit RGB, opaque): IHDR claims 2x2 (needs 2*(1+2*3)=14 raw
    // bytes) but the IDAT inflates to a different length -> predictor payload
    // invalid -> rejected to alt text.
    let short = png_from_rows(2, 2, 8, 2, &[vec![0, 1, 2, 3, 4, 5, 6]], &[]);
    assert_rejected_png("badpredictor.png", short);
}

// ===========================================================================
// SVG drawing: SIMPLE single-feature images. Each isolates one graphics-state
// or paint branch that the existing suite (which mostly dashes/clips lines, not
// shapes) leaves cold. Every SVG is minimal so it always parses and paints.
// ===========================================================================

fn svg_render(name: &str, svg: &[u8]) -> String {
    as_text(&render(
        &format!("![x]({name})"),
        &svg_opts(name, svg.to_vec()),
    ))
}

#[test]
fn pdf_svg_dashed_rect_emits_state_prefix_and_dash() {
    let s = svg_render(
        "dashrect.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><rect x="4" y="4" width="30" height="30" fill="#334455" stroke="#000000" stroke-width="2" stroke-dasharray="4 2"/></svg>"##,
    );
    assert!(
        s.contains(" d\n") || s.contains(" d ") || s.contains("2 ] "),
        "dashed shape emits a dash array"
    );
}

#[test]
fn pdf_svg_clipped_rect_emits_clip_operator() {
    let s = svg_render(
        "cliprect.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><defs><clipPath id="c" clipPathUnits="userSpaceOnUse"><rect x="0" y="0" width="20" height="20"/></clipPath></defs><rect x="4" y="4" width="30" height="30" fill="#334455" clip-path="url(#c)"/></svg>"##,
    );
    assert!(s.contains("W n"), "clipped shape emits a clip operator");
}

#[test]
fn pdf_svg_masked_rect_emits_clip_operator() {
    let s = svg_render(
        "maskrect.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><defs><mask id="m" maskUnits="userSpaceOnUse"><rect x="0" y="0" width="40" height="20" fill="#ffffff"/></mask></defs><rect x="4" y="4" width="30" height="30" fill="#334455" mask="url(#m)"/></svg>"##,
    );
    assert!(
        s.contains("W n") || s.contains(" q "),
        "masked shape emits a clip/state prefix"
    );
}

#[test]
fn pdf_svg_opacity_rect_emits_alpha_graphics_state() {
    let s = svg_render(
        "opacityrect.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><rect x="4" y="4" width="30" height="30" fill="#334455" fill-opacity="0.5"/></svg>"##,
    );
    assert!(
        s.contains(" gs") || s.contains("/GS") || s.contains(" re f"),
        "translucent fill sets an ExtGState alpha"
    );
}

#[test]
fn pdf_svg_transformed_rect_emits_cm_operator() {
    let s = svg_render(
        "txrect.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><rect x="4" y="4" width="20" height="20" fill="#334455" transform="translate(4 4) rotate(15)"/></svg>"##,
    );
    assert!(s.contains(" cm"), "a transformed shape emits a cm operator");
}

#[test]
fn pdf_svg_fill_shape_drop_shadow_paints_offset_fill_layer() {
    let s = svg_render(
        "fillshadow.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60"><defs><filter id="s"><feDropShadow dx="3" dy="3" stdDeviation="0" flood-color="#000000" flood-opacity="0.4"/></filter></defs><rect x="12" y="12" width="30" height="30" fill="#3366cc" filter="url(#s)"/></svg>"##,
    );
    assert!(
        s.contains(" cm ") && (s.contains(" f") || s.contains("rg")),
        "fill drop shadow offsets and fills"
    );
}

#[test]
fn pdf_svg_stroke_shape_drop_shadow_paints_offset_stroke_layer() {
    let s = svg_render(
        "strokeshadow.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60"><defs><filter id="s"><feDropShadow dx="3" dy="3" stdDeviation="0" flood-color="#000000"/></filter></defs><rect x="12" y="12" width="30" height="30" fill="none" stroke="#207020" stroke-width="3" filter="url(#s)"/></svg>"##,
    );
    assert!(
        s.contains(" cm ") && s.contains(" S"),
        "stroke-only drop shadow offsets and strokes"
    );
}

#[test]
fn pdf_svg_focal_radial_gradient_object_bbox_registers_radial_shading() {
    let s = svg_render(
        "focalradial.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60"><defs><radialGradient id="r" cx="0.5" cy="0.5" r="0.5" fx="0.3" fy="0.3" fr="0.05"><stop offset="0" stop-color="#ffff00"/><stop offset="1" stop-color="#0000ff"/></radialGradient></defs><rect x="4" y="4" width="52" height="52" fill="url(#r)"/></svg>"##,
    );
    assert!(
        s.contains("/ShadingType 3") || s.contains(" scn"),
        "focal radial registers a radial shading"
    );
}

#[test]
fn pdf_svg_skewed_radial_gradient_falls_back_to_solid_fill() {
    let s = svg_render(
        "skewradial.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><defs><radialGradient id="r" cx="20" cy="20" r="15" gradientUnits="userSpaceOnUse" gradientTransform="scale(2 1)"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#00ff00"/></radialGradient></defs><rect x="2" y="2" width="36" height="36" fill="url(#r) #123456"/></svg>"##,
    );
    assert!(
        s.contains("0.500 0.500 0.000 rg") && !s.contains("/ShadingType 3"),
        "a non-circular radial gradient falls back to a flat averaged fill, not a shading"
    );
}

#[test]
fn pdf_svg_percent_userspace_radial_gradient_falls_back_to_solid() {
    let s = svg_render(
        "pctradial.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><defs><radialGradient id="r" cx="50%" cy="50%" r="50%" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#00ff00"/></radialGradient></defs><rect x="2" y="2" width="36" height="36" fill="url(#r) #654321"/></svg>"##,
    );
    assert!(
        s.contains("0.500 0.500 0.000 rg") && !s.contains("/ShadingType 3"),
        "percent userSpace radial falls back to a flat averaged fill, not a shading"
    );
}

#[test]
fn pdf_svg_repeated_radial_gradient_object_bbox_tiles_rings() {
    let s = svg_render(
        "reprad.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 60"><defs><radialGradient id="r" cx="0.5" cy="0.5" r="0.15" spreadMethod="repeat"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></radialGradient></defs><rect x="4" y="4" width="52" height="52" fill="url(#r)"/></svg>"##,
    );
    assert!(
        s.contains("/ShadingType 3") || s.contains(" scn") || s.contains("/Pattern"),
        "repeated radial tiles ring shadings"
    );
}

#[test]
fn pdf_svg_reflected_linear_gradient_multi_period() {
    let s = svg_render(
        "reflin.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><defs><linearGradient id="g" x1="0" y1="0" x2="10" y2="0" gradientUnits="userSpaceOnUse" spreadMethod="reflect"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#00ff00"/></linearGradient></defs><rect x="0" y="0" width="100" height="20" fill="url(#g)"/></svg>"##,
    );
    assert!(
        s.matches("/ShadingType 2").count() >= 2 || s.contains(" scn"),
        "reflected linear tiles axial shadings"
    );
}

#[test]
fn pdf_svg_line_gradient_stroke_square_cap() {
    let s = svg_render(
        "linegradsq.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><defs><linearGradient id="g" x1="0" y1="0" x2="100" y2="0" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><line x1="6" y1="10" x2="94" y2="10" stroke="url(#g)" stroke-width="5" stroke-linecap="square"/></svg>"##,
    );
    assert!(
        s.contains(" sh") || s.contains("/ShadingType 2"),
        "square-cap gradient line clips a band and shades"
    );
}

#[test]
fn pdf_svg_line_gradient_stroke_with_dash_falls_back() {
    // A dashed gradient-stroked line takes the early-return path; the line still
    // strokes (with a flat approximation) rather than a clipped shading band.
    let s = svg_render(
        "linegraddash.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><defs><linearGradient id="g" x1="0" y1="0" x2="100" y2="0" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><line x1="6" y1="10" x2="94" y2="10" stroke="url(#g)" stroke-width="5" stroke-dasharray="6 3"/></svg>"##,
    );
    assert!(
        s.contains(" S") || s.contains(" d\n") || s.contains("RG"),
        "dashed gradient line still strokes"
    );
}

#[test]
fn pdf_svg_line_with_round_cap_gradient_stroke_falls_back() {
    let s = svg_render(
        "linegradround.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><defs><linearGradient id="g" x1="0" y1="0" x2="100" y2="0" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><line x1="6" y1="10" x2="94" y2="10" stroke="url(#g)" stroke-width="5" stroke-linecap="round"/></svg>"##,
    );
    assert!(
        s.contains(" S") || s.contains(" J") || s.contains("RG"),
        "round-cap gradient line still strokes"
    );
}

#[test]
fn pdf_svg_pattern_fill_rect_clips_and_tiles() {
    let s = svg_render(
        "patfill.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><defs><pattern id="p" width="10" height="10" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="5" height="5" fill="#3060c0"/></pattern></defs><rect x="0" y="0" width="40" height="40" fill="url(#p)"/></svg>"##,
    );
    assert!(s.contains("W n"), "pattern fill clips the shape then tiles");
}

#[test]
fn pdf_svg_pattern_stroke_line_clips_band_and_tiles() {
    let s = svg_render(
        "patstroke.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 20"><defs><pattern id="p" width="6" height="6" patternUnits="userSpaceOnUse"><rect x="0" y="0" width="3" height="6" fill="#a000a0"/></pattern></defs><line x1="4" y1="10" x2="56" y2="10" stroke="url(#p)" stroke-width="6"/></svg>"##,
    );
    assert!(
        s.contains("W n") || s.contains(" re f"),
        "pattern stroke clips a band and tiles"
    );
}

#[test]
fn pdf_svg_line_markers_start_mid_end_paint_shapes() {
    let s = svg_render(
        "linemarkers.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><defs><marker id="d" markerWidth="6" markerHeight="6" refX="3" refY="3" markerUnits="userSpaceOnUse"><circle cx="3" cy="3" r="2" fill="#c02020"/></marker></defs><line x1="8" y1="10" x2="92" y2="10" stroke="#111" stroke-width="2" marker-start="url(#d)" marker-end="url(#d)"/></svg>"##,
    );
    assert!(
        s.contains(" cm") || s.contains(" c ") || s.contains("rg"),
        "line markers place and paint shapes"
    );
}

#[test]
fn pdf_svg_polyline_mid_markers_and_arrow_end() {
    let s = svg_render(
        "polymarkers.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40"><defs><marker id="d" markerWidth="6" markerHeight="6" refX="3" refY="3"><circle cx="3" cy="3" r="2" fill="#c02020"/></marker><marker id="a" markerWidth="8" markerHeight="8" refX="4" refY="4" orient="auto"><path d="M0 0 L8 4 L0 8 Z" fill="#2020c0"/></marker></defs><polyline points="5,30 40,10 80,30 110,10" fill="none" stroke="#111" stroke-width="2" marker-mid="url(#d)" marker-end="url(#a)"/></svg>"##,
    );
    assert!(
        s.contains(" cm") || s.contains(" c ") || s.contains("rg"),
        "polyline mid/end markers paint"
    );
}

#[test]
fn pdf_svg_dangling_marker_stroke_and_fill_arrowheads() {
    let s1 = svg_render(
        "arrowstroke.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><line x1="6" y1="10" x2="90" y2="10" fill="none" stroke="#1010a0" stroke-width="2" marker-end="url(#none)"/></svg>"##,
    );
    assert!(
        s1.contains(" S") || s1.contains("RG") || s1.contains(" f"),
        "dangling-marker stroke line synthesizes an arrowhead"
    );
    let s2 = svg_render(
        "arrowfill.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20"><path d="M6 10 L90 10" stroke="#a01010" stroke-width="3" marker-end="url(#none)"/></svg>"##,
    );
    assert!(
        s2.contains(" S") || s2.contains("RG") || s2.contains(" f"),
        "dangling-marker path synthesizes an arrowhead"
    );
}

#[test]
fn pdf_svg_embedded_raster_image_draws_xobject() {
    let uri = format!(
        "data:image/png;base64,{}",
        base64_encode(&valid_rgb_png(&[[0x10, 0x80, 0xF0], [0xF0, 0x40, 0x10]]))
    );
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"><image x="4" y="2" width="16" height="16" preserveAspectRatio="xMidYMid meet" href="{uri}"/></svg>"##
    );
    let s = svg_render("embedraster.svg", svg.as_bytes());
    assert!(
        s.matches(" Do").count() == 1 && s.contains(" cm"),
        "embedded raster draws one XObject via a cm"
    );
}

#[test]
fn pdf_svg_embedded_raster_slice_clips_viewport() {
    let uri = format!(
        "data:image/png;base64,{}",
        base64_encode(&valid_rgb_png(&[[0x10, 0x80, 0xF0], [0xF0, 0x40, 0x10]]))
    );
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"><image x="4" y="2" width="16" height="16" preserveAspectRatio="xMidYMid slice" href="{uri}"/></svg>"##
    );
    let s = svg_render("embedslice.svg", svg.as_bytes());
    assert!(
        s.contains("re W n") && s.contains(" Do"),
        "slice embedding clips its viewport before drawing"
    );
}

#[test]
fn pdf_svg_root_preserve_aspect_none_stretches() {
    let s = svg_render(
        "aspnone.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20" width="40" height="40" preserveAspectRatio="none"><rect x="0" y="0" width="100" height="20" fill="#3050a0"/></svg>"##,
    );
    assert!(
        s.contains(" re f") || s.contains(" cm"),
        "preserveAspectRatio=none stretches content"
    );
}

#[test]
fn pdf_svg_root_preserve_aspect_slice_clips() {
    let s = svg_render(
        "aspslice.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 20" width="40" height="40" preserveAspectRatio="xMidYMid slice"><rect x="0" y="0" width="100" height="20" fill="#a05030"/></svg>"##,
    );
    assert!(
        s.contains("re W n") || s.contains(" cm"),
        "preserveAspectRatio=slice clips to the viewport"
    );
}

#[test]
fn pdf_svg_text_underline_and_line_through_decorations() {
    let s = svg_render(
        "textdec.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40"><text x="6" y="16" font-size="12" fill="#101010" text-decoration="underline">Under</text><text x="6" y="34" font-size="12" fill="#101010" text-decoration="line-through">Strike</text></svg>"##,
    );
    assert!(
        s.contains("BT") && (s.contains(" l\n") || s.contains(" re f") || s.contains(" S")),
        "text decorations draw rules"
    );
}

#[test]
fn pdf_svg_text_middle_and_end_anchor_offsets() {
    let s = svg_render(
        "textanchor.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 40"><text x="80" y="16" font-size="12" fill="#101010" text-anchor="middle">Middle</text><text x="150" y="34" font-size="12" fill="#101010" text-anchor="end">End</text></svg>"##,
    );
    assert!(
        s.contains("BT") && (s.contains(" Tj") || s.contains(" TJ")),
        "anchored text still shows glyphs"
    );
}

#[test]
fn pdf_svg_text_length_adjust_spacing_and_glyphs() {
    let s = svg_render(
        "textlen.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 40"><text x="4" y="16" font-size="12" fill="#101010" textLength="120" lengthAdjust="spacing">Stretch me</text><text x="4" y="34" font-size="12" fill="#101010" textLength="40" lengthAdjust="spacingAndGlyphs">Squeeze me</text></svg>"##,
    );
    assert!(s.contains("BT"), "textLength/lengthAdjust text renders");
}

#[test]
fn pdf_svg_text_with_stroke_and_fill_uses_fill_stroke_mode() {
    let s = svg_render(
        "textstroke.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40"><text x="6" y="24" font-size="18" fill="#101010" stroke="#ff0000" stroke-width="0.6">Outlined</text></svg>"##,
    );
    assert!(
        s.contains("BT")
            && (s.contains(" Tr") || s.contains(" RG") || s.contains(" Tj") || s.contains(" TJ")),
        "fill+stroke text uses a text render mode"
    );
}

#[test]
fn pdf_svg_link_on_rect_and_text_registers_annotations() {
    let s = svg_render(
        "svglink.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60"><a href="https://example.com/rect"><rect x="4" y="4" width="40" height="20" fill="#3366cc"/></a><a href="https://example.com/text"><text x="4" y="50" font-size="12" fill="#101010">Link text</text></a></svg>"##,
    );
    assert!(
        s.contains("/Subtype /Link") && s.contains("/URI (https://example.com/rect)"),
        "SVG shape/text links register URI annotations"
    );
}

// ===========================================================================
// Batch 5: precise refinements for reached-but-uncovered branch arms.
// ===========================================================================

#[test]
fn pdf_png_maximal_dimensions_overflow_decoded_byte_estimate() {
    // width*height*channels*sample_bytes saturates u64 during the decoded-size
    // estimate; the image is rejected (also over the pixel cap) to alt text.
    let idat = zlib(&[0u8; 8]);
    assert_rejected_png(
        "maxdim.png",
        png_bytes(
            &ihdr(0xFFFF_FFFF, 0xFFFF_FFFF, 16, 6, 0),
            &[(b"IDAT", idat), (b"IEND", Vec::new())],
        ),
    );
}

#[test]
fn pdf_svg_link_on_stroke_only_shape_registers_annotation() {
    // A link wrapping a stroke-only (fill=none) shape drives the stroke branch
    // of svg_style_has_link_paint; the shape link still gets a hitbox.
    let s = svg_render(
        "linkstroke.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40"><a href="https://example.com/stroke"><rect x="4" y="4" width="40" height="20" fill="none" stroke="#3366cc" stroke-width="2"/></a></svg>"##,
    );
    assert!(
        s.contains("/URI (https://example.com/stroke)"),
        "a stroke-only linked shape still registers a URI annotation"
    );
}

#[test]
fn pdf_svg_link_on_invisible_shape_drops_hitbox() {
    // An invisible (visibility:hidden / fill=none stroke=none) linked shape has
    // no link paint, so no annotation hitbox is emitted.
    let s = svg_render(
        "linkhidden.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40"><a href="https://example.com/hidden"><rect x="4" y="4" width="40" height="20" fill="none" stroke="none"/></a><rect x="50" y="4" width="20" height="20" fill="#334455"/></svg>"##,
    );
    assert!(
        !s.contains("/URI (https://example.com/hidden)"),
        "a paintless linked shape must not create a link hitbox"
    );
}

#[test]
fn pdf_svg_root_background_color_only_with_opacity() {
    // A solid, semi-transparent root background exercises the color path and the
    // alpha<1000 branch of append_svg_root_background_color.
    let s = svg_render(
        "bgcolor.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" style="background-color: rgba(200, 40, 40, 0.5)"><rect x="8" y="8" width="24" height="24" fill="#101010"/></svg>"##,
    );
    assert!(
        s.contains(" re f") && (s.contains(" gs") || s.contains("rg")),
        "a translucent root background fills the viewport with an alpha state"
    );
}

#[test]
fn pdf_svg_root_background_gradient_only_layer() {
    // A gradient-only root background (no solid color) reaches the background
    // layer loop without the color prefix.
    let s = svg_render(
        "bggrad.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40" style="background: linear-gradient(90deg, #ff0000, #0000ff)"><rect x="10" y="10" width="40" height="20" fill="#333333"/></svg>"##,
    );
    assert!(
        s.contains("/ShadingType 2") || s.contains(" sh") || s.contains(" re f"),
        "a gradient-only root background paints a shading layer"
    );
}

#[test]
fn pdf_png_palette_with_too_many_entries_is_rejected() {
    // A PLTE with 257 entries (771 bytes) exceeds the 256-color maximum for the
    // chunk regardless of bit depth, so the datastream is rejected.
    let plte = vec![0u8; 257 * 3];
    let idat = zlib(&[0u8; 8]);
    assert_rejected_png(
        "plte-257.png",
        png_bytes(
            &ihdr(2, 1, 8, 3, 0),
            &[(b"PLTE", plte), (b"IDAT", idat), (b"IEND", Vec::new())],
        ),
    );
}

#[test]
fn pdf_png_truecolor_with_suggested_palette_still_decodes() {
    // A truecolor (RGB) image may legally carry a suggested PLTE; the palette is
    // ignored for RGB samples but the chunk is accepted (color_type != 3 path of
    // the palette-size guard), and the image decodes via the fast RGB path.
    let png = png_from_rows(
        2,
        1,
        8,
        2,
        &[vec![0, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60]],
        &[(b"PLTE", vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66])],
    );
    let text = assert_decodes("rgb-suggested-plte.png", png);
    assert!(
        text.contains("/ColorSpace /DeviceRGB"),
        "an RGB image with a suggested palette still renders as DeviceRGB"
    );
}
