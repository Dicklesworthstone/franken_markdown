#![allow(clippy::unwrap_used, clippy::panic)]

use super::super::super::{parse_json, position, read_frame, run, write_frame};
use super::*;

const A: &str = "file:///docs/a.md";
const B: &str = "file:///docs/b.md";

fn documents(entries: &[(&str, &str)]) -> BTreeMap<String, Buffer> {
    entries
        .iter()
        .map(|(uri, text)| {
            (
                (*uri).to_string(),
                Buffer {
                    text: (*text).to_string(),
                    version: 1,
                    synchronized: true,
                },
            )
        })
        .collect()
}

fn params(source: &str, needle: &str, declaration: bool) -> Json {
    let offset = source.find(needle).unwrap() + needle.chars().next().unwrap().len_utf8();
    object([
        (
            "position",
            position(LineIndex::new(source).position(source, offset)),
        ),
        (
            "context",
            object([("includeDeclaration", Json::Bool(declaration))]),
        ),
    ])
}

fn array(value: Json) -> Vec<Json> {
    match value {
        Json::Array(values) => values,
        value => panic!("expected array, got {value:?}"),
    }
}

fn search(
    documents: &BTreeMap<String, Buffer>,
    uri: &str,
    needle: &str,
    declaration: bool,
) -> Vec<Json> {
    array(
        request(
            "textDocument/references",
            &params(&documents[uri].text, needle, declaration),
            uri,
            documents,
        )
        .unwrap(),
    )
}

fn starts(rows: &[Json]) -> Vec<(String, u64)> {
    rows.iter()
        .map(|row| {
            (
                row.get("uri").and_then(Json::as_str).unwrap().to_string(),
                row.get("range")
                    .and_then(|range| range.get("start"))
                    .and_then(|start| start.get("line"))
                    .and_then(Json::as_u64)
                    .unwrap(),
            )
        })
        .collect()
}

#[test]
fn references_deduplicate_source_blocks_and_honor_include_declaration() {
    let source = "# Topic\n\n[one](#topic) [two](#topic)\n\n> [nested](#topic)\n\n## Other\n\n[again](#topic)";
    let docs = documents(&[(A, source)]);
    let rows = search(&docs, A, "Topic", false);
    assert_eq!(starts(&rows), [(A.into(), 2), (A.into(), 4), (A.into(), 8)]);
    let rows = search(&docs, A, "Topic", true);
    assert_eq!(
        starts(&rows),
        [(A.into(), 0), (A.into(), 2), (A.into(), 4), (A.into(), 8)]
    );
    assert_eq!(
        search(&docs, A, "#topic", false),
        search(&docs, A, "Topic", false)
    );
}

#[test]
fn cross_buffer_references_do_not_conflate_equal_heading_ids() {
    let docs = documents(&[
        (A, "# Same\n\n[local](#same)"),
        (B, "# Same\n\n[local](#same)\n\n[other](a.md?s=1#s%61me)"),
    ]);
    let rows = search(&docs, B, "a.md?s=1#s%61me", true);
    assert_eq!(starts(&rows), [(A.into(), 0), (A.into(), 2), (B.into(), 4)]);
    assert_eq!(search(&docs, A, "Same", true), rows);
}

#[test]
fn file_uri_and_extensionless_links_use_the_existing_open_buffer_resolver() {
    let target = "file:///docs/guide.md";
    let docs = documents(&[
        (target, "# Chapter"),
        (
            A,
            "[a](guide#chapter)\n\n[b](./guide.md#chapter)\n\n[c](file:///docs/guide.md#chapter)",
        ),
    ]);
    assert_eq!(
        starts(&search(&docs, target, "Chapter", false)),
        [(A.into(), 0), (A.into(), 2), (A.into(), 4)]
    );
}

#[test]
fn ambiguous_open_uri_aliases_are_not_chosen_by_map_order() {
    let docs = documents(&[
        (A, "[a](guide.md#chapter)"),
        ("file:///docs/guide.md", "# Chapter"),
        ("file:/docs/guide.md", "# Chapter"),
    ]);
    assert!(search(&docs, A, "guide.md#chapter", true).is_empty());
    assert!(search(&docs, "file:///docs/guide.md", "Chapter", false).is_empty());
}

#[test]
fn nested_heading_targets_keep_distinct_ids_even_when_their_source_owner_is_shared() {
    let docs = documents(&[
        (A, "# Root\n\n> # First\n>\n> # Second"),
        (B, "[first](a.md#first)\n\n[second](a.md#second)"),
    ]);
    assert_eq!(
        starts(&search(&docs, B, "a.md#second", false)),
        [(B.into(), 2)]
    );
    assert_eq!(
        starts(&search(&docs, B, "a.md#first", false)),
        [(B.into(), 0)]
    );
    assert!(search(&docs, A, "Second", false).is_empty());
}

#[test]
fn duplicate_heading_suffixes_follow_publication_instead_of_title_matching() {
    let source = "# Same\n\n# Same";
    let snapshot = Snapshot::parse(source).unwrap();
    let first = snapshot
        .analysis
        .anchors
        .iter()
        .find(|a| a.block_index == 0)
        .unwrap();
    let second = snapshot
        .analysis
        .anchors
        .iter()
        .find(|a| a.block_index == 1)
        .unwrap();
    assert_ne!(first.id, second.id);
    let references = format!("[a](a.md#{})\n\n[b](a.md#{})", first.id, second.id);
    let docs = documents(&[(A, source), (B, &references)]);
    assert_eq!(
        starts(&search(&docs, B, &format!("a.md#{}", second.id), false)),
        [(B.into(), 2)]
    );
}

#[test]
fn reference_style_and_container_links_select_the_same_target() {
    let docs = documents(&[
        (A, "# Topic"),
        (
            B,
            "> [quoted][section]\n\n- [listed](a.md#topic)\n\n[section]: a.md#topic",
        ),
    ]);
    assert_eq!(
        starts(&search(&docs, B, "section", false)),
        [(B.into(), 0), (B.into(), 2)]
    );
    assert_eq!(
        search(&docs, B, "section", false),
        search(&docs, A, "Topic", false)
    );
}

#[test]
fn unicode_folded_reference_uses_share_the_parser_target_and_original_source_ranges() {
    let source = "> 😀 [quoted][ﬃ]\r\n\r\n- [listed][ſ]\r\n\r\n![image][ﬃ]\r\n\r\n[FFI]: a.md#topic\r\n[S]: a.md#topic\r\n[ﬃ]: a.md#wrong\r\n";
    let docs = documents(&[(A, "# Topic\n\n# Wrong"), (B, source)]);
    let rows = search(&docs, B, "ﬃ", false);
    assert_eq!(starts(&rows), [(B.into(), 0), (B.into(), 2)]);
    assert_eq!(rows, search(&docs, B, "ſ", false));
    assert_eq!(rows, search(&docs, A, "Topic", false));
    assert!(search(&docs, A, "Wrong", false).is_empty());
    let spans = Snapshot::parse(source).unwrap().spans;
    for (row, span) in rows.iter().zip(&spans) {
        assert_eq!(
            row.get("range"),
            Some(&range(&LineIndex::new(source), source, *span))
        );
    }
}

#[test]
fn images_code_external_urls_and_unpublished_notes_do_not_become_usages() {
    let source = "# Topic\n\n`[code](#topic)`\n\n![image](#topic)\n\n```md\n[fenced](#topic)\n```\n\n[remote](https://example.com/#topic)\n\n[^unused]: [hidden](#topic)\n\n[real](#topic)";
    let docs = documents(&[(A, source)]);
    let rows = search(&docs, A, "Topic", false);
    assert_eq!(rows.len(), 1);
    assert!(search(&docs, A, "#topic", false).is_empty());
    assert!(search(&docs, A, "https://example.com/#topic", false).is_empty());
}

#[test]
fn utf16_crlf_and_encoded_fragments_preserve_original_source_ranges() {
    let source = "# Café\r\n\r\n😀 [one](#caf%C3%A9)\r\n\r\n[another](#caf%C3%A9)";
    let docs = documents(&[(A, source)]);
    let rows = search(&docs, A, "#caf%C3%A9", false);
    assert_eq!(starts(&rows), [(A.into(), 2), (A.into(), 4)]);
    let spans = Snapshot::parse(source).unwrap().spans;
    assert_eq!(
        rows[0].get("range"),
        Some(&range(&LineIndex::new(source), source, spans[1]))
    );
    let invalid = object([
        (
            "position",
            position(Position {
                line: 2,
                character: 1,
            }),
        ),
        (
            "context",
            object([("includeDeclaration", Json::Bool(false))]),
        ),
    ]);
    assert_eq!(
        request("textDocument/references", &invalid, A, &docs)
            .unwrap_err()
            .0,
        -32602
    );
}

#[test]
fn stale_buffers_refuse_workspace_results_without_breaking_local_highlights() {
    let mut docs = documents(&[(A, "# Topic\n\n[one](#topic)"), (B, "unrelated")]);
    docs.get_mut(B).unwrap().synchronized = false;
    let query = params(&docs[A].text, "Topic", false);
    assert_eq!(
        request("textDocument/references", &query, A, &docs)
            .unwrap_err()
            .0,
        -32801
    );
    let rows = array(request("textDocument/documentHighlight", &query, A, &docs).unwrap());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("kind").and_then(Json::as_u64), Some(3));
    assert_eq!(rows[1].get("kind").and_then(Json::as_u64), Some(2));
}

#[test]
fn cross_buffer_highlights_return_only_current_document_usages() {
    let docs = documents(&[
        (A, "# Topic"),
        (B, "[one](a.md#topic)\n\n[two](a.md#topic)"),
    ]);
    let rows = array(
        request(
            "textDocument/documentHighlight",
            &params(&docs[B].text, "a.md#topic", true),
            B,
            &docs,
        )
        .unwrap(),
    );
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.get("uri").is_none()));
    assert!(
        rows.iter()
            .all(|row| row.get("kind").and_then(Json::as_u64) == Some(2))
    );
}

#[test]
fn malformed_context_and_unsynchronized_targets_return_protocol_errors() {
    let mut docs = documents(&[(A, "# Topic"), (B, "[one](a.md#topic)")]);
    let missing = object([(
        "position",
        position(Position {
            line: 0,
            character: 0,
        }),
    )]);
    assert_eq!(
        request("textDocument/references", &missing, A, &docs)
            .unwrap_err()
            .0,
        -32602
    );
    let invalid = object([
        (
            "position",
            position(Position {
                line: 0,
                character: 0,
            }),
        ),
        ("context", object([("includeDeclaration", string("true"))])),
    ]);
    assert_eq!(
        request("textDocument/references", &invalid, A, &docs)
            .unwrap_err()
            .0,
        -32602
    );
    docs.get_mut(A).unwrap().synchronized = false;
    let query = params(&docs[B].text, "a.md#topic", false);
    assert_eq!(
        request("textDocument/references", &query, B, &docs)
            .unwrap_err()
            .0,
        -32801
    );
}

#[test]
fn excessive_results_are_rejected_not_silently_truncated() {
    let source = format!("# Topic\n\n{}", "[one](#topic)\n\n".repeat(MAX_RESULTS + 1));
    let docs = documents(&[(A, &source)]);
    assert_eq!(
        request(
            "textDocument/references",
            &params(&source, "Topic", false),
            A,
            &docs
        )
        .unwrap_err()
        .0,
        -32803
    );
    let uri = "u".repeat(8192);
    let mut locations = Locations::default();
    for start in 0..128 {
        locations
            .add(&uri, SourceSpan::new(start, start + 1), false)
            .unwrap();
    }
    assert!(
        locations
            .add(&uri, SourceSpan::new(128, 129), false)
            .is_err()
    );
}

#[test]
fn synchronized_edits_and_closed_buffers_are_observed_on_the_next_request() {
    let mut docs = documents(&[(A, "# Topic"), (B, "[one](a.md#topic)")]);
    assert_eq!(search(&docs, A, "Topic", false).len(), 1);
    let changes = [object([("text", string("[one](a.md#other)"))])];
    docs.get_mut(B)
        .unwrap()
        .change(2, &changes, MAX_DOCUMENT_BYTES)
        .unwrap();
    assert!(search(&docs, A, "Topic", false).is_empty());
    docs.get_mut(B)
        .unwrap()
        .change(
            3,
            &[object([("text", string("[one](a.md#topic)"))])],
            MAX_DOCUMENT_BYTES,
        )
        .unwrap();
    assert_eq!(search(&docs, A, "Topic", false).len(), 1);
    docs.remove(B);
    assert!(search(&docs, A, "Topic", false).is_empty());
}

#[test]
fn framed_protocol_advertises_and_dispatches_both_providers() {
    let messages = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"untitled:one","languageId":"markdown","version":1,"text":"# Topic\n\n[one](#topic)"}}}"##,
        r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{"textDocument":{"uri":"untitled:one"},"position":{"line":0,"character":2},"context":{"includeDeclaration":false}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"textDocument/documentHighlight","params":{"textDocument":{"uri":"untitled:one"},"position":{"line":0,"character":2}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ];
    let mut input = Vec::new();
    for message in messages {
        parse_json(message).unwrap();
        write_frame(&mut input, message).unwrap();
    }
    let mut output = Vec::new();
    assert!(run(&mut std::io::Cursor::new(input), &mut output).unwrap());
    let mut reader = std::io::Cursor::new(output);
    let mut replies = BTreeMap::new();
    while let Some(body) = read_frame(&mut reader, 8 * 1024 * 1024).unwrap() {
        let reply = parse_json(&body).unwrap();
        if let Some(id) = reply.get("id").and_then(Json::as_u64) {
            replies.insert(id, reply);
        }
    }
    let capabilities = replies[&1]
        .get("result")
        .unwrap()
        .get("capabilities")
        .unwrap();
    assert_eq!(
        capabilities.get("referencesProvider"),
        Some(&Json::Bool(true))
    );
    assert_eq!(
        capabilities.get("documentHighlightProvider"),
        Some(&Json::Bool(true))
    );
    assert_eq!(array(replies[&2].get("result").unwrap().clone()).len(), 1);
    assert_eq!(array(replies[&3].get("result").unwrap().clone()).len(), 2);
}
