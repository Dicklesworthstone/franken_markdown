//! Behavior tests use the real server, parser, publication walk and URI resolver.
#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;
use super::super::{Phase, Server, notification, number, object, parse_json, position, read_frame, run, string, write_frame};

const A: &str = "file:///docs/a.md";
const B: &str = "file:///docs/b.md";
const C: &str = "file:///docs/c.md";

fn server() -> Server {
    Server { phase: Phase::Running, ..Server::default() }
}

fn open(server: &mut Server, uri: &str, text: &str, version: i32) -> Vec<Json> {
    server.handle(notification("textDocument/didOpen", object([("textDocument", object([
        ("uri", string(uri)), ("text", string(text)),
        ("version", Json::Number(f64::from(version))),
    ]))])))
}

fn change(server: &mut Server, uri: &str, version: i32, changes: Vec<Json>) -> Vec<Json> {
    server.handle(notification("textDocument/didChange", object([
        ("textDocument", object([("uri", string(uri)), ("version", Json::Number(f64::from(version)))])),
        ("contentChanges", Json::Array(changes)),
    ])))
}

fn replace(server: &mut Server, uri: &str, text: &str, version: i32) -> Vec<Json> {
    change(server, uri, version, vec![object([("text", string(text))])])
}

fn reject(server: &mut Server, uri: &str, version: i32) -> Vec<Json> {
    let outside = object([("line", number(99999)), ("character", number(0))]);
    change(server, uri, version, vec![object([
        ("range", object([("start", outside.clone()), ("end", outside)])),
        ("text", string("rejected")),
    ])])
}

fn close(server: &mut Server, uri: &str) -> Vec<Json> {
    server.handle(notification("textDocument/didClose", object([
        ("textDocument", object([("uri", string(uri))])),
    ])))
}

fn report<'a>(replies: &'a [Json], uri: &str) -> Option<&'a Json> {
    replies.iter().find(|reply| {
        reply.get("method").and_then(Json::as_str) == Some("textDocument/publishDiagnostics")
            && reply.get("params").and_then(|p| p.get("uri")).and_then(Json::as_str) == Some(uri)
    })
}

fn entries(report: &Json) -> &[Json] {
    match report.get("params").and_then(|p| p.get("diagnostics")) {
        Some(Json::Array(entries)) => entries,
        _ => panic!("expected complete diagnostic publication"),
    }
}

fn codes(report: &Json) -> Vec<&str> {
    entries(report).iter().filter_map(|entry| entry.get("code").and_then(Json::as_str)).collect()
}

fn version(report: &Json) -> i32 {
    super::super::text::integer(report.get("params").unwrap().get("version").unwrap()).unwrap()
}

#[test]
fn opening_a_target_checks_inbound_links_without_editing_the_origin() {
    let mut server = server();
    let first = open(&mut server, A, "[go](b.md#topic)", 4);
    assert!(entries(report(&first, A).unwrap()).is_empty());
    let second = open(&mut server, B, "# Different", 1);
    assert_eq!(second[0].get("params").unwrap().get("uri"), Some(&string(B)));
    assert_eq!(codes(report(&second, A).unwrap()), ["workspace_missing_anchor"]);
    assert_eq!(version(report(&second, A).unwrap()), 4);
    assert_eq!(server.diagnostics.rebuilds, 2);
}

#[test]
fn target_rename_and_repair_replace_the_inbound_diagnostic_set() {
    let mut server = server();
    open(&mut server, B, "# Topic", 1);
    open(&mut server, A, "[go](b.md#topic)", 7);
    let broken = replace(&mut server, B, "# Renamed", 2);
    assert_eq!(codes(report(&broken, A).unwrap()), ["workspace_missing_anchor"]);
    let repaired = replace(&mut server, B, "# Topic", 3);
    assert!(entries(report(&repaired, A).unwrap()).is_empty());
    assert_eq!(version(report(&repaired, A).unwrap()), 7);
    assert_eq!(server.diagnostics.rebuilds, 4);
}

#[test]
fn local_parser_and_anchor_findings_survive_cross_document_refresh() {
    let mut server = server();
    let source = "[local](#absent) [remote](b.md#topic)\n\n```rust\nunfinished";
    let initial = open(&mut server, A, source, 1);
    let local = entries(report(&initial, A).unwrap()).to_vec();
    assert!(local.iter().any(|finding| finding.get("code").is_none()));
    assert!(codes(report(&initial, A).unwrap()).contains(&"missing_anchor"));
    let broken = open(&mut server, B, "# Elsewhere", 1);
    let combined = entries(report(&broken, A).unwrap());
    assert_eq!(&combined[..local.len()], local.as_slice());
    assert_eq!(combined.last().unwrap().get("code"), Some(&string("workspace_missing_anchor")));
    let repaired = replace(&mut server, B, "# Topic", 2);
    assert_eq!(entries(report(&repaired, A).unwrap()), local.as_slice());
}

#[test]
fn cache_reuses_other_revisions_and_does_not_resend_unchanged_findings() {
    let mut server = server();
    open(&mut server, B, "# Elsewhere", 1);
    open(&mut server, A, "[go](b.md#topic)", 1);
    let unrelated = open(&mut server, C, "# Unrelated", 1);
    assert!(report(&unrelated, A).is_none());
    assert_eq!(server.diagnostics.rebuilds, 3);
    let still_broken = replace(&mut server, B, "# Still absent", 2);
    assert!(report(&still_broken, A).is_none());
    assert!(report(&still_broken, B).is_some());
    assert_eq!(server.diagnostics.rebuilds, 4);
    assert!(server.diagnostics.refresh(&server.documents, None).is_empty());
    assert_eq!(server.diagnostics.rebuilds, 4);
}

#[test]
fn duplicate_heading_ids_in_other_documents_do_not_repair_the_wrong_target() {
    let mut server = server();
    open(&mut server, B, "# Missing", 1);
    open(&mut server, C, "# Topic", 1);
    let replies = open(&mut server, A, "[b](b.md#topic)\n\n[c](c.md#topic)", 1);
    let findings = entries(report(&replies, A).unwrap());
    assert_eq!(findings.len(), 1);
    assert!(findings[0].get("message").unwrap().as_str().unwrap().contains("b.md#topic"));
}

#[test]
fn relative_explicit_file_and_extensionless_paths_use_opened_buffer_identity() {
    let mut server = server();
    open(&mut server, B, "# Topic", 1);
    let source = "[a](./b.md#topic)\n\n[b](b#topic)\n\n[c](file:///docs/b.md#topic)\n\n[d](b.md?view=print#topic)";
    let valid = open(&mut server, A, source, 1);
    assert!(entries(report(&valid, A).unwrap()).is_empty());
    let invalid = replace(&mut server, B, "# Other", 2);
    assert_eq!(codes(report(&invalid, A).unwrap()), vec!["workspace_missing_anchor"; 4]);
}

#[test]
fn directory_index_and_percent_encoded_file_names_resolve_without_io() {
    let mut server = server();
    open(&mut server, "file:///docs/chapter/index.md", "# Topic", 1);
    open(&mut server, "file:///docs/a%20b.md", "# Topic", 1);
    let source = "[directory](chapter/#topic)\n\n[encoded](a%20b.md#topic)";
    let replies = open(&mut server, A, source, 1);
    assert!(entries(report(&replies, A).unwrap()).is_empty());
    let replies = replace(&mut server, "file:///docs/a%20b.md", "# Other", 2);
    assert_eq!(codes(report(&replies, A).unwrap()), ["workspace_missing_anchor"]);
}

#[test]
fn ambiguous_aliases_and_unopened_files_are_unchecked_not_missing() {
    let mut server = server();
    open(&mut server, B, "# Other", 1);
    open(&mut server, A, "[go](b.md#topic) [not open](absent.md#topic)", 1);
    let replies = open(&mut server, "file:/docs/b.md", "# Other", 1);
    assert!(entries(report(&replies, A).unwrap()).is_empty());
    let replies = close(&mut server, "file:/docs/b.md");
    assert_eq!(codes(report(&replies, A).unwrap()), ["workspace_missing_anchor"]);
}

#[test]
fn unicode_fragments_and_crlf_ranges_use_the_original_source_block() {
    let mut server = server();
    open(&mut server, B, "# Café", 1);
    let source = "`b.md#missing`\r\n\r\n😀 [yes](b.md#caf%C3%A9) [no](b.md#absent)\r\n";
    let replies = open(&mut server, A, source, 1);
    let findings = entries(report(&replies, A).unwrap());
    assert_eq!(findings.len(), 1);
    let parsed = franken_markdown::parse_markdown_spanned(source);
    let span = parsed.blocks.last().unwrap().span;
    let index = LineIndex::new(source);
    let range = findings[0].get("range").unwrap();
    assert_eq!(range.get("start"), Some(&position(index.position(source, span.start))));
    assert_eq!(range.get("end"), Some(&position(index.position(source, span.end))));
    assert_eq!(range.get("start").unwrap().get("line"), Some(&number(2)));
}

#[test]
fn malformed_fragments_keep_a_distinct_code_and_network_urls_are_ignored() {
    let mut server = server();
    open(&mut server, B, "# Topic", 1);
    let replies = open(&mut server, A,
        "[bad](b.md#%ff)\n\n[bad](b.md#to%00pic)\n\n[external](https://example.com/#%ff)\n\n[network](//example.com/b.md#%ff)", 1);
    assert_eq!(codes(report(&replies, A).unwrap()), vec!["workspace_invalid_fragment"; 2]);
}

#[test]
fn repeated_reference_style_and_nested_links_are_deduplicated_per_block() {
    let mut server = server();
    open(&mut server, B, "# Topic", 1);
    let source = "> [a][section] [b][section]\n\n- [c](b.md#absent)\n\n[section]: b.md#absent";
    let replies = open(&mut server, A, source, 1);
    let findings = entries(report(&replies, A).unwrap());
    assert_eq!(findings.len(), 2);
    assert_ne!(findings[0].get("range"), findings[1].get("range"));
    assert_eq!(codes(report(&replies, A).unwrap()), vec!["workspace_missing_anchor"; 2]);
}

#[test]
fn code_images_and_unpublished_note_bodies_do_not_create_edges() {
    let mut server = server();
    open(&mut server, B, "# Topic", 1);
    let source = "`[code](b.md#missing)`\n\n![asset](b.md#missing)\n\n```md\n[fenced](b.md#missing)\n```\n\n[^unused]: [hidden](b.md#missing)\n\n[real](b.md#topic)";
    let replies = open(&mut server, A, source, 1);
    assert!(entries(report(&replies, A).unwrap()).is_empty());
    let replies = replace(&mut server, B, "# Other", 2);
    assert_eq!(codes(report(&replies, A).unwrap()), ["workspace_missing_anchor"]);
}

#[test]
fn referenced_notes_and_real_heading_note_collisions_follow_publication() {
    let mut server = server();
    open(&mut server, A, "[note](b.md#fn-note)", 1);
    let hidden = open(&mut server, B, "[^note]: Hidden", 1);
    assert_eq!(codes(report(&hidden, A).unwrap()), ["workspace_missing_anchor"]);
    let visible = replace(&mut server, B, "note[^note]\n\n[^note]: Visible", 2);
    assert!(entries(report(&visible, A).unwrap()).is_empty());
    let collision = replace(&mut server, B, "# fn-note\n\nnote[^note]\n\n[^note]: Visible", 3);
    assert_eq!(codes(report(&collision, A).unwrap()), ["workspace_ambiguous_anchor"]);
}

#[test]
fn rejected_target_edit_invalidates_inbound_checks_until_full_resynchronization() {
    let mut server = server();
    open(&mut server, A, "[go](b.md#topic)", 1);
    open(&mut server, B, "# Topic", 1);
    let replies = reject(&mut server, B, 2);
    assert!(entries(report(&replies, B).unwrap()).is_empty());
    assert_eq!(codes(report(&replies, A).unwrap()), ["workspace_target_unsynchronized"]);
    assert!(!server.diagnostics.entries.contains_key(B));
    let replies = replace(&mut server, B, "# Topic", 3);
    assert!(entries(report(&replies, A).unwrap()).is_empty());
    assert_eq!(version(report(&replies, B).unwrap()), 3);
}

#[test]
fn rejected_source_diagnostics_are_not_resurrected_by_an_unrelated_edit() {
    let mut server = server();
    open(&mut server, B, "# Other", 1);
    open(&mut server, A, "[go](b.md#topic)", 1);
    let replies = reject(&mut server, A, 2);
    assert!(entries(report(&replies, A).unwrap()).is_empty());
    let replies = replace(&mut server, B, "# Topic", 2);
    assert!(report(&replies, A).is_none());
    assert!(!server.diagnostics.entries.contains_key(A));
}

#[test]
fn target_close_clears_stale_findings_and_reopen_is_a_new_incarnation() {
    let mut server = server();
    open(&mut server, B, "# Other", 1);
    open(&mut server, A, "[go](b.md#topic)", 1);
    let replies = close(&mut server, B);
    assert!(entries(report(&replies, B).unwrap()).is_empty());
    assert!(entries(report(&replies, A).unwrap()).is_empty());
    let replies = open(&mut server, B, "# Topic", 1);
    assert!(report(&replies, A).is_none());
    assert_eq!(server.diagnostics.rebuilds, 3);
    let replies = replace(&mut server, B, "# Other", 2);
    assert_eq!(codes(report(&replies, A).unwrap()), ["workspace_missing_anchor"]);
}

#[test]
fn stale_versions_do_not_replace_cached_anchors_or_trigger_a_reparse() {
    let mut server = server();
    open(&mut server, B, "# Topic", 5);
    open(&mut server, A, "[go](b.md#topic)", 1);
    let replies = replace(&mut server, B, "# Other", 4);
    assert!(report(&replies, A).is_none());
    assert!(report(&replies, B).is_none());
    assert_eq!(server.documents[B].text, "# Topic");
    assert_eq!(server.diagnostics.rebuilds, 2);
}

#[test]
fn combined_diagnostics_enforce_the_existing_cap_and_preserve_local_priority() {
    let mut server = server();
    open(&mut server, B, "# Topic", 1);
    let source = format!("[local](#absent)\n\n{}", "[remote](b.md#absent)\n\n".repeat(MAX_DIAGNOSTICS));
    let replies = open(&mut server, A, &source, 1);
    let findings = entries(report(&replies, A).unwrap());
    assert_eq!(findings.len(), MAX_DIAGNOSTICS);
    assert_eq!(findings[0].get("code"), Some(&string("missing_anchor")));
    assert_eq!(findings.last().unwrap().get("code"), Some(&string("diagnostics_truncated")));
    assert!(findings.iter().all(|f| f.get("message").unwrap().as_str().unwrap().chars().count() <= 512));
}

#[test]
fn inventory_refusal_retains_local_findings_and_reports_incomplete_workspace_checks() {
    let mut server = server();
    let source = format!("[local](#absent)\n\n[huge]({}.md#topic)", "x".repeat(8192));
    let replies = open(&mut server, A, &source, 1);
    let report = report(&replies, A).unwrap();
    assert_eq!(codes(report), ["missing_anchor", "workspace_link_analysis_incomplete"]);
}

#[test]
fn unavailable_target_analysis_is_not_mistaken_for_a_missing_or_valid_anchor() {
    let mut server = server();
    open(&mut server, A, "[go](b.md#topic)", 1);
    let source = format!("# Topic\n\n{}", "[bad](#absent) ".repeat(4097));
    let replies = open(&mut server, B, &source, 1);
    assert_eq!(codes(report(&replies, B).unwrap()), ["link_analysis_incomplete"]);
    assert_eq!(codes(report(&replies, A).unwrap()), ["workspace_link_analysis_incomplete"]);
}

#[test]
fn global_reference_admission_is_explicit_and_recovers_when_buffers_close() {
    let mut server = server();
    let target = "file:///docs/target.md";
    open(&mut server, target, "# Topic", 1);
    let repeated = "[go](target.md#topic) ".repeat(11_000);
    open(&mut server, A, &repeated, 1);
    open(&mut server, B, &repeated, 1);
    let replies = open(&mut server, C, &repeated, 1);
    for uri in [A, B, C] {
        assert_eq!(codes(report(&replies, uri).unwrap()), ["workspace_link_analysis_incomplete"]);
    }
    let replies = close(&mut server, C);
    assert!(entries(report(&replies, A).unwrap()).is_empty());
    assert!(entries(report(&replies, B).unwrap()).is_empty());
}

#[test]
fn shutdown_releases_cached_analyses_and_published_finding_sets() {
    let mut server = server();
    open(&mut server, A, "[go](b.md#topic)", 1);
    assert_eq!(server.diagnostics.entries.len(), 1);
    server.handle(object([
        ("jsonrpc", string("2.0")), ("id", number(1)), ("method", string("shutdown")),
    ]));
    assert!(server.diagnostics.entries.is_empty());
    assert!(server.documents.is_empty());
}

#[test]
fn framed_session_refreshes_untouched_origins_with_their_own_versions() {
    let messages = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///docs/a.md","version":10,"text":"[go](b.md#topic)"}}}"##,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///docs/b.md","version":1,"text":"# Other"}}}"##,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///docs/b.md","version":2},"contentChanges":[{"text":"# Topic"}]}}"##,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ];
    let mut input = Vec::new();
    for message in messages {
        parse_json(message).unwrap();
        write_frame(&mut input, message).unwrap();
    }
    let mut output = Vec::new();
    assert!(run(&mut std::io::Cursor::new(input), &mut output).unwrap());
    let mut output = std::io::Cursor::new(output);
    let mut inbound = Vec::new();
    while let Some(body) = read_frame(&mut output, 8 * 1024 * 1024).unwrap() {
        let message = parse_json(&body).unwrap();
        if report(std::slice::from_ref(&message), A).is_some() { inbound.push(message); }
    }
    assert_eq!(inbound.len(), 3);
    assert!(inbound.iter().all(|report| version(report) == 10));
    assert!(entries(&inbound[0]).is_empty());
    assert_eq!(codes(&inbound[1]), ["workspace_missing_anchor"]);
    assert!(entries(&inbound[2]).is_empty());
}
