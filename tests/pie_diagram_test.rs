//! Real shared-compiler and document-output checks for Mermaid pie charts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfOptions, is_diagram_code, render_diagram_svg, render_html, render_pdf,
};

#[test]
fn clockwise_quarters_use_exact_values_and_large_arc_geometry() {
    let source = "%% comment\npie showData title Allocation\n\"Quarter\" : 0.25\n\"Rest\" : 0.75\n";
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    assert!(svg.contains("<title>Allocation</title>"));
    assert!(svg.contains("Quarter [0.25]"));
    assert!(svg.contains("Rest [0.75]"));
    // The first slice begins at 12 o'clock and turns clockwise to 3 o'clock.
    assert!(
        svg.contains("L 144.000 70.000 A 120.000 120.000 0 0 1 264.000 190.000 Z"),
        "{svg}"
    );
    assert!(
        svg.contains("L 264.000 190.000 A 120.000 120.000 0 1 1 144.000 70.000 Z"),
        "{svg}"
    );
    assert!(svg.contains("Quarter: 0.25 (25%); Rest: 0.75 (75%)"));
    assert_eq!(svg, render_diagram_svg(source, "mermaid").unwrap());
    assert_eq!(svg, render_diagram_svg(source, "pie").unwrap());
    assert!(is_diagram_code(source, ""));
}

#[test]
fn single_and_dominant_slices_paint_a_circle_instead_of_an_empty_arc() {
    for source in [
        "pie\n\"Whole\": 12.34",
        "pie showData\n\"Whole\": 184467440737095516.15\n\"Tiny\": 0.01",
    ] {
        let svg = render_diagram_svg(source, "mermaid").unwrap();
        assert!(svg.contains("<circle "), "{svg}");
        assert!(svg.contains("100%"));
        assert!(!svg.contains("NaN"));
        assert!(!svg.contains("inf"));
    }
}

#[test]
fn labels_titles_comments_and_percentages_remain_safe_and_readable() {
    let source = "pie\n title <Revenue & costs>\n\"A %% retained\": 0.1 %% ignored comment\n\"<script>alert(1)</script>\": 0.2\n";
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    assert!(svg.contains("&lt;Revenue &amp; costs&gt;"));
    assert!(svg.contains("A %% retained"));
    assert!(svg.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("ignored comment"));
    assert!(svg.contains("33.3%"));
    assert!(svg.contains("66.7%"));
    assert!(!svg.contains("[0.1]"), "showData is optional");
    let label = "Long revenue category ".repeat(8);
    let svg = render_diagram_svg(&format!("pie title Report\n\"{label}\": 1"), "mermaid").unwrap();
    assert!(
        svg.matches("<text x=\"318\"").count() > 1,
        "long labels wrap"
    );
}

#[test]
fn invalid_or_unsupported_pies_preserve_complete_source_in_html_and_pdf() {
    for source in [
        "pie",
        "pie\n\"A\": 0",
        "pie\n\"A\": -1",
        "pie\n\"A\": NaN",
        "pie\n\"A\": inf",
        "pie\n\"A\": 1e3",
        "pie\n\"A\": 1.234",
        "pie\n\"A\": 1.",
        "pie\n\"A\": 184467440737095517",
        "pie\n\"A\": 1\n\"A\": 2",
        "pie\n\"\": 1",
        "pie\nA: 1",
        "pie showData unexpected\n\"A\": 1",
        "pie title\n\"A\": 1",
        "pie title First\ntitle Second\n\"A\": 1",
        "pie\n\"A\": 1\nunknown directive",
        "%%{init: { 'pie': { 'donutHole': 0.5 }}}%%\npie\n\"A\": 1",
    ] {
        assert!(render_diagram_svg(source, "mermaid").is_none(), "{source}");
        let markdown = format!("```mermaid\n{source}\n```\n");
        let html = render_html(&markdown, &HtmlOptions::default()).unwrap();
        assert!(html.contains("language-mermaid"));
        assert!(!html.contains("<div class=\"fmd-diagram-wrapper\">"));
    }
    let markdown = "```mermaid\npie title Broken\n\"Amount\": -1\n```\n";
    let pdf = render_pdf(markdown, &PdfOptions::default()).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/S /Figure"));
    let layer = franken_markdown::pdf::verification_text_layer(
        &franken_markdown::parse_markdown(markdown),
        &PdfOptions::default(),
    )
    .unwrap();
    assert!(
        layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text.contains("pie title Broken"))
    );
}

#[test]
fn diagram_admission_is_bounded_without_partial_charts() {
    let many: String = (0..65).map(|n| format!("\"Category {n}\": 1\n")).collect();
    for source in [
        format!("pie\n{many}"),
        format!("pie\n\"{}\": 1", "x".repeat(513)),
        format!("pie title {}\n\"A\": 1", "x".repeat(1025)),
        format!("pie\n\"A\": 1\n%%{}", "x".repeat(65536)),
        "pie\n\"bad\0label\": 1".into(),
    ] {
        assert!(render_diagram_svg(&source, "mermaid").is_none());
    }
    assert!(!is_diagram_code("piece of ordinary code", ""));
}

#[test]
fn pie_named_nodes_do_not_override_explicit_flowchart_and_sequence_languages() {
    assert!(
        render_diagram_svg("pie --> cake", "flowchart")
            .unwrap()
            .contains("fmd-flowchart")
    );
    assert!(
        render_diagram_svg("pie ->> cake: hi", "sequence")
            .unwrap()
            .contains("fmd-sequence")
    );
}
