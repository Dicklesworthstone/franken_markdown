//! Versioned parser and internal-anchor diagnostics without font loading,
//! PDF layout, URI reads, or a second Markdown grammar.
//!
//! Link semantics come from shared HTML/book navigation analysis. Locations
//! are the parser's enclosing top-level block, NOT guessed inline spans. A
//! target is reported once per enclosing block, including repeated references.

use std::collections::BTreeSet;

use franken_markdown::book::validation::analyze_document_links;
use franken_markdown::{DiagnosticSeverity, Document, SourceSpan, parse_markdown_spanned};

use super::{Buffer, Json, LineIndex, notification, number, object, position, string};

const MAX_DIAGNOSTICS: usize = 1024;
const MAX_MESSAGE_CHARS: usize = 512;

fn bounded_message(message: &str) -> String {
    let mut chars = message.chars();
    let mut result: String = chars.by_ref().take(MAX_MESSAGE_CHARS).collect();
    if chars.next().is_some() {
        result.pop();
        result.push('…');
    }
    result
}

fn finding(source: &str, index: &LineIndex, span: SourceSpan, severity: usize,
    message: &str, code: Option<&str>) -> Json
{
    let mut value = object([
        ("range", object([
            ("start", position(index.position(source, span.start))),
            ("end", position(index.position(source, span.end.max(span.start)))),
        ])),
        ("severity", number(severity)),
        ("source", string("fmd")),
        ("message", string(&bounded_message(message))),
    ]);
    if let (Json::Object(fields), Some(code)) = (&mut value, code) {
        fields.insert("code".to_owned(), string(code));
    }
    value
}

pub(super) fn publish(uri: &str, buffer: &Buffer) -> Json {
    let document = parse_markdown_spanned(&buffer.text);
    let index = LineIndex::new(&buffer.text);
    let source = buffer.text.as_str();
    // Retain one extra item only to distinguish exactly-at-limit from actual
    // truncation. The final wire array never exceeds MAX_DIAGNOSTICS.
    let mut findings: Vec<_> = document.diagnostics.iter().take(MAX_DIAGNOSTICS + 1)
        .map(|diagnostic| finding(source, &index, diagnostic.span,
            match diagnostic.severity { DiagnosticSeverity::Error => 1, DiagnosticSeverity::Warning => 2 },
            &diagnostic.message, None))
        .collect();

    if findings.len() <= MAX_DIAGNOSTICS {
        // Move the original AST rather than cloning whole nested documents.
        let (spans, blocks): (Vec<_>, Vec<_>) = document.blocks.into_iter()
            .map(|block| (block.span, block.node)).unzip();
        let plain = Document { blocks };
        match analyze_document_links(&plain) {
            Ok(report) => {
                let mut seen = BTreeSet::new();
                for reference in report.references {
                    let Some(issue) = reference.finding else { continue; };
                    // The shared report owns true top-level provenance even
                    // when note emission moves its body later in reading order.
                    if !seen.insert((reference.block_index, issue.code, issue.destination.clone())) {
                        continue;
                    }
                    let Some(span) = spans.get(reference.block_index).copied()
                        .filter(|span| source.get(span.start..span.end).is_some())
                    else {
                        findings.push(finding(source, &index, SourceSpan::new(0, 0), 2,
                            "Link locations could not be validated; this diagnostic list is incomplete.",
                            Some("link_analysis_incomplete")));
                        break;
                    };
                    // Bound before formatting: many references to one long
                    // target must not multiply it into a huge response.
                    let mut characters = issue.destination.chars();
                    let mut destination: String = characters.by_ref().take(160).collect();
                    if characters.next().is_some() { destination.push('…'); }
                    findings.push(finding(source, &index, span, 2,
                        &format!("{} Target: {destination}. The range covers its enclosing Markdown block.", issue.message),
                        Some(issue.code)));
                    if findings.len() > MAX_DIAGNOSTICS { break; }
                }
            }
            Err(_) => findings.push(finding(source, &index, SourceSpan::new(0, 0), 2,
                "Internal-link analysis exceeded its admission limits. Link verification is incomplete; parser diagnostics are retained.",
                Some("link_analysis_incomplete"))),
        }
    }

    if findings.len() > MAX_DIAGNOSTICS {
        findings.truncate(MAX_DIAGNOSTICS - 1);
        findings.push(finding(source, &index, SourceSpan::new(0, 0), 2,
            "Additional diagnostics were omitted after reaching the 1024-item limit. Fix the displayed findings to reveal more.",
            Some("diagnostics_truncated")));
    }
    notification("textDocument/publishDiagnostics", object([
        ("uri", string(uri)), ("version", Json::Number(f64::from(buffer.version))),
        ("diagnostics", Json::Array(findings)),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{Phase, Server, parse_json};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn entries(report: &Json) -> &[Json] {
        match report.get("params").and_then(|params| params.get("diagnostics")) {
            Some(Json::Array(entries)) => entries,
            _ => &[],
        }
    }

    fn inspect(source: &str) -> Json {
        publish("untitled:anchors", &Buffer { text: source.to_owned(), version: 7, synchronized: true })
    }

    fn anchors(report: &Json) -> Vec<&Json> {
        entries(report).iter().filter(|entry| entry.get("code").and_then(Json::as_str)
            == Some("missing_anchor")).collect()
    }

    #[test]
    fn matching_later_headings_do_not_hide_an_unresolved_target() {
        let report = inspect("[valid](#later) [bad](#missing)\n\n# Later\n");
        let found = anchors(&report);
        assert_eq!(found.len(), 1);
        assert!(found[0].get("message").and_then(Json::as_str).is_some_and(|message| message.contains("#missing")));
        assert_eq!(report.get("params").and_then(|params| params.get("version")).and_then(Json::as_u64), Some(7));
    }

    #[test]
    fn code_and_external_resources_are_not_treated_as_heading_links() {
        let report = inspect("`[x](#missing)`\n\n```md\n[x](#missing)\n```\n\n[x](https://example.invalid/#missing) [x](other.md#missing) ![x](missing.png)\n");
        assert!(anchors(&report).is_empty());
    }

    #[test]
    fn repeated_references_are_deduplicated_per_real_enclosing_block() {
        let report = inspect("[one][id] [two][id]\n\n[three][id]\n\n[id]: #absent\n");
        let found = anchors(&report);
        assert_eq!(found.len(), 2);
        assert_ne!(found[0].get("range"), found[1].get("range"));
    }

    #[test]
    fn nested_headings_and_links_follow_the_shared_render_audit() {
        let source = "# Root\n\n> ## Nested\n> [yes](#root) [no](#absent)\n\n- [yes](#nested)\n- [no](#absent)\n";
        let report = inspect(source);
        let found = anchors(&report);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|entry| entry.get("message").and_then(Json::as_str)
            .is_some_and(|message| message.contains("#absent"))));
    }

    #[test]
    fn source_ranges_cover_the_real_unicode_crlf_block_not_a_matching_literal() -> TestResult {
        let source = "`#absent`\r\n\r\n😀 [bad](#absent)\r\n";
        let report = inspect(source);
        let found = anchors(&report);
        assert_eq!(found.len(), 1);
        let range = found[0].get("range").ok_or("missing range")?;
        assert_eq!(range.get("start").and_then(|p| p.get("line")).and_then(Json::as_u64), Some(2));
        assert_eq!(range.get("start").and_then(|p| p.get("character")).and_then(Json::as_u64), Some(0));
        let parsed = parse_markdown_spanned(source);
        let block = parsed.blocks.last().ok_or("no paragraph")?;
        let index = LineIndex::new(source);
        assert_eq!(range.get("end"), Some(&position(index.position(source, block.span.end))));
        Ok(())
    }

    #[test]
    fn parser_findings_are_preserved_alongside_anchor_findings() {
        let source = "[bad](#absent)\n\n```rust\nlet unfinished = true;\n";
        let expected = parse_markdown_spanned(source).diagnostics;
        assert!(!expected.is_empty());
        let report = inspect(source);
        assert_eq!(anchors(&report).len(), 1);
        for diagnostic in expected {
            assert!(entries(&report).iter().any(|entry| entry.get("message").and_then(Json::as_str)
                == Some(bounded_message(&diagnostic.message).as_str())));
        }
    }

    #[test]
    fn diagnostic_limit_is_explicit_and_messages_remain_bounded() {
        let report = inspect(&"[bad](#absent)\n\n".repeat(MAX_DIAGNOSTICS + 2));
        assert_eq!(entries(&report).len(), MAX_DIAGNOSTICS);
        assert_eq!(entries(&report).last().and_then(|entry| entry.get("code")).and_then(Json::as_str), Some("diagnostics_truncated"));
        let long = inspect(&format!("[bad](#{})\n", "λ".repeat(2000)));
        assert_eq!(anchors(&long).len(), 1);
        assert!(entries(&long).iter().all(|entry| entry.get("message").and_then(Json::as_str)
            .is_some_and(|message| message.chars().count() <= MAX_MESSAGE_CHARS)));
    }

    #[test]
    fn encoded_fragments_and_root_links_follow_publication_rules() {
        let report = inspect("# Here\n\n[encoded](#%68ere) [root](#) [query](?mode=read) [bad](#%zz)\n");
        assert!(anchors(&report).is_empty());
        assert_eq!(entries(&report).iter().filter(|entry| entry.get("code").and_then(Json::as_str)
            == Some("invalid_fragment")).count(), 1);
    }

    #[test]
    fn emitted_footnote_collisions_are_reported_but_unused_note_examples_are_not() {
        let source = "# fn-note\n\n[^note] [ambiguous](#fn-note)\n\n[^note]: Text.\n\n[^unused]: [example](#absent)\n";
        let report = inspect(source);
        assert!(anchors(&report).is_empty());
        assert_eq!(entries(&report).iter().filter(|entry| entry.get("code").and_then(Json::as_str)
            == Some("ambiguous_anchor")).count(), 1);
    }

    #[test]
    fn analyzer_admission_failure_is_not_published_as_a_clean_document() {
        let report = inspect(&"[bad](#absent) ".repeat(4097));
        assert_eq!(entries(&report).len(), 1);
        assert_eq!(entries(&report)[0].get("code").and_then(Json::as_str), Some("link_analysis_incomplete"));
    }

    #[test]
    fn live_open_rename_repair_and_rejected_edit_do_not_publish_stale_findings() -> TestResult {
        let mut server = Server { phase: Phase::Running, ..Server::default() };
        let messages = [
            r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///not-read.md","version":1,"text":"# Here\n\n[go](#here)\n"}}}"###,
            r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///not-read.md","version":2},"contentChanges":[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":6}},"text":"There"}]}}"###,
            r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///not-read.md","version":3},"contentChanges":[{"text":"# There\n\n[go](#there)\n"}]}}"###,
            r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///not-read.md","version":4},"contentChanges":[{"range":{"start":{"line":99,"character":0},"end":{"line":99,"character":0}},"text":"x"}]}}"###,
        ];
        for (index, input) in messages.iter().enumerate() {
            let replies = server.handle(parse_json(input)?);
            let report = replies.iter().find(|reply| reply.get("method").and_then(Json::as_str)
                == Some("textDocument/publishDiagnostics")).ok_or("missing publication")?;
            assert_eq!(anchors(report).len(), usize::from(index == 1));
            if index < 3 {
                assert_eq!(report.get("params").and_then(|p| p.get("version")).and_then(Json::as_u64), Some((index + 1) as u64));
            } else {
                assert!(entries(report).is_empty());
                assert!(!server.documents["file:///not-read.md"].synchronized);
            }
        }
        Ok(())
    }
}
