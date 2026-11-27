//! Dialect parity through the real framed server, including semantic edit proofs.

use super::{Json, TestResult, array, session};

const URI: &str = "file:///profiles/start.md";

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn text(value: &str) -> Json {
    Json::String(value.to_string())
}

fn number(value: usize) -> Json {
    Json::Number(value as f64)
}

fn field(mut value: Json, key: &str, item: Json) -> Json {
    let Json::Object(fields) = &mut value else {
        panic!("expected object");
    };
    fields.insert(key.to_owned(), item);
    value
}

fn request(id: usize, method: &str, params: Json) -> Json {
    object([
        ("jsonrpc", text("2.0")),
        ("id", number(id)),
        ("method", text(method)),
        ("params", params),
    ])
}

fn notification(method: &str, params: Json) -> Json {
    object([
        ("jsonrpc", text("2.0")),
        ("method", text(method)),
        ("params", params),
    ])
}

fn initialize(id: usize, options: Option<Json>) -> Json {
    let params = object([(
        "capabilities",
        object([
            (
                "workspace",
                object([
                    (
                        "workspaceEdit",
                        object([("documentChanges", Json::Bool(true))]),
                    ),
                    ("fileOperations", object([("willRename", Json::Bool(true))])),
                ]),
            ),
            (
                "textDocument",
                object([(
                    "documentSymbol",
                    object([("hierarchicalDocumentSymbolSupport", Json::Bool(true))]),
                )]),
            ),
        ]),
    )]);
    request(
        id,
        "initialize",
        options.map_or(params.clone(), |options| {
            field(params, "initializationOptions", options)
        }),
    )
}

fn profile(value: &str) -> Option<Json> {
    Some(object([("profile", text(value))]))
}

fn open(uri: &str, source: &str, version: usize) -> Json {
    notification(
        "textDocument/didOpen",
        object([(
            "textDocument",
            object([
                ("uri", text(uri)),
                ("languageId", text("markdown")),
                ("text", text(source)),
                ("version", number(version)),
            ]),
        )]),
    )
}

fn replace(uri: &str, source: &str, version: usize) -> Json {
    notification(
        "textDocument/didChange",
        object([
            (
                "textDocument",
                object([("uri", text(uri)), ("version", number(version))]),
            ),
            (
                "contentChanges",
                Json::Array(vec![object([("text", text(source))])]),
            ),
        ]),
    )
}

fn position(source: &str, offset: usize) -> Json {
    let before = &source[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    object([
        ("line", number(line)),
        (
            "character",
            number(source[start..offset].encode_utf16().count()),
        ),
    ])
}

fn document(uri: &str) -> Json {
    object([("textDocument", object([("uri", text(uri))]))])
}

fn at(uri: &str, source: &str, marker: &str, within: usize) -> Json {
    let offset = source.find(marker).expect("source marker") + within;
    field(document(uri), "position", position(source, offset))
}

fn run(mut messages: Vec<Json>) -> Result<Vec<Json>, Box<dyn std::error::Error>> {
    messages.push(request(90_000, "shutdown", Json::Null));
    messages.push(notification("exit", Json::Null));
    let encoded: Vec<_> = messages.iter().map(Json::to_json_string).collect();
    let (status, replies) = session(&encoded.iter().map(String::as_str).collect::<Vec<_>>())?;
    assert!(status.success(), "server status: {status}");
    Ok(replies)
}

fn reply(replies: &[Json], id: usize) -> &Json {
    replies
        .iter()
        .find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id as u64))
        .expect("response ID")
}

fn result(replies: &[Json], id: usize) -> &Json {
    let response = reply(replies, id);
    response
        .get("result")
        .unwrap_or_else(|| panic!("request {id}: {}", response.to_json_string()))
}

fn error_code(replies: &[Json], id: usize) -> i32 {
    reply(replies, id)
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Json::as_f64)
        .expect("protocol error") as i32
}

fn reports<'a>(replies: &'a [Json], uri: &str) -> Vec<&'a Json> {
    replies
        .iter()
        .filter(|reply| {
            reply.get("method").and_then(Json::as_str) == Some("textDocument/publishDiagnostics")
        })
        .filter_map(|reply| reply.get("params"))
        .filter(|params| params.get("uri").and_then(Json::as_str) == Some(uri))
        .collect()
}

fn diagnostics(report: &Json) -> &[Json] {
    array(report.get("diagnostics").expect("diagnostics"))
}

fn offset(source: &str, value: &Json) -> usize {
    let line = value.get("line").and_then(Json::as_u64).expect("line") as usize;
    let units = value
        .get("character")
        .and_then(Json::as_u64)
        .expect("character") as usize;
    let start: usize = source.split_inclusive('\n').take(line).map(str::len).sum();
    let mut consumed = 0;
    for (byte, ch) in source[start..].char_indices() {
        if consumed == units {
            return start + byte;
        }
        consumed += ch.len_utf16();
        assert!(consumed <= units, "edit split a surrogate pair");
    }
    assert_eq!(consumed, units);
    source.len()
}

fn apply(source: &str, edit: &Json, uri: &str, version: usize) -> String {
    let mut changes = Vec::new();
    for document in edit.get("documentChanges").map(array).unwrap_or_default() {
        let target = document.get("textDocument").expect("edit document");
        if target.get("uri").and_then(Json::as_str) != Some(uri) {
            continue;
        }
        assert_eq!(
            target.get("version").and_then(Json::as_u64),
            Some(version as u64)
        );
        for edit in array(document.get("edits").expect("edits")) {
            let range = edit.get("range").expect("range");
            changes.push((
                offset(source, range.get("start").unwrap()),
                offset(source, range.get("end").unwrap()),
                edit.get("newText").and_then(Json::as_str).unwrap(),
            ));
        }
    }
    changes.sort_by_key(|&(start, end, _)| (start, end));
    for pair in changes.windows(2) {
        assert!(pair[0].1 <= pair[1].0, "overlapping edits");
    }
    let mut amended = source.to_owned();
    for (start, end, replacement) in changes.into_iter().rev() {
        amended.replace_range(start..end, replacement);
    }
    amended
}

#[test]
fn invalid_profiles_do_not_initialize_or_poison_a_later_valid_session() -> TestResult {
    let invalid = [
        Json::Bool(true),
        Json::Array(vec![]),
        object([("profile", Json::Null)]),
        object([("profile", number(1))]),
        object([("profile", Json::Bool(true))]),
        object([("profile", object([]))]),
        object([("profile", text("gfm"))]),
        object([("profile", text("CommonMark-GFM"))]),
        object([("profile", text(" commonmark-gfm "))]),
        object([("profile", text("unknown"))]),
    ];
    let mut messages: Vec<_> = invalid
        .into_iter()
        .enumerate()
        .map(|(i, options)| initialize(i + 1, Some(options)))
        .collect();
    messages.push(initialize(100, profile("commonmark-gfm")));
    messages.push(initialize(101, profile("gfm-plus")));
    messages.push(open(URI, "[bad]:\n: Definition.\n", 7));
    let replies = run(messages)?;
    for id in 1..=10 {
        assert_eq!(error_code(&replies, id), -32602);
    }
    assert!(result(&replies, 100).get("capabilities").is_some());
    assert_eq!(error_code(&replies, 101), -32600);
    let reports = reports(&replies, URI);
    assert_eq!(
        diagnostics(reports[0])[0]
            .get("code")
            .and_then(Json::as_str),
        Some("malformed_reference")
    );
    Ok(())
}

#[test]
fn selected_grammar_owns_parser_link_diagnostics_and_original_crlf_ranges() -> TestResult {
    let source = "[bad]:\r\n: Definition.\r\n\r\n😀 [Term\r\n: definition](#missing)\r\n";
    for (options, commonmark) in [
        (None, false),
        (Some(Json::Null), false),
        (Some(object([("clientFeature", text("ignored"))])), false),
        (profile("gfm-plus"), false),
        (profile("commonmark-gfm"), true),
    ] {
        let replies = run(vec![initialize(1, options), open(URI, source, 7)])?;
        let report = reports(&replies, URI)[0];
        assert_eq!(report.get("version").and_then(Json::as_u64), Some(7));
        let findings = diagnostics(report);
        if !commonmark {
            assert!(findings.is_empty(), "{findings:?}");
            continue;
        }
        assert_eq!(findings.len(), 2);
        for (code, start, end) in [
            ("malformed_reference", 0, 6),
            (
                "missing_anchor",
                source.find('😀').unwrap(),
                source.trim_end().len(),
            ),
        ] {
            let finding = findings
                .iter()
                .find(|finding| finding.get("code").and_then(Json::as_str) == Some(code))
                .expect("dialect finding");
            assert_eq!(
                finding.get("range"),
                Some(&object([
                    ("start", position(source, start)),
                    ("end", position(source, end))
                ]))
            );
        }
    }
    Ok(())
}

#[test]
fn inline_navigation_completions_references_and_heading_edits_share_the_profile() -> TestResult {
    let source = "😀 [Term\r\n: definition](#target)\r\n\r\n# Target\r\n";
    for (options, commonmark) in [
        (None, false),
        (profile("gfm-plus"), false),
        (profile("commonmark-gfm"), true),
    ] {
        let link = at(URI, source, "#target", 3);
        let heading = at(URI, source, "# Target", 3);
        let references = field(
            heading.clone(),
            "context",
            object([("includeDeclaration", Json::Bool(true))]),
        );
        let replies = run(vec![
            initialize(1, options),
            open(URI, source, 7),
            request(2, "textDocument/definition", link.clone()),
            request(3, "textDocument/completion", link.clone()),
            request(4, "textDocument/references", references),
            request(5, "textDocument/documentHighlight", heading.clone()),
            request(6, "textDocument/prepareRename", heading.clone()),
            request(
                7,
                "textDocument/rename",
                field(heading, "newName", text("Renamed")),
            ),
            request(8, "textDocument/definition", link.clone()),
            request(9, "textDocument/prepareRename", link.clone()),
            request(
                10,
                "textDocument/rename",
                field(link, "newName", text("Renamed")),
            ),
        ])?;
        if commonmark {
            assert_eq!(
                result(&replies, 9).get("placeholder").and_then(Json::as_str),
                Some("Target")
            );
            assert_eq!(result(&replies, 10), result(&replies, 7));
            assert_eq!(
                result(&replies, 2).get("uri").and_then(Json::as_str),
                Some(URI)
            );
            assert_eq!(
                array(result(&replies, 3).get("items").unwrap())[0]
                    .get("label")
                    .and_then(Json::as_str),
                Some("#target")
            );
        } else {
            assert_eq!(result(&replies, 2), &Json::Null);
            assert!(array(result(&replies, 3).get("items").unwrap()).is_empty());
            assert_eq!(result(&replies, 9), &Json::Null);
            assert_eq!(result(&replies, 10), &Json::Null);
        }
        assert_eq!(
            array(result(&replies, 4)).len(),
            if commonmark { 2 } else { 1 }
        );
        assert_eq!(
            array(result(&replies, 5)).len(),
            if commonmark { 2 } else { 1 }
        );
        assert_eq!(
            result(&replies, 6)
                .get("placeholder")
                .and_then(Json::as_str),
            Some("Target")
        );
        let expected = source.replace("# Target", "# Renamed");
        let expected = if commonmark {
            expected.replace("#target", "#renamed")
        } else {
            expected
        };
        assert_eq!(apply(source, result(&replies, 7), URI, 7), expected);
        assert_eq!(
            result(&replies, 2),
            result(&replies, 8),
            "edit proposals must leave client-owned text unchanged"
        );
    }
    Ok(())
}

#[test]
fn reference_label_provenance_and_probes_keep_multiline_commonmark_links_intact() -> TestResult {
    let source = "[É😀\r\n: definition][ref]\r\n\r\n[ref]: #target\r\n\r\n# Target\r\n";
    let selected = at(URI, source, "][ref]", 4);
    let replies = run(vec![
        initialize(1, profile("commonmark-gfm")),
        open(URI, source, 7),
        request(2, "textDocument/definition", selected.clone()),
        request(3, "textDocument/completion", selected.clone()),
        request(4, "textDocument/prepareRename", selected.clone()),
        request(
            5,
            "textDocument/rename",
            field(selected, "newName", text("renamed")),
        ),
    ])?;
    assert_eq!(
        result(&replies, 2).get("uri").and_then(Json::as_str),
        Some(URI)
    );
    assert_eq!(
        array(result(&replies, 3).get("items").unwrap())[0]
            .get("label")
            .and_then(Json::as_str),
        Some("ref")
    );
    assert_eq!(
        result(&replies, 4)
            .get("placeholder")
            .and_then(Json::as_str),
        Some("ref")
    );
    assert_eq!(
        apply(source, result(&replies, 5), URI, 7),
        source.replace("[ref]", "[renamed]")
    );
    Ok(())
}

#[test]
fn footnote_symbols_and_edits_exclude_markers_in_dialect_owned_code_spans() -> TestResult {
    let source = "😀 `[^note]\r\n: literal`\r\n\r\n[^note]:\r\n    # Hidden\r\n";
    for (options, commonmark) in [
        (profile("gfm-plus"), false),
        (profile("commonmark-gfm"), true),
    ] {
        let literal = at(URI, source, "[^note]", 4);
        let declaration = at(URI, source, "[^note]:", 4);
        let replies = run(vec![
            initialize(1, options),
            open(URI, source, 7),
            request(2, "textDocument/definition", literal.clone()),
            request(3, "textDocument/completion", literal),
            request(4, "workspace/symbol", object([("query", text("hidden"))])),
            request(
                5,
                "textDocument/references",
                field(
                    declaration.clone(),
                    "context",
                    object([("includeDeclaration", Json::Bool(true))]),
                ),
            ),
            request(
                6,
                "textDocument/rename",
                field(declaration, "newName", text("renamed")),
            ),
        ])?;
        assert_eq!(result(&replies, 2) == &Json::Null, commonmark);
        assert_eq!(
            array(result(&replies, 3).get("items").unwrap()).is_empty(),
            commonmark
        );
        assert_eq!(array(result(&replies, 4)).len(), usize::from(!commonmark));
        assert_eq!(
            array(result(&replies, 5)).len(),
            if commonmark { 1 } else { 2 }
        );
        let expected = if commonmark {
            source.replace("[^note]:", "[^renamed]:")
        } else {
            source.replace("[^note]", "[^renamed]")
        };
        assert_eq!(apply(source, result(&replies, 6), URI, 7), expected);
    }
    Ok(())
}

#[test]
fn cross_buffer_diagnostics_and_file_moves_use_current_profiled_destinations() -> TestResult {
    let chapter = "file:///profiles/chapter.md";
    let source =
        "[Term\r\n: definition](chapter.md#missing)\r\n\r\n![Alt\r\n: image](asset.png)\r\n";
    let moves = object([(
        "files",
        Json::Array(vec![
            object([
                ("oldUri", text(chapter)),
                ("newUri", text("file:///profiles/manual/renamed.md")),
            ]),
            object([
                ("oldUri", text("file:///profiles/asset.png")),
                ("newUri", text("file:///profiles/images/new.png")),
            ]),
        ]),
    )]);
    for (options, commonmark) in [
        (profile("gfm-plus"), false),
        (profile("commonmark-gfm"), true),
    ] {
        let replies = run(vec![
            initialize(1, options),
            open(URI, source, 7),
            open(chapter, "# Present\n", 4),
            replace(chapter, "# Missing\n", 5),
            request(2, "textDocument/definition", at(URI, source, "#missing", 3)),
            request(3, "workspace/willRenameFiles", moves.clone()),
        ])?;
        let updates = reports(&replies, URI);
        if commonmark {
            assert_eq!(updates.len(), 3);
            assert_eq!(
                diagnostics(updates[1])[0]
                    .get("code")
                    .and_then(Json::as_str),
                Some("workspace_missing_anchor")
            );
            assert!(diagnostics(updates[2]).is_empty());
            assert!(
                updates
                    .iter()
                    .all(|report| report.get("version").and_then(Json::as_u64) == Some(7))
            );
            assert_eq!(
                result(&replies, 2).get("uri").and_then(Json::as_str),
                Some(chapter)
            );
        } else {
            assert_eq!(updates.len(), 1);
            assert!(diagnostics(updates[0]).is_empty());
            assert_eq!(result(&replies, 2), &Json::Null);
        }
        let expected = if commonmark {
            source
                .replace("chapter.md", "manual/renamed.md")
                .replace("asset.png", "images/new.png")
        } else {
            source.to_owned()
        };
        assert_eq!(apply(source, result(&replies, 3), URI, 7), expected);
    }
    Ok(())
}

#[test]
fn folds_and_selection_ranges_follow_profiled_semantic_block_ownership() -> TestResult {
    let source = "First\n: one\n\nSecond\n: two\n\n# Last\n";
    for (options, commonmark) in [
        (profile("gfm-plus"), false),
        (profile("commonmark-gfm"), true),
    ] {
        let selection = field(
            document(URI),
            "positions",
            Json::Array(vec![position(source, source.find("Second").unwrap() + 2)]),
        );
        let replies = run(vec![
            initialize(1, options),
            open(URI, source, 7),
            request(2, "textDocument/foldingRange", document(URI)),
            request(3, "textDocument/selectionRange", selection),
            request(4, "textDocument/documentSymbol", document(URI)),
        ])?;
        assert_eq!(array(result(&replies, 2)).len(), usize::from(!commonmark));
        let owner = array(result(&replies, 3))[0]
            .get("parent")
            .and_then(|parent| parent.get("range"))
            .expect("semantic block range");
        assert_eq!(
            owner
                .get("start")
                .and_then(|position| position.get("line"))
                .and_then(Json::as_u64),
            Some(if commonmark { 3 } else { 0 })
        );
        assert_eq!(
            array(result(&replies, 4))[0]
                .get("name")
                .and_then(Json::as_str),
            Some("Last")
        );
    }
    Ok(())
}
