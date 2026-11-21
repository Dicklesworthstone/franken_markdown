#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::super::{Phase, Position, Server, position};
use super::super::{Parser, request_in_workspace};
use super::*;

const ORIGIN: &str = "file:///docs/start.md";

fn buffers(uris: &[&str]) -> BTreeMap<String, Buffer> {
    uris.iter()
        .map(|uri| {
            (
                (*uri).to_string(),
                Buffer {
                    text: "# Chapter\n".into(),
                    version: 1,
                    synchronized: true,
                },
            )
        })
        .collect()
}

fn call_in(uri: &str, marked: &str, documents: &BTreeMap<String, Buffer>) -> (String, Json) {
    let offset = marked.find('¦').expect("caret marker");
    let source = marked.replace('¦', "");
    let params = object([
        ("textDocument", object([("uri", string(uri))])),
        (
            "position",
            position(LineIndex::new(&source).position(&source, offset)),
        ),
    ]);
    let result = request_in_workspace(Parser::default(), "textDocument/completion", &params, uri, &source, documents)
        .expect("completion request");
    (source, result)
}

fn items(result: &Json) -> &[Json] {
    match result.get("items") {
        Some(Json::Array(items)) => items,
        _ => panic!("expected a completion list"),
    }
}

fn labels(result: &Json) -> Vec<&str> {
    items(result)
        .iter()
        .map(|item| item.get("label").and_then(Json::as_str).unwrap())
        .collect()
}

fn apply(source: &str, item: &Json) -> String {
    let edit = item.get("textEdit").unwrap();
    let range = edit.get("range").unwrap();
    let index = LineIndex::new(source);
    let start = index
        .offset(
            source,
            Position::parse(range.get("start").unwrap()).unwrap(),
        )
        .unwrap();
    let end = index
        .offset(source, Position::parse(range.get("end").unwrap()).unwrap())
        .unwrap();
    let mut updated = source.to_string();
    updated.replace_range(
        start..end,
        edit.get("newText").and_then(Json::as_str).unwrap(),
    );
    updated
}

#[test]
fn opened_files_complete_empty_closed_and_unfinished_destinations() {
    let docs = buffers(&["file:///docs/chapter.md"]);
    for (marked, expected) in [
        ("[next](¦)", "[next](chapter.md)"),
        ("[next](¦", "[next](chapter.md"),
        ("[next](cha¦)", "[next](chapter.md)"),
        ("[next](cha¦", "[next](chapter.md"),
        ("[next](<cha¦>)", "[next](<chapter.md>)"),
        ("[next](<cha¦", "[next](<chapter.md"),
    ] {
        let (source, result) = call_in(ORIGIN, marked, &docs);
        assert_eq!(labels(&result), ["chapter.md"], "{marked}");
        assert_eq!(apply(&source, &items(&result)[0]), expected);
        assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(false)));
    }
}

#[test]
fn unicode_edits_preserve_query_fragment_delimiters_title_and_surrounding_source() {
    let docs = buffers(&["file:///manual/Other%20chapter%3F%23.md"]);
    let (source, result) = call_in(
        ORIGIN,
        "# Start\r\n\r\n😀 [next](<../manual/Other%20ch¦apter-old.md?view=1#part> \"Keep me\") tail\r\n",
        &docs,
    );
    assert_eq!(labels(&result), ["Other chapter?#.md"]);
    assert_eq!(
        apply(&source, &items(&result)[0]),
        "# Start\r\n\r\n😀 [next](<../manual/Other%20chapter%3F%23.md?view=1#part> \"Keep me\") tail\r\n"
    );
    assert_eq!(
        items(&result)[0].get("filterText").and_then(Json::as_str),
        Some("../manual/Other%20chapter%3F%23.md")
    );
}

#[test]
fn nested_parser_owned_links_receive_the_same_path_completion() {
    let docs = buffers(&["file:///docs/chapter.md"]);
    for marked in [
        "> [next](cha¦)\n",
        "- [next](cha¦)\n",
        "| heading |\n| --- |\n| [next](cha¦) |\n",
        "Term\n: [next](cha¦)\n",
        "Use[^n].\n\n[^n]: [next](cha¦)\n",
    ] {
        let (source, result) = call_in(ORIGIN, marked, &docs);
        assert_eq!(labels(&result), ["chapter.md"], "{marked}");
        assert!(apply(&source, &items(&result)[0]).contains("[next](chapter.md)"));
    }
}

#[test]
fn literal_images_references_and_hidden_notes_do_not_receive_file_edits() {
    let docs = buffers(&["file:///docs/chapter.md"]);
    for marked in [
        "`[next](cha¦)`",
        "```md\n[next](cha¦)\n```",
        "    [next](cha¦)\n",
        "![image](cha¦)",
        "bare cha¦",
        "<span title=\"[next](cha¦)\">text</span>",
        "[next](elsewhere.md \"title ](cha¦)\")",
        "[next][cha¦]\n\n[chapter]: /elsewhere\n",
        "[^unused]: [next](cha¦)\n",
    ] {
        let (_, result) = call_in(ORIGIN, marked, &docs);
        assert!(items(&result).is_empty(), "{marked}");
    }
}

#[test]
fn directories_come_only_from_open_buffers_and_are_deduplicated() {
    let docs = buffers(&[
        "file:///docs/chapters/first.md",
        "file:///docs/chapters/second.md",
        "file:///docs/chapter.md",
        "file:///docs/closed-sibling.md",
    ]);
    let (_, result) = call_in(ORIGIN, "[next](cha¦)", &docs);
    assert_eq!(labels(&result), ["chapter.md", "chapters/"]);
    assert_eq!(items(&result)[0].get("kind"), Some(&number(17)));
    assert_eq!(items(&result)[1].get("kind"), Some(&number(19)));
    let (source, result) = call_in(ORIGIN, "[next](./chapters/¦)", &docs);
    assert_eq!(labels(&result), ["first.md", "second.md"]);
    assert_eq!(
        apply(&source, &items(&result)[0]),
        "[next](./chapters/first.md)"
    );
    let (_, result) = call_in(ORIGIN, "[next](unknown/¦)", &docs);
    assert!(items(&result).is_empty());
}

#[test]
fn open_aliases_unsynchronized_targets_and_conflicting_paths_are_not_guessed() {
    let mut docs = buffers(&[
        "file:///docs/chapter.md",
        "file://localhost/docs/chapter.md",
        "file:///docs/chapters",
        "file:///docs/chapters/child.md",
        "file:///docs/change.md",
    ]);
    docs.get_mut("file:///docs/change.md").unwrap().synchronized = false;
    assert!(items(&call_in(ORIGIN, "[next](cha¦)", &docs).1).is_empty());
    docs.remove("file://localhost/docs/chapter.md");
    assert_eq!(
        labels(&call_in(ORIGIN, "[next](cha¦)", &docs).1),
        ["chapter.md"]
    );
    docs.remove("file:///docs/chapter.md");
    assert!(items(&call_in(ORIGIN, "[next](cha¦)", &docs).1).is_empty());
    docs.get_mut("file:///docs/change.md").unwrap().synchronized = true;
    assert_eq!(
        labels(&call_in(ORIGIN, "[next](cha¦)", &docs).1),
        ["change.md"]
    );
}

#[test]
fn parent_root_and_absolute_paths_use_navigation_identity_rules() {
    let docs = buffers(&["file:///manual/chapter.md"]);
    for marked in [
        "[next](../manual/cha¦)",
        "[next](/manual/cha¦)",
        "[next](file:///manual/cha¦)",
        "[next](file://localhost/manual/cha¦)",
    ] {
        let (source, result) = call_in(ORIGIN, marked, &docs);
        assert_eq!(labels(&result), ["chapter.md"]);
        let updated = apply(&source, &items(&result)[0]);
        assert_eq!(updated, source.replace("cha)", "chapter.md)"));
    }
    let docs = buffers(&[
        "file:///C:/manual/chapter.md",
        "file:///D:/manual/change.md",
    ]);
    let (_, result) = call_in("file:///c:/docs/start.md", "[next](/manual/cha¦)", &docs);
    assert_eq!(labels(&result), ["chapter.md"]);
    let docs = buffers(&[
        "file://HOST/share/chapter.md",
        "file://other/share/change.md",
    ]);
    let (_, result) = call_in("file://host/share/start.md", "[next](cha¦)", &docs);
    assert_eq!(labels(&result), ["chapter.md"]);
}

#[test]
fn percent_decoding_is_single_pass_and_encoded_separators_are_rejected() {
    let docs = buffers(&[
        "file:///docs/café.md",
        "file:///docs/literal%252F.md",
        "file:///docs/a%28b%29%5Bc%5D.md",
    ]);
    for (marked, expected) in [
        ("[next](caf%C3%A9¦)", "[next](caf%C3%A9.md)"),
        ("[next](literal%252¦)", "[next](literal%252F.md)"),
        ("[next](a%28¦)", "[next](a%28b%29%5Bc%5D.md)"),
    ] {
        let (source, result) = call_in(ORIGIN, marked, &docs);
        assert_eq!(items(&result).len(), 1, "{marked}");
        assert_eq!(apply(&source, &items(&result)[0]), expected);
    }
    for marked in ["[next](literal%2F¦)", "[next](a%5C¦)", "[next](caf%00¦)"] {
        assert!(
            items(&call_in(ORIGIN, marked, &docs).1).is_empty(),
            "{marked}"
        );
    }
    let (_, result) = call_in(ORIGIN, "[next](caf%¦)", &docs);
    assert!(items(&result).is_empty());
    assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(true)));
}

#[test]
fn queries_external_urls_and_existing_child_suffixes_are_not_rewritten() {
    let docs = buffers(&["file:///docs/chapter.md", "file:///docs/chapters/index.md"]);
    for marked in [
        "[next](chapter.md?q=cha¦)",
        "[next](https://example.invalid/cha¦)",
        "[next](//host/docs/cha¦)",
        "[next](../../cha¦)",
        "[next](cha¦pters/index.md)",
    ] {
        assert!(
            items(&call_in(ORIGIN, marked, &docs).1).is_empty(),
            "{marked}"
        );
    }
    // Existing fragment completion still owns the position after '#'.
    let (_, result) = call_in(ORIGIN, "[next](chapter.md#cha¦)", &docs);
    assert_eq!(labels(&result), ["#chapter"]);
}

#[test]
fn markdown_entities_cannot_authorize_an_edit_for_a_different_file_identity() {
    let docs = buffers(&[
        "file:///docs/A%26B/chapter.md",
        "file:///docs/A%26amp%3BB/chapter.md",
        "file:///docs/chapter.md",
    ]);
    for marked in [
        "[next](A&amp;B/cha¦)",
        "[next](A&amp;B/cha¦",
        "[next](cha¦&#35;part)",
        "[next](cha¦&#x3F;view=1)",
        "[next](cha¦&amp;name.md)",
    ] {
        assert!(
            items(&call_in(ORIGIN, marked, &docs).1).is_empty(),
            "{marked}"
        );
    }
    let (source, result) = call_in(ORIGIN, "[next](A%26B/cha¦)", &docs);
    assert_eq!(labels(&result), ["chapter.md"]);
    assert_eq!(
        apply(&source, &items(&result)[0]),
        "[next](A%26B/chapter.md)"
    );
    // Entities in a later query do not affect the path's source boundaries.
    let (source, result) = call_in(ORIGIN, "[next](cha¦?a=1&amp;b=2)", &docs);
    assert_eq!(labels(&result), ["chapter.md"]);
    assert_eq!(
        apply(&source, &items(&result)[0]),
        "[next](chapter.md?a=1&amp;b=2)"
    );
}

#[test]
fn opaque_buffers_need_an_explicit_absolute_file_destination() {
    let docs = buffers(&["file:///docs/chapter.md"]);
    assert!(items(&call_in("untitled:one", "[next](cha¦)", &docs).1).is_empty());
    assert_eq!(
        labels(&call_in("untitled:one", "[next](file:///docs/cha¦)", &docs).1),
        ["chapter.md"]
    );
}

#[test]
fn expanded_encoded_destinations_remain_inside_the_output_budget() {
    let name = format!("a{}.md", "é".repeat(2000));
    let uri = format!("file:///docs/{name}");
    let docs = buffers(&[&uri]);
    let (_, result) = call_in(ORIGIN, "[next](a¦)", &docs);
    assert!(items(&result).is_empty());
    assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(true)));
}

#[test]
fn server_dispatch_requires_current_source_and_keeps_completion_queries_read_only() {
    let mut server = Server {
        phase: Phase::Running,
        documents: buffers(&[ORIGIN, "file:///docs/chapter.md"]),
        ..Server::default()
    };
    server.documents.get_mut(ORIGIN).unwrap().text = "[next](cha)".into();
    let params = object([
        ("textDocument", object([("uri", string(ORIGIN))])),
        (
            "position",
            position(Position {
                line: 0,
                character: 10,
            }),
        ),
    ]);
    let result = server.navigate("textDocument/completion", &params).unwrap();
    assert_eq!(labels(&result), ["chapter.md"]);
    assert_eq!(server.documents[ORIGIN].text, "[next](cha)");
    assert_eq!(server.documents[ORIGIN].version, 1);
    server.documents.get_mut(ORIGIN).unwrap().synchronized = false;
    assert!(matches!(
        server.navigate("textDocument/completion", &params),
        Err((-32801, _))
    ));
}
