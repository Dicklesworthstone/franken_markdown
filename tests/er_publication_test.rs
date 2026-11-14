//! Exercise native entity diagrams through the publication backends.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::verification_text_layer;
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::{HtmlOptions, PdfOptions, parse_markdown, render_html, render_pdf};

const SOURCE: &str = concat!(
    "# Data model\n\n```mermaid\nerDiagram\n",
    "CUSTOMER {\n  int id PK\n  string email UK\n}\n",
    "ORDER {\n  int id PK\n  int customer_id FK\n}\n",
    "CUSTOMER ||--o{ ORDER : \"places & tracks\"\n",
    "```\n"
);

#[test]
fn entity_tables_and_relationships_become_figures_in_html_pdf_and_svg() {
    let html = render_html(SOURCE, &HtmlOptions::default()).unwrap();
    assert!(html.contains("class=\"fmd-diagram fmd-er\""));
    for label in ["CUSTOMER", "ORDER", "email", "customer_id", "PK", "FK", "UK"] {
        assert!(html.contains(&format!(">{label}</text>")), "{label}");
    }
    assert!(html.contains("places &amp; tracks"));
    assert!(!html.contains("language-mermaid"));
    assert!(!html.contains("<script"));

    let doc = parse_markdown(SOURCE);
    let opts = PdfOptions::default();
    let pdf = render_pdf(SOURCE, &opts).unwrap();
    assert_eq!(pdf, render_pdf(SOURCE, &opts).unwrap());
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/S /Figure").count(), 1);
    assert!(raw.contains("/Alt (Entity relationship diagram)"));
    let layer = verification_text_layer(&doc, &opts).unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().any(|run| run.kind == "image"));
    assert!(!runs.iter().any(|run| run.text.contains("erDiagram")));

    let (poster, _, warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
    let poster = String::from_utf8(poster).unwrap();
    assert!(poster.contains("data:image/svg+xml;base64,"));
    assert!(poster.contains("aria-label=\"Entity relationship diagram\""));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn unsupported_entity_features_retain_the_complete_source_in_html_and_pdf() {
    let source = "```mermaid\nerDiagram\nCUSTOMER ||--o{ ORDER : places\nstyle CUSTOMER fill:red\n```\n";
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("language-mermaid"));
    assert!(!html.contains("class=\"fmd-diagram fmd-er\""));
    assert!(html.contains("CUSTOMER"));
    assert!(html.contains("ORDER"));

    let layer = verification_text_layer(&parse_markdown(source), &PdfOptions::default()).unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().all(|run| run.kind != "image"));
    assert!(runs.iter().any(|run| run.text.contains("style CUSTOMER")));
    assert!(runs.iter().any(|run| run.text.contains("ORDER")));
}
