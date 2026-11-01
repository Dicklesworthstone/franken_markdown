use super::super::{Phase, Server, notification};
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const URI: &str = "file:///docs/reference.md";

fn buffer(source: &str) -> Buffer {
    Buffer {
        text: source.to_string(),
        version: 7,
        synchronized: true,
    }
}

fn params(source: &str, offset: usize, name: &str) -> Json {
    object([
        ("textDocument", object([("uri", string(URI))])),
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

fn edits(result: &Json) -> &[Json] {
    result
        .get("documentChanges")
        .map(array)
        .unwrap_or_default()
        .first()
        .and_then(|change| change.get("edits"))
        .map(array)
        .unwrap_or_default()
}

fn apply(source: &str, result: &Json) -> Result<String, &'static str> {
    let index = LineIndex::new(source);
    let mut revised = source.to_string();
    for edit in edits(result).iter().rev() {
        let range = edit.get("range").ok_or("missing range")?;
        let start = index.offset(
            source,
            Position::parse(range.get("start").ok_or("missing start")?)?,
        )?;
        let end = index.offset(
            source,
            Position::parse(range.get("end").ok_or("missing end")?)?,
        )?;
        revised.replace_range(
            start..end,
            edit.get("newText")
                .and_then(Json::as_str)
                .ok_or("missing replacement")?,
        );
    }
    Ok(revised)
}

fn call(method: &str, marked: &str, name: &str) -> Result<(String, Option<Json>), Failure> {
    let offset = marked.find('¦').ok_or((-32602, "missing caret"))?;
    let source = marked.replace('¦', "");
    let documents = BTreeMap::from([(URI.to_string(), buffer(&source))]);
    let result = request(
        method,
        &params(&source, offset, name),
        URI,
        documents.get(URI).ok_or((-32602, "missing document"))?,
        &documents,
    )?;
    Ok((source, result))
}

fn rename(marked: &str, name: &str) -> Result<(String, Json), Box<dyn std::error::Error>> {
    let (source, result) =
        call("textDocument/rename", marked, name).map_err(|(_, message)| message)?;
    Ok((source, result.ok_or("missing rename result")?))
}

fn refusal(marked: &str, name: &str) -> Result<Failure, &'static str> {
    call("textDocument/rename", marked, name)
        .err()
        .ok_or("expected refusal")
}

#[test]
fn full_collapsed_shortcut_links_and_images_preserve_visible_content_and_metadata() -> TestResult {
    let marked = "[Shown][re¦f] [ref][] [ref] ![Alt][ref] ![ref][] ![ref]\n\n[ref]: <image.svg#part> \"Original title\"\n";
    let (source, result) = rename(marked, "manual")?;
    let expected = "[Shown][manual] [ref][manual] [ref][manual] ![Alt][manual] ![ref][manual] ![ref][manual]\n\n[manual]: <image.svg#part> \"Original title\"\n";
    assert_eq!(apply(&source, &result)?, expected);
    assert_eq!(parse_markdown(&source), parse_markdown(expected));
    assert_eq!(edits(&result).len(), 7);
    let changes = array(result.get("documentChanges").ok_or("no changes")?);
    let doc = changes[0].get("textDocument").ok_or("missing identity")?;
    assert_eq!(doc.get("uri").and_then(Json::as_str), Some(URI));
    assert_eq!(doc.get("version").and_then(Json::as_f64), Some(7.0));
    Ok(())
}

#[test]
fn definition_origin_renames_unused_and_shadowed_definitions_without_changing_precedence()
-> TestResult {
    let (source, result) = rename(
        "[Stra¦ße]: /first \"Title\"\n[strasse]: /shadow\n\n[STRASSE] [label][Straße]\n",
        "guide",
    )?;
    assert_eq!(
        apply(&source, &result)?,
        "[guide]: /first \"Title\"\n[guide]: /shadow\n\n[STRASSE][guide] [label][guide]\n"
    );
    assert_eq!(
        parse_markdown(&source),
        parse_markdown(&apply(&source, &result)?)
    );
    let (source, result) = rename("[un¦used]: /target\n", "future")?;
    assert_eq!(apply(&source, &result)?, "[future]: /target\n");
    Ok(())
}

#[test]
fn original_utf16_ranges_survive_bom_crlf_containers_tables_and_dormant_notes() -> TestResult {
    let marked = "\u{feff}> [ref]: /asset\r\n>\r\n> 😀 [shown][r¦ef]\r\n\r\n-\t[ref][]\r\n\r\n| Cell |\r\n| --- |\r\n| [ref] |\r\n\r\n[^dormant]: ![ref]\r\n";
    let (source, result) = rename(marked, "new")?;
    assert_eq!(
        apply(&source, &result)?,
        "\u{feff}> [new]: /asset\r\n>\r\n> 😀 [shown][new]\r\n\r\n-\t[ref][new]\r\n\r\n| Cell |\r\n| --- |\r\n| [ref][new] |\r\n\r\n[^dormant]: ![ref][new]\r\n"
    );
    let position = edits(&result)[1]
        .get("range")
        .and_then(|range| range.get("start"))
        .ok_or("missing range")?;
    assert_eq!(position.get("line").and_then(Json::as_u64), Some(2));
    assert_eq!(position.get("character").and_then(Json::as_u64), Some(13));
    Ok(())
}

#[test]
fn same_normalized_key_keeps_shortcuts_and_collapsed_forms_and_noop_is_empty() -> TestResult {
    let (source, result) = rename("[shown][re¦f] [REF][] [ref]\n\n[ref]: /url\n", "REF")?;
    assert_eq!(
        apply(&source, &result)?,
        "[shown][REF] [REF][] [ref]\n\n[REF]: /url\n"
    );
    assert_eq!(edits(&result).len(), 2);
    let (_, noop) = rename("[r¦ef]\n\n[ref]: /url\n", "ref")?;
    assert_eq!(noop, object([("documentChanges", Json::Array(Vec::new()))]));
    Ok(())
}

#[test]
fn prepare_rename_selects_only_the_original_key_and_not_visible_full_reference_text() -> TestResult
{
    for marked in [
        "[display][r¦ef]\n\n[ref]: /url",
        "[r¦ef][]\n\n[ref]: /url",
        "[r¦ef]\n\n[ref]: /url",
        "[r¦ef]: /url",
    ] {
        let (source, result) =
            call("textDocument/prepareRename", marked, "").map_err(|(_, reason)| reason)?;
        let result = result.ok_or("missing prepare result")?;
        assert_eq!(
            result.get("placeholder").and_then(Json::as_str),
            Some("ref")
        );
        let range = result.get("range").ok_or("missing selection")?;
        let index = LineIndex::new(&source);
        let start = index.offset(
            &source,
            Position::parse(range.get("start").ok_or("no start")?)?,
        )?;
        let end = index.offset(&source, Position::parse(range.get("end").ok_or("no end")?)?)?;
        assert_eq!(&source[start..end], "ref");
    }
    assert!(
        call(
            "textDocument/prepareRename",
            "[di¦splay][ref]\n\n[ref]: /url",
            ""
        )
        .map_err(|(_, reason)| reason)?
        .1
        .is_none()
    );
    Ok(())
}

#[test]
fn collisions_and_newly_resolved_literal_text_refuse_the_whole_edit() -> TestResult {
    assert_eq!(
        refusal("[re¦f]\n\n[ref]: /old\n[Straße]: /other", "STRASSE")?.0,
        -32803
    );
    let failure = refusal("[re¦f] [new]\n\n[ref]: /old", "new")?;
    assert_eq!(failure.0, -32803);
    assert!(failure.1.contains("meaning"));
    Ok(())
}

#[test]
fn accepted_references_inside_both_image_alt_forms_keep_the_exact_alt_text() -> TestResult {
    let marked =
        "![see [ref]](image.png) ![outer [ref]][picture]\n\n[re¦f]: /old\n[picture]: /image";
    let (source, result) = rename(marked, "new")?;
    let revised = apply(&source, &result)?;
    assert_eq!(
        revised,
        "![see [ref][new]](image.png) ![outer [ref][new]][picture]\n\n[new]: /old\n[picture]: /image"
    );
    assert_eq!(parse_markdown(&source), parse_markdown(&revised));
    Ok(())
}

#[test]
fn name_validation_uses_native_grammar_and_pre_fold_scalar_limit() -> TestResult {
    for name in ["", "   ", "a\nb", "a\tb", "bad]label", "[nested]", "^note"] {
        assert_eq!(
            refusal("[re¦f]\n\n[ref]: /url", name)?.0,
            -32602,
            "{name:?}"
        );
    }
    assert_eq!(
        refusal("[re¦f]\n\n[ref]: /url", &"a".repeat(1000))?.0,
        -32602
    );
    let name = "ﬃ".repeat(999);
    let (source, result) = rename("[re¦f]\n\n[ref]: /url", &name)?;
    assert_eq!(
        parse_markdown(&source),
        parse_markdown(&apply(&source, &result)?)
    );
    let (source, result) = rename("[re¦f]\n\n[ref]: /url", r"new\]label")?;
    assert_eq!(
        apply(&source, &result)?,
        "[ref][new\\]label]\n\n[new\\]label]: /url"
    );
    Ok(())
}

#[test]
fn noncontiguous_occurrences_refuse_instead_of_silently_omitting_a_use() -> TestResult {
    for marked in [
        "> [two\n> words]: /url\n>\n> [shown][two¦ words]",
        "[two¦ words]: /url\n\n> [two\n> words][]",
    ] {
        assert_eq!(refusal(marked, "new")?.0, -32803);
    }
    Ok(())
}

#[test]
fn dormant_note_definitions_and_literals_keep_document_global_reference_meaning() -> TestResult {
    let marked = "[^dormant]: [ref]: /first\n\n[ref]: /shadow\n\n[shown][re¦f]\n";
    let (source, result) = rename(marked, "new")?;
    let revised = apply(&source, &result)?;
    assert_eq!(
        revised,
        "[^dormant]: [new]: /first\n\n[new]: /shadow\n\n[shown][new]\n"
    );
    assert_eq!(parse_markdown(&source), parse_markdown(&revised));
    assert_eq!(
        refusal("[re¦f]: /old\n\n[^dormant]: [new]\n", "new")?.0,
        -32803
    );
    assert_eq!(
        refusal("[re¦f]: /old\n\n[^dormant]: [NEW]: /other\n", "new")?.0,
        -32803
    );
    Ok(())
}

#[test]
fn code_html_inline_destinations_and_unresolved_text_are_not_reference_selections() -> TestResult {
    for marked in [
        "`[r¦ef]`\n\n[ref]: /url",
        "```md\n[r¦ef]\n```\n\n[ref]: /url",
        "<div>\n[r¦ef]\n</div>\n\n[ref]: /url",
        "[missing¦]",
        "[label](/r¦ef)\n\n[ref]: /url",
    ] {
        assert!(
            call("textDocument/rename", marked, "new")
                .map_err(|(_, reason)| reason)?
                .1
                .is_none(),
            "{marked}"
        );
    }
    Ok(())
}

#[test]
fn symbol_and_edit_budgets_refuse_complete_requests() -> TestResult {
    let many_symbols = format!("[r¦ef]\n\n{}", "[ref]: /url\n".repeat(MAX_SYMBOLS));
    assert_eq!(refusal(&many_symbols, "new")?.0, -32803);
    let many_edits = format!("[r¦ef] {}\n\n[ref]: /url\n", "[ref] ".repeat(MAX_EDITS));
    assert_eq!(refusal(&many_edits, "new")?.0, -32803);
    let heading = format!("# Ti¦tle\n\n{}", "[ref]: /url\n".repeat(MAX_SYMBOLS + 1));
    assert!(
        call("textDocument/rename", &heading, "new")
            .map_err(|(_, reason)| reason)?
            .1
            .is_none(),
        "the reference provider must delegate positions it does not own"
    );
    Ok(())
}

#[test]
fn dispatch_keeps_document_version_and_rejects_stale_or_unversioned_clients() -> TestResult {
    let source = "[ref]\n\n[ref]: /url";
    let mut server = Server {
        phase: Phase::Running,
        versioned_edits: true,
        ..Server::default()
    };
    server.documents.insert(URI.to_string(), buffer(source));
    let result = server
        .navigate("textDocument/rename", &params(source, 2, "new"))
        .map_err(|(_, reason)| reason)?;
    assert_eq!(apply(source, &result)?, "[ref][new]\n\n[new]: /url");
    assert_eq!(
        server.documents.get(URI).ok_or("missing document")?.text,
        source
    );
    server.versioned_edits = false;
    assert_eq!(
        server
            .navigate("textDocument/rename", &params(source, 2, "new"))
            .err()
            .ok_or("expected capability refusal")?
            .0,
        -32601
    );
    server.versioned_edits = true;
    server
        .documents
        .get_mut(URI)
        .ok_or("missing document")?
        .synchronized = false;
    assert_eq!(
        server
            .navigate("textDocument/rename", &params(source, 2, "new"))
            .err()
            .ok_or("expected stale refusal")?
            .0,
        -32801
    );
    // A full replacement can resynchronize the same URI and its current version.
    let _ = server.handle(notification(
        "textDocument/didChange",
        object([
            (
                "textDocument",
                object([("uri", string(URI)), ("version", Json::Number(8.0))]),
            ),
            (
                "contentChanges",
                Json::Array(vec![object([("text", string(source))])]),
            ),
        ]),
    ));
    let result = server
        .navigate("textDocument/rename", &params(source, 2, "new"))
        .map_err(|(_, reason)| reason)?;
    assert_eq!(
        array(result.get("documentChanges").ok_or("missing changes")?)[0]
            .get("textDocument")
            .and_then(|doc| doc.get("version"))
            .and_then(Json::as_f64),
        Some(8.0)
    );
    Ok(())
}
