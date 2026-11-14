//! Actual ER grammar, endpoint notation, table and routing geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{is_diagram_code, render_diagram_svg};

const SCHEMA: &str = concat!(
    "erDiagram\n",
    "CUSTOMER ||--o{ ORDER : places\n",
    "ORDER ||--|{ LINE_ITEM : contains\n",
    "PRODUCT }o..o{ LINE_ITEM : \"appears in\"\n",
    "CUSTOMER[\"Customer account\"] {\n",
    "  int id PK \"Stable identifier\"\n",
    "  string email UK\n",
    "  string? nickname\n",
    "}\n",
    "ORDER { int *id string customer_id FK }\n",
    "LINE_ITEM {\n",
    "  int order_id PK, FK\n",
    "  int product_id PK, FK\n",
    "  decimal(10,2) amount \"Exact amount\"\n",
    "}\n",
);

fn attribute<'a>(tag: &'a str, key: &str) -> &'a str {
    tag.split_once(&format!("{key}=\""))
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
}

fn number(tag: &str, key: &str) -> f32 {
    attribute(tag, key).parse().unwrap()
}

fn path_points(tag: &str) -> Vec<(f32, f32)> {
    let numbers: Vec<f32> = attribute(tag, "d")
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter_map(|part| part.parse().ok())
        .collect();
    assert_eq!(numbers.len() % 2, 0, "{tag}");
    numbers.chunks_exact(2).map(|pair| (pair[0], pair[1])).collect()
}

#[test]
fn schema_keeps_entities_attributes_aliases_keys_comments_and_relationships() {
    let svg = render_diagram_svg(SCHEMA, "mermaid").unwrap();
    assert!(svg.contains("class=\"fmd-diagram fmd-er\""));
    assert!(svg.contains("<title>Entity relationship diagram</title>"));
    assert_eq!(svg.matches("class=\"fmd-er-entity-border\"").count(), 4);
    assert_eq!(svg.matches("class=\"fmd-er-edge\"").count(), 3);
    assert!(svg.contains(">Customer account</text>"));
    assert!(!svg.contains(">CUSTOMER</text>"), "late alias wins over implicit label");
    for text in [
        "ORDER", "LINE_ITEM", "PRODUCT", "int", "id", "PK", "FK", "UK", "PK, FK",
        "Stable identifier", "email", "string?", "nickname", "customer_id", "order_id",
        "product_id", "decimal(10,2)", "amount", "Exact amount", "places", "contains",
        "appears in",
    ] {
        assert!(svg.contains(&format!(">{text}</text>")), "{text}");
    }
    assert!(!svg.contains(">*id</text>"), "starred primary key has a key column");
    assert_eq!(
        svg.lines()
            .filter(|line| line.contains("class=\"fmd-er-edge\"") && line.contains("stroke-dasharray"))
            .count(),
        1
    );
    assert!(!svg.contains("<g"), "shared Flow accepts only flat generated scenes");
    assert_eq!(svg, render_diagram_svg(SCHEMA, "mermaid").unwrap());
    assert_eq!(svg, render_diagram_svg(SCHEMA, "").unwrap());
    assert_eq!(svg, render_diagram_svg(SCHEMA, "er").unwrap());
    assert!(is_diagram_code(SCHEMA, ""));
    assert!(is_diagram_code(SCHEMA, "er"));
}

#[test]
fn all_symbolic_cardinalities_keep_their_independent_zero_one_and_many_marks() {
    let left = [("|o", true, false), ("||", false, false), ("}o", true, true), ("}|", false, true)];
    let right = [("o|", true, false), ("||", false, false), ("o{", true, true), ("|{", false, true)];
    for (first, optional_a, many_a) in left {
        for (second, optional_b, many_b) in right {
            let svg = render_diagram_svg(&format!("erDiagram\nA {first}--{second} B : relates"), "er")
                .unwrap();
            let symbols: Vec<_> = svg.lines()
                .filter(|line| line.contains("class=\"fmd-er-cardinality\""))
                .collect();
            let forked = symbols.iter().filter(|line| path_points(line).len() == 3).count();
            let stems = symbols.iter().filter(|line| line.contains("data-cardinality=")).count();
            let bars = symbols.len() - forked - stems;
            assert_eq!(stems, 2);
            assert_eq!(forked, usize::from(many_a) + usize::from(many_b), "{first} {second}");
            assert_eq!(
                svg.matches("class=\"fmd-er-cardinality-circle\"").count(),
                usize::from(optional_a) + usize::from(optional_b),
                "{first} {second}"
            );
            assert_eq!(
                bars,
                usize::from(!many_a) + usize::from(!many_b)
                    + usize::from(!optional_a) + usize::from(!optional_b),
                "{first} {second}"
            );
            assert!(!svg.contains("marker-start="));
            assert!(!svg.contains("marker-end="), "crow feet are actual portable geometry");
        }
    }
}

#[test]
fn quoted_names_and_aliases_own_syntax_and_escape_markup() {
    let source = concat!(
        "%% introduction\nerDiagram %% header\n",
        "\"部门 甲\" ||--o{ \"订单\" : \"Owns %% and : |{ <script> & text\"\n",
        "\"部门 甲\"[\"<Account> & safely quoted\"] {\n",
        "  string 名称 \"Keep %% } { [ ] and <img> & text\"\n",
        "}\n",
        "\"订单\" %% trailing comment\n",
    );
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    for text in [
        "&lt;Account&gt; &amp; safely quoted",
        "名称",
        "订单",
        "Owns %% and : |{ &lt;script&gt; &amp; text",
    ] {
        assert!(svg.contains(&format!(">{text}</text>")), "{text}");
    }
    assert!(svg.contains("Keep %% } { [ ] and &lt;img&gt; &amp; text"));
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<img>"));
    assert!(!svg.contains("trailing comment"));
    assert_eq!(svg.matches("class=\"fmd-er-entity-border\"").count(), 2);
    assert_eq!(svg.matches("class=\"fmd-er-edge\"").count(), 1);
    assert!(svg.contains("xml:space=\"preserve\""));
    assert_eq!(svg, render_diagram_svg(&source.replace('\n', "\r\n"), "mermaid").unwrap());
}

#[test]
fn attribute_blocks_preserve_source_order_and_can_extend_an_existing_entity() {
    let source = "erDiagram\nA {}\nA { int first PK }\nA {\nstring second\n}\nA[Account]\nA[Account]\n";
    let svg = render_diagram_svg(source, "er").unwrap();
    assert_eq!(svg.matches("class=\"fmd-er-entity-border\"").count(), 1);
    assert_eq!(svg.matches(">Account</text>").count(), 1);
    assert!(svg.find(">first</text>").unwrap() < svg.find(">second</text>").unwrap());
    assert!(render_diagram_svg("erDiagram\nA[First]\nA[Second]", "er").is_none());
    assert!(render_diagram_svg("erDiagram\n\"direction\"", "er").is_some());
}

#[test]
fn cardinality_marks_face_the_entity_and_keep_the_declared_direction() {
    for (direction, axis) in [
        ("TB", (0.0_f32, 1.0_f32)), ("BT", (0.0, -1.0)),
        ("LR", (1.0, 0.0)), ("RL", (-1.0, 0.0)),
    ] {
        let svg = render_diagram_svg(
            &format!("erDiagram\ndirection {direction}\nA }}|--o{{ B : relates"), "er"
        ).unwrap();
        let nodes: Vec<_> = svg.lines()
            .filter(|line| line.contains("class=\"fmd-er-entity-border\""))
            .collect();
        let centers: Vec<_> = nodes.iter().map(|node| (
            number(node, "x") + number(node, "width") / 2.0,
            number(node, "y") + number(node, "height") / 2.0,
        )).collect();
        assert!((centers[1].0 - centers[0].0) * axis.0
            + (centers[1].1 - centers[0].1) * axis.1 > 0.0, "{direction}");
        let stems: Vec<_> = svg.lines()
            .filter(|line| line.contains("data-cardinality="))
            .map(path_points)
            .collect();
        assert_eq!(stems.len(), 2);
        let forks: Vec<_> = svg.lines()
            .filter(|line| line.contains("class=\"fmd-er-cardinality\""))
            .map(path_points)
            .filter(|points| points.len() == 3)
            .collect();
        assert_eq!(forks.len(), 2);
        for index in 0..2 {
            let sign = if index == 0 { 1.0 } else { -1.0 };
            let stem = &stems[index];
            let fork = &forks[index];
            let outward = |point: (f32, f32)| {
                ((point.0 - stem[0].0) * axis.0 + (point.1 - stem[0].1) * axis.1) * sign
            };
            assert!(outward(stem[1]) > 0.0);
            assert!(outward(fork[0]) >= 0.0);
            assert!(outward(fork[1]) > outward(fork[0]));
            assert!(outward(fork[1]) < outward(stem[1]));
            assert_eq!(outward(fork[0]), outward(fork[2]));
            let node = nodes[index];
            let (x, y, w, h) = (
                number(node, "x"), number(node, "y"),
                number(node, "width"), number(node, "height"),
            );
            let expected = (
                x + w / 2.0 + axis.0 * sign * w / 2.0,
                y + h / 2.0 + axis.1 * sign * h / 2.0,
            );
            assert!((stem[0].0 - expected.0).abs() < 0.11, "{direction}");
            assert!((stem[0].1 - expected.1).abs() < 0.11, "{direction}");
        }
    }
}

#[test]
fn self_parallel_and_return_relationships_keep_distinct_paths_and_captions() {
    let body = concat!(
        "A ||--o{ B : \"first relation\"\n",
        "A ||--o{ B : \"second relation\"\n",
        "B }o..|| A : \"return relation\"\n",
        "A |o--o| A : \"self relation\"\n",
        "A ||--|{ C : \"another relation\"\n",
    );
    for direction in ["TB", "BT", "LR", "RL"] {
        let svg = render_diagram_svg(&format!("erDiagram\ndirection {direction}\n{body}"), "er")
            .unwrap();
        let paths: Vec<_> = svg.lines()
            .filter(|line| line.contains("class=\"fmd-er-edge\""))
            .map(|line| attribute(line, "d"))
            .collect();
        assert_eq!(paths.len(), 5);
        assert_ne!(paths[0], paths[1], "{direction}: parallel relationships overlap");
        let captions: Vec<_> = svg.lines()
            .filter(|line| line.contains("class=\"fmd-er-label-bg\""))
            .map(|line| {
                let (x, y) = (number(line, "x"), number(line, "y"));
                [x, y, x + number(line, "width"), y + number(line, "height")]
            }).collect();
        assert_eq!(captions.len(), 5);
        for (index, a) in captions.iter().enumerate() {
            for b in &captions[index + 1..] {
                assert!(a[2] <= b[0] || a[0] >= b[2] || a[3] <= b[1] || a[1] >= b[3],
                    "{direction}: captions overlap {a:?} {b:?}");
            }
        }
        let root = svg.lines().next().unwrap();
        let (width, height) = (number(root, "width"), number(root, "height"));
        for line in svg.lines() {
            if line.starts_with("<rect ") {
                let (x, y) = (number(line, "x"), number(line, "y"));
                assert!(x >= 0.0 && y >= 0.0);
                assert!(x + number(line, "width") <= width + 0.51);
                assert!(y + number(line, "height") <= height + 0.51);
            } else if line.starts_with("<circle ") {
                let (cx, cy, r) = (number(line, "cx"), number(line, "cy"), number(line, "r"));
                assert!(cx - r >= 0.0 && cy - r >= 0.0);
                assert!(cx + r <= width + 0.51 && cy + r <= height + 0.51);
            } else if line.starts_with("<path ") {
                for (x, y) in path_points(line) {
                    assert!(x >= 0.0 && y >= 0.0);
                    assert!(x <= width + 0.51 && y <= height + 0.51);
                }
            }
        }
    }
}

#[test]
fn wide_letters_fit_entity_titles_and_each_attribute_column() {
    let title = "W".repeat(48);
    let svg = render_diagram_svg(&format!("erDiagram\nA[\"{title}\"]"), "er").unwrap();
    let node = svg.lines().find(|line| line.contains("class=\"fmd-er-entity-border\"")).unwrap();
    let text = svg.lines().find(|line| line.contains("class=\"fmd-er-title\"")).unwrap();
    // W in the declared Arial/Helvetica family uses about .94 em. Reserve .95
    // here: the former eight-pixel heuristic would clip a sustained wide run.
    let half_ink = 48.0 * 13.0 * 0.95 / 2.0;
    assert!(number(text, "x") - half_ink >= number(node, "x"));
    assert!(number(text, "x") + half_ink <= number(node, "x") + number(node, "width"));

    let name = "W".repeat(32);
    let comment = "M".repeat(40);
    let svg = render_diagram_svg(
        &format!("erDiagram\nA {{ string {name} PK \"{comment}\" }}"), "er"
    ).unwrap();
    let node = svg.lines().find(|line| line.contains("class=\"fmd-er-entity-border\"")).unwrap();
    let row: Vec<_> = svg.lines()
        .filter(|line| line.contains("class=\"fmd-er-attribute\""))
        .collect();
    assert_eq!(row.len(), 4);
    for (index, line) in row.iter().enumerate() {
        let content = line.split_once('>').unwrap().1.split_once("</text>").unwrap().0;
        let right = row.get(index + 1)
            .map_or(number(node, "x") + number(node, "width"), |next| number(next, "x"));
        let ink = content.chars().count() as f32 * 12.0 * 0.95;
        assert!(number(line, "x") + ink < right, "{line}");
        assert!(number(line, "y") < number(node, "y") + number(node, "height"));
    }
}

#[test]
fn malformed_and_unsupported_input_refuses_the_entire_diagram() {
    for suffix in [
        "A ||--o{ B", "A ||--o{ B :", "A ||--o{ : label",
        "A ||--o{ B : multiple unquoted words", "A ||--o{ B : \"unclosed",
        "A o|--|{ B : wrong_side", "A ||--> B : wrong_arrow",
        "A ||..|{ B : label trailing", "A ||--o{ B : label; C",
        "A ||--o{ B : label\nA { int", "A { int id } trailing",
        "A { int id PK, PK }", "A { int id PK, UNKNOWN }",
        "A { int id \"unterminated }", "A { 123 id }", "A { int 123 }",
        "A { int id } }", "A[Alias] ||--o{ B : label", "A[\"Unclosed]",
        "A:::highlight", "A ||--o{ B:::highlight : label",
        "style A fill:red", "classDef highlight fill:red", "class A highlight",
        "subgraph Group\nA\nend", "accTitle: Required title", "accDescr: Required prose",
        "%%{init: {theme: 'dark'}}%%", "%% {init: {theme: 'dark'}}%%",
        "direction SIDEWAYS", "direction TD", "direction LR\ndirection RL",
        "A; B", "{", "}", "end",
    ] {
        let source = format!("erDiagram\nExisting ||--o{{ Kept : retains\n{suffix}");
        assert!(render_diagram_svg(&source, "mermaid").is_none(), "{source}");
    }
}

#[test]
fn admission_limits_apply_to_complete_source_entities_relationships_and_attributes() {
    let entities = (0..256).map(|index| format!("E{index}\n")).collect::<String>();
    let exact = format!("erDiagram\n{entities}");
    assert_eq!(
        render_diagram_svg(&exact, "er").unwrap().matches("class=\"fmd-er-entity-border\"").count(),
        256
    );
    assert!(render_diagram_svg(&format!("{exact}Overflow"), "er").is_none());

    let relationships = "A ||--o{ B : owns\n".repeat(1024);
    let exact = format!("erDiagram\n{relationships}");
    assert_eq!(
        render_diagram_svg(&exact, "er").unwrap().matches("class=\"fmd-er-edge\"").count(), 1024
    );
    assert!(render_diagram_svg(&format!("{exact}A ||--o{{ B : extra"), "er").is_none());

    let attributes = (0..128).map(|index| format!("int field{index}\n")).collect::<String>();
    let exact = format!("erDiagram\nA {{\n{attributes}}}");
    assert!(render_diagram_svg(&exact, "er").unwrap().contains(">field127</text>"));
    assert!(render_diagram_svg(&format!("erDiagram\nA {{\n{attributes}int extra\n}}"), "er").is_none());

    let entities = (0..16).map(|index| format!("E{index} {{\n{attributes}}}\n")).collect::<String>();
    let exact = format!("erDiagram\n{entities}");
    assert!(render_diagram_svg(&exact, "er").is_some());
    assert!(render_diagram_svg(&format!("{exact}Extra {{ int field }}"), "er").is_none());

    let prefix = "erDiagram\nA\n%%";
    let exact = format!("{prefix}{}", "x".repeat(65536 - prefix.len()));
    assert!(render_diagram_svg(&exact, "er").is_some());
    assert!(render_diagram_svg(&format!("{exact}x"), "er").is_none());
    assert!(render_diagram_svg(&format!("erDiagram\n{}", "E".repeat(128)), "er").is_some());
    assert!(render_diagram_svg(&format!("erDiagram\nA[\"{}\"]", "x".repeat(512)), "er").is_some());
    for source in [
        format!("erDiagram\n{}", "E".repeat(129)),
        format!("erDiagram\nA[\"{}\"]", "x".repeat(513)),
        format!("erDiagram\nA ||--o{{ B : \"{}\"", "x".repeat(513)),
        format!("erDiagram\nA {{ string id \"{}\" }}", "x".repeat(513)),
        "erDiagram\nA[\"invalid\0label\"]".to_owned(),
        "erDiagram\nA[\"invalid\u{ffff}label\"]".to_owned(),
    ] {
        assert!(render_diagram_svg(&source, "er").is_none());
    }
}

#[test]
fn explicit_er_language_requires_its_header_and_owns_its_grammar() {
    for source in [
        "", "erDiagram", "erDiagramExtra\nA", "A ||--o{ B : relates",
        "flowchart TD\nA --> B", "sequenceDiagram\nA->>B: Message",
        "stateDiagram-v2\nA --> B", "pie\n\"A\": 1", "+---+\n| A |\n+---+",
    ] {
        assert!(render_diagram_svg(source, "er").is_none(), "{source}");
    }
    assert!(!is_diagram_code("erDiagrammatic prose", ""));
    assert!(render_diagram_svg("erDiagram\nA", "er").is_some());
}

#[test]
fn quoted_tabs_have_explicit_width_without_merging_entity_identities() {
    let source = "erDiagram\n\"A\tB\" ||--o{ \"A    B\" : \"label\twords\"\n\"A\tB\" { string name \"comment\twords\" }\n";
    let svg = render_diagram_svg(source, "er").unwrap();
    assert_eq!(svg.matches("class=\"fmd-er-entity-border\"").count(), 2);
    assert_eq!(svg.matches(">A    B</text>").count(), 2);
    assert!(svg.contains(">label    words</text>"));
    assert!(svg.contains(">comment    words</text>"));
    assert!(!svg.lines().filter(|line| line.starts_with("<text ")).any(|line| line.contains('\t')));
}
