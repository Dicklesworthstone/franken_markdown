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

#[derive(Clone, Copy, Debug)]
struct Bounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

fn number(tag: &str, key: &str) -> f32 {
    attribute(tag, key).parse().unwrap()
}

fn node_bounds(svg: &str) -> Vec<Bounds> {
    svg.lines()
        .filter(|line| {
            ["fmd-node-rect", "fmd-node-rounded", "fmd-node-diamond"]
                .iter()
                .any(|class| line.contains(&format!("class=\"{class}\"")))
        })
        .map(|line| {
            if line.starts_with("<circle ") {
                let radius = number(line, "r");
                Bounds {
                    left: number(line, "cx") - radius,
                    top: number(line, "cy") - radius,
                    right: number(line, "cx") + radius,
                    bottom: number(line, "cy") + radius,
                }
            } else if line.starts_with("<polygon ") {
                let points: Vec<f32> = attribute(line, "points")
                    .split([',', ' '])
                    .map(|value| value.parse().unwrap())
                    .collect();
                Bounds {
                    left: points
                        .iter()
                        .step_by(2)
                        .copied()
                        .fold(f32::INFINITY, f32::min),
                    top: points
                        .iter()
                        .skip(1)
                        .step_by(2)
                        .copied()
                        .fold(f32::INFINITY, f32::min),
                    right: points
                        .iter()
                        .step_by(2)
                        .copied()
                        .fold(f32::NEG_INFINITY, f32::max),
                    bottom: points
                        .iter()
                        .skip(1)
                        .step_by(2)
                        .copied()
                        .fold(f32::NEG_INFINITY, f32::max),
                }
            } else {
                Bounds {
                    left: number(line, "x"),
                    top: number(line, "y"),
                    right: number(line, "x") + number(line, "width"),
                    bottom: number(line, "y") + number(line, "height"),
                }
            }
        })
        .collect()
}

fn route_points(tag: &str) -> Vec<(f32, f32)> {
    if tag.starts_with("<line ") {
        return vec![
            (number(tag, "x1"), number(tag, "y1")),
            (number(tag, "x2"), number(tag, "y2")),
        ];
    }
    let path = attribute(tag, "d");
    assert!(
        path.chars()
            .all(|c| !c.is_ascii_alphabetic() || matches!(c, 'M' | 'L')),
        "return-route probe expects straight segments: {path}"
    );
    let numbers: Vec<f32> = path
        .split([' ', ','])
        .filter(|value| !value.is_empty() && !matches!(*value, "M" | "L"))
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(numbers.len() % 2, 0);
    numbers
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

fn assert_return_route_clear(svg: &str, route: &str) {
    let points = route_points(route);
    let view: Vec<f32> = attribute(svg.lines().next().unwrap(), "viewBox")
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    for &(x, y) in &points {
        assert!(x.is_finite() && y.is_finite());
        assert!(
            x >= 0.0 && y >= 0.0 && x <= view[2] && y <= view[3],
            "{route}"
        );
    }
    // Test the actual emitted segments against all node interiors, not a
    // renderer helper or a control-point approximation to a curved path.
    for pair in points.windows(2) {
        let ((x1, y1), (x2, y2)) = (pair[0], pair[1]);
        assert!(x1 == x2 || y1 == y2, "{route}");
        for node in node_bounds(svg) {
            let crosses = if x1 == x2 {
                x1 > node.left + 0.1
                    && x1 < node.right - 0.1
                    && y1.max(y2) > node.top + 0.1
                    && y1.min(y2) < node.bottom - 0.1
            } else {
                y1 > node.top + 0.1
                    && y1 < node.bottom - 0.1
                    && x1.max(x2) > node.left + 0.1
                    && x1.min(x2) < node.right - 0.1
            };
            assert!(!crosses, "connection crosses {node:?}: {route}");
        }
    }
}

#[test]
fn self_loops_leave_and_return_to_every_shape_without_crossing_its_interior() {
    for direction in ["TD", "BT", "LR", "RL"] {
        for node in [
            "A[Retry]",
            "A(Retry)",
            "A([Retry])",
            "A{Retry}",
            "A((Retry))",
        ] {
            let source = format!("flowchart {direction}\n{node}-->|Again|A");
            let svg = render_diagram_svg(&source, "mermaid").unwrap();
            let route = svg
                .lines()
                .find(|line| line.contains("class=\"fmd-edge-path\""))
                .unwrap();
            assert_return_route_clear(&svg, route);
            let points = route_points(route);
            assert!(
                points.len() >= 4,
                "a visible loop needs an external route: {svg}"
            );
            assert_ne!(points.first(), points.last());
            let node = node_bounds(&svg)[0];
            let start = *points.first().unwrap();
            let end = *points.last().unwrap();
            match direction {
                "TD" => assert!(start.1 == node.bottom && end.1 == node.top),
                "BT" => assert!(start.1 == node.top && end.1 == node.bottom),
                "LR" => assert!(start.0 == node.right && end.0 == node.left),
                "RL" => assert!(start.0 == node.left && end.0 == node.right),
                _ => unreachable!(),
            }
            assert!(svg.contains(">Again</text>"));
            assert_eq!(svg, render_diagram_svg(&source, "mermaid").unwrap());
        }
    }
}

#[test]
fn feedback_connections_bypass_nodes_in_each_direction_and_keep_their_styles() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let source = format!(
            "flowchart {direction}\nA[Start]-->B{{Inspect}}-->C((Done)); C-. Retry .->A; C== Resume ===B; A-- Again -->A"
        );
        let svg = render_diagram_svg(&source, "mermaid").unwrap();
        let routes: Vec<_> = svg
            .lines()
            .filter(|line| line.contains("class=\"fmd-edge-path\""))
            .collect();
        assert_eq!(routes.len(), 5);
        let mut rails = Vec::new();
        for route in &routes[2..] {
            assert_return_route_clear(&svg, route);
            let points = route_points(route);
            rails.push(
                points
                    .iter()
                    .map(|&(x, y)| {
                        if matches!(direction, "TD" | "BT") {
                            x
                        } else {
                            y
                        }
                    })
                    .fold(0.0, f32::max),
            );
        }
        assert_ne!(
            rails[0], rails[1],
            "parallel returns must remain distinguishable"
        );
        assert_ne!(rails[0], rails[2]);
        assert_ne!(rails[1], rails[2]);
        assert!(routes[2].contains("stroke-dasharray=\"4,4\""));
        assert!(!routes[2].contains("marker-end:none"));
        assert!(routes[3].contains("style=\"marker-end:none;stroke-width:3\""));
        for label in ["Retry", "Resume", "Again"] {
            assert!(svg.contains(&format!(">{label}</text>")));
        }
    }
}

#[test]
fn long_return_labels_and_their_routes_remain_inside_the_actual_svg_canvas() {
    let label = "W".repeat(512);
    for direction in ["TD", "BT", "LR", "RL"] {
        let source = format!("flowchart {direction}\nA[Retry]-->|{label}|A");
        let svg = render_diagram_svg(&source, "mermaid").unwrap();
        let view: Vec<f32> = attribute(svg.lines().next().unwrap(), "viewBox")
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert!(
            view[2] < 10000.0 && view[3] < 10000.0,
            "bounded single-loop canvas"
        );
        let label_box = svg
            .lines()
            .find(|line| line.contains("class=\"fmd-edge-label-bg\""))
            .unwrap();
        let left = number(label_box, "x");
        let top = number(label_box, "y");
        let right = left + number(label_box, "width");
        let bottom = top + number(label_box, "height");
        assert!(
            left >= 0.0 && top >= 0.0 && right <= view[2] && bottom <= view[3],
            "{svg}"
        );
        for node in node_bounds(&svg) {
            assert!(
                right <= node.left
                    || left >= node.right
                    || bottom <= node.top
                    || top >= node.bottom,
                "label is hidden under its node"
            );
        }
        let route = svg
            .lines()
            .find(|line| line.contains("class=\"fmd-edge-path\""))
            .unwrap();
        assert_return_route_clear(&svg, route);
        assert!(svg.contains(&format!(">{label}</text>")));
    }
}

#[test]
fn forward_connections_skipping_layers_bypass_intervening_nodes() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let source = format!(
            "flowchart {direction}\nA[Start]-->B{{Inspect}}-->C((Done)); A-. Skip inspection .->C"
        );
        let svg = render_diagram_svg(&source, "mermaid").unwrap();
        let routes: Vec<_> = svg
            .lines()
            .filter(|line| line.contains("class=\"fmd-edge-path\""))
            .collect();
        assert_eq!(routes.len(), 3);
        assert_return_route_clear(&svg, routes[2]);
        assert!(routes[2].contains("stroke-dasharray=\"4,4\""));
        assert!(!routes[2].contains("marker-end:none"));
        assert!(svg.contains(">Skip inspection</text>"));
    }
}

#[test]
fn forward_captions_fit_the_canvas_and_do_not_cover_their_endpoint_nodes() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let label = "W".repeat(128);
        let source = format!("flowchart {direction}\nA[Source]-->|{label}|B[Destination]");
        let svg = render_diagram_svg(&source, "mermaid").unwrap();
        let view: Vec<f32> = attribute(svg.lines().next().unwrap(), "viewBox")
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        let caption = svg
            .lines()
            .find(|line| line.contains("class=\"fmd-edge-label-bg\""))
            .unwrap();
        let (left, top) = (number(caption, "x"), number(caption, "y"));
        let (right, bottom) = (
            left + number(caption, "width"),
            top + number(caption, "height"),
        );
        assert!(left >= 0.0 && top >= 0.0 && right <= view[2] && bottom <= view[3]);
        for node in node_bounds(&svg) {
            assert!(
                right <= node.left
                    || left >= node.right
                    || bottom <= node.top
                    || top >= node.bottom,
                "{direction}: caption covers endpoint {node:?}"
            );
        }
        assert!(svg.contains(&format!(">{label}</text>")));
    }
}
