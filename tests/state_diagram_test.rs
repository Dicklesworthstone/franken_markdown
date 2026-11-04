//! State-machine grammar and actual shared publication-output regressions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfOptions, is_diagram_code, parse_markdown, render_diagram_svg, render_html,
    render_pdf,
};

const MACHINE: &str = concat!(
    "stateDiagram-v2\n",
    "[*] --> Idle\n",
    "Idle --> Check: request received\n",
    "state \"Waiting for input\" as Idle\n",
    "state Check <<choice>>\n",
    "Check --> Running: if n >= 0\n",
    "Check --> Idle: if n < 0\n",
    "Running --> Running: keep working\n",
    "Running --> [*]: finish\n",
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

#[test]
fn state_machine_keeps_aliases_choices_initial_final_and_every_transition() {
    let svg = render_diagram_svg(MACHINE, "mermaid").unwrap();
    assert!(svg.contains("class=\"fmd-diagram fmd-state\""));
    assert!(svg.contains("<title>State diagram</title>"));
    assert_eq!(svg.matches("class=\"fmd-edge-path\"").count(), 6);
    assert_eq!(svg.matches("class=\"fmd-node-text\"").count(), 2);
    assert_eq!(
        svg.matches("<circle ").count(),
        3,
        "start and double-ring end"
    );
    assert_eq!(svg.matches("<polygon ").count(), 1, "choice diamond");
    assert!(svg.contains(">Waiting for input</text>"));
    assert!(
        !svg.contains(">Idle</text>"),
        "late alias replaces implicit label"
    );
    for text in [
        "Running",
        "request received",
        "if n &gt;= 0",
        "if n &lt; 0",
        "keep working",
        "finish",
    ] {
        assert!(svg.contains(&format!(">{text}</text>")), "{text}");
    }
    assert_eq!(svg, render_diagram_svg(MACHINE, "mermaid").unwrap());
    assert_eq!(svg, render_diagram_svg(MACHINE, "state").unwrap());
    assert_eq!(svg, render_diagram_svg(MACHINE, "").unwrap());
    assert!(is_diagram_code(MACHINE, ""));
    let legacy = MACHINE.replacen("stateDiagram-v2", "stateDiagram", 1);
    assert_eq!(svg, render_diagram_svg(&legacy, "mermaid").unwrap());
}

#[test]
fn state_descriptions_comments_and_literal_markup_keep_their_ownership() {
    let source = concat!(
        "%% introduction\nstateDiagram-v2 %% header\n",
        "A --> B: a colon: an arrow --> and \"quoted words\" %% ignored\n",
        "state \"Keep %% [*] --> <script> & symbols\" as A\n",
        "B: Completed <safely> & ready\n",
    );
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    assert!(svg.contains(">Keep %% [*] --&gt; &lt;script&gt; &amp; symbols</text>"));
    assert!(svg.contains(">Completed &lt;safely&gt; &amp; ready</text>"));
    assert!(svg.contains("a colon: an arrow --&gt; and"));
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("ignored"));
    assert_eq!(svg.matches("class=\"fmd-edge-path\"").count(), 1);
    assert_eq!(
        svg,
        render_diagram_svg(&source.replace('\n', "\r\n"), "mermaid").unwrap()
    );
}

#[test]
fn forks_and_joins_are_bars_perpendicular_to_each_declared_direction() {
    for direction in ["TD", "TB", "BT", "LR", "RL"] {
        let source = format!(
            "stateDiagram-v2\nstate fork <<fork>>\nstate join <<join>>\n[*] --> fork\nfork --> Left\nfork --> Right\nLeft --> join\nRight --> join\njoin --> [*]\ndirection {direction}"
        );
        let svg = render_diagram_svg(&source, "mermaid").unwrap();
        let bars: Vec<_> = svg
            .lines()
            .filter(|line| line.starts_with("<rect ") && line.contains("class=\"fmd-state-solid\""))
            .collect();
        assert_eq!(bars.len(), 2);
        for bar in bars {
            let (width, height) = (number(bar, "width"), number(bar, "height"));
            assert_eq!(width > height, matches!(direction, "TD" | "TB" | "BT"));
            assert!(width > 0.0 && height > 0.0);
        }
        let markers: Vec<_> = svg
            .lines()
            .filter(|line| {
                line.starts_with("<circle ") && line.contains("class=\"fmd-state-solid\"")
            })
            .collect();
        assert_eq!(markers.len(), 2);
        let coordinate = if matches!(direction, "LR" | "RL") {
            "cx"
        } else {
            "cy"
        };
        assert_eq!(
            number(markers[0], coordinate) < number(markers[1], coordinate),
            matches!(direction, "TD" | "TB" | "LR"),
            "{direction}"
        );
    }
}

#[test]
fn unsupported_state_syntax_refuses_the_complete_machine() {
    for suffix in [
        "state Composite {\nNested --> Other\n}",
        "note right of Idle: Keep this information",
        "note left of Idle\nKeep this information\nend note",
        "Idle:::highlight",
        "Idle --> Running:::highlight",
        "classDef highlight fill:red",
        "class Idle highlight",
        "style Idle fill:red",
        "click Idle callback",
        "accTitle: Required accessible title",
        "accDescr: Required accessible description",
        "%%{init: {theme: 'dark'}}%%",
        "state Pseudo <<history>>",
        "state Bad <<choice>> trailing",
        "state \"Unclosed as Alias",
        "Idle -->",
        "Idle --> Running --> End",
        "Idle --> Running; Running --> End",
        "Idle: First description\nIdle: Second description",
        "state Idle <<choice>>\nIdle: Named choice",
        "direction SIDEWAYS",
        "direction LR\ndirection RL",
        "[*]",
        "---\ntitle: Frontmatter\n---",
    ] {
        let source = format!("stateDiagram-v2\n[*] --> Idle\n{suffix}");
        assert!(render_diagram_svg(&source, "mermaid").is_none(), "{source}");
        let html = render_html(
            &format!("```mermaid\n{source}\n```"),
            &HtmlOptions::default(),
        )
        .unwrap();
        assert!(
            !html.contains("class=\"fmd-diagram fmd-state\""),
            "{source}"
        );
        assert!(html.contains("language-mermaid"), "{source}");
    }
}

#[test]
fn state_machine_limits_are_checked_before_drawing_any_nodes() {
    let nodes = (0..256).map(|n| format!("S{n}\n")).collect::<String>();
    let exact = format!("stateDiagram-v2\n{nodes}");
    assert_eq!(
        render_diagram_svg(&exact, "mermaid")
            .unwrap()
            .matches("class=\"fmd-node-text\"")
            .count(),
        256
    );
    let edges = "A --> B\n".repeat(1024);
    assert_eq!(
        render_diagram_svg(&format!("stateDiagram-v2\n{edges}"), "mermaid")
            .unwrap()
            .matches("class=\"fmd-edge-path\"")
            .count(),
        1024
    );
    for source in [
        format!("{exact}S256"),
        format!("stateDiagram-v2\n{edges}A --> B"),
        format!("stateDiagram-v2\n{}", "A".repeat(129)),
        format!("stateDiagram-v2\nA: {}", "x".repeat(513)),
        format!("stateDiagram-v2\nA --> B: {}", "x".repeat(513)),
        format!("stateDiagram-v2\nA\n%%{}", "x".repeat(65536)),
        "stateDiagram-v2\nA: bad\0label".to_owned(),
    ] {
        assert!(render_diagram_svg(&source, "mermaid").is_none());
    }
}

#[test]
fn state_diagrams_reach_html_tagged_pdf_and_svg_posters() {
    let markdown = format!("```mermaid\n{MACHINE}```\n");
    let svg = render_diagram_svg(MACHINE, "mermaid").unwrap();
    let html = render_html(&markdown, &HtmlOptions::default()).unwrap();
    assert!(html.contains(&svg));
    let pdf = render_pdf(&markdown, &PdfOptions::default()).unwrap();
    assert_eq!(pdf, render_pdf(&markdown, &PdfOptions::default()).unwrap());
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/S /Figure").count(), 1);
    assert!(raw.contains("/Alt (State diagram)"));
    let layer = franken_markdown::pdf::verification_text_layer(
        &parse_markdown(&markdown),
        &PdfOptions::default(),
    )
    .unwrap();
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
            .any(|run| run.text.contains("stateDiagram"))
    );
    let (poster, _, warnings) = franken_markdown::svg::render_svg_with_diagnostics(
        &parse_markdown(&markdown),
        &franken_markdown::svg::SvgOptions::default(),
    );
    let poster = String::from_utf8(poster).unwrap();
    assert!(poster.contains("data:image/svg+xml;base64,"));
    assert!(poster.contains("aria-label=\"State diagram\""));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn unsupported_state_machine_remains_readable_pdf_source() {
    let source = "```mermaid\nstateDiagram-v2\nstate Composite {\nInner --> Finish\n}\n```";
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/S /Figure"));
    let layer = franken_markdown::pdf::verification_text_layer(
        &parse_markdown(source),
        &PdfOptions::default(),
    )
    .unwrap();
    let runs: Vec<_> = layer.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().any(|run| run.text.contains("Composite")));
    assert!(runs.iter().any(|run| run.text.contains("Inner")));
}

#[test]
fn explicit_diagram_languages_own_their_grammar() {
    assert!(
        render_diagram_svg("stateDiagram --> Finished", "flowchart")
            .unwrap()
            .contains("fmd-flowchart")
    );
    assert!(
        render_diagram_svg("stateDiagram->>Finished: Message", "sequence")
            .unwrap()
            .contains("fmd-sequence")
    );
    assert!(!is_diagram_code("stateDiagrammatic prose", ""));
    assert!(render_diagram_svg("stateDiagram-v2", "mermaid").is_none());
    for source in [
        "A --> B",
        "flowchart TD\nA --> B",
        "sequenceDiagram\nA->>B: Message",
        "pie\n\"A\": 1",
        "+---+\n| A |\n+---+",
    ] {
        assert!(render_diagram_svg(source, "state").is_none(), "{source}");
    }
}

#[test]
fn distinct_transition_guards_never_overpaint_each_other() {
    for direction in ["TD", "BT", "LR", "RL"] {
        for body in [
            "Idle --> Running: first transition guard\nIdle --> Running: second transition guard",
            "state choose <<choice>>\nchoose --> A: first transition guard\nchoose --> B: second transition guard",
            "A --> B: forward transition guard\nB --> A: return transition guard\nA --> C: later wider transition guard",
        ] {
            let svg = render_diagram_svg(
                &format!("stateDiagram-v2\ndirection {direction}\n{body}"),
                "mermaid",
            )
            .unwrap();
            let captions: Vec<_> = svg
                .lines()
                .filter(|line| line.contains("class=\"fmd-edge-label-bg\""))
                .map(|line| {
                    let (x, y) = (number(line, "x"), number(line, "y"));
                    [x, y, x + number(line, "width"), y + number(line, "height")]
                })
                .collect();
            assert!(captions.len() >= 2);
            for (index, a) in captions.iter().enumerate() {
                for b in &captions[index + 1..] {
                    assert!(
                        a[2] <= b[0] || a[0] >= b[2] || a[3] <= b[1] || a[1] >= b[3],
                        "{direction}: transition captions overlap: {a:?} and {b:?}\n{svg}"
                    );
                }
            }
            let paths: Vec<_> = svg
                .lines()
                .filter(|line| line.contains("class=\"fmd-edge-path\""))
                .collect();
            assert_ne!(
                paths[0], paths[1],
                "distinct transitions need distinct routes"
            );
        }
    }
}

#[test]
fn diagram_type_metadata_comes_from_the_root_not_a_state_label() {
    let source = "```mermaid\nstateDiagram-v2\nA: fmd-sequence\nA --> B\n```";
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/Alt (State diagram)"));
    assert!(!raw.contains("/Alt (Sequence diagram)"));
}
