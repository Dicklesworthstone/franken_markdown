//! Native publication selects the same palettes as the shared renderers.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::config::FmdConfig;
use franken_markdown::{
    FontFamily, FontScale, HtmlOptions, PageMargins, PageSize, PdfASettings, PdfOptions,
    SvgOptions, SystemAppearance, Theme, parse_markdown, render_html, render_pdf,
};

const SOURCE: &str = "# Appearance\n\nReadable **body** and [local navigation](#appearance).\n\n```rust\nlet answer = 42;\n```\n\n| Item | Value |\n| --- | --- |\n| A | B |\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "fmd-native-appearance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("document.md"), SOURCE).unwrap();
        Self(root)
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

    fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.0.join(path)).unwrap()
    }

    fn paired(&self, flags: &[&str]) {
        succeeded(
            &self
                .command()
                .args(["document.md", "--to", "both", "--out", "publication.html"])
                .args(flags)
                .output()
                .unwrap(),
        );
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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

fn palettes() -> [(&'static str, Theme); 6] {
    [
        ("auto", Theme::default()),
        (
            "light",
            Theme::default().with_appearance(SystemAppearance::Light),
        ),
        (
            "dark",
            Theme::default().with_appearance(SystemAppearance::Dark),
        ),
        ("charcoal", Theme::charcoal()),
        ("high-contrast-light", Theme::high_contrast_light()),
        ("high-contrast-dark", Theme::high_contrast_dark()),
    ]
}

#[test]
fn appearance_configuration_round_trips_and_refuses_unknown_presets_without_writes() {
    let workspace = Workspace::new();
    let default = workspace.run(&["config", "get", "appearance"]);
    succeeded(&default);
    assert_eq!(default.stdout, b"auto\n");
    for (name, theme) in palettes() {
        succeeded(&workspace.run(&["config", "set", "appearance", name]));
        let stored = String::from_utf8(workspace.bytes("config")).unwrap();
        assert!(stored.contains(&format!("appearance={name}\n")));
        let config = FmdConfig::parse(&stored).unwrap();
        assert_eq!(config.to_theme(), theme);
        assert_eq!(config.get_resolved("appearance").as_deref(), Some(name));
        let show = workspace.run(&["config", "show", "--json"]);
        succeeded(&show);
        assert!(
            String::from_utf8_lossy(&show.stdout).contains(&format!("\"appearance\":\"{name}\""))
        );
    }
    let saved = workspace.bytes("config");
    for invalid in [
        "",
        "Dark",
        " dark",
        "dark ",
        "high_contrast_dark",
        "unknown",
    ] {
        let output = workspace.run(&["config", "set", "appearance", invalid, "--json"]);
        assert_eq!(output.status.code(), Some(64));
        assert_eq!(workspace.bytes("config"), saved);
    }
    let ignored = workspace.run(&["--no-config", "config", "get", "appearance"]);
    succeeded(&ignored);
    assert_eq!(ignored.stdout, b"auto\n");
}

#[test]
fn paired_render_uses_existing_palettes_and_automatic_output_is_unchanged() {
    let workspace = Workspace::new();
    workspace.paired(&[]);
    let legacy_html = workspace.bytes("publication.html");
    let legacy_pdf = workspace.bytes("publication.pdf");
    for (name, theme) in palettes() {
        workspace.paired(&["--appearance", name]);
        assert_eq!(
            workspace.bytes("publication.html"),
            render_html(
                SOURCE,
                &HtmlOptions {
                    theme: theme.clone(),
                    ..Default::default()
                }
            )
            .unwrap()
            .as_bytes(),
            "{name} HTML"
        );
        assert_eq!(
            workspace.bytes("publication.pdf"),
            render_pdf(
                SOURCE,
                &PdfOptions {
                    theme,
                    ..Default::default()
                }
            )
            .unwrap(),
            "{name} PDF"
        );
        if name == "auto" {
            assert_eq!(workspace.bytes("publication.html"), legacy_html);
            assert_eq!(workspace.bytes("publication.pdf"), legacy_pdf);
        }
    }
}

#[test]
fn explicit_palette_preserves_configured_typography_and_paper() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), "appearance=charcoal\nfont=serif\nfont_scale=125%\npage_size=360x504\nmargin_top_pt=24\nmargin_right_pt=30\nmargin_bottom_pt=36\nmargin_left_pt=42\n").unwrap();
    for (flags, mut theme) in [
        (vec![], Theme::charcoal()),
        (
            vec!["--appearance", "high-contrast-light"],
            Theme::high_contrast_light(),
        ),
        (vec!["--appearance", "auto"], Theme::default()),
    ] {
        theme = theme
            .with_font(FontFamily::Serif)
            .with_font_scale(FontScale::parse("125%").unwrap());
        theme.page.size = PageSize {
            name: "custom",
            width_pt: 360.0,
            height_pt: 504.0,
        };
        theme.page.margins = PageMargins {
            top_pt: 24.0,
            right_pt: 30.0,
            bottom_pt: 36.0,
            left_pt: 42.0,
        };
        workspace.paired(&flags);
        assert_eq!(
            workspace.bytes("publication.html"),
            render_html(
                SOURCE,
                &HtmlOptions {
                    theme: theme.clone(),
                    ..Default::default()
                }
            )
            .unwrap()
            .as_bytes()
        );
        assert_eq!(
            workspace.bytes("publication.pdf"),
            render_pdf(
                SOURCE,
                &PdfOptions {
                    theme,
                    base_font_size: Some(13.75),
                    ..Default::default()
                }
            )
            .unwrap()
        );
    }
    workspace.paired(&["--no-config"]);
    assert_eq!(
        workspace.bytes("publication.pdf"),
        render_pdf(SOURCE, &PdfOptions::default()).unwrap()
    );
    let saved = workspace.bytes("publication.html");
    let rejected = workspace.run(&[
        "missing.md",
        "--appearance",
        "unknown",
        "--out",
        "publication.html",
        "--json",
    ]);
    assert_eq!(rejected.status.code(), Some(64));
    assert_eq!(workspace.bytes("publication.html"), saved);
}

#[test]
fn high_contrast_reaches_native_epub_and_svg_publications() {
    let workspace = Workspace::new();
    let document = parse_markdown(SOURCE);
    let theme = Theme::high_contrast_dark();
    for target in ["epub", "svg"] {
        let output = workspace.run(&[
            "document.md",
            "--appearance",
            "high-contrast-dark",
            "--to",
            target,
            "--out",
            "-",
        ]);
        succeeded(&output);
        let expected = if target == "epub" {
            franken_markdown::render_epub(
                &document,
                &HtmlOptions {
                    theme: theme.clone(),
                    ..Default::default()
                },
            )
            .unwrap()
        } else {
            franken_markdown::render_svg(
                &document,
                &SvgOptions {
                    theme: theme.clone(),
                    ..Default::default()
                },
            )
        };
        assert_eq!(output.stdout, expected, "{target}");
    }
}

#[test]
fn book_edition_applies_configured_and_explicit_palettes_to_all_formats() {
    let workspace = Workspace::new();
    fs::create_dir(workspace.0.join("book")).unwrap();
    fs::write(workspace.0.join("book/chapter.md"), SOURCE).unwrap();
    fs::write(workspace.0.join("config"), "appearance=charcoal\n").unwrap();
    let renderer = BookRenderer::new(&[BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }])
    .unwrap();
    for (flags, theme, background) in [
        (vec![], Theme::charcoal(), "#1a1d21"),
        (
            vec!["--appearance", "high-contrast-light"],
            Theme::high_contrast_light(),
            "#ffffff",
        ),
    ] {
        succeeded(
            &workspace
                .command()
                .args([
                    "book",
                    "book",
                    "--to",
                    "all",
                    "--out-dir",
                    "edition",
                    "--toc=false",
                    "--pdf-page-numbers=false",
                ])
                .args(flags)
                .output()
                .unwrap(),
        );
        let html = String::from_utf8(workspace.bytes("edition/chapter.html")).unwrap();
        assert!(html.contains(&format!("--fmd-bg: {background};")));
        assert!(!html.contains("@media (prefers-color-scheme: dark)"));
        let options = HtmlOptions {
            theme: theme.clone(),
            title: Some("Appearance".into()),
            ..Default::default()
        };
        assert_eq!(
            workspace.bytes("edition/book.epub"),
            franken_markdown::epub::render_book_epub(renderer.book(), &options).unwrap()
        );
        assert_eq!(
            workspace.bytes("edition/book.pdf"),
            renderer
                .render_pdf_with_options(
                    &PdfOptions {
                        theme,
                        title: Some("Appearance".into()),
                        ..Default::default()
                    },
                    PdfASettings::OFF,
                )
                .unwrap()
        );
    }
    let output = workspace.run(&["book", "book", "--check-links", "--appearance", "dark"]);
    assert_eq!(output.status.code(), Some(64));
}

#[test]
fn verification_uses_explicit_appearance_and_keeps_presentation_config_isolated() {
    let workspace = Workspace::new();
    fs::write(
        workspace.0.join("config"),
        "appearance=charcoal\nfont=serif\npage_size=a5\nfont_scale=125%\n",
    )
    .unwrap();
    for (flags, theme) in [
        (vec![], Theme::default()),
        (
            vec!["--appearance", "high-contrast-dark"],
            Theme::high_contrast_dark(),
        ),
    ] {
        let output = workspace
            .command()
            .args(["verify", "document.md", "--json"])
            .args(flags)
            .output()
            .unwrap();
        let report = franken_markdown::verify::verify_pdf_result(
            &parse_markdown(SOURCE),
            &PdfOptions {
                theme,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            output.status.code(),
            Some(i32::from(report.verdict != "clean"))
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            franken_markdown::verify::to_json(&report)
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

#[test]
fn watch_reloads_configured_palettes_and_retains_them_after_source_edits() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), "appearance=charcoal\n").unwrap();
    let mut watch = Watch(
        workspace
            .command()
            .args([
                "watch",
                "document.md",
                "--to",
                "both",
                "--out",
                "watched.html",
                "--interval",
                "25",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let wait = |watch: &mut Watch, source: &str, theme: Theme| {
        let html = render_html(
            source,
            &HtmlOptions {
                theme: theme.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        let pdf = render_pdf(
            source,
            &PdfOptions {
                theme,
                ..Default::default()
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while !fs::read(workspace.0.join("watched.html"))
            .is_ok_and(|bytes| bytes == html.as_bytes())
            || !fs::read(workspace.0.join("watched.pdf")).is_ok_and(|bytes| bytes == pdf)
        {
            assert!(
                watch.0.try_wait().unwrap().is_none(),
                "watch exited before publication"
            );
            assert!(
                Instant::now() < deadline,
                "watch did not publish selected palette"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&mut watch, SOURCE, Theme::charcoal());
    fs::write(
        workspace.0.join("config"),
        "appearance=high-contrast-dark\n",
    )
    .unwrap();
    wait(&mut watch, SOURCE, Theme::high_contrast_dark());
    let updated = format!("{SOURCE}\nAn updated paragraph.\n");
    fs::write(workspace.0.join("document.md"), &updated).unwrap();
    wait(&mut watch, &updated, Theme::high_contrast_dark());
}

#[cfg(feature = "batch")]
#[test]
fn batch_inherits_palette_and_accepts_an_explicit_override() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("config"), "appearance=charcoal\n").unwrap();
    for (flags, theme) in [
        (vec![], Theme::charcoal()),
        (
            vec!["--appearance", "high-contrast-dark"],
            Theme::high_contrast_dark(),
        ),
    ] {
        succeeded(
            &workspace
                .command()
                .args([
                    "batch",
                    "document.md",
                    "--to",
                    "both",
                    "--out-dir",
                    "batch",
                    "--workers",
                    "1",
                ])
                .args(flags)
                .output()
                .unwrap(),
        );
        assert_eq!(
            workspace.bytes("batch/document.html"),
            render_html(
                SOURCE,
                &HtmlOptions {
                    theme: theme.clone(),
                    ..Default::default()
                }
            )
            .unwrap()
            .as_bytes()
        );
        assert_eq!(
            workspace.bytes("batch/document.pdf"),
            render_pdf(
                SOURCE,
                &PdfOptions {
                    theme,
                    ..Default::default()
                }
            )
            .unwrap()
        );
    }
}
