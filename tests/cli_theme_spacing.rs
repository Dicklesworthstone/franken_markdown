//! Native publication adapters preserve opt-in theme spacing and its explicit
//! off override, without changing HTML or legacy PDF defaults.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{PdfASettings, PdfOptions, render_pdf};

const SOURCE: &str = "# Shared spacing\n\nFirst paragraph with enough words to establish normal text geometry.\n\nSecond paragraph keeps its own space.\n\n> A quotation.\n\n- First item\n- Second item\n\n```text\ncode\n```\n\n| Left | Right |\n| --- | --- |\n| One | Two |\n\nFinal paragraph.\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-theme-spacing-{}-{}",
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

    fn pdf(&self, flags: &[&str]) -> Vec<u8> {
        let output = self
            .command()
            .args(["document.md", "--to", "pdf", "--out", "document.pdf"])
            .args(flags)
            .output()
            .unwrap();
        succeeded(&output);
        fs::read(self.0.join("document.pdf")).unwrap()
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn options(enabled: bool) -> PdfOptions {
    PdfOptions {
        use_theme_spacing: enabled,
        ..PdfOptions::default()
    }
}

#[test]
fn render_and_combined_output_apply_theme_spacing_only_when_enabled() {
    let workspace = Workspace::new();
    let legacy = render_pdf(SOURCE, &options(false)).unwrap();
    let themed = render_pdf(SOURCE, &options(true)).unwrap();
    assert!(
        legacy != themed,
        "this fixture must exercise visible spacing differences"
    );
    assert!(workspace.pdf(&[]) == legacy);
    assert!(workspace.pdf(&["--pdf-theme-spacing=false"]) == legacy);
    assert!(workspace.pdf(&["--pdf-theme-spacing"]) == themed);
    succeeded(&workspace.run(&[
        "document.md",
        "--to",
        "both",
        "--out",
        "both.html",
        "--pdf-theme-spacing",
    ]));
    assert!(fs::read(workspace.0.join("both.pdf")).unwrap() == themed);
    let html = fs::read(workspace.0.join("both.html")).unwrap();
    succeeded(&workspace.run(&["document.md", "--out", "plain.html"]));
    assert!(fs::read(workspace.0.join("plain.html")).unwrap() == html);
}

#[test]
fn configured_theme_spacing_round_trips_and_explicit_false_or_no_config_overrides_it() {
    let workspace = Workspace::new();
    let show = workspace.run(&["config", "show", "--json"]);
    succeeded(&show);
    // `show` serializes only explicitly set render defaults; `get` reports
    // the effective built-in value when a key is absent from the config.
    assert!(!String::from_utf8_lossy(&show.stdout).contains("\"pdf_theme_spacing\""));
    let get = workspace.run(&["config", "get", "pdf_theme_spacing", "--json"]);
    succeeded(&get);
    assert!(String::from_utf8_lossy(&get.stdout).contains("\"value\":\"false\""));
    succeeded(&workspace.run(&["config", "set", "pdf_theme_spacing", "true"]));
    let show = workspace.run(&["config", "show", "--json"]);
    succeeded(&show);
    assert!(String::from_utf8_lossy(&show.stdout).contains("\"pdf_theme_spacing\":true"));
    assert!(workspace.pdf(&[]) == render_pdf(SOURCE, &options(true)).unwrap());
    let legacy = render_pdf(SOURCE, &options(false)).unwrap();
    assert!(workspace.pdf(&["--pdf-theme-spacing=false"]) == legacy);
    assert!(workspace.pdf(&["--no-config"]) == legacy);
    succeeded(&workspace.run(&["document.md", "--out", "document.html"]));
    let saved = fs::read(workspace.0.join("document.html")).unwrap();
    let invalid = workspace.run(&[
        "document.md",
        "--out",
        "document.html",
        "--pdf-theme-spacing",
    ]);
    assert_eq!(invalid.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("--pdf-theme-spacing"));
    assert_eq!(fs::read(workspace.0.join("document.html")).unwrap(), saved);
    let invalid = workspace.run(&["config", "set", "pdf_theme_spacing", "sometimes"]);
    assert!(!invalid.status.success());
    assert!(workspace.pdf(&[]) == render_pdf(SOURCE, &options(true)).unwrap());
}

#[test]
fn book_exports_forward_theme_spacing_and_allow_a_configured_opt_out() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.0.join("book")).unwrap();
    fs::write(workspace.0.join("book/chapter.md"), SOURCE).unwrap();
    fs::write(workspace.0.join("config"), "pdf_theme_spacing=true\n").unwrap();
    let inputs = [BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }];
    let book = BookRenderer::new(&inputs).unwrap();
    for (enabled, flags) in [(true, vec![]), (false, vec!["--pdf-theme-spacing=false"])] {
        let output = workspace
            .command()
            .args([
                "book",
                "book",
                "--to",
                "pdf",
                "--out",
                "book.pdf",
                "--toc=false",
                "--pdf-page-numbers=false",
            ])
            .args(flags)
            .output()
            .unwrap();
        succeeded(&output);
        let mut expected = options(enabled);
        expected.title = Some("Shared spacing".into());
        assert!(
            fs::read(workspace.0.join("book.pdf")).unwrap()
                == book
                    .render_pdf_with_options(&expected, PdfASettings::OFF)
                    .unwrap()
        );
    }
    let invalid = workspace.run(&[
        "book",
        "book",
        "--to",
        "epub",
        "--out",
        "book.epub",
        "--pdf-theme-spacing",
    ]);
    assert_eq!(invalid.status.code(), Some(64));
    assert!(!workspace.0.join("book.epub").exists());
}

struct Watch(Child);
impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn watch_rebuilds_retain_theme_spacing() {
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
                "--pdf-theme-spacing",
                "--interval",
                "25",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let wait = |child: &mut Watch, source: &str| {
        let expected = render_pdf(source, &options(true)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !fs::read(workspace.0.join("watched.pdf")).is_ok_and(|bytes| bytes == expected) {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "watch exited before publication"
            );
            assert!(
                Instant::now() < deadline,
                "watch did not publish theme spacing"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&mut child, SOURCE);
    let updated = SOURCE.replace(
        "Final paragraph.",
        "Updated final paragraph.\n\nAnother paragraph.",
    );
    fs::write(workspace.0.join("document.md"), &updated).unwrap();
    wait(&mut child, &updated);
}

#[cfg(feature = "batch")]
#[test]
fn batch_uses_theme_spacing_config_and_honors_an_explicit_opt_out() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), "pdf_theme_spacing=true\n").unwrap();
    for (enabled, flags) in [(true, vec![]), (false, vec!["--pdf-theme-spacing=false"])] {
        let output = workspace
            .command()
            .args([
                "batch",
                "document.md",
                "--to",
                "pdf",
                "--out-dir",
                "batch",
                "--workers",
                "1",
            ])
            .args(flags)
            .output()
            .unwrap();
        succeeded(&output);
        assert!(
            fs::read(workspace.0.join("batch/document.pdf")).unwrap()
                == render_pdf(SOURCE, &options(enabled)).unwrap()
        );
    }
}
