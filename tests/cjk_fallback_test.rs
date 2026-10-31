//! Real CJK fallback: inspect emitted glyph IDs, embedded font programs, and
//! ToUnicode recovery rather than accepting successful layout of .notdef boxes.
#![cfg(feature = "cjk-face")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::fonts::{cjk_bytes, load_cjk};
use franken_markdown::pdf::{PdfEmitOptions, verification_text_layer};
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::text::Font;
use franken_markdown::theme::FontFamily;
use franken_markdown::{
    PdfOptions, PdfRunningBand, PdfRunningContent, RenderWarning, parse_markdown, render_pdf,
    render_pdf_document_emitted, render_warnings,
};

fn options() -> PdfOptions {
    PdfOptions {
        page_numbers: false,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

/// Follow xref offsets so binary font bytes cannot masquerade as PDF objects.
fn objects(pdf: &[u8]) -> BTreeMap<usize, &[u8]> {
    let marker = b"startxref\n";
    let at = pdf
        .windows(marker.len())
        .rposition(|s| s == marker)
        .unwrap();
    let xref: usize = std::str::from_utf8(&pdf[at + marker.len()..])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[xref..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines
        .next()
        .unwrap()
        .strip_prefix("0 ")
        .unwrap()
        .parse()
        .unwrap();
    let mut offsets: Vec<_> = lines
        .take(count)
        .enumerate()
        .filter_map(|(id, line)| {
            let mut words = line.split_whitespace();
            let offset = words.next().unwrap().parse::<usize>().unwrap();
            words.next().unwrap();
            (words.next() == Some("n")).then_some((offset, id))
        })
        .collect();
    offsets.sort_unstable();
    offsets
        .iter()
        .enumerate()
        .map(|(index, &(offset, id))| {
            let end = offsets.get(index + 1).map_or(xref, |entry| entry.0);
            let object = &pdf[offset..end];
            let header = format!("{id} 0 obj\n");
            assert!(object.starts_with(header.as_bytes()));
            (id, &object[header.len()..])
        })
        .collect()
}

fn dictionary(object: &[u8]) -> &str {
    let end = object.iter().position(|&byte| byte == b'\n').unwrap();
    std::str::from_utf8(&object[..end]).unwrap()
}

fn reference(dictionary: &str, key: &str) -> usize {
    dictionary
        .split_once(key)
        .unwrap()
        .1
        .trim_start_matches([' ', '['])
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn stream(object: &[u8]) -> Vec<u8> {
    let dict = dictionary(object);
    let length = reference(dict, "/Length ");
    let marker = b"\nstream\n";
    let start = object
        .windows(marker.len())
        .position(|s| s == marker)
        .unwrap()
        + marker.len();
    let bytes = &object[start..start + length];
    if dict.contains("/FlateDecode") {
        franken_markdown::zlib_decompress(bytes, 16 * 1024 * 1024).unwrap()
    } else {
        bytes.to_vec()
    }
}

fn hex_units(text: &str) -> Vec<u16> {
    assert_eq!(text.len() % 4, 0);
    text.as_bytes()
        .chunks_exact(4)
        .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
        .collect()
}

fn unicode_map(cmap: &[u8]) -> BTreeMap<u16, String> {
    let mut result = BTreeMap::new();
    for block in std::str::from_utf8(cmap)
        .unwrap()
        .split("beginbfchar\n")
        .skip(1)
    {
        for line in block.split_once("endbfchar").unwrap().0.lines() {
            let (cid, unicode) = line.split_once("> <").unwrap();
            let cid = u16::from_str_radix(cid.trim_start_matches('<'), 16).unwrap();
            let text = String::from_utf16(&hex_units(unicode.trim_end_matches('>'))).unwrap();
            assert!(result.insert(cid, text).is_none());
        }
    }
    result
}

fn font_refs(page: &str) -> BTreeMap<&str, usize> {
    let fields: Vec<_> = page
        .split_once("/Font <<")
        .unwrap()
        .1
        .split_once(">>")
        .unwrap()
        .0
        .split_whitespace()
        .collect();
    assert_eq!(fields.len() % 4, 0);
    fields
        .chunks_exact(4)
        .map(|part| {
            assert_eq!(&part[2..], &["0", "R"]);
            (part[0], part[1].parse().unwrap())
        })
        .collect()
}

/// Decode the glyphs actually painted through each page's font resources.
/// All fixtures avoid synthetic hyphens and composed-mark ActualText spans.
fn painted_text(pdf: &[u8]) -> String {
    let objects = objects(pdf);
    let mut text = String::new();
    for page in objects
        .values()
        .map(|object| dictionary(object))
        .filter(|dict| dict.starts_with("<< /Type /Page "))
    {
        let fonts: BTreeMap<_, _> = font_refs(page)
            .into_iter()
            .map(|(slot, id)| {
                let font = dictionary(objects[&id]);
                (
                    slot,
                    unicode_map(&stream(objects[&reference(font, "/ToUnicode ")])),
                )
            })
            .collect();
        let bytes = stream(objects[&reference(page, "/Contents ")]);
        let content = std::str::from_utf8(&bytes).unwrap();
        for run in content.split("BT ").skip(1) {
            let run = run.split_once(" ET").unwrap().0;
            let words: Vec<_> = run.split_whitespace().collect();
            let tf = words.iter().position(|&word| word == "Tf").unwrap();
            let cmap = &fonts[words[tf - 2]];
            for encoded in run.split('<').skip(1) {
                for cid in hex_units(encoded.split_once('>').unwrap().0) {
                    assert_ne!(cid, 0, "painted text must never use .notdef");
                    text.push_str(cmap.get(&cid).expect("painted glyph has a ToUnicode entry"));
                }
            }
        }
    }
    text
}

fn missing(source: &str, opts: &PdfOptions) -> Vec<RenderWarning> {
    render_warnings(&parse_markdown(source), opts)
        .into_iter()
        .filter(|warning| matches!(warning, RenderWarning::MissingGlyphs { .. }))
        .collect()
}

#[test]
fn bundled_cjk_maps_real_han_kana_and_punctuation_outlines() {
    let font = load_cjk().expect("cjk-face bundles a parseable font");
    assert!(font.has_glyf_outlines());
    for ch in "中文日本語かなカナ龍齉龘鬱檸檬麒麟「」【】。、！？々〆〇ー".chars()
    {
        let glyph = font.glyph_index(ch);
        assert_ne!(glyph, 0, "missing {ch:?} U+{:04X}", ch as u32);
        assert!(
            font.glyph_bbox(glyph).is_some(),
            "{ch:?} must have real ink"
        );
    }
    assert_eq!(
        font.glyph_index('한'),
        0,
        "SC fallback does not claim Hangul coverage"
    );
}

#[test]
fn mixed_cjk_pdf_paints_subset_glyphs_and_recovers_exact_source() {
    let source = "Latin 中文、日本語かなカナ。";
    let opts = options();
    assert!(missing(source, &opts).is_empty());
    let pdf = render_pdf(source, &opts).unwrap();
    assert_eq!(painted_text(&pdf), source);
    let objects = objects(&pdf);
    let page = objects
        .values()
        .map(|object| dictionary(object))
        .find(|dict| dict.starts_with("<< /Type /Page "))
        .unwrap();
    let refs = font_refs(page);
    assert!(refs.contains_key("/F1"), "Latin keeps its primary face");
    let type0 = dictionary(objects[&refs["/F11"]]);
    let cid = dictionary(objects[&reference(type0, "/DescendantFonts ")]);
    assert!(cid.contains("/CIDToGIDMap /Identity"));
    let descriptor = dictionary(objects[&reference(cid, "/FontDescriptor ")]);
    let bytes = stream(objects[&reference(descriptor, "/FontFile2 ")]);
    assert!(
        bytes.len() < cjk_bytes().unwrap().len() / 100,
        "embed only used glyphs"
    );
    let subset = Font::parse(bytes).unwrap();
    let unicode = unicode_map(&stream(objects[&reference(type0, "/ToUnicode ")]));
    for ch in "中文日本語かなカナ。、".chars() {
        let glyph = subset.glyph_index(ch);
        assert_ne!(glyph, 0, "{ch:?} survives subsetting");
        assert_eq!(unicode[&glyph], ch.to_string());
        assert!(subset.glyph_bbox(glyph).is_some());
    }
}

#[test]
fn cjk_fallback_reaches_styles_tables_code_links_and_running_text() {
    let source = "# 中文标题\n\nEnglish **中文粗体** and *日本語* with [リンク](https://example.com) and `代码`.\n\n| 项目 | 値 |\n| --- | --- |\n| 内容 | 確認 |\n\n```text\n中文 = 日本語\n```\n";
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let mut opts = options();
        opts.theme.font = family;
        opts.running = PdfRunningContent {
            header: PdfRunningBand {
                left: Some("文档".into()),
                ..PdfRunningBand::default()
            },
            footer: PdfRunningBand {
                right: Some("頁 {page}".into()),
                ..PdfRunningBand::default()
            },
            ..PdfRunningContent::default()
        };
        assert!(missing(source, &opts).is_empty());
        let pdf = render_pdf(source, &opts).unwrap();
        let text = painted_text(&pdf);
        for expected in [
            "中文标题",
            "中文粗体",
            "日本語",
            "リンク",
            "代码",
            "项目",
            "値",
            "内容",
            "確認",
            "中文 = 日本語",
            "文档",
            "頁 1",
        ] {
            assert!(
                text.contains(expected),
                "{family:?} lost {expected:?}: {text:?}"
            );
        }
        assert!(String::from_utf8_lossy(&pdf).contains("/URI (https://example.com)"));
    }
}

#[test]
fn cjk_lines_keep_complete_text_inside_the_page_and_are_deterministic() {
    let source = "中文日本語かなカナ。「混合排版」".repeat(24);
    let mut opts = options();
    opts.theme.page.size.width_pt = 220.0;
    opts.theme.page.size.height_pt = 240.0;
    opts.theme.page.margins = franken_markdown::PageMargins {
        top_pt: 30.0,
        right_pt: 30.0,
        bottom_pt: 30.0,
        left_pt: 30.0,
    };
    let doc = parse_markdown(&source);
    let pdf = render_pdf(&source, &opts).unwrap();
    assert_eq!(painted_text(&pdf), source);
    assert_eq!(pdf, render_pdf(&source, &opts).unwrap());
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap()
    );
    let layer = verification_text_layer(&doc, &opts).unwrap();
    assert!(layer.page_count > 1);
    assert!(
        layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .all(|run| run.overshoot.is_none())
    );
}

#[test]
fn ascii_documents_never_embed_the_cjk_font() {
    let pdf = render_pdf(
        "# Plain ASCII\n\nKeep original body, **bold**, *italic*, and `code` fonts.",
        &options(),
    )
    .unwrap();
    let objects = objects(&pdf);
    for page in objects
        .values()
        .map(|object| dictionary(object))
        .filter(|dict| dict.starts_with("<< /Type /Page "))
    {
        assert!(!font_refs(page).contains_key("/F11"));
    }
    assert!(
        pdf.len() < 80_000,
        "unused CJK font bytes must not enter the PDF"
    );
}

#[test]
fn uncovered_scripts_keep_precise_missing_glyph_warnings() {
    let warnings = missing("中文 한글 \u{13000}", &options());
    assert_eq!(warnings.len(), 1);
    assert!(
        matches!(&warnings[0], RenderWarning::MissingGlyphs { count: 3, sample }
        if sample.contains('한') && sample.contains('글') && sample.contains('\u{13000}'))
    );
}

#[test]
fn cjk_svg_has_real_outlines_and_no_missing_characters() {
    let source = "中文日本語かなカナ「確認」。";
    let doc = parse_markdown(source);
    let (bytes, report, warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
    assert_eq!(report.glyphs_missing, 0);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(report.glyphs_drawn, source.chars().count());
    assert!(report.paths_emitted >= 10);
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("<path ") && svg.contains("<use "));
}

#[cfg(feature = "emoji-face")]
#[test]
fn cjk_and_emoji_keep_independent_font_resources() {
    let source = "中文 🚀 日本語 ✅";
    let opts = options();
    assert!(missing(source, &opts).is_empty());
    let pdf = render_pdf(source, &opts).unwrap();
    assert_eq!(painted_text(&pdf), source);
    let objects = objects(&pdf);
    let page = objects
        .values()
        .map(|object| dictionary(object))
        .find(|dict| dict.starts_with("<< /Type /Page "))
        .unwrap();
    let refs = font_refs(page);
    assert_ne!(
        refs["/F7"], refs["/F11"],
        "existing emoji slot remains distinct from CJK"
    );
    let (_, report, warnings) =
        render_svg_with_diagnostics(&parse_markdown(source), &SvgOptions::default());
    assert_eq!(report.glyphs_missing, 0);
    assert!(warnings.is_empty(), "{warnings:?}");
}
