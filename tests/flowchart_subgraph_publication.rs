//! Grouped architecture diagrams use the same vector scene in each output.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::dep_invalidation::FlowSession;
use franken_markdown::display::{AccessibleReadingRole, DisplayItem};
use franken_markdown::flow_display::{
    FlowDisplayLimits, FlowInlineStyle, FlowLayoutOptions, FlowTextRole,
};
use franken_markdown::fonts::BundledFlowFonts;
use franken_markdown::pdf::verification_text_layer;
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::{
    FontFamily, HtmlOptions, PdfOptions, parse_markdown, render_diagram_svg, render_html,
    render_pdf,
};

const CODE: &str = concat!(
    "flowchart LR\n",
    "subgraph Client[Client apps]\nUI[Browser]\nend\n",
    "subgraph Server[Service]\nAPI[API] --> Worker[Worker]\n",
    "subgraph Data[\"Data & storage\"]\nCache[Cache]\nend\n",
    "Worker --> Cache\nend\nUI -. request .-> API\nCache -->|reply| UI\n",
);

#[test]
fn widest_bold_group_titles_keep_actual_bundled_font_ink_inside_their_boxes() {
    use franken_markdown::text::Font;
    use std::collections::BTreeMap;

    let coordinate = |tag: &str, name: &str| -> f32 {
        tag.split_once(&format!("{name}=\""))
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0
            .parse()
            .unwrap()
    };
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let fonts = BundledFlowFonts::new(family).unwrap();
        let mut decoded = BTreeMap::<u64, Font>::new();
        for character in ["W", "M"] {
            for count in [60, 512] {
                let title = character.repeat(count);
                let run = fonts
                    .shape(
                        &title,
                        13.0,
                        FlowTextRole::Body,
                        FlowInlineStyle {
                            bold: true,
                            ..FlowInlineStyle::default()
                        },
                    )
                    .unwrap();
                let mut min_ink = f32::INFINITY;
                let mut max_ink = f32::NEG_INFINITY;
                for cluster in &run.clusters {
                    let mut pen = cluster.x_start;
                    for glyph in &run.glyphs[cluster.glyph_range.clone()] {
                        let font = decoded.entry(glyph.font_id.0).or_insert_with(|| {
                            Font::parse(fonts.font_bytes(glyph.font_id).unwrap().to_vec()).unwrap()
                        });
                        let bbox = font.glyph_bbox(glyph.glyph_id).unwrap();
                        let scale = 13.0 / f32::from(font.units_per_em);
                        min_ink = min_ink.min(pen + glyph.x_offset + f32::from(bbox[0]) * scale);
                        max_ink = max_ink.max(pen + glyph.x_offset + f32::from(bbox[2]) * scale);
                        pen += glyph.x_advance;
                    }
                }
                for direction in ["TD", "TB", "BT", "LR", "RL"] {
                    let code = format!("flowchart {direction}\nsubgraph G[\"{title}\"]\nA\nend");
                    let svg = render_diagram_svg(&code, "mermaid").unwrap();
                    let bounds = svg
                        .lines()
                        .find(|line| line.contains("data-subgraph-id=\"G\""))
                        .unwrap();
                    let label = svg
                        .lines()
                        .find(|line| line.contains("class=\"fmd-subgraph-title\""))
                        .unwrap();
                    let left = coordinate(label, "x") - run.total_advance / 2.0 + min_ink;
                    let right = coordinate(label, "x") - run.total_advance / 2.0 + max_ink;
                    assert!(
                        left >= coordinate(bounds, "x")
                            && right <= coordinate(bounds, "x") + coordinate(bounds, "width"),
                        "{family:?}, {direction}, {count} {character}: ink {left}..{right} outside {bounds}"
                    );
                }
            }
        }
    }
}

fn markdown(code: &str) -> String {
    format!("```mermaid\n{code}```\n")
}

#[test]
fn compound_flowcharts_reach_html_tagged_pdf_and_svg_as_complete_figures() {
    let source = markdown(CODE);
    let svg = render_diagram_svg(CODE, "mermaid").unwrap();
    let html = render_html(&source, &HtmlOptions::default()).unwrap();
    assert!(html.contains(&svg));
    assert!(!html.contains("language-mermaid"));
    assert!(!html.contains("<script"));
    for label in [
        "Client apps",
        "Service",
        "Data &amp; storage",
        "Browser",
        "Worker",
        "Cache",
        "request",
        "reply",
    ] {
        assert!(
            html.contains(&format!(">{label}</text>")),
            "missing {label}"
        );
    }

    let document = parse_markdown(&source);
    let options = PdfOptions::default();
    let bytes = render_pdf(&source, &options).unwrap();
    assert_eq!(bytes, render_pdf(&source, &options).unwrap());
    let pdf = String::from_utf8_lossy(&bytes);
    assert_eq!(pdf.matches("/S /Figure").count(), 1);
    assert!(pdf.contains("/Alt (Diagram)"));
    assert!(!pdf.contains("/Subtype /Image"), "groups stay vector");
    let layer = verification_text_layer(&document, &options).unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().any(|run| run.kind == "image"));
    assert!(!runs.iter().any(|run| run.text.contains("subgraph")));

    let (poster, _, warnings) = render_svg_with_diagnostics(&document, &SvgOptions::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    let poster = String::from_utf8(poster).unwrap();
    assert!(poster.contains("data:image/svg+xml;base64,"));
    assert!(poster.contains("aria-label=\"Diagram\""));
}

#[test]
fn flow_retains_group_labels_vector_routes_and_exact_accessible_source_at_narrow_widths() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for width in [240.0, 800.0] {
        let source = markdown(CODE);
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
        let diagrams: Vec<_> = display
            .reading_order()
            .iter()
            .filter(|node| node.role == AccessibleReadingRole::Diagram)
            .collect();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].text, CODE);
        assert!(diagrams[0].bounds.x >= 0.0 && diagrams[0].bounds.right() <= width + 0.01);
        let text: String = display
            .items()
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect();
        for expected in [
            "Client apps",
            "Service",
            "Data & storage",
            "Browser",
            "API",
            "Worker",
            "Cache",
            "request",
            "reply",
        ] {
            assert!(
                text.contains(expected),
                "missing native label {expected}: {text}"
            );
        }
        assert!(!text.contains("subgraph"));
        assert!(
            display
                .items()
                .iter()
                .any(|item| matches!(item, DisplayItem::Path(path) if path.stroke.is_some()))
        );
        assert!(session.pending_assets().is_empty());
    }
}

#[test]
fn unsupported_group_directions_and_endpoints_keep_the_entire_source_everywhere() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for code in [
        "flowchart TD\nsubgraph KeepMe\ndirection LR\nBefore --> After\nend\n",
        "flowchart TD\nsubgraph KeepMe\nBefore --> After\nend\nKeepMe --> Tail\n",
    ] {
        let source = markdown(code);
        let html = render_html(&source, &HtmlOptions::default()).unwrap();
        assert!(!html.contains("class=\"fmd-diagram fmd-flowchart\""));
        let code_html = html
            .split_once("<code class=\"language-mermaid\">")
            .unwrap()
            .1
            .split_once("</code>")
            .unwrap()
            .0;
        let mut visible = String::new();
        let mut rest = code_html;
        while let Some(start) = rest.find('<') {
            visible.push_str(&rest[..start]);
            let end = start + rest[start..].find('>').unwrap() + 1;
            let tag = &rest[start..end];
            assert!(tag == "</span>" || tag.starts_with("<span class=\"tok-"));
            rest = &rest[end..];
        }
        visible.push_str(rest);
        let visible = visible
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&");
        assert_eq!(visible, code, "no partial graph or missing source");

        let layer =
            verification_text_layer(&parse_markdown(&source), &PdfOptions::default()).unwrap();
        let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
        assert!(runs.iter().all(|run| run.kind != "image"));
        let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();
        for label in ["KeepMe", "Before", "After"] {
            assert!(text.contains(label));
        }
        let session = FlowSession::new(
            &source,
            8,
            FlowDisplayLimits::default(),
            FlowLayoutOptions::default(),
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap();
        assert!(
            session
                .display()
                .reading_order()
                .iter()
                .any(|node| node.role == AccessibleReadingRole::Diagram && node.text == code)
        );
    }
}
