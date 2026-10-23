//! Batch publication must preserve every source and loaded resource in the plan.
#![cfg(feature = "batch")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#246"/></svg>"##;
const SOURCE: &str = "# Original source\n\nThis paragraph must survive publication.\n";

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-batch-preservation-{}-{}",
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
            .env("FMD_CONFIG", self.path("absent.config"))
            .env_remove("SOURCE_DATE_EPOCH");
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command()
            .args(["--no-config", "batch"])
            .args(args)
            .arg("--json")
            .output()
            .unwrap()
    }
    fn assert_contents(&self, name: &str, expected: impl AsRef<[u8]>) {
        assert_eq!(
            std::fs::read(self.path(name)).unwrap(),
            expected.as_ref(),
            "{name}"
        );
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_refused(output: Output) {
    assert_eq!(
        output.status.code(),
        Some(73),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("refusing to overwrite"),
        "receipt: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn svg_export_preserves_its_same_stem_original_artwork() {
    let dir = Directory::new();
    dir.write("chart.md", "# Chart\n\n![Chart](chart.svg)\n");
    dir.write("chart.svg", SVG);
    assert_refused(dir.run(&["chart.md", "--to", "svg"]));
    dir.assert_contents("chart.svg", SVG);
}

#[test]
fn every_primary_source_is_protected_before_combined_outputs_are_written() {
    let dir = Directory::new();
    dir.write("a/doc.md", SOURCE);
    dir.write(
        "out/doc.html",
        "# Another explicitly selected Markdown source\n",
    );
    dir.write("out/doc.pdf", "previous PDF publication");
    assert_refused(dir.run(&[
        "a/doc.md",
        "out/doc.html",
        "--out-dir",
        "out",
        "--to",
        "both",
        "--workers",
        "2",
    ]));
    dir.assert_contents("a/doc.md", SOURCE);
    dir.assert_contents(
        "out/doc.html",
        "# Another explicitly selected Markdown source\n",
    );
    dir.assert_contents("out/doc.pdf", "previous PDF publication");
}

#[test]
fn a_workers_output_cannot_replace_another_jobs_image() {
    for workers in ["1", "2"] {
        let dir = Directory::new();
        dir.write("a.md", "# Consumer\n\n![Original](out/b.svg)\n");
        dir.write("b.md", "# Producer\n");
        dir.write("out/b.svg", SVG);
        let output = dir.run(&[
            "a.md",
            "b.md",
            "--out-dir",
            "out",
            "--to",
            "svg",
            "--workers",
            workers,
            "--continue-on-error",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let receipt = String::from_utf8_lossy(&output.stdout);
        assert!(receipt.contains("refusing to overwrite"), "{receipt}");
        assert!(receipt.contains("\"failed\":1"), "{receipt}");
        assert!(
            dir.path("out/a.svg").is_file(),
            "safe jobs must still complete"
        );
        dir.assert_contents("out/b.svg", SVG);
    }
}

#[cfg(unix)]
#[test]
fn other_jobs_primary_sources_are_protected_through_file_aliases() {
    for symlink in [false, true] {
        let dir = Directory::new();
        dir.write("a.md", "# Producer\n");
        dir.write("b/source.md", SOURCE);
        std::fs::create_dir(dir.path("out")).unwrap();
        if symlink {
            std::os::unix::fs::symlink("../b/source.md", dir.path("out/a.html")).unwrap();
        } else {
            std::fs::hard_link(dir.path("b/source.md"), dir.path("out/a.html")).unwrap();
        }
        assert_refused(dir.run(&["a.md", "b/source.md", "--out-dir", "out", "--workers", "2"]));
        dir.assert_contents("b/source.md", SOURCE);
        dir.assert_contents("out/a.html", SOURCE);
        assert!(dir.path("out/source.html").is_file());
        assert_eq!(
            std::fs::symlink_metadata(dir.path("out/a.html"))
                .unwrap()
                .is_symlink(),
            symlink
        );
    }
}

#[cfg(unix)]
#[test]
fn other_jobs_image_resources_are_protected_through_file_aliases() {
    for symlink in [false, true] {
        let dir = Directory::new();
        dir.write("reader.md", "# Reader\n\n![Original](original.svg)\n");
        dir.write("writer.md", "# Writer\n");
        dir.write("original.svg", SVG);
        std::fs::create_dir(dir.path("out")).unwrap();
        if symlink {
            std::os::unix::fs::symlink("../original.svg", dir.path("out/writer.svg")).unwrap();
        } else {
            std::fs::hard_link(dir.path("original.svg"), dir.path("out/writer.svg")).unwrap();
        }
        assert_refused(dir.run(&[
            "reader.md",
            "writer.md",
            "--out-dir",
            "out",
            "--to",
            "svg",
            "--workers",
            "2",
        ]));
        dir.assert_contents("original.svg", SVG);
        dir.assert_contents("out/writer.svg", SVG);
        assert_eq!(
            std::fs::symlink_metadata(dir.path("out/writer.svg"))
                .unwrap()
                .is_symlink(),
            symlink
        );
    }
}

#[test]
fn explicit_mapped_images_are_protected_even_outside_the_source_directory() {
    let dir = Directory::new();
    dir.write("source/doc.md", "# Chart\n\n![Original](logo)\n");
    dir.write("out/doc.svg", SVG);
    assert_refused(dir.run(&[
        "source/doc.md",
        "--out-dir",
        "out",
        "--to",
        "svg",
        "--pdf-image",
        "logo=out/doc.svg",
    ]));
    dir.assert_contents("out/doc.svg", SVG);
}

#[test]
fn loaded_stylesheets_and_fonts_cannot_be_replaced_by_outputs() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    dir.write("out/doc.html", "body { color: navy; }\n");
    dir.write("out/doc.pdf", "previous PDF publication");
    assert_refused(dir.run(&[
        "doc.md",
        "--out-dir",
        "out",
        "--to",
        "both",
        "--css",
        "out/doc.html",
    ]));
    dir.assert_contents("out/doc.html", "body { color: navy; }\n");
    dir.assert_contents("out/doc.pdf", "previous PDF publication");

    let font = include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
    dir.write("out/doc.html", font);
    assert_refused(dir.run(&[
        "doc.md",
        "--out-dir",
        "out",
        "--pdf-font",
        "body-regular=out/doc.html",
    ]));
    dir.assert_contents("out/doc.html", font);
}

#[test]
fn loaded_configuration_is_protected() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    dir.write("out/doc.html", "font=serif\n");
    let output = dir
        .command()
        .env("FMD_CONFIG", dir.path("out/doc.html"))
        .args(["batch", "doc.md", "--out-dir", "out", "--json"])
        .output()
        .unwrap();
    assert_refused(output);
    dir.assert_contents("out/doc.html", "font=serif\n");
}

#[test]
fn public_library_entrypoint_also_protects_automatically_loaded_resources() {
    use franken_markdown::batch::{
        BatchMode, BatchOptions, BatchPlan, FileErrorKind, OutputFormat, run_batch_blocking,
    };
    let dir = Directory::new();
    dir.write("chart.md", "# Chart\n\n![Original](chart.svg)\n");
    dir.write("chart.svg", SVG);
    let plan = BatchPlan {
        inputs: vec![dir.path("chart.md")],
        format: OutputFormat::Svg,
        out_dir: None,
    };
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
    let receipt = run_batch_blocking(plan, &opts).unwrap();
    assert_eq!(receipt.first_failure_kind(), Some(FileErrorKind::Output));
    assert_eq!(receipt.failed_count(), 1);
    dir.assert_contents("chart.svg", SVG);
}

#[test]
fn distinct_outputs_and_source_assets_can_still_be_published_together() {
    let dir = Directory::new();
    dir.write("chart.md", "# Chart\n\n![Original](artwork.svg)\n");
    dir.write("artwork.svg", SVG);
    let output = dir.run(&["chart.md", "--to", "both", "--out-dir", "site"]);
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path("site/chart.html").is_file());
    assert!(
        std::fs::read(dir.path("site/chart.pdf"))
            .unwrap()
            .starts_with(b"%PDF")
    );
    dir.assert_contents("artwork.svg", SVG);
}

#[test]
fn input_read_failures_keep_their_original_exit_category() {
    let dir = Directory::new();
    let original = [0xff, 0xfe, 0xfd];
    dir.write("doc.html", original);
    let output = dir.run(&["doc.html"]);
    assert_eq!(
        output.status.code(),
        Some(66),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    dir.assert_contents("doc.html", original);
}
