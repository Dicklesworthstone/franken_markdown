//! Native PDF entry points retain citation return navigation and its opt-out.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{PdfASettings, PdfOptions, render_pdf};

const SOURCE: &str =
    "# Citations\n\nAn important claim[^source].\n\n[^source]: The supporting evidence.\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-footnote-backlinks-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("document.md"), SOURCE).unwrap();
        Self(path)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.0.join("config"))
            .env_remove("SOURCE_DATE_EPOCH");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn render(&self, name: &str, flags: &[&str]) -> Vec<u8> {
        let output = self
            .command()
            .args(["document.md", "--to", "pdf", "--out", name])
            .args(flags)
            .output()
            .unwrap();
        succeeded(&output);
        assert!(output.stdout.is_empty());
        fs::read(self.0.join(name)).unwrap()
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn options(enabled: bool) -> PdfOptions {
    PdfOptions {
        footnote_backlinks: enabled,
        ..PdfOptions::default()
    }
}

#[test]
fn render_flag_keeps_native_bytes_and_explicit_false_keeps_default_output() {
    let workspace = Workspace::new();
    let output = workspace.run(&[
        "--pdf-footnote-backlinks",
        "document.md",
        "--to",
        "both",
        "--out",
        "paired.html",
    ]);
    succeeded(&output);
    let expected = render_pdf(SOURCE, &options(true)).unwrap();
    assert_eq!(fs::read(workspace.0.join("paired.pdf")).unwrap(), expected);
    assert!(
        fs::read_to_string(workspace.0.join("paired.html"))
            .unwrap()
            .contains("fnref-")
    );
    let default = render_pdf(SOURCE, &PdfOptions::default()).unwrap();
    assert_ne!(
        expected, default,
        "The option must add actual return navigation"
    );
    assert_eq!(workspace.render("default.pdf", &[]), default);
    assert_eq!(
        workspace.render("disabled.pdf", &["--pdf-footnote-backlinks=false"]),
        default
    );
}

#[test]
fn configured_return_links_round_trip_and_flags_override_without_affecting_html() {
    let workspace = Workspace::new();
    succeeded(&workspace.run(&["config", "set", "pdf_footnote_backlinks", "true"]));
    let show = workspace.run(&["config", "show", "--json"]);
    succeeded(&show);
    assert!(String::from_utf8_lossy(&show.stdout).contains("\"pdf_footnote_backlinks\":true"));
    assert_eq!(
        workspace.render("configured.pdf", &[]),
        render_pdf(SOURCE, &options(true)).unwrap()
    );
    assert_eq!(
        workspace.render("disabled.pdf", &["--pdf-footnote-backlinks=false"]),
        render_pdf(SOURCE, &options(false)).unwrap()
    );
    assert_eq!(
        workspace.render("no-config.pdf", &["--no-config"]),
        render_pdf(SOURCE, &options(false)).unwrap()
    );
    succeeded(&workspace.run(&["document.md", "--out", "document.html"]));
    let saved = fs::read(workspace.0.join("document.html")).unwrap();
    let invalid = workspace.run(&[
        "document.md",
        "--out",
        "document.html",
        "--pdf-footnote-backlinks",
    ]);
    assert_eq!(invalid.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("--pdf-footnote-backlinks"));
    assert_eq!(fs::read(workspace.0.join("document.html")).unwrap(), saved);
}

#[test]
fn book_exports_forward_the_same_option_and_allow_a_configured_opt_out() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.0.join("book")).unwrap();
    fs::write(workspace.0.join("book/chapter.md"), SOURCE).unwrap();
    fs::write(workspace.0.join("config"), "pdf_footnote_backlinks=true\n").unwrap();
    let input = [BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }];
    let book = BookRenderer::new(&input).unwrap();
    for (enabled, flag, name) in [
        (true, "--pdf-footnote-backlinks", "enabled.pdf"),
        (false, "--pdf-footnote-backlinks=false", "disabled.pdf"),
    ] {
        succeeded(&workspace.run(&[
            "book",
            "book",
            "--to",
            "pdf",
            "--out",
            name,
            "--toc=false",
            "--pdf-page-numbers=false",
            flag,
        ]));
        // The native book command derives publication metadata from its first
        // heading; the explicit library-options API leaves the title unset.
        let mut expected_options = options(enabled);
        expected_options.title = Some("Citations".into());
        assert_eq!(
            fs::read(workspace.0.join(name)).unwrap(),
            book.render_pdf_with_options(&expected_options, PdfASettings::OFF)
                .unwrap()
        );
    }
}

struct Watch(Child);
impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_pdf(child: &mut Watch, path: &std::path::Path, expected: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if fs::read(path).is_ok_and(|bytes| bytes == expected) {
            return;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "watch exited before publication"
        );
        assert!(
            Instant::now() < deadline,
            "watch did not publish the requested PDF options"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn watch_rebuilds_keep_return_navigation() {
    let workspace = Workspace::new();
    let mut child = Watch(
        workspace
            .command()
            .args([
                "watch",
                "document.md",
                "--to",
                "pdf",
                "--out",
                "watched.pdf",
                "--pdf-footnote-backlinks",
                "--interval",
                "25",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let path = workspace.0.join("watched.pdf");
    wait_for_pdf(
        &mut child,
        &path,
        &render_pdf(SOURCE, &options(true)).unwrap(),
    );
    let updated = SOURCE.replace(
        "important claim",
        "updated claim with another citation[^other]",
    ) + "\n[^other]: More supporting evidence.\n";
    fs::write(workspace.0.join("document.md"), &updated).unwrap();
    wait_for_pdf(
        &mut child,
        &path,
        &render_pdf(&updated, &options(true)).unwrap(),
    );
}

#[cfg(feature = "batch")]
#[test]
fn batch_uses_configured_return_links_and_an_explicit_opt_out() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), "pdf_footnote_backlinks=true\n").unwrap();
    for (enabled, extra) in [
        (true, vec![]),
        (false, vec!["--pdf-footnote-backlinks=false"]),
    ] {
        let output = workspace
            .command()
            .args([
                "batch",
                "document.md",
                "--to",
                "pdf",
                "--out-dir",
                "batch-output",
                "--workers",
                "1",
            ])
            .args(extra)
            .output()
            .unwrap();
        succeeded(&output);
        assert_eq!(
            fs::read(workspace.0.join("batch-output/document.pdf")).unwrap(),
            render_pdf(SOURCE, &options(enabled)).unwrap()
        );
    }
}
