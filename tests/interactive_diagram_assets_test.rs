//! Published diagram data must match the actual vector image shown on first open.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::{
    HtmlOptions, html::base64_encode, parse_markdown, render_diagram_svg, render_html,
    render_interactive_html,
};

const MANIFEST: &str = "<script type=\"application/json\" id=\"fmd-diagram-assets\">";

fn workspace(source: &str) -> String {
    render_interactive_html(&parse_markdown(source), source, &HtmlOptions::default())
}

#[test]
fn duplicate_and_nested_diagrams_publish_one_exact_script_safe_binding() {
    let code = "graph TD\n%% </ScRiPt>\naccTitle: Café &lt;/script&gt;\nA-->B\n";
    let fence = format!("~~~mermaid\n{code}~~~");
    let quoted = fence
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!("{fence}\n\n{quoted}\n\n~~~mermaid\nnot a valid diagram\n~~~\n");
    let svg = render_diagram_svg(code, "mermaid").expect("valid accessible flowchart");
    let encoded = base64_encode(svg.as_bytes());
    let html = workspace(&source);
    let manifest = html
        .split_once(MANIFEST)
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;

    // Both vector figures are present in the initial preview, but edits only
    // need one language/source key. The malformed fence keeps its code fallback.
    assert_eq!(html.matches(&svg).count(), 2);
    assert!(html.contains("<pre><code class=\"language-mermaid\">"));
    let source_field =
        r#"graph TD\u000a%% \u003c/ScRiPt>\u000aaccTitle: Café &lt;/script&gt;\u000aA-->B\u000a"#;
    let alt_field = r#"Café \u003c/script>"#;
    assert_eq!(
        manifest,
        format!(r#"[["mermaid","{source_field}","{alt_field}","{encoded}"]]"#)
    );
    assert!(!manifest.contains('<'));
    assert_eq!(html.matches(MANIFEST).count(), 1);

    let ordinary = render_html(&source, &HtmlOptions::default()).unwrap();
    assert_eq!(ordinary.matches(&svg).count(), 2);
    assert!(!ordinary.contains(MANIFEST));
    assert!(!ordinary.contains(&encoded));
}

#[test]
fn an_ordinary_or_rejected_fence_does_not_create_a_manifest() {
    for source in [
        "~~~rust\nlet x = 1;\n~~~\n",
        "~~~mermaid\nno such diagram syntax\n~~~\n",
        "Just text.\n",
    ] {
        assert!(!workspace(source).contains(MANIFEST), "{source}");
    }
}
