#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use super::super::{Json, LineIndex, Phase, Position, Server, number, object, position, string};

const ORIGIN: &str = "file:///manual/guide/start.md";
const TARGET: &str = "file:///manual/Other%20chapter.md";

fn buffers(entries: &[(&str, &str)]) -> BTreeMap<String, Buffer> {
    entries.iter().map(|(uri, text)| ((*uri).to_string(), Buffer {
        text: (*text).to_string(), version: 1, synchronized: true,
    })).collect()
}

fn opened(entries: &[(&str, &str)]) -> Server {
    Server { phase: Phase::Running, documents: buffers(entries), ..Server::default() }
}

fn at(server: &mut Server, marked: &str) -> Json {
    let offset = marked.find('¦').expect("caret marker");
    let source = marked.replace('¦', "");
    let requested = LineIndex::new(&source).position(&source, offset);
    server.documents.insert(ORIGIN.into(), Buffer {
        text: source, version: 1, synchronized: true,
    });
    object([
        ("textDocument", object([("uri", string(ORIGIN))])),
        ("position", position(requested)),
    ])
}

fn call(server: &mut Server, method: &str, marked: &str) -> Json {
    let params = at(server, marked);
    server.navigate(method, &params).expect("navigation")
}

fn labels(result: &Json) -> Vec<&str> {
    match result.get("items") {
        Some(Json::Array(items)) => items.iter()
            .filter_map(|item| item.get("label").and_then(Json::as_str)).collect(),
        _ => vec![],
    }
}

#[test]
fn logical_uri_identity_preserves_encoding_and_platform_boundaries() {
    for (origin, destination, target) in [
        (ORIGIN, "../Other%20chapter.md?q=1#part", TARGET),
        (ORIGIN, "/manual/Other%20chapter.md", TARGET),
        (ORIGIN, "file://localhost/manual/Other%20chapter.md#part", TARGET),
        ("file:///c:/manual/a.md", "b.md", "file:///C:/manual/b.md"),
        ("file://HOST/share/a.md", "b.md", "file://host/share/b.md"),
        (ORIGIN, "caf%C3%A9.md", "file:///manual/guide/café.md"),
        (ORIGIN, "literal%252F.md", "file:///manual/guide/literal%252F.md"),
        (ORIGIN, "what%3Fever%23x.md#part", "file:///manual/guide/what%3Fever%23x.md"),
    ] {
        let docs = buffers(&[(target, "# Part")]);
        assert_eq!(resolve(&docs, origin, destination).unwrap(), Some((target, "# Part")));
    }
}

#[test]
fn directory_and_extensionless_links_require_a_unique_open_candidate() {
    let mut docs = buffers(&[("file:///manual/guide/topic.md", "md")]);
    assert_eq!(resolve(&docs, ORIGIN, "topic#x").unwrap().unwrap().1, "md");
    docs.extend(buffers(&[("file:///manual/guide/topic/README.markdown", "index")]));
    assert!(resolve(&docs, ORIGIN, "topic#x").unwrap().is_none());
    assert_eq!(resolve(&docs, ORIGIN, "topic/#x").unwrap().unwrap().1, "index");
    docs.extend(buffers(&[("file:///manual/guide/topic", "exact")]));
    assert_eq!(resolve(&docs, ORIGIN, "topic#x").unwrap().unwrap().1, "exact");
    assert!(resolve(&docs, ORIGIN, "topic.pdf#x").unwrap().is_none());
    docs.extend(buffers(&[("file:///manual/index.md", "parent")]));
    assert_eq!(resolve(&docs, ORIGIN, "../#x").unwrap().unwrap().1, "parent");
}

#[test]
fn alias_ambiguity_and_unsynchronized_targets_never_pick_a_revision() {
    let mut docs = buffers(&[(TARGET, "first")]);
    docs.extend(buffers(&[("file://localhost/manual/Other chapter.md", "second")]));
    assert!(resolve(&docs, ORIGIN, "../Other%20chapter.md").unwrap().is_none());
    docs.remove("file://localhost/manual/Other chapter.md");
    docs.get_mut(TARGET).unwrap().synchronized = false;
    assert!(matches!(resolve(&docs, ORIGIN, "../Other%20chapter.md"), Err((-32801, _))));
    docs.remove(TARGET);
    assert!(resolve(&docs, ORIGIN, "../Other%20chapter.md").unwrap().is_none());
}

#[test]
fn malformed_and_external_urls_do_not_gain_open_buffer_authority() {
    let docs = buffers(&[("file:///manual/guide/topic.md", "# Topic")]);
    for destination in [
        "https://example.test/topic.md#topic", "//host/topic.md#topic", "data:topic.md",
        "file://user@host/manual/guide/topic.md", "file:topic.md", "topic%2F.md",
        "topic%5C.md", "topic%00.md", "topic%zz.md", "topic%FF.md", "topic\\x.md",
        "../../../topic.md", "%2e%2e/%2e%2e/%2e%2e/topic.md",
    ] {
        assert!(resolve(&docs, ORIGIN, destination).unwrap().is_none(), "{destination}");
    }
    assert!(resolve(&docs, "untitled:one", "topic.md").unwrap().is_none());
    assert!(addressed("file:///C:/a.md", "../topic.md").is_none());
    assert!(file_uri("file:///manual/a.md?revision=2").is_none());
    assert!(file_uri("file:///manual/a.md#draft").is_none());
}

#[test]
fn definitions_use_target_ids_and_target_utf16_source_ranges() {
    let mut server = opened(&[(TARGET, "# Other\r\n\r\n# Shared\r\n\r\n# Shared\r\n")]);
    let result = call(&mut server, "textDocument/definition",
        "# Shared\r\n\r\n😀 [go](<../Other%20chapter.md?q=1#shar¦ed-2> \"Keep\")");
    assert_eq!(result.get("uri").and_then(Json::as_str), Some(TARGET));
    assert_eq!(result.get("range").and_then(|r| r.get("start")),
        Some(&position(Position { line: 4, character: 0 })));
    assert!(server.documents[ORIGIN].text.contains("#shared-2> \"Keep\")"));
    let result = call(&mut server, "textDocument/definition",
        "[go](../Oth¦er%20chapter.md#shared)");
    assert_eq!(result.get("range").and_then(|r| r.get("start")),
        Some(&position(Position { line: 2, character: 0 })));
}

#[test]
fn bare_file_links_and_empty_fragments_address_the_target_document_root() {
    let mut server = opened(&[(TARGET, "prose before a heading\n\n# Target")]);
    for source in [
        "[go](../Other%20chap¦ter.md)",
        "[go](../Other%20chapter.md#¦)",
        "[go](../Other%20chapter.md?q=¦1)",
    ] {
        let result = call(&mut server, "textDocument/definition", source);
        assert_eq!(result.get("uri").and_then(Json::as_str), Some(TARGET));
        let zero = position(Position { line: 0, character: 0 });
        assert_eq!(result.get("range").and_then(|r| r.get("start")), Some(&zero));
        assert_eq!(result.get("range").and_then(|r| r.get("end")), Some(&zero));
    }
}

#[test]
fn completion_edits_only_the_fragment_in_the_origin_buffer() {
    let mut server = opened(&[(TARGET, "# Shared\n\n# Shared\n")]);
    let result = call(&mut server, "textDocument/completion",
        "# Shadow\r\n\r\n😀 [go](<../Other%20chapter.md?mode=read#%73h¦ared-old> \"Keep\") tail");
    assert_eq!(labels(&result), ["#shared", "#shared-2"]);
    let Json::Array(items) = result.get("items").unwrap() else { panic!("items"); };
    let edit = items[0].get("textEdit").unwrap();
    let range = edit.get("range").unwrap();
    let source = &server.documents[ORIGIN].text;
    let index = LineIndex::new(source);
    let start = index.offset(source, Position::parse(range.get("start").unwrap()).unwrap()).unwrap();
    let end = index.offset(source, Position::parse(range.get("end").unwrap()).unwrap()).unwrap();
    assert_eq!(&source[start..end], "#%73hared-old");
    let mut changed = source.clone();
    changed.replace_range(start..end, edit.get("newText").unwrap().as_str().unwrap());
    assert!(changed.contains("../Other%20chapter.md?mode=read#shared> \"Keep\") tail"));
    assert_eq!(server.documents[TARGET].text, "# Shared\n\n# Shared\n");
}

#[test]
fn unfinished_cross_file_completions_and_encoded_filenames_work() {
    let mut server = opened(&[(TARGET, "# Shared")]);
    for source in [
        "[go](../Other%20chapter.md#sh¦",
        "[go](<../Other%20chapter.md#sh¦",
    ] {
        assert_eq!(labels(&call(&mut server, "textDocument/completion", source)), ["#shared"]);
    }
    server.documents.extend(buffers(&[("file:///manual/guide/A&B.md", "# Shared")]));
    assert_eq!(labels(&call(&mut server, "textDocument/completion",
        "[go](A&amp;B.md#sh¦)")), ["#shared"]);
    let result = call(&mut server, "textDocument/completion", "[go](../Other%20chapter.md#%7¦)");
    assert!(labels(&result).is_empty());
    assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(true)));
}

#[test]
fn cross_file_probes_still_exclude_code_images_html_and_link_titles() {
    let mut server = opened(&[(TARGET, "# Shared")]);
    for source in [
        "`[go](../Other%20chapter.md#sha¦red)`",
        "![go](../Other%20chapter.md#sha¦red)",
        "```md\n[go](../Other%20chapter.md#sha¦red)\n```",
        "<span title=\"[go](../Other%20chapter.md#sha¦red)\">x</span>",
        "[go](#elsewhere \"title ](../Other%20chapter.md#sha¦red)\")",
        "[go](https://example.test/Other%20chapter.md#sha¦red)",
    ] {
        assert_eq!(call(&mut server, "textDocument/definition", source), Json::Null, "{source}");
        assert!(labels(&call(&mut server, "textDocument/completion", source)).is_empty(), "{source}");
    }
}

#[test]
fn referenced_notes_and_nested_target_headings_keep_publication_semantics() {
    let mut server = opened(&[(TARGET, "> # Shared\n> text\n\nUse[^n].\n\n[^n]: note\n\n[^unused]: hidden\n")]);
    let result = call(&mut server, "textDocument/definition", "[go](../Other%20chapter.md#shar¦ed)");
    assert_eq!(result.get("range").and_then(|r| r.get("start")),
        Some(&position(Position { line: 0, character: 0 })));
    let result = call(&mut server, "textDocument/definition", "[go](../Other%20chapter.md#fn-¦n)");
    assert_eq!(result.get("range").and_then(|r| r.get("start")),
        Some(&position(Position { line: 5, character: 0 })));
    assert_eq!(call(&mut server, "textDocument/definition",
        "[go](../Other%20chapter.md#fn-un¦used)"), Json::Null);
    server.documents.get_mut(TARGET).unwrap().text = "# fn-n\n\nUse[^n].\n\n[^n]: note\n".into();
    assert_eq!(call(&mut server, "textDocument/definition",
        "[go](../Other%20chapter.md#fn-¦n)"), Json::Null);
}

#[test]
fn target_changes_resynchronization_and_close_are_observed_immediately() {
    let mut server = opened(&[(TARGET, "# Shared")]);
    let params = at(&mut server, "[go](../Other%20chapter.md#sha¦red)");
    assert_ne!(server.navigate("textDocument/definition", &params).unwrap(), Json::Null);
    let change = |version, changes| object([
        ("textDocument", object([("uri", string(TARGET)), ("version", number(version))])),
        ("contentChanges", changes),
    ]);
    server.change(&change(2, Json::Array(vec![object([("text", string("# Renamed"))])]))).unwrap();
    assert_eq!(server.navigate("textDocument/definition", &params).unwrap(), Json::Null);
    server.change(&change(3, Json::Null)).unwrap();
    assert!(matches!(server.navigate("textDocument/definition", &params), Err((-32801, _))));
    assert!(matches!(server.navigate("textDocument/completion", &params), Err((-32801, _))));
    server.change(&change(4, Json::Array(vec![object([("text", string("# Shared"))])]))).unwrap();
    assert_ne!(server.navigate("textDocument/definition", &params).unwrap(), Json::Null);
    server.close(&object([("textDocument", object([("uri", string(TARGET))]))])).unwrap();
    assert_eq!(server.navigate("textDocument/definition", &params).unwrap(), Json::Null);
}

#[test]
fn protocol_dispatch_returns_the_exact_open_target_uri() {
    let mut server = opened(&[(TARGET, "# Shared")]);
    let params = at(&mut server, "[go](../Other%20chapter.md#sha¦red)");
    let replies = server.handle(object([
        ("jsonrpc", string("2.0")), ("id", string("cross-document")),
        ("method", string("textDocument/definition")), ("params", params),
    ]));
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].get("id").and_then(Json::as_str), Some("cross-document"));
    assert_eq!(replies[0].get("result").and_then(|r| r.get("uri")).and_then(Json::as_str), Some(TARGET));
}
