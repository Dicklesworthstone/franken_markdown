//! Public rendering regressions for opt-in financial tables.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::verification_text_layer;
use franken_markdown::{
    PdfASettings, PdfEmitOptions, PdfOptions, PdfPageEmission, parse_markdown, render_pdf,
    render_pdf_emitted, render_pdf_pdfa,
};

const STATEMENT: &str = include_str!("fixtures/financial_statement.md");

fn options() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        title: Some("Financial statement".to_owned()),
        ..PdfOptions::default()
    }
}

/// Follow actual page content references and declared stream lengths, without
/// assuming that a small stream is compressed or scanning font data as text.
fn contents(pdf: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(pdf)
        .split("/Contents ")
        .skip(1)
        .map(|reference| {
            let number = reference.split_whitespace().next().unwrap();
            let header = format!("\n{number} 0 obj\n");
            let start = pdf
                .windows(header.len())
                .position(|bytes| bytes == header.as_bytes())
                .unwrap()
                + header.len();
            let object = &pdf[start..];
            let stream = object
                .windows(8)
                .position(|bytes| bytes == b"\nstream\n")
                .unwrap();
            let dictionary = std::str::from_utf8(&object[..stream]).unwrap();
            let length: usize = dictionary
                .split_once("/Length ")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let encoded = &object[stream + 8..stream + 8 + length];
            let decoded = if dictionary.contains("/Filter /FlateDecode") {
                franken_markdown::zlib_decompress(encoded, 16 << 20).unwrap()
            } else {
                assert!(!dictionary.contains("/Filter"));
                encoded.to_vec()
            };
            String::from_utf8(decoded).unwrap()
        })
        .collect()
}

fn artifact_rules(content: &str) -> Vec<[f32; 4]> {
    content
        .split("/Artifact BMC\n")
        .skip(1)
        .filter_map(|block| {
            let artifact = block.split_once("\nEMC\n").unwrap().0;
            let coordinates = artifact.split_once(" w ")?.1;
            let values: Vec<_> = coordinates.split_whitespace().collect();
            if values.len() != 7 || values[2] != "m" || values[5..] != ["l", "S"] {
                return None;
            }
            Some([0, 1, 3, 4].map(|index| values[index].parse().unwrap()))
        })
        .collect()
}

#[test]
fn financial_pdf_emits_real_single_and_double_artifact_rules_without_marker_cells() {
    let pdf = render_pdf(STATEMENT, &options()).unwrap();
    let pages = contents(&pdf);
    assert_eq!(pages.len(), 1);
    let rules = artifact_rules(&pages[0]);
    assert_eq!(rules.len(), 5, "header, bottom, single and double footing");
    let footings: Vec<_> = rules.iter().filter(|rule| rule[0] > 72.01).collect();
    assert_eq!(footings.len(), 3);
    assert!(
        footings
            .iter()
            .all(|rule| { rule[0] < rule[2] && rule[2] < 540.0 && rule[1] == rule[3] })
    );
    assert!((footings[1][1] - footings[2][1] - 2.0).abs() < 0.01);
    assert!(footings[0][1] > footings[1][1]);

    let objects = String::from_utf8_lossy(&pdf);
    assert_eq!(
        objects.matches("/S /TR ").count(),
        4,
        "one header and three data rows"
    );
    assert_eq!(objects.matches("/S /TD ").count(), 6);
    assert_eq!(objects.matches("/S /TH ").count(), 2);
    assert!(objects.contains("/URI (https://example.com/revenue)"));

    let layer = verification_text_layer(&parse_markdown(STATEMENT), &options()).unwrap();
    let rows: Vec<_> = layer.pages[0]
        .runs
        .iter()
        .filter(|run| run.kind == "table_row")
        .collect();
    assert_eq!(rows.len(), 3);
    let text = rows.iter().map(|run| run.text.as_str()).collect::<String>();
    for value in [
        "Revenue",
        "1,234.5",
        "Costs",
        "(234.50)",
        "Total",
        "1,000.000",
    ] {
        assert!(
            text.contains(value),
            "missing original financial text {value}"
        );
    }
    assert!(!text.contains("---") && !text.contains("==="));
}

#[test]
fn financial_pdf_is_deterministic_across_emitters_and_pdfa_keeps_rules_and_links() {
    let opts = options();
    let chunked = render_pdf(STATEMENT, &opts).unwrap();
    assert_eq!(chunked, render_pdf(STATEMENT, &opts).unwrap());
    let monolithic = render_pdf_emitted(
        STATEMENT,
        &opts,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(chunked, monolithic);
    let archival = render_pdf_pdfa(STATEMENT, &opts, PdfASettings::a2b()).unwrap();
    assert_eq!(
        archival,
        render_pdf_pdfa(STATEMENT, &opts, PdfASettings::a2b()).unwrap()
    );
    assert_eq!(contents(&archival), contents(&chunked));
    let objects = String::from_utf8_lossy(&archival);
    assert!(objects.contains("<pdfaid:part>2</pdfaid:part>"));
    assert!(objects.contains("/Type /OutputIntent"));
    assert!(objects.contains("/URI (https://example.com/revenue)"));
}

#[test]
fn financial_multi_page_pdf_preserves_every_row_and_never_repeats_footings() {
    let mut source = String::from("| Item | Amount |\n| --- | ---. |\n");
    for row in 0..36 {
        source.push_str(&format!("| Entry{row} | {row}.125 |\n| | === |\n"));
    }
    for optimal in [false, true] {
        let mut opts = options();
        opts.optimal_pagination = optimal;
        opts.theme.page.size.height_pt = 220.0;
        opts.theme.page.margins.top_pt = 30.0;
        opts.theme.page.margins.bottom_pt = 30.0;
        let layer = verification_text_layer(&parse_markdown(&source), &opts).unwrap();
        let pdf = render_pdf(&source, &opts).unwrap();
        let pages = contents(&pdf);
        assert!(pages.len() > 2);
        assert_eq!(pages.len(), layer.page_count);
        let footings: Vec<_> = pages
            .iter()
            .flat_map(|page| artifact_rules(page))
            .filter(|rule| rule[0] > 72.01)
            .collect();
        assert_eq!(footings.len(), 72);
        assert!(footings.iter().all(|rule| rule[1] - 0.35 >= 29.99));
        let all_rows: Vec<_> = layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .filter(|run| run.kind == "table_row")
            .collect();
        assert_eq!(all_rows.len(), 36);
        for (index, row) in all_rows.iter().enumerate() {
            assert_eq!(row.text, format!("Entry{index}{index}.125"));
            assert!(row.overshoot.is_none());
        }
        for page in &layer.pages {
            assert!(page.runs.iter().any(|run| run.kind == "table_header"));
        }
    }
}

#[test]
fn gfm_right_alignment_keeps_marker_cells_as_literal_text() {
    let source = STATEMENT.replace("---.", "---:");
    let layer = verification_text_layer(&parse_markdown(&source), &options()).unwrap();
    let rows: Vec<_> = layer.pages[0]
        .runs
        .iter()
        .filter(|run| run.kind == "table_row")
        .collect();
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().any(|run| run.text == "---"));
    assert!(rows.iter().any(|run| run.text == "==="));
    let pdf = render_pdf(&source, &options()).unwrap();
    assert_eq!(artifact_rules(&contents(&pdf)[0]).len(), 2);
}
