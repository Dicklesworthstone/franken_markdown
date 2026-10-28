//! Exact PDF planning keeps short captions and list introductions with a legal
//! first subject fragment. Decode actual page/font references and painted text,
//! independently of layout, planner internals, and the verification text layer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pagination::height::HeightPaginationError;
use franken_markdown::{
    Block, PdfEmitOptions, PdfImageAsset, PdfOptions, PdfPaginationFailure, RenderError,
    parse_markdown, render_pdf, render_pdf_document_reported, render_pdf_reported, render_warnings,
};
use std::collections::BTreeMap;

struct Objects<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
    xref: usize,
}

impl<'a> Objects<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|part| part == marker)
            .unwrap();
        let xref = std::str::from_utf8(&bytes[at + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(section[0], "0");
        let count: usize = section[1].parse().unwrap();
        let offsets = lines
            .take(count)
            .map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields[2] == "n" {
                    fields[0].parse().unwrap()
                } else {
                    0
                }
            })
            .collect();
        Self {
            bytes,
            offsets,
            xref,
        }
    }

    fn body(&self, id: usize) -> &'a [u8] {
        let start = self.offsets[id];
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&at| at > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        let object = &self.bytes[start..end];
        assert!(
            object.starts_with(header.as_bytes()),
            "xref must point to the actual object"
        );
        &object[header.len()..]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let object = self.body(id);
        let end = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap_or(object.len());
        std::str::from_utf8(&object[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let object = self.body(id);
        let at = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..at]).unwrap();
        let length = integer_after(dictionary, "/Length ");
        let payload = &object[at + 8..at + 8 + length];
        let bytes = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(
                !dictionary.contains("/Filter"),
                "extend the decoder for a new stream filter"
            );
            payload.to_vec()
        };
        String::from_utf8(bytes).unwrap()
    }
}

fn integer_after(dictionary: &str, key: &str) -> usize {
    dictionary
        .split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn unicode_map(objects: &Objects<'_>, font: usize) -> BTreeMap<u16, String> {
    let cmap = objects.stream(integer_after(objects.dictionary(font), "/ToUnicode "));
    let mut chars = false;
    let mut map = BTreeMap::new();
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            chars = true;
            continue;
        }
        if line == "endbfchar" {
            chars = false;
            continue;
        }
        if !chars {
            continue;
        }
        let entries: Vec<_> = line.split_whitespace().collect();
        let id = u16::from_str_radix(entries[0].trim_matches(['<', '>']), 16).unwrap();
        let encoded = entries[1].trim_matches(['<', '>']);
        let units = encoded
            .as_bytes()
            .chunks_exact(4)
            .map(|part| u16::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        map.insert(id, String::from_utf16(&units).unwrap());
    }
    assert!(
        !map.is_empty(),
        "painted font must have a selectable glyph map"
    );
    map
}

fn painted_pages(pdf: &[u8]) -> Vec<String> {
    let objects = Objects::new(pdf);
    let page_tree = (1..objects.offsets.len())
        .find(|&id| objects.dictionary(id).contains("/Type /Pages "))
        .unwrap();
    let kids = objects
        .dictionary(page_tree)
        .split_once("/Kids [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(kids.len() % 3, 0);
    let mut page_text = Vec::new();
    for kid in kids.chunks_exact(3) {
        assert_eq!(&kid[1..], &["0", "R"]);
        let page = objects.dictionary(kid[0].parse().unwrap());
        let resources = page
            .split_once("/Font <<")
            .unwrap()
            .1
            .split_once(">>")
            .unwrap()
            .0
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(resources.len() % 4, 0);
        let fonts = resources
            .chunks_exact(4)
            .map(|entry| (entry[0], unicode_map(&objects, entry[1].parse().unwrap())))
            .collect::<BTreeMap<_, _>>();
        let content = objects.stream(integer_after(page, "/Contents "));
        let mut painted = String::new();
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let separated = line
                .replace('[', " [ ")
                .replace(']', " ] ")
                .replace('<', " <")
                .replace('>', "> ");
            let words: Vec<_> = separated.split_whitespace().collect();
            let mut font = None;
            let mut mode = 0;
            let mut text = String::new();
            for (index, word) in words.iter().enumerate() {
                match *word {
                    "Tf" => font = Some(&fonts[words[index - 2]]),
                    "Tr" => mode = words[index - 1].parse::<u8>().unwrap(),
                    "Tj" | "TJ" => {
                        if mode != 3 {
                            painted.push_str(&text);
                        }
                        text.clear();
                    }
                    hex if hex.starts_with('<') && hex.ends_with('>') => {
                        let font = font.expect("painted glyph needs an active font");
                        let hex = hex.trim_matches(['<', '>']);
                        assert_eq!(hex.len() % 4, 0);
                        for glyph in hex.as_bytes().chunks_exact(4) {
                            let id = u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16)
                                .unwrap();
                            text.push_str(
                                font.get(&id)
                                    .expect("painted glyph must have a Unicode mapping"),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        page_text.push(painted.chars().filter(|c| !c.is_whitespace()).collect());
    }
    page_text
}

fn options(content_height: f32, figure_height: u32) -> PdfOptions {
    let mut options = PdfOptions {
        strict_pagination: true,
        image_assets: vec![PdfImageAsset::new(
            "figure.svg",
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"80\" height=\"{figure_height}\" \
                 viewBox=\"0 0 80 {figure_height}\"><rect width=\"80\" height=\"{figure_height}\" \
                 fill=\"#eee\"/><text x=\"4\" y=\"24\" font-size=\"12\">FIGURE</text></svg>"
            )
            .into_bytes(),
        )],
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 400.0;
    options.theme.page.size.height_pt = content_height + 96.0;
    options.theme.page.margins = franken_markdown::PageMargins {
        top_pt: 48.0,
        right_pt: 36.0,
        bottom_pt: 48.0,
        left_pt: 36.0,
    };
    options
}

fn caption(count: usize) -> String {
    (0..count)
        .map(|index| format!("CAP{index:02}."))
        .collect::<Vec<_>>()
        .join("  \n")
}

fn plain(pages: &[String]) -> String {
    pages.concat().replace('•', "")
}

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

fn exact_pages(source: &str, options: &PdfOptions) -> Vec<String> {
    let rendered = render_pdf_reported(source, options)
        .unwrap_or_else(|error| panic!("expected an exact layout for {source:?}: {error}"));
    assert!(rendered.pagination.page_breaks.is_exact(), "{source}");
    let monolithic = render_pdf_document_reported(
        &parse_markdown(source),
        options,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(
        rendered, monolithic,
        "both emitters use the same hard keeps"
    );
    let mut best_effort = options.clone();
    best_effort.strict_pagination = false;
    best_effort.optimal_pagination = true;
    assert_eq!(
        rendered.bytes,
        render_pdf(source, &best_effort).unwrap(),
        "strictness does not change successful exact output"
    );
    painted_pages(&rendered.bytes)
}

fn assert_infeasible(error: &RenderError) {
    assert!(
        matches!(
            error,
            RenderError::Pagination(PdfPaginationFailure::Planner(
                HeightPaginationError::NoFeasibleLayout { .. }
            ))
        ),
        "expected typed infeasibility, got {error}"
    );
    assert_eq!(error.code(), "pagination_infeasible");
}

#[test]
fn short_captions_move_with_the_first_subject_fragment() {
    let subjects = [
        ("![Figure](figure.svg)", "FIGURE"),
        ("| HEAD |\n| --- |\n| CELL |", "HEADCELL"),
        ("```text\nCODEA\nCODEB\n```", "CODEACODEB"),
        ("- ITEMA  \n  ITEMB", "ITEMAITEMB"),
    ];
    let options = options(148.0, 80);
    for count in 1..=3 {
        let preceding = (0..9 - count)
            .map(|index| format!("PRE{index:02}."))
            .collect::<Vec<_>>()
            .join("  \n");
        let caption = caption(count);
        let control = format!("{preceding}\n\n{caption}");
        let control_pages = exact_pages(&control, &options);
        assert_eq!(control_pages.len(), 1, "the caption fits without a subject");
        let before_subject = compact(&control);

        for (subject, expected) in subjects {
            let source = format!("{control}\n\n{subject}");
            let pages = exact_pages(&source, &options);
            assert_eq!(pages.len(), 2, "{source}");
            assert!(!pages[0].contains("CAP00."), "move the entire caption");
            assert!(pages[1].contains(&compact(&caption)));
            assert!(pages[1].contains(expected), "keep the subject with it");
            assert_eq!(
                plain(&pages),
                format!("{before_subject}{expected}"),
                "preserve every painted source character in order"
            );
        }
    }
}

fn oversized_cases() -> Vec<(String, PdfOptions, String)> {
    let caption = caption(3);
    let mut code = options(72.0, 172);
    // Each block fits alone at this explicitly requested size, but not even
    // the first code line can accompany the three-line introduction.
    code.base_font_size = Some(14.0);
    vec![
        (
            "CAP00.\n\n![Figure](figure.svg)".to_string(),
            options(148.0, 172),
            "CAP00.FIGURE".to_string(),
        ),
        (
            format!("{caption}\n\n```text\nCODEA\nCODEB\n```"),
            code,
            format!("{}CODEACODEB", compact(&caption)),
        ),
        (
            format!("{caption}\n\n| HEAD |\n| --- |\n| CELL |"),
            options(72.0, 172),
            format!("{}HEADCELL", compact(&caption)),
        ),
        (
            format!("{caption}\n\n- ITEMA  \n  ITEMB"),
            options(80.0, 172),
            format!("{}ITEMAITEMB", compact(&caption)),
        ),
    ]
}

#[test]
fn oversized_kept_groups_refuse_strict_exports_and_report_successful_fallbacks() {
    for (source, mut options, expected) in oversized_cases() {
        let strict = render_pdf_reported(&source, &options).unwrap_err();
        assert_infeasible(&strict);

        let monolithic = render_pdf_document_reported(
            &parse_markdown(&source),
            &options,
            PdfEmitOptions::monolithic(),
        )
        .unwrap_err();
        assert_infeasible(&monolithic);
        assert_eq!(strict.to_string(), monolithic.to_string());

        options.strict_pagination = false;
        options.optimal_pagination = true;
        let fallback = render_pdf_reported(&source, &options).unwrap();
        assert!(!fallback.pagination.page_breaks.is_exact());
        let reason = fallback.pagination.page_breaks.fallback.as_ref().unwrap();
        assert!(matches!(&strict, RenderError::Pagination(actual) if actual == reason));
        assert_eq!(plain(&painted_pages(&fallback.bytes)), expected);
        assert!(
            render_warnings(&parse_markdown(&source), &options)
                .iter()
                .any(|warning| warning.code() == "pagination_infeasible")
        );

        options.optimal_pagination = false;
        let ordinary = render_pdf_reported(&source, &options).unwrap();
        assert!(!ordinary.pagination.page_breaks.requested_optimal);
        assert!(ordinary.pagination.page_breaks.fallback.is_none());
        assert_eq!(plain(&painted_pages(&ordinary.bytes)), expected);
        assert_eq!(ordinary.bytes, render_pdf(&source, &options).unwrap());
    }
}

#[test]
fn long_subjects_split_after_a_valid_first_fragment_without_losing_content() {
    let rows = (0..24)
        .map(|index| format!("ROW{index:02}"))
        .collect::<Vec<_>>();
    let subjects = [
        format!("```text\n{}\n```", rows.join("\n")),
        format!(
            "| HEAD |\n| --- |\n{}",
            rows.iter()
                .map(|row| format!("| {row} |\n"))
                .collect::<String>()
        ),
        format!("- {}", rows.join("  \n  ")),
    ];
    for (kind, subject) in subjects.iter().enumerate() {
        let source = format!("CAP00.\n\n{subject}");
        let pages = exact_pages(&source, &options(100.0, 80));
        assert!(pages.len() >= 3, "exercise real subject continuations");
        assert!(pages[0].contains("CAP00."));
        assert!(pages[0].contains("ROW00"));
        if kind == 2 {
            assert!(pages[0].contains("ROW01"), "retain list paragraph minima");
        }
        let mut content = plain(&pages);
        if kind == 1 {
            assert!(
                pages.iter().all(|page| page.contains("HEAD")),
                "every table continuation retains its header"
            );
            content = content.replace("HEAD", "");
        }
        assert_eq!(content, format!("CAP00.{}", rows.concat()));
    }
}

#[test]
fn caption_inference_respects_quote_and_list_item_ownership() {
    let sources = [
        "> CAP00.\n\n![Figure](figure.svg)",
        "CAP00.\n\n> ![Figure](figure.svg)",
        "- CAP00.\n\n![Figure](figure.svg)",
        "- CAP00.\n- ![Figure](figure.svg)",
    ];
    let options = options(148.0, 172);
    for source in sources {
        let pages = exact_pages(source, &options);
        assert_eq!(pages.len(), 2);
        assert!(pages[0].contains("CAP00."));
        assert!(pages[1].contains("FIGURE"));
        assert_eq!(plain(&pages), "CAP00.FIGURE");
    }

    // Two sibling quotes have equal nesting depth and x offsets, but different
    // container identities. Explicit AST blocks avoid merging them in Markdown.
    let mut document = parse_markdown("> CAP00.");
    document
        .blocks
        .extend(parse_markdown("> ![Figure](figure.svg)").blocks);
    let rendered =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap();
    assert!(rendered.pagination.page_breaks.is_exact());
    assert_eq!(painted_pages(&rendered.bytes), ["CAP00.", "FIGURE"]);
}

#[test]
fn nested_introductions_form_transitive_keeps_only_with_their_own_subjects() {
    let source = "CAP00.\n\n- CAP01.\n\n  - ![Figure](figure.svg)";
    // Either adjacent pair fits, but the complete kept chain does not.
    let error = render_pdf(source, &options(180.0, 172)).unwrap_err();
    assert_infeasible(&error);
    let pages = exact_pages(source, &options(200.0, 172));
    assert_eq!(pages.len(), 1);
    assert_eq!(plain(&pages), "CAP00.CAP01.FIGURE");

    // A short caption introducing a table within a quoted list item retains
    // that item's quote/list ownership, even when the table header is empty.
    let source = "> - CAP00.\n>\n>   | |\n>   | --- |\n>   | CELL |";
    let pages = exact_pages(source, &options(100.0, 80));
    assert_eq!(pages.len(), 1);
    assert_eq!(plain(&pages), "CAP00.CELL");
}

#[test]
fn prepared_endnote_boundaries_do_not_create_caption_bonds() {
    let mut document = parse_markdown("");
    document.blocks = vec![
        Block::FootnoteDefinition {
            id: "fmd-fn-1".to_string(),
            blocks: parse_markdown("CAP00.").blocks,
        },
        Block::FootnoteDefinition {
            id: "fmd-fn-2".to_string(),
            blocks: parse_markdown("![Figure](figure.svg)").blocks,
        },
    ];
    let options = options(148.0, 172);
    let rendered =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap();
    assert!(rendered.pagination.page_breaks.is_exact());
    assert_eq!(painted_pages(&rendered.bytes), ["NotesCAP00.", "FIGURE"]);
    let monolithic =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::monolithic()).unwrap();
    assert_eq!(rendered, monolithic);

    // A note's own caption still binds its figure. Only entry into a new
    // prepared note cancels the inferred association.
    document.blocks = vec![Block::FootnoteDefinition {
        id: "fmd-fn-1".to_string(),
        blocks: parse_markdown("CAP00.\n\n![Figure](figure.svg)").blocks,
    }];
    let error =
        render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap_err();
    assert_infeasible(&error);
}

#[test]
fn explicit_page_breaks_override_inferred_caption_bonds() {
    for (source, options, expected) in oversized_cases() {
        let mut document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2);
        document.blocks.insert(1, Block::PageBreak);
        let rendered =
            render_pdf_document_reported(&document, &options, PdfEmitOptions::default()).unwrap();
        assert!(rendered.pagination.page_breaks.is_exact());
        let pages = painted_pages(&rendered.bytes);
        assert_eq!(pages.len(), 2);
        assert_eq!(plain(&pages), expected);
        let monolithic =
            render_pdf_document_reported(&document, &options, PdfEmitOptions::monolithic())
                .unwrap();
        assert_eq!(rendered, monolithic);
    }
}

#[test]
fn ordinary_paragraphs_long_introductions_and_eof_remain_independent() {
    let end = caption(3);
    let pages = exact_pages(&end, &options(72.0, 80));
    assert_eq!(pages, [compact(&end)]);

    let following = "BODY00.  \nBODY01.  \nBODY02.";
    let source = format!("{end}\n\n{following}");
    let pages = exact_pages(&source, &options(72.0, 80));
    assert_eq!(pages, [compact(&end), compact(following)]);

    let long = caption(4);
    let source = format!("{long}\n\n- ITEMA  \n  ITEMB");
    let pages = exact_pages(&source, &options(80.0, 80));
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0], compact(&long));
    assert_eq!(plain(&pages), format!("{}ITEMAITEMB", compact(&long)));
}

#[test]
fn wasm_uses_the_same_extended_exact_contract() {
    for (source, options, _) in oversized_cases() {
        let browser_options = franken_markdown::wasm::WasmRenderOptions {
            theme: options.theme.clone(),
            pdf_image_assets: options.image_assets.clone(),
            base_font_size: options.base_font_size,
            strict_pagination: true,
            ..franken_markdown::wasm::WasmRenderOptions::default()
        };
        let native = render_pdf(&source, &options).unwrap_err();
        let browser = franken_markdown::wasm::render_pdf(&source, &browser_options).unwrap_err();
        assert_infeasible(&browser);
        assert_eq!(native.to_string(), browser.to_string());
    }
    let source = "CAP00.\n\n![Figure](figure.svg)";
    let options = options(148.0, 80);
    let browser_options = franken_markdown::wasm::WasmRenderOptions {
        theme: options.theme.clone(),
        pdf_image_assets: options.image_assets.clone(),
        strict_pagination: true,
        ..franken_markdown::wasm::WasmRenderOptions::default()
    };
    let browser = franken_markdown::wasm::render_pdf(source, &browser_options).unwrap();
    assert_eq!(browser.bytes, render_pdf(source, &options).unwrap());
    assert_eq!(painted_pages(&browser.bytes), ["CAP00.FIGURE"]);
}
