#![forbid(unsafe_code)]

//! Integration tests for renderer-neutral math and first-party diagram display (FCB-034.A).
//!
//! Contract:
//! - Uses `fmd-math` and qualified first-party vector forms with glyph/path provenance.
//! - No external TeX engine, no JavaScript, no ambient scripting.
//! - Diagram output flows through qualified first-party vector parsing/geometry,
//!   not executing embedded script or arbitrary markup.
//! - Unsupported elements display a source-preserving fallback and a concise capability explanation.
//! - Source anchors survive the bridge to the display list.

use franken_markdown::display::{
    DisplayItem, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};
use franken_markdown::math_display::{
    contains_hostile_markup, diagram_anchor, diagram_to_display,
    diagram_to_display_with_fallback, extract_diagram_blocks, extract_math_spans,
    math_anchor, math_to_display, DiagramError,
};
use fmd_math::Engine;

fn bundled_engine() -> Engine {
    Engine::bundled().expect("bundled math engine loads successfully")
}

#[test]
fn math_display_typesets_and_preserves_source_spans() {
    let engine = bundled_engine();
    let source = "\\frac{a + 1}{b - 1}";
    let offset = 120;
    let items = math_to_display(source, &engine, 50.0, 100.0, 16.0, offset)
        .expect("valid math expression typesets");

    assert!(!items.is_empty(), "items must be generated");

    // Must contain both vector rules (fraction bar) and text glyphs
    let has_vector = items.iter().any(|i| match i {
        DisplayItem::Vector(v) => {
            v.shape == VectorShapeType::HorizontalRule && v.color_role == "math"
        }
        _ => false,
    });
    assert!(has_vector, "fraction bar must be a vector rule");

    let glyph_count = items
        .iter()
        .filter(|i| matches!(i, DisplayItem::Text(_)))
        .count();
    assert!(glyph_count >= 6, "must have text runs for a, +, 1, b, -, 1");

    // Verify all source spans are contained within the math source slice
    for item in &items {
        let span = item.source_span();
        assert!(
            span.start >= offset,
            "span start {} must be >= offset {}",
            span.start,
            offset
        );
        assert!(
            span.end <= offset + source.len() + 1,
            "span end {} must be <= max {}",
            span.end,
            offset + source.len() + 1
        );
        let b = item.bounds();
        assert!(b.width > 0.0 && b.height > 0.0, "finite positive bounds");
    }
}

#[test]
fn math_semantic_anchor_bounds_formula_region() {
    let engine = bundled_engine();
    let source = "x^2 + y^2 = z^2";
    let offset = 42;
    let layout = engine
        .typeset(source, fmd_math::Style::Display)
        .expect("typesets");
    let anchor = math_anchor(source, &layout, 20.0, 30.0, 14.0, offset);

    assert_eq!(anchor.anchor_id, format!("math-{offset}"));
    assert!(!anchor.is_heading);
    assert_eq!(anchor.source_span.start, offset);
    assert_eq!(anchor.source_span.end, offset + source.len());
    assert!(anchor.bounds.width > 0.0);
    assert!(anchor.bounds.height > 0.0);
}

#[test]
fn math_hostile_expansion_is_bounded_and_safe() {
    let engine = bundled_engine();
    let hostile_inputs = [
        "",
        "\\\\",
        "{}",
        "\\frac{}{}",
        "\\frac{\\frac{\\frac{\\frac{1}{2}}{3}}{4}}{5}",
        "x_{y_{z_{w}}}",
        "\\sqrt{\\sqrt{\\sqrt{x}}}",
        "𝕸𝖆𝖙𝖍 𝔘𝔫𝔦𝔠𝔬𝔡𝔢 𝕊𝕪𝕞𝖇𝕠𝕝𝕤 ∑ ∏ ∫",
        "$#%^&*()",
    ];

    for hostile in hostile_inputs {
        let res = math_to_display(hostile, &engine, 0.0, 0.0, 16.0, 0);
        if let Ok(items) = res {
            for item in &items {
                let b = item.bounds();
                assert!(b.x.is_finite());
                assert!(b.y.is_finite());
                assert!(b.width.is_finite() && b.width >= 0.0);
                assert!(b.height.is_finite() && b.height >= 0.0);
            }
        }
    }
}

#[test]
fn diagram_flowchart_vector_layout_produces_nodes_and_edges() {
    let source = "graph TD\n  Client[Client App] --> Server[Core Server]\n  Server --> DB[(Database)]";
    let offset = 500;
    let items = diagram_to_display("mermaid", source, 20.0, 30.0, 14.0, offset)
        .expect("flowchart parses cleanly");

    let node_boxes: Vec<&DisplayVectorPath> = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Vector(v) if v.shape == VectorShapeType::DiagramBox => Some(v),
            _ => None,
        })
        .collect();

    let connectors: Vec<&DisplayVectorPath> = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Vector(v) if v.shape == VectorShapeType::DiagramConnector => Some(v),
            _ => None,
        })
        .collect();

    let arrows: Vec<&DisplayVectorPath> = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Vector(v) if v.shape == VectorShapeType::DiagramArrow => Some(v),
            _ => None,
        })
        .collect();

    let text_labels: Vec<&DisplayTextRun> = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Text(t) if t.color_role == "diagram-text" => Some(t),
            _ => None,
        })
        .collect();

    assert_eq!(node_boxes.len(), 3, "must emit 3 node box vectors");
    assert_eq!(connectors.len(), 2, "must emit 2 connector lines");
    assert_eq!(arrows.len(), 2, "must emit 2 arrowheads");
    assert_eq!(text_labels.len(), 3, "must emit 3 text labels");

    // All source spans must point into original source
    for item in &items {
        let span = item.source_span();
        assert!(span.start >= offset);
        assert!(span.end <= offset + source.len());
    }

    let anchor = diagram_anchor(source, &items, 20.0, 30.0, offset);
    assert_eq!(anchor.anchor_id, format!("diagram-{offset}"));
    assert!(anchor.bounds.width > 0.0);
    assert!(anchor.bounds.height > 0.0);
}

#[test]
fn diagram_undirected_connector_flowchart() {
    let source = "flowchart LR\n  Alpha --- Beta\n  Beta --- Gamma";
    let items = diagram_to_display("flowchart", source, 0.0, 0.0, 16.0, 0)
        .expect("undirected flowchart parses");

    let arrows = items
        .iter()
        .filter(|i| match i {
            DisplayItem::Vector(v) => v.shape == VectorShapeType::DiagramArrow,
            _ => false,
        })
        .count();
    assert_eq!(arrows, 0, "undirected flowchart has no arrowheads");

    let connectors = items
        .iter()
        .filter(|i| match i {
            DisplayItem::Vector(v) => v.shape == VectorShapeType::DiagramConnector,
            _ => false,
        })
        .count();
    assert_eq!(connectors, 2, "must emit 2 connectors");
}

#[test]
fn diagram_hostile_script_rejected_as_negative_control() {
    // Negative control: Embedded script/markup must NEVER be evaluated or parsed into vectors
    let hostile_scripts = [
        "graph TD\n  A[<script>document.location='http://evil.com'</script>] --> B",
        "flowchart TD\n  A --> B\n  <iframe src='bad.html'></iframe>",
        "diagram\n  A[onclick=alert(1)] --> B",
        "graph LR\n  A[<foreignObject>bad</foreignObject>] --> B",
        "javascript:alert(1)",
    ];

    for script in hostile_scripts {
        assert!(
            contains_hostile_markup(script),
            "hostile markup detector must catch {script}"
        );
        let res = diagram_to_display("mermaid", script, 0.0, 0.0, 14.0, 0);
        assert!(
            matches!(res, Err(DiagramError::HostileContent(_))),
            "vector parser must refuse hostile markup"
        );

        // Fallback display must safely present source without execution
        let fallback = diagram_to_display_with_fallback("mermaid", script, 0.0, 0.0, 14.0, 0);
        assert!(!fallback.is_empty());
        let has_note = fallback.iter().any(|i| match i {
            DisplayItem::Text(t) => t.color_role == "diagram-fallback-note",
            _ => false,
        });
        assert!(has_note, "must present capability note");
    }
}

#[test]
fn diagram_unsupported_language_falls_back_to_source_with_explanation() {
    let source = "title Sequence\nAlice->Bob: Authentication Request";
    let res = diagram_to_display("plantuml", source, 10.0, 20.0, 14.0, 100);
    assert!(
        matches!(res, Err(DiagramError::UnsupportedLanguage(_))),
        "plantuml returns typed unsupported error"
    );

    let fallback = diagram_to_display_with_fallback("plantuml", source, 10.0, 20.0, 14.0, 100);
    assert!(!fallback.is_empty(), "fallback items rendered");

    // Verify fallback contains note explaining dialect is not natively supported
    let note = fallback
        .iter()
        .find(|i| match i {
            DisplayItem::Text(t) => t.color_role == "diagram-fallback-note",
            _ => false,
        })
        .expect("note must exist");
    assert!(
        note.source_span().start >= 100,
        "note source span must preserve offset"
    );

    // Verify source lines are preserved
    let text_lines: Vec<&DisplayTextRun> = fallback
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Text(t) if t.color_role == "diagram-fallback-text" => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(text_lines.len(), 2);
    assert_eq!(text_lines[0].text, "title Sequence");
    assert_eq!(text_lines[1].text, "Alice->Bob: Authentication Request");
}

#[test]
fn extract_math_and_diagram_blocks_from_markdown() {
    let document = "\
# Document with Math and Diagrams

Here is inline math $E = mc^2$ and a formula:

$$\\int_0^1 x dx = \\frac{1}{2}$$

Below is an architecture diagram:

```mermaid
graph TD
  A[Reader] --> B[Parser]
  B --> C[DisplayList]
```

And unsupported diagram:

```d2
x -> y
```
";
    let math_spans = extract_math_spans(document);
    assert_eq!(math_spans.len(), 2);
    assert_eq!(math_spans[0].0, "E = mc^2");
    assert_eq!(math_spans[1].0, "\\int_0^1 x dx = \\frac{1}{2}");

    let diagram_blocks = extract_diagram_blocks(document);
    assert_eq!(diagram_blocks.len(), 2);
    assert_eq!(diagram_blocks[0].language, "mermaid");
    assert!(diagram_blocks[0].source.contains("graph TD"));
    assert_eq!(diagram_blocks[1].language, "d2");
    assert!(diagram_blocks[1].source.contains("x -> y"));
}
