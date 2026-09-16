//! End-to-end footnote fidelity through the real public PDF entrypoints.
//! Expected documents are assembled independently, not by calling the note
//! transformer under test, so discarded blocks cannot make both sides pass.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Align, Block, DefinitionItem, Document, FontAssets, Inline, List, ListItem,
    PageMargins, PageSize, PdfASettings, PdfEmitOptions, PdfOptions, Table,
    parse_markdown, render_pdf_document, render_pdf_document_emitted,
    render_pdf_document_pdfa, render_pdf_document_profiled,
};
use franken_markdown::verify::{self, VerifyReport};

fn text(value: &str) -> Inline { Inline::Text(value.to_string()) }
fn paragraph(value: &str) -> Block { Block::Paragraph(vec![text(value)]) }
fn reference(id: &str) -> Inline { Inline::FootnoteRef { id: id.to_string() } }
fn notes_heading() -> Block {
    Block::Heading { level: 2, inlines: vec![text("Notes")] }
}

fn rich_document() -> (Document, Document) {
    let body = vec![
        paragraph("PARAPROOF"),
        Block::CodeBlock { lang: None, code: "CODEPROOF = 42;\n".to_string() },
        Block::List(List {
            ordered: false, start: 1, tight: false,
            items: vec![ListItem { task: None, blocks: vec![paragraph("LISTPROOF")] }],
        }),
        Block::Table(Table {
            align: vec![Align::Left], head: vec![vec![text("HEADERPROOF")]],
            rows: vec![vec![vec![text("CELLPROOF")]]],
        }),
        Block::BlockQuote(vec![paragraph("QUOTEPROOF")]),
        Block::DefinitionList(vec![DefinitionItem {
            terms: vec![vec![text("TERMPROOF")]],
            definitions: vec![vec![text("MEANINGPROOF")]],
        }]),
        Block::MathBlock("x^2".to_string()),
        paragraph("LASTPROOF"),
    ];
    let doc = Document { blocks: vec![
        Block::Heading { level: 1, inlines: vec![text("Study")] },
        Block::Paragraph(vec![text("Evidence"), reference("methods")]),
        Block::FootnoteDefinition { id: "methods".into(), blocks: body.clone() },
    ] };
    let mut expected = vec![
        Block::Heading { level: 1, inlines: vec![text("Study")] },
        Block::Paragraph(vec![text("Evidence"), text("[1]")]),
        notes_heading(),
        Block::Paragraph(vec![text("[1] "), text("PARAPROOF")]),
    ];
    expected.extend(body.into_iter().skip(1));
    (doc, Document { blocks: expected })
}

fn extracted(report: &VerifyReport) -> String {
    report.pages.iter().flat_map(|page| &page.runs)
        .map(|run| run.text.as_str()).collect::<Vec<_>>().join("\n")
}

#[test]
fn normal_profiled_and_explicit_emission_preserve_every_rich_note_block() {
    let (doc, expected) = rich_document();
    let original = doc.clone();
    let options = PdfOptions::default();
    let expected_bytes = render_pdf_document(&expected, &options).expect("independent expected PDF");
    assert_eq!(render_pdf_document(&doc, &options).unwrap(), expected_bytes);
    assert_eq!(render_pdf_document_profiled(&doc, &options).unwrap().bytes, expected_bytes);
    assert_eq!(render_pdf_document_emitted(&doc, &options, PdfEmitOptions::default()).unwrap(), expected_bytes);
    assert_eq!(doc, original, "all entrypoints borrow the input document");
}

#[test]
fn pdfa_renders_the_same_complete_notes_with_archival_identification() {
    let (doc, expected) = rich_document();
    let options = PdfOptions { metadata_epoch_seconds: Some(1_700_000_000), ..PdfOptions::default() };
    let settings = PdfASettings::a2b();
    let actual = render_pdf_document_pdfa(&doc, &options, settings).unwrap();
    let expected = render_pdf_document_pdfa(&expected, &options, PdfASettings::a2b()).unwrap();
    assert_eq!(actual, expected);
    assert!(actual.starts_with(b"%PDF-"));
}

#[test]
fn verification_text_layer_contains_the_same_notes_as_the_emitted_pdf() {
    let (doc, expected) = rich_document();
    let options = PdfOptions::default();
    let actual = verify::verify_pdf(&doc, &options).unwrap();
    let expected = verify::verify_pdf(&expected, &options).unwrap();
    assert_eq!(actual.pages, expected.pages, "verification must use PDF document preparation");
    assert_eq!(actual.page_count, expected.page_count);
    let text = extracted(&actual);
    for marker in ["PARAPROOF", "CODEPROOF", "LISTPROOF", "HEADERPROOF",
        "CELLPROOF", "QUOTEPROOF", "TERMPROOF", "MEANINGPROOF", "LASTPROOF"]
    {
        assert!(text.contains(marker), "missing {marker} in verified PDF text:\n{text}");
    }
    assert_eq!(verify::to_json(&actual), verify::to_json(&verify::verify_pdf(&doc, &options).unwrap()));
}

#[test]
fn first_reference_order_matches_numbered_bodies_even_when_definitions_are_reversed() {
    let doc = Document { blocks: vec![
        Block::FootnoteDefinition { id: "a".into(), blocks: vec![paragraph("ALPHAPROOF")] },
        Block::FootnoteDefinition { id: "b".into(), blocks: vec![paragraph("BETAPROOF")] },
        Block::Paragraph(vec![text("First"), reference("b"), text(" then "), reference("a")]),
    ] };
    let expected = Document { blocks: vec![
        Block::Paragraph(vec![text("First"), text("[1]"), text(" then "), text("[2]")]),
        notes_heading(),
        Block::Paragraph(vec![text("[1] "), text("BETAPROOF")]),
        Block::Paragraph(vec![text("[2] "), text("ALPHAPROOF")]),
    ] };
    assert_eq!(render_pdf_document(&doc, &PdfOptions::default()).unwrap(),
        render_pdf_document(&expected, &PdfOptions::default()).unwrap());
}

#[test]
fn long_footnotes_are_included_in_verification_page_counts() {
    let doc = Document { blocks: vec![
        Block::Paragraph(vec![text("Evidence"), reference("long")]),
        Block::FootnoteDefinition { id: "long".into(), blocks: (0..20)
            .map(|index| paragraph(&format!("NOTE{index} Evidence remains visible on every page.")))
            .collect() },
    ] };
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize { name: "note-test", width_pt: 240.0, height_pt: 160.0 };
    options.theme.page.margins = PageMargins { top_pt: 20.0, right_pt: 20.0, bottom_pt: 20.0, left_pt: 20.0 };
    let profile = render_pdf_document_profiled(&doc, &options).unwrap();
    let report = verify::verify_pdf(&doc, &options).unwrap();
    let pages = profile.stages.iter().find(|stage| stage.stage == "page_content_stream_generation")
        .or_else(|| profile.stages.iter().find(|stage| stage.stage == "pagination"))
        .expect("PDF page count stage").count as usize;
    assert!(report.page_count > 1, "the long note must really paginate");
    assert_eq!(report.page_count, pages, "verify must not count only the short body");
    let text = extracted(&report);
    assert!(text.contains("NOTE0"));
    assert!(text.contains("NOTE19"));
}

#[test]
fn generated_notes_anchor_resolves_and_unresolved_note_images_are_reported() {
    let doc = Document { blocks: vec![
        Block::Paragraph(vec![Inline::Link { dest: "#notes".into(), title: None,
            content: vec![text("Evidence notes")] }, reference("image")]),
        Block::FootnoteDefinition { id: "image".into(), blocks: vec![Block::Paragraph(vec![
            Inline::Image { dest: "evidence.svg".into(), title: None, alt: "Evidence plot".into() },
        ])] },
    ] };
    let report = verify::verify_pdf(&doc, &PdfOptions::default()).unwrap();
    assert!(report.anchors_unresolved.is_empty(), "the PDF contains the generated Notes heading");
    assert_eq!(report.anchors_resolved, 1);
    assert!(report.findings.iter().any(|finding| finding.detail.contains("evidence.svg")));
}

#[test]
fn accessibility_keeps_source_heading_context_instead_of_inventing_a_notes_heading_skip() {
    let doc = Document { blocks: vec![
        Block::Heading { level: 1, inlines: vec![text("Study")] },
        Block::Paragraph(vec![reference("appendix")]),
        Block::FootnoteDefinition { id: "appendix".into(), blocks: vec![
            Block::Heading { level: 5, inlines: vec![text("Source appendix heading")] },
            paragraph("PROOF"),
        ] },
    ] };
    let report = verify::verify_pdf(&doc, &PdfOptions::default()).unwrap();
    assert!(!report.findings.iter().any(|finding| finding.code == "heading_level_skip"));
    assert!(extracted(&report).contains("PROOF"));
}

#[test]
fn verification_rejects_invalid_host_fonts_like_rendering() {
    let doc = parse_markdown("# Study\n");
    let options = PdfOptions { font_assets: FontAssets {
        body_regular: Some(vec![0; 8]), ..FontAssets::default()
    }, ..PdfOptions::default() };
    assert!(render_pdf_document(&doc, &options).is_err());
    assert!(verify::verify_pdf(&doc, &options).is_none());
}

#[cfg(feature = "cli")]
#[test]
fn cli_verify_includes_code_and_table_content_from_parsed_markdown_notes() {
    use std::{fs, process::Command, time::{SystemTime, UNIX_EPOCH}};
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("fmd-note-fidelity-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("notes.md");
    let markdown = "# Study\n\nEvidence[^methods].\n\n[^methods]: PARAPROOF\n\n    ```text\n    CODEPROOF\n    ```\n\n    | HEADERPROOF |\n    | --- |\n    | CELLPROOF |\n";
    fs::write(&input, markdown).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "verify"]).arg(&input).arg("--json").output().unwrap();
    assert!(matches!(output.status.code(), Some(0 | 1)), "{}", String::from_utf8_lossy(&output.stderr));
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("CODEPROOF"), "CLI omitted the note code: {report}");
    assert!(report.contains("CELLPROOF"), "CLI omitted the note table: {report}");
}
