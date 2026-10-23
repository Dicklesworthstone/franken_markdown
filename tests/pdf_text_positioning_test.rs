//! Emitted-PDF regressions for retained GPOS mark positions. The small fixture
//! has independently recorded HarfBuzz coordinates, including mark-to-mark.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::{PdfEmitOptions, render_warnings, verification_text_layer};
use franken_markdown::{
    FontAssets, PdfImageAsset, PdfOptions, PdfRunningImage, PdfRunningImagePosition, parse_markdown,
};

const FIXTURE: &[u8] = include_bytes!("../fmd-font/fonts/test-shaping/FmdShaping.ttf");
const STACKED: &str = "a\u{0301}\u{0307}";

fn options() -> PdfOptions {
    PdfOptions {
        font_assets: FontAssets {
            body_regular: Some(FIXTURE.to_vec()),
            body_bold: Some(FIXTURE.to_vec()),
            body_italic: Some(FIXTURE.to_vec()),
            body_bold_italic: Some(FIXTURE.to_vec()),
            mono_regular: Some(FIXTURE.to_vec()),
            ..FontAssets::default()
        },
        base_font_size: Some(10.0),
        page_numbers: false,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

/// A valid variant whose combining marks have nonzero hmtx advances. GPOS
/// attachment makes them zero in admitted runs; literal fallback must retain
/// their real widths. This exposes admission changes hidden by zero-width
/// fixture marks. Recompute checksums so this remains a valid host font.
fn nonzero_mark_font(hyphen_alias: bool) -> Vec<u8> {
    let mut bytes = FIXTURE.to_vec();
    let u16_at = |bytes: &[u8], at| u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap());
    let u32_at = |bytes: &[u8], at| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    let record = |tag: &[u8]| {
        (0..usize::from(u16_at(&bytes, 4)))
            .map(|index| 12 + index * 16)
            .find(|&at| &bytes[at..at + 4] == tag)
            .unwrap()
    };
    let hmtx_record = record(b"hmtx");
    let hmtx = u32_at(&bytes, hmtx_record + 8) as usize;
    let hmtx_len = u32_at(&bytes, hmtx_record + 12) as usize;
    let head = u32_at(&bytes, record(b"head") + 8) as usize;
    let cmap_record = record(b"cmap");
    let cmap = u32_at(&bytes, cmap_record + 8) as usize;
    let cmap_len = u32_at(&bytes, cmap_record + 12) as usize;
    for id in [7, 8] {
        bytes[hmtx + id * 4..hmtx + id * 4 + 2].copy_from_slice(&500u16.to_be_bytes());
    }
    if hyphen_alias {
        // This fixture's two encoding records share a format-4 subtable.
        // Reuse its first (space) segment as a covered hyphen backed by the
        // existing f outline. The table repro contains no visible spaces.
        let subtable = cmap + u32_at(&bytes, cmap + 8) as usize;
        assert_eq!(u16_at(&bytes, subtable), 4);
        let count = usize::from(u16_at(&bytes, subtable + 6)) / 2;
        let end = subtable + 14;
        let start = end + 2 * count + 2;
        let delta = start + 2 * count;
        assert_eq!(u16_at(&bytes, end), 0x20);
        assert_eq!(u16_at(&bytes, start), 0x20);
        for at in [end, start] {
            bytes[at..at + 2].copy_from_slice(&0x2du16.to_be_bytes());
        }
        bytes[delta..delta + 2].copy_from_slice(&4u16.wrapping_sub(0x2d).to_be_bytes());
    }
    let checksum = |bytes: &[u8]| {
        bytes.chunks(4).fold(0u32, |sum, chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum.wrapping_add(u32::from_be_bytes(word))
        })
    };
    let sum = checksum(&bytes[hmtx..hmtx + hmtx_len]);
    bytes[hmtx_record + 4..hmtx_record + 8].copy_from_slice(&sum.to_be_bytes());
    let sum = checksum(&bytes[cmap..cmap + cmap_len]);
    bytes[cmap_record + 4..cmap_record + 8].copy_from_slice(&sum.to_be_bytes());
    bytes[head + 8..head + 12].fill(0);
    let adjustment = 0xb1b0_afbau32.wrapping_sub(checksum(&bytes));
    bytes[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    bytes
}

fn content(pdf: &[u8]) -> String {
    let mut all = String::new();
    for reference in String::from_utf8_lossy(pdf).split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let header = format!("\n{number} 0 obj\n");
        let offset = pdf
            .windows(header.len())
            .position(|x| x == header.as_bytes())
            .unwrap()
            + header.len();
        let object = &pdf[offset..];
        let start = object.windows(8).position(|x| x == b"\nstream\n").unwrap();
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
        let bytes = &object[start + 8..start + 8 + len];
        if dictionary.contains("/FlateDecode") {
            let decoded = franken_markdown::zlib_decompress(bytes, 32 * 1024 * 1024).unwrap();
            all.push_str(std::str::from_utf8(&decoded).unwrap());
        } else {
            all.push_str(std::str::from_utf8(bytes).unwrap());
        }
    }
    assert!(!all.is_empty());
    all
}

fn render(source: &str, opts: &PdfOptions) -> Vec<u8> {
    franken_markdown::render_pdf_document(&parse_markdown(source), opts).unwrap()
}

fn near(left: f32, right: f32) {
    assert!((left - right).abs() < 0.015, "{left} != {right}");
}

/// The subset glyph id whose ToUnicode entry has this exact UTF-16BE source.
fn unicode_id(pdf: &[u8], source: &str) -> u16 {
    let suffix = format!("> <{source}>");
    String::from_utf8_lossy(pdf)
        .lines()
        .find_map(|line| {
            let id = line.strip_suffix(&suffix)?.strip_prefix('<')?;
            u16::from_str_radix(id, 16).ok()
        })
        .unwrap_or_else(|| panic!("missing ToUnicode source {source}"))
}

/// Retained glyph matrices and subset ids from an emitted positioned text run.
fn glyphs(line: &str) -> Vec<([f32; 6], u16)> {
    let words: Vec<_> = line.split_whitespace().collect();
    words
        .iter()
        .enumerate()
        .filter(|(_, word)| **word == "Tm")
        .map(|(index, _)| {
            let matrix =
                std::array::from_fn(|component| words[index - 6 + component].parse().unwrap());
            let id = words[index + 1].trim_matches(['<', '>']);
            assert_eq!(words[index + 2], "Tj");
            (matrix, u16::from_str_radix(id, 16).unwrap())
        })
        .collect()
}

fn marked_draws(stream: &str) -> Vec<&str> {
    stream
        .lines()
        .filter(|line| line.contains("BT /F") && line.contains("> Tj"))
        .collect()
}

#[test]
fn emitted_mark_and_stacked_mark_positions_match_independent_harfbuzz_fixture() {
    let opts = options();
    // The saved oracle gives advance=500, mark offsets=(-350,500) and
    // (-350,650), in a 1000-unit em. A following b starts at x=500.
    for (source, expected) in [
        (
            "a\u{0301}b",
            vec![("0061", 0.0, 0.0), ("0301", 1.5, 5.0), ("0062", 5.0, 0.0)],
        ),
        (
            STACKED,
            vec![("0061", 0.0, 0.0), ("0301", 1.5, 5.0), ("0307", 1.5, 6.5)],
        ),
    ] {
        let pdf = render(source, &opts);
        let stream = content(&pdf);
        let draws = marked_draws(&stream);
        assert_eq!(draws.len(), 1);
        let actual = glyphs(draws[0]);
        assert_eq!(actual.len(), expected.len());
        let origin = actual[0].0;
        for ((matrix, id), (unicode, x, y)) in actual.iter().zip(expected) {
            assert_eq!(*id, unicode_id(&pdf, unicode));
            assert_ne!(*id, 0, "every positioned glyph survived subsetting");
            assert_eq!(&matrix[..4], &[1.0, 0.0, 0.0, 1.0]);
            near(matrix[4] - origin[4], x);
            near(matrix[5] - origin[5], y);
        }
        assert!(render_warnings(&parse_markdown(source), &opts).is_empty());
    }
}

#[test]
fn positioned_ligatures_keep_font_subset_mapping_and_exact_source_extraction() {
    let source = format!("fi{STACKED}b");
    let opts = options();
    let pdf = render(&source, &opts);
    let stream = content(&pdf);
    let draws = marked_draws(&stream);
    assert_eq!(draws.len(), 1);
    let actual = glyphs(draws[0]);
    assert_eq!(actual.len(), 5, "fi is one glyph, both marks stay present");
    assert_eq!(actual[0].1, unicode_id(&pdf, "00660069"));
    assert_eq!(actual[1].1, unicode_id(&pdf, "0061"));
    near(actual[1].0[4] - actual[0].0[4], 5.0);
    near(actual[3].0[4] - actual[0].0[4], 6.5);
    near(actual[4].0[4] - actual[0].0[4], 10.0);
    assert_eq!(
        stream
            .matches("/ActualText <FEFF006600690061030103070062>")
            .count(),
        1
    );
    let layer = verification_text_layer(&parse_markdown(&source), &opts).unwrap();
    assert_eq!(layer.pages[0].runs[0].text, source);
}

#[test]
fn repeated_positioned_content_is_stable_across_cache_hits_pages_and_emit_modes() {
    use franken_markdown::ast::{Block, Document, Inline};
    let opts = options();
    let mut blocks = Vec::new();
    for index in 0..3 {
        if index > 0 {
            blocks.push(Block::PageBreak);
        }
        blocks.push(Block::Paragraph(vec![Inline::Text(STACKED.into())]));
        blocks.push(Block::Paragraph(vec![Inline::Text(STACKED.into())]));
    }
    let doc = Document { blocks };
    let chunked =
        franken_markdown::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default())
            .unwrap();
    let monolithic =
        franken_markdown::render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic())
            .unwrap();
    assert_eq!(chunked, monolithic);
    assert_eq!(
        chunked,
        franken_markdown::render_pdf_document(&doc, &opts).unwrap()
    );
    let stream = content(&chunked);
    assert_eq!(stream.matches("/ActualText <FEFF006103010307>").count(), 6);
    let draws = marked_draws(&stream);
    assert_eq!(draws.len(), 6);
    for draw in draws {
        let actual = glyphs(draw);
        near(actual[2].0[4] - actual[0].0[4], 1.5);
        near(actual[2].0[5] - actual[0].0[5], 6.5);
    }
}

#[test]
fn tall_mark_ink_grows_lines_and_link_annotations_without_overflowing_pages() {
    let mut opts = options();
    opts.theme.page.size.height_pt = 240.0;
    opts.theme.page.margins.top_pt = 20.0;
    opts.theme.page.margins.bottom_pt = 20.0;
    let plain = verification_text_layer(&parse_markdown("a  \na"), &opts).unwrap();
    let marked =
        verification_text_layer(&parse_markdown(&format!("{STACKED}  \n{STACKED}")), &opts)
            .unwrap();
    let plain_gap = (plain.pages[0].runs[0].y - plain.pages[0].runs[1].y).abs();
    let marked_gap = (marked.pages[0].runs[0].y - marked.pages[0].runs[1].y).abs();
    assert!(
        marked_gap >= plain_gap + 0.49,
        "{plain_gap} -> {marked_gap}"
    );
    let source = format!(
        "# {STACKED}\n\n[{STACKED}](https://example.com)\n\n```\n{STACKED}\n{STACKED}\n```\n\n| {STACKED} |\n| --- |\n| {STACKED} |\n\n> {STACKED}\n"
    );
    let doc = parse_markdown(&source);
    let layer = verification_text_layer(&doc, &opts).unwrap();
    for run in layer.pages.iter().flat_map(|page| &page.runs) {
        assert!(run.overshoot.is_none(), "{:?}", run);
        if run.text.contains(STACKED) {
            assert!(run.y + run.size * 1.05 <= 220.015, "{:?}", run);
            assert!(run.y >= 19.985, "{:?}", run);
        }
    }
    let pdf = render(&source, &opts);
    let raw = String::from_utf8_lossy(&pdf);
    let annotation = raw.split("/Subtype /Link ").nth(1).unwrap();
    let coordinates = annotation
        .split_once("/Rect [")
        .unwrap()
        .1
        .split(']')
        .next()
        .unwrap();
    let rect: Vec<f32> = coordinates
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    assert!(
        rect[3] - rect[1] >= 13.29,
        "the 1.05-em stack is inside {rect:?}"
    );
}

#[test]
fn long_marked_paragraphs_and_wrapped_code_keep_clusters_and_source() {
    let mut opts = options();
    opts.theme.page.size.width_pt = 180.0;
    opts.theme.page.margins.left_pt = 20.0;
    opts.theme.page.margins.right_pt = 20.0;
    let prose = std::iter::repeat_n(format!("{STACKED}b"), 1050)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(prose.chars().count() > 4096);
    let doc = parse_markdown(&prose);
    assert!(render_warnings(&doc, &opts).is_empty());
    let layer = verification_text_layer(&doc, &opts).unwrap();
    let joined: String = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .flat_map(|run| run.text.chars().filter(|ch| !ch.is_whitespace()))
        .collect();
    assert_eq!(
        joined,
        prose
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
    );
    let pdf = franken_markdown::render_pdf_document(&doc, &opts).unwrap();
    let stream = content(&pdf);
    assert!(!marked_draws(&stream).is_empty());
    for draw in marked_draws(&stream) {
        assert!(glyphs(draw).iter().all(|(_, id)| *id != 0));
    }
    let code = STACKED.repeat(100);
    let markdown = format!("```\n{code}\n```\n\n| a | b |\n| --- | --- |\n| {code} | {code} |\n");
    let layer = verification_text_layer(&parse_markdown(&markdown), &opts).unwrap();
    let mut joined_code = String::new();
    for run in layer.pages.iter().flat_map(|page| &page.runs) {
        assert!(run.overshoot.is_none(), "{:?}", run);
        assert!(
            !run.text
                .chars()
                .next()
                .is_some_and(|ch| matches!(ch as u32, 0x0300..=0x036f))
        );
        if run.kind == "code" {
            joined_code.push_str(&run.text);
        }
    }
    assert_eq!(joined_code, code);
    render(&markdown, &opts);
}

#[test]
fn unsupported_and_excessive_marks_have_stable_deduplicated_verify_findings() {
    let opts = options();
    for (word, code) in [
        ("a\u{034f}".to_owned(), "text_shaping_unsupported"),
        (format!("a{}", "\u{0301}".repeat(65)), "text_shaping_limit"),
    ] {
        let source = format!("{word}\n\n{STACKED} {word} {STACKED}");
        let doc = parse_markdown(&source);
        let warnings = render_warnings(&doc, &opts);
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.code() == code)
                .count(),
            1
        );
        let report = franken_markdown::verify::verify_pdf(&doc, &opts).unwrap();
        let findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.code == code)
            .collect();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("Latin mark positioning"));
        assert!(franken_markdown::verify::to_json(&report).contains(code));
        let text: String = report
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .map(|run| run.text.as_str())
            .collect();
        assert!(text.contains(&word));
        let pdf = render(&source, &opts);
        let stream = content(&pdf);
        assert!(
            !marked_draws(&stream).is_empty(),
            "supported neighbours remain positioned"
        );
    }
}

#[test]
fn embedded_svg_keeps_mark_offsets_through_rotation_and_stroke_replay() {
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    for (attributes, coefficients) in [
        ("transform='rotate(90 30 30)'", [0.0, -1.0, 1.0, 0.0]),
        (
            "transform='rotate(45 30 30)'",
            [diagonal, -diagonal, diagonal, diagonal],
        ),
        (
            "textLength='5.075' lengthAdjust='spacingAndGlyphs'",
            [1.015, 0.0, 0.0, 1.0],
        ),
    ] {
        let mut opts = options();
        opts.image_assets.push(PdfImageAsset {
            destination: "marked.svg".into(),
            bytes: format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'><text x='30' y='30' {attributes} font-size='10' fill='#123456' stroke='#654321' stroke-width='1' paint-order='stroke fill'>{STACKED}</text></svg>"
            ).into_bytes(),
        });
        let source = "![a](marked.svg)";
        assert!(render_warnings(&parse_markdown(source), &opts).is_empty());
        let pdf = render(source, &opts);
        let stream = content(&pdf);
        let draws = marked_draws(&stream);
        assert_eq!(draws.len(), 2);
        assert_eq!(stream.matches("/ActualText <FEFF006103010307>").count(), 1);
        let expected_ids = [
            unicode_id(&pdf, "0061"),
            unicode_id(&pdf, "0301"),
            unicode_id(&pdf, "0307"),
        ];
        for draw in draws {
            let actual = glyphs(draw);
            assert_eq!(
                actual.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
                expected_ids
            );
            // Dimensionless coefficients retain six decimal places. A 1.5%
            // glyph expansion must remain 1.015, and a 45-degree rotation must
            // retain its diagonal rather than round 0.707107 to 0.71.
            for (matrix, _) in &actual {
                for (value, expected) in matrix[..4].iter().zip(coefficients) {
                    assert!(
                        (value - expected).abs() <= 0.000_001,
                        "{attributes}: {value} != {expected}; {draw}"
                    );
                }
            }
            let [a, b, c, d, x, y] = actual[0].0;
            let size: f32 = draw
                .split_once(" Tf ")
                .unwrap()
                .0
                .split_whitespace()
                .next_back()
                .unwrap()
                .parse()
                .unwrap();
            near(actual[2].0[4] - x, a * size * 0.15 + c * size * 0.65);
            near(actual[2].0[5] - y, b * size * 0.15 + d * size * 0.65);
        }
    }
}

#[test]
fn svg_spacing_and_text_length_keep_marks_with_their_ligature_aware_cluster() {
    let source = format!("fi{STACKED}b");
    let mut origins = Vec::new();
    for (attributes, positions, origin_shift) in [
        ("", [0.0, 0.5, 0.65, 0.65, 1.0], 0.0),
        ("letter-spacing='2'", [0.0, 0.7, 0.85, 0.85, 1.4], -0.2),
        (
            "textLength='25' lengthAdjust='spacing'",
            [0.0, 1.0, 1.15, 1.15, 2.0],
            -0.5,
        ),
    ] {
        let mut opts = options();
        opts.image_assets.push(PdfImageAsset {
            destination: "spaced.svg".into(),
            bytes: format!("<svg xmlns='http://www.w3.org/2000/svg' width='100' height='50'><text x='50' y='30' text-anchor='middle' font-size='10' {attributes}>{source}</text></svg>").into_bytes(),
        });
        let pdf = render("![a](spaced.svg)", &opts);
        let stream = content(&pdf);
        let draws = marked_draws(&stream);
        assert_eq!(draws.len(), 1);
        let actual = glyphs(draws[0]);
        assert_eq!(actual.len(), 5, "ligature plus base, marks and next base");
        assert_eq!(actual[0].1, unicode_id(&pdf, "00660069"));
        let size: f32 = draws[0]
            .split_once(" Tf ")
            .unwrap()
            .0
            .split_whitespace()
            .next_back()
            .unwrap()
            .parse()
            .unwrap();
        let origin = actual[0].0[4];
        origins.push(origin);
        near((origin - origins[0]) / size, origin_shift);
        for ((matrix, _), x) in actual.iter().zip(positions) {
            near((matrix[4] - origin) / size, x);
        }
        near((actual[3].0[5] - actual[1].0[5]) / size, 0.65);
        assert_eq!(
            stream
                .matches("/ActualText <FEFF006600690061030103070062>")
                .count(),
            1
        );
    }
    // The space is its own cluster: word spacing and letter spacing add once,
    // while the two marks retain their base-relative offsets.
    let mut opts = options();
    opts.image_assets.push(PdfImageAsset {
        destination: "words.svg".into(),
        bytes: format!("<svg xmlns='http://www.w3.org/2000/svg' width='100' height='50'><text x='10' y='30' font-size='10' letter-spacing='2' word-spacing='3'>fi{STACKED} b</text></svg>").into_bytes(),
    });
    let pdf = render("![a](words.svg)", &opts);
    let stream = content(&pdf);
    let draws = marked_draws(&stream);
    assert_eq!(draws.len(), 1);
    let actual = glyphs(draws[0]);
    assert_eq!(actual.len(), 6);
    let size: f32 = draws[0]
        .split_once(" Tf ")
        .unwrap()
        .0
        .split_whitespace()
        .next_back()
        .unwrap()
        .parse()
        .unwrap();
    let origin = actual[0].0[4];
    for ((matrix, _), x) in actual.iter().zip([0.0, 0.7, 0.85, 0.85, 1.4, 2.2]) {
        near((matrix[4] - origin) / size, x);
    }
}

#[test]
fn emergency_word_pieces_are_refitted_when_joining_changes_strict_admission() {
    let font = nonzero_mark_font(false);
    let mut opts = options();
    opts.font_assets = FontAssets {
        body_regular: Some(font.clone()),
        body_bold: Some(font.clone()),
        body_italic: Some(font.clone()),
        body_bold_italic: Some(font.clone()),
        mono_regular: Some(font),
        ..FontAssets::default()
    };
    opts.theme.page.size.width_pt = 150.0;
    opts.theme.page.size.height_pt = 300.0;
    opts.theme.page.margins.left_pt = 20.0;
    opts.theme.page.margins.right_pt = 20.0;
    opts.theme.page.margins.top_pt = 20.0;
    opts.theme.page.margins.bottom_pt = 20.0;
    let word = format!("{STACKED}{}\u{034f}", "b".repeat(20));
    for optimal in [false, true] {
        opts.optimal_pagination = optimal;
        for (source, marker) in [
            (word.clone(), ""),
            (format!("[{word}](https://example.com)"), ""),
            (format!("### {word}"), ""),
            (format!("- {word}"), "•"),
        ] {
            let doc = parse_markdown(&source);
            let layer = verification_text_layer(&doc, &opts).unwrap();
            let rows: Vec<_> = layer
                .pages
                .iter()
                .flat_map(|page| &page.runs)
                .filter(|run| !run.text.is_empty())
                .collect();
            assert!(
                rows.len() >= 2,
                "the final 116.2pt fallback cannot fit a 110pt measure"
            );
            assert!(rows.iter().all(|run| run.overshoot.is_none()), "{rows:?}");
            let joined: String = rows.iter().map(|run| run.text.as_str()).collect();
            assert_eq!(joined.strip_prefix(marker).unwrap(), word);
            let pdf = franken_markdown::render_pdf_document(&doc, &opts).unwrap();
            assert!(
                !marked_draws(&content(&pdf)).is_empty(),
                "supported wrapped prefix stays positioned"
            );
        }
    }
}

#[test]
fn table_separator_pieces_keep_complete_word_admission_and_fit_the_cell() {
    let font = nonzero_mark_font(true);
    let mut opts = options();
    opts.font_assets = FontAssets {
        body_regular: Some(font.clone()),
        body_bold: Some(font),
        ..FontAssets::default()
    };
    opts.table_font_size = Some(10.0);
    opts.theme.page.size.width_pt = 150.0;
    opts.theme.page.size.height_pt = 300.0;
    opts.theme.page.margins.left_pt = 20.0;
    opts.theme.page.margins.right_pt = 20.0;
    opts.theme.page.margins.top_pt = 20.0;
    opts.theme.page.margins.bottom_pt = 20.0;
    let word = format!("{STACKED}-{}\u{034f}", "b".repeat(16));
    let source = format!("| a |\n| --- |\n| {word} |\n");
    let doc = parse_markdown(&source);
    let layer = verification_text_layer(&doc, &opts).unwrap();
    let rows: Vec<_> = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| run.kind == "table_row")
        .collect();
    let joined: String = rows.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(joined, word);
    assert!(
        rows.len() > 1 || rows[0].size < 9.5,
        "the allocator must see the complete fallback width before fitting"
    );
    let pdf = franken_markdown::render_pdf_document(&doc, &opts).unwrap();
    let stream = content(&pdf);
    let draws: Vec<_> = stream
        .lines()
        .filter(|line| line.starts_with("BT /F1 "))
        .collect();
    assert_eq!(draws.len(), rows.len());
    for draw in draws {
        // Every glyph used in this adversarial fallback word has a 500-unit
        // hmtx advance. Read the actual emitted TJ adjustments independently
        // of the layout's width cache and check the cell's symmetric padding.
        let (prefix, array) = draw.split_once(" Tm [").unwrap();
        let words: Vec<_> = prefix.split_whitespace().collect();
        let size: f32 = words[2].parse().unwrap();
        let x: f32 = words[words.len() - 2].parse().unwrap();
        let mut rest = array.split_once(']').unwrap().0;
        let mut advance = 0.0_f32;
        while !rest.is_empty() {
            if let Some(hex) = rest.strip_prefix('<') {
                let (glyphs, tail) = hex.split_once('>').unwrap();
                advance += (glyphs.len() / 4) as f32 * 500.0;
                rest = tail;
            } else {
                let end = rest.find('<').unwrap_or(rest.len());
                advance -= rest[..end].trim().parse::<f32>().unwrap();
                rest = &rest[end..];
            }
        }
        let right = x + advance * size / 1000.0;
        assert!(right <= 150.0 - x + 0.02, "{draw}: right={right}");
    }
}

#[test]
fn running_footer_rule_clears_positioned_ink_and_rejects_tight_margins() {
    const FOOTER: &str = "/Artifact <</Type /Pagination /Subtype /Footer>> BDC\n";
    let mut opts = options();
    opts.theme.page.margins.bottom_pt = 30.0;
    opts.running.footer.center = Some(STACKED.into());
    opts.running.footer.rule = true;
    let pdf = render("b", &opts);
    let stream = content(&pdf);
    let footer = stream.split_once(FOOTER).unwrap().1;
    let draws = marked_draws(footer);
    assert_eq!(draws.len(), 1);
    let actual = glyphs(draws[0]);
    let size: f32 = draws[0]
        .split_once(" Tf ")
        .unwrap()
        .0
        .split_whitespace()
        .next_back()
        .unwrap()
        .parse()
        .unwrap();
    let rule = footer.lines().find(|line| line.ends_with(" l S")).unwrap();
    let words: Vec<_> = rule.split_whitespace().collect();
    let moveto = words.iter().position(|word| *word == "m").unwrap();
    let rule_y: f32 = words[moveto - 1].parse().unwrap();
    near(rule_y, actual[0].0[5] + size * 1.05 + 3.0);
    assert!(rule_y + 0.25 <= 30.0);
    // The old global ascent permits this 29pt band, but the real attached ink
    // and rule need 29.84pt. It must fail rather than enter the body margin.
    opts.theme.page.margins.bottom_pt = 29.0;
    let error = franken_markdown::render_pdf_document(&parse_markdown("b"), &opts)
        .unwrap_err()
        .to_string();
    assert!(error.contains("pdf_running_footer_does_not_fit"), "{error}");
    opts.running.footer.center = Some("a".into());
    assert!(franken_markdown::render_pdf_document(&parse_markdown("b"), &opts).is_ok());
    opts.running.footer.center = Some(STACKED.into());
    opts.running.skip_first_page = true;
    assert!(franken_markdown::render_pdf_document(&parse_markdown("b"), &opts).is_ok());

    let mut header = options();
    header.theme.page.margins.top_pt = 30.0;
    header.running.header.left = Some(format!("a\u{0301}{}", "\u{0307}".repeat(20)));
    assert!(render_warnings(&parse_markdown("b"), &header).is_empty());
    let error = franken_markdown::render_pdf_document(&parse_markdown("b"), &header)
        .unwrap_err()
        .to_string();
    assert!(error.contains("pdf_running_header_does_not_fit"), "{error}");
    header.running.header.left = Some("a".into());
    assert!(franken_markdown::render_pdf_document(&parse_markdown("b"), &header).is_ok());
}

#[test]
fn running_sources_and_logo_text_report_positioning_fallback_findings() {
    let doc = parse_markdown("b");
    for (source, code) in [
        ("a\u{034f}".to_owned(), "text_shaping_unsupported"),
        (format!("a{}", "\u{0301}".repeat(65)), "text_shaping_limit"),
    ] {
        let mut opts = options();
        opts.title = Some(source.clone());
        opts.running.header.left = Some("{title}".into());
        opts.running.footer.right = Some(source);
        let warnings = render_warnings(&doc, &opts);
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.code() == code)
                .count(),
            1,
            "running findings deduplicate across resolved templates"
        );
        let report = franken_markdown::verify::verify_pdf(&doc, &opts).unwrap();
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.code == code)
                .count(),
            1
        );
    }
    let mut opts = options();
    opts.image_assets.push(PdfImageAsset {
        destination: "logo.svg".into(),
        bytes: "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='50'><text x='10' y='30' font-size='10'>a\u{034f}</text></svg>".as_bytes().to_vec(),
    });
    opts.running.header.image = Some(PdfRunningImage {
        dest: "logo.svg".into(),
        position: PdfRunningImagePosition::Left,
        height_pt: Some(20),
    });
    assert_eq!(
        render_warnings(&doc, &opts)
            .iter()
            .filter(|warning| warning.code() == "text_shaping_unsupported")
            .count(),
        1,
        "a logo need not also occur in body Markdown to receive diagnostics"
    );
    assert!(franken_markdown::render_pdf_document(&doc, &opts).is_ok());
}
