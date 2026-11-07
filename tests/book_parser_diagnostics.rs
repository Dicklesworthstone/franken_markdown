//! Parser fallbacks remain observable through retained and native publication.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{
    BookInput, BookRenderer, BookWorkspace, build_book, build_book_with_diagnostics,
};
use franken_markdown::{HtmlOptions, parse_markdown_spanned};

fn source(path: &str, markdown: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: markdown.into(),
    }
}

fn work_limited() -> String {
    format!("{}leaf{}", "[".repeat(32), "](/target)".repeat(32))
}

fn over_diagnostic_limit() -> String {
    (0..4097)
        .map(|index| format!("[broken_{index}]:\n\n"))
        .collect()
}

#[test]
fn every_parser_fallback_retains_the_exact_parser_code_and_byte_range() {
    for (code, markdown) in [
        ("inline_work_limit", work_limited()),
        (
            "inline_depth_limit",
            format!(
                "{}{}leaf{}",
                "text ".repeat(1024),
                "![".repeat(80),
                "](/image)".repeat(80),
            ),
        ),
        ("block_depth_limit", format!("{}leaf β", "> ".repeat(132))),
    ] {
        let markdown = format!("# Préface\r\n\r\n{markdown}\r\n");
        let expected = parse_markdown_spanned(&markdown);
        assert_eq!(expected.diagnostics.len(), 1, "{code}");
        let renderer = BookRenderer::new(&[source("guide/../chapter.md", &markdown)]).unwrap();
        let report = renderer.source_diagnostics();
        assert_eq!(report.diagnostic_count(), 1);
        assert!(!report.is_empty());
        let chapter = &report.chapters()[0];
        assert_eq!(chapter.path, "chapter.md");
        assert_eq!(chapter.source_bytes, markdown.len());
        assert_eq!(chapter.diagnostics, expected.diagnostics);
        assert_eq!(chapter.diagnostics[0].code, code);
        assert!(chapter.diagnostics[0].span.slice(&markdown).is_some());
        assert!(
            report
                .to_json()
                .contains("\"coordinateSpace\":\"expanded_chapter\"")
        );
    }
}

#[test]
fn diagnostics_do_not_change_the_book_ast_or_publication_bytes() {
    let markdown = format!(
        "# Start\n\n{}\n\n[broken]:\n\n```rust\nunclosed",
        work_limited()
    );
    let inputs = [source("start.md", &markdown)];
    let plain = build_book(&inputs).unwrap();
    let (diagnosed, report) = build_book_with_diagnostics(&inputs).unwrap();
    assert_eq!(plain.chapters[0].doc, diagnosed.chapters[0].doc);
    assert_eq!(
        report.chapters()[0]
            .diagnostics
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>(),
        ["inline_work_limit", "malformed_reference", "unclosed_fence"],
    );
    let renderer = BookRenderer::new(&inputs).unwrap();
    assert_eq!(renderer.source_diagnostics(), &report);
    assert_eq!(
        renderer.render_epub().unwrap(),
        franken_markdown::epub::render_book_epub(&plain, &HtmlOptions::default()).unwrap(),
    );
    assert_eq!(
        renderer.render_site().unwrap(),
        franken_markdown::book::render_book_site(&plain, &HtmlOptions::default()).unwrap(),
    );
}

#[test]
fn includes_report_the_expanded_and_rebased_chapter_not_invented_original_offsets() {
    let hostile = work_limited();
    let inputs = [
        source(
            "manual/deep/start.md",
            "# Start\n\n{{#include ../../fragments/body.txt}}\n",
        ),
        source("end.md", "# End\n"),
    ];
    let includes = [source(
        "fragments/body.txt",
        &format!("[Guide](../end.md)\n\n{hostile}\n"),
    )];
    let expanded = format!("# Start\n\n[Guide](<../../end.md>)\n\n{hostile}\n");
    let renderer = BookRenderer::from_sources(&inputs, &includes).unwrap();
    let report = renderer.source_diagnostics();
    assert_eq!(report.chapters().len(), 2);
    assert_eq!(report.diagnostic_count(), 1);
    let chapter = &report.chapters()[0];
    assert_eq!(chapter.path, "manual/deep/start.md");
    assert_eq!(chapter.source_bytes, expanded.len());
    assert_eq!(
        chapter.diagnostics,
        parse_markdown_spanned(&expanded).diagnostics
    );
    assert_eq!(
        chapter.diagnostics[0].span.slice(&expanded),
        Some(hostile.as_str())
    );
    assert!(report.chapters()[1].diagnostics.is_empty());
    assert!(!report.to_json().contains("fragments/body.txt"));
    assert_eq!(renderer.validate_links().unwrap().finding_count(), 0);
}

#[test]
fn unused_resources_and_literal_code_do_not_fabricate_parser_warnings() {
    let hostile = work_limited();
    let renderer = BookRenderer::from_sources(
        &[source(
            "start.md",
            &format!("# Start\n\n```\n{hostile}\n```\n"),
        )],
        &[source("unused.txt", "[broken]:\n\n```unclosed")],
    )
    .unwrap();
    assert!(renderer.source_diagnostics().is_empty());
    assert_eq!(renderer.source_diagnostics().chapters().len(), 1);
    assert!(
        renderer
            .source_diagnostics()
            .to_json()
            .contains("\"diagnostics\":0")
    );
}

#[test]
fn retained_edits_publish_diagnostics_with_their_ast_and_clear_repaired_findings() {
    let mut workspace = BookWorkspace::from_sources(
        &[
            source("first.md", "# First\n\n{{#include fragment.txt}}\n"),
            source("second.md", "# Second\n\n[broken]:\n"),
        ],
        &[source("fragment.txt", &work_limited())],
    )
    .unwrap();
    workspace.options_mut().title = Some("Retain presentation".into());
    assert_eq!(workspace.source_diagnostics().diagnostic_count(), 2);
    let second = workspace.source_diagnostics().chapters()[1].clone();
    let update = workspace
        .update_sources_at_revision(&[source("fragment.txt", "Repaired prose.\n")], 0)
        .unwrap();
    assert_eq!(update.reparsed_chapters, [0]);
    assert_eq!(workspace.source_diagnostics().diagnostic_count(), 1);
    assert!(
        workspace.source_diagnostics().chapters()[0]
            .diagnostics
            .is_empty()
    );
    assert_eq!(workspace.source_diagnostics().chapters()[1], second);
    assert_eq!(
        workspace.options().title.as_deref(),
        Some("Retain presentation")
    );
    let report = workspace.source_diagnostics().to_json();
    let before = workspace.book().chapters[0].doc.clone();
    for (markdown, revision) in [(work_limited(), 0), ("{{#include absent.txt}}".into(), 1)] {
        assert!(
            workspace
                .update_sources_at_revision(&[source("fragment.txt", &markdown)], revision)
                .is_err()
        );
        assert_eq!(workspace.source_revision(), 1);
        assert_eq!(workspace.source_diagnostics().to_json(), report);
        assert_eq!(workspace.book().chapters[0].doc, before);
    }
    workspace
        .replace_sources_at_revision(&[source("renamed.md", "# Clean\n")], &[], 1)
        .unwrap();
    assert_eq!(
        workspace.source_diagnostics().chapters()[0].path,
        "renamed.md"
    );
    assert!(workspace.source_diagnostics().is_empty());
}

#[test]
fn diagnostic_limit_refusal_preserves_the_last_usable_workspace_revision() {
    let mut workspace = BookWorkspace::new(&[source("start.md", "# Good\n")]).unwrap();
    let report = workspace.source_diagnostics().to_json();
    let before = workspace.book().chapters[0].doc.clone();
    let rejected = source("start.md", &over_diagnostic_limit());
    let error = workspace
        .update_sources_at_revision(std::slice::from_ref(&rejected), 0)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("more than 4096 parser diagnostics")
    );
    assert_eq!(workspace.source_revision(), 0);
    assert_eq!(workspace.source_diagnostics().to_json(), report);
    assert_eq!(workspace.book().chapters[0].doc, before);
    assert!(BookRenderer::new(&[rejected]).is_err());
}

#[test]
fn serialized_report_budget_refuses_instead_of_returning_a_partial_report() {
    let suffix = "x".repeat(1500);
    let inputs: Vec<_> = (0..2800)
        .map(|index| source(&format!("{index}-{suffix}.md"), "# Small\n"))
        .collect();
    let error = BookRenderer::new(&inputs).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("serialized report exceeds 4 MiB")
    );
}

#[cfg(feature = "mcp")]
mod mcp {
    use super::*;
    use franken_markdown::mcp::{
        DEFAULT_MAX_INPUT_BYTES, JsonValue as Json, handle_tool_call, parse_json,
    };

    fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
        Json::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        )
    }

    fn text(value: &str) -> Json {
        Json::String(value.into())
    }

    fn args(markdown: &str) -> Json {
        object([(
            "chapters",
            Json::Array(vec![object([
                ("path", text("chapter.md")),
                ("markdown", text(markdown)),
            ])]),
        )])
    }

    fn call(tool: &str, args: Json) -> Json {
        handle_tool_call(
            Some(&object([("name", text(tool)), ("arguments", args)])),
            DEFAULT_MAX_INPUT_BYTES,
        )
        .unwrap()
    }

    fn reports(result: &Json) -> Vec<Json> {
        let Json::Array(content) = result.get("content").unwrap() else {
            panic!("content")
        };
        content
            .iter()
            .map(|item| parse_json(item.get("text").and_then(Json::as_str).unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn verification_keeps_navigation_first_and_reports_source_fallbacks_separately() {
        let markdown = format!("# Start\n\n{}\n", work_limited());
        let renderer = BookRenderer::new(&[source("chapter.md", &markdown)]).unwrap();
        let result = call("fmd.verify_book", args(&markdown));
        let reports = reports(&result);
        assert_eq!(reports.len(), 2);
        assert_eq!(
            reports[0],
            parse_json(&renderer.validate_links().unwrap().to_json().unwrap()).unwrap()
        );
        assert_eq!(
            reports[1],
            parse_json(&renderer.source_diagnostics().to_json()).unwrap()
        );
        assert_eq!(
            reports[0]
                .get("summary")
                .unwrap()
                .get("findings")
                .and_then(Json::as_u64),
            Some(0)
        );
        assert_eq!(
            reports[1]
                .get("summary")
                .unwrap()
                .get("diagnostics")
                .and_then(Json::as_u64),
            Some(1)
        );
        assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
    }

    #[test]
    fn publication_returns_the_same_source_findings_with_its_epub() {
        let markdown = work_limited();
        let mut args = args(&markdown);
        let Json::Object(fields) = &mut args else {
            panic!("args")
        };
        fields.insert("to".into(), text("epub"));
        fields.insert("strictLinks".into(), Json::Bool(true));
        let result = reports(&call("fmd.render_book", args));
        let expected = BookRenderer::new(&[source("chapter.md", &markdown)]).unwrap();
        assert_eq!(
            result[0].get("sourceDiagnostics"),
            Some(&parse_json(&expected.source_diagnostics().to_json()).unwrap())
        );
        assert!(result[0].get("outputs").is_some());
        let clean = reports(&call("fmd.verify_book", self::args("# Clean\n")));
        assert_eq!(clean.len(), 1, "clean calls retain their original response");
    }
}

#[cfg(feature = "cli")]
mod native {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn workspace(markdown: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "fmd-book-parser-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("book")).unwrap();
        fs::write(root.join("book/chapter.md"), markdown).unwrap();
        root
    }

    fn invoke(root: &std::path::Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "book", "book"])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap()
    }

    #[test]
    fn ordinary_publication_reports_warnings_in_json_and_human_modes() {
        let markdown = format!("# Start\n\n{}\n", work_limited());
        let root = workspace(&markdown);
        let expected = BookRenderer::new(&[source("chapter.md", &markdown)])
            .unwrap()
            .source_diagnostics()
            .to_json();
        let output = invoke(
            &root,
            &[
                "--to",
                "html",
                "--out-dir",
                "site",
                "--json",
                "--deny-broken-links",
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let receipt = String::from_utf8(output.stdout).unwrap();
        assert!(
            receipt.contains(&format!("\"source_diagnostics\":{expected}")),
            "{receipt}"
        );
        assert!(receipt.contains("inline_work_limit"));
        assert!(!receipt.contains("\"warnings\":[]"));
        assert!(
            fs::read_to_string(root.join("site/chapter.html"))
                .unwrap()
                .contains("leaf")
        );
        let human = invoke(&root, &["--to", "html", "--out-dir", "human"]);
        assert!(human.status.success());
        assert!(human.stdout.is_empty());
        let stderr = String::from_utf8(human.stderr).unwrap();
        assert!(stderr.contains("inline_work_limit"));
        assert!(stderr.contains("expanded chapter bytes"));
        assert!(stderr.contains("chapter.md"));
    }

    #[test]
    fn source_check_preserves_link_json_and_emits_the_complete_parser_report_on_stderr() {
        let markdown = work_limited();
        let root = workspace(&markdown);
        let renderer = BookRenderer::new(&[source("chapter.md", &markdown)]).unwrap();
        let output = invoke(&root, &["--check-links", "--json"]);
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            renderer.validate_links().unwrap().to_json().unwrap()
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains(&format!(
                "\"report\":{}",
                renderer.source_diagnostics().to_json()
            )),
            "{stderr}"
        );
        assert!(stderr.contains("\"code\":\"book_source_diagnostics\""));
        assert!(!root.join("site").exists());
    }

    #[test]
    fn refused_diagnostic_report_cannot_replace_existing_publication() {
        let root = workspace(&over_diagnostic_limit());
        fs::create_dir(root.join("site")).unwrap();
        fs::write(root.join("site/chapter.html"), "last good publication").unwrap();
        let output = invoke(
            &root,
            &["--to", "html", "--out-dir", "site", "--json"],
        );
        assert_eq!(output.status.code(), Some(66));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("book_source_diagnostics")
        );
        assert_eq!(
            fs::read_to_string(root.join("site/chapter.html")).unwrap(),
            "last good publication"
        );
    }
}

#[cfg(feature = "wasm-book")]
#[test]
fn browser_binding_returns_diagnostics_from_the_current_native_capture() {
    let markdown = work_limited();
    let mut browser = franken_markdown::book::browser::FmdBook::new(
        vec!["start.md".into()],
        vec![markdown.clone()],
    )
    .unwrap();
    let expected = BookRenderer::new(&[source("start.md", &markdown)]).unwrap();
    assert_eq!(
        browser.source_diagnostics(),
        expected.source_diagnostics().to_json()
    );
    browser
        .update_sources(
            vec!["start.md".into()],
            vec!["# Repaired\n".into()],
            0.0,
        )
        .unwrap();
    let repaired = BookRenderer::new(&[source("start.md", "# Repaired\n")]).unwrap();
    assert_eq!(
        browser.source_diagnostics(),
        repaired.source_diagnostics().to_json()
    );
}
