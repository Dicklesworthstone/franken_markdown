//! Public diagram and publication regressions for graph ownership and geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfOptions, is_diagram_code, parse_markdown, render_diagram_svg, render_html,
    render_pdf,
};

fn attribute<'a>(tag: &'a str, key: &str) -> &'a str {
    tag.split_once(&format!("{key}=\""))
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
}

fn position(svg: &str, label: &str) -> (f32, f32) {
    let line = svg
        .lines()
        .find(|line| {
            line.contains("class=\"fmd-node-text\"") && line.ends_with(&format!(">{label}</text>"))
        })
        .unwrap();
    (
        attribute(line, "x").parse().unwrap(),
        attribute(line, "y").parse().unwrap(),
    )
}

fn nodes(svg: &str) -> usize {
    svg.matches("class=\"fmd-node-text\"").count()
}

fn edges(svg: &str) -> usize {
    svg.matches("class=\"fmd-edge-path\"").count()
}

#[test]
fn chained_and_grouped_edges_keep_every_node_and_dependency() {
    let svg = render_diagram_svg(
        "flowchart LR\nA[Start] -- first --> B[Process] -- second --> C[Done]",
        "mermaid",
    )
    .unwrap();
    assert_eq!(nodes(&svg), 3);
    assert_eq!(edges(&svg), 2);
    assert!(position(&svg, "Start").0 < position(&svg, "Process").0);
    assert!(position(&svg, "Process").0 < position(&svg, "Done").0);
    for label in ["first", "second"] {
        assert!(svg.contains(&format!(">{label}</text>")));
    }

    let grouped = render_diagram_svg("graph TD\nA & B-->C & D-->E", "mermaid").unwrap();
    assert_eq!(nodes(&grouped), 5);
    assert_eq!(
        edges(&grouped),
        6,
        "complete bipartite groups then two links to E"
    );
    for first in ["A", "B"] {
        for middle in ["C", "D"] {
            assert!(position(&grouped, first).1 < position(&grouped, middle).1);
            assert!(position(&grouped, middle).1 < position(&grouped, "E").1);
        }
    }
}

#[test]
fn quoted_labels_own_arrows_brackets_comments_and_statement_separators() {
    let source = r#"flowchart TD
A["Read A --> B; %% [x] {y} (z)"] -->|"yes | no; -->"| B["<script> & done"]
B -- "say --> or ---" --> C[End] %% ignored --> Z
"#;
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    assert_eq!(nodes(&svg), 3);
    assert_eq!(edges(&svg), 2);
    assert!(svg.contains(">Read A --&gt; B; %% [x] {y} (z)</text>"));
    assert!(svg.contains(">yes | no; --&gt;</text>"));
    assert!(svg.contains(">say --&gt; or ---</text>"));
    assert!(svg.contains(">&lt;script&gt; &amp; done</text>"));
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("ignored"));
}

#[test]
fn semicolon_statements_and_final_explicit_definitions_are_preserved() {
    let source = "%% heading\ngraph LR; A[Old]-->B; B-->C; A{Final}; A-->C;";
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    assert_eq!(nodes(&svg), 3);
    assert_eq!(edges(&svg), 3);
    assert!(svg.contains(">Final</text>"));
    assert!(!svg.contains(">Old</text>"));
    assert_eq!(svg.matches("<polygon ").count(), 1);
    assert!(is_diagram_code("graph; A-->B", ""));
    assert!(render_diagram_svg("graph; A-->B", "").is_some());
    assert!(render_diagram_svg("A-->B; B-->C", "flowchart").is_some());
    assert!(render_diagram_svg("pie-->cake", "flowchart").is_some());
}

#[test]
fn directions_reorder_nodes_and_point_edges_toward_their_destinations() {
    for (direction, horizontal, forward) in [
        ("TD", false, true),
        ("TB", false, true),
        ("BT", false, false),
        ("LR", true, true),
        ("RL", true, false),
    ] {
        let svg = render_diagram_svg(
            &format!("flowchart {direction}\nA[First]-->B[Second]-->C[Third]"),
            "mermaid",
        )
        .unwrap();
        let axis = |point: (f32, f32)| if horizontal { point.0 } else { point.1 };
        let a = axis(position(&svg, "First"));
        let b = axis(position(&svg, "Second"));
        let c = axis(position(&svg, "Third"));
        assert_eq!(a < b && b < c, forward, "{direction}: {svg}");
        assert_eq!(a > b && b > c, !forward, "{direction}: {svg}");
        for line in svg.lines().filter(|line| line.starts_with("<line ")) {
            let start: f32 = attribute(line, if horizontal { "x1" } else { "y1" })
                .parse()
                .unwrap();
            let end: f32 = attribute(line, if horizontal { "x2" } else { "y2" })
                .parse()
                .unwrap();
            assert_eq!(start < end, forward, "{direction}: {line}");
        }
    }
}

#[test]
fn joining_a_short_and_long_dependency_path_keeps_the_join_after_both() {
    let svg = render_diagram_svg("flowchart TD\nA-->C; A-->B; B-->C", "mermaid").unwrap();
    assert!(position(&svg, "A").1 < position(&svg, "B").1);
    assert!(position(&svg, "B").1 < position(&svg, "C").1);
}

#[test]
fn open_dotted_and_thick_links_keep_their_visual_meaning() {
    for (syntax, style, dashed) in [
        ("A---B", "marker-end:none", false),
        ("A-- open ---B", "marker-end:none", false),
        ("A==>B", "stroke-width:3", false),
        ("A== thick ==>B", "stroke-width:3", false),
        ("A===B", "marker-end:none;stroke-width:3", false),
        ("A-.->B", "", true),
        ("A-. dotted .->B", "", true),
        ("A-.-B", "marker-end:none", true),
    ] {
        let svg = render_diagram_svg(&format!("flowchart LR\n{syntax}"), "mermaid").unwrap();
        let edge = svg.lines().find(|line| line.starts_with("<line ")).unwrap();
        if !style.is_empty() {
            assert_eq!(attribute(edge, "style"), style, "{syntax}");
        }
        assert_eq!(
            edge.contains("stroke-dasharray=\"4,4\""),
            dashed,
            "{syntax}"
        );
    }
}

#[test]
fn circle_geometry_contains_its_label_and_stays_inside_the_canvas() {
    let svg =
        render_diagram_svg("graph TD\nA((Long circle label))-->B([Done])", "mermaid").unwrap();
    let circle = svg
        .lines()
        .find(|line| line.starts_with("<circle "))
        .unwrap();
    let radius: f32 = attribute(circle, "r").parse().unwrap();
    let cy: f32 = attribute(circle, "cy").parse().unwrap();
    assert!(
        radius > 60.0,
        "the old fixed 19px radius clipped this label"
    );
    assert!(cy - radius >= 0.0);
    assert!(position(&svg, "Done").1 > cy + radius);
}

#[test]
fn malformed_or_unsupported_statements_preserve_the_entire_source() {
    for suffix in [
        "B-->",
        "B[unterminated",
        "B[\"unterminated]",
        "B[Label] ignored",
        "B & --> C",
        "B-->||",
        "B-->C ::: red",
        "B[[Subroutine]]",
        "B[/Slanted/]",
        "subgraph Group\nB-->C\nend",
        "classDef blue fill:blue",
        "click B callback",
        "%%{init: {theme: 'dark'}}%%",
        "B@{ shape: cloud }",
    ] {
        let source = format!("flowchart TD\nA[Keep entire source]-->B\n{suffix}");
        assert!(render_diagram_svg(&source, "mermaid").is_none(), "{source}");
        let html = render_html(
            &format!("```mermaid\n{source}\n```"),
            &HtmlOptions::default(),
        )
        .unwrap();
        assert!(html.contains("language-mermaid"), "{source}");
        assert!(!html.contains("class=\"fmd-diagram-wrapper\""), "{source}");
        assert!(html.contains("Keep entire source"));
    }
    assert!(render_diagram_svg("flowchart SIDEWAYS\nA-->B", "mermaid").is_none());
}

#[test]
fn graph_size_and_fanout_are_bounded_before_drawing() {
    let group = |prefix: &str, count: usize| {
        (0..count)
            .map(|n| format!("{prefix}{n}"))
            .collect::<Vec<_>>()
            .join(" & ")
    };
    let exact = format!("graph TD\n{}-->{}", group("A", 32), group("B", 32));
    assert_eq!(edges(&render_diagram_svg(&exact, "mermaid").unwrap()), 1024);
    for source in [
        format!("graph TD\n{}-->{}", group("A", 33), group("B", 33)),
        format!(
            "graph TD\n{}",
            (0..257).map(|n| format!("A{n};")).collect::<String>()
        ),
        format!("graph TD\n{}-->B", "A".repeat(129)),
        format!("graph TD\nA[{}]-->B", "x".repeat(513)),
        format!("graph TD\nA-->B\n%%{}", "x".repeat(65536)),
        "graph TD\nA[bad\0label]-->B".into(),
    ] {
        assert!(render_diagram_svg(&source, "mermaid").is_none());
    }
}

#[test]
fn complete_graphs_reach_html_pdf_and_svg_and_fallback_remains_source() {
    let source = "flowchart RL; A[Start]-->B[Process]-->C[Done];";
    let markdown = format!("```mermaid\n{source}\n```\n");
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    let html = render_html(&markdown, &HtmlOptions::default()).unwrap();
    assert!(html.contains(&svg));
    let pdf = render_pdf(&markdown, &PdfOptions::default()).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&pdf).matches("/S /Figure").count(),
        1
    );
    assert_eq!(pdf, render_pdf(&markdown, &PdfOptions::default()).unwrap());
    let (poster, _, warnings) = franken_markdown::svg::render_svg_with_diagnostics(
        &parse_markdown(&markdown),
        &franken_markdown::svg::SvgOptions::default(),
    );
    assert!(
        String::from_utf8(poster)
            .unwrap()
            .contains("data:image/svg+xml;base64,")
    );
    assert!(warnings.is_empty(), "{warnings:?}");

    let unsupported = "```mermaid\nflowchart TD\nA-->B\nsubgraph KeepMe\nB-->C\nend\n```";
    let layer = franken_markdown::pdf::verification_text_layer(
        &parse_markdown(unsupported),
        &PdfOptions::default(),
    )
    .unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().all(|run| run.kind != "image"));
    assert!(runs.iter().any(|run| run.text.contains("KeepMe")));
}
