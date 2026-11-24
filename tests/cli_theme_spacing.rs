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

const NUMERIC_SPACING: [&str; 3] = [
    "--pdf-line-height=2.5",
    "--pdf-table-cell-padding-x-em=1.25",
    "--pdf-table-cell-padding-y-em=0",
];

fn numeric_options() -> PdfOptions {
    let mut options = options(true);
    options.theme.spacing.line_height = 2.5;
    options.theme.spacing.table_cell_padding_x_em = 1.25;
    options.theme.spacing.table_cell_padding_y_em = 0.0;
    options
}

const NUMERIC_CONFIG: &str =
    "pdf_line_height=2.5\npdf_table_cell_padding_x_em=1.25\npdf_table_cell_padding_y_em=0\n";

#[test]
fn numeric_config_round_trips_and_preserves_per_value_precedence() {
    let workspace = Workspace::new();
    let verification = workspace.run(&["verify", "document.md", "--json"]);
    for (key, input, canonical) in [
        ("pdf_line_height", "2.500", "2.5"),
        ("pdf_table_cell_padding_x_em", "1.25", "1.25"),
        ("pdf_table_cell_padding_y_em", "0.0", "0"),
    ] {
        succeeded(&workspace.run(&["config", "set", key, input]));
        let get = workspace.run(&["config", "get", key, "--json"]);
        succeeded(&get);
        assert!(
            String::from_utf8_lossy(&get.stdout).contains(&format!("\"value\":\"{canonical}\""))
        );
    }
    let show = workspace.run(&["config", "show", "--json"]);
    succeeded(&show);
    let json = String::from_utf8_lossy(&show.stdout);
    assert!(json.contains("\"pdf_line_height\":2.5"));
    assert!(json.contains("\"pdf_table_cell_padding_x_em\":1.25"));
    assert!(json.contains("\"pdf_table_cell_padding_y_em\":0"));
    let get = workspace.run(&["config", "get", "pdf_theme_spacing", "--json"]);
    succeeded(&get);
    assert!(String::from_utf8_lossy(&get.stdout).contains("\"value\":\"true\""));

    let expected = render_pdf(SOURCE, &numeric_options()).unwrap();
    assert!(workspace.pdf(&[]) == expected);
    let legacy = render_pdf(SOURCE, &options(false)).unwrap();
    assert!(workspace.pdf(&["--pdf-theme-spacing=false"]) == legacy);
    assert!(workspace.pdf(&["--no-config"]) == legacy);
    let mut overridden = numeric_options();
    overridden.theme.spacing.line_height = 1.25;
    let overridden = render_pdf(SOURCE, &overridden).unwrap();
    assert!(workspace.pdf(&["--pdf-line-height=1.25"]) == overridden);

    succeeded(&workspace.run(&["document.md", "--to", "both", "--out", "configured.html"]));
    assert!(fs::read(workspace.0.join("configured.pdf")).unwrap() == expected);
    succeeded(&workspace.run(&["document.md", "--out", "plain.html", "--no-config"]));
    assert!(
        fs::read(workspace.0.join("configured.html")).unwrap()
            == fs::read(workspace.0.join("plain.html")).unwrap()
    );

    // Verification is reproducible from explicit layout options alone.
    let configured_verification = workspace.run(&["verify", "document.md", "--json"]);
    assert_eq!(
        configured_verification.status.code(),
        verification.status.code()
    );
    assert_eq!(configured_verification.stdout, verification.stdout);

    succeeded(&workspace.run(&["config", "set", "pdf_theme_spacing", "false"]));
    assert!(workspace.pdf(&[]) == legacy);
    assert!(workspace.pdf(&["--pdf-theme-spacing"]) == expected);
    assert!(workspace.pdf(&["--pdf-line-height=1.25"]) == overridden);
    let get = workspace.run(&["config", "get", "pdf_theme_spacing", "--json"]);
    succeeded(&get);
    assert!(String::from_utf8_lossy(&get.stdout).contains("\"value\":\"false\""));
}

#[test]
fn invalid_numeric_config_preserves_the_previous_settings() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), NUMERIC_CONFIG).unwrap();
    let saved = fs::read(workspace.0.join("config")).unwrap();
    for (key, value) in [
        ("pdf_line_height", "0.9999999999"),
        ("pdf_line_height", "4.0000000001"),
        ("pdf_line_height", "NaN"),
        ("pdf_table_cell_padding_x_em", "8.0000000001"),
        ("pdf_table_cell_padding_y_em", "-0.000001"),
        ("pdf_table_cell_padding_y_em", "inf"),
    ] {
        let invalid = workspace.run(&["config", "set", key, value]);
        assert!(!invalid.status.success());
        assert_eq!(fs::read(workspace.0.join("config")).unwrap(), saved);
    }
    assert!(workspace.pdf(&[]) == render_pdf(SOURCE, &numeric_options()).unwrap());
}

#[test]
fn book_inherits_numeric_config_and_allows_independent_overrides() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.0.join("book")).unwrap();
    fs::write(workspace.0.join("book/chapter.md"), SOURCE).unwrap();
    fs::write(workspace.0.join("config"), NUMERIC_CONFIG).unwrap();
    let inputs = [BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }];
    let book = BookRenderer::new(&inputs).unwrap();
    let mut overridden = numeric_options();
    overridden.theme.spacing.table_cell_padding_y_em = 0.75;
    for (mut expected, flags) in [
        (numeric_options(), vec![]),
        (overridden, vec!["--pdf-table-cell-padding-y-em=0.75"]),
        (options(false), vec!["--pdf-theme-spacing=false"]),
    ] {
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
        expected.title = Some("Shared spacing".into());
        assert!(
            fs::read(workspace.0.join("book.pdf")).unwrap()
                == book
                    .render_pdf_with_options(&expected, PdfASettings::OFF)
                    .unwrap()
        );
    }
}

#[test]
fn watch_reloads_numeric_defaults_and_retains_explicit_overrides() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), NUMERIC_CONFIG).unwrap();
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
                "--pdf-table-cell-padding-x-em=0.25",
                "--interval",
                "25",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let wait = |child: &mut Watch, options: &PdfOptions| {
        let expected = render_pdf(SOURCE, options).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !fs::read(workspace.0.join("watched.pdf")).is_ok_and(|bytes| bytes == expected) {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "watch exited before publication"
            );
            assert!(
                Instant::now() < deadline,
                "watch did not apply numeric spacing"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let mut expected = numeric_options();
    expected.theme.spacing.table_cell_padding_x_em = 0.25;
    wait(&mut child, &expected);
    fs::write(
        workspace.0.join("config"),
        NUMERIC_CONFIG.replace("pdf_line_height=2.5", "pdf_line_height=1.25"),
    )
    .unwrap();
    expected.theme.spacing.line_height = 1.25;
    wait(&mut child, &expected);
}

#[cfg(feature = "batch")]
#[test]
fn batch_inherits_numeric_config_and_overrides_individual_values() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), NUMERIC_CONFIG).unwrap();
    succeeded(&workspace.run(&[
        "batch",
        "document.md",
        "--to",
        "pdf",
        "--out-dir",
        "batch",
        "--workers",
        "1",
        "--pdf-line-height=1.25",
    ]));
    let mut expected = numeric_options();
    expected.theme.spacing.line_height = 1.25;
    assert!(
        fs::read(workspace.0.join("batch/document.pdf")).unwrap()
            == render_pdf(SOURCE, &expected).unwrap()
    );
}

#[test]
fn numeric_controls_match_native_pdf_and_leave_paired_html_alone() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), "pdf_theme_spacing=false\n").unwrap();
    let expected = render_pdf(SOURCE, &numeric_options()).unwrap();
    assert!(expected != render_pdf(SOURCE, &options(true)).unwrap());
    assert!(workspace.pdf(&NUMERIC_SPACING) == expected);

    let output = workspace
        .command()
        .args(["document.md", "--to", "both", "--out", "paired.html"])
        .args(NUMERIC_SPACING)
        .output()
        .unwrap();
    succeeded(&output);
    assert!(fs::read(workspace.0.join("paired.pdf")).unwrap() == expected);
    succeeded(&workspace.run(&["document.md", "--out", "plain.html"]));
    assert!(
        fs::read(workspace.0.join("paired.html")).unwrap()
            == fs::read(workspace.0.join("plain.html")).unwrap()
    );
}

#[test]
fn invalid_numeric_or_conflicting_spacing_preserves_output() {
    let workspace = Workspace::new();
    let saved = workspace.pdf(&[]);
    for flags in [
        vec!["--pdf-line-height=0.9999999999"],
        vec!["--pdf-line-height=4.0000000001"],
        vec!["--pdf-line-height=NaN"],
        vec!["--pdf-table-cell-padding-x-em=-0.000001"],
        vec!["--pdf-table-cell-padding-y-em=8.0000000001"],
        vec!["--pdf-line-height=2", "--pdf-theme-spacing=false"],
    ] {
        let result = workspace
            .command()
            .args(["document.md", "--to", "pdf", "--out", "document.pdf"])
            .args(flags)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(fs::read(workspace.0.join("document.pdf")).unwrap() == saved);
    }
    let refused = workspace.run(&["missing.md", "--to", "html", "--pdf-line-height=2"]);
    assert_eq!(refused.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("require --to pdf"));
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
    for (mut expected, flags) in [
        (options(true), vec![]),
        (options(false), vec!["--pdf-theme-spacing=false"]),
        (numeric_options(), NUMERIC_SPACING.to_vec()),
    ] {
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
    let invalid = workspace.run(&["book", "book", "--check-links", "--pdf-line-height=2"]);
    assert!(!invalid.status.success());
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
    for (expected, flags) in [
        (options(true), vec![]),
        (options(false), vec!["--pdf-theme-spacing=false"]),
        (numeric_options(), NUMERIC_SPACING.to_vec()),
    ] {
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
                == render_pdf(SOURCE, &expected).unwrap()
        );
    }
}
