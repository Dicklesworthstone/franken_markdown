//! Batch inputs use the same bounded, source-aware includes as native render.
#![cfg(feature = "batch")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#2468ac"/></svg>"##;

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-batch-transclusion-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .arg("--no-config")
            .env("SOURCE_DATE_EPOCH", "1700000000");
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn batch(&self, args: &[&str]) -> Output {
        self.command()
            .arg("batch")
            .args(args)
            .arg("--json")
            .output()
            .unwrap()
    }
    fn bytes(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.path(name)).unwrap()
    }
    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}; stdout={}; stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn input_failed(output: &Output, reason: &str) {
    assert_eq!(
        output.status.code(),
        Some(66),
        "stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(reason),
        "receipt: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn recursive_selected_includes_and_encoded_assets_match_single_render_in_every_format() {
    let dir = Workspace::new();
    dir.write(
        "root.md",
        "# Root\n\n{{#include \"parts #中/chapter.txt\":shown}}\n\n```md\n{{#include literal-missing.txt}}\n```\n",
    );
    dir.write(
        "parts #中/chapter.txt",
        "{{#include unselected-missing.txt}}\n<!-- ANCHOR: shown -->\n{{#include deep/cards.txt:2:6}}\n<!-- ANCHOR_END: shown -->\n",
    );
    dir.write(
        "parts #中/deep/cards.txt",
        "```md\n## Included selected\n\n![Chart](percent%2520%23plot.svg?raw=1#panel)\n\n[Next](../next.md?view=1#target)\n```\n",
    );
    dir.write("parts #中/deep/percent%20#plot.svg", SVG);
    dir.write("percent%20#plot.svg", "wrong asset in the root directory");

    for target in ["html", "pdf", "both", "epub", "svg"] {
        let extension = if target == "both" { "html" } else { target };
        let expected = format!("single.{extension}");
        succeeded(&dir.run(&["root.md", "--to", target, "--out", &expected]));
        succeeded(&dir.batch(&["root.md", "--to", target, "--out-dir", "batch"]));
        assert_eq!(
            dir.bytes(&format!("batch/root.{extension}")),
            dir.bytes(&expected),
            "target {target}"
        );
        if target == "both" {
            assert_eq!(dir.bytes("batch/root.pdf"), dir.bytes("single.pdf"));
        }
    }
    let html = dir.text("batch/root.html");
    assert!(html.contains("Included selected"));
    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(html.contains("href=\"parts%20%23%E4%B8%AD/next.md?view=1#target\""));
    assert!(html.contains("{{#include literal-missing.txt}}"));
    assert!(!html.contains("unselected-missing"));
}

#[test]
fn included_reference_definitions_rebase_both_root_and_fragment_image_uses() {
    let dir = Workspace::new();
    dir.write(
        "root.md",
        "![Root use][chart]\n\n{{#include parts/shared.txt}}\n",
    );
    dir.write(
        "parts/shared.txt",
        "![Included use][chart]\n\n[chart]: <figure.svg?raw=1#panel> 'Shared chart'\n",
    );
    dir.write("parts/figure.svg", SVG);
    succeeded(&dir.run(&["root.md", "--out", "single.html"]));
    succeeded(&dir.batch(&["root.md", "--out-dir", "batch"]));
    let html = dir.text("batch/root.html");
    assert_eq!(html, dir.text("single.html"));
    assert_eq!(html.matches("data:image/svg+xml;base64,").count(), 2);
    assert_eq!(html.matches("title=\"Shared chart\"").count(), 2);
}

#[test]
fn missing_includes_are_deterministic_per_file_errors_with_continue_on_error() {
    let dir = Workspace::new();
    dir.write("a-good.md", "# Good\n\n{{#include shared.txt}}\n");
    dir.write("shared.txt", "The shared content was expanded.\n");
    dir.write("b-bad.md", "{{#include parts/missing.txt}}\n");
    dir.write("out/b-bad.html", "previous failed-input publication");
    let args = [
        "a-good.md",
        "b-bad.md",
        "--out-dir",
        "out",
        "--workers",
        "2",
        "--continue-on-error",
    ];
    let first = dir.batch(&args);
    let second = dir.batch(&args);
    succeeded(&first);
    succeeded(&second);
    assert_eq!(
        first.stdout, second.stdout,
        "receipts must not depend on completion order"
    );
    assert!(String::from_utf8_lossy(&first.stdout).contains("include_missing"));
    assert!(
        dir.text("out/a-good.html")
            .contains("The shared content was expanded.")
    );
    assert_eq!(
        dir.text("out/b-bad.html"),
        "previous failed-input publication"
    );
    input_failed(
        &dir.batch(&["b-bad.md", "--out-dir", "out"]),
        "include_missing",
    );
}

#[test]
fn cycles_invalid_utf8_and_both_input_budgets_preserve_previous_sibling_outputs() {
    for case in ["cycle", "utf8", "file_limit", "expanded_limit"] {
        let dir = Workspace::new();
        dir.write("root.md", "{{#include part.txt}}\n");
        let expected = match case {
            "cycle" => {
                dir.write("part.txt", "{{#include part.txt}}\n");
                "include_cycle"
            }
            "utf8" => {
                dir.write("part.txt", [0xff, 0xfe]);
                "include_invalid_utf8"
            }
            "file_limit" => {
                dir.write("part.txt", "x".repeat(100));
                "include_oversize"
            }
            "expanded_limit" => {
                dir.write("root.md", "{{#include part.txt}}\n{{#include part.txt}}\n");
                dir.write("part.txt", "x".repeat(40));
                "include_size"
            }
            _ => unreachable!(),
        };
        dir.write("out/root.html", "previous HTML");
        dir.write("out/root.pdf", "previous PDF");
        input_failed(
            &dir.batch(&[
                "root.md",
                "--to",
                "both",
                "--out-dir",
                "out",
                "--max-input-bytes",
                "64",
            ]),
            expected,
        );
        assert_eq!(dir.text("out/root.html"), "previous HTML", "{case}");
        assert_eq!(dir.text("out/root.pdf"), "previous PDF", "{case}");
    }
}

#[test]
fn includes_and_percent_encoded_image_paths_keep_the_input_directory_sandbox() {
    let dir = Workspace::new();
    dir.write("outside.txt", "Content outside the input directory.\n");
    dir.write("docs/root.md", "{{#include ../outside.txt}}\n");
    input_failed(
        &dir.batch(&["docs/root.md", "--out-dir", "out"]),
        "include_escape",
    );
    assert!(!dir.path("out/root.html").exists());

    dir.write("outside.svg", SVG);
    dir.write(
        "docs/root.md",
        "![Direct](%2e%2e/outside.svg)\n\n{{#include parts/shared.txt}}\n",
    );
    dir.write(
        "docs/parts/shared.txt",
        "![Included](%2e%2e/%2e%2e/outside.svg)\n",
    );
    succeeded(&dir.batch(&["docs/root.md", "--out-dir", "out"]));
    assert!(
        !dir.text("out/root.html")
            .contains("data:image/svg+xml;base64,")
    );
}

#[test]
fn every_jobs_included_sources_are_protected_before_either_combined_output() {
    for workers in ["1", "2"] {
        let dir = Workspace::new();
        dir.write(
            "consumer.md",
            "# Consumer\n\n{{#include out/producer.html}}\n",
        );
        dir.write("producer.md", "# Producer\n");
        dir.write("out/producer.html", "## Original shared fragment\n");
        dir.write("out/producer.pdf", "previous producer PDF");
        let output = dir.batch(&[
            "consumer.md",
            "producer.md",
            "--to",
            "both",
            "--out-dir",
            "out",
            "--workers",
            workers,
            "--continue-on-error",
        ]);
        succeeded(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("refusing to overwrite"));
        assert!(
            dir.text("out/consumer.html")
                .contains("Original shared fragment")
        );
        assert_eq!(
            dir.text("out/producer.html"),
            "## Original shared fragment\n"
        );
        assert_eq!(dir.text("out/producer.pdf"), "previous producer PDF");
    }
}

#[test]
fn fragments_consulted_before_an_include_error_still_protect_other_jobs() {
    let dir = Workspace::new();
    dir.write("a-consumer.md", "{{#include out/producer.html}}\n");
    dir.write("producer.md", "# Producer\n");
    let original = "# Shared fragment\n\n{{#include missing.txt}}\n";
    dir.write("out/producer.html", original);
    dir.write("out/producer.pdf", "previous producer PDF");
    let output = dir.batch(&[
        "a-consumer.md",
        "producer.md",
        "--to",
        "both",
        "--out-dir",
        "out",
        "--workers",
        "2",
        "--continue-on-error",
    ]);
    input_failed(&output, "include_missing");
    assert!(String::from_utf8_lossy(&output.stdout).contains("refusing to overwrite"));
    assert_eq!(dir.text("out/producer.html"), original);
    assert_eq!(dir.text("out/producer.pdf"), "previous producer PDF");
    assert!(!dir.path("out/a-consumer.html").exists());
    assert!(!dir.path("out/a-consumer.pdf").exists());
}

#[test]
fn included_rebased_image_assets_are_protected_from_other_jobs() {
    let dir = Workspace::new();
    dir.write(
        "consumer.md",
        "# Consumer\n\n{{#include parts/chart.txt}}\n",
    );
    dir.write(
        "parts/chart.txt",
        "![Original artwork](../out/pr%C3%B8ducer.svg)\n",
    );
    dir.write("prøducer.md", "# Producer\n");
    dir.write("out/prøducer.svg", SVG);
    let output = dir.batch(&[
        "consumer.md",
        "prøducer.md",
        "--to",
        "svg",
        "--out-dir",
        "out",
        "--workers",
        "2",
        "--continue-on-error",
    ]);
    succeeded(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("refusing to overwrite"));
    assert_eq!(dir.bytes("out/prøducer.svg"), SVG);
    assert!(dir.path("out/consumer.svg").is_file());
}

#[cfg(unix)]
#[test]
fn included_source_aliases_are_preserved_across_jobs() {
    for symlink_output in [false, true] {
        let dir = Workspace::new();
        dir.write("reader.md", "{{#include alias.txt}}\n");
        dir.write("writer.md", "# Writer\n");
        dir.write("parts/shared.txt", "# Shared through an alias\n");
        std::os::unix::fs::symlink("parts/shared.txt", dir.path("alias.txt")).unwrap();
        std::fs::create_dir(dir.path("out")).unwrap();
        if symlink_output {
            std::os::unix::fs::symlink("../parts/shared.txt", dir.path("out/writer.html")).unwrap();
        } else {
            std::fs::hard_link(dir.path("parts/shared.txt"), dir.path("out/writer.html")).unwrap();
        }
        let output = dir.batch(&[
            "reader.md",
            "writer.md",
            "--out-dir",
            "out",
            "--workers",
            "2",
        ]);
        assert_eq!(
            output.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("refusing to overwrite"));
        assert_eq!(dir.text("parts/shared.txt"), "# Shared through an alias\n");
        assert_eq!(dir.text("out/writer.html"), "# Shared through an alias\n");
        assert_eq!(
            std::fs::symlink_metadata(dir.path("out/writer.html"))
                .unwrap()
                .is_symlink(),
            symlink_output
        );
    }
}

#[test]
fn the_public_batch_api_expands_sources_and_keeps_include_errors_typed() {
    use franken_markdown::batch::{
        BatchMode, BatchOptions, BatchPlan, FileErrorKind, OutputFormat, run_batch_blocking,
    };
    let dir = Workspace::new();
    dir.write("root.md", "# Root\n\n{{#include shared.txt}}\n");
    dir.write("shared.txt", "Shared content from the public batch API.\n");
    let opts = BatchOptions {
        html: franken_markdown::HtmlOptions::default(),
        pdf: franken_markdown::PdfOptions::default(),
        mode: BatchMode::Throughput,
        workers: Some(1),
        mem_budget: None,
        continue_on_error: false,
        timeout_secs: None,
        max_input_bytes: 64 * 1024 * 1024,
        max_pdf_image_bytes: 32 * 1024 * 1024,
    };
    let plan = || BatchPlan {
        inputs: vec![dir.path("root.md")],
        format: OutputFormat::Html,
        out_dir: Some(dir.path("out")),
    };
    let receipt = run_batch_blocking(plan(), &opts).unwrap();
    assert_eq!(receipt.ok_count(), 1);
    let original = dir.bytes("out/root.html");
    assert!(
        String::from_utf8_lossy(&original).contains("Shared content from the public batch API.")
    );
    dir.write("root.md", "{{#include missing.txt}}\n");
    let receipt = run_batch_blocking(plan(), &opts).unwrap();
    assert_eq!(receipt.first_failure_kind(), Some(FileErrorKind::Input));
    assert_eq!(dir.bytes("out/root.html"), original);
}

// Keep the same workspace between runs so absolute diagnostic paths are stable.
// Compare full receipts within a worker configuration, and per-file receipts
// across configurations whose scheduling metadata intentionally differs.
fn assert_generated_include_is_refused(dir: &Workspace, producer: &str, exit_code: i32) {
    let input = format!("{producer}.md");
    let mut files_receipt = None;
    for workers in ["1", "2"] {
        let args = [
            "middle-consumer.md",
            &input,
            "--to",
            "both",
            "--out-dir",
            "out",
            "--workers",
            workers,
            "--continue-on-error",
        ];
        let first = dir.batch(&args);
        let second = dir.batch(&args);
        for output in [&first, &second] {
            assert_eq!(
                output.status.code(),
                Some(exit_code),
                "stdout={}; stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let receipt = String::from_utf8_lossy(&output.stdout);
            assert!(receipt.contains("\"failed\":2"), "{receipt}");
            assert!(receipt.contains("include_missing"), "{receipt}");
            assert!(receipt.contains("refusing to overwrite"), "{receipt}");
            assert!(receipt.contains("\"error_kind\":\"input\""), "{receipt}");
            assert!(receipt.contains("\"error_kind\":\"output\""), "{receipt}");
        }
        assert_eq!(first.stdout, second.stdout);
        let text = String::from_utf8(first.stdout).unwrap();
        let files = text.split_once("\"files\":").unwrap().1.to_owned();
        if let Some(expected) = &files_receipt {
            assert_eq!(&files, expected);
        } else {
            files_receipt = Some(files);
        }
        for stem in [producer, "middle-consumer"] {
            for extension in ["html", "pdf"] {
                assert!(
                    !dir.path(&format!("out/{stem}.{extension}")).exists(),
                    "a failed include or its producer must not publish output"
                );
            }
        }
        assert!(
            !dir.path("out").exists(),
            "preflight refusal and include failure must happen before output setup"
        );
    }
}

#[test]
fn missing_include_producers_are_refused_before_serial_or_parallel_scheduling() {
    // Both source orders matter: without the preflight, producer-first serial
    // execution makes the missing include spuriously succeed.
    for (producer, exit_code) in [("a-producer", 73), ("z-producer", 66)] {
        let dir = Workspace::new();
        dir.write(&format!("{producer}.md"), "# Generated fragment\n");
        dir.write(
            "middle-consumer.md",
            format!("# Consumer\n\n{{{{#include out/{producer}.html}}}}\n"),
        );
        assert_generated_include_is_refused(&dir, producer, exit_code);
    }
}

#[cfg(unix)]
#[test]
fn missing_include_aliases_protect_their_future_targets_before_publication() {
    for directory_alias in [false, true] {
        let dir = Workspace::new();
        dir.write("a-producer.md", "# Generated fragment\n");
        let include = if directory_alias {
            std::os::unix::fs::symlink("out", dir.path("alias")).unwrap();
            "alias/a-producer.html"
        } else {
            std::os::unix::fs::symlink("out/a-producer.html", dir.path("alias")).unwrap();
            "alias"
        };
        dir.write(
            "middle-consumer.md",
            format!("# Consumer\n\n{{{{#include {include}}}}}\n"),
        );
        assert_generated_include_is_refused(&dir, "a-producer", 73);
        assert!(
            std::fs::symlink_metadata(dir.path("alias"))
                .unwrap()
                .is_symlink()
        );
    }
}
