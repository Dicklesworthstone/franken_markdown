use super::super::{Phase, Server, notification};
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const URI: &str = "file:///docs/guide.md";

fn buffer(source: &str, version: i32) -> Buffer {
    Buffer {
        text: source.to_string(),
        version,
        synchronized: true,
    }
}

fn params(uri: &str, source: &str, offset: usize, name: &str) -> Json {
    object([
        ("textDocument", object([("uri", string(uri))])),
        (
            "position",
            position(LineIndex::new(source).position(source, offset)),
        ),
        ("newName", string(name)),
    ])
}

fn array(value: &Json) -> &[Json] {
    if let Json::Array(values) = value {
        values
    } else {
        &[]
    }
}

fn edits<'a>(result: &'a Json, uri: &str) -> &'a [Json] {
    result
        .get("documentChanges")
        .map(array)
        .unwrap_or_default()
        .iter()
        .find(|change| {
            change
                .get("textDocument")
                .and_then(|doc| doc.get("uri"))
                .and_then(Json::as_str)
                == Some(uri)
        })
        .and_then(|change| change.get("edits"))
        .map(array)
        .unwrap_or_default()
}

fn source_range(source: &str, range: &Json) -> Result<Range<usize>, &'static str> {
    let index = LineIndex::new(source);
    Ok(index.offset(
        source,
        Position::parse(range.get("start").ok_or("missing start")?)?,
    )?
        ..index.offset(
            source,
            Position::parse(range.get("end").ok_or("missing end")?)?,
        )?)
}

fn apply_result(source: &str, result: &Json, uri: &str) -> Result<String, &'static str> {
    let mut output = source.to_string();
    for edit in edits(result, uri).iter().rev() {
        let range = source_range(source, edit.get("range").ok_or("missing range")?)?;
        output.replace_range(
            range,
            edit.get("newText")
                .and_then(Json::as_str)
                .ok_or("missing new text")?,
        );
    }
    Ok(output)
}

fn call(marked: &str, name: &str) -> Result<(String, Json), Box<dyn std::error::Error>> {
    let offset = marked.find('¦').ok_or("missing caret")?;
    let source = marked.replace('¦', "");
    let documents = BTreeMap::from([(URI.to_string(), buffer(&source, 7))]);
    let result = request(
        "textDocument/rename",
        &params(URI, &source, offset, name),
        URI,
        documents.get(URI).ok_or("missing document")?,
        &documents,
    )
    .map_err(|(_, message)| message)?;
    Ok((source, result))
}

fn failure(marked: &str, name: &str) -> Result<Failure, &'static str> {
    let offset = marked.find('¦').ok_or("missing caret")?;
    let source = marked.replace('¦', "");
    let documents = BTreeMap::from([(URI.to_string(), buffer(&source, 7))]);
    request(
        "textDocument/rename",
        &params(URI, &source, offset, name),
        URI,
        documents.get(URI).ok_or("missing document")?,
        &documents,
    )
    .err()
    .ok_or("expected refusal")
}

#[test]
fn atx_title_and_fragment_edits_preserve_delimiters_query_title_and_crlf() -> TestResult {
    let marked = "  ## Old¦ title ## \r\n\r\n[x](?view=print#old-title \"Title\") [y](<#old-title>) ![image](#old-title)\r\n";
    let (source, result) = call(marked, "New heading")?;
    assert_eq!(
        apply_result(&source, &result, URI)?,
        "  ## New heading ## \r\n\r\n[x](?view=print#new-heading \"Title\") [y](<#new-heading>) ![image](#old-title)\r\n"
    );
    assert_eq!(edits(&result, URI).len(), 3);
    let change = &array(result.get("documentChanges").ok_or("no changes")?)[0];
    assert_eq!(
        change
            .get("textDocument")
            .and_then(|doc| doc.get("version"))
            .and_then(Json::as_f64),
        Some(7.0)
    );
    assert_eq!(
        source,
        marked.replace('¦', ""),
        "the live source is never mutated"
    );
    Ok(())
}

#[test]
fn setext_title_supports_bom_frontmatter_unicode_and_original_utf16_ranges() -> TestResult {
    let marked = "\u{feff}---\r\ntitle: Saved metadata\r\n---\r\n\r\n猫 Old¦\r\n=======\r\n\r\n😀 [jump](#%E7%8C%AB-old)\r\n";
    let (source, result) = call(marked, "新章")?;
    assert_eq!(
        apply_result(&source, &result, URI)?,
        "\u{feff}---\r\ntitle: Saved metadata\r\n---\r\n\r\n新章\r\n=======\r\n\r\n😀 [jump](#%E6%96%B0%E7%AB%A0)\r\n"
    );
    let first = &edits(&result, URI)[0];
    let range = first.get("range").ok_or("missing range")?;
    assert_eq!(
        range
            .get("start")
            .and_then(|p| p.get("line"))
            .and_then(Json::as_u64),
        Some(4)
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
fn reference_definitions_edit_once_without_rewriting_uses_unused_or_shadowed_definitions()
-> TestResult {
    let marked = "# Ol¦d\n\n[one][ref] [ref][] [ref]\n\n[ref]:\n  <#old>\n  \"definition title\"\n\n[ref]: #old 'shadowed'\n[unused]: #old\n\n![only image][image]\n\n[image]: #old\n";
    let (source, result) = call(marked, "New")?;
    assert_eq!(
        apply_result(&source, &result, URI)?,
        source
            .replacen("# Old", "# New", 1)
            .replacen("<#old>", "<#new>", 1)
    );
    assert_eq!(edits(&result, URI).len(), 2);
    Ok(())
}

#[test]
fn source_tokens_in_all_containers_and_dormant_notes_are_updated() -> TestResult {
    let marked = "# Ol¦d\n\n> [quote](#old)\n\n- [list](#old)\n\n| head |\n| --- |\n| [cell](#old) |\n\n[^note]: [dormant](#old)\n\nTerm\n: [definition](#old)\n";
    let (source, result) = call(marked, "New")?;
    assert_eq!(
        apply_result(&source, &result, URI)?,
        source
            .replacen("# Old", "# New", 1)
            .replace("(#old)", "(#new)")
    );
    assert_eq!(edits(&result, URI).len(), 6);
    Ok(())
}

#[test]
fn cross_file_edits_keep_original_uris_versions_paths_queries_and_percent_fragments() -> TestResult
{
    let source = "# Old\n";
    let origin = "file:///docs/sub/source.md";
    let other = "[a](../guide.md?view=print#%6Fld \"title\")\n[b](../guide#old)\n[c](file:///docs/guide.md#old)\n";
    let unrelated_uri = "file:///docs/unrelated.md";
    let unrelated = "# Old\n\n[local](#old) [other](https://example.test/guide.md#old)\n";
    let documents = BTreeMap::from([
        (URI.to_string(), buffer(source, 9)),
        (origin.to_string(), buffer(other, 32)),
        (unrelated_uri.to_string(), buffer(unrelated, 1)),
    ]);
    let result = request(
        "textDocument/rename",
        &params(URI, source, 3, "Next"),
        URI,
        documents.get(URI).ok_or("missing source")?,
        &documents,
    )
    .map_err(|(_, m)| m)?;
    assert_eq!(apply_result(source, &result, URI)?, "# Next\n");
    assert_eq!(
        apply_result(other, &result, origin)?,
        "[a](../guide.md?view=print#next \"title\")\n[b](../guide#next)\n[c](file:///docs/guide.md#next)\n"
    );
    assert!(edits(&result, unrelated_uri).is_empty());
    let changes = array(result.get("documentChanges").ok_or("missing changes")?);
    assert_eq!(changes.len(), 2);
    for change in changes {
        let doc = change.get("textDocument").ok_or("missing document")?;
        let uri = doc.get("uri").and_then(Json::as_str).ok_or("missing uri")?;
        assert_eq!(
            doc.get("version").and_then(Json::as_f64),
            Some(f64::from(documents[uri].version))
        );
    }
    Ok(())
}

#[test]
fn entities_escapes_and_angle_url_delimiters_survive_fragment_only_edits() -> TestResult {
    let marked = "# Ol¦d\n\n[a](<guide.md?x=1&amp;y=2&#35;%6Fld> \"title\") [b](?x=1&y=2\\#old) [c](?x=&#35;old) [d](#&#111;ld)\n\n[r][ref]\n\n[ref]: <guide.md?x=1&amp;y=2&num;old> 'reference title'\n";
    let (source, result) = call(marked, "New")?;
    assert_eq!(
        apply_result(&source, &result, URI)?,
        "# New\n\n[a](<guide.md?x=1&amp;y=2&#35;new> \"title\") [b](?x=1&y=2\\#new) [c](?x=&#35;new) [d](#new)\n\n[r][ref]\n\n[ref]: <guide.md?x=1&amp;y=2&num;new> 'reference title'\n"
    );
    Ok(())
}

#[test]
fn shared_reference_destinations_used_by_images_refuse_the_entire_edit() -> TestResult {
    let marked = "# Ol¦d\n\n[link][same] ![image][same] [independent](#old)\n\n[same]: #old\n";
    let (code, message) = failure(marked, "New")?;
    assert_eq!(code, -32803);
    assert!(message.contains("shared with an image"));
    Ok(())
}

#[test]
fn changing_duplicate_suffixes_refuses_but_renaming_the_last_duplicate_is_safe() -> TestResult {
    let (code, message) = failure(
        "# Sa¦me\n\n# Same\n\n[a](#same) [b](#same-2)\n",
        "Different",
    )?;
    assert_eq!(code, -32803);
    assert!(message.contains("other heading or footnote anchors"));
    let (source, result) = call(
        "# Same\n\n# Sa¦me\n\n[a](#same) [b](#same-2)\n",
        "Different",
    )?;
    assert_eq!(
        apply_result(&source, &result, URI)?,
        "# Same\n\n# Different\n\n[a](#same) [b](#different)\n"
    );
    Ok(())
}

#[test]
fn emitted_footnote_collisions_and_disappearing_footnotes_refuse() -> TestResult {
    assert_eq!(
        failure("# Ol¦d\n\nA[^x]\n\n[^x]: Note\n", "fn x")?.0,
        -32803
    );
    assert_eq!(failure("# Ol¦d[^x]\n\n[^x]: Note\n", "New")?.0, -32803);
    assert_eq!(
        failure("# fn-¦x\n\nA[^x]\n\n[^x]: Note\n", "New")?.0,
        -32803
    );
    Ok(())
}

#[test]
fn heading_text_is_literal_and_html_code_math_and_image_lookalikes_are_preserved() -> TestResult {
    let marked = "# **Ol¦d**\n\n<div id=\"new\">\n[html](#old)\n</div>\n\n`[code](#old)` $[math](#old)$ ![image](#old) [real](#old)\n\n```md\n# Old\n[code](#old)\n```\n";
    let (source, result) = call(marked, "New * & [literal]")?;
    let amended = apply_result(&source, &result, URI)?;
    assert!(amended.starts_with("# New \\* \\& \\[literal\\]\n"));
    assert!(amended.contains("<div id=\"new\">\n[html](#old)\n</div>"));
    assert!(amended.contains("`[code](#old)` $[math](#old)$ ![image](#old) [real](#new-literal)"));
    assert!(amended.ends_with("```md\n# Old\n[code](#old)\n```\n"));
    assert_eq!(
        parse_markdown(&amended).blocks[0],
        Block::Heading {
            level: 1,
            inlines: vec![Inline::Text("New * & [literal]".to_string())],
        }
    );
    Ok(())
}

#[test]
fn declaration_only_scope_excludes_nested_headings_and_multiline_setext() -> TestResult {
    for marked in [
        "# Old\n\n[a](#ol¦d)\n",
        "> # Ne¦sted\n",
        "- # Ne¦sted\n",
        "two¦\nlines\n=====\n",
        "```\n# Ol¦d\n```\n",
    ] {
        let (_, result) = call(marked, "New")?;
        assert_eq!(result, Json::Null, "{marked}");
    }
    Ok(())
}

#[test]
fn declaration_caret_boundary_accepts_title_end_but_not_the_following_paragraph() -> TestResult {
    for marked in ["# Old\n¦body", "# Old\r\n¦body"] {
        let (source, result) = call(marked, "New")?;
        let parsed = parse_markdown_spanned(&source);
        assert_eq!(parsed.blocks[0].span.end, "# Old".len());
        assert_eq!(result, Json::Null);
    }
    for marked in ["# Old¦", "# Old¦\nbody", "# Old¦\r\nbody"] {
        let (source, result) = call(marked, "New")?;
        assert_eq!(
            apply_result(&source, &result, URI)?,
            source.replacen("# Old", "# New", 1)
        );
        let documents = BTreeMap::from([(URI.to_string(), buffer(&source, 4))]);
        let prepared = request(
            "textDocument/prepareRename",
            &params(URI, &source, "# Old".len(), "unused"),
            URI,
            &documents[URI],
            &documents,
        )
        .map_err(|(_, m)| m)?;
        let range = source_range(&source, prepared.get("range").ok_or("missing range")?)?;
        assert_eq!(&source[range], "Old");
    }
    Ok(())
}

#[test]
fn prepare_rename_returns_only_title_range_and_noop_keeps_inline_markup() -> TestResult {
    let source = "  ### **Old** ###\n";
    let documents = BTreeMap::from([(URI.to_string(), buffer(source, 4))]);
    let result = request(
        "textDocument/prepareRename",
        &params(URI, source, 8, "unused"),
        URI,
        documents.get(URI).ok_or("missing source")?,
        &documents,
    )
    .map_err(|(_, m)| m)?;
    let range = source_range(source, result.get("range").ok_or("missing range")?)?;
    assert_eq!(&source[range], "**Old**");
    assert_eq!(
        result.get("placeholder").and_then(Json::as_str),
        Some("Old")
    );
    let (_, noop) = call("  ### **Ol¦d** ###\n", "Old")?;
    assert_eq!(noop, object([("documentChanges", Json::Array(Vec::new()))]));
    Ok(())
}

#[test]
fn stale_buffers_and_file_identity_ambiguity_refuse_without_partial_results() -> TestResult {
    let source = "# Old\n";
    let mut documents = BTreeMap::from([
        (URI.to_string(), buffer(source, 2)),
        (
            "file:///docs/other.md".to_string(),
            buffer("[x](guide.md#old)\n", 9),
        ),
    ]);
    documents
        .get_mut("file:///docs/other.md")
        .ok_or("missing other")?
        .synchronized = false;
    let run = |documents: &BTreeMap<String, Buffer>| {
        request(
            "textDocument/rename",
            &params(URI, source, 3, "New"),
            URI,
            &documents[URI],
            documents,
        )
    };
    assert_eq!(
        run(&documents).err().ok_or("expected stale refusal")?.0,
        -32801
    );
    documents
        .get_mut("file:///docs/other.md")
        .ok_or("missing other")?
        .synchronized = true;
    documents.insert("file://localhost/docs/%67uide.md".into(), buffer(source, 3));
    assert_eq!(
        run(&documents)
            .err()
            .ok_or("expected target alias refusal")?
            .0,
        -32803
    );
    let documents = BTreeMap::from([
        (URI.to_string(), buffer(source, 2)),
        (
            "file:///docs/other.md".to_string(),
            buffer("[x](guide.md#old)\n", 9),
        ),
        (
            "file:///docs/./other.md".to_string(),
            buffer("[x](guide.md#old)\n", 10),
        ),
    ]);
    assert_eq!(
        run(&documents)
            .err()
            .ok_or("expected edited alias refusal")?
            .0,
        -32803
    );
    Ok(())
}

#[test]
fn ambiguous_extensionless_targets_and_unmapped_autolinks_refuse() -> TestResult {
    let source = "# Old\n";
    for (other, additional) in [
        (
            "[x](guide#old)\n",
            Some(("file:///docs/guide.markdown", "# Old\n")),
        ),
        ("<file:///docs/guide.md#old>\n", None),
    ] {
        let mut documents = BTreeMap::from([
            (URI.to_string(), buffer(source, 2)),
            ("file:///docs/other.md".to_string(), buffer(other, 9)),
        ]);
        if let Some((uri, text)) = additional {
            documents.insert(uri.into(), buffer(text, 1));
        }
        let result = request(
            "textDocument/rename",
            &params(URI, source, 3, "New"),
            URI,
            documents.get(URI).ok_or("missing source")?,
            &documents,
        );
        assert_eq!(
            result.err().ok_or("expected complete-coverage refusal")?.0,
            -32803
        );
        assert_eq!(documents[URI].text, source);
    }
    Ok(())
}

#[test]
fn invalid_names_and_edit_budget_fail_atomically() -> TestResult {
    for name in [
        "",
        " leading",
        "trailing ",
        "two\nlines",
        "tab\tname",
        "bad\0name",
        "line\u{2028}separator",
        "paragraph\u{2029}separator",
    ] {
        assert_eq!(failure("# Ol¦d\n\n[x](#old)\n", name)?.0, -32602);
    }
    assert_eq!(
        failure("# Ol¦d\n", &"x".repeat(MAX_NAME_BYTES + 1))?.0,
        -32602
    );
    let source = format!("# Ol¦d\n\n{}", "[x](#old)\n".repeat(MAX_EDITS));
    assert_eq!(failure(&source, "New")?.0, -32803);
    Ok(())
}

#[test]
fn source_and_session_limits_remain_hard_limits_for_renames() -> TestResult {
    let source = format!(
        "# Ol¦d\n\n{}",
        "x".repeat(MAX_DOCUMENT_BYTES - "# Old\n\n".len())
    );
    assert_eq!(failure(&source, "Longer")?.0, -32803);
    let source = "# Old\n";
    let mut documents = BTreeMap::from([(URI.to_string(), buffer(source, 1))]);
    for n in 0..8 {
        documents.insert(
            format!("untitled:large{n}"),
            buffer(&"x".repeat(MAX_DOCUMENT_BYTES), 1),
        );
    }
    let result = request(
        "textDocument/rename",
        &params(URI, source, 3, "New"),
        URI,
        &documents[URI],
        &documents,
    );
    assert_eq!(result.err().ok_or("expected session refusal")?.0, -32803);
    Ok(())
}

#[test]
fn dispatch_negotiates_versioned_edits_and_keeps_footnote_rename_working() -> TestResult {
    let mut server = Server {
        phase: Phase::Running,
        versioned_edits: true,
        ..Server::default()
    };
    let source = "# Old\n\nA[^note] [jump](#old)\n\n[^note]: Body\n";
    let messages = server.handle(notification(
        "textDocument/didOpen",
        object([(
            "textDocument",
            object([
                ("uri", string(URI)),
                ("version", Json::Number(8.0)),
                ("text", string(source)),
            ]),
        )]),
    ));
    assert!(!messages.is_empty());
    let rename = server
        .navigate("textDocument/rename", &params(URI, source, 3, "New"))
        .map_err(|(_, m)| m)?;
    assert!(apply_result(source, &rename, URI)?.starts_with("# New\n"));
    let note = source.find("[^note]").ok_or("missing note")? + 3;
    let note_edit = server
        .navigate("textDocument/rename", &params(URI, source, note, "renamed"))
        .map_err(|(_, m)| m)?;
    assert!(apply_result(source, &note_edit, URI)?.contains("A[^renamed]"));
    server.versioned_edits = false;
    assert_eq!(
        server
            .navigate("textDocument/rename", &params(URI, source, 3, "New"))
            .err()
            .ok_or("expected negotiation refusal")?
            .0,
        -32601
    );
    assert_eq!(server.documents[URI].text, source);
    Ok(())
}

#[test]
fn live_changes_are_used_and_opaque_buffers_have_no_inferred_file_authority() -> TestResult {
    let uri = "untitled:guide";
    let source = "# Old\n\n[local](#old) [file](guide.md#old)\n";
    let mut documents = BTreeMap::from([(uri.to_string(), buffer(source, 4))]);
    let result = request(
        "textDocument/rename",
        &params(uri, source, 3, "New"),
        uri,
        &documents[uri],
        &documents,
    )
    .map_err(|(_, m)| m)?;
    assert_eq!(
        apply_result(source, &result, uri)?,
        "# New\n\n[local](#new) [file](guide.md#old)\n"
    );
    documents.get_mut(uri).ok_or("missing source")?.text =
        "# Current\n\n[local](#current)\n".into();
    documents.get_mut(uri).ok_or("missing source")?.version = 5;
    let current = &documents[uri];
    let result = request(
        "textDocument/rename",
        &params(uri, &current.text, 4, "Latest"),
        uri,
        current,
        &documents,
    )
    .map_err(|(_, m)| m)?;
    assert_eq!(
        apply_result(&current.text, &result, uri)?,
        "# Latest\n\n[local](#latest)\n"
    );
    Ok(())
}
