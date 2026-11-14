//! Class-diagram grammar, relationship semantics and measured scene regressions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{is_diagram_code, render_diagram_svg};

fn render(source: &str) -> String {
    render_diagram_svg(source, "mermaid").unwrap_or_else(|| panic!("diagram refused:\n{source}"))
}

fn attr<'a>(element: &'a str, name: &str) -> &'a str {
    element
        .split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
}

fn number(element: &str, name: &str) -> f32 {
    attr(element, name).parse().unwrap()
}

fn elements<'a>(svg: &'a str, class: &str) -> Vec<&'a str> {
    svg.lines()
        .filter(|line| line.contains(&format!("class=\"{class}\"")))
        .collect()
}

fn content(element: &str) -> &str {
    element.split_once('>').unwrap().1.split_once("</").unwrap().0
}

fn rect(element: &str) -> [f32; 4] {
    let x = number(element, "x");
    let y = number(element, "y");
    [x, y, x + number(element, "width"), y + number(element, "height")]
}

fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

#[test]
fn declared_implicit_and_aliased_classes_keep_members_and_compartments() {
    let source = concat!(
        "classDiagram\n",
        "Animal <|-- Duck\n",
        "class Animal[\"Living record\"]\n",
        "Animal : +int age\n",
        "Animal: +isMammal() bool\n",
        "class Duck {\n",
        "  +String beakColor\n",
        "  #swim(speed) void\n",
        "  ~quack()\n",
        "}\n",
    );
    let svg = render(source);
    assert!(svg.starts_with("<svg "));
    assert!(svg.contains("class=\"fmd-diagram fmd-class\""));
    assert!(svg.contains("<title>Class diagram</title>"));
    assert_eq!(elements(&svg, "fmd-class-node").len(), 2);
    assert_eq!(elements(&svg, "fmd-class-divider").len(), 4);
    assert_eq!(elements(&svg, "fmd-class-relation").len(), 1);
    assert_eq!(elements(&svg, "fmd-class-inheritance").len(), 1);
    for text in [
        "Living record", "Duck", "+int age", "+isMammal() bool",
        "+String beakColor", "#swim(speed) void", "~quack()",
    ] {
        assert!(svg.contains(&format!(">{text}</text>")), "{text}\n{svg}");
    }
    assert!(!svg.contains(">Animal</text>"), "the late alias owns the title");
    assert_eq!(svg, render(source));
    assert_eq!(svg, render(&source.replace('\n', "\r\n")));

    let dividers = elements(&svg, "fmd-class-divider");
    let members = elements(&svg, "fmd-class-member");
    let age = members.iter().find(|line| content(line) == "+int age").unwrap();
    let method = members.iter().find(|line| content(line) == "+isMammal() bool").unwrap();
    assert!(number(age, "y") > number(dividers[0], "y1"));
    assert!(number(age, "y") < number(dividers[1], "y1"));
    assert!(number(method, "y") > number(dividers[1], "y1"));
}

#[test]
fn class_block_and_colon_members_have_identical_rendered_meaning() {
    let block = "classDiagram\nclass Account {\n+String owner\n+deposit(amount) bool\n}\n";
    let colon = "classDiagram\nclass Account\nAccount: +String owner\nAccount: +deposit(amount) bool\n";
    assert_eq!(render(block), render(colon));
    assert_eq!(
        render("classDiagram\nclass A { +name }\n"),
        render("classDiagram\nclass A\nA: +name\n")
    );
    assert_eq!(
        render("classDiagram\nclass A {}\n"),
        render("classDiagram\nclass A\n")
    );
}

#[test]
fn stereotypes_static_fields_and_abstract_methods_remain_meaningful() {
    let sources = [
        "classDiagram\nclass Shape <<interface>>\nShape: +draw()*\nShape: +int count$\n",
        "classDiagram\nclass Shape\n<<interface>> Shape\nShape: +draw()*\nShape: +int count$\n",
        "classDiagram\nclass Shape {\n<<interface>>\n+draw()*\n+int count$\n}\n",
    ];
    let svg = render(sources[0]);
    for source in sources {
        assert_eq!(svg, render(source));
    }
    assert!(svg.contains(">&lt;&lt;interface&gt;&gt;</text>"));
    assert!(svg.contains(">+draw()*</text>"), "the abstract classifier stays visible");
    assert!(svg.contains(">+int count</text>"));
    assert_eq!(elements(&svg, "fmd-class-static").len(), 1, "static field has an underline");
    assert!(!svg.contains("font-style:"), "Flow's existing body/bold text profile is preserved");
}

#[test]
fn every_standard_relationship_draws_the_correct_endpoint_and_line() {
    for (operator, endpoint, dashed) in [
        ("<|--", Some("fmd-class-inheritance"), false),
        ("--|>", Some("fmd-class-inheritance"), false),
        ("*--", Some("fmd-class-composition"), false),
        ("--*", Some("fmd-class-composition"), false),
        ("o--", Some("fmd-class-aggregation"), false),
        ("--o", Some("fmd-class-aggregation"), false),
        ("-->", Some("fmd-class-association"), false),
        ("<--", Some("fmd-class-association"), false),
        ("--", None, false),
        ("..>", Some("fmd-class-association"), true),
        ("<..", Some("fmd-class-association"), true),
        ("..|>", Some("fmd-class-inheritance"), true),
        ("<|..", Some("fmd-class-inheritance"), true),
        ("..", None, true),
    ] {
        let svg = render(&format!("classDiagram\nA {operator} B: relation\n"));
        assert_eq!(elements(&svg, "fmd-class-node").len(), 2, "{operator}");
        let paths = elements(&svg, "fmd-class-relation");
        assert_eq!(paths.len(), 1, "{operator}");
        assert_eq!(paths[0].contains("stroke-dasharray=\"5,4\""), dashed, "{operator}");
        for class in [
            "fmd-class-inheritance", "fmd-class-composition",
            "fmd-class-aggregation", "fmd-class-association",
        ] {
            assert_eq!(
                elements(&svg, class).len(),
                usize::from(endpoint == Some(class)),
                "{operator} must preserve its relationship type: {svg}"
            );
        }
        assert!(svg.contains(">relation</text>"));
    }
    let both = render("classDiagram\nA <|--|> B\n");
    assert_eq!(elements(&both, "fmd-class-inheritance").len(), 2);
    let mixed = render("classDiagram\nA o..* B\n");
    assert_eq!(elements(&mixed, "fmd-class-aggregation").len(), 1);
    assert_eq!(elements(&mixed, "fmd-class-composition").len(), 1);
    assert!(elements(&mixed, "fmd-class-relation")[0].contains("stroke-dasharray"));
}


#[test]
fn spaced_class_names_starting_with_o_are_not_consumed_as_aggregation_markers() {
    for (operator, name) in [("--", "order"), ("..", "object"), ("<--", "owner"), ("<|--", "orange")] {
        let svg = render(&format!("classDiagram\nA {operator} {name}\n"));
        assert_eq!(elements(&svg, "fmd-class-node").len(), 2);
        assert!(svg.contains(&format!(">{name}</text>")), "{svg}");
        assert!(elements(&svg, "fmd-class-aggregation").is_empty(), "{svg}");
    }
    let svg = render("classDiagram\nA --o order\n");
    assert!(svg.contains(">order</text>"));
    assert_eq!(elements(&svg, "fmd-class-aggregation").len(), 1);
}

#[test]
fn endpoint_shapes_touch_their_authored_class_in_each_direction() {
    for direction in ["TD", "TB", "BT", "LR", "RL"] {
        for (operator, owner) in [("<|--", 0), ("--|>", 1)] {
            let svg = render(&format!("classDiagram\ndirection {direction}\nParent {operator} Child\n"));
            let nodes = elements(&svg, "fmd-class-node");
            let a = rect(nodes[0]);
            let b = rect(nodes[1]);
            let horizontal = matches!(direction, "LR" | "RL");
            let forward = matches!(direction, "TD" | "TB" | "LR");
            if horizontal {
                assert_eq!(a[0] < b[0], forward, "{direction}");
            } else {
                assert_eq!(a[1] < b[1], forward, "{direction}");
            }
            let head = elements(&svg, "fmd-class-inheritance")[0];
            let tip: Vec<f32> = attr(head, "points")
                .split_whitespace().next().unwrap()
                .split(',').map(|value| value.parse().unwrap()).collect();
            let node = rect(nodes[owner]);
            let expected = match (direction, owner) {
                ("LR", 0) | ("RL", 1) => [node[2], (node[1] + node[3]) / 2.0],
                ("LR", 1) | ("RL", 0) => [node[0], (node[1] + node[3]) / 2.0],
                ("TD" | "TB", 0) | ("BT", 1) => [(node[0] + node[2]) / 2.0, node[3]],
                _ => [(node[0] + node[2]) / 2.0, node[1]],
            };
            assert!((tip[0] - expected[0]).abs() <= 0.1, "{direction}: {svg}");
            assert!((tip[1] - expected[1]).abs() <= 0.1, "{direction}: {svg}");
        }
    }
}

#[test]
fn multiplicities_stay_on_their_own_ends_and_do_not_cover_captions() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let svg = render(&format!(
            "classDiagram\ndirection {direction}\nCustomer \"1\" --> \"0..*\" Order: places\n"
        ));
        let cards = elements(&svg, "fmd-class-cardinality");
        assert_eq!(cards.len(), 2);
        assert_eq!(content(cards[0]), "1");
        assert_eq!(content(cards[1]), "0..*");
        let label = elements(&svg, "fmd-class-label")[0];
        let axis = if matches!(direction, "LR" | "RL") { "x" } else { "y" };
        let (start, middle, end) = (number(cards[0], axis), number(label, axis), number(cards[1], axis));
        if matches!(direction, "TD" | "LR") {
            assert!(start < middle && middle < end, "{direction}: {svg}");
        } else {
            assert!(start > middle && middle > end, "{direction}: {svg}");
        }
        assert_scene_bounds(&svg);
    }
}

#[test]
fn branches_parallel_relationships_cycles_and_self_links_keep_all_labels_clear() {
    for direction in ["TD", "BT", "LR", "RL"] {
        for body in [
            "A \"1\" --> \"*\" B: first\nA \"1\" --> \"*\" C: second\n",
            "A \"1\" --> \"*\" B: first\nA \"1\" ..> \"*\" B: second\n",
            "A \"1\" --> \"*\" A: parent\nA --> B: next\nB ..> A: return\n",
            "A --> B: short\nA --> C: a different longer caption\nB --> C: joined\n",
        ] {
            let svg = render(&format!("classDiagram\ndirection {direction}\n{body}"));
            assert_eq!(
                elements(&svg, "fmd-class-relation").len(),
                body.lines().count(),
                "{direction}"
            );
            let paths = elements(&svg, "fmd-class-relation");
            for (index, path) in paths.iter().enumerate() {
                for other in &paths[index + 1..] {
                    assert_ne!(attr(path, "d"), attr(other, "d"), "distinct relationships need distinct routes");
                }
            }
            assert_scene_bounds(&svg);
        }
    }
}

fn assert_scene_bounds(svg: &str) {
    let root = svg.lines().next().unwrap();
    let width = number(root, "width");
    let height = number(root, "height");
    assert!(width.is_finite() && width > 0.0);
    assert!(height.is_finite() && height > 0.0);
    let nodes: Vec<_> = elements(svg, "fmd-class-node").into_iter().map(rect).collect();
    let labels: Vec<_> = elements(svg, "fmd-class-label-bg").into_iter().map(rect).collect();
    for bounds in nodes.iter().chain(&labels) {
        assert!(bounds[0] >= 0.0 && bounds[1] >= 0.0, "{bounds:?}\n{svg}");
        assert!(bounds[2] <= width + 0.1 && bounds[3] <= height + 0.1, "{bounds:?}\n{svg}");
    }
    for (index, label) in labels.iter().enumerate() {
        for other in &labels[index + 1..] {
            assert!(!overlap(*label, *other), "caption overlap: {label:?}, {other:?}\n{svg}");
        }
        for node in &nodes {
            assert!(!overlap(*label, *node), "caption covered by node: {label:?}, {node:?}\n{svg}");
        }
    }
}

#[test]
fn long_wide_members_wrap_without_losing_text_or_leaving_the_class_box() {
    let value = "W".repeat(512);
    let svg = render(&format!("classDiagram\nclass A\nA: {value}\n"));
    let node = rect(elements(&svg, "fmd-class-node")[0]);
    let members = elements(&svg, "fmd-class-member");
    assert!(members.len() > 1);
    assert_eq!(members.iter().map(|line| content(line)).collect::<String>(), value);
    for line in members {
        let x = number(line, "x");
        let y = number(line, "y");
        // W is a deliberately wide glyph. Even a conservative one-em width
        // per character must remain inside the node's content rectangle.
        assert!(x >= node[0] + 10.0);
        assert!(x + content(line).chars().count() as f32 * 12.0 <= node[2] - 10.0);
        assert!(y > node[1] && y < node[3]);
    }
    assert_scene_bounds(&svg);
}

#[test]
fn quoted_names_aliases_and_member_text_cannot_inject_svg() {
    let source = concat!(
        "%% a complete line comment\n",
        "classDiagram\n",
        "class `Odd <name>`[\"<script> & literal %% label\"]\n",
        "`Odd <name>`: +value: <tag> & two  spaces\n",
        "`Odd <name>` --> Result: a colon: and an arrow -->\n",
    );
    let svg = render(source);
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<tag>"));
    assert!(svg.contains("&lt;script&gt; &amp; literal %%"));
    assert!(svg.contains("+value: &lt;tag&gt; &amp; two  "));
    assert!(svg.contains("a colon: and an arrow --&gt;"));
    assert!(!svg.contains("complete line comment"));
    assert!(!svg.contains("<g"), "generated scenes retain the existing flat primitive profile");
}

#[test]
fn unsupported_and_malformed_syntax_refuses_the_entire_diagram() {
    for suffix in [
        "namespace Group {\nclass Hidden\n}",
        "note \"must not disappear\"",
        "note for A \"important\"",
        "click A href \"https://example.test\"",
        "link A \"https://example.test\"",
        "callback A \"handler\"",
        "style A fill:red",
        "classDef highlighted fill:red",
        "cssClass \"A\" highlighted",
        "class A:::highlighted",
        "accTitle: Required metadata",
        "accDescr: Required metadata",
        "%%{init: { theme: 'dark' }}%%",
        "class Generic~T~",
        "A: +List~string~ entries",
        "A ()-- B",
        "A --() B",
        "A -->",
        "A --> B --> C",
        "A --> B; class C",
        "A --> B: label; class C",
        "A --x B",
        "A \"1\" --> \"*\"",
        "A \"unterminated --> B",
        "class A[\"Unterminated]",
        "class A[\"First\"]\nclass A[\"Second\"]",
        "class A <<interface>>\n<<service>> A",
        "class A {\n+unclosed",
        "class A {\nclass Nested\n}",
        "class A { +first; +second }",
        "A: +method(",
        "A: +method)",
        "A: +method()$*",
        "direction SIDEWAYS",
        "direction LR\ndirection RL",
        "UnclaimedBareStatement",
    ] {
        let source = format!("classDiagram\nclass A\n{suffix}\n");
        assert!(render_diagram_svg(&source, "mermaid").is_none(), "{source}");
    }
    for source in [
        "classDiagram",
        "classDiagram\n%% just a comment",
        "classDiagram\nclass A\n}",
        "---\ntitle: Unhandled\n---\nclassDiagram\nclass A",
        "classDiagram\nclass A\nA: bad\0text",
        "classDiagram\nclass A\nA: bad\u{fffe}text",
        "classDiagram\nclass A[\"bad\u{ffff}text\"]",
        "classDiagram\nclass ` `",
    ] {
        assert!(render_diagram_svg(source, "mermaid").is_none(), "{source}");
    }
}

#[test]
fn class_fences_and_automatic_headers_share_one_grammar() {
    let source = "%% comment\nclassDiagram\nclass A\nA --> B\n";
    let expected = render(source);
    assert_eq!(expected, render_diagram_svg(source, "").unwrap());
    assert_eq!(expected, render_diagram_svg(source, "class").unwrap());
    assert!(is_diagram_code(source, ""));
    assert!(!is_diagram_code("classDiagrammatic text", ""));
    for foreign in [
        "A --> B",
        "flowchart TD\nA --> B",
        "stateDiagram\nA --> B",
        "sequenceDiagram\nA->>B: message",
        "pie\n\"A\": 1",
        "+---+\n| A |\n+---+",
    ] {
        assert!(render_diagram_svg(foreign, "class").is_none(), "{foreign}");
    }
}


#[test]
fn self_and_return_endpoint_shapes_stay_on_their_terminal_segments() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let svg = render(&format!(
            "classDiagram\ndirection {direction}\nA o--* A: self\n"
        ));
        let edge = elements(&svg, "fmd-class-relation")[0];
        let points: Vec<f32> = attr(edge, "d").split_whitespace()
            .filter_map(|value| value.parse().ok()).collect();
        for (kind, reverse) in [("fmd-class-aggregation", false), ("fmd-class-composition", true)] {
            let (tip, corner) = if reverse {
                (
                    [points[points.len() - 2], points[points.len() - 1]],
                    [points[points.len() - 4], points[points.len() - 3]],
                )
            } else {
                ([points[0], points[1]], [points[2], points[3]])
            };
            let delta = [corner[0] - tip[0], corner[1] - tip[1]];
            let length = delta[0].hypot(delta[1]);
            assert!(length > 0.0);
            let head = elements(&svg, kind)[0];
            for pair in attr(head, "points").split_whitespace() {
                let xy: Vec<f32> = pair.split(',').map(|value| value.parse().unwrap()).collect();
                let along = ((xy[0] - tip[0]) * delta[0] + (xy[1] - tip[1]) * delta[1]) / length;
                assert!((-0.1..=length - 0.5).contains(&along), "{direction}: marker crosses its elbow\n{svg}");
            }
        }
        assert_scene_bounds(&svg);
    }
}

#[test]
fn limits_admit_boundaries_and_refuse_extra_data_without_partial_scenes() {
    let classes = (0..256).map(|n| format!("class C{n}\n")).collect::<String>();
    let source = format!("classDiagram\n{classes}");
    assert_eq!(elements(&render(&source), "fmd-class-node").len(), 256);
    assert!(render_diagram_svg(&format!("{source}class Excess\n"), "mermaid").is_none());

    let relations = "A --> B\n".repeat(1024);
    let source = format!("classDiagram\n{relations}");
    assert_eq!(elements(&render(&source), "fmd-class-relation").len(), 1024);
    assert!(render_diagram_svg(&format!("{source}A --> B\n"), "mermaid").is_none());

    let members = (0..64).map(|n| format!("+value{n}\n")).collect::<String>();
    let source = format!("classDiagram\nclass A {{\n{members}}}\n");
    assert_eq!(elements(&render(&source), "fmd-class-member").len(), 64);
    assert!(render_diagram_svg(&format!("{source}A: +excess\n"), "mermaid").is_none());

    let mut source = "classDiagram\n".to_owned();
    for class in 0..32 {
        source.push_str(&format!("class A{class} {{\n{members}}}\n"));
    }
    assert_eq!(elements(&render(&source), "fmd-class-member").len(), 2048);
    assert!(render_diagram_svg(&format!("{source}class Excess\nExcess: +member\n"), "mermaid").is_none());

    for (valid, invalid) in [
        (format!("classDiagram\nclass {}\n", "a".repeat(128)), format!("classDiagram\nclass {}\n", "a".repeat(129))),
        (format!("classDiagram\nclass A[\"{}\"]\n", "a".repeat(512)), format!("classDiagram\nclass A[\"{}\"]\n", "a".repeat(513))),
        (format!("classDiagram\nA: {}\n", "a".repeat(512)), format!("classDiagram\nA: {}\n", "a".repeat(513))),
        (format!("classDiagram\nA \"{}\" --> B\n", "1".repeat(64)), format!("classDiagram\nA \"{}\" --> B\n", "1".repeat(65))),
    ] {
        assert!(render_diagram_svg(&valid, "mermaid").is_some());
        assert!(render_diagram_svg(&invalid, "mermaid").is_none());
    }
    let prefix = "classDiagram\nclass A\n%%";
    let source = format!("{prefix}{}", "x".repeat(65536 - prefix.len()));
    assert!(render_diagram_svg(&source, "mermaid").is_some());
    assert!(render_diagram_svg(&format!("{source}x"), "mermaid").is_none());
}
