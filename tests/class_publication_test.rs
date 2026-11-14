//! Exercise the shared class-diagram compiler through document renderers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::verification_text_layer;
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::{HtmlOptions, PdfOptions, parse_markdown, render_html, render_pdf};

const SOURCE: &str = concat!(
    "# Architecture\n\n```mermaid\nclassDiagram\n",
    "class Document {\n  +String title\n  +render() String\n}\n",
    "Document \"1\" *-- \"many\" Renderer : renders & caches\n",
    "Renderer <|.. HtmlRenderer : implements\n",
    "```\n"
);

#[test]
fn class_members_and_relationships_become_figures_in_html_pdf_and_svg() {
    let html = render_html(SOURCE, &HtmlOptions::default()).unwrap();
    assert!(html.contains("class=\"fmd-diagram fmd-class\""));
    for label in [
        "+String title",
        "+render() String",
        "renders &amp; caches",
        "implements",
    ] {
        assert!(html.contains(label), "lost class diagram text {label:?}");
    }
    assert!(!html.contains("language-mermaid"));
    assert!(!html.contains("<script"));

    let doc = parse_markdown(SOURCE);
    let opts = PdfOptions::default();
    let first = render_pdf(SOURCE, &opts).unwrap();
    assert_eq!(first, render_pdf(SOURCE, &opts).unwrap());
    let raw = String::from_utf8_lossy(&first);
    assert_eq!(raw.matches("/S /Figure").count(), 1);
    assert!(raw.contains("/Alt (Class diagram)"));
    let layer = verification_text_layer(&doc, &opts).unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().any(|run| run.kind == "image"));
    assert!(!runs.iter().any(|run| run.text.contains("classDiagram")));

    let (poster, _, warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
    let poster = String::from_utf8(poster).unwrap();
    assert!(poster.contains("data:image/svg+xml;base64,"));
    assert!(poster.contains("aria-label=\"Class diagram\""));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn unsupported_class_features_preserve_the_whole_document_source() {
    let source = "```mermaid\nclassDiagram\nclass Before\nnote for Before \"Retain this note\"\nBefore --> After\n```\n";
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("language-mermaid"));
    assert!(!html.contains("class=\"fmd-diagram fmd-class\""));
    assert!(html.contains("Retain this note"));

    let doc = parse_markdown(source);
    let layer = verification_text_layer(&doc, &PdfOptions::default()).unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().all(|run| run.kind != "image"));
    assert!(runs.iter().any(|run| run.text.contains("Retain this note")));
    assert!(runs.iter().any(|run| run.text.contains("Before")));
    assert!(runs.iter().any(|run| run.text.contains("After")));
}
