use super::super::{Phase, Server, notification};
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const URI: &str = "file:///docs/article.md";

fn buffer(source: &str) -> Buffer {
    Buffer {
        text: source.to_string(),
        version: 7,
        synchronized: true,
    }
}

fn params(source: &str, offset: usize, name: &str, declarations: bool) -> Json {
    object([
        ("textDocument", object([("uri", string(URI))])),
        (
            "position",
            position(LineIndex::new(source).position(source, offset)),
        ),
        (
            "context",
            object([("includeDeclaration", Json::Bool(declarations))]),
        ),
        ("newName", string(name)),
    ])
}

fn call(
    method: &str,
    marked: &str,
    name: &str,
    declarations: bool,
) -> Result<Json, Box<dyn std::error::Error>> {
    let offset = marked.find('¦').ok_or("fixture requires a caret")?;
    let source = marked.replace('¦', "");
    let documents = BTreeMap::from([(URI.to_string(), buffer(&source))]);
    Ok(request(
        method,
        &params(&source, offset, name, declarations),
        URI,
        documents.get(URI).ok_or("missing source")?,
        &documents,
    )
    .map_err(|(_, message)| message)?
    .unwrap_or(Json::Null))
}

fn array(value: &Json) -> &[Json] {
    if let Json::Array(values) = value {
        values
    } else {
        &[]
    }
}

fn span(source: &str, value: &Json) -> Result<SourceSpan, &'static str> {
    let range = value.get("range").ok_or("missing range")?;
    let index = LineIndex::new(source);
    Ok(SourceSpan::new(
        index.offset(
            source,
            Position::parse(range.get("start").ok_or("missing start")?)?,
        )?,
        index.offset(
            source,
            Position::parse(range.get("end").ok_or("missing end")?)?,
        )?,
    ))
}

fn apply(source: &str, result: &Json) -> Result<String, &'static str> {
    let changes = array(
        result
            .get("documentChanges")
            .ok_or("missing documentChanges")?,
    );
    if changes.len() != 1 {
        return Err("rename must affect one document");
    }
    let edits = array(changes[0].get("edits").ok_or("missing edits")?);
    let mut output = source.to_string();
    for edit in edits.iter().rev() {
        let range = span(source, edit)?;
        output.replace_range(
            range.start..range.end,
            edit.get("newText")
                .and_then(Json::as_str)
                .ok_or("missing newText")?,
        );
    }
    Ok(output)
}

#[test]
fn definition_returns_the_exact_forward_label_with_utf16_and_crlf() -> TestResult {
    let marked = "😀 reference [^猫¦😀]\r\n\r\n[^猫😀]: note text\r\n";
    let source = marked.replace('¦', "");
    let result = call("textDocument/definition", marked, "unused", false)?;
    let target = span(&source, &result)?;
    let expected = source.rfind("猫😀").ok_or("missing definition")?;
    assert_eq!(target, SourceSpan::new(expected, expected + "猫😀".len()));
    assert_eq!(result.get("uri").and_then(Json::as_str), Some(URI));
    let range = result.get("range").ok_or("no range")?;
    assert_eq!(
        range
            .get("start")
            .and_then(|p| p.get("character"))
            .and_then(Json::as_u64),
        Some(2)
    );
    assert_eq!(
        range
            .get("end")
            .and_then(|p| p.get("character"))
            .and_then(Json::as_u64),
        Some(5)
    );
    Ok(())
}

#[test]
fn references_keep_each_occurrence_and_exclude_other_case_and_unicode_spellings() -> TestResult {
    let marked = "[^No¦te] [^Note] [^note] [^é] [^e\u{301}]\n\n[^Note]: first\n\n[^note]: lower\n\n[^é]: precomposed\n\n[^e\u{301}]: combining\n";
    let source = marked.replace('¦', "");
    let references = call("textDocument/references", marked, "unused", false)?;
    assert_eq!(array(&references).len(), 2);
    for item in array(&references) {
        assert_eq!(span(&source, item)?.slice(&source), Some("Note"));
    }
    let including = call("textDocument/references", marked, "unused", true)?;
    assert_eq!(array(&including).len(), 3);
    let highlights = call("textDocument/documentHighlight", marked, "unused", false)?;
    let kinds: Vec<_> = array(&highlights)
        .iter()
        .map(|item| item.get("kind").and_then(Json::as_u64))
        .collect();
    assert_eq!(kinds, [Some(2), Some(2), Some(3)]);
    assert!(
        span(&source, &array(&including)[0])?.end < span(&source, &array(&including)[1])?.start
    );
    Ok(())
}

#[test]
fn nested_containers_notes_and_dormant_bodies_have_precise_symbols() -> TestResult {
    let source = "Root [^n].\n\n> Quote [^n].\n\n- List [^n].\n\n| Head | Other |\n| --- | --- |\n| [^n] | x |\n\nTerm\n: Definition [^n].\n\n[^n]: body\n\n    continuation [^n]\n\n[^unused]: dormant [^n]\n";
    let inventory =
        Inventory::parse(source, candidates(source).map_err(|(_, e)| e)?).map_err(|(_, e)| e)?;
    let notes: Vec<_> = inventory
        .notes
        .iter()
        .filter(|note| note.candidate.id == "n")
        .collect();
    assert_eq!(notes.len(), 8);
    assert_eq!(
        notes
            .iter()
            .filter(|note| note.kind == Kind::Definition)
            .count(),
        1
    );
    for note in notes {
        assert_eq!(note.candidate.label.slice(source), Some("n"));
    }
    Ok(())
}

#[test]
fn code_escapes_images_link_destinations_titles_html_and_math_never_become_symbols() -> TestResult {
    for literal in [
        "`[^n¦]`",
        "`` [^n¦] ``",
        "```md\n[^n¦]\n```",
        "    [^n¦]",
        "\\[^n¦]",
        "![alt [^n¦]](image.png)",
        "[link](https://example.test/[^n¦])",
        "[link](url \"[^n¦]\")",
        "<span title=\"[^n¦]\">text</span>",
        "$[^n¦]$",
        "$$\n[^n¦]\n$$",
    ] {
        let source = format!("Real [^n].\n\n{literal}\n\n[^n]: note\n");
        for method in [
            "textDocument/definition",
            "textDocument/references",
            "textDocument/prepareRename",
        ] {
            assert_eq!(
                call(method, &source, "changed", true)?,
                Json::Null,
                "{method}: {literal}"
            );
        }
        let marker = source
            .replace('¦', "")
            .replacen("Real [^n]", "Real [^n¦]", 1);
        let result = call("textDocument/references", &marker, "changed", true)?;
        assert_eq!(
            array(&result).len(),
            2,
            "literal poisoned the real inventory: {literal}"
        );
    }
    Ok(())
}

#[test]
fn duplicate_definitions_navigate_to_first_and_rename_the_complete_namespace() -> TestResult {
    let marked = "Use [^n¦].\n\n[^n]: first\n\n> [^n]: duplicate\n";
    let source = marked.replace('¦', "");
    let definition = call("textDocument/definition", marked, "unused", false)?;
    assert_eq!(
        span(&source, &definition)?.start,
        source.find("[^n]: first").ok_or("definition")? + 2
    );
    let references = call("textDocument/references", marked, "unused", true)?;
    assert_eq!(array(&references).len(), 3);
    let renamed = call("textDocument/rename", marked, "new", true)?;
    assert_eq!(apply(&source, &renamed)?, source.replace("[^n]", "[^new]"));
    Ok(())
}

#[test]
fn unresolved_references_can_be_found_and_renamed_without_inventing_a_definition() -> TestResult {
    let marked = "One [^miss¦ing], two [^missing].";
    let source = marked.replace('¦', "");
    assert_eq!(
        call("textDocument/definition", marked, "unused", false)?,
        Json::Null
    );
    assert_eq!(
        array(&call("textDocument/references", marked, "unused", false)?).len(),
        2
    );
    let prepared = call("textDocument/prepareRename", marked, "unused", true)?;
    assert_eq!(
        prepared.get("placeholder").and_then(Json::as_str),
        Some("missing")
    );
    let renamed = call("textDocument/rename", marked, "future", true)?;
    assert_eq!(apply(&source, &renamed)?, "One [^future], two [^future].");
    Ok(())
}

#[test]
fn raw_label_characters_follow_the_actual_parser_without_entity_or_case_normalization() -> TestResult
{
    for id in ["a\"b", "a<b", "a&amp;b", "a\\b", "é", "😀", "Case", "a`b"] {
        let marked = format!("Use [^{id}¦].\n\n[^{id}]: text\n");
        let source = marked.replace('¦', "");
        let definition = call("textDocument/definition", &marked, "unused", false)?;
        assert_eq!(span(&source, &definition)?.slice(&source), Some(id));
        let renamed = call("textDocument/rename", &marked, "new", true)?;
        assert_eq!(apply(&source, &renamed)?, "Use [^new].\n\n[^new]: text\n");
    }
    Ok(())
}

#[test]
fn rename_edits_are_local_versioned_and_leave_every_literal_byte_unchanged() -> TestResult {
    let marked = "😀 [^n¦] `[^n]` and [^n]\r\n\r\n[^n]: old body\r\n";
    let source = marked.replace('¦', "");
    let offset = marked.find('¦').ok_or("caret")?;
    let documents = BTreeMap::from([
        (URI.to_string(), buffer(&source)),
        (
            "file:///docs/other.md".to_string(),
            buffer("Other [^n].\n\n[^n]: independent\n"),
        ),
    ]);
    let result = request(
        "textDocument/rename",
        &params(&source, offset, "猫😀", true),
        URI,
        documents.get(URI).ok_or("source")?,
        &documents,
    )
    .map_err(|(_, e)| e)?
    .ok_or("missing rename")?;
    let changes = array(result.get("documentChanges").ok_or("changes")?);
    assert_eq!(changes.len(), 1);
    let document = changes[0].get("textDocument").ok_or("textDocument")?;
    assert_eq!(document.get("uri").and_then(Json::as_str), Some(URI));
    assert_eq!(document.get("version").and_then(Json::as_u64), Some(7));
    assert_eq!(
        apply(&source, &result)?,
        "😀 [^猫😀] `[^n]` and [^猫😀]\r\n\r\n[^猫😀]: old body\r\n"
    );
    assert_eq!(documents.get(URI).ok_or("source")?.text, source);
    assert_eq!(
        documents
            .get("file:///docs/other.md")
            .ok_or("other")?
            .version,
        7
    );
    Ok(())
}

#[test]
fn invalid_names_existing_names_and_context_changing_names_are_rejected() -> TestResult {
    let marked = "[^n¦] [^other]\n\n[^n]: note\n";
    for name in [
        "",
        "has space",
        "x\ny",
        "a[b",
        "a]b",
        "\u{7f}",
        &"x".repeat(129),
        "other",
    ] {
        assert!(
            call("textDocument/rename", marked, name, true).is_err(),
            "accepted {name:?}"
        );
    }
    let table = "| Header |\n| --- |\n| [^n¦] |\n\n[^n]: note\n";
    assert!(valid_new_name("a|b"));
    assert!(call("textDocument/rename", table, "a|b", true).is_err());
    let collision = "# fn-new\n\n[^n¦]\n\n[^n]: note\n";
    assert!(call("textDocument/rename", collision, "new", true).is_err());
    Ok(())
}

#[test]
fn ordinary_anchor_links_block_prepare_and_rename_but_not_navigation() -> TestResult {
    for link in [
        "[see](#fn-n)",
        "[see](?view=read#%66n-n)",
        "[see][note]\n\n[note]: #fn-n",
        "[^dormant]: [see](#fn-n)",
    ] {
        let marked = format!("[^n¦]\n\n[^n]: note\n\n{link}\n");
        assert_ne!(
            call("textDocument/definition", &marked, "new", true)?,
            Json::Null
        );
        for method in ["textDocument/prepareRename", "textDocument/rename"] {
            assert!(
                call(method, &marked, "new", true).is_err(),
                "{method}: {link}"
            );
        }
    }
    // A destination-looking string in code or a link title has no link authority.
    let safe = "[^n¦]\n\n[^n]: note\n\n`[see](#fn-n)` [safe](https://example.test \"#fn-n\")\n";
    assert!(call("textDocument/rename", safe, "new", true).is_ok());
    Ok(())
}

#[test]
fn cross_file_ordinary_anchor_links_and_stale_buffers_refuse_refactoring() -> TestResult {
    let source = "[^n]\n\n[^n]: note\n";
    let mut documents = BTreeMap::from([
        (URI.to_string(), buffer(source)),
        (
            "file:///docs/reader.md".to_string(),
            buffer("[see](article.md#fn-n)"),
        ),
    ]);
    for method in ["textDocument/prepareRename", "textDocument/rename"] {
        assert_eq!(
            request(
                method,
                &params(source, 2, "new", true),
                URI,
                documents.get(URI).ok_or("source")?,
                &documents
            )
            .err()
            .map(|(code, _)| code),
            Some(-32803)
        );
    }
    let other = documents
        .get_mut("file:///docs/reader.md")
        .ok_or("reader")?;
    other.text = "independent".to_string();
    other.synchronized = false;
    assert_eq!(
        request(
            "textDocument/rename",
            &params(source, 2, "new", true),
            URI,
            documents.get(URI).ok_or("source")?,
            &documents
        )
        .err()
        .map(|(code, _)| code),
        Some(-32801)
    );
    // Read-only note navigation has no dependency on unrelated buffers.
    assert!(
        request(
            "textDocument/references",
            &params(source, 2, "new", true),
            URI,
            documents.get(URI).ok_or("source")?,
            &documents
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn prepared_range_matches_rename_and_same_name_is_a_noop() -> TestResult {
    let marked = "[^a¦]\r\n\r\n[^a]: body\r\n";
    let source = marked.replace('¦', "");
    let prepared = call("textDocument/prepareRename", marked, "a", true)?;
    assert_eq!(span(&source, &prepared)?.slice(&source), Some("a"));
    let result = call("textDocument/rename", marked, "a", true)?;
    let changes = array(result.get("documentChanges").ok_or("changes")?);
    assert!(array(changes[0].get("edits").ok_or("edits")?).is_empty());
    assert_eq!(apply(&source, &result)?, source);
    Ok(())
}

#[test]
fn a_large_namespace_uses_complete_bulk_provenance_and_limits_refuse_partial_results() -> TestResult
{
    let source = format!("{}\n\n[^n]: note\n", "[^n] ".repeat(1000));
    let inventory =
        Inventory::parse(&source, candidates(&source).map_err(|(_, e)| e)?).map_err(|(_, e)| e)?;
    assert_eq!(inventory.notes.len(), 1001);
    let oversized = "[^n] ".repeat(MAX_SYMBOLS + 1);
    assert_eq!(
        Inventory::parse(&oversized, candidates(&oversized).map_err(|(_, e)| e)?)
            .err()
            .map(|(code, _)| code),
        Some(-32803)
    );
    let literals = format!("`{}`", "[^n] ".repeat(MAX_CANDIDATES + 1));
    assert_eq!(
        candidates(&literals).err().map(|(code, _)| code),
        Some(-32803)
    );
    Ok(())
}

#[test]
fn request_coordinates_and_reference_context_are_validated() -> TestResult {
    let source = "😀[^n]\n\n[^n]: note\n";
    let documents = BTreeMap::from([(URI.to_string(), buffer(source))]);
    let buffer = documents.get(URI).ok_or("source")?;
    let bad = object([
        (
            "position",
            object([("line", number(0)), ("character", number(1))]),
        ),
        (
            "context",
            object([("includeDeclaration", Json::Bool(false))]),
        ),
    ]);
    assert_eq!(
        request("textDocument/definition", &bad, URI, buffer, &documents)
            .err()
            .map(|(code, _)| code),
        Some(-32602)
    );
    let no_context = object([(
        "position",
        position(LineIndex::new(source).position(source, 6)),
    )]);
    assert_eq!(
        request(
            "textDocument/references",
            &no_context,
            URI,
            buffer,
            &documents
        )
        .err()
        .map(|(code, _)| code),
        Some(-32602)
    );
    assert!(
        request(
            "textDocument/documentHighlight",
            &no_context,
            URI,
            buffer,
            &documents
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn server_advertises_only_versioned_rename_and_keeps_heading_navigation() -> TestResult {
    for versioned in [false, true] {
        let mut server = Server::default();
        let initialize = object([
            ("jsonrpc", string("2.0")),
            ("id", number(1)),
            ("method", string("initialize")),
            (
                "params",
                object([(
                    "capabilities",
                    object([(
                        "workspace",
                        object([(
                            "workspaceEdit",
                            object([("documentChanges", Json::Bool(versioned))]),
                        )]),
                    )]),
                )]),
            ),
        ]);
        let replies = server.handle(initialize);
        let advertised = replies[0]
            .get("result")
            .and_then(|r| r.get("capabilities"))
            .and_then(|c| c.get("renameProvider"))
            .ok_or("rename capability")?;
        if versioned {
            assert_eq!(advertised.get("prepareProvider"), Some(&Json::Bool(true)));
        } else {
            assert_eq!(advertised, &Json::Bool(false));
        }
        let source = "# Heading\n\n[^n]\n\n[^n]: note\n\n[heading](#heading)\n";
        server.documents.insert(URI.to_string(), buffer(source));
        let rename = server.navigate(
            "textDocument/rename",
            &params(source, source.find("[^n]").ok_or("note")? + 2, "new", true),
        );
        if versioned {
            assert_eq!(
                apply(source, &rename.map_err(|(_, e)| e)?)?,
                source.replace("[^n]", "[^new]")
            );
            assert_eq!(
                server
                    .navigate(
                        "textDocument/prepareRename",
                        &params(source, 3, "new", true)
                    )
                    .map_err(|(_, e)| e)?,
                Json::Null
            );
        } else {
            assert_eq!(rename.err().map(|(code, _)| code), Some(-32601));
        }
        let heading = server
            .navigate(
                "textDocument/definition",
                &params(
                    source,
                    source.rfind("#heading").ok_or("link")? + 2,
                    "unused",
                    false,
                ),
            )
            .map_err(|(_, e)| e)?;
        assert_eq!(span(source, &heading)?.start, 0);
    }
    Ok(())
}

#[test]
fn rejected_changes_block_rename_until_full_text_resynchronization() -> TestResult {
    let source = "[^n]\n\n[^n]: original\n";
    let mut server = Server {
        phase: Phase::Running,
        versioned_edits: true,
        ..Server::default()
    };
    server.documents.insert(URI.to_string(), buffer(source));
    server.handle(notification(
        "textDocument/didChange",
        object([
            (
                "textDocument",
                object([("uri", string(URI)), ("version", number(8))]),
            ),
            (
                "contentChanges",
                Json::Array(vec![object([
                    (
                        "range",
                        object([
                            (
                                "start",
                                object([("line", number(99)), ("character", number(0))]),
                            ),
                            (
                                "end",
                                object([("line", number(99)), ("character", number(0))]),
                            ),
                        ]),
                    ),
                    ("text", string("bad")),
                ])]),
            ),
        ]),
    ));
    assert_eq!(
        server
            .navigate("textDocument/rename", &params(source, 2, "new", true))
            .err()
            .map(|(code, _)| code),
        Some(-32801)
    );
    let replacement = "😀 [^z]\r\n\r\n[^z]: replacement\r\n";
    server.handle(notification(
        "textDocument/didChange",
        object([
            (
                "textDocument",
                object([("uri", string(URI)), ("version", number(9))]),
            ),
            (
                "contentChanges",
                Json::Array(vec![object([("text", string(replacement))])]),
            ),
        ]),
    ));
    let rename = server
        .navigate(
            "textDocument/rename",
            &params(
                replacement,
                replacement.find("[^z]").ok_or("new note")? + 2,
                "fresh",
                true,
            ),
        )
        .map_err(|(_, e)| e)?;
    let changes = array(rename.get("documentChanges").ok_or("changes")?);
    assert_eq!(
        changes[0]
            .get("textDocument")
            .and_then(|d| d.get("version"))
            .and_then(Json::as_u64),
        Some(9)
    );
    assert_eq!(
        apply(replacement, &rename)?,
        replacement.replace("[^z]", "[^fresh]")
    );
    Ok(())
}

#[test]
fn rename_honors_remaining_session_byte_budget() -> TestResult {
    let source = "[^long]\n\n[^long]: note\n";
    let mut documents = BTreeMap::from([(URI.to_string(), buffer(source))]);
    let mut padding = MAX_SESSION_BYTES - source.len() - 4;
    for ordinal in 0..MAX_DOCUMENTS - 1 {
        if padding == 0 {
            break;
        }
        let bytes = padding.min(MAX_DOCUMENT_BYTES);
        documents.insert(
            format!("file:///docs/padding-{ordinal}.md"),
            buffer(&" ".repeat(bytes)),
        );
        padding -= bytes;
    }
    assert_eq!(
        padding, 0,
        "fixture must fit in the open-document count limit"
    );
    let session_bytes: usize = documents.values().map(|buffer| buffer.text.len()).sum();
    assert_eq!(session_bytes, MAX_SESSION_BYTES - 4);
    let current = documents.get(URI).ok_or("source")?;

    // Each of two labels grows by three bytes; the second edit crosses the
    // session cap although the edited document is far below its own limit.
    let refused = request(
        "textDocument/rename",
        &params(source, 2, "longest", true),
        URI,
        current,
        &documents,
    );
    assert_eq!(refused.err().map(|(code, _)| code), Some(-32803));
    assert_eq!(current.text, source);
    assert_eq!(current.version, 7);

    // Exactly consuming the remaining budget and releasing bytes both work.
    for (name, remaining) in [("longer", 0), ("n", 10)] {
        let renamed = request(
            "textDocument/rename",
            &params(source, 2, name, true),
            URI,
            current,
            &documents,
        )
        .map_err(|(_, error)| error)?
        .ok_or("missing rename")?;
        let amended = apply(source, &renamed)?;
        assert_eq!(amended, source.replace("[^long]", &format!("[^{name}]")));
        let amended_session_bytes = session_bytes - source.len() + amended.len();
        assert_eq!(MAX_SESSION_BYTES - amended_session_bytes, remaining);
    }
    Ok(())
}

#[test]
fn possible_anchor_targets_through_open_uri_aliases_block_rename() -> TestResult {
    let source = "[^n]\n\n[^n]: note\n";
    for alias in [
        "file:///docs/./article.md",
        "file://localhost/docs/article.md",
        "file:///docs/%61rticle.md",
    ] {
        let reader = "file:///docs/reader.md";
        let mut documents = BTreeMap::from([
            (URI.to_string(), buffer(source)),
            (alias.to_string(), buffer(source)),
            (reader.to_string(), buffer("[see](article.md#fn-n)")),
        ]);
        // Navigation cannot select one of two URI aliases, but either is a
        // possible rename target and the ordinary anchor link must survive.
        for method in ["textDocument/prepareRename", "textDocument/rename"] {
            assert_eq!(
                request(
                    method,
                    &params(source, 2, "new", true),
                    URI,
                    documents.get(URI).ok_or("source")?,
                    &documents,
                )
                .err()
                .map(|(code, _)| code),
                Some(-32803),
                "allowed {method} with ambiguous alias {alias}"
            );
        }

        documents.remove(reader);
        documents.get_mut(alias).ok_or("alias")?.text = format!("{source}\n[see](#fn-n)\n");
        assert_eq!(
            request(
                "textDocument/rename",
                &params(source, 2, "new", true),
                URI,
                documents.get(URI).ok_or("source")?,
                &documents,
            )
            .err()
            .map(|(code, _)| code),
            Some(-32803),
            "local anchor link in alias {alias} must also remain valid"
        );

        // Merely opening an alias does not forbid a local label edit when no
        // ordinary anchor link depends on the renamed label.
        documents.get_mut(alias).ok_or("alias")?.text = source.to_string();
        let renamed = request(
            "textDocument/rename",
            &params(source, 2, "new", true),
            URI,
            documents.get(URI).ok_or("source")?,
            &documents,
        )
        .map_err(|(_, error)| error)?
        .ok_or("missing rename")?;
        assert_eq!(apply(source, &renamed)?, source.replace("[^n]", "[^new]"));
    }
    Ok(())
}

#[test]
fn excess_footnote_literals_do_not_disable_heading_navigation() -> TestResult {
    let source = format!(
        "# Heading\n\n```\n{}\n```\n\n[heading](#heading)\n",
        "[^n] ".repeat(MAX_CANDIDATES + 1),
    );
    let mut server = Server {
        phase: Phase::Running,
        ..Server::default()
    };
    server.documents.insert(URI.to_string(), buffer(&source));
    let definition = server
        .navigate(
            "textDocument/definition",
            &params(
                &source,
                source.rfind("#heading").ok_or("link")? + 2,
                "unused",
                false,
            ),
        )
        .map_err(|(_, error)| error)?;
    assert_eq!(span(&source, &definition)?.start, 0);
    Ok(())
}

#[test]
fn maximal_utf8_labels_remain_navigable_at_both_marker_edges() -> TestResult {
    let id = format!("{}ab", "猫".repeat(42));
    assert_eq!(id.len(), 128);
    let source = format!(
        "{}[^{id}]{}\n\n[^{id}]: note\n",
        "😀".repeat(50),
        "😀".repeat(50),
    );
    let documents = BTreeMap::from([(URI.to_string(), buffer(&source))]);
    let start = source.find("[^").ok_or("reference")?;
    let expected = source.rfind("[^").ok_or("definition")? + 2;
    for offset in [start, start + 1, start + 2, start + 2 + id.len()] {
        let definition = request(
            "textDocument/definition",
            &params(&source, offset, "unused", false),
            URI,
            documents.get(URI).ok_or("source")?,
            &documents,
        )
        .map_err(|(_, error)| error)?
        .ok_or("missing definition")?;
        assert_eq!(span(&source, &definition)?.start, expected);
        assert_eq!(
            span(&source, &definition)?.slice(&source),
            Some(id.as_str())
        );
    }
    Ok(())
}
