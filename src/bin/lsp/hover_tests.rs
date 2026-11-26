#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{Phase, Server, position};
use super::*;

const ORIGIN: &str = "file:///manual/start.md";
const TARGET: &str = "file:///manual/other.md";

fn opened(sources: &[(&str, &str)]) -> Server {
    Server {
        phase: Phase::Running,
        hover_format: Format::Plaintext,
        documents: sources
            .iter()
            .map(|(uri, source)| {
                (
                    (*uri).to_string(),
                    Buffer {
                        text: (*source).to_string(),
                        version: 1,
                        synchronized: true,
                    },
                )
            })
            .collect(),
        ..Server::default()
    }
}

fn hover_at(server: &Server, uri: &str, needle: &str) -> Result<Json, Failure> {
    let source = &server.documents[uri].text;
    let offset = source.find(needle).expect("fixture cursor");
    let params = object([
        ("textDocument", object([("uri", string(uri))])),
        (
            "position",
            position(LineIndex::new(source).position(source, offset)),
        ),
    ]);
    server.navigate("textDocument/hover", &params)
}

fn content(result: &Json) -> &str {
    let contents = result.get("contents").expect("hover contents");
    assert_eq!(contents.get("kind").and_then(Json::as_str), Some("plaintext"));
    assert!(
        result.get("range").is_none(),
        "target ranges are not origin ranges"
    );
    contents
        .get("value")
        .and_then(Json::as_str)
        .expect("hover text")
}

#[test]
fn heading_hover_includes_authored_section_content_and_stops_at_its_sibling() {
    let source = "[Read](#topic)\n\n# Topic\n\nUseful **content** and [link](command:unsafe).\n\n## Detail\n\nChild content.\n\n# Next\n\nOther section.\n";
    let server = opened(&[(ORIGIN, source)]);
    let result = hover_at(&server, ORIGIN, "#topic").unwrap();
    let value = content(&result);
    assert!(value.starts_with("# Topic\n\nUseful **content**"));
    assert!(
        value.contains("[link](command:unsafe)"),
        "source is inert plaintext"
    );
    assert!(value.contains("## Detail\n\nChild content."));
    assert!(!value.contains("# Next"));
    assert!(!value.contains("[Read]"));
    assert_eq!(server.documents[ORIGIN].text, source);
}

#[test]
fn reference_uses_resolve_the_actual_other_buffer_and_setext_heading() {
    let source = "[Read][manual]\n\n[manual]: other.md#topic\n";
    let target = "Topic\n=====\n\nCurrent target text.\n\nNext\n====\n\nOther section.\n";
    let server = opened(&[(ORIGIN, source), (TARGET, target)]);
    let result = hover_at(&server, ORIGIN, "Read").unwrap();
    assert_eq!(content(&result), "Topic\n=====\n\nCurrent target text.");
}

#[test]
fn multiline_footnotes_show_the_definition_body_without_following_prose() {
    let source = "Text[^n].\n\n[^n]: First **paragraph**.\n\n    Second paragraph with a [link](https://example.test/).\n\nUnrelated prose.\n";
    let server = opened(&[(ORIGIN, source)]);
    let result = hover_at(&server, ORIGIN, "^n").unwrap();
    let value = content(&result);
    assert!(value.starts_with("[^n]: First **paragraph**."));
    assert!(value.contains("Second paragraph"));
    assert!(!value.contains("Unrelated prose"));
    assert!(!value.contains("Text[^n]"));
}

#[test]
fn bare_open_file_links_show_leading_document_content() {
    let source = "[Read](other.md)";
    let target = "Introductory paragraph.\n\nA second paragraph.\n\n# Section\n\nLater text.";
    let server = opened(&[(ORIGIN, source), (TARGET, target)]);
    let result = hover_at(&server, ORIGIN, "other.md").unwrap();
    assert_eq!(
        content(&result),
        "Introductory paragraph.\n\nA second paragraph."
    );
}

#[test]
fn nested_targets_keep_their_honest_enclosing_parser_block() {
    let source = "[Read](#nested)\n\n> ## Nested\n>\n> Quoted content.\n\nFollowing prose.";
    let server = opened(&[(ORIGIN, source)]);
    let result = hover_at(&server, ORIGIN, "#nested").unwrap();
    let value = content(&result);
    assert!(value.contains("> ## Nested"));
    assert!(value.contains("> Quoted content."));
    assert!(!value.contains("Following prose"));
}

#[test]
fn literal_unresolved_external_and_ambiguous_targets_have_no_hover() {
    for (source, needle) in [
        ("\x60[Read](#topic)\x60\n\n# Topic", "#topic"),
        ("\x60\x60\x60md\n[Read](#topic)\n\x60\x60\x60\n\n# Topic", "#topic"),
        ("![Image](#topic)\n\n# Topic", "#topic"),
        ("[Read](#missing)\n\n# Topic", "#missing"),
        ("[Read](missing.md)", "missing.md"),
        ("[Read](https://example.test/)", "https://"),
        ("Unresolved[^missing].", "^missing"),
    ] {
        let server = opened(&[(ORIGIN, source)]);
        assert_eq!(hover_at(&server, ORIGIN, needle).unwrap(), Json::Null);
    }
    let server = opened(&[
        (ORIGIN, "[Read](other.md#topic)"),
        (TARGET, "# Topic\n\nFirst buffer."),
        ("file:///manual/%6Fther.md", "# Topic\n\nAlias buffer."),
    ]);
    assert_eq!(hover_at(&server, ORIGIN, "other.md").unwrap(), Json::Null);
}

#[test]
fn current_target_text_is_used_and_unsynchronized_buffers_are_refused() {
    let mut server = opened(&[
        (ORIGIN, "😀 [Read](other.md#topic)\r\n"),
        (TARGET, "# Topic\r\n\r\nOriginal."),
    ]);
    assert!(content(&hover_at(&server, ORIGIN, "other.md").unwrap()).contains("Original."));
    server.documents.get_mut(TARGET).unwrap().text = "# Topic\r\n\r\nChanged.".into();
    assert!(content(&hover_at(&server, ORIGIN, "other.md").unwrap()).contains("Changed."));
    server.documents.get_mut(TARGET).unwrap().synchronized = false;
    assert!(matches!(
        hover_at(&server, ORIGIN, "other.md"),
        Err((-32801, _))
    ));
    server.documents.get_mut(TARGET).unwrap().synchronized = true;
    server.documents.get_mut(ORIGIN).unwrap().synchronized = false;
    assert!(matches!(
        hover_at(&server, ORIGIN, "other.md"),
        Err((-32801, _))
    ));
}

#[test]
fn selected_profile_preserves_multiline_link_ownership() {
    let source = "😀 [Term\r\n: definition](other.md#target)\r\n";
    for (profile, visible) in [("commonmark-gfm", true), ("gfm-plus", false)] {
        let mut server = opened(&[
            (ORIGIN, source),
            (TARGET, "# Target\r\n\r\nProfile-aware target content."),
        ]);
        server.parser = Parser::initialize(&object([(
            "initializationOptions",
            object([("profile", string(profile))]),
        )]))
        .unwrap();
        let result = hover_at(&server, ORIGIN, "other.md").unwrap();
        if visible {
            assert!(content(&result).contains("Profile-aware target content."));
        } else {
            assert_eq!(result, Json::Null);
        }
    }
}

#[test]
fn hover_formats_follow_client_preference_and_keep_legacy_clients_compatible() {
    assert_eq!(Format::negotiate(None), Format::Legacy);
    assert_eq!(Format::negotiate(Some(&object([]))), Format::Legacy);
    let cases: &[(&[&str], Format)] = &[
        (&["plaintext"], Format::Plaintext),
        (&["markdown"], Format::Markdown),
        (&["markdown", "plaintext"], Format::Markdown),
        (&["plaintext", "markdown"], Format::Plaintext),
        (&["future-format", "markdown"], Format::Markdown),
        (&["future-format"], Format::Disabled),
        (&[], Format::Disabled),
    ];
    for &(formats, expected) in cases {
        let capabilities = object([(
            "hover",
            object([(
                "contentFormat",
                Json::Array(formats.iter().map(|value| string(value)).collect()),
            )]),
        )]);
        assert_eq!(Format::negotiate(Some(&capabilities)), expected);
    }
}

#[test]
fn markdown_only_hover_cannot_activate_authored_fences_html_or_commands() {
    let source = "\x60\x60\x60\x60\n<img src=\"https://example.test/private\">\n\x60\x60\x60\x60\n[Run](command:danger)";
    let encoded = Format::Markdown.contents(source.into()).unwrap();
    assert_eq!(encoded.get("kind").and_then(Json::as_str), Some("markdown"));
    let value = encoded.get("value").and_then(Json::as_str).unwrap();
    let fence = "\x60".repeat(5);
    assert_eq!(value, format!("{fence}\n{source}\n{fence}"));
    let legacy = Format::Legacy.contents(source.into()).unwrap();
    assert_eq!(legacy.get("language").and_then(Json::as_str), Some("markdown"));
    assert_eq!(legacy.get("value").and_then(Json::as_str), Some(source));
    assert!(legacy.get("kind").is_none());

    let dense = "\x60".repeat(MAX_PREVIEW_BYTES);
    let bounded = preview(&dense);
    let encoded = Format::Markdown.contents(bounded.clone()).unwrap();
    let value = encoded.get("value").and_then(Json::as_str).unwrap();
    assert!(value.len() <= MAX_HOVER_BYTES);
    assert!(value.contains(&bounded));
    assert!(value.lines().next().unwrap().len() > bounded.lines().next().unwrap().len());
}

#[test]
fn preview_budgets_preserve_scalars_and_count_all_editor_line_endings() {
    let large = "😀".repeat(MAX_PREVIEW_BYTES);
    let value = preview(&large);
    assert!(value.len() <= MAX_PREVIEW_BYTES);
    assert!(value.ends_with(TRUNCATION));
    assert!(
        value
            .trim_end_matches(TRUNCATION)
            .chars()
            .all(|ch| ch == '😀')
    );
    for separator in ["\n", "\r\n", "\r"] {
        let lines = (0..80)
            .map(|line| format!("source line {line}"))
            .collect::<Vec<_>>()
            .join(separator);
        let value = preview(&lines);
        assert!(value.contains("source line 47"));
        assert!(!value.contains("source line 48"));
        assert!(value.ends_with(TRUNCATION));
        assert!(value.len() <= MAX_PREVIEW_BYTES);
    }
}
