#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use super::super::{Phase, Server};
use franken_markdown::{Block, Inline};

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

fn items(result: &Json) -> &[Json] {
    match result.get("items") {
        Some(Json::Array(items)) => items,
        _ => &[],
    }
}

fn start(result: &Json) -> Option<&Json> {
    result.get("range").and_then(|range| range.get("start"))
}

#[test]
fn definitions_and_completions_cover_all_parsed_inline_containers() {
    for body in [
        "> [go](#tar¦get)",
        "- [go](#tar¦get)\n- sibling",
        "1. [x] **[go](#tar¦get)**\n2. sibling",
        "> - *[go](#tar¦get)*",
        "| [go](#tar¦get) |\n| --- |\n| body |",
        "| head |\n| --- |\n| [go](#tar¦get) |",
        "[go](#tar¦get)\n: definition",
        "Term\n: ~~[go](#tar¦get)~~",
        "Use[^n].\n\n[^n]: [go](#tar¦get)",
    ] {
        let source = format!("# Target\n\n{body}\n");
        let definition = call("textDocument/definition", &source);
        assert_eq!(definition.get("uri").and_then(Json::as_str), Some(URI), "{body}");
        assert_eq!(start(&definition), Some(&position(Position { line: 0, character: 0 })), "{body}");
        let completion = call("textDocument/completion", &source);
        assert_eq!(items(&completion).len(), 1, "{body}");
        assert_eq!(items(&completion)[0].get("label").and_then(Json::as_str), Some("#target"));
    }
}

#[test]
fn nested_edits_preserve_utf16_coordinates_container_markers_and_siblings() {
    let (source, offset) = marked(
        "# Target\r\n\r\n> - 😀 [go](?mode=read#tar¦get-old \"Keep\")\r\n> - sibling [go](#target)\r\n",
    );
    let result = request("textDocument/completion", &params(&source, offset), URI, &source).unwrap();
    let edit = items(&result)[0].get("textEdit").unwrap();
    let range = edit.get("range").unwrap();
    let index = LineIndex::new(&source);
    let begin = index.offset(&source, Position::parse(range.get("start").unwrap()).unwrap()).unwrap();
    let end = index.offset(&source, Position::parse(range.get("end").unwrap()).unwrap()).unwrap();
    assert_eq!(&source[begin..end], "#target-old");
    assert_eq!(index.position(&source, begin).character, "> - 😀 [go](?mode=read".encode_utf16().count());
    let mut edited = source.clone();
    edited.replace_range(begin..end, edit.get("newText").unwrap().as_str().unwrap());
    assert_eq!(edited, source.replacen("#target-old", "#target", 1));
}

#[test]
fn repeated_nested_links_resolve_the_selected_occurrence_not_the_first() {
    let source = "# One\n\n# Two\n\n- [same](#one)\n- [same](#tw¦o)\n";
    assert_eq!(start(&call("textDocument/definition", source)), Some(&position(Position { line: 2, character: 0 })));
}

#[test]
fn unreferenced_notes_are_inert_and_transitively_referenced_notes_are_visible() {
    let source = "# Target\n\n[^n]: [go](#tar¦get)\n";
    assert_eq!(call("textDocument/definition", source), Json::Null);
    assert!(items(&call("textDocument/completion", source)).is_empty());
    let visible = "# Target\n\nUse[^a].\n\n[^a]: See[^b].\n\n[^b]: [go](#tar¦get)\n";
    assert_eq!(start(&call("textDocument/definition", visible)), Some(&position(Position { line: 0, character: 0 })));
}

#[test]
fn nested_literal_code_math_images_and_titles_do_not_gain_authority() {
    for body in [
        "> `[go](#tar¦get)`",
        "- ![go](#tar¦get)",
        "- <span title=\"[go](#tar¦get)\">x</span>",
        "> ```md\n> [go](#tar¦get)\n> ```",
        "| code |\n| --- |\n| `[go](#tar¦get)` |",
        "Term\n: $[go](#tar¦get)$",
        "Use[^n].\n\n[^n]: `[go](#tar¦get)`",
        "- [go](#target \"title ](#tar¦get)\")",
    ] {
        let source = format!("# Target\n\n{body}\n");
        assert_eq!(call("textDocument/definition", &source), Json::Null, "{body}");
        assert!(items(&call("textDocument/completion", &source)).is_empty(), "{body}");
    }
}

#[test]
fn unfinished_destinations_at_the_container_end_can_be_completed_not_navigated() {
    for body in ["> [go](#tar¦", "- [go](<#tar¦", "> - [go](#tar¦"] {
        let source = format!("# Target\n\n{body}");
        assert_eq!(items(&call("textDocument/completion", &source)).len(), 1, "{body}");
        assert_eq!(call("textDocument/definition", &source), Json::Null);
    }
}

#[test]
fn encoded_probe_names_anywhere_in_a_container_cannot_forge_a_selected_link() {
    let source = "# Target\n\n- [real](#fmd&#45;lsp-probe-0)\n- `[fake](#tar¦get)`\n";
    assert_eq!(call("textDocument/definition", source), Json::Null);
    assert!(items(&call("textDocument/completion", source)).is_empty());
    let valid = "# Target\n\n- [real](#fmd&#45;lsp-probe-0)\n- [go](#tar¦get)\n";
    assert_eq!(start(&call("textDocument/definition", valid)), Some(&position(Position { line: 0, character: 0 })));
}

#[test]
fn structural_restoration_does_not_hide_sibling_or_container_metadata_changes() {
    let original = parse_markdown("- [go](#target)\n- sibling\n");
    let mut probed = parse_markdown("- [go](#probe)\n- sibling\n");
    assert_eq!(structure::count(&probed.blocks, "#probe"), 1);
    let mut destination = None;
    structure::restore(&mut probed.blocks[0], &original.blocks[0], "#probe", &mut destination);
    assert_eq!(destination.as_deref(), Some("#target"));
    assert_eq!(probed, original);
    let mut changed = parse_markdown("- [go](#probe)\n- changed\n");
    structure::restore(&mut changed.blocks[0], &original.blocks[0], "#probe", &mut destination);
    assert_ne!(changed, original);
    let mut changed = parse_markdown("1. [go](#probe)\n2. sibling\n");
    structure::restore(&mut changed.blocks[0], &original.blocks[0], "#probe", &mut destination);
    assert_ne!(changed, original);
    assert_eq!(structure::count(&[Block::Paragraph(vec![Inline::Code("#probe".into())])], "#probe"), 0);
}

#[test]
fn nested_cross_file_navigation_uses_only_synchronized_target_buffers() {
    let (source, offset) = marked("- [go](target.md#tar¦get)\n");
    let mut server = Server { phase: Phase::Running, ..Server::default() };
    for (uri, text) in [(URI, source.as_str()), ("file:///docs/target.md", "# Intro\n\n# Target\n")] {
        server.documents.insert(uri.into(), Buffer { text: text.into(), version: 1, synchronized: true });
    }
    let params = params(&source, offset);
    let result = server.navigate("textDocument/definition", &params).unwrap();
    assert_eq!(result.get("uri").and_then(Json::as_str), Some("file:///docs/target.md"));
    assert_eq!(start(&result), Some(&position(Position { line: 2, character: 0 })));
    server.documents.get_mut("file:///docs/target.md").unwrap().synchronized = false;
    assert!(matches!(server.navigate("textDocument/definition", &params), Err((-32801, _))));
    assert!(matches!(server.navigate("textDocument/completion", &params), Err((-32801, _))));
}

#[test]
fn hidden_note_links_do_not_consult_even_an_unsynchronized_remote_target() {
    let (source, offset) = marked("[^n]: [go](target.md#tar¦get)\n");
    // A resolver that would fail makes any accidental target lookup observable.
    let result = request_resolving("textDocument/definition", &params(&source, offset), URI, &source, |_| {
        Err((-32801, "must not resolve a hidden source link"))
    }).unwrap();
    assert_eq!(result, Json::Null);
    // Keep a separate completion check: completion must apply the same visibility gate.
    let result = request_resolving("textDocument/completion", &params(&source, offset), URI, &source, |_| {
        Err((-32801, "must not resolve a hidden source link"))
    }).unwrap();
    assert!(items(&result).is_empty());
}
