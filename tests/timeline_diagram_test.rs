//! Timeline grammar, ownership and emitted geometry through the public compiler.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{is_diagram_code, render_diagram_svg};

const ROADMAP: &str = concat!(
    "%% A complete source-owned timeline\n",
    "timeline\n",
    "title Roadmap & milestones\n",
    "section Discovery\n",
    "Ideas : Interview users : Map needs\n",
    "      : Review risks\n",
    "Prototype : Try layout\n",
    "section Delivery\n",
    "Build : Ship engine : Publish docs\n",
);

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

fn text(element: &str) -> &str {
    element
        .split_once('>')
        .unwrap()
        .1
        .strip_suffix("</text>")
        .unwrap()
}

fn labels(svg: &str, class: &str) -> Vec<String> {
    elements(svg, class)
        .into_iter()
        .map(|element| text(element).to_string())
        .collect()
}

fn assert_canvas(svg: &str) {
    let view: Vec<f32> = attr(svg.lines().next().unwrap(), "viewBox")
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(view.len(), 4);
    assert_eq!(&view[..2], &[0.0, 0.0]);
    assert!(view.iter().all(|value| value.is_finite()));
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
            x + width < view[2] && y + height < view[3],
            "{rect}\n{view:?}"
        );
    }
    for line in svg.lines().filter(|line| line.starts_with("<line ")) {
        let (x1, y1, x2, y2) = (
            number(line, "x1"),
            number(line, "y1"),
            number(line, "x2"),
            number(line, "y2"),
        );
        assert_ne!(
            (x1, y1),
            (x2, y2),
            "zero-length primitives must not enter Flow"
        );
        assert!([x1, x2].iter().all(|&x| x >= 0.0 && x <= view[2]));
        assert!([y1, y2].iter().all(|&y| y >= 0.0 && y <= view[3]));
    }
    for label in svg.lines().filter(|line| line.starts_with("<text ")) {
        assert!(
            number(label, "x") > 0.0 && number(label, "x") < view[2],
            "{label}"
        );
        assert!(
            number(label, "y") > 0.0 && number(label, "y") < view[3],
            "{label}"
        );
    }
    assert!(!svg.contains("=\"NaN\"") && !svg.contains("=\"inf\""));
}

#[test]
fn timeline_periods_events_and_continuations_keep_source_order() {
    let svg = render_diagram_svg(ROADMAP, "mermaid").unwrap();
    assert_eq!(svg, render_diagram_svg(ROADMAP, "timeline").unwrap());
    assert_eq!(svg, render_diagram_svg(ROADMAP, "").unwrap());
    assert_eq!(svg, render_diagram_svg(ROADMAP, "mermaid").unwrap());
    assert_eq!(
        svg,
        render_diagram_svg(&ROADMAP.replace("timeline\n", "timeline LR\n"), "mermaid").unwrap()
    );
    assert!(is_diagram_code(ROADMAP, ""));
    assert!(is_diagram_code("invalid source", "timeline"));
    assert!(svg.contains("<title>Roadmap &amp; milestones</title>"));
    assert_eq!(
        labels(&svg, "fmd-timeline-period-label"),
        ["Ideas", "Prototype", "Build"]
    );
    assert_eq!(
        labels(&svg, "fmd-timeline-event-label"),
        [
            "Interview users",
            "Map needs",
            "Review risks",
            "Try layout",
            "Ship engine",
            "Publish docs"
        ]
    );
    assert_eq!(
        labels(&svg, "fmd-timeline-section-label"),
        ["Discovery", "Delivery"]
    );
    assert!(svg.contains(
        "Discovery: Ideas: Interview users; Map needs; Review risks. Prototype: Try layout. Delivery: Build: Ship engine; Publish docs."
    ));
    assert_eq!(elements(&svg, "fmd-timeline-point").len(), 3);
    assert_eq!(elements(&svg, "fmd-timeline-arrow").len(), 1);
    assert_canvas(&svg);
}

#[test]
fn horizontal_periods_and_section_bands_have_disjoint_ordered_ownership() {
    let svg = render_diagram_svg(ROADMAP, "timeline").unwrap();
    let periods = elements(&svg, "fmd-timeline-period");
    let events = elements(&svg, "fmd-timeline-event");
    let sections = elements(&svg, "fmd-timeline-section");
    assert_eq!((periods.len(), events.len(), sections.len()), (3, 6, 2));
    for pair in periods.windows(2) {
        assert_eq!(number(pair[0], "y"), number(pair[1], "y"));
        assert!(number(pair[0], "x") + number(pair[0], "width") < number(pair[1], "x"));
    }
    assert_eq!(attr(periods[0], "fill"), attr(periods[1], "fill"));
    assert_ne!(attr(periods[1], "fill"), attr(periods[2], "fill"));
    assert_eq!(attr(periods[0], "fill"), attr(sections[0], "fill"));
    assert_eq!(attr(periods[2], "fill"), attr(sections[1], "fill"));
    assert_eq!(number(sections[0], "x"), number(periods[0], "x"));
    assert_eq!(
        number(sections[0], "x") + number(sections[0], "width"),
        number(periods[1], "x") + number(periods[1], "width")
    );
    assert_eq!(number(sections[1], "x"), number(periods[2], "x"));
    assert!(number(sections[0], "y") + number(sections[0], "height") < number(periods[0], "y"));
    for (index, event) in events.iter().enumerate() {
        let period = periods[if index < 3 {
            0
        } else if index == 3 {
            1
        } else {
            2
        }];
        assert_eq!(number(event, "x"), number(period, "x"));
        assert_eq!(attr(event, "fill"), attr(period, "fill"));
        assert!(number(event, "y") > number(period, "y") + number(period, "height"));
    }
    for group in [&events[..3], &events[4..]] {
        for pair in group.windows(2) {
            assert!(number(pair[0], "y") + number(pair[0], "height") < number(pair[1], "y"));
        }
    }
}

#[test]
fn top_down_layout_keeps_periods_in_order_and_places_events_beside_their_period() {
    let svg =
        render_diagram_svg(&ROADMAP.replace("timeline\n", "timeline TD\n"), "mermaid").unwrap();
    let periods = elements(&svg, "fmd-timeline-period");
    let events = elements(&svg, "fmd-timeline-event");
    assert_eq!(
        labels(&svg, "fmd-timeline-period-label"),
        ["Ideas", "Prototype", "Build"]
    );
    assert_eq!(labels(&svg, "fmd-timeline-event-label").len(), 6);
    for pair in periods.windows(2) {
        assert_eq!(number(pair[0], "x"), number(pair[1], "x"));
        assert!(number(pair[0], "y") + number(pair[0], "height") < number(pair[1], "y"));
    }
    for (index, event) in events.iter().enumerate() {
        let period = periods[if index < 3 {
            0
        } else if index == 3 {
            1
        } else {
            2
        }];
        assert!(number(event, "x") > number(period, "x") + number(period, "width"));
        assert_eq!(attr(event, "fill"), attr(period, "fill"));
    }
    let axis = elements(&svg, "fmd-timeline-axis")[0];
    assert_eq!(number(axis, "x1"), number(axis, "x2"));
    assert!(number(axis, "y2") > number(axis, "y1"));
    assert_canvas(&svg);
    let single = render_diagram_svg("timeline TD\nOne : Only event", "timeline").unwrap();
    assert_canvas(&single);
}

#[test]
fn same_named_periods_remain_distinct_and_section_changes_retire_continuations() {
    let svg = render_diagram_svg(
        "timeline\nBefore : Preface\nsection Work\nNow : First\nNow : Second\n : Third\nsection Next\nLater : Fourth",
        "timeline",
    ).unwrap();
    assert_eq!(
        labels(&svg, "fmd-timeline-period-label"),
        ["Before", "Now", "Now", "Later"]
    );
    assert_eq!(
        labels(&svg, "fmd-timeline-event-label"),
        ["Preface", "First", "Second", "Third", "Fourth"]
    );
    let periods = elements(&svg, "fmd-timeline-period");
    assert_ne!(attr(periods[0], "fill"), attr(periods[1], "fill"));
    assert_eq!(attr(periods[1], "fill"), attr(periods[2], "fill"));
    assert_ne!(attr(periods[2], "fill"), attr(periods[3], "fill"));
    assert!(
        render_diagram_svg(
            "timeline\nBefore : Visible\nsection Next\n : Must not attach to Before",
            "timeline"
        )
        .is_none()
    );
}

#[test]
fn unsectioned_periods_receive_individual_colors() {
    let svg = render_diagram_svg("timeline\nOne : A\nTwo : B\nThree : C", "timeline").unwrap();
    let periods = elements(&svg, "fmd-timeline-period");
    assert!(elements(&svg, "fmd-timeline-section").is_empty());
    assert_ne!(attr(periods[0], "fill"), attr(periods[1], "fill"));
    assert_ne!(attr(periods[1], "fill"), attr(periods[2], "fill"));
}

#[test]
fn explicit_breaks_entities_and_nonbreaking_spaces_survive_as_safe_text() {
    let svg = render_diagram_svg(
        concat!(
            "timeline\n",
            "title <Roadmap & plans>\n",
            "section Early<br>work\n",
            "P #58; Q : Before<br/>After<br />Last : &lt;script&gt; : 12&#160;345&#8239;678\n",
            "Literal : &lt;br&gt; : #9829; &amp; #infin;\n",
        ),
        "timeline",
    )
    .unwrap();
    assert!(
        svg.lines()
            .next()
            .unwrap()
            .contains("xml:space=\"preserve\"")
    );
    assert_eq!(
        labels(&svg, "fmd-timeline-period-label"),
        ["P : Q", "Literal"]
    );
    assert_eq!(
        labels(&svg, "fmd-timeline-section-label"),
        ["Early", "work"]
    );
    assert_eq!(
        labels(&svg, "fmd-timeline-event-label"),
        [
            "Before",
            "After",
            "Last",
            "&lt;script&gt;",
            "12\u{00a0}345\u{202f}678",
            "&lt;br&gt;",
            "♥ &amp; ∞"
        ]
    );
    assert_eq!(elements(&svg, "fmd-timeline-event").len(), 5);
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<br"));
    assert!(svg.contains("<title>&lt;Roadmap &amp; plans&gt;</title>"));
    assert_canvas(&svg);
}

#[test]
fn literal_markup_and_attribute_looking_labels_never_acquire_svg_authority() {
    let source = concat!(
        "timeline\n",
        "title </title><script>alert(1)</script>\n",
        "<img onerror=alert(1)> : \"><svg onload=alert(1)> : <foreignObject>body</foreignObject>\n",
    );
    let svg = render_diagram_svg(source, "timeline").unwrap();
    assert_eq!(svg.matches("<svg ").count(), 1);
    assert_eq!(svg.matches("<title>").count(), 1);
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<foreignObject>"));
    assert!(!svg.contains("<img "));
    assert!(!svg.contains("href="));
    assert!(svg.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(svg.contains("&lt;foreignObject&gt;body&lt;/foreignObject&gt;"));
    assert_canvas(&svg);
}

#[test]
fn wrapped_words_and_combining_clusters_keep_all_source_characters() {
    let word = "antidisestablishmentarianism".repeat(6);
    let marked = "a\u{0301}".repeat(30);
    for direction in ["LR", "TD"] {
        let source = format!("timeline {direction}\nPhase : {word} : {marked}");
        let svg = render_diagram_svg(&source, "timeline").unwrap();
        let text = labels(&svg, "fmd-timeline-event-label");
        assert!(text.len() > 3);
        assert_eq!(text.concat(), format!("{word}{marked}"));
        assert!(text.iter().all(|line| !line.starts_with('\u{0301}')));
        assert_canvas(&svg);
    }
    let bound_phrase = format!("{}{}{}", "a".repeat(30), '\u{00a0}', "b".repeat(30));
    assert!(render_diagram_svg(&format!("timeline\nPhase : {bound_phrase}"), "timeline").is_none());
    let huge_cluster = format!("a{}", "\u{0301}".repeat(200));
    assert!(render_diagram_svg(&format!("timeline\nPhase : {huge_cluster}"), "timeline").is_none());
}

#[test]
fn complete_comments_newline_conventions_and_literal_comment_markers_are_distinct() {
    let source = "%% comment\ntimeline\n%% ignored\nPhase : literal %% text\n : next\n";
    let expected = render_diagram_svg(source, "timeline").unwrap();
    assert_eq!(
        labels(&expected, "fmd-timeline-event-label"),
        ["literal %% text", "next"]
    );
    for newline in ["\r\n", "\r"] {
        assert_eq!(
            expected,
            render_diagram_svg(&source.replace('\n', newline), "timeline").unwrap()
        );
    }
    assert!(!expected.contains("ignored"));
}

#[test]
fn malformed_or_unsupported_timelines_refuse_the_complete_figure() {
    for source in [
        "timeline",
        "timeline RL\nPeriod : Event",
        "timeline TD extra\nPeriod : Event",
        "timeline\n: Orphan",
        "timeline\nPeriod",
        "timeline\nPeriod :",
        "timeline\nPeriod : First :: Last",
        "timeline\nPeriod : First\n :",
        "timeline\ntitle\nPeriod : Event",
        "timeline\ntitle First\ntitle Second\nPeriod : Event",
        "timeline\nsection\nPeriod : Event",
        "timeline\nsection Empty\nsection Nonempty\nPeriod : Event",
        "timeline\nPeriod : Event\nsection Empty",
        "timeline\nPeriod : Event\ntimeline",
        "timeline\nPeriod : Event\nstyle P fill:red",
        "timeline\nPeriod : Event\nclassDef custom fill:red",
        "timeline\nPeriod : Event\nclick P href \"https://example.com\"",
        "timeline\nPeriod : Event\naccTitle: Unhandled metadata",
        "%%{init: {'timeline': {'disableMulticolor': true}}}%%\ntimeline\nPeriod : Event",
        "---\nconfig:\n  theme: dark\n---\ntimeline\nPeriod : Event",
        "timeline\nPeriod : bad\0label",
        "timeline\nPeriod : &#0;",
        "timeline\nPeriod : &#x110000;",
        "timeline\nPeriod : <br><br>",
    ] {
        assert!(
            render_diagram_svg(source, "timeline").is_none(),
            "accepted {source:?}"
        );
        assert!(
            render_diagram_svg(source, "mermaid").is_none(),
            "accepted {source:?}"
        );
    }
}

#[test]
fn period_event_and_section_limits_admit_the_boundary_without_partial_output() {
    let periods = (0..64)
        .map(|n| format!("P{n} : Event {n}\n"))
        .collect::<String>();
    let source = format!("timeline\n{periods}");
    let svg = render_diagram_svg(&source, "timeline").unwrap();
    assert_eq!(elements(&svg, "fmd-timeline-period").len(), 64);
    assert!(render_diagram_svg(&(source + "Extra : Last"), "timeline").is_none());

    let events = (0..256).map(|n| format!(" : E{n}")).collect::<String>();
    let source = format!("timeline\nPeriod{events}");
    let svg = render_diagram_svg(&source, "timeline").unwrap();
    assert_eq!(elements(&svg, "fmd-timeline-event").len(), 256);
    assert!(render_diagram_svg(&(source + " : extra"), "timeline").is_none());

    let sections = (0..32)
        .map(|n| format!("section S{n}\nP{n} : E{n}\n"))
        .collect::<String>();
    let source = format!("timeline\n{sections}");
    let svg = render_diagram_svg(&source, "timeline").unwrap();
    assert_eq!(elements(&svg, "fmd-timeline-section").len(), 32);
    assert!(render_diagram_svg(&(source + "section Extra\nLast : Event"), "timeline").is_none());
}

#[test]
fn source_label_and_wrapped_line_budgets_refuse_without_truncating_text() {
    let label = "x".repeat(1024);
    let source = format!("timeline\ntitle {label}\n{label} : {label}");
    assert!(render_diagram_svg(&source, "timeline").is_some());
    for source in [
        format!("timeline\ntitle {label}x\nP : E"),
        format!("timeline\n{label}x : E"),
        format!("timeline\nP : {label}x"),
        format!("timeline\nsection {label}x\nP : E"),
    ] {
        assert!(render_diagram_svg(&source, "timeline").is_none());
    }

    let prefix = "timeline\nP : E\n%%";
    let exact = format!("{prefix}{}", "x".repeat(64 * 1024 - prefix.len()));
    assert!(render_diagram_svg(&exact, "timeline").is_some());
    assert!(render_diagram_svg(&(exact + "x"), "timeline").is_none());
    assert!(
        render_diagram_svg(
            &format!("{}timeline\nP : E", " ".repeat(64 * 1024)),
            "timeline"
        )
        .is_none()
    );

    let events = |breaks: usize| {
        let label = format!("{}x", "x<br>".repeat(breaks));
        let periods = (0..64)
            .map(|n| format!("P{n}{}\n", format!(" : {label}").repeat(4)))
            .collect::<String>();
        format!("timeline\n{periods}")
    };
    assert!(render_diagram_svg(&events(6), "timeline").is_some());
    assert!(render_diagram_svg(&events(7), "timeline").is_none());
}

#[test]
fn explicit_fences_keep_their_grammar_and_nearby_headers_do_not_match() {
    for source in [
        "flowchart LR\nA --> B",
        "sequenceDiagram\nA->>B: Call",
        "pie\n\"Amount\": 1",
        "stateDiagram-v2\nA --> B",
    ] {
        assert!(render_diagram_svg(source, "timeline").is_none());
    }
    assert!(
        render_diagram_svg("timeline --> release", "flowchart")
            .unwrap()
            .contains("fmd-flowchart")
    );
    assert!(
        render_diagram_svg("timeline ->> release: message", "sequence")
            .unwrap()
            .contains("fmd-sequence")
    );
    assert!(render_diagram_svg("timeline\nP : E", "pie").is_none());
    assert!(!is_diagram_code("timelines\nP : E", ""));
    assert!(!is_diagram_code("timelineExtra\nP : E", ""));
    assert!(render_diagram_svg("gantt\nsection Work\nTask :2026-01-01, 1d", "mermaid").is_none());
}

#[test]
fn complete_canvases_obey_shared_svg_side_and_area_admission() {
    let single_period = |label: &str| format!("timeline\nP{}", format!(" : {label}").repeat(256));
    let tall = single_period("First<br>Second<br>Third");
    let svg = render_diagram_svg(&single_period("Event"), "timeline").unwrap();
    assert_canvas(&svg);
    assert!(number(svg.lines().next().unwrap(), "height") <= 16_384.0);
    assert!(render_diagram_svg(&tall, "timeline").is_none());

    let wide = |event_count: usize| {
        let label = "One<br>Two<br>Three<br>Four<br>Five<br>Six<br>Seven";
        let first = format!("P0{}\n", format!(" : {label}").repeat(event_count));
        let rest = (1..64)
            .map(|n| format!("P{n} : Event\n"))
            .collect::<String>();
        format!("timeline\n{first}{rest}")
    };
    let svg = render_diagram_svg(&wide(32), "timeline").unwrap();
    let root = svg.lines().next().unwrap();
    let area = f64::from(number(root, "width")) * f64::from(number(root, "height"));
    assert!(area < 100_000_000.0);
    assert!(render_diagram_svg(&wide(64), "timeline").is_none());

    let vertical = (0..64)
        .map(|n| format!("P{n} : First : Second : Third : Fourth\n"))
        .collect::<String>();
    assert!(render_diagram_svg(&format!("timeline TD\n{vertical}"), "timeline").is_some());
    assert!(
        render_diagram_svg(
            &format!(
                "timeline TD\n{}",
                vertical
                    .replace("First", "First<br>Again")
                    .replace("Second", "Second<br>Again")
                    .replace("Third", "Third<br>Again")
                    .replace("Fourth", "Fourth<br>Again")
            ),
            "timeline"
        )
        .is_none()
    );
}
