//! Verification audits the composed source and preserves every input file.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{PdfOptions, Profile, parse_markdown_with_profile, verify};

static NEXT: AtomicU64 = AtomicU64::new(0);
const TTL: &str = "18446744073709551615";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fmd-verify-sources-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(path.join("parts")).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, text: impl AsRef<[u8]>) {
        fs::write(self.0.join(path), text).unwrap();
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "verify"])
            .args(args)
            .arg("--json")
            .current_dir(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(source) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
        } else {
            drop(child.stdin.take());
        }
        child.wait_with_output().unwrap()
    }
}

fn assert_report(output: Output, expanded: &str, profile: Option<Profile>) {
    let report = verify::verify_pdf_result(
        &parse_markdown_with_profile(expanded, profile),
        &PdfOptions {
            profile,
            ..PdfOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        output.status.code(),
        Some(i32::from(report.verdict != "clean")),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        verify::to_json(&report)
    );
    assert!(output.stderr.is_empty());
}

fn assert_error(output: Output, code: i32, detail: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(code), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(stderr.contains(detail), "{stderr}");
}

#[test]
fn expanded_heading_and_nested_selected_prose_share_the_published_report() {
    let workspace = Workspace::new();
    workspace.write(
        "document.md",
        "# Main\n\n[Read](#included)\n\n{{#include parts/body.md:section}}\n",
    );
    workspace.write(
        "parts/body.md",
        "ignored\n<!-- ANCHOR: section -->\n## Included\n\n{{#include more.md:2:3}}\n<!-- ANCHOR_END: section -->\nignored\n",
    );
    workspace.write("parts/more.md", "ignored\nSelected prose.\n\nignored\n");
    assert_report(
        workspace.run(&["document.md"], None),
        "# Main\n\n[Read](#included)\n\n## Included\n\nSelected prose.\n\n",
        None,
    );
}

#[test]
fn findings_inside_includes_are_not_silently_replaced_with_literal_directives() {
    let workspace = Workspace::new();
    workspace.write("document.md", "# Main\n\n{{#include parts/body.md}}\n");
    workspace.write("parts/body.md", "## Included\n\n[Broken](#absent)\n");
    assert_report(
        workspace.run(&["document.md"], None),
        "# Main\n\n## Included\n\n[Broken](#absent)\n",
        None,
    );
}

#[test]
fn included_resource_destinations_keep_their_defining_directory() {
    let workspace = Workspace::new();
    workspace.write("document.md", "# Main\n\n{{#include parts/body.md}}\n");
    workspace.write("parts/body.md", "![Plot](plot.svg)\n");
    assert_report(
        workspace.run(&["document.md"], None),
        "# Main\n\n![Plot](parts/plot.svg)\n",
        None,
    );
}

#[test]
fn unresolved_cyclic_and_invalid_source_graphs_fail_without_a_partial_report_or_cache() {
    let workspace = Workspace::new();
    for (body, detail) in [
        ("{{#include missing.md}}\n", "include_missing"),
        ("{{#include body.md}}\n", "include_cycle"),
    ] {
        workspace.write("document.md", "# Main\n\n{{#include parts/body.md}}\n");
        workspace.write("parts/body.md", body);
        assert_error(
            workspace.run(
                &["document.md", "--links", "--links-cache", "cache.jsonl"],
                None,
            ),
            66,
            detail,
        );
        assert!(!workspace.0.join("cache.jsonl").exists());
    }
    workspace.write("parts/body.md", [0xff, 0xfe]);
    assert_error(
        workspace.run(&["document.md"], None),
        66,
        "include_invalid_utf8",
    );
}

#[cfg(unix)]
#[test]
fn escaping_include_aliases_are_refused_before_reading_their_contents() {
    let workspace = Workspace::new();
    let outside = Workspace::new();
    outside.write("outside.md", "Outside material.\n");
    std::os::unix::fs::symlink(outside.0.join("outside.md"), workspace.0.join("alias.md")).unwrap();
    workspace.write("document.md", "{{#include alias.md}}\n");
    assert_error(workspace.run(&["document.md"], None), 66, "include_escape");
}

#[test]
fn input_and_expansion_budgets_apply_before_verification() {
    let workspace = Workspace::new();
    workspace.write("document.md", "{{#include parts/body.md}}\n");
    workspace.write("parts/body.md", "x".repeat(81));
    assert_error(
        workspace.run(&["document.md", "--max-input-bytes", "80"], None),
        66,
        "include_oversize",
    );
    workspace.write(
        "parts/body.md",
        "small input repeated until the combined document is too large.\n",
    );
    workspace.write(
        "document.md",
        "{{#include parts/body.md}}\n{{#include parts/body.md}}\n",
    );
    assert_error(
        workspace.run(&["document.md", "--max-input-bytes", "80"], None),
        66,
        "include_size",
    );
    assert_error(
        workspace.run(&["document.md", "--max-input-bytes", "1"], None),
        66,
        "input",
    );
}

#[test]
fn stdin_refuses_active_includes_but_code_examples_remain_literal() {
    let workspace = Workspace::new();
    assert_error(
        workspace.run(&["-"], Some("{{#include secret.md}}\n")),
        64,
        "needs a file input",
    );
    for source in [
        "```md\n{{#include missing.md}}\n```\n",
        "Example `\n{{#include missing.md}}\n`\n",
        "> ```md\n> {{#include missing.md}}\n> ```\n",
    ] {
        assert_report(workspace.run(&["-"], Some(source)), source, None);
    }
}

#[test]
fn authoring_profile_owns_both_expansion_and_verification() {
    let workspace = Workspace::new();
    let source = "Term\n: Definition\n";
    for name in ["commonmark-gfm", "gfm-plus"] {
        let profile = Profile::parse(name).unwrap();
        workspace.write("document.md", "{{#include parts/body.md}}\n");
        workspace.write("parts/body.md", source);
        assert_report(
            workspace.run(&["document.md", "--profile", name], None),
            source,
            Some(profile),
        );
    }
    // In the CommonMark grammar this is one inline code span; treating the
    // colon line as an extended definition would expose the literal include.
    let literal = "Term `\n: definition\n{{#include missing.md}}\nend `\n";
    assert_report(
        workspace.run(&["-", "--profile", "commonmark-gfm"], Some(literal)),
        literal,
        Profile::parse("commonmark-gfm"),
    );
    assert_error(
        workspace.run(&["does-not-exist.md", "--profile", "unknown"], None),
        64,
        "unknown markdown authoring profile",
    );
}

#[test]
fn link_cache_cannot_replace_any_included_source_or_its_aliases() {
    let workspace = Workspace::new();
    let source = "# Included\n\n[Reference](https://example.invalid/reference)\n";
    workspace.write("document.md", "{{#include parts/body.md}}\n");
    workspace.write("parts/body.md", source);
    let mut caches = vec!["parts/body.md", "parts/./body.md"];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("parts/body.md", workspace.0.join("alias.jsonl")).unwrap();
        fs::hard_link(
            workspace.0.join("parts/body.md"),
            workspace.0.join("hard.jsonl"),
        )
        .unwrap();
        caches.extend(["alias.jsonl", "hard.jsonl"]);
    }
    for cache in caches {
        assert_error(
            workspace.run(&["document.md", "--links", "--links-cache", cache], None),
            64,
            "refusing to overwrite verify input",
        );
        assert_eq!(
            fs::read_to_string(workspace.0.join("parts/body.md")).unwrap(),
            source
        );
    }
}

#[test]
fn external_checks_cover_tables_definitions_notes_and_supported_html_without_fetching_literals() {
    let workspace = Workspace::new();
    let source = concat!(
        "# Main\n\n",
        "| [Header](https://example.invalid/header) |\n| --- |\n",
        "| [Cell](https://example.invalid/cell) |\n\n",
        "[Term](https://example.invalid/term)\n: [Definition](https://example.invalid/definition)\n\n",
        "Cite[^n].\n\n[^n]: [Note](https://example.invalid/note)\n\n",
        "[^unused]: [Uncited note](https://example.invalid/uncited)\n\n",
        "<a href=\"https://example.invalid/html\">HTML reference</a>\n\n",
        "[Duplicate](https://example.invalid/cell)\n\n",
        "`[literal](https://example.invalid/never)`\n",
        "![Figure](https://example.invalid/image)\n",
    );
    workspace.write("document.md", "{{#include parts/body.md}}\n");
    workspace.write("parts/body.md", source);
    let urls = [
        "header",
        "cell",
        "term",
        "definition",
        "note",
        "uncited",
        "html",
    ];
    let cache = urls.iter().map(|name| format!(
        "{{\"url\":\"https://example.invalid/{name}\",\"status\":404,\"ok\":false,\"checked\":0}}\n"
    )).collect::<String>();
    workspace.write("cache.jsonl", cache);
    let output = workspace.run(
        &[
            "document.md",
            "--links",
            "--links-cache",
            "cache.jsonl",
            "--links-ttl-secs",
            TTL,
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let report = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        report.matches("\"code\":\"link_broken\"").count(),
        urls.len(),
        "{report}"
    );
    let updated = fs::read_to_string(workspace.0.join("cache.jsonl")).unwrap();
    assert_eq!(updated.lines().count(), urls.len(), "{updated}");
    for url in urls {
        assert!(
            updated.contains(&format!("https://example.invalid/{url}\"")),
            "{updated}"
        );
    }
    assert!(!updated.contains("/never"));
    assert!(!updated.contains("/image"));
    assert_eq!(
        fs::read_to_string(workspace.0.join("parts/body.md")).unwrap(),
        source
    );
}

#[test]
fn parser_findings_identify_expanded_coordinates() {
    let workspace = Workspace::new();
    workspace.write("document.md", "# Main\n\n{{#include parts/body.md}}\n");
    workspace.write("parts/body.md", "```rust\nunclosed\n");
    let output = workspace.run(&["document.md"], None);
    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("expanded source bytes"), "{report}");
    assert!(report.contains("unclosed_fence"), "{report}");
}
