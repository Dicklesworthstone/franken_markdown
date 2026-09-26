use super::*;
use super::super::{Phase, Server, notification};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn params(source: &str, offset: usize) -> Json {
    object([
        ("textDocument", object([("uri", string("untitled:links"))])),
        ("position", position(LineIndex::new(source).position(source, offset))),
    ])
}
fn marked(source: &str) -> Result<(String, usize), &'static str> {
    let offset = source.find('¦').ok_or("test requires caret marker")?;
    Ok((source.replace('¦', ""), offset))
}
fn call(method: &str, marked_source: &str) -> Result<Json, Box<dyn std::error::Error>> {
    let (source, offset) = marked(marked_source)?;
    Ok(request(method, &params(&source, offset), "untitled:links", &source).map_err(|(_, reason)| reason)?)
}
fn items(result: &Json) -> &[Json] {
    match result.get("items") { Some(Json::Array(items)) => items, _ => &[] }
}
fn labels(result: &Json) -> Vec<&str> {
    items(result).iter().filter_map(|item| item.get("label").and_then(Json::as_str)).collect()
}
fn end_line(result: &Json) -> Option<u64> {
    result.get("range").and_then(|r| r.get("end")).and_then(|p| p.get("line")).and_then(Json::as_u64)
}

#[test]
fn completion_uses_canonical_ids_and_supports_an_unclosed_destination() -> TestResult {
    let source = "# Same\n\n# Same\n\n# Same 2\n\n[go](#sa¦";
    let result = call("textDocument/completion", source)?;
    assert_eq!(labels(&result), ["#same", "#same-2", "#same-2-2"]);
    assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(false)));
    assert_eq!(call("textDocument/definition", source)?, Json::Null);
    Ok(())
}

#[test]
fn definition_resolves_encoded_forward_heading_without_changing_the_source() -> TestResult {
    let source = "[go](#%74ar¦get)\n\n# Target\n";
    let result = call("textDocument/definition", source)?;
    assert_eq!(result.get("uri").and_then(Json::as_str), Some("untitled:links"));
    assert_eq!(result.get("range").and_then(|r| r.get("start")).and_then(|p| p.get("line")).and_then(Json::as_u64), Some(2));
    assert!(end_line(&result).is_some());
    assert_eq!(result, call("textDocument/definition", source)?);
    Ok(())
}

#[test]
fn edit_replaces_only_fragment_preserving_query_title_and_surrogate_coordinates() -> TestResult {
    let marked_source = "# Same\r\n\r\n😀 [go](?mode=read#sa¦me-old \"Keep title\") tail\r\n";
    let (source, offset) = marked(marked_source)?;
    let result = request("textDocument/completion", &params(&source, offset), "untitled:links", &source)
        .map_err(|(_, reason)| reason)?;
    let edit = items(&result).first().and_then(|item| item.get("textEdit")).ok_or("missing edit")?;
    let range = edit.get("range").ok_or("missing range")?;
    let index = LineIndex::new(&source);
    let start = index.offset(&source, Position::parse(range.get("start").ok_or("missing start")?)?)?;
    let end = index.offset(&source, Position::parse(range.get("end").ok_or("missing end")?)?)?;
    assert_eq!(&source[start..end], "#same-old");
    assert_eq!(index.position(&source, start).character, source[source.rfind("😀").ok_or("no emoji")?..start].encode_utf16().count());
    let mut replaced = source.clone();
    replaced.replace_range(start..end, edit.get("newText").and_then(Json::as_str).ok_or("missing text")?);
    assert!(replaced.contains("😀 [go](?mode=read#same \"Keep title\") tail"));
    Ok(())
}

#[test]
fn angle_destinations_and_percent_encoded_completion_prefixes_are_supported() -> TestResult {
    let result = call("textDocument/completion", "# Same\n\n[go](<#%73a¦>)")?;
    assert_eq!(labels(&result), ["#same"]);
    assert_eq!(items(&result)[0].get("filterText").and_then(Json::as_str), Some("#%73ame"));
    let result = call("textDocument/completion", "# Same\n\n[go](<#sa¦")?;
    assert_eq!(labels(&result), ["#same"]);
    Ok(())
}

#[test]
fn code_images_html_titles_files_and_bare_hashes_cannot_become_link_candidates() -> TestResult {
    for body in [
        "`[go](#tar¦get)`", "`` [go](#tar¦get) ``", "![go](#tar¦get)",
        "```md\n[go](#tar¦get)\n```", "<span title=\"[go](#tar¦get)\">x</span>",
        "[go](https://example.invalid/#tar¦get)", "[go](other.md#tar¦get)",
        "[go][tar¦get]", "bare #tar¦get", "[go](#wrong \"title ](#tar¦get)\")",
    ] {
        let source = format!("# Target\n\n{body}");
        assert!(items(&call("textDocument/completion", &source)?).is_empty(), "{body}");
        assert_eq!(call("textDocument/definition", &source)?, Json::Null, "{body}");
    }
    Ok(())
}

#[test]
fn nested_targets_keep_their_real_enclosing_source_block() -> TestResult {
    let result = call("textDocument/definition", "> # Nested\n> text\n\n[go](#nes¦ted)")?;
    assert_eq!(result.get("range").and_then(|r| r.get("start")), Some(&position(Position { line: 0, character: 0 })));
    assert!(end_line(&result).is_some_and(|line| line >= 1));
    // Origin locations inside containers are deliberately not guessed.
    assert_eq!(call("textDocument/definition", "# Target\n\n> [go](#tar¦get)")?, Json::Null);
    Ok(())
}

#[test]
fn missing_invalid_and_ambiguous_anchors_never_jump_to_an_arbitrary_target() -> TestResult {
    for destination in ["#mis¦sing", "#%z¦z", "#fn-¦a"] {
        let source = format!("# fn-a\n\nUse[^a]. [go]({destination})\n\n[^a]: note\n");
        assert_eq!(call("textDocument/definition", &source)?, Json::Null);
    }
    let result = call("textDocument/completion", "# fn-a\n\nUse[^a]. [go](#fn-¦)\n\n[^a]: note\n")?;
    assert!(items(&result).is_empty());
    Ok(())
}

#[test]
fn generated_note_target_and_empty_fragment_have_distinct_locations() -> TestResult {
    let result = call("textDocument/definition", "Use[^a]. [go](#fn-¦a)\n\n[^a]: note\n")?;
    assert_eq!(result.get("range").and_then(|r| r.get("start")).and_then(|p| p.get("line")).and_then(Json::as_u64), Some(2));
    let result = call("textDocument/definition", "# Top\n\n[go](#¦)")?;
    let zero = position(Position { line: 0, character: 0 });
    assert_eq!(result.get("range").and_then(|r| r.get("start")), Some(&zero));
    assert_eq!(result.get("range").and_then(|r| r.get("end")), Some(&zero));
    Ok(())
}

#[test]
fn probe_cannot_confuse_a_code_token_with_an_existing_encoded_destination() -> TestResult {
    let source = "# Target\n\n[existing](#fmd&#45;lsp-probe-0) `[code](#tar¦get)`";
    assert_eq!(call("textDocument/definition", source)?, Json::Null);
    assert!(items(&call("textDocument/completion", source)?).is_empty());
    Ok(())
}

#[test]
fn incomplete_percent_escape_retries_without_inventing_a_completion() -> TestResult {
    let result = call("textDocument/completion", "# Target\n\n[go](#%7¦)")?;
    assert!(items(&result).is_empty());
    assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(true)));
    Ok(())
}

#[test]
fn completions_are_bounded_and_report_incompleteness() -> TestResult {
    let mut source: String = (0..MAX_COMPLETIONS + 20).map(|i| format!("# Heading {i}\n\n")).collect();
    source.push_str("[go](#hea¦)");
    let result = call("textDocument/completion", &source)?;
    assert_eq!(items(&result).len(), MAX_COMPLETIONS);
    assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(true)));
    assert!(result.to_json_string().len() < super::super::MAX_FRAME_BYTES);
    Ok(())
}

#[test]
fn invalid_positions_and_large_blocks_are_rejected_explicitly() -> TestResult {
    let source = "😀 [go](#target)";
    let bad = object([("position", position(Position { line: 0, character: 1 }))]);
    assert!(matches!(request("textDocument/completion", &bad, "untitled:links", source), Err((-32602, _))));
    let source = format!("{} [go](#target)", "x".repeat(MAX_BLOCK_BYTES));
    let offset = source.len() - 1;
    assert!(matches!(request("textDocument/completion", &params(&source, offset), "untitled:links", &source), Err((-32803, _))));
    Ok(())
}

#[test]
fn server_dispatch_negotiates_link_features_and_rejects_unsynchronized_navigation() -> TestResult {
    let mut server = Server::default();
    let replies = server.handle(object([
        ("jsonrpc", string("2.0")), ("id", number(1)), ("method", string("initialize")), ("params", object([])),
    ]));
    let capabilities = replies[0].get("result").and_then(|r| r.get("capabilities")).ok_or("no capabilities")?;
    assert_eq!(capabilities.get("definitionProvider"), Some(&Json::Bool(true)));
    assert!(capabilities.get("completionProvider").is_some());
    let (source, offset) = marked("# Target\n\n[go](#tar¦get)")?;
    server.handle(notification("textDocument/didOpen", object([("textDocument", object([
        ("uri", string("untitled:links")), ("version", number(1)), ("text", string(&source)),
    ]))])));
    assert!(server.phase == Phase::Running);
    let replies = server.handle(object([
        ("jsonrpc", string("2.0")), ("id", string("definition")), ("method", string("textDocument/definition")),
        ("params", params(&source, offset)),
    ]));
    assert_eq!(replies[0].get("id").and_then(Json::as_str), Some("definition"));
    assert_eq!(replies[0].get("result").and_then(|r| r.get("uri")).and_then(Json::as_str), Some("untitled:links"));
    server.documents.get_mut("untitled:links").ok_or("missing buffer")?.synchronized = false;
    assert!(matches!(server.navigate("textDocument/definition", &params(&source, offset)), Err((-32801, _))));
    Ok(())
}

#[test]
fn current_buffer_revision_controls_navigation_after_heading_rename() -> TestResult {
    let mut server = Server { phase: Phase::Running, ..Server::default() };
    let (source, offset) = marked("# Target\n\n[go](#tar¦get)")?;
    server.open(&object([("textDocument", object([
        ("uri", string("untitled:links")), ("version", number(1)), ("text", string(&source)),
    ]))]))?;
    assert_ne!(server.navigate("textDocument/definition", &params(&source, offset)).map_err(|(_, reason)| reason)?, Json::Null);
    let changed = source.replacen("Target", "Renamed", 1);
    server.change(&object([
        ("textDocument", object([("uri", string("untitled:links")), ("version", number(2))])),
        ("contentChanges", Json::Array(vec![object([("text", string(&changed))])])),
    ]))?;
    let changed_offset = changed.find("#target").ok_or("no fragment")? + 4;
    assert_eq!(server.navigate("textDocument/definition", &params(&changed, changed_offset)).map_err(|(_, reason)| reason)?, Json::Null);
    Ok(())
}
