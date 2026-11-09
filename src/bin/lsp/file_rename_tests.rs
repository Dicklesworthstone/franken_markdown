use super::super::{Phase, Position, Server, integer, number};
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const ROOT: &str = "file:///docs/README.md";

fn buffers(entries: &[(&str, &str)]) -> BTreeMap<String, Buffer> {
    entries
        .iter()
        .enumerate()
        .map(|(index, (uri, source))| {
            (
                (*uri).to_string(),
                Buffer {
                    text: (*source).to_string(),
                    version: index as i32 + 11,
                    synchronized: true,
                },
            )
        })
        .collect()
}

fn params(moves: &[(&str, &str)]) -> Json {
    object([(
        "files",
        Json::Array(
            moves
                .iter()
                .map(|(old, new)| object([("oldUri", string(old)), ("newUri", string(new))]))
                .collect(),
        ),
    )])
}

fn array(value: &Json) -> Result<&[Json], &'static str> {
    match value {
        Json::Array(items) => Ok(items),
        _ => Err("expected array"),
    }
}

fn apply_changes(
    documents: &BTreeMap<String, Buffer>,
    result: &Json,
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let mut amended: BTreeMap<_, _> = documents
        .iter()
        .map(|(uri, buffer)| (uri.clone(), buffer.text.clone()))
        .collect();
    if result == &Json::Null {
        return Ok(amended);
    }
    for change in array(
        result
            .get("documentChanges")
            .ok_or("missing documentChanges")?,
    )? {
        assert!(
            change.get("kind").is_none(),
            "the server must not issue file operations"
        );
        let document = change.get("textDocument").ok_or("missing textDocument")?;
        let uri = document
            .get("uri")
            .and_then(Json::as_str)
            .ok_or("missing old URI")?;
        let original = documents
            .get(uri)
            .ok_or("edits must address an originally opened URI")?;
        assert_eq!(
            document.get("version").and_then(integer),
            Some(original.version)
        );
        let index = LineIndex::new(&original.text);
        let mut previous = 0;
        let mut edits = Vec::new();
        for edit in array(change.get("edits").ok_or("missing edits")?)? {
            let range = edit.get("range").ok_or("missing range")?;
            let start = index.offset(
                &original.text,
                Position::parse(range.get("start").ok_or("missing start")?)?,
            )?;
            let end = index.offset(
                &original.text,
                Position::parse(range.get("end").ok_or("missing end")?)?,
            )?;
            assert!(start >= previous && end >= start);
            previous = end;
            edits.push((
                start..end,
                edit.get("newText")
                    .and_then(Json::as_str)
                    .ok_or("missing text")?,
            ));
        }
        let output = amended.get_mut(uri).ok_or("missing old buffer")?;
        for (range, text) in edits.into_iter().rev() {
            output.replace_range(range, text);
        }
    }
    Ok(amended)
}

fn changed(
    documents: &BTreeMap<String, Buffer>,
    moves: &[(&str, &str)],
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let result = request(documents, &params(moves)).map_err(|(_, reason)| reason)?;
    apply_changes(documents, &result)
}

#[test]
fn file_move_updates_inline_images_and_every_reference_definition_without_changing_titles()
-> TestResult {
    let source = "😀 [inline](<chapter.md?view=1#intro> \"Keep\") ![chart](chapter.md?raw=1) [full][guide] [collapsed][] [shortcut]\r\n\r\n[guide]: chapter.md#intro \"Title\"\r\n\r\n[collapsed]: chapter.md\r\n\r\n[shortcut]: chapter.md\r\n\r\n[unused]: chapter.md\r\n\r\n[guide]: chapter.md#shadow\r\n";
    let documents = buffers(&[(ROOT, source), ("file:///docs/chapter.md", "# Intro\n")]);
    let output = changed(
        &documents,
        &[("file:///docs/chapter.md", "file:///docs/chapter-new.md")],
    )?;
    assert_eq!(output[ROOT], source.replace("chapter.md", "chapter-new.md"));
    assert_eq!(
        documents[ROOT].text, source,
        "requests cannot mutate live buffers"
    );
    assert_eq!(output["file:///docs/chapter.md"], "# Intro\n");
    assert!(!output.contains_key("file:///docs/chapter-new.md"));
    Ok(())
}

#[test]
fn moving_a_source_rebases_unopened_assets_and_links_while_preserving_self_fragments() -> TestResult
{
    let origin = "file:///docs/ch/start.md";
    let source = "[up](../shared.md?q=1#part) ![pixel](../img/a.png) [sibling](sibling.md) [self](start.md#self) [anchor](#self) [query](?view=1#self) [dir](./) [root](/docs/shared.md) [web](https://example.test/a.md)\n";
    let documents = buffers(&[(origin, source)]);
    let output = changed(&documents, &[(origin, "file:///manual/start.md")])?;
    assert_eq!(
        output[origin],
        "[up](../docs/shared.md?q=1#part) ![pixel](../docs/img/a.png) [sibling](../docs/ch/sibling.md) [self](start.md#self) [anchor](#self) [query](?view=1#self) [dir](../docs/ch/) [root](/docs/shared.md) [web](https://example.test/a.md)\n"
    );
    Ok(())
}

#[test]
fn folder_move_uses_one_parent_event_and_keeps_relative_subtree_destinations() -> TestResult {
    let root = "[one](chapters/one.md#one) ![plot](chapters/assets/plot.png)\n";
    let chapter = "[two](two.md) ![plot](assets/plot.png) [outside](../cover.md)\n";
    let documents = buffers(&[(ROOT, root), ("file:///docs/chapters/one.md", chapter)]);
    let output = changed(&documents, &[("file:///docs/chapters", "file:///handbook")])?;
    assert_eq!(
        output[ROOT],
        "[one](../handbook/one.md#one) ![plot](../handbook/assets/plot.png)\n"
    );
    assert_eq!(
        output["file:///docs/chapters/one.md"],
        "[two](two.md) ![plot](assets/plot.png) [outside](../docs/cover.md)\n"
    );
    Ok(())
}

#[test]
fn simultaneous_swaps_and_chains_do_not_reapply_a_move_or_depend_on_request_order() -> TestResult {
    let a = "file:///docs/A.md";
    let b = "file:///docs/B.md";
    let c = "file:///docs/C.md";
    let documents = buffers(&[
        (ROOT, "[A](A.md) [B](B.md)\n"),
        (a, "[B](B.md)\n"),
        (b, "[A](A.md)\n"),
    ]);
    let swapped = changed(&documents, &[(a, b), (b, a)])?;
    assert_eq!(swapped[ROOT], "[A](B.md) [B](A.md)\n");
    assert_eq!(swapped[a], "[B](A.md)\n");
    assert_eq!(swapped[b], "[A](B.md)\n");
    assert_eq!(
        request(&documents, &params(&[(a, b), (b, a)])),
        request(&documents, &params(&[(b, a), (a, b)]))
    );
    let chained = changed(&documents, &[(a, b), (b, c)])?;
    assert_eq!(chained[ROOT], "[A](B.md) [B](C.md)\n");
    assert_eq!(chained[a], "[B](C.md)\n");
    assert_eq!(chained[b], "[A](B.md)\n");
    Ok(())
}

#[test]
fn unique_open_extension_and_directory_aliases_follow_their_actual_chapters() -> TestResult {
    let documents = buffers(&[
        (ROOT, "[guide](guide?view=1#intro) [index](topic/#part)\n"),
        ("file:///docs/guide.md", "# Intro\n"),
        ("file:///docs/topic/index.md", "# Part\n"),
    ]);
    let output = changed(
        &documents,
        &[
            ("file:///docs/guide.md", "file:///docs/manual.md"),
            ("file:///docs/topic/index.md", "file:///docs/new/index.md"),
        ],
    )?;
    assert_eq!(
        output[ROOT],
        "[guide](manual.md?view=1#intro) [index](new/index.md#part)\n"
    );
    Ok(())
}

#[test]
fn extensionless_images_keep_resource_identity_when_a_same_named_chapter_exists() -> TestResult {
    let documents = buffers(&[
        (ROOT, "[chapter](guide) ![asset](guide)\n"),
        ("file:///docs/guide.md", "# Guide\n"),
    ]);
    let output = changed(
        &documents,
        &[("file:///docs/guide.md", "file:///docs/manual.md")],
    )?;
    assert_eq!(output[ROOT], "[chapter](manual.md) ![asset](guide)\n");
    let output = changed(&documents, &[("file:///docs/guide", "file:///docs/asset")])?;
    assert_eq!(output[ROOT], "[chapter](guide) ![asset](asset)\n");
    Ok(())
}

#[test]
fn shared_and_unused_definitions_require_compatible_link_and_resource_edits() -> TestResult {
    let moves = [("file:///docs/guide.md", "file:///docs/manual.md")];
    for source in [
        "[chapter][shared] ![asset][shared]\n\n[shared]: guide\n",
        "[unused]: guide\n",
    ] {
        let documents = buffers(&[(ROOT, source), ("file:///docs/guide.md", "# Guide\n")]);
        assert!(
            request(&documents, &params(&moves))
                .err()
                .is_some_and(|error| error.1.contains("incompatible")),
            "{source}"
        );
    }
    let source = "[chapter][shared] ![asset][shared]\n\n[shared]: guide.md\n";
    let documents = buffers(&[(ROOT, source), ("file:///docs/guide.md", "# Guide\n")]);
    assert_eq!(
        changed(&documents, &moves)?[ROOT],
        source.replace("guide.md", "manual.md")
    );
    Ok(())
}

#[test]
fn new_alias_collisions_are_repaired_to_the_previous_explicit_target() -> TestResult {
    for (source, target, new, expected) in [
        (
            "[go](topic/#part)\n",
            "file:///docs/topic/index.md",
            "file:///docs/topic/README.md",
            "[go](topic/index.md#part)\n",
        ),
        (
            "[go](topic#part)\n",
            "file:///docs/topic.md",
            "file:///docs/topic",
            "[go](topic.md#part)\n",
        ),
    ] {
        for opened in [false, true] {
            let mut documents = buffers(&[(ROOT, source), (target, "# Part\n")]);
            if opened {
                documents.extend(buffers(&[("file:///docs/spare.md", "# Spare\n")]));
            }
            let output = changed(&documents, &[("file:///docs/spare.md", new)])?;
            assert_eq!(output[ROOT], expected, "renamed source open: {opened}");
        }
    }
    Ok(())
}

#[test]
fn closed_folder_endpoints_cannot_capture_aliases_or_overwrite_known_subtrees() -> TestResult {
    let documents = buffers(&[
        (ROOT, "[go](guide#part)\n"),
        ("file:///docs/guide.md", "# Part\n"),
    ]);
    let output = changed(
        &documents,
        &[("file:///closed/folder/", "file:///docs/guide/")],
    )?;
    assert_eq!(output[ROOT], "[go](guide.md#part)\n");

    let documents = buffers(&[
        (ROOT, "[go](topic/#part)\n"),
        ("file:///docs/topic/index.md", "# Part\n"),
    ]);
    assert!(
        request(
            &documents,
            &params(&[("file:///closed/folder/", "file:///docs/topic/")]),
        )
        .err()
        .is_some_and(|error| error.1.contains("subtree"))
    );
    Ok(())
}

#[test]
fn unopened_exact_targets_are_supported_but_unproven_aliases_are_refused() -> TestResult {
    let moves = [("file:///docs/guide.md", "file:///docs/manual.md")];
    let documents = buffers(&[(ROOT, "[go](guide.md#intro)\n")]);
    assert_eq!(
        changed(&documents, &moves)?[ROOT],
        "[go](manual.md#intro)\n"
    );
    let documents = buffers(&[(ROOT, "[go](guide#intro)\n")]);
    let error = request(&documents, &params(&moves))
        .err()
        .ok_or("an unopened alias must not be guessed")?;
    assert!(error.1.contains("target opened"));
    let documents = buffers(&[
        (ROOT, "[go](guide#intro)\n"),
        ("file:///docs/guide.md", "# Intro\n"),
        ("file:///docs/guide/index.md", "# Intro\n"),
    ]);
    assert!(
        request(&documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("ambiguous"))
    );
    Ok(())
}

#[test]
fn cross_drive_and_authority_moves_use_absolute_file_uris() -> TestResult {
    let origin = "file:///C:/docs/start.md";
    let observer = "file:///C:/overview.md";
    let documents = buffers(&[
        (
            origin,
            "![art](art.png) ![root](/images/a.png) ![explicit](/C:/images/a.png) ![other](/D:/images/a.png) ![encoded](/D%3A/images/a.png) ![letter](/%43:/images/a.png)\n",
        ),
        (observer, "[go](docs/start.md)\n"),
    ]);
    let output = changed(&documents, &[(origin, "file:///D:/manual/start.md")])?;
    assert_eq!(
        output[origin],
        "![art](file:///C:/docs/art.png) ![root](/C:/images/a.png) ![explicit](/C:/images/a.png) ![other](/D:/images/a.png) ![encoded](/C:/D%3A/images/a.png) ![letter](/C:/C%3A/images/a.png)\n"
    );
    assert_eq!(output[observer], "[go](file:///D:/manual/start.md)\n");
    let origin = "file://host/share/start.md";
    let documents = buffers(&[(origin, "[sibling](other.md) [root](/share/other.md)\n")]);
    let output = changed(&documents, &[(origin, "file://other/books/start.md")])?;
    assert_eq!(
        output[origin],
        "[sibling](file://host/share/other.md) [root](file://host/share/other.md)\n"
    );
    Ok(())
}

#[test]
fn unicode_percent_encoding_and_entity_queries_preserve_exact_decoded_destinations() -> TestResult {
    let source = "\u{feff}😀 [go](<chap&#116;er.md?view=1&amp;x=2&#35;caf%C3%A9> \"Keep\")\r\n";
    let documents = buffers(&[(ROOT, source)]);
    let result = request(
        &documents,
        &params(&[(
            "file:///docs/chapter.md",
            "file:///docs/new%20dir/é%20(draft).md",
        )]),
    )
    .map_err(|(_, reason)| reason)?;
    let output = apply_changes(&documents, &result)?;
    assert_eq!(
        output[ROOT],
        "\u{feff}😀 [go](<new%20dir/%C3%A9%20%28draft%29.md?view=1\\&x=2#caf%C3%A9> \"Keep\")\r\n"
    );
    let changes = array(result.get("documentChanges").ok_or("missing changes")?)?;
    let edits = array(changes[0].get("edits").ok_or("missing edits")?)?;
    let start = edits[0]
        .get("range")
        .and_then(|range| range.get("start"))
        .ok_or("missing start")?;
    assert_eq!(start.get("line").and_then(Json::as_u64), Some(0));
    assert_eq!(start.get("character").and_then(Json::as_u64), Some(9));
    let tokens = destination_spans(&output[ROOT]);
    assert_eq!(tokens.len(), 1);
    assert_eq!(
        tokens[0].dest,
        "new%20dir/%C3%A9%20%28draft%29.md?view=1&x=2#caf%C3%A9"
    );
    Ok(())
}

#[test]
fn code_escaped_markers_math_html_and_external_urls_remain_literal() -> TestResult {
    let source = "[real](old.md)\n\n`[code](old.md)`\n\n```markdown\n[fence](old.md)\n{{#include old.md}}\n```\n\n\\[escaped](old.md)\n\n$[math](old.md)$\n\n<a href=\"old.md\">HTML</a>\n\n[remote](https://example.test/old.md) [network](//example.test/old.md) [email](mailto:old.md)\n";
    let documents = buffers(&[(ROOT, source)]);
    let output = changed(
        &documents,
        &[("file:///docs/old.md", "file:///docs/new.md")],
    )?;
    assert_eq!(
        output[ROOT],
        source.replacen("[real](old.md)", "[real](new.md)", 1)
    );
    Ok(())
}

#[test]
fn nested_containers_and_dormant_note_destinations_use_real_parser_ownership() -> TestResult {
    let source = "> [quote](old.md)\n\n- ![alt](old.md)\n\n| Header |\n| --- |\n| [cell](old.md) |\n\nTerm\n: [definition](old.md)\n\n[^unused]: [dormant](old.md)\n";
    let documents = buffers(&[(ROOT, source)]);
    let output = changed(
        &documents,
        &[("file:///docs/old.md", "file:///docs/new.md")],
    )?;
    assert_eq!(output[ROOT], source.replace("old.md", "new.md"));
    Ok(())
}

#[test]
fn affected_unmapped_autolinks_refuse_the_entire_edit_instead_of_returning_a_partial_fix()
-> TestResult {
    let source = "[mapped](old.md)\n\n<file:///docs/old.md>\n";
    let documents = buffers(&[(ROOT, source)]);
    let error = request(
        &documents,
        &params(&[("file:///docs/old.md", "file:///docs/new.md")]),
    )
    .err()
    .ok_or("an unmapped affected semantic link must refuse the edit")?;
    assert_eq!(error.0, -32803);
    assert!(error.1.contains("preserve every affected destination"));
    assert_eq!(documents[ROOT].text, source);
    Ok(())
}

#[test]
fn active_and_malformed_includes_are_refused_but_code_examples_are_safe() -> TestResult {
    let moves = [("file:///docs/old.md", "file:///docs/new.md")];
    for source in [
        "{{#include old.md}}\n",
        "{{#include \"old.md\":3:8}}\n",
        "{{#include}}\n",
    ] {
        let documents = buffers(&[(ROOT, source)]);
        assert!(
            request(&documents, &params(&moves))
                .err()
                .is_some_and(|error| error.1.contains("transclusion")),
            "{source}"
        );
    }
    let source = "`{{#include old.md}}`\n\n[go](old.md)\n";
    let documents = buffers(&[(ROOT, source)]);
    assert_eq!(
        changed(&documents, &moves)?[ROOT],
        "`{{#include old.md}}`\n\n[go](new.md)\n"
    );
    Ok(())
}

#[test]
fn stale_documents_aliases_and_known_resource_collisions_are_transactional_refusals() -> TestResult
{
    let moves = [("file:///docs/old.md", "file:///docs/new.md")];
    let mut documents = buffers(&[(ROOT, "[go](old.md)\n"), ("untitled:stale", "draft")]);
    documents
        .get_mut("untitled:stale")
        .ok_or("missing draft")?
        .synchronized = false;
    assert_eq!(
        request(&documents, &params(&moves))
            .err()
            .map(|error| error.0),
        Some(-32801)
    );
    let documents = buffers(&[
        (ROOT, "[go](old.md)\n"),
        ("file://localhost/docs/README.md", "other revision"),
    ]);
    assert!(
        request(&documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("aliases"))
    );
    let documents = buffers(&[
        (ROOT, "[go](old.md)\n"),
        ("file:///docs/new.md", "# Existing\n"),
    ]);
    assert!(
        request(&documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("collides"))
    );
    let documents = buffers(&[(ROOT, "[old](old.md) ![existing](new.md)\n")]);
    assert!(
        request(&documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("merge distinct"))
    );
    Ok(())
}

#[test]
fn request_validation_rejects_roots_overlaps_duplicate_identities_and_invalid_uris() -> TestResult {
    let documents = buffers(&[(ROOT, "[go](old.md)\n")]);
    for moves in [
        vec![("file:///", "file:///moved")],
        vec![("file:///C:/", "file:///D:/moved")],
        vec![("https://example.test/old.md", "file:///new.md")],
        vec![("file:///docs/old.md?x=1", "file:///new.md")],
        vec![("file:///docs/%2F.md", "file:///new.md")],
        vec![("file:///docs/old.md", "file:///docs/old.md/child.md")],
        vec![
            ("file:///docs", "file:///new"),
            ("file:///docs/old.md", "file:///other.md"),
        ],
        vec![
            ("file:///docs/old.md", "file:///new.md"),
            ("file://localhost/docs/old.md", "file:///other.md"),
        ],
        vec![
            ("file:///docs/old.md", "file:///new.md"),
            ("file:///docs/other.md", "file://localhost/new.md"),
        ],
    ] {
        assert!(request(&documents, &params(&moves)).is_err(), "{moves:?}");
    }
    assert_eq!(
        request(&documents, &object([])).err().map(|error| error.0),
        Some(-32602)
    );
    assert_eq!(
        request(&documents, &params(&[])).map_err(|(_, reason)| reason)?,
        Json::Null
    );
    assert_eq!(
        request(
            &documents,
            &params(&[("file:///docs/old.md", "file://localhost/docs/old.md")])
        )
        .map_err(|(_, reason)| reason)?,
        Json::Null
    );
    Ok(())
}

#[test]
fn operation_document_and_response_budgets_refuse_incomplete_changes() -> TestResult {
    let documents = buffers(&[(ROOT, "[go](old.md)\n")]);
    let files = Json::Array(
        (0..129)
            .map(|index| {
                object([
                    ("oldUri", string(&format!("file:///old/{index}.md"))),
                    ("newUri", string(&format!("file:///new/{index}.md"))),
                ])
            })
            .collect(),
    );
    assert!(
        request(&documents, &object([("files", files)]))
            .err()
            .is_some_and(|error| error.1.contains("128-operation"))
    );
    let oversized = "x".repeat(MAX_DOCUMENT_BYTES + 1);
    let documents = buffers(&[(ROOT, &oversized)]);
    assert!(
        request(
            &documents,
            &params(&[("file:///docs/old.md", "file:///docs/new.md")])
        )
        .err()
        .is_some_and(|error| error.1.contains("source budget"))
    );
    let source = "[go](old.md) ".repeat(MAX_EDITS + 1);
    let documents = buffers(&[(ROOT, &source)]);
    assert!(
        request(
            &documents,
            &params(&[("file:///docs/old.md", "file:///docs/new.md")])
        )
        .err()
        .is_some_and(|error| error.1.contains("edit response budget"))
    );
    Ok(())
}

#[test]
fn opaque_buffers_allow_explicit_file_links_but_never_infer_a_relative_directory() -> TestResult {
    let origin = "untitled:draft";
    let documents = buffers(&[(
        origin,
        "[absolute](file:///docs/old.md#part) [relative](old.md#part)\n",
    )]);
    let output = changed(
        &documents,
        &[("file:///docs/old.md", "file:///docs/new.md")],
    )?;
    assert_eq!(
        output[origin],
        "[absolute](file:///docs/new.md#part) [relative](old.md#part)\n"
    );
    Ok(())
}

#[test]
fn initialize_negotiates_versioned_file_operation_edits_and_dispatch_keeps_buffers_unchanged()
-> TestResult {
    for (versions, operations, enabled) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (true, true, true),
    ] {
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
                        object([
                            (
                                "workspaceEdit",
                                object([("documentChanges", Json::Bool(versions))]),
                            ),
                            (
                                "fileOperations",
                                object([("willRename", Json::Bool(operations))]),
                            ),
                        ]),
                    )]),
                )]),
            ),
        ]);
        let replies = server.handle(initialize);
        assert_eq!(server.file_renames, enabled);
        assert!(server.phase == Phase::Running);
        let advertised = replies[0]
            .get("result")
            .and_then(|result| result.get("capabilities"))
            .and_then(|caps| caps.get("workspace"))
            .and_then(|workspace| workspace.get("fileOperations"))
            .and_then(|operations| operations.get("willRename"));
        assert_eq!(advertised.is_some(), enabled);
        server.documents = buffers(&[(ROOT, "[go](old.md)\n")]);
        let call = object([
            ("jsonrpc", string("2.0")),
            ("id", number(2)),
            ("method", string("workspace/willRenameFiles")),
            (
                "params",
                params(&[("file:///docs/old.md", "file:///docs/new.md")]),
            ),
        ]);
        let replies = server.handle(call);
        if enabled {
            assert_eq!(
                apply_changes(
                    &server.documents,
                    replies[0].get("result").ok_or("missing result")?
                )?[ROOT],
                "[go](new.md)\n"
            );
        } else {
            assert_eq!(
                replies[0]
                    .get("error")
                    .and_then(|error| error.get("code"))
                    .and_then(integer),
                Some(-32601)
            );
        }
        assert_eq!(server.documents[ROOT].text, "[go](old.md)\n");
    }
    Ok(())
}
