//! Generic Mermaid types survive publication and retain their exact source in Flow.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::dep_invalidation::FlowSession;
use franken_markdown::display::{AccessibleReadingRole, DisplayItem};
use franken_markdown::flow_display::{FlowDisplayLimits, FlowLayoutOptions};
use franken_markdown::fonts::BundledFlowFonts;
use franken_markdown::pdf::verification_text_layer;
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::{
    FontFamily, HtmlOptions, PdfOptions, parse_markdown, render_html, render_pdf,
};

const CODE: &str = concat!(
    "classDiagram\n",
    "class Store~T~ {\n",
    "  ~List~T~ items\n",
    "  +read() List~List~T~~\n",
    "}\n",
    "Store --> Record: holds\n",
);

fn source(code: &str) -> String {
    format!("```mermaid\n{code}```\n")
}

#[test]
fn generic_classes_reach_html_pdf_and_svg_as_complete_figures() {
    let source = source(CODE);
    let html = render_html(&source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("class=\"fmd-diagram fmd-class\""));
    for text in [
        "Store&lt;T&gt;",
        "~List&lt;T&gt; items",
        "+read() List&lt;List&lt;T&gt;&gt;",
    ] {
        assert!(html.contains(text), "missing rendered type {text:?}");
    }
    assert!(!html.contains("language-mermaid"));
    assert!(!html.contains("<script"));

    let document = parse_markdown(&source);
    let options = PdfOptions::default();
    let pdf = render_pdf(&source, &options).unwrap();
    assert_eq!(pdf, render_pdf(&source, &options).unwrap());
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/S /Figure").count(), 1);
    assert!(raw.contains("/Alt (Class diagram)"));
    let layer = verification_text_layer(&document, &options).unwrap();
    assert!(
        layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.kind == "image")
    );
    assert!(
        !layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text.contains("classDiagram"))
    );

    let (poster, _, warnings) = render_svg_with_diagnostics(&document, &SvgOptions::default());
    let poster = String::from_utf8(poster).unwrap();
    assert!(poster.contains("data:image/svg+xml;base64,"));
    assert!(poster.contains("aria-label=\"Class diagram\""));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn flow_draws_generic_labels_and_preserves_the_exact_accessible_source() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for width in [240.0, 800.0] {
        let source = source(CODE);
        let session = FlowSession::new(
            &source,
            8,
            FlowDisplayLimits::default(),
            FlowLayoutOptions {
                viewport_width: width,
                ..FlowLayoutOptions::default()
            },
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap();
        let display = session.display();
        let nodes: Vec<_> = display
            .reading_order()
            .iter()
            .filter(|node| node.role == AccessibleReadingRole::Diagram)
            .collect();
        assert_eq!(nodes.len(), 1);
        assert_eq!(
            nodes[0].text, CODE,
            "accessible source keeps authored tildes and whitespace"
        );
        assert!(nodes[0].source_span.slice(&source).unwrap().contains(CODE));
        assert!(nodes[0].bounds.x >= 0.0 && nodes[0].bounds.right() <= width + 0.01);
        assert!(
            display
                .items()
                .iter()
                .any(|item| matches!(item, DisplayItem::Path(_)))
        );
        let text: String = display
            .items()
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect();
        for expected in [
            "Store<T>",
            "~List<T> items",
            "+read() List<List<T>>",
            "Record",
            "holds",
        ] {
            assert!(
                text.contains(expected),
                "missing native label {expected:?}: {text}"
            );
        }
        assert!(
            !text.contains("classDiagram"),
            "the supported diagram must not fall back to source"
        );
        assert!(session.pending_assets().is_empty());
    }
}

#[test]
fn malformed_generic_publications_retain_all_authored_source() {
    let code = "classDiagram\nclass Before\nBefore: +Map~K,V~ values\nBefore --> After\n";
    let source = source(code);
    let html = render_html(&source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("language-mermaid"));
    assert!(!html.contains("class=\"fmd-diagram fmd-class\""));
    for text in ["Before", "After", "Map~K,V~"] {
        assert!(html.contains(text), "source fallback lost {text}");
    }
    let layer = verification_text_layer(&parse_markdown(&source), &PdfOptions::default()).unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().all(|run| run.kind != "image"));
    let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();
    assert!(text.contains("Before"));
    assert!(text.contains("After"));
    assert!(text.contains("Map~K,V~"));
}
