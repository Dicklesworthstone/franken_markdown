#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use super::super::{Phase, Server};

fn buffers(entries: &[(&str, &str)]) -> BTreeMap<String, Buffer> {
    entries.iter().map(|(uri, text)| ((*uri).to_string(), Buffer {
        text: (*text).to_string(), version: 1, synchronized: true,
    })).collect()
}

fn search(documents: &BTreeMap<String, Buffer>, query: &str) -> Vec<Json> {
    let result = request(documents, &object([("query", string(query))])).expect("search");
    let Json::Array(items) = result else { panic!("symbol array"); };
    items
}

fn names(items: &[Json]) -> Vec<&str> {
    items.iter().map(|item| item.get("name").unwrap().as_str().unwrap()).collect()
}

fn location(item: &Json) -> &Json {
    item.get("location").expect("location")
}

#[test]
fn ranking_prefers_exact_then_prefix_then_substring_then_distributed_terms() {
    assert_eq!(rank("install", "install", "a", "install", &["install"]), Some(0));
    assert_eq!(rank("installation", "installation", "a", "install", &["install"]), Some(1));
    assert_eq!(rank("quick install", "quick-install", "a", "install", &["install"]), Some(2));
    assert_eq!(rank("guide", "guide", "install.md", "install", &["install"]), Some(3));
    assert_eq!(rank("other", "other", "a", "install", &["install"]), None);
    let docs = buffers(&[
        ("file:///a.md", "# Quick install\n\n# Installation"),
        ("file:///z.md", "# Install"),
    ]);
    assert_eq!(names(&search(&docs, "INSTALL")), ["Install", "Installation", "Quick install"]);
}

#[test]
fn unicode_and_filename_terms_find_the_exact_buffer_and_current_range() {
    let docs = buffers(&[
        ("file:///docs/guide.md", "😀\r\n\r\n# ÉCOLE\r\n"),
        ("untitled:draft", "# ÉCOLE"),
    ]);
    let items = search(&docs, "école GUIDE");
    assert_eq!(names(&items), ["ÉCOLE"]);
    assert_eq!(location(&items[0]).get("uri").and_then(Json::as_str), Some("file:///docs/guide.md"));
    assert_eq!(location(&items[0]).get("range").and_then(|r| r.get("start")),
        Some(&position(Position { line: 2, character: 0 })));
    assert!(items[0].get("containerName").unwrap().as_str().unwrap().contains("#école"));
    assert_eq!(search(&docs, "école").len(), 2);
}

#[test]
fn duplicate_titles_remain_distinct_and_order_is_reproducible() {
    let docs = buffers(&[
        ("file:///b.md", "# Shared"),
        ("file:///a.md", "# Shared\n\n# Shared"),
    ]);
    let items = search(&docs, "shared");
    assert_eq!(items.len(), 3);
    assert_eq!(items, search(&docs, "shared"));
    assert_eq!(location(&items[0]).get("uri").and_then(Json::as_str), Some("file:///a.md"));
    assert_eq!(location(&items[1]).get("uri").and_then(Json::as_str), Some("file:///a.md"));
    assert!(items[1].get("containerName").unwrap().as_str().unwrap().ends_with("#shared-2"));
    assert_eq!(location(&items[2]).get("uri").and_then(Json::as_str), Some("file:///b.md"));
}

#[test]
fn nested_and_referenced_note_headings_use_real_owners_not_text_search() {
    let docs = buffers(&[("untitled:notes",
        "> # Nested\n> body\n\nUse[^n].\n\n[^n]: # Visible\n\n[^u]: # Hidden\n\n```md\n# Example\n```\n")]);
    let items = search(&docs, "");
    assert_eq!(names(&items), ["Nested", "Visible"]);
    assert_eq!(location(&items[0]).get("range").and_then(|r| r.get("start")),
        Some(&position(Position { line: 0, character: 0 })));
    assert_eq!(location(&items[1]).get("range").and_then(|r| r.get("start")),
        Some(&position(Position { line: 5, character: 0 })));
    let ambiguous = buffers(&[("untitled:collision", "# fn-n\n\nUse[^n].\n\n[^n]: note")]);
    assert!(search(&ambiguous, "fn-n").is_empty());
}

#[test]
fn best_results_can_come_from_later_files_after_the_heap_is_full() {
    let many: String = (0..MAX_RESULTS + 20).map(|n| format!("# Needle long {n:03}\n\n")).collect();
    let docs = buffers(&[("file:///a.md", &many), ("file:///z.md", "# Needle")]);
    let items = search(&docs, "needle");
    assert_eq!(items.len(), MAX_RESULTS);
    assert_eq!(names(&items)[0], "Needle");
    assert_eq!(location(&items[0]).get("uri").and_then(Json::as_str), Some("file:///z.md"));
    assert_eq!(items, search(&docs, "needle"));
}

#[test]
fn queries_and_all_admission_limits_fail_without_partial_results() {
    let docs = buffers(&[]);
    for params in [object([]), object([("query", number(1))]),
        object([("query", string(&"x".repeat(MAX_QUERY_BYTES + 1)))]),
        object([("query", string(&"term ".repeat(MAX_QUERY_TERMS + 1)))]),
        object([("query", string("a\0b"))]),
    ] {
        assert!(matches!(request(&docs, &params), Err((-32602, _))));
    }
    let huge = "x".repeat(MAX_DOCUMENT_BYTES + 1);
    assert!(matches!(request(&buffers(&[("a", &huge)]), &object([("query", string(""))])), Err((-32803, _))));
    let heading = format!("# {}", "x".repeat(MAX_HEADING_BYTES + 1));
    assert!(matches!(request(&buffers(&[("a", &heading)]), &object([("query", string(""))])), Err((-32803, _))));
    let many: String = (0..super::super::navigation::MAX_NAVIGATION_ITEMS + 1)
        .map(|n| format!("# Heading {n}\n\n")).collect();
    assert!(matches!(request(&buffers(&[("a", &many)]), &object([("query", string(""))])), Err((-32803, _))));
    let mut too_many = BTreeMap::new();
    for n in 0..MAX_DOCUMENTS + 1 {
        too_many.extend(buffers(&[(&n.to_string(), "")]));
    }
    assert!(matches!(request(&too_many, &object([("query", string(""))])), Err((-32803, _))));
    let one = "x".repeat(MAX_DOCUMENT_BYTES);
    let mut too_large = BTreeMap::new();
    for n in 0..MAX_SESSION_BYTES / MAX_DOCUMENT_BYTES + 1 {
        too_large.extend(buffers(&[(&n.to_string(), &one)]));
    }
    assert!(matches!(request(&too_large, &object([("query", string(""))])), Err((-32803, _))));
}

#[test]
fn response_budget_accounts_for_escaped_uri_copies_before_returning_json() {
    let uri = format!("untitled:{}", "\u{1}".repeat(8000));
    let source: String = (0..100).map(|n| format!("# Heading {n}\n\n")).collect();
    let docs = buffers(&[(&uri, &source)]);
    assert!(matches!(request(&docs, &object([("query", string(""))])), Err((-32803, _))));
    let narrowed = search(&docs, "heading 99");
    assert_eq!(narrowed.len(), 1);
    assert!(Json::Array(narrowed).to_json_string().len() < MAX_RESPONSE_BYTES);
}

#[test]
fn change_resynchronization_close_and_shutdown_remove_obsolete_symbols() {
    let mut server = Server {
        phase: Phase::Running,
        documents: buffers(&[("untitled:live", "# Old"), ("untitled:other", "# Other")]),
        ..Server::default()
    };
    let change = |version, changes| object([
        ("textDocument", object([("uri", string("untitled:live")), ("version", number(version))])),
        ("contentChanges", changes),
    ]);
    server.change(&change(2, Json::Array(vec![object([("text", string("# New"))])]))).unwrap();
    assert!(search(&server.documents, "old").is_empty());
    assert_eq!(names(&search(&server.documents, "new")), ["New"]);
    server.change(&change(3, Json::Null)).unwrap();
    assert_eq!(names(&search(&server.documents, "")), ["Other"]);
    server.change(&change(4, Json::Array(vec![object([("text", string("# Recovered"))])]))).unwrap();
    assert_eq!(names(&search(&server.documents, "recovered")), ["Recovered"]);
    server.close(&object([("textDocument", object([("uri", string("untitled:live"))]))])).unwrap();
    assert_eq!(names(&search(&server.documents, "")), ["Other"]);
    server.handle(object([("jsonrpc", string("2.0")), ("id", number(1)), ("method", string("shutdown"))]));
    assert!(server.documents.is_empty());
}

#[test]
fn protocol_negotiates_and_dispatches_without_requiring_a_text_document() {
    let request = |method, params| object([
        ("jsonrpc", string("2.0")), ("id", string("symbols")),
        ("method", string(method)), ("params", params),
    ]);
    let mut server = Server::default();
    let early = server.handle(request("workspace/symbol", object([("query", string(""))])));
    assert_eq!(early[0].get("error").and_then(|e| e.get("code")), Some(&Json::Number(-32002.0)));
    let init = server.handle(request("initialize", object([])));
    assert_eq!(init[0].get("result").and_then(|r| r.get("capabilities"))
        .and_then(|c| c.get("workspaceSymbolProvider")), Some(&Json::Bool(true)));
    server.documents = buffers(&[("untitled:one", "# Searchable")]);
    let replies = server.handle(request("workspace/symbol", object([("query", string("Searchable"))])));
    assert_eq!(replies[0].get("id").and_then(Json::as_str), Some("symbols"));
    let Json::Array(items) = replies[0].get("result").expect("result") else { panic!("items"); };
    assert_eq!(names(items), ["Searchable"]);
    assert_eq!(items[0].get("kind"), Some(&number(15)));
    let invalid = server.handle(request("workspace/symbol", object([])));
    assert_eq!(invalid[0].get("error").and_then(|e| e.get("code")), Some(&Json::Number(-32602.0)));
    server.handle(request("shutdown", Json::Null));
    let late = server.handle(request("workspace/symbol", object([("query", string(""))])));
    assert_eq!(late[0].get("error").and_then(|e| e.get("code")), Some(&Json::Number(-32600.0)));
}
