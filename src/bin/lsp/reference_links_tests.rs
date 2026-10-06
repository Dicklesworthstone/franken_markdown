#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use super::super::{Json, LineIndex, Position, object, position, request, string};
use super::super::super::{Buffer, Phase, Server, number};

const URI: &str = "file:///docs/source.md";

fn marked(source: &str) -> (String, usize) {
    let offset = source.find('¦').expect("caret marker");
    (source.replace('¦', ""), offset)
}

fn params(source: &str, offset: usize) -> Json {
    object([
        ("textDocument", object([("uri", string(URI))])),
        ("position", position(LineIndex::new(source).position(source, offset))),
    ])
}

fn call(method: &str, source: &str) -> Json {
    let (source, offset) = marked(source);
    request(method, &params(&source, offset), URI, &source).expect("navigation")
}

fn start(result: &Json) -> Option<&Json> {
    result.get("range").and_then(|range| range.get("start"))
}

#[test]
fn full_collapsed_and_shortcut_references_resolve_from_labels_or_link_text() {
    for body in [
        "[Text][gui¦de]", "[Te¦xt][guide]", "[gui¦de][]", "[guide][¦]", "[gui¦de]",
    ] {
        let source = format!("# Target\n\n{body}\n\n[guide]: #target\n");
        let result = call("textDocument/definition", &source);
        assert_eq!(result.get("uri").and_then(Json::as_str), Some(URI), "{body}");
        assert_eq!(start(&result), Some(&position(Position { line: 0, character: 0 })), "{body}");
    }
}

#[test]
fn parser_owns_case_whitespace_forward_definitions_and_first_definition_precedence() {
    let source = "[go][  A   B¦ ]\n\n[a b]: #first\n[A B]: #second\n\n# First\n\n# Second\n";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 5, character: 0 })));
    let source = "[go][Éco¦le]\n\n[école]: #target\n\n# Target\n";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 4, character: 0 })));
}

#[test]
fn reference_titles_and_formatted_text_are_preserved_during_probe_restoration() {
    for body in ["[**Te¦xt**][id]", "[**Text**][i¦d]", "[text [nest¦ed]][id]"] {
        let source = format!("# Target\n\n{body}\n\n[id]: <#target> \"Keep &amp; title\"\n");
        assert_eq!(start(&call("textDocument/definition", &source)), Some(&position(Position { line: 0, character: 0 })), "{body}");
    }
    let source = "# Target\n\n[go][i¦d]\n\n[id]:\n  <#target>\n  'multiline definition title'\n";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 0, character: 0 })));
}

#[test]
fn reference_uses_work_inside_every_container_and_only_visible_footnotes() {
    for body in [
        "> [go][i¦d]", "- [go][i¦d]", "> - **[go][i¦d]**",
        "| [go][i¦d] |\n| --- |\n| cell |",
        "| head |\n| --- |\n| [go][i¦d] |",
        "Term\n: [go][i¦d]", "[go][i¦d]\n: description",
        "Use[^n].\n\n[^n]: [go][i¦d]",
    ] {
        let source = format!("# Target\n\n{body}\n\n[id]: #target \"Title\"\n");
        assert_eq!(start(&call("textDocument/definition", &source)), Some(&position(Position { line: 0, character: 0 })), "{body}");
    }
    assert_eq!(call("textDocument/definition", "# Target\n\n[^n]: [go][i¦d]\n\n[id]: #target"), Json::Null);
}

#[test]
fn code_images_raw_html_math_and_undefined_labels_are_not_navigation_sources() {
    for body in [
        "`[go][i¦d]`", "![go][i¦d]", "![i¦d][]", "![i¦d]", "[go][miss¦ing]",
        "```md\n[go][i¦d]\n```", "<span title=\"[go][i¦d]\">x</span>",
        "[go](#target \"[go][i¦d]\")", "$[go][i¦d]$", "plain i¦d",
        "- `[go][i¦d]`", "| head |\n| --- |\n| ![go][i¦d] |",
        "\\[i¦d]",
    ] {
        let source = format!("# Target\n\n{body}\n\n[id]: #target\n");
        assert_eq!(call("textDocument/definition", &source), Json::Null, "{body}");
    }
}

#[test]
fn adjacent_reference_precedence_is_parser_proven_and_equivalent_ambiguity_is_inert() {
    let source = "[foo][ba¦r][baz]\n\n[bar]: #first\n[baz]: #second\n\n# First\n\n# Second";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 5, character: 0 })));
    let source = "[foo][ba¦r][baz]\n\n[baz]: #second\n\n# Second";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 4, character: 0 })));
    // Both possible middle-bracket interpretations recreate the identical AST.
    let ambiguous = "[x][x¦][x]\n\n[x]: #target\n\n# Target";
    assert_eq!(call("textDocument/definition", ambiguous), Json::Null);
}

#[test]
fn missing_malformed_and_ambiguous_targets_never_fabricate_a_definition() {
    for destination in ["#missing", "#%zz", "#fn-n", "https://example.invalid/#target"] {
        let source = format!("# fn-n\n\nUse[^n]. [go][i¦d]\n\n[^n]: note\n\n[id]: {destination}\n");
        assert_eq!(call("textDocument/definition", &source), Json::Null, "{destination}");
    }
    let root = call("textDocument/definition", "# Target\n\n[go][i¦d]\n\n[id]: <>\n");
    let zero = position(Position { line: 0, character: 0 });
    assert_eq!(start(&root), Some(&zero));
    assert_eq!(root.get("range").and_then(|r| r.get("end")), Some(&zero));
}

#[test]
fn escaped_labels_and_utf16_positions_use_the_existing_parser() {
    let source = "😀 [go][a\\]b¦]\r\n\r\n[a\\]b]: #%74arget\r\n\r\n# Target\r\n";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 4, character: 0 })));
}

#[test]
fn reference_labels_do_not_receive_fragment_completion_edits() {
    let result = call("textDocument/completion", "# Target\n\n[go][tar¦get]\n\n[target]: #target\n");
    assert_eq!(result.get("items"), Some(&Json::Array(Vec::new())));
}

#[test]
fn source_scanner_and_probe_work_have_explicit_budgets() {
    let source = "[".repeat(MAX_CANDIDATES + 1) + "x" + &"]".repeat(MAX_CANDIDATES + 1);
    assert!(matches!(candidates(&source, SourceSpan::new(0, source.len()), MAX_CANDIDATES + 1), Err((-32803, _))));
    let source = format!("[{}]", "a".repeat(MAX_DESTINATION_BYTES));
    assert!(matches!(candidates(&source, SourceSpan::new(0, source.len()), 2), Err((-32803, _))));
    let source = "x".repeat(MAX_BLOCK_BYTES + 1);
    assert!(matches!(candidates(&source, SourceSpan::new(0, source.len()), 2), Err((-32803, _))));
    assert!(candidates("[multi\nline]", SourceSpan::new(0, 12), 2).unwrap().is_empty());
    let collisions: String = (0..32).map(|n| format!("#fmd-lsp-probe-{n} ")).collect();
    let source = format!("# Target\n\n{collisions}\n\n[go][i¦d]\n\n[id]: #target");
    let (source, offset) = marked(&source);
    assert!(matches!(request("textDocument/definition", &params(&source, offset), URI, &source), Err((-32803, _))));
}

#[test]
fn cross_file_references_follow_current_buffer_revisions_and_exact_target_uri() {
    let (source, offset) = marked("- 😀 [go][gui¦de]\r\n\r\n[guide]: target.md#target \"Read\"\r\n");
    let target = "file:///docs/target.md";
    let mut server = Server { phase: Phase::Running, ..Server::default() };
    for (uri, text) in [(URI, source.as_str()), (target, "# Intro\r\n\r\n# Target\r\n")] {
        server.documents.insert(uri.into(), Buffer { text: text.into(), version: 1, synchronized: true });
    }
    let params = params(&source, offset);
    let result = server.navigate("textDocument/definition", &params).unwrap();
    assert_eq!(result.get("uri").and_then(Json::as_str), Some(target));
    assert_eq!(start(&result), Some(&position(Position { line: 2, character: 0 })));
    let changed = source.replace("target.md#target", "target.md#intro");
    server.change(&object([
        ("textDocument", object([("uri", string(URI)), ("version", number(2))])),
        ("contentChanges", Json::Array(vec![object([("text", string(&changed))])])),
    ])).unwrap();
    assert_eq!(start(&server.navigate("textDocument/definition", &params).unwrap()), Some(&position(Position { line: 0, character: 0 })));
    server.documents.get_mut(target).unwrap().synchronized = false;
    assert!(matches!(server.navigate("textDocument/definition", &params), Err((-32801, _))));
    server.close(&object([("textDocument", object([("uri", string(target))]))])).unwrap();
    assert_eq!(server.navigate("textDocument/definition", &params).unwrap(), Json::Null);
}

#[test]
fn parser_probe_is_unique_even_when_existing_destinations_use_entities() {
    let source = "# Target\n\n[other](#fmd&#45;lsp-probe-0) [go][i¦d]\n\n[id]: #target\n";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 0, character: 0 })));
}
