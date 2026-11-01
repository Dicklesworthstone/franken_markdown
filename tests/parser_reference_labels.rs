//! Exact edit locations must come from accepted parser nodes, not text search.
#![allow(clippy::expect_used)]
use franken_markdown::parse::{ReferenceLabelForm as Form, reference_label_spans};

#[test]
fn all_reference_forms_keep_visible_labels_separate_from_replacement_tokens() {
    let source =
        "[shown][ref] [ref][] [ref] ![alt][ref] ![ref][] ![ref]\n\n[ref]: /asset \"Title\"\n";
    let spans = reference_label_spans(source);
    let actual: Vec<_> = spans
        .iter()
        .map(|span| {
            (
                span.form,
                span.label.as_ref().map(|range| &source[range.clone()]),
                span.replacement
                    .as_ref()
                    .map(|range| &source[range.clone()]),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (Form::Full, Some("ref"), Some("ref")),
            (Form::Collapsed, Some("ref"), Some("[]")),
            (Form::Shortcut, Some("ref"), Some("")),
            (Form::Full, Some("ref"), Some("ref")),
            (Form::Collapsed, Some("ref"), Some("[]")),
            (Form::Shortcut, Some("ref"), Some("")),
            (Form::Definition, Some("ref"), Some("ref")),
        ]
    );
    for span in spans {
        assert_eq!(span.key, "ref");
        if span.form == Form::Shortcut {
            let end = span.replacement.expect("exact insertion").start;
            assert_eq!(&source[end - 1..end], "]");
        }
    }
}

#[test]
fn unicode_identity_retains_unused_shadowed_and_whitespace_variant_definitions() {
    let source =
        "[shown][STRASSE]\n\n[Straße]: /first\n[strasse]: /shadow\n[ UNUSED\t KEY ]: /unused\n";
    let spans = reference_label_spans(source);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.key.as_str())
            .collect::<Vec<_>>(),
        ["strasse", "strasse", "strasse", "unused key"]
    );
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.form == Form::Definition)
            .count(),
        3
    );
}

#[test]
fn container_copies_keep_original_crlf_bom_tabs_and_repeated_use_offsets() {
    let source = "\u{feff}> [ref]: /target\r\n>\r\n> 😀 [a][ref]\r\n\r\n-\t[b][ref]\r\n\r\n| Cell |\r\n| --- |\r\n| [c][ref] |\r\n\r\n[^unused]: [d][ref]\r\n\r\n[long repeated text][ref]\r\n\r\n[long repeated text][ref]\r\n";
    let spans = reference_label_spans(source);
    let expected: Vec<_> = source
        .match_indices("ref")
        .map(|(start, _)| start..start + 3)
        .collect();
    assert_eq!(
        spans
            .iter()
            .map(|span| span.label.clone().expect("exact source"))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        spans.iter().filter(|span| span.form == Form::Full).count(),
        6
    );
}

#[test]
fn literal_inline_urls_code_math_html_and_escaped_reference_lookalikes_are_excluded() {
    let source = "[ref]: /target\n\n`[ref]` $[ref]$ \\[ref] [text](/ref) ![alt](/ref) [missing]\n\n```md\n[ref]\n[code]: /fake\n```\n\n<div>\n[ref]\n[html]: /fake\n</div>\n\n    [ref]\n    [indented]: /fake\n\nparagraph\n[paragraph]: /fake\n";
    let spans = reference_label_spans(source);
    assert_eq!(
        spans.len(),
        1,
        "literal spelling must not become a symbol: {spans:?}"
    );
    assert_eq!(spans[0].form, Form::Definition);
}

#[test]
fn image_alt_keeps_accepted_labels_while_rejected_outer_links_discard_speculation() {
    let source = "![outer [inner]][picture]\n\n[outer [inner]][picture]\n\n[inner]: /inside\n[picture]: /image\n";
    let spans = reference_label_spans(source);
    let uses: Vec<_> = spans
        .iter()
        .filter(|span| span.form != Form::Definition)
        .collect();
    // Accepted references inside an image determine its flattened alt text.
    // The rejected outer link reparses its inner link, without duplicate spans.
    assert_eq!(uses.len(), 4, "{spans:?}");
    assert_eq!(
        uses.iter()
            .map(|span| span.key.as_str())
            .collect::<Vec<_>>(),
        ["inner", "picture", "inner", "picture"]
    );
    assert!(
        uses[2].label.as_ref().expect("inner source").start
            > source.find("\n\n").expect("second paragraph")
    );
}

#[test]
fn noncontiguous_multiline_labels_remain_visible_as_uneditable_occurrences() {
    let source = "> [two\n> words]: /target\n>\n> [shown][two words]\n> [two\n> words][]\n";
    let spans = reference_label_spans(source);
    assert_eq!(spans.len(), 3, "{spans:?}");
    assert!(spans.iter().all(|span| span.key == "two words"));
    assert!(
        spans
            .iter()
            .any(|span| span.form == Form::Definition && span.label.is_none())
    );
    assert!(
        spans
            .iter()
            .any(|span| span.form == Form::Collapsed && span.label.is_none())
    );
    assert!(
        spans
            .iter()
            .any(|span| span.form == Form::Full && span.label.is_some())
    );
}

#[test]
fn escaped_labels_keep_literal_identity_and_exact_source_spelling() {
    let source = "[shown][a\\]b] [shown][a&amp;b]\n\n[a\\]b]: /one\n[a&amp;b]: /two\n";
    let spans = reference_label_spans(source);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.key.as_str())
            .collect::<Vec<_>>(),
        ["a\\]b", "a&amp;b", "a\\]b", "a&amp;b"]
    );
    assert!(spans.iter().all(|span| {
        span.label
            .as_ref()
            .is_some_and(|range| source[range.clone()] == span.key)
    }));
}

#[test]
fn ordinary_destination_tracking_keeps_its_existing_contract() {
    let source =
        "[visible][ref] ![alt][ref] [explicit](<file.md#part>)\n\n[ref]: /asset \"Title\"\n";
    let before = franken_markdown::parse::destination_spans(source);
    let _labels = reference_label_spans(source);
    assert_eq!(before, franken_markdown::parse::destination_spans(source));
    assert_eq!(
        before
            .iter()
            .map(|span| (&source[span.range.clone()], span.dest.as_str()))
            .collect::<Vec<_>>(),
        [("<file.md#part>", "file.md#part"), ("/asset", "/asset")]
    );
}

#[test]
fn exhausted_inline_work_revokes_accepted_reference_labels_and_recovers_per_paragraph() {
    let hostile = format!("[discard][ref] {}end", "$value ".repeat(4096));
    let source = format!("[ref]: /target\n\n{hostile}\n\n[good][ref]\n\n{hostile}\n\n[good][ref]");
    let spans = reference_label_spans(&source);
    assert_eq!(spans.len(), 3, "fallback text has no accepted references");
    assert_eq!(spans[0].form, Form::Definition);
    let positions: Vec<_> = source
        .match_indices("[good][ref]")
        .map(|(start, _)| start + 7..start + 10)
        .collect();
    assert_eq!(
        spans[1..]
            .iter()
            .map(|span| span.label.clone().expect("exact surviving use"))
            .collect::<Vec<_>>(),
        positions
    );
    assert!(spans[1..].iter().all(|span| span.form == Form::Full));
}
