//! Opt-in analysis follows the bounded native include graph without rewriting it.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{Document, Profile, apply_profile, parse_markdown_with_profile};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fmd-analysis-includes-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, content: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.0.join("config"))
            .args(args)
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

    fn bytes(&self, name: &str) -> Vec<u8> {
        fs::read(self.0.join(name)).unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn refused(output: Output, code: i32, detail: &str) {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(detail), "{stderr}");
}

fn document(source: &str, profile: Option<Profile>) -> Document {
    let parsed = parse_markdown_with_profile(source, profile);
    if profile.is_some() {
        apply_profile(&parsed, profile).into_owned()
    } else {
        parsed
    }
}

fn stats(source: &str, profile: Option<Profile>) -> String {
    format!(
        "{}\n",
        franken_markdown::doc_stats::compute_doc_stats(source, &document(source, profile))
            .to_json()
    )
}

fn diff(old: &str, new: &str, old_name: &str, new_name: &str) -> String {
    format!(
        "{}\n",
        franken_markdown::diff::compute_diff(
            &document(old, None),
            &document(new, None),
            old_name,
            new_name,
        )
        .to_json()
    )
}

#[test]
fn stats_counts_nested_selected_content_and_resolves_its_headings() {
    let dir = Workspace::new();
    let source = "# Main\n\n[Read](#included)\n\n{{#include parts/body.md:section}}\n";
    dir.write("main.md", source);
    dir.write(
        "parts/body.md",
        "{{#include unselected-missing.md}}\n<!-- ANCHOR: section -->\n## Included\n\n{{#include more.md:2:3}}\n\n[Chapter](chapter.md#topic)\n<!-- ANCHOR_END: section -->\nignored\n",
    );
    dir.write("parts/more.md", "ignored\nSelected prose.\n\nignored\n");
    let expanded = "# Main\n\n[Read](#included)\n\n## Included\n\nSelected prose.\n\n\n[Chapter](parts/chapter.md#topic)\n";
    let reported = text(dir.run(&["stats", "main.md", "--expand-includes", "--json"], None));
    assert_eq!(reported, stats(expanded, None));
    assert!(reported.contains("\"headings_total\":2"), "{reported}");
    assert!(!reported.contains("broken_internal_anchor"), "{reported}");
    assert!(!reported.contains("unselected-missing"), "{reported}");
    let raw = text(dir.run(&["stats", "main.md", "--json"], None));
    assert_eq!(raw, stats(source, None));
    assert!(raw.contains("broken_internal_anchor"), "{raw}");
    assert_eq!(dir.bytes("main.md"), source.as_bytes());
}

#[test]
fn diff_sees_changed_bodies_behind_identical_directives_in_separate_revision_roots() {
    let dir = Workspace::new();
    let source = "# Manual\n\n{{#include parts/body.md}}\n";
    let old_body = "## Chapter\n\nA shared sentence with the old conclusion.\n";
    let new_body = "## Chapter\n\nA shared sentence with the new conclusion.\n";
    dir.write("before/main.md", source);
    dir.write("after/main.md", source);
    dir.write("before/parts/body.md", old_body);
    dir.write("after/parts/body.md", new_body);
    let old = format!("# Manual\n\n{old_body}");
    let new = format!("# Manual\n\n{new_body}");

    let raw = text(dir.run(&["diff", "before/main.md", "after/main.md", "--json"], None));
    assert_eq!(raw, diff(source, source, "before/main.md", "after/main.md"));
    assert!(raw.contains("\"modified_blocks\":0"), "{raw}");
    let expanded = text(dir.run(
        &[
            "diff",
            "before/main.md",
            "after/main.md",
            "--expand-includes",
            "--json",
        ],
        None,
    ));
    assert_eq!(
        expanded,
        diff(&old, &new, "before/main.md", "after/main.md")
    );
    assert!(expanded.contains("\"modified_blocks\":1"), "{expanded}");

    text(dir.run(
        &[
            "diff",
            "before/main.md",
            "after/main.md",
            "--expand-includes",
            "--out",
            "review.html",
        ],
        None,
    ));
    let html = String::from_utf8(dir.bytes("review.html")).unwrap();
    assert!(!html.contains("{{#include"), "{html}");
    assert!(html.contains("Chapter"));
    assert!(html.contains("old conclusion"));
    assert!(html.contains("new conclusion"));
    assert_eq!(dir.bytes("before/main.md"), source.as_bytes());
    assert_eq!(dir.bytes("after/main.md"), source.as_bytes());
    assert_eq!(dir.bytes("before/parts/body.md"), old_body.as_bytes());
    assert_eq!(dir.bytes("after/parts/body.md"), new_body.as_bytes());
}

#[test]
fn omission_keeps_raw_analysis_and_does_not_require_the_include_graph() {
    let dir = Workspace::new();
    let source = "# Report\n\n{{#include missing.md}}\n";
    dir.write("old.md", source);
    dir.write("new.md", source);
    assert_eq!(
        text(dir.run(&["stats", "old.md", "--json"], None)),
        stats(source, None)
    );
    assert_eq!(
        text(dir.run(&["diff", "old.md", "new.md", "--json"], None)),
        diff(source, source, "old.md", "new.md")
    );
    refused(
        dir.run(&["stats", "old.md", "--expand-includes", "--json"], None),
        66,
        "include_missing",
    );
    refused(
        dir.run(
            &["diff", "old.md", "new.md", "--expand-includes", "--json"],
            None,
        ),
        66,
        "include_missing",
    );
}

#[test]
fn selected_and_configured_profiles_control_expansion_before_analysis() {
    let dir = Workspace::new();
    let literal = "`Example\n: code\n{{#include missing.md}}\ntail`\n";
    dir.write("old.md", literal);
    dir.write("new.md", literal);
    dir.write("config", "profile=commonmark-gfm\n");
    let expected = stats(literal, Some(Profile::CommonMarkGfm));
    assert_eq!(
        text(dir.run(&["stats", "old.md", "--expand-includes", "--json"], None)),
        expected
    );
    let configured_diff = text(dir.run(
        &["diff", "old.md", "new.md", "--expand-includes", "--json"],
        None,
    ));
    assert!(configured_diff.contains("\"modified_blocks\":0"));
    for args in [
        vec![
            "stats",
            "old.md",
            "--expand-includes",
            "--profile",
            "gfm-plus",
            "--json",
        ],
        vec![
            "diff",
            "old.md",
            "new.md",
            "--expand-includes",
            "--profile",
            "gfm-plus",
            "--json",
        ],
        vec![
            "--no-config",
            "stats",
            "old.md",
            "--expand-includes",
            "--json",
        ],
    ] {
        refused(dir.run(&args, None), 66, "include_missing");
    }
    dir.write("config", "profile=gfm-plus\n");
    assert_eq!(
        text(dir.run(
            &[
                "stats",
                "old.md",
                "--expand-includes",
                "--profile",
                "commonmark-gfm",
                "--json"
            ],
            None,
        )),
        expected
    );
    assert_eq!(
        text(dir.run(
            &[
                "diff",
                "old.md",
                "new.md",
                "--expand-includes",
                "--profile",
                "commonmark-gfm",
                "--json"
            ],
            None,
        )),
        configured_diff
    );
}

#[test]
fn stdin_and_text_reject_only_active_includes_even_with_a_positional_path() {
    let dir = Workspace::new();
    for literal in [
        "```md\n{{#include missing.md}}\n```\n",
        "Example `\n{{#include missing.md}}\n`\n",
        "> ```md\n> {{#include missing.md}}\n> ```\n",
        "`Example\n: code\n{{#include missing.md}}\ntail`\n",
    ] {
        dir.write("same.md", literal);
        let expected = stats(literal, Some(Profile::CommonMarkGfm));
        assert_eq!(
            text(dir.run(
                &[
                    "stats",
                    "-",
                    "--expand-includes",
                    "--profile",
                    "commonmark-gfm",
                    "--json"
                ],
                Some(literal),
            )),
            expected
        );
        assert_eq!(
            text(dir.run(
                &[
                    "stats",
                    "unused.md",
                    "--text",
                    literal,
                    "--expand-includes",
                    "--profile",
                    "commonmark-gfm",
                    "--json"
                ],
                None,
            )),
            expected
        );
        for inputs in [["-", "same.md"], ["same.md", "-"]] {
            let report = text(dir.run(
                &[
                    "diff",
                    inputs[0],
                    inputs[1],
                    "--expand-includes",
                    "--profile",
                    "commonmark-gfm",
                    "--json",
                ],
                Some(literal),
            ));
            assert!(report.contains("\"inserted_blocks\":0"), "{report}");
            assert!(report.contains("\"deleted_blocks\":0"), "{report}");
            assert!(report.contains("\"modified_blocks\":0"), "{report}");
        }
    }

    let active = "{{#include body.md}}\n";
    dir.write("same.md", "# Comparison\n");
    dir.write(
        "body.md",
        "# Present on disk but not authorized by stdin/text\n",
    );
    for (args, stdin) in [
        (
            vec!["stats", "-", "--expand-includes", "--json"],
            Some(active),
        ),
        (
            vec![
                "stats",
                "same.md",
                "--text",
                active,
                "--expand-includes",
                "--json",
            ],
            None,
        ),
        (
            vec!["diff", "-", "same.md", "--expand-includes", "--json"],
            Some(active),
        ),
        (
            vec!["diff", "same.md", "-", "--expand-includes", "--json"],
            Some(active),
        ),
    ] {
        refused(dir.run(&args, stdin), 64, "needs a file input");
    }
}

#[test]
fn incomplete_or_invalid_include_graphs_never_publish_partial_reports() {
    let dir = Workspace::new();
    dir.write("main.md", "{{#include parts/body.md}}\n");
    dir.write("other.md", "# Comparison\n");
    dir.write("review.html", "previous report");
    for (body, error) in [
        ("{{#include missing.md}}\n".as_bytes(), "include_missing"),
        ("{{#include body.md}}\n".as_bytes(), "include_cycle"),
        (&[0xff, 0xfe][..], "include_invalid_utf8"),
    ] {
        dir.write("parts/body.md", body);
        refused(
            dir.run(&["stats", "main.md", "--expand-includes", "--json"], None),
            66,
            error,
        );
        refused(
            dir.run(
                &[
                    "diff",
                    "main.md",
                    "other.md",
                    "--expand-includes",
                    "--out",
                    "review.html",
                ],
                None,
            ),
            66,
            error,
        );
        assert_eq!(dir.bytes("review.html"), b"previous report");
    }
}

#[test]
fn source_and_expansion_byte_limits_apply_to_each_analyzed_revision() {
    let dir = Workspace::new();
    dir.write("other.md", "# Comparison\n");
    dir.write("review.html", "previous report");
    for (source, body, limit, error) in [
        (
            "{{#include parts/body.md}}\n",
            "x".repeat(81),
            "80",
            "include_oversize",
        ),
        (
            "{{#include parts/body.md}}\n{{#include parts/body.md}}\n",
            "small input repeated until the combined document is too large.\n".to_string(),
            "80",
            "include_size",
        ),
        (
            "{{#include parts/body.md}}\n",
            "small\n".to_string(),
            "1",
            "input",
        ),
    ] {
        dir.write("main.md", source);
        dir.write("parts/body.md", body);
        refused(
            dir.run(
                &[
                    "stats",
                    "main.md",
                    "--expand-includes",
                    "--max-input-bytes",
                    limit,
                    "--json",
                ],
                None,
            ),
            66,
            error,
        );
        for inputs in [["main.md", "other.md"], ["other.md", "main.md"]] {
            refused(
                dir.run(
                    &[
                        "diff",
                        inputs[0],
                        inputs[1],
                        "--expand-includes",
                        "--max-input-bytes",
                        limit,
                        "--out",
                        "review.html",
                    ],
                    None,
                ),
                66,
                error,
            );
            assert_eq!(dir.bytes("review.html"), b"previous report");
        }
    }
}

#[test]
fn diff_output_cannot_replace_primary_included_or_configured_inputs_and_aliases() {
    let dir = Workspace::new();
    let primary = "{{#include parts/body.md:visible}}\n";
    let container =
        "ignored\n<!-- ANCHOR: visible -->\n{{#include more.md}}\n<!-- ANCHOR_END: visible -->\n";
    for side in ["before", "after"] {
        dir.write(&format!("{side}/main.md"), primary);
        dir.write(&format!("{side}/parts/body.md"), container);
        dir.write(&format!("{side}/parts/more.md"), format!("# {side}\n"));
    }
    dir.write("config", "profile=commonmark-gfm\n");
    let inputs = [
        "before/main.md",
        "after/main.md",
        "before/parts/body.md",
        "after/parts/body.md",
        "before/parts/more.md",
        "after/parts/more.md",
        "config",
    ];
    let originals: Vec<_> = inputs.iter().map(|name| dir.bytes(name)).collect();
    let aliases = ["before/parts/./body.md", "after/parts/../parts/more.md"];
    let mut destinations = inputs.to_vec();
    destinations.extend(aliases);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("before/parts/more.md", dir.0.join("alias.html")).unwrap();
        fs::hard_link(dir.0.join("after/parts/body.md"), dir.0.join("hard.html")).unwrap();
        std::os::unix::fs::symlink("config", dir.0.join("config-alias.html")).unwrap();
        destinations.extend(["alias.html", "hard.html", "config-alias.html"]);
    }
    for output in destinations {
        refused(
            dir.run(
                &[
                    "diff",
                    "before/main.md",
                    "after/main.md",
                    "--expand-includes",
                    "--out",
                    output,
                ],
                None,
            ),
            64,
            "refusing to overwrite diff input",
        );
        for (name, original) in inputs.iter().zip(&originals) {
            assert_eq!(&dir.bytes(name), original, "{name}");
        }
    }
}

#[test]
fn unselected_include_targets_are_not_read_or_claimed_as_output_inputs() {
    let dir = Workspace::new();
    dir.write("main.md", "{{#include parts/body.md:visible}}\n");
    dir.write(
        "parts/body.md",
        "{{#include ../review.html}}\n<!-- ANCHOR: visible -->\n# Selected\n<!-- ANCHOR_END: visible -->\n",
    );
    dir.write("review.html", "previous report");
    text(dir.run(
        &[
            "diff",
            "main.md",
            "main.md",
            "--expand-includes",
            "--out",
            "review.html",
        ],
        None,
    ));
    let report = String::from_utf8(dir.bytes("review.html")).unwrap();
    assert!(report.contains("Selected"));
    assert!(!report.contains("{{#include"));
}

#[cfg(unix)]
#[test]
fn expansion_refuses_include_aliases_outside_the_revision_root() {
    let dir = Workspace::new();
    let outside = Workspace::new();
    outside.write("secret.md", [0xff, 0xfe]);
    std::os::unix::fs::symlink(outside.0.join("secret.md"), dir.0.join("alias.md")).unwrap();
    dir.write("main.md", "{{#include alias.md}}\n");
    dir.write("other.md", "# Comparison\n");
    dir.write("review.html", "previous report");
    refused(
        dir.run(&["stats", "main.md", "--expand-includes", "--json"], None),
        66,
        "include_escape",
    );
    refused(
        dir.run(
            &[
                "diff",
                "main.md",
                "other.md",
                "--expand-includes",
                "--out",
                "review.html",
            ],
            None,
        ),
        66,
        "include_escape",
    );
    assert_eq!(dir.bytes("review.html"), b"previous report");
    assert_eq!(outside.bytes("secret.md"), [0xff, 0xfe]);
}
