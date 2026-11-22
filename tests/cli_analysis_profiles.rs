//! Document intelligence and revision review must use the publication dialect.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{Theme, parse_markdown};

const LINK: &str = "[Cross-line β\r\n: link](#missing)\r\n";
const OLD: &str = "*Term\n: old value*\n";
const NEW: &str = "*Term\n: new value*\n";

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-analysis-profiles-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command.current_dir(&self.0).arg("--no-config").args(args);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn stdin(&self, args: &[&str], source: &str) -> Output {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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

#[test]
fn stats_uses_the_selected_grammar_before_cross_line_delimiters_are_resolved() {
    let directory = Directory::new();
    directory.write("source.md", LINK);
    for (profile, paragraphs, links) in [("commonmark-gfm", 1, 1), ("gfm-plus", 0, 0)] {
        let raw = text(directory.run(&["stats", "--text", LINK, "--profile", profile, "--json"]));
        assert!(
            raw.contains(&format!("\"paragraphs\":{paragraphs}")),
            "{raw}"
        );
        assert!(raw.contains(&format!("\"links_total\":{links}")), "{raw}");
        assert_eq!(raw.contains("broken_internal_anchor"), links == 1, "{raw}");
        assert!(raw.contains(&format!("\"bytes\":{}", LINK.len())), "{raw}");
        assert_eq!(
            raw,
            text(directory.run(&["stats", "source.md", "--profile", profile, "--json"]))
        );
        assert_eq!(
            raw,
            text(directory.stdin(&["stats", "-", "--profile", profile, "--json"], LINK))
        );
    }
    assert_eq!(
        std::fs::read(directory.0.join("source.md")).unwrap(),
        LINK.as_bytes()
    );
}

#[test]
fn stats_applies_explicit_page_break_semantics_and_keeps_legacy_reports() {
    let directory = Directory::new();
    let source = "<!-- pagebreak -->\n";
    let legacy = franken_markdown::doc_stats::compute_doc_stats(source, &parse_markdown(source));
    assert_eq!(
        text(directory.run(&["stats", "--text", source, "--json"])),
        format!("{}\n", legacy.to_json())
    );
    assert_eq!(
        text(directory.run(&["stats", "--text", source])),
        format!("{}\n", legacy.to_human_report())
    );
    let extended =
        text(directory.run(&["stats", "--text", source, "--profile", "gfm-plus", "--json"]));
    assert!(extended.contains("\"characters\":0"), "{extended}");
    let ordinary = text(directory.run(&[
        "stats",
        "--text",
        source,
        "--profile",
        "commonmark-gfm",
        "--json",
    ]));
    assert_eq!(ordinary, format!("{}\n", legacy.to_json()));
}

#[test]
fn diff_selects_one_dialect_for_both_revisions_and_its_html_report() {
    let directory = Directory::new();
    directory.write("old.md", OLD);
    directory.write("new.md", NEW);
    for (profile, modified, inserted, deleted) in
        [("commonmark-gfm", 1, 0, 0), ("gfm-plus", 0, 1, 1)]
    {
        let json =
            text(directory.run(&["diff", "old.md", "new.md", "--profile", profile, "--json"]));
        for (name, count) in [
            ("modified_blocks", modified),
            ("inserted_blocks", inserted),
            ("deleted_blocks", deleted),
        ] {
            assert!(json.contains(&format!("\"{name}\":{count}")), "{json}");
        }
        let html = text(directory.run(&["diff", "old.md", "new.md", "--profile", profile]));
        assert_eq!(html.contains("<dl>"), profile == "gfm-plus", "{profile}");
        if profile == "commonmark-gfm" {
            assert!(html.contains("<em>Term\n: old value</em>"));
            assert!(html.contains("<em>Term\n: new value</em>"));
        }
        let written = directory.run(&[
            "diff",
            "old.md",
            "new.md",
            "--profile",
            profile,
            "--out",
            "review.html",
        ]);
        assert!(written.status.success());
        assert!(written.stdout.is_empty());
        assert_eq!(
            std::fs::read_to_string(directory.0.join("review.html")).unwrap() + "\n",
            html
        );
        assert_eq!(
            text(directory.stdin(
                &["diff", "-", "new.md", "--profile", profile, "--json"],
                OLD,
            )),
            json.replace("\"old_name\":\"old.md\"", "\"old_name\":\"-\"")
        );
    }
    assert_eq!(
        std::fs::read(directory.0.join("old.md")).unwrap(),
        OLD.as_bytes()
    );
    assert_eq!(
        std::fs::read(directory.0.join("new.md")).unwrap(),
        NEW.as_bytes()
    );
}

#[test]
fn profiled_diff_compares_authored_page_breaks_and_preserves_omitted_defaults() {
    let directory = Directory::new();
    let old = "<!-- pagebreak -->\n";
    let new = "  <!-- pagebreak -->\t\n";
    directory.write("old.md", old);
    directory.write("new.md", new);
    let legacy = franken_markdown::diff::compute_diff(
        &parse_markdown(old),
        &parse_markdown(new),
        "old.md",
        "new.md",
    );
    assert_eq!(
        text(directory.run(&["diff", "old.md", "new.md", "--json"])),
        format!("{}\n", legacy.to_json())
    );
    assert_eq!(
        text(directory.run(&["diff", "old.md", "new.md"])),
        format!("{}\n", legacy.to_html(&Theme::default()))
    );
    let extended = text(directory.run(&[
        "diff",
        "old.md",
        "new.md",
        "--profile",
        "gfm-plus",
        "--json",
    ]));
    assert!(extended.contains("\"unchanged_blocks\":1"), "{extended}");
    assert!(extended.contains("\"inserted_blocks\":0"), "{extended}");
    assert!(extended.contains("\"deleted_blocks\":0"), "{extended}");
    assert!(
        extended.contains("\"similarity_ratio\":1.000"),
        "{extended}"
    );
    assert_eq!(
        text(directory.run(&[
            "diff",
            "old.md",
            "new.md",
            "--profile",
            "commonmark-gfm",
            "--json",
        ])),
        format!("{}\n", legacy.to_json())
    );
}

#[test]
fn invalid_profiles_fail_before_input_or_output_and_valid_profiles_keep_bounds() {
    let directory = Directory::new();
    directory.write("review.html", "previous report");
    for args in [
        vec!["stats", "missing.md", "--profile", "unknown", "--json"],
        vec![
            "diff",
            "missing.md",
            "also-missing.md",
            "--profile",
            "unknown",
            "--out",
            "review.html",
            "--json",
        ],
    ] {
        let output = directory.run(&args);
        assert_eq!(output.status.code(), Some(64));
        assert!(output.stdout.is_empty());
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(
            diagnostic.contains("unknown markdown authoring profile"),
            "{diagnostic}"
        );
        assert!(!diagnostic.contains("input_error"));
    }
    assert_eq!(
        std::fs::read_to_string(directory.0.join("review.html")).unwrap(),
        "previous report"
    );
    for profile in ["commonmark-gfm", "gfm-plus"] {
        let output = directory.run(&[
            "stats",
            "--text",
            LINK,
            "--profile",
            profile,
            "--max-input-bytes",
            "4",
            "--json",
        ]);
        assert_eq!(output.status.code(), Some(66));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("input_error"));
    }
}

#[test]
fn help_discovers_the_same_dialect_choices_for_stats_and_diff() {
    let directory = Directory::new();
    for command in ["stats", "diff"] {
        let help = text(directory.run(&[command, "--help"]));
        assert!(help.contains("--profile <PROFILE>"), "{help}");
        assert!(help.contains("commonmark-gfm"), "{help}");
        assert!(help.contains("gfm-plus"), "{help}");
    }
}
