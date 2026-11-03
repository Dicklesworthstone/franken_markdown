//! Sequence semantics and emitted geometry, through the public shared compiler.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfOptions, is_diagram_code, parse_markdown, render_diagram_svg, render_html,
    render_pdf,
};

const WORKFLOW: &str = "sequenceDiagram\nautonumber\ntitle Retried request\nactor U as Customer\nparticipant A as API\nU->>+A: Submit\nloop Until accepted\n A->>A: Retry internally\n alt accepted\n  A-->>U: Accepted\n else overloaded\n  A-->>U: Retry later\n end\nend\nA-->>-U: Complete\nNote left of U: Keep this private\n";

fn elements<'a>(svg: &'a str, class: &str) -> Vec<&'a str> {
    let class = format!("class=\"{class}\"");
    svg.lines().filter(|line| line.contains(&class)).collect()
}

fn attr<'a>(element: &'a str, name: &str) -> &'a str {
    element
        .split_once(&format!(" {name}=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

fn number(element: &str, name: &str) -> f32 {
    attr(element, name).parse().unwrap()
}

fn visible_text(svg: &str) -> String {
    svg.lines()
        .filter(|line| line.starts_with("<text "))
        .map(|line| {
            line.split_once('>')
                .unwrap()
                .1
                .split("</text>")
                .next()
                .unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn path_numbers(path: &str) -> Vec<f32> {
    attr(path, "d")
        .split_whitespace()
        .filter_map(|word| word.parse().ok())
        .collect()
}

fn assert_rects_in_canvas(svg: &str) {
    let view = attr(svg.lines().next().unwrap(), "viewBox")
        .split_whitespace()
        .map(|value| value.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    for rect in svg.lines().filter(|line| line.starts_with("<rect ")) {
        let (x, y, width, height) = (
            number(rect, "x"),
            number(rect, "y"),
            number(rect, "width"),
            number(rect, "height"),
        );
        assert!(
            x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0,
            "{rect}"
        );
        assert!(
            x + width <= view[2] + 0.1 && y + height <= view[3] + 0.1,
            "{rect}\n{}",
            svg.lines().next().unwrap()
        );
    }
}

#[test]
fn sequence_control_flow_numbering_and_activation_are_visible() {
    let svg = render_diagram_svg(WORKFLOW, "mermaid").unwrap();
    let text = visible_text(&svg);
    for expected in [
        "Retried request",
        "Customer",
        "API",
        "[loop] Until accepted",
        "[alt] accepted",
        "[else] overloaded",
        "1. Submit",
        "2. Retry internally",
        "3. Accepted",
        "4. Retry later",
        "5. Complete",
        "Keep this private",
    ] {
        assert!(text.contains(expected), "lost {expected:?}: {text}");
    }
    assert!(!text.contains("+A") && !text.contains("-U"));
    assert_eq!(elements(&svg, "fmd-seq-lifeline").len(), 2);
    assert_eq!(elements(&svg, "fmd-seq-msg-line").len(), 5);
    assert_eq!(elements(&svg, "fmd-seq-frame").len(), 2);
    assert_eq!(elements(&svg, "fmd-seq-branch").len(), 1);
    let activation = elements(&svg, "fmd-seq-activation");
    assert_eq!(activation.len(), 1);
    let messages = elements(&svg, "fmd-seq-msg-line");
    assert_eq!(number(activation[0], "y"), number(messages[0], "y1"));
    assert!(
        number(activation[0], "y") + number(activation[0], "height") >= number(messages[4], "y1")
    );
    assert_eq!(svg, render_diagram_svg(WORKFLOW, "mermaid").unwrap());
    assert_rects_in_canvas(&svg);
}

#[test]
fn self_messages_have_a_visible_outward_and_return_path_for_every_arrow_kind() {
    for arrow in [
        "->", "-->", "->>", "-->>", "-)", "--)", "-x", "--x", "<<->>", "<<-->>",
    ] {
        let svg = render_diagram_svg(
            &format!("sequenceDiagram\nA{arrow}A: self message"),
            "mermaid",
        )
        .unwrap();
        let messages = elements(&svg, "fmd-seq-msg-line");
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0].starts_with("<path "),
            "{arrow}: {}",
            messages[0]
        );
        let points = path_numbers(messages[0]);
        assert_eq!(points.len(), 8);
        assert!(points[2] > points[0] + 20.0);
        assert_eq!(points[2], points[4]);
        assert!(points[5] > points[1] + 10.0);
        assert_eq!(points[0], points[6]);
        assert_eq!(points[5], points[7]);
        assert_eq!(
            messages[0].contains("stroke-dasharray"),
            arrow.contains("--")
        );
        let filled = elements(&svg, "fmd-seq-arrow").len();
        let open = elements(&svg, "fmd-seq-arrow-open").len();
        match arrow {
            "->" | "-->" => assert_eq!((filled, open), (0, 0)),
            "->>" | "-->>" => assert_eq!((filled, open), (1, 0)),
            "<<->>" | "<<-->>" => assert_eq!((filled, open), (2, 0)),
            _ => assert_eq!((filled, open), (0, 1)),
        }
        assert_rects_in_canvas(&svg);
    }
}

#[test]
fn note_sides_span_the_correct_lifelines_and_multiline_text_is_retained() {
    let source = "sequenceDiagram\nparticipant A\nparticipant B\nNote left of A: left<br/>second\nnote right of B: right\nNote over A: centered\nNote over A,B: across\n";
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    let lifelines = elements(&svg, "fmd-seq-lifeline");
    let (a, b) = (number(lifelines[0], "x1"), number(lifelines[1], "x1"));
    let notes = elements(&svg, "fmd-seq-note");
    assert!(number(notes[0], "x") + number(notes[0], "width") < a);
    assert!(number(notes[1], "x") > b);
    assert_eq!(number(notes[2], "x") + number(notes[2], "width") / 2.0, a);
    assert!(number(notes[3], "x") < a && number(notes[3], "x") + number(notes[3], "width") > b);
    assert!(number(notes[0], "height") > number(notes[1], "height"));
    assert!(visible_text(&svg).contains("left\nsecond"));
    assert!(!svg.contains("&lt;br"));
    assert_rects_in_canvas(&svg);
}

#[test]
fn late_aliases_actors_character_references_and_statement_boundaries_are_owned() {
    let source = "%% comment\nsequenceDiagram; A->>B: Before#59; after #9829; #infin;; participant\tA\tas\tClient<br/>name; actor B as Server; B->>A: literal %% message\naccTitle: Accessible &amp; safe\naccDescr: All &lt;interactions&gt;\n";
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    assert_eq!(elements(&svg, "fmd-seq-lifeline").len(), 2);
    assert_eq!(elements(&svg, "fmd-seq-msg-line").len(), 2);
    assert_eq!(
        svg.matches("<circle ").count(),
        2,
        "actor symbol at both ends"
    );
    let text = visible_text(&svg);
    assert!(text.contains("Before; after ♥ ∞"));
    assert!(text.contains("Client\nname"));
    assert!(text.contains("literal %% message"));
    assert!(svg.contains("<title>Accessible &amp; safe</title>"));
    assert!(svg.contains("<desc>All &lt;interactions&gt;</desc>"));
    assert_eq!(
        svg,
        render_diagram_svg(&source.replace('\n', "\r"), "mermaid").unwrap()
    );
    assert!(render_diagram_svg("pie->>cake: hi", "sequence").is_some());
    assert!(!is_diagram_code("sequenceDiagramUnexpected\nA->>B: hi", ""));
}

#[test]
fn nested_fragments_keep_parallel_and_critical_branch_conditions() {
    let source = "sequenceDiagram\npar first\nA->>B: request\nand second\ncritical connect\nB->>C: connect\noption timeout\nB->>B: retry\nend\nend\nopt optional\nbreak failure\nC--xA: stop\nend\nend";
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    let text = visible_text(&svg);
    for expected in [
        "[par] first",
        "[and] second",
        "[critical] connect",
        "[option] timeout",
        "[opt] optional",
        "[break] failure",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    let frames = elements(&svg, "fmd-seq-frame");
    assert_eq!(frames.len(), 4);
    for (outer, inner) in [(frames[0], frames[1]), (frames[2], frames[3])] {
        assert!(number(outer, "x") < number(inner, "x"));
        assert!(number(outer, "y") < number(inner, "y"));
        assert!(
            number(outer, "y") + number(outer, "height")
                > number(inner, "y") + number(inner, "height")
        );
    }
    assert_rects_in_canvas(&svg);
}

#[test]
fn stacked_activations_are_balanced_and_unclosed_valid_bars_reach_the_lifeline_end() {
    let source = "sequenceDiagram\nA->>+B: first\nB->>+B: nested\nB-->>-B: unwind\nB-->>-A: finish\nactivate A\nA->>B: still active";
    let svg = render_diagram_svg(source, "mermaid").unwrap();
    let bars = elements(&svg, "fmd-seq-activation");
    assert_eq!(bars.len(), 3);
    assert!(number(bars[1], "x") > number(bars[0], "x"));
    assert!(number(bars[1], "y") >= number(bars[0], "y"));
    assert!(
        number(bars[1], "y") + number(bars[1], "height")
            <= number(bars[0], "y") + number(bars[0], "height")
    );
    let lifelines = elements(&svg, "fmd-seq-lifeline");
    assert_eq!(
        number(bars[2], "y") + number(bars[2], "height"),
        number(lifelines[0], "y2")
    );
    assert_rects_in_canvas(&svg);
}

#[test]
fn long_text_wraps_and_shapes_stay_inside_the_canvas() {
    let label = "Wide WWW label αβ 中文 ".repeat(18);
    let source = format!(
        "sequenceDiagram\ntitle {label}\nparticipant A as {label}\nA->>A: {label}\nNote left of A: {label}\nNote right of A: {label}\nloop {label}\nA->>B: {label}\nend"
    );
    let svg = render_diagram_svg(&source, "mermaid").unwrap();
    assert!(elements(&svg, "fmd-seq-msg-text").len() > 4);
    assert!(elements(&svg, "fmd-seq-note-text").len() > 4);
    assert_rects_in_canvas(&svg);
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn malformed_or_unsupported_statements_preserve_the_whole_source() {
    for body in [
        "loop retry\nA->>B: hi",
        "end\nA->>B: hi",
        "loop retry\nelse wrong\nend\nA->>B: hi",
        "alt yes\nand wrong\nend\nA->>B: hi",
        "deactivate A\nA->>B: hi",
        "A->>-B: hi",
        "A->>+B: hi\ndeactivate B\ndeactivate B",
        "A->>>B: hi",
        "A->>B",
        "participant A as",
        "Note under A: hi",
        "Note over A,B,C: hi",
        "Note right of A,B: hi",
        "A->>B: retained\nunknown directive",
        "rect rgb(0,0,0)\nA->>B: retained\nend",
        "box services\nparticipant A\nend\nA->>B: hi",
        "create participant A\nA->>B: hi",
        "participant A@{type: boundary}\nA->>B: hi",
        "A->>()B: hi",
        "A-|\\B: hi",
        "autonumber 1.5 0.25\nA->>B: hi",
        "A->>B: hi\nlink A: Go @ https://example.test",
        "A->>B: #0;",
    ] {
        let source = format!("sequenceDiagram\n{body}");
        assert!(render_diagram_svg(&source, "mermaid").is_none(), "{source}");
        let html = render_html(
            &format!("```mermaid\n{source}\n```"),
            &HtmlOptions::default(),
        )
        .unwrap();
        assert!(html.contains("language-mermaid"));
        assert!(!html.contains("class=\"fmd-diagram fmd-sequence\""));
    }
    assert!(render_diagram_svg("%%{init: {}}%%\nsequenceDiagram\nA->>B: hi", "mermaid").is_none());
}

#[test]
fn diagram_admission_bounds_all_source_dimensions() {
    let participants = (0..64)
        .map(|i| format!("participant P{i}\n"))
        .collect::<String>();
    assert!(render_diagram_svg(&format!("sequenceDiagram\n{participants}"), "mermaid").is_some());
    assert!(
        render_diagram_svg(
            &format!("sequenceDiagram\n{participants}participant P64"),
            "mermaid"
        )
        .is_none()
    );
    let events = "A->>B: hi\n".repeat(1024);
    assert!(render_diagram_svg(&format!("sequenceDiagram\n{events}"), "mermaid").is_some());
    assert!(
        render_diagram_svg(
            &format!("sequenceDiagram\n{events}A->>B: excess"),
            "mermaid"
        )
        .is_none()
    );
    for source in [
        format!(
            "sequenceDiagram\n{}A->>B: hi\n{}",
            "loop x\n".repeat(17),
            "end\n".repeat(17)
        ),
        format!("sequenceDiagram\n{}", "activate A\n".repeat(33)),
        format!("sequenceDiagram\nA->>B: {}", "x".repeat(1025)),
        format!("sequenceDiagram\nparticipant {}", "x".repeat(129)),
        format!("sequenceDiagram\nA->>B: hi\n%%{}", "x".repeat(65536)),
        "sequenceDiagram\nA->>B: bad\0text".into(),
    ] {
        assert!(render_diagram_svg(&source, "mermaid").is_none());
    }
}

#[test]
fn shared_publication_paths_use_the_complete_sequence_figure_or_complete_code() {
    let markdown = format!("```mermaid\n{WORKFLOW}\n```\n");
    let svg = render_diagram_svg(WORKFLOW, "mermaid").unwrap();
    let html = render_html(&markdown, &HtmlOptions::default()).unwrap();
    assert!(html.contains(&svg));
    let pdf = render_pdf(&markdown, &PdfOptions::default()).unwrap();
    assert_eq!(pdf, render_pdf(&markdown, &PdfOptions::default()).unwrap());
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/S /Figure") && raw.contains("/Alt (Sequence diagram)"));
    assert!(!raw.contains("/Subtype /Image"), "diagram stays vector");
    let (poster, _, warnings) = franken_markdown::svg::render_svg_with_diagnostics(
        &parse_markdown(&markdown),
        &franken_markdown::svg::SvgOptions::default(),
    );
    assert!(warnings.is_empty());
    assert!(
        String::from_utf8(poster)
            .unwrap()
            .contains("aria-label=\"Sequence diagram\"")
    );

    let unsupported =
        "```mermaid\nsequenceDiagram\nA->>B: KEEPTHIS\nrect rgb(0,0,0)\nB->>A: KEEPTHAT\nend\n```";
    let layer = franken_markdown::pdf::verification_text_layer(
        &parse_markdown(unsupported),
        &PdfOptions::default(),
    )
    .unwrap();
    let runs = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .collect::<Vec<_>>();
    assert!(runs.iter().all(|run| run.kind != "image"));
    for text in ["KEEPTHIS", "KEEPTHAT", "rect rgb"] {
        assert!(runs.iter().any(|run| run.text.contains(text)), "{text}");
    }
}
