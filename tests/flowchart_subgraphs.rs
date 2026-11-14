//! Compound group ownership, geometry and complete-source admission.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::render_diagram_svg;
use std::collections::BTreeMap;

fn attribute<'a>(tag: &'a str, name: &str) -> &'a str {
    tag.split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
}

fn number(tag: &str, name: &str) -> f32 {
    attribute(tag, name).parse().unwrap()
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Bounds {
    fn rect(tag: &str) -> Self {
        Self {
            x: number(tag, "x"),
            y: number(tag, "y"),
            width: number(tag, "width"),
            height: number(tag, "height"),
        }
    }

    fn contains(self, other: Self) -> bool {
        self.x <= other.x + 0.11
            && self.y <= other.y + 0.11
            && self.x + self.width + 0.11 >= other.x + other.width
            && self.y + self.height + 0.11 >= other.y + other.height
    }

    fn overlaps(self, other: Self) -> bool {
        self.x + self.width > other.x + 0.11
            && other.x + other.width > self.x + 0.11
            && self.y + self.height > other.y + 0.11
            && other.y + other.height > self.y + 0.11
    }

    fn segment_inside(self, a: (f32, f32), b: (f32, f32)) -> bool {
        let (min_x, max_x) = (a.0.min(b.0), a.0.max(b.0));
        let (min_y, max_y) = (a.1.min(b.1), a.1.max(b.1));
        if (a.0 - b.0).abs() < 0.11 {
            a.0 > self.x + 0.11
                && a.0 < self.x + self.width - 0.11
                && max_y > self.y + 0.11
                && min_y < self.y + self.height - 0.11
        } else {
            a.1 > self.y + 0.11
                && a.1 < self.y + self.height - 0.11
                && max_x > self.x + 0.11
                && min_x < self.x + self.width - 0.11
        }
    }
}

fn scope<'a>(svg: &'a str, kind: &str, id: &str) -> &'a str {
    svg.lines()
        .find(|line| line.contains(&format!("data-{kind}-id=\"{id}\"")))
        .unwrap()
}

fn group(svg: &str, id: &str) -> Bounds {
    Bounds::rect(scope(svg, "subgraph", id).lines().next().unwrap())
}

fn node(svg: &str, id: &str) -> Bounds {
    let shape = scope(svg, "node", id).lines().next().unwrap();
    if shape.starts_with("<rect ") {
        Bounds::rect(shape)
    } else if shape.starts_with("<circle ") {
        let radius = number(shape, "r");
        Bounds {
            x: number(shape, "cx") - radius,
            y: number(shape, "cy") - radius,
            width: radius * 2.0,
            height: radius * 2.0,
        }
    } else {
        let coordinates: Vec<f32> = attribute(shape, "points")
            .split([',', ' '])
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        let xs: Vec<_> = coordinates.iter().step_by(2).copied().collect();
        let ys: Vec<_> = coordinates.iter().skip(1).step_by(2).copied().collect();
        let x = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let y = ys.iter().copied().fold(f32::INFINITY, f32::min);
        Bounds {
            x,
            y,
            width: xs.into_iter().fold(f32::NEG_INFINITY, f32::max) - x,
            height: ys.into_iter().fold(f32::NEG_INFINITY, f32::max) - y,
        }
    }
}

fn paths(svg: &str) -> Vec<Vec<(f32, f32)>> {
    svg.lines()
        .filter(|line| line.contains("class=\"fmd-edge-path\""))
        .map(|line| {
            let words: Vec<_> = attribute(line, "d").split_whitespace().collect();
            words
                .chunks_exact(3)
                .map(|point| {
                    assert!(matches!(point[0], "M" | "L"));
                    (point[1].parse().unwrap(), point[2].parse().unwrap())
                })
                .collect()
        })
        .collect()
}

fn descendant(owner: &str, group: &str, parents: &BTreeMap<&str, &str>) -> bool {
    let mut current = owner;
    loop {
        if current == group {
            return true;
        }
        let Some(parent) = parents.get(current) else {
            return false;
        };
        current = parent;
    }
}

#[test]
fn nested_subgraphs_keep_real_ownership_and_obstacle_free_relationships_in_every_direction() {
    let body = concat!(
        "Start[Start]\n",
        "subgraph Service[\"Service layer\"]\n",
        " A[API] --> B([Worker])\n",
        " subgraph Store[Persistence]\n C[Cache] --> D((Index))\n end\n",
        " B --> C\nend\n",
        "subgraph Client[Clients]\n E(Web) --> F{Choose}\nend\n",
        "Start --> E\nF -->|request| A\nB -.->|status| E\n",
        "D --> D\nC ==> A\nA --- F\nF -->|parallel| A\n"
    );
    let owners = BTreeMap::from([
        ("Start", ""),
        ("A", "Service"),
        ("B", "Service"),
        ("C", "Store"),
        ("D", "Store"),
        ("E", "Client"),
        ("F", "Client"),
    ]);
    let parents = BTreeMap::from([("Store", "Service")]);
    let edges = [
        ("A", "B"),
        ("C", "D"),
        ("B", "C"),
        ("E", "F"),
        ("Start", "E"),
        ("F", "A"),
        ("B", "E"),
        ("D", "D"),
        ("C", "A"),
        ("A", "F"),
        ("F", "A"),
    ];
    for direction in ["TD", "TB", "BT", "LR", "RL"] {
        let code = format!("flowchart {direction}\n{body}");
        let svg = render_diagram_svg(&code, "mermaid").unwrap();
        assert_eq!(svg, render_diagram_svg(&code, "mermaid").unwrap());
        let root = svg.lines().next().unwrap();
        let canvas = Bounds {
            x: 0.0,
            y: 0.0,
            width: number(root, "width"),
            height: number(root, "height"),
        };
        let groups = BTreeMap::from([
            ("Service", group(&svg, "Service")),
            ("Store", group(&svg, "Store")),
            ("Client", group(&svg, "Client")),
        ]);
        let nodes: BTreeMap<_, _> = owners.keys().map(|&id| (id, node(&svg, id))).collect();
        for (&id, &bounds) in &nodes {
            assert!(
                canvas.contains(bounds),
                "{direction}: node {id} outside canvas"
            );
            for (&group_id, &group_bounds) in &groups {
                if descendant(owners[id], group_id, &parents) {
                    assert!(
                        group_bounds.contains(bounds),
                        "{direction}: {id} outside {group_id}"
                    );
                    assert!(
                        bounds.y >= group_bounds.y + 32.0,
                        "{direction}: node covers group title"
                    );
                } else {
                    assert!(
                        !group_bounds.overlaps(bounds),
                        "{direction}: foreign node {id} inside {group_id}"
                    );
                }
            }
        }
        for (&a, &bounds) in &groups {
            assert!(canvas.contains(bounds));
            for (&b, &other) in &groups {
                if a != b && !descendant(a, b, &parents) && !descendant(b, a, &parents) {
                    assert!(
                        !bounds.overlaps(other),
                        "{direction}: sibling groups {a}, {b} overlap"
                    );
                }
            }
        }
        assert!(groups["Service"].contains(groups["Store"]));
        for (&a, &bounds) in &nodes {
            for (&b, &other) in &nodes {
                if a != b {
                    assert!(!bounds.overlaps(other), "overlapping {a}, {b}");
                }
            }
        }
        let routes = paths(&svg);
        assert_eq!(routes.len(), edges.len());
        let horizontal = matches!(direction, "LR" | "RL");
        let forward = matches!(direction, "TD" | "TB" | "LR");
        for (points, &(from, to)) in routes.iter().zip(&edges) {
            let port = |bounds: Bounds, outgoing: bool| {
                if horizontal {
                    (
                        bounds.x
                            + if forward == outgoing {
                                bounds.width
                            } else {
                                0.0
                            },
                        bounds.y + bounds.height / 2.0,
                    )
                } else {
                    (
                        bounds.x + bounds.width / 2.0,
                        bounds.y
                            + if forward == outgoing {
                                bounds.height
                            } else {
                                0.0
                            },
                    )
                }
            };
            for (actual, expected) in [
                (points[0], port(nodes[from], true)),
                (*points.last().unwrap(), port(nodes[to], false)),
            ] {
                assert!(
                    (actual.0 - expected.0).abs() < 0.11 && (actual.1 - expected.1).abs() < 0.11,
                    "{direction}: wrong port {from}->{to}"
                );
            }
            for &point in points {
                assert!(
                    point.0 >= 0.0
                        && point.1 >= 0.0
                        && point.0 <= canvas.width
                        && point.1 <= canvas.height
                );
            }
            for segment in points.windows(2) {
                assert!(
                    (segment[0].0 - segment[1].0).abs() < 0.11
                        || (segment[0].1 - segment[1].1).abs() < 0.11
                );
                for (&id, &bounds) in &nodes {
                    assert!(
                        !bounds.segment_inside(segment[0], segment[1]),
                        "{direction}: {from}->{to} crosses node {id}: {segment:?}"
                    );
                }
                for (&id, &bounds) in &groups {
                    if !descendant(owners[from], id, &parents)
                        && !descendant(owners[to], id, &parents)
                    {
                        assert!(
                            !bounds.segment_inside(segment[0], segment[1]),
                            "{direction}: {from}->{to} crosses foreign group {id}"
                        );
                    }
                }
            }
        }
        assert_ne!(
            routes[5], routes[10],
            "parallel relationships need distinct routes"
        );
        for line in svg
            .lines()
            .filter(|line| line.contains("class=\"fmd-edge-label-bg\""))
        {
            let caption = Bounds::rect(line);
            assert!(canvas.contains(caption));
            assert!(nodes.values().all(|node| !node.overlaps(caption)));
        }
        for label in [
            "Service layer",
            "Persistence",
            "Clients",
            "request",
            "status",
            "parallel",
        ] {
            assert!(svg.contains(&format!(">{label}</text>")), "missing {label}");
        }
        assert!(svg.contains("stroke-dasharray=\"4,4\""));
        assert!(svg.contains("style=\"stroke-width:3\""));
        assert!(svg.contains("style=\"marker-end:none\""));
    }
}

#[test]
fn nested_membership_and_later_cross_group_references_do_not_move_existing_nodes() {
    let code = concat!(
        "graph TD\nA[First]\nsubgraph Outer\nA\n",
        "subgraph Inner\nA --> B\nend\nC\nend\n",
        "subgraph Other\nA --> D\nend\nD --> A\nA[Final]\n"
    );
    let svg = render_diagram_svg(code, "mermaid").unwrap();
    assert!(group(&svg, "Inner").contains(node(&svg, "A")));
    assert!(group(&svg, "Inner").contains(node(&svg, "B")));
    assert!(group(&svg, "Outer").contains(node(&svg, "C")));
    assert!(!group(&svg, "Other").overlaps(node(&svg, "A")));
    assert!(group(&svg, "Other").contains(node(&svg, "D")));
    assert_eq!(svg.matches("data-node-id=\"A\"").count(), 1);
    assert!(svg.contains(">Final</text>"));
    assert!(!svg.contains(">First</text>"));
    assert_eq!(paths(&svg).len(), 3);
}

#[test]
fn named_titled_and_semicolon_groups_preserve_literal_labels() {
    let code = "%% diagram\nflowchart LR; subgraph Backend [\"<API> & storage; %% [x] -->\"]; A --> B; end; subgraphical --> ending;";
    let svg = render_diagram_svg(code, "mermaid").unwrap();
    assert!(svg.contains(">&lt;API&gt; &amp; storage; %% [x] --&gt;</text>"));
    assert!(!svg.contains("<API>"));
    assert!(svg.contains(">subgraphical</text>"));
    assert!(svg.contains(">ending</text>"));
    assert_eq!(
        svg,
        render_diagram_svg(&code.replace('\n', "\r\n"), "mermaid").unwrap()
    );
    assert!(render_diagram_svg("subgraph G; A --> B; end", "flowchart").is_some());
}

#[test]
fn unsupported_or_malformed_group_contracts_refuse_the_entire_graph() {
    for body in [
        "subgraph G\nA --> B\n",
        "end\nA --> B",
        "subgraph G\nA\nend\nend",
        "subgraph G\ndirection LR\nA --> B\nend",
        "subgraph G\nA --> B\nend\nG --> C",
        "G --> C\nsubgraph G\nA --> B\nend",
        "subgraph G\nG --> A\nend",
        "subgraph G\nA\nend\nsubgraph G\nB\nend",
        "subgraph G\nend\nA",
        "subgraph G[\"\"]\nA\nend",
        "subgraph G[\"unterminated]\nA\nend",
        "subgraph G[title] trailing\nA\nend",
        "subgraph title with spaces\nA\nend",
        "subgraph G\nA --> B\nend\nclassDef group fill:red",
        "subgraph G\nA --> B\nend\nG@{ view: collapsed }",
    ] {
        let code = format!("graph TD\nBefore[Keep entire source]\n{body}\nAfter");
        assert!(render_diagram_svg(&code, "mermaid").is_none(), "{code}");
    }
}

#[test]
fn subgraph_depth_count_and_canvas_work_are_bounded() {
    let nested = |depth| {
        format!(
            "graph TD\n{}A --> B\n{}",
            (0..depth)
                .map(|i| format!("subgraph G{i}\n"))
                .collect::<String>(),
            "end\n".repeat(depth)
        )
    };
    assert!(render_diagram_svg(&nested(16), "mermaid").is_some());
    assert!(render_diagram_svg(&nested(17), "mermaid").is_none());
    let groups = |count| {
        format!(
            "graph LR\n{}",
            (0..count)
                .map(|i| format!("subgraph G{i}\nN{i}\nend\n"))
                .collect::<String>()
        )
    };
    assert!(render_diagram_svg(&groups(64), "mermaid").is_some());
    assert!(render_diagram_svg(&groups(65), "mermaid").is_none());
    let code = format!(
        "graph TD\nsubgraph G\n{}{}end",
        (0..255)
            .map(|i| format!("N{i} --> N{}\n", i + 1))
            .collect::<String>(),
        "N0 --> N255\n".repeat(769)
    );
    assert!(code.len() < 64 * 1024);
    assert!(
        render_diagram_svg(&code, "mermaid").is_none(),
        "a million-coordinate canvas must preserve source instead of emitting unusable geometry"
    );
    assert!(render_diagram_svg("graph TD\nsubgraph Fresh\nA --> B\nend", "mermaid").is_some());
}
