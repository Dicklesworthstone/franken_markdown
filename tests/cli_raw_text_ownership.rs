//! Raw Markdown is one opaque CLI value, even when it resembles an option.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Output};

use franken_markdown::{HtmlOptions, parse_markdown};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .arg("--no-config")
        .args(args)
        .env_remove("SOURCE_DATE_EPOCH")
        .output()
        .unwrap()
}

fn check_html(output: Output, source: &str) {
    assert!(
        output.status.success(),
        "source={source:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let html = String::from_utf8(output.stdout).unwrap();
    let body = html
        .split_once("<main class=\"fmd\">\n")
        .expect("HTML document body")
        .1
        .split_once("</main>")
        .unwrap()
        .0;
    let expected = franken_markdown::html::render_fragment(
        &parse_markdown(source).blocks,
        &HtmlOptions::default(),
    );
    assert_eq!(body.trim_end(), expected.trim_end(), "{source:?}");
}

#[test]
fn raw_markdown_accepts_lists_rules_and_frontmatter_without_equals_syntax() {
    for source in [
        "- first\n- second\n",
        "- [x] done\n- [ ] pending\n",
        "---\n\nA rule.\n",
        "---\ntitle: Example\n---\n# Title\n",
        "-     [x] literal code\n",
        "--option-looking prose\n",
    ] {
        check_html(run(&["--text", source, "--out", "-"]), source);
        check_html(run(&["render", "--text", source, "--out", "-"]), source);
    }
}

#[test]
fn raw_option_spellings_and_aliases_are_never_executed_or_rewritten() {
    for source in [
        "--help",
        "--version",
        "--",
        "--jsno",
        "--jsoon",
        "--jason",
        "--json=true",
        "--no-colour",
        "--colour=never",
        "--color=never",
    ] {
        check_html(run(&["--text", source, "--out", "-"]), source);
        check_html(run(&["render", "--text", source, "--out", "-"]), source);
    }
}

#[test]
fn stats_consumes_the_identical_raw_text_with_separate_or_attached_values() {
    for source in ["- first\n- second", "--no-colour", "--json=true", "--"] {
        let separate = run(&["stats", "--text", source, "--json"]);
        let attached = run(&["stats", &format!("--text={source}"), "--json"]);
        assert!(separate.status.success(), "{source:?}: {separate:?}");
        assert!(attached.status.success(), "{source:?}: {attached:?}");
        assert_eq!(separate.stdout, attached.stdout, "{source:?}");
        assert!(separate.stderr.is_empty());
    }
}

#[test]
fn real_alias_flags_still_work_on_both_sides_of_an_opaque_source_value() {
    for alias in ["--jsno", "--jsoon", "--jason", "--json=true"] {
        check_html(run(&[alias, "--text", "--jason", "--out", "-"]), "--jason");
        check_html(
            run(&["render", "--text", "--no-colour", alias, "--out", "-"]),
            "--no-colour",
        );
        let stats = run(&[alias, "stats", "--text", "--jason"]);
        assert!(stats.status.success(), "{alias}: {stats:?}");
        let expected = run(&["stats", "--text=--jason", "--json"]);
        assert_eq!(stats.stdout, expected.stdout);
    }
}

#[test]
fn raw_text_still_enforces_input_limits_and_parses_later_options() {
    let output = run(&[
        "--text",
        "- oversized item",
        "--max-input-bytes",
        "4",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(66));
    assert!(output.stdout.is_empty());
    let diagnostics = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostics.contains("input_error"));
    assert!(diagnostics.contains("exceeds --max-input-bytes 4"));

    let output = run(&["--text", "- item", "--to", "svg"]);
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("<svg"));
    assert_eq!(run(&["--text"]).status.code(), Some(64));
}

#[test]
fn end_of_options_preserves_option_shaped_file_names() {
    let directory = std::env::temp_dir().join(format!(
        "fmd-raw-text-ownership-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let source = "# Literal file name\n";
    std::fs::write(directory.join("--jason"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "--out", "-", "--", "--jason"])
        .current_dir(&directory)
        .env_remove("SOURCE_DATE_EPOCH")
        .output()
        .unwrap();
    check_html(output, source);
}
