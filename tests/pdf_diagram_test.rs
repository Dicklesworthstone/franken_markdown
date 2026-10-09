//! Mermaid flowchart/sequence fences render as vector figures in PDF, the
//! same diagrams HTML draws; Mermaid types the compiler does not support stay
//! readable code in both outputs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::{VerifyTextRun, verification_text_layer};
use franken_markdown::{HtmlOptions, PdfOptions, parse_markdown, render_html, render_pdf};

const FLOW: &str = "```mermaid\ngraph TD\n  A[Start] --> B{Decide}\n  B -->|yes| C[Done]\n```\n";
const SEQUENCE: &str = "```mermaid\nsequenceDiagram\n  Alice->>Bob: Hi\n```\n";
const PIE: &str = "```mermaid\npie title Pets\n  \"Dogs\" : 386\n```\n";

fn runs(markdown: &str) -> Vec<VerifyTextRun> {
    verification_text_layer(&parse_markdown(markdown), &PdfOptions::default())
        .unwrap()
        .pages
        .into_iter()
        .flat_map(|page| page.runs)
        .collect()
}

#[test]
fn flowchart_and_sequence_fences_are_pdf_figures_not_source() {
    for fence in [FLOW, SEQUENCE] {
        let runs = runs(fence);
        assert!(
            runs.iter().any(|run| run.kind == "image"),
            "diagram is a figure: {runs:?}"
        );
        assert!(
            !runs
                .iter()
                .any(|run| run.text.contains("graph TD") || run.text.contains("->>")),
            "diagram source is not printed: {runs:?}"
        );
    }
    let pdf = render_pdf(&format!("{FLOW}\n{SEQUENCE}"), &PdfOptions::default()).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(
        raw.matches("/S /Figure").count(),
        2,
        "both tagged as figures"
    );
    assert!(raw.contains("/Alt (Sequence diagram)"));
}

#[test]
fn unsupported_mermaid_types_stay_code_in_pdf_and_html() {
    let runs = runs(PIE);
    assert!(runs.iter().all(|run| run.kind != "image"), "{runs:?}");
    assert!(runs.iter().any(|run| run.text.contains("pie title Pets")));
    let html = render_html(PIE, &HtmlOptions::default()).unwrap();
    assert!(!html.contains("<div class=\"fmd-diagram-wrapper\">"));
    assert!(html.contains("language-mermaid"));
    let html = render_html(FLOW, &HtmlOptions::default()).unwrap();
    assert!(html.contains("<div class=\"fmd-diagram-wrapper\">"));
}

#[test]
fn identical_diagrams_share_one_image_and_render_deterministically() {
    let markdown = format!("{FLOW}\nText.\n\n{FLOW}");
    let a = render_pdf(&markdown, &PdfOptions::default()).unwrap();
    assert_eq!(a, render_pdf(&markdown, &PdfOptions::default()).unwrap());
    let raw = String::from_utf8_lossy(&a);
    assert_eq!(raw.matches("/S /Figure").count(), 2);
}
