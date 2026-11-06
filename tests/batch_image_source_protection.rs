//! Batch image ownership is independent of file existence, supplied overrides,
//! input order, and worker count.
#![cfg(feature = "batch")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#246"/></svg>"##;

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-batch-image-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .current_dir(&self.0)
            .env_remove("SOURCE_DATE_EPOCH")
            .args(["--no-config", "batch"])
            .args(args)
            .arg("--json")
            .output()
            .unwrap()
    }
}

fn receipt(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_refused(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(73),
        "stdout={} stderr={}",
        receipt(output),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        receipt(output).contains("refusing to overwrite"),
        "{}",
        receipt(output)
    );
}

#[test]
fn missing_same_stem_artwork_cannot_become_its_own_export() {
    let dir = Directory::new();
    dir.write("chart.md", "![Chart](chart.svg)\n");
    assert_refused(&dir.run(&["chart.md", "--to", "svg"]));
    assert!(!dir.path("chart.svg").exists());
}

#[test]
fn missing_cross_job_images_are_protected_in_every_order_and_worker_count() {
    for workers in ["1", "2"] {
        for inputs in [["reader.md", "writer.md"], ["writer.md", "reader.md"]] {
            let dir = Directory::new();
            dir.write(
                "reader.md",
                "# Reader\n\n![Future image](site/writer.svg)\n",
            );
            dir.write("writer.md", "# Writer\n");
            let output = dir.run(&[
                inputs[0],
                inputs[1],
                "--to",
                "svg",
                "--out-dir",
                "site",
                "--workers",
                workers,
                "--continue-on-error",
            ]);
            assert!(output.status.success(), "{}", receipt(&output));
            let report = receipt(&output);
            assert!(report.contains("refusing to overwrite"), "{report}");
            assert!(report.contains("\"failed\":1"), "{report}");
            assert!(
                dir.path("site/reader.svg").is_file(),
                "unrelated safe job must finish"
            );
            assert!(
                !dir.path("site/writer.svg").exists(),
                "output created another job's input"
            );
        }
    }
}

#[test]
fn explicit_overrides_preserve_both_missing_and_existing_originals() {
    for original_exists in [false, true] {
        let dir = Directory::new();
        dir.write("chart.md", "![Chart](chart.svg)\n");
        dir.write("override.svg", SVG);
        if original_exists {
            dir.write("chart.svg", SVG);
        }
        assert_refused(&dir.run(&[
            "chart.md",
            "--to",
            "svg",
            "--pdf-image",
            "chart.svg=override.svg",
        ]));
        if original_exists {
            assert_eq!(std::fs::read_to_string(dir.path("chart.svg")).unwrap(), SVG);
        } else {
            assert!(!dir.path("chart.svg").exists());
        }
        assert_eq!(
            std::fs::read_to_string(dir.path("override.svg")).unwrap(),
            SVG
        );
    }
}

#[cfg(unix)]
#[test]
fn dangling_image_entry_survives_same_stem_publication() {
    let dir = Directory::new();
    dir.write("chart.md", "![Chart](chart.svg)\n");
    std::os::unix::fs::symlink("future.svg", dir.path("chart.svg")).unwrap();
    assert_refused(&dir.run(&["chart.md", "--to", "svg"]));
    assert_eq!(
        std::fs::read_link(dir.path("chart.svg")).unwrap(),
        PathBuf::from("future.svg")
    );
    assert!(!dir.path("future.svg").exists());
}

#[cfg(unix)]
#[test]
fn other_jobs_cannot_create_the_target_of_a_dangling_image_alias() {
    let dir = Directory::new();
    dir.write("reader.md", "![Chart](chart.svg)\n");
    dir.write("future.md", "# Future\n");
    std::os::unix::fs::symlink("future.svg", dir.path("chart.svg")).unwrap();
    assert_refused(&dir.run(&["reader.md", "future.md", "--to", "svg", "--workers", "2"]));
    assert_eq!(
        std::fs::read_link(dir.path("chart.svg")).unwrap(),
        PathBuf::from("future.svg")
    );
    assert!(!dir.path("future.svg").exists());
}

#[test]
fn library_batch_entrypoint_protects_missing_image_sources() {
    use franken_markdown::batch::{
        BatchMode, BatchOptions, BatchPlan, FileErrorKind, OutputFormat, run_batch_blocking,
    };
    let dir = Directory::new();
    dir.write("chart.md", "![Chart](chart.svg)\n");
    let plan = BatchPlan {
        inputs: vec![dir.path("chart.md")],
        format: OutputFormat::Svg,
        out_dir: None,
    };
    let options = BatchOptions {
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
    let report = run_batch_blocking(plan, &options).unwrap();
    assert_eq!(report.first_failure_kind(), Some(FileErrorKind::Output));
    assert_eq!(report.failed_count(), 1);
    assert!(!dir.path("chart.svg").exists());
}

#[test]
fn supplied_images_and_missing_originals_can_publish_to_distinct_paths() {
    let dir = Directory::new();
    dir.write("chart.md", "![Chart](chart.svg)\n");
    dir.write("override.svg", SVG);
    let output = dir.run(&[
        "chart.md",
        "--to",
        "svg",
        "--out-dir",
        "site",
        "--pdf-image",
        "chart.svg=override.svg",
    ]);
    assert!(output.status.success(), "{}", receipt(&output));
    assert!(dir.path("site/chart.svg").is_file());
    assert!(!dir.path("chart.svg").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path("override.svg")).unwrap(),
        SVG
    );
}
