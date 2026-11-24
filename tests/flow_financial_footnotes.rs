//! Financial citations keep their source-owned decimal boundary through live
//! Flow projection, actual bundled-font shaping, and browser interaction.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::FontFamily;
use franken_markdown::dep_invalidation::{BrowserFlowSession, FlowAssetReuse, FlowSession};
use franken_markdown::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayItem, DisplayList, DisplayTextRun,
};
use franken_markdown::flow_display::{
    FlowDisplayLimits, FlowInlineStyle, FlowLayoutError, FlowLayoutOptions, ResumableFlowDisplay,
};
use franken_markdown::fonts::BundledFlowFonts;

const SOURCE: &str = "| Item | Amount |\n| --- | ---. |\n\
    | Plain | 123.4 |\n\
    | Linked | [**7**.005](https://example.com/amount)[^a] |\n\
    | Account | (€42.50)[^b] |\n\
    | Count | 17[^a][^b] |\n\
    | Spaced | 6.70 [^a] |\n\
    | | === |\n\n[^a]: Alpha note.\n\n[^b]: Beta note.\n";

fn options(width: f32) -> FlowLayoutOptions {
    FlowLayoutOptions {
        viewport_width: width,
        ..FlowLayoutOptions::default()
    }
}

fn engine(source: &str, batch: usize) -> ResumableFlowDisplay {
    let mut engine = ResumableFlowDisplay::new(source, batch);
    engine.process_all().unwrap();
    engine
}

fn render(source: &str, width: f32, styled: bool) -> DisplayList {
    let engine = engine(source, 64);
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    if styled {
        engine
            .to_styled_display_list(options(width), |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap()
    } else {
        engine
            .to_shaped_display_list(options(width), |text, size, role| {
                fonts.shape(text, size, role, FlowInlineStyle::default())
            })
            .unwrap()
    }
}

fn amounts(display: &DisplayList) -> Vec<&AccessibleReadingNode> {
    display
        .reading_order()
        .iter()
        .filter(|node| node.role == AccessibleReadingRole::TableRow)
        .map(|row| row.children.last().unwrap())
        .collect()
}

fn cell_texts<'a>(
    display: &'a DisplayList,
    cell: &AccessibleReadingNode,
) -> Vec<&'a DisplayTextRun> {
    display
        .items()
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text(text)
                if text.bounds.y >= cell.bounds.y
                    && text.bounds.y < cell.bounds.bottom()
                    && text.bounds.x >= cell.bounds.x
                    && text.bounds.x < cell.bounds.right() =>
            {
                Some(text)
            }
            _ => None,
        })
        .collect()
}

/// Read the host's actual glyph clusters at an authored numeric boundary. This
/// does not reuse the table alignment producer or infer a width from characters.
fn anchor(display: &DisplayList, cell: &AccessibleReadingNode, offset: usize) -> f32 {
    let mut consumed = 0;
    for text in cell_texts(display, cell) {
        let end = consumed + text.text.len();
        if offset <= end {
            let local = offset - consumed;
            let run = text.font_run.as_ref().unwrap();
            if local == text.text.len() {
                return text.bounds.x + run.total_advance;
            }
            return text.bounds.x
                + run
                    .clusters
                    .iter()
                    .find(|cluster| cluster.byte_range.start == local)
                    .expect("numeric boundary belongs to an actual text cluster")
                    .x_start;
        }
        consumed = end;
    }
    panic!("numeric boundary {offset} absent from {:?}", cell.text);
}

fn assert_cell_text(display: &DisplayList, cell: &AccessibleReadingNode, expected: &str) {
    assert_eq!(cell.text, expected);
    let mut actual = String::new();
    for text in cell_texts(display, cell) {
        actual.push_str(&text.text);
        assert!(text.bounds.right() <= cell.bounds.right() - 3.99);
        assert!(text.bounds.bottom() <= cell.bounds.bottom() + 0.01);
        assert_eq!(text.source_span, cell.source_span);
        let run = text.font_run.as_ref().unwrap();
        assert_eq!(run.logical_text, text.text);
        assert!(run.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
    }
    assert_eq!(actual, expected, "no amount or citation text may disappear");
}

#[test]
fn resolved_trailing_citations_share_the_numeric_anchor_without_losing_text() {
    for styled in [false, true] {
        for width in [360.0, 720.0] {
            let display = render(SOURCE, width, styled);
            let cells = amounts(&display);
            assert_eq!(cells.len(), 5);
            let mut anchors = Vec::new();
            for (cell, (expected, offset)) in cells.iter().zip([
                ("123.4", 3),
                ("7.005[1]", 1),
                ("(€42.50)[2]", 6),
                ("17[1][2]", 2),
                ("6.70 [1]", 1),
            ]) {
                assert_cell_text(&display, cell, expected);
                let fragments = cell_texts(&display, cell);
                assert!(
                    fragments
                        .iter()
                        .all(|text| text.bounds.y == fragments[0].bounds.y),
                    "a measured fitting amount must keep its complete citation on one line: {expected}"
                );
                anchors.push(anchor(&display, cell, offset));
            }
            assert!(
                anchors.iter().all(|x| (*x - anchors[0]).abs() < 0.01),
                "{styled}/{width}: numeric anchors disagree: {anchors:?}",
            );
        }
    }
}

#[test]
fn numeric_citation_links_keep_browser_hits_selection_and_source_ownership() {
    let display = render(SOURCE, 720.0, true);
    let browser = BrowserFlowSession::new(SOURCE, "sans", options(720.0)).unwrap();
    let cell = amounts(&display)[1];
    assert_eq!(cell.text, "7.005[1]");
    let citation = cell_texts(&display, cell)
        .into_iter()
        .find(|text| text.text == "[1]")
        .expect("a fitting citation remains a complete linked text run");
    let index = display
        .items()
        .iter()
        .position(|item| matches!(item, DisplayItem::Text(text) if std::ptr::eq(text, citation)))
        .unwrap();
    let selected = browser.select_item_json(1, 1, index, 0, 3).unwrap();
    assert!(selected.contains("\"text\":\"[1]\""));
    assert!(selected.contains("\"coordinateSpace\":\"fragment-utf16\""));
    let hit = browser
        .hit_test_json(
            1,
            1,
            citation.bounds.x + citation.bounds.width * 0.5,
            citation.bounds.y + citation.bounds.height * 0.5,
        )
        .unwrap();
    assert!(hit.contains("\"linkTarget\":\"#fmd:note:1\""));
    assert!(display.anchors().any(|link| {
        !link.is_heading
            && link.anchor_id == "#fmd:note:1"
            && link.bounds == citation.bounds
            && link.source_span == cell.source_span
    }));
    assert!(
        display
            .anchors()
            .any(|link| { !link.is_heading && link.anchor_id == "https://example.com/amount" })
    );
    assert!(
        display
            .anchors()
            .any(|target| { target.is_heading && target.anchor_id == "fmd:note:1" })
    );
    assert_eq!(
        browser.copy_source(1, cell.source_span).unwrap(),
        cell.source_span.slice(SOURCE).unwrap()
    );
    assert_eq!(browser.source(), SOURCE);
    assert!(
        browser
            .reading_json(1, 1, 0, 64)
            .unwrap()
            .contains("7.005[1]")
    );
}

#[test]
fn only_resolved_trailing_citations_confer_decimal_admission() {
    for (candidate, expected) in [
        ("12.[^a]30", "12.[2]30"),
        ("12.30[^missing]", "12.30[^missing]"),
        ("12.30[\\[1\\]](#fmd:note:1)", "12.30[1]"),
        ("12.30[^a]kg", "12.30[2]kg"),
        ("`12.30`[^a]", "12.30[2]"),
    ] {
        let source = format!(
            "| Item | Amount |\n| --- | ---. |\n\
             | Anchor | 1,234.567[^b] |\n| Candidate | {candidate} |\n\n\
             [^a]: Alpha.\n\n[^b]: Beta.\n"
        );
        let display = render(&source, 720.0, true);
        let cell = amounts(&display)[1];
        assert_cell_text(&display, cell, expected);
        let right = cell_texts(&display, cell).last().unwrap().bounds.right();
        assert!(
            (right - (cell.bounds.right() - 4.0)).abs() < 0.01,
            "{candidate:?} must retain ordinary right alignment"
        );
    }
}

#[test]
fn multidigit_citations_are_measured_inside_narrow_shared_grids() {
    let mut source = String::from("| Item | Amount |\n| --- | ---. |\n");
    for number in 1..=12 {
        source.push_str(&format!("| Row {number} | {number}.25[^n{number}] |\n"));
    }
    for number in 1..=12 {
        source.push_str(&format!("\n[^n{number}]: Note {number}.\n"));
    }
    for width in [180.0, 240.0, 420.0] {
        let display = render(&source, width, true);
        let cells = amounts(&display);
        assert_eq!(cells.len(), 12);
        for (index, cell) in cells.iter().enumerate() {
            let number = index + 1;
            assert_cell_text(&display, cell, &format!("{number}.25[{number}]"));
        }
        // At a roomy width, a second digit in a note label must enlarge the
        // common suffix envelope, not move the associated decimal point.
        if width == 420.0 {
            let first = anchor(&display, cells[0], 1);
            for (index, cell) in cells.iter().enumerate() {
                let offset = if index < 9 { 1 } else { 2 };
                assert!((anchor(&display, cell, offset) - first).abs() < 0.01);
            }
        }
    }
}

#[test]
fn incremental_publication_and_definition_edits_recompute_trusted_anchors() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let complete = render(SOURCE, 720.0, true);
    let mut partial = ResumableFlowDisplay::new(SOURCE, 1);
    while partial.blocks().is_empty() {
        partial.step().unwrap();
    }
    let header = partial
        .to_styled_display_list(options(720.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert_eq!(header.items(), &complete.items()[..header.items().len()]);
    partial.process_all().unwrap();
    assert_eq!(
        partial
            .to_styled_display_list(options(720.0), |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap(),
        complete,
    );

    let mut session = FlowSession::new(
        SOURCE,
        1,
        FlowDisplayLimits::default(),
        options(720.0),
        |text, size, role, style| fonts.shape(text, size, role, style),
    )
    .unwrap();
    let unresolved = SOURCE.replace("[^a]: Alpha note.\n", "");
    session
        .replace_source(
            1,
            &unresolved,
            FlowAssetReuse::Invalidate,
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap();
    assert_eq!(session.display(), &render(&unresolved, 720.0, true));
    assert_eq!(amounts(session.display())[1].text, "7.005[^a]");
    session
        .replace_source(
            2,
            SOURCE,
            FlowAssetReuse::Invalidate,
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap();
    assert_eq!(session.display(), &complete);
    assert_eq!(session.source(), SOURCE);
}

#[test]
fn citation_measurement_obeys_the_existing_shaping_budget() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let engine = engine(SOURCE, 64);
    let mut bytes = 0;
    engine
        .to_styled_display_list(options(720.0), |text, size, role, style| {
            bytes += text.len();
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert_eq!(
        engine
            .to_styled_display_list(
                FlowLayoutOptions {
                    max_total_shape_bytes: bytes - 1,
                    ..options(720.0)
                },
                |text, size, role, style| fonts.shape(text, size, role, style)
            )
            .unwrap_err(),
        FlowLayoutError::BudgetExceeded("total shaping bytes"),
    );
    assert_eq!(engine.source(), SOURCE);
}
