use super::super::{Buffer, Phase, Server};
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const URI: &str = "untitled:labels";

fn params(source: &str, offset: usize) -> Json {
    object([
        ("textDocument", object([("uri", string(URI))])),
        ("position", position(LineIndex::new(source).position(source, offset))),
    ])
}

fn complete(marked: &str) -> Result<(String, Json), Box<dyn std::error::Error>> {
    let offset = marked.find('¦').ok_or("missing caret")?;
    let source = marked.replace('¦', "");
    let result = request(&params(&source, offset), &source)
        .map_err(|(_, reason)| reason)?.unwrap_or_else(|| list(Vec::new(), false));
    Ok((source, result))
}

fn items(result: &Json) -> &[Json] {
    match result.get("items") { Some(Json::Array(items)) => items, _ => &[] }
}

fn labels(result: &Json) -> Vec<&str> {
    items(result).iter().filter_map(|item| item.get("label").and_then(Json::as_str)).collect()
}

fn apply(source: &str, item: &Json) -> Result<String, &'static str> {
    let edit = item.get("textEdit").ok_or("missing edit")?;
    let span = edit.get("range").ok_or("missing range")?;
    let index = LineIndex::new(source);
    let start = index.offset(source, Position::parse(span.get("start").ok_or("missing start")?)?)?;
    let end = index.offset(source, Position::parse(span.get("end").ok_or("missing end")?)?)?;
    let mut result = source.to_string();
    result.replace_range(start..end, edit.get("newText").and_then(Json::as_str).ok_or("missing replacement")?);
    Ok(result)
}

#[test]
fn footnote_mid_label_edits_preserve_unicode_crlf_and_surrounding_source() -> TestResult {
    let (source, result) = complete("😀 Claim [^re¦mainder] end\r\n\r\n[^research]: Evidence.\r\n\r\n[^Research]: Different case.\r\n")?;
    assert_eq!(labels(&result), ["[^research]"]);
    let edit = items(&result)[0].get("textEdit").ok_or("missing edit")?;
    let span = edit.get("range").ok_or("missing range")?;
    assert_eq!(span.get("start").and_then(|p| p.get("character")).and_then(Json::as_u64), Some(11));
    assert_eq!(span.get("end").and_then(|p| p.get("character")).and_then(Json::as_u64), Some(20));
    assert_eq!(apply(&source, &items(&result)[0])?, source.replacen("[^remainder]", "[^research]", 1));
    Ok(())
}

#[test]
fn unfinished_and_empty_note_markers_close_without_eating_following_prose() -> TestResult {
    for marked in ["Claim [^re¦ tail", "Claim [^re¦mainder", "Claim [^¦]", "Claim [^¦"] {
        let marked = format!("{marked}\n\n[^research]: Evidence.\n");
        let (source, result) = complete(&marked)?;
        assert_eq!(labels(&result), ["[^research]"], "{marked}");
        let changed = apply(&source, &items(&result)[0])?;
        assert!(changed.starts_with("Claim [^research]"), "{changed}");
        if source.contains(" tail") { assert!(changed.contains("[^research] tail")); }
        assert!(changed.ends_with("[^research]: Evidence.\n"));
    }
    Ok(())
}

#[test]
fn notes_include_dormant_and_nested_definitions_once_in_parser_order() -> TestResult {
    let (_, result) = complete("Claim [^¦]\n\n[^zebra]: First.\n\n> [^alpha]: Nested.\n\n[^zebra]: Duplicate.\n\n```\n[^fake]: Code.\n```\n")?;
    assert_eq!(labels(&result), ["[^zebra]", "[^alpha]"]);
    assert_eq!(items(&result)[0].get("sortText").and_then(Json::as_str), Some("0000"));
    assert_eq!(items(&result)[1].get("sortText").and_then(Json::as_str), Some("0001"));
    Ok(())
}

#[test]
fn note_completions_exclude_literal_and_definition_contexts() -> TestResult {
    for body in [
        "`[^re¦]`", "```\n[^re¦]\n```", "\\[^re¦]", "![alt [^re¦]](/image)",
        "[label](/path/[^re¦])", "[label](/path \"[^re¦]\")", "<span title=\"[^re¦]\">x</span>",
        "$[^re¦]$", "[^re¦]: Definition label.", "`prefix [^re`ma¦] tail", "`prefix [^re`ma¦ tail",
    ] {
        let (_, result) = complete(&format!("{body}\n\n[^research]: Evidence.\n"))?;
        assert!(items(&result).is_empty(), "literal must not complete: {body}");
    }
    Ok(())
}

#[test]
fn reference_prefix_uses_parser_unicode_folding_and_replaces_the_whole_label() -> TestResult {
    let (source, result) = complete("😀 [Guide][STRAS¦se-old] tail\r\n\r\n[Straße]: /manual \"Keep title\"\r\n")?;
    assert_eq!(labels(&result), ["Straße"]);
    assert_eq!(items(&result)[0].get("filterText").and_then(Json::as_str), Some("STRASse"));
    assert_eq!(apply(&source, &items(&result)[0])?, source.replacen("[STRASse-old]", "[Straße]", 1));
    Ok(())
}

#[test]
fn reference_whitespace_and_duplicate_identity_follow_the_parser() -> TestResult {
    let (_, result) = complete("[Guide][  MANUAL ¦suffix]\n\n[Manual Guide]: /first\n\n[manual\tguide]: /shadowed\n\n[Manualish]: /other\n")?;
    assert_eq!(labels(&result), ["Manual Guide"]);
    Ok(())
}

#[test]
fn explicit_empty_reference_suffixes_and_image_references_preserve_visible_labels() -> TestResult {
    let (source, result) = complete("[Guide][¦]\n\n[Guide]: /old\n\n[Manual]: /new\n")?;
    assert_eq!(labels(&result), ["Guide", "Manual"]);
    assert!(apply(&source, &items(&result)[1])?.starts_with("[Guide][Manual]"));
    let (source, result) = complete("![Chart *alt*][im¦age-old]\n\n[image]: /figure.png \"Title\"\n")?;
    assert_eq!(labels(&result), ["image"]);
    assert!(apply(&source, &items(&result)[0])?.starts_with("![Chart *alt*][image]"));
    Ok(())
}

#[test]
fn unfinished_reference_labels_close_and_preserve_suffix_text() -> TestResult {
    for marked in ["[Guide][ma¦ tail", "[Guide][ma¦nual-old", "[Guide][¦"] {
        let (source, result) = complete(&format!("{marked}\n\n[manual]: /guide\n"))?;
        assert_eq!(labels(&result), ["manual"], "{marked}");
        let changed = apply(&source, &items(&result)[0])?;
        assert!(changed.starts_with("[Guide][manual]"));
        if source.contains(" tail") { assert!(changed.contains("[manual] tail")); }
    }
    Ok(())
}

#[test]
fn reference_completion_does_not_offer_edits_in_other_markdown_contexts() -> TestResult {
    for body in [
        "`[Guide][ma¦]`", "```\n[Guide][ma¦]\n```", "[Guide]\\[ma¦]", "[Guide](ma¦)",
        "[label](/path/[Guide][ma¦])", "[label](/path \"[Guide][ma¦]\")",
        "<span title=\"[Guide][ma¦]\">x</span>", "$[Guide][ma¦]$", "[ma¦]: /definition",
        "[outer [Guide][ma¦]](/outer)", "[outer [Guide][ma¦]][outer]",
        "`prefix [Guide][ma`nu¦] tail", "`prefix [Guide][ma`nu¦ tail",
        "[ma¦]", "[Guide][manual]¦",
    ] {
        let (_, result) = complete(&format!("{body}\n\n[manual]: /guide\n\n[outer]: /outer\n"))?;
        assert!(items(&result).is_empty(), "literal or enclosing link must not complete: {body}");
    }
    Ok(())
}

#[test]
fn completion_supports_real_nested_containers_and_keeps_frontmatter_untouched() -> TestResult {
    for body in ["> [Guide][ma¦]", "- [Guide][ma¦]", "Term\n: [Guide][ma¦]", "[^unused]: [Guide][ma¦]"] {
        let marked = format!("\u{feff}---\r\ntitle: Document\r\n---\r\n{body}\n\n[manual]: /guide\n");
        let (source, result) = complete(&marked)?;
        assert_eq!(labels(&result), ["manual"], "{body}");
        assert!(apply(&source, &items(&result)[0])?.starts_with("\u{feff}---\r\ntitle: Document\r\n---\r\n"));
    }
    let (_, result) = complete("---\ntitle: '[Guide][ma¦]'\n---\n\n[manual]: /guide\n")?;
    assert!(items(&result).is_empty());
    Ok(())
}

#[test]
fn completion_probes_do_not_depend_on_a_closed_final_fence() -> TestResult {
    let (_, result) = complete("[Guide][ma¦]\n\n[manual]: /guide\n\n```rust\nunfinished\n")?;
    assert_eq!(labels(&result), ["manual"]);
    Ok(())
}

#[test]
fn completion_results_are_bounded_deterministic_and_mark_truncation() -> TestResult {
    for note in [false, true] {
        let mut marked = if note { "Claim [^n¦]\n\n" } else { "[Guide][n¦]\n\n" }.to_string();
        for n in 0..300 {
            marked.push_str(&format!("[{}n{n:03}]: /value\n\n", if note { "^" } else { "" }));
        }
        let (_, result) = complete(&marked)?;
        assert_eq!(items(&result).len(), MAX_COMPLETIONS);
        assert_eq!(result.get("isIncomplete"), Some(&Json::Bool(true)));
        assert_eq!(items(&result)[255].get("sortText").and_then(Json::as_str), Some("0255"));
        assert_eq!(result, complete(&marked)?.1);
    }
    Ok(())
}

#[test]
fn invalid_positions_and_document_budgets_fail_without_clamping_surrogates() {
    let source = "😀 [Guide][ma]\n\n[manual]: /guide\n";
    let bad = object([("position", object([("line", number(0)), ("character", number(1))]))]);
    assert_eq!(request(&bad, source).err().map(|e| e.0), Some(-32602));
    let oversized = "a".repeat(MAX_DOCUMENT_BYTES + 1);
    assert_eq!(request(&object([]), &oversized).err().map(|e| e.0), Some(-32803));
}

#[test]
fn server_dispatch_uses_current_buffer_preserves_fragment_completion_and_refuses_stale_text() -> TestResult {
    let source = "[Guide][ma]\n\n[manual]: /guide\n";
    let mut server = Server { phase: Phase::Running, ..Server::default() };
    server.documents.insert(URI.to_string(), Buffer { text: source.to_string(), version: 1, synchronized: true });
    let result = server.navigate("textDocument/completion", &params(source, 10)).map_err(|(_,e)| e)?;
    assert_eq!(labels(&result), ["manual"]);
    assert_eq!(server.documents.get(URI).ok_or("missing buffer")?.text, source);
    let source = "# Target\n\n[go](#ta)\n";
    server.documents.insert(URI.to_string(), Buffer { text: source.to_string(), version: 2, synchronized: true });
    let result = server.navigate("textDocument/completion", &params(source, source.find("ta)").ok_or("missing fragment")? + 2)).map_err(|(_,e)| e)?;
    assert_eq!(labels(&result), ["#target"]);
    server.documents.get_mut(URI).ok_or("missing buffer")?.synchronized = false;
    assert_eq!(server.navigate("textDocument/completion", &params(source, 0)).err().map(|e| e.0), Some(-32801));
    Ok(())
}
