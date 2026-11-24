//! Native verification can measure the same explicit layout and local bytes
//! as publication. Presentation defaults and network assets remain explicit.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{
    BaselineGrid, FontAssetSlot, FontFamily, FontScale, PageStyle, PdfImageAsset, PdfOptions,
    parse_markdown_with_profile, verify,
};

const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"80\"><rect width=\"120\" height=\"80\" fill=\"red\"/></svg>";
const PROSE: &str = "A publication needs its actual paper, type size and resources to determine line breaks and page boundaries. This paragraph contains **strong text**, *emphasis* and an [external reference](https://example.invalid/reference).\n\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fmd-verify-layout-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("parts")).unwrap();
        fs::create_dir(path.join("no-fetchers")).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, content: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), content).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.0.join("config"))
            .env("PATH", self.0.join("no-fetchers"))
            .env_remove("SOURCE_DATE_EPOCH");
        command
    }

    fn verify(&self, args: &[&str], source: Option<&str>) -> Output {
        let mut child = self
            .command()
            .arg("verify")
            .args(args)
            .arg("--json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(source) = source {
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
}

fn report(output: Output, source: &str, options: &PdfOptions) -> verify::VerifyReport {
    let parsed = parse_markdown_with_profile(source, options.profile);
    let expected = verify::verify_pdf_result(&parsed, options).unwrap();
    assert_eq!(
        output.status.code(),
        Some(i32::from(expected.verdict != "clean")),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        verify::to_json(&expected)
    );
    assert!(output.stderr.is_empty());
    expected
}

fn refused(output: Output, code: i32, detail: &str) {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(detail), "{stderr}");
    assert!(!stderr.contains("checked 1 external link"), "{stderr}");
}

#[test]
fn default_layout_ignores_presentation_config_frontmatter_and_local_images() {
    let workspace = Workspace::new();
    let source = "---\nlang: de\ntoc: true\n---\n# Report\n\n![Figure](plot.svg)\n";
    workspace.write("document.md", source);
    workspace.write("plot.svg", SVG);
    workspace.write(
        "config",
        "font=serif\nfont_scale=125%\npage_size=a4\ncustom_css=missing.css\n",
    );
    let baseline = report(
        workspace.verify(&["document.md"], None),
        source,
        &PdfOptions::default(),
    );
    assert!(
        baseline
            .findings
            .iter()
            .any(|f| f.code == "unresolved_image")
    );
    workspace.write("config", "not-a-valid-config-line");
    report(
        workspace.verify(&["document.md", "--no-config"], None),
        source,
        &PdfOptions::default(),
    );
}

#[test]
fn local_images_from_includes_html_and_notes_use_their_rebased_source_paths() {
    let workspace = Workspace::new();
    workspace.write("document.md", "# Report\n\n{{#include parts/body.md}}\n");
    workspace.write("parts/plot one.svg", SVG);
    workspace.write(
        "parts/body.md",
        "![Figure](plot%20one.svg)\n\nA note.[^n]\n\n[^n]: ![Note figure](plot%20one.svg)\n",
    );
    let expanded = "# Report\n\n![Figure](parts/plot%20one.svg)\n\nA note.[^n]\n\n[^n]: ![Note figure](parts/plot%20one.svg)\n";
    let options = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "parts/plot%20one.svg",
            SVG.as_bytes().to_vec(),
        )],
        ..Default::default()
    };
    let checked = report(
        workspace.verify(&["document.md", "--load-local-images"], None),
        expanded,
        &options,
    );
    assert!(
        !checked
            .findings
            .iter()
            .any(|f| f.code == "unresolved_image")
    );
    let html = "# Report\n\n<img src=\"parts/plot%20one.svg\" alt=\"Figure\">\n";
    workspace.write("html.md", html);
    report(
        workspace.verify(&["html.md", "--load-local-images"], None),
        html,
        &options,
    );
}

#[test]
fn explicit_images_override_local_files_and_authorize_remote_keys_without_fetching() {
    let workspace = Workspace::new();
    let source = "# Report\n\n![Local](plot.svg)\n\n![Remote](https://example.invalid/plot?v=1)\n";
    workspace.write("document.md", source);
    workspace.write("plot.svg", "not an image");
    workspace.write("supplied=plot.svg", SVG);
    let options = PdfOptions {
        image_assets: vec![
            PdfImageAsset::new("plot.svg", SVG.as_bytes().to_vec()),
            PdfImageAsset::new("https://example.invalid/plot?v=1", SVG.as_bytes().to_vec()),
        ],
        ..Default::default()
    };
    let checked = report(
        workspace.verify(
            &[
                "document.md",
                "--load-local-images",
                "--pdf-image",
                "plot.svg=supplied=plot.svg",
                "--pdf-image",
                "https://example.invalid/plot?v=1=supplied=plot.svg",
            ],
            None,
        ),
        source,
        &options,
    );
    assert_eq!(checked.verdict, "clean");
    assert_eq!(
        fs::read_to_string(workspace.0.join("plot.svg")).unwrap(),
        "not an image"
    );
}

#[test]
fn stdin_can_use_explicit_images_and_host_fonts_in_the_actual_measured_layout() {
    let workspace = Workspace::new();
    workspace.write("plot.svg", SVG);
    workspace.write("body.ttf", franken_markdown::text::bundled::CM_REGULAR);
    let source = format!("# Report\n\n![Figure](asset)\n\n{}", PROSE.repeat(3));
    let mut options = PdfOptions {
        image_assets: vec![PdfImageAsset::new("asset", SVG.as_bytes().to_vec())],
        ..Default::default()
    };
    options
        .font_assets
        .set_slot(
            FontAssetSlot::BodyRegular,
            franken_markdown::text::bundled::CM_REGULAR.to_vec(),
        )
        .unwrap();
    options
        .font_assets
        .set_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
    let checked = report(
        workspace.verify(
            &[
                "-",
                "--pdf-image",
                "asset=plot.svg",
                "--pdf-font",
                "body-regular=body.ttf",
                "--pdf-font-weight",
                "650",
            ],
            Some(&source),
        ),
        &source,
        &options,
    );
    assert!(
        checked
            .findings
            .iter()
            .any(|f| f.code == "font_weight_ignored_static")
    );
    let baseline = verify::verify_pdf_result(
        &parse_markdown_with_profile(&source, None),
        &PdfOptions::default(),
    )
    .unwrap();
    assert_ne!(checked.pages, baseline.pages);
}

#[test]
fn selected_geometry_typography_contents_and_note_policy_match_pdf_publication() {
    let workspace = Workspace::new();
    let source = format!(
        "# Layout\n\n{}\n## Notes\n\nA citation.[^n]\n\n[^n]: The complete note.\n\n```rust\nlet value = 1;\n```\n",
        PROSE.repeat(14)
    );
    workspace.write("document.md", &source);
    let scale = FontScale::parse("125%").unwrap();
    let mut options = PdfOptions {
        theme: franken_markdown::Theme::default()
            .with_font(FontFamily::Serif)
            .with_font_scale(scale),
        base_font_size: Some(scale.pdf_base_pt()),
        heading_scale: Some(1.2),
        table_font_size: Some(9.0),
        lang: Some("en".into()),
        toc: true,
        toc_depth: Some(2),
        code_line_numbers: true,
        optimal_pagination: true,
        strict_pagination: true,
        footnote_backlinks: true,
        use_theme_spacing: true,
        gradual_demerits: true,
        river_penalty: true,
        pareto_line_breaking: true,
        microtype: franken_markdown::layout::MicrotypeOptions::CONSERVATIVE,
        ..Default::default()
    };
    options.theme.page = PageStyle::from_points([360.0, 504.0], [24.0, 30.0, 36.0, 42.0]).unwrap();
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(15.0).unwrap());
    let flags = [
        "--font",
        "serif",
        "--font-scale",
        "125%",
        "--lang",
        "en",
        "--page-size",
        "360x504",
        "--margin-top-pt",
        "24",
        "--margin-right-pt",
        "30",
        "--margin-bottom-pt",
        "36",
        "--margin-left-pt",
        "42",
        "--pdf-baseline-grid-pt",
        "15",
        "--pdf-heading-scale",
        "1.2",
        "--pdf-table-font-size",
        "9",
        "--toc",
        "--toc-depth",
        "2",
        "--pdf-line-numbers",
        "--pdf-optimal-pagination",
        "--pdf-strict-pagination",
        "--pdf-footnote-backlinks",
        "--pdf-theme-spacing",
        "--microtype",
        "protrusion",
        "--typography-homogeneous",
        "--typography-antiriver",
        "--typography-pareto",
    ];
    let mut verify_flags = vec!["document.md"];
    verify_flags.extend(flags);
    let checked = report(workspace.verify(&verify_flags, None), &source, &options);
    assert!(checked.page_count > 1);
    let published = workspace
        .command()
        .args([
            "--no-config",
            "document.md",
            "--to",
            "pdf",
            "--out",
            "-",
            "--no-remote-images",
        ])
        .args(flags)
        .output()
        .unwrap();
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stderr)
    );
    assert_eq!(
        published.stdout,
        franken_markdown::render_pdf(&source, &options).unwrap()
    );
    let marker = b"/Type /Page ";
    assert_eq!(
        checked.page_count,
        published
            .stdout
            .windows(marker.len())
            .filter(|bytes| *bytes == marker)
            .count()
    );
}

#[test]
fn strict_layout_refusal_occurs_before_report_or_link_cache() {
    let workspace = Workspace::new();
    workspace.write("document.md", "A  \nB  \nC");
    workspace.write("cache.jsonl", "previous cache");
    refused(
        workspace.verify(
            &[
                "document.md",
                "--pdf-base-font-size",
                "24",
                "--margin-top-pt",
                "360",
                "--margin-bottom-pt",
                "360",
                "--pdf-strict-pagination",
                "--links",
                "--links-cache",
                "cache.jsonl",
            ],
            None,
        ),
        70,
        "pagination_infeasible",
    );
    assert_eq!(
        fs::read_to_string(workspace.0.join("cache.jsonl")).unwrap(),
        "previous cache"
    );
}

#[test]
fn page_budget_is_enforced_after_fitting_instead_of_reporting_an_unpublishable_success() {
    let workspace = Workspace::new();
    workspace.write(
        "document.md",
        (0..40).map(|i| format!("Row {i}  \n")).collect::<String>(),
    );
    refused(
        workspace.verify(
            &[
                "document.md",
                "--page-size",
                "144x144",
                "--margin-top-pt",
                "36",
                "--margin-right-pt",
                "36",
                "--margin-bottom-pt",
                "36",
                "--margin-left-pt",
                "36",
                "--fit-to-pages",
                "1",
            ],
            None,
        ),
        70,
        "page_budget_unmet",
    );
    workspace.write("document.md", "# Fits\n\nOne paragraph.\n");
    report(
        workspace.verify(&["document.md", "--target-pages", "1"], None),
        "# Fits\n\nOne paragraph.\n",
        &PdfOptions {
            fit_to_pages: Some(1),
            ..Default::default()
        },
    );
}

#[test]
fn invalid_options_fail_before_reading_source_and_incompatible_policies_retain_errors() {
    let workspace = Workspace::new();
    for (flags, detail) in [
        (vec!["--page-size", "144x144"], "invalid_page"),
        (vec!["--font-scale", "unknown"], "unknown font scale"),
        (vec!["--pdf-font-face", "1"], "requires a mapped --pdf-font"),
        (vec!["--fit-to-pages", "0"], "invalid_page_budget"),
        (
            vec!["--pdf-baseline-grid-pt", "14", "--fit-to-pages", "1"],
            "pdf_baseline_grid_incompatible",
        ),
    ] {
        let mut args = vec!["missing.md"];
        args.extend(flags);
        refused(workspace.verify(&args, None), 64, detail);
    }
    refused(
        workspace.verify(&["-", "--load-local-images"], None),
        64,
        "needs a file input",
    );
    workspace.write("document.md", "# Report\n");
    refused(
        workspace.verify(
            &[
                "document.md",
                "--pdf-split-footnotes",
                "--pdf-optimal-pagination",
            ],
            None,
        ),
        70,
        "pdf_split_footnotes",
    );
}

#[test]
fn oversized_missing_invalid_and_nonregular_assets_have_truthful_results() {
    let workspace = Workspace::new();
    workspace.write("document.md", "![Figure](plot.svg)\n");
    workspace.write("plot.svg", SVG);
    for flags in [
        vec!["--load-local-images"],
        vec!["--pdf-image", "plot.svg=plot.svg"],
    ] {
        let mut args = vec!["document.md", "--max-pdf-image-bytes", "8"];
        args.extend(flags);
        refused(workspace.verify(&args, None), 66, "--max-pdf-image-bytes 8");
    }
    refused(
        workspace.verify(&["document.md", "--pdf-image", "plot.svg=parts"], None),
        66,
        "must be a regular file",
    );
    refused(
        workspace.verify(&["document.md", "--pdf-font", "regular=missing.ttf"], None),
        66,
        "reading body-regular font",
    );
    workspace.write("invalid.svg", "not an image");
    let checked = report(
        workspace.verify(
            &[
                "document.md",
                "--load-local-images",
                "--pdf-image",
                "plot.svg=invalid.svg",
            ],
            None,
        ),
        "![Figure](plot.svg)\n",
        &PdfOptions {
            image_assets: vec![PdfImageAsset::new("plot.svg", b"not an image".to_vec())],
            ..Default::default()
        },
    );
    assert!(
        checked
            .findings
            .iter()
            .any(|f| f.code == "unsupported_image")
    );
}

#[test]
fn link_cache_preserves_explicit_resources_even_with_stdin() {
    let workspace = Workspace::new();
    workspace.write("plot.svg", SVG);
    workspace.write("body.ttf", franken_markdown::text::bundled::CM_REGULAR);
    let source = "# Report\n\n![Figure](asset)\n\n[Reference](https://example.invalid/reference)\n";
    for cache in ["plot.svg", "body.ttf"] {
        refused(
            workspace.verify(
                &[
                    "-",
                    "--pdf-image",
                    "asset=plot.svg",
                    "--pdf-font",
                    "regular=body.ttf",
                    "--links",
                    "--links-cache",
                    cache,
                ],
                Some(source),
            ),
            64,
            "refusing to overwrite verify input",
        );
    }
    assert_eq!(
        fs::read_to_string(workspace.0.join("plot.svg")).unwrap(),
        SVG
    );
    assert_eq!(
        fs::read(workspace.0.join("body.ttf")).unwrap(),
        franken_markdown::text::bundled::CM_REGULAR
    );
}

#[cfg(unix)]
#[test]
fn link_cache_preserves_local_image_aliases_and_missing_overridden_destinations() {
    let workspace = Workspace::new();
    workspace.write("document.md", "![Figure](parts/plot.svg)\n");
    workspace.write("parts/plot.svg", SVG);
    std::os::unix::fs::symlink("parts/plot.svg", workspace.0.join("alias")).unwrap();
    fs::hard_link(workspace.0.join("parts/plot.svg"), workspace.0.join("hard")).unwrap();
    for cache in ["alias", "hard", "parts/plot.svg"] {
        refused(
            workspace.verify(
                &[
                    "document.md",
                    "--load-local-images",
                    "--links",
                    "--links-cache",
                    cache,
                ],
                None,
            ),
            64,
            "refusing to overwrite verify input",
        );
    }
    workspace.write("document.md", "![Figure](missing.svg)\n");
    refused(
        workspace.verify(
            &[
                "document.md",
                "--load-local-images",
                "--pdf-image",
                "missing.svg=parts/plot.svg",
                "--links",
                "--links-cache",
                "missing.svg",
            ],
            None,
        ),
        64,
        "refusing to overwrite verify input",
    );
    assert!(!workspace.0.join("missing.svg").exists());
    assert_eq!(
        fs::read_to_string(workspace.0.join("parts/plot.svg")).unwrap(),
        SVG
    );
}

#[cfg(unix)]
#[test]
fn automatic_image_loading_stays_inside_the_source_directory_and_does_not_fetch_urls() {
    let workspace = Workspace::new();
    let outside = Workspace::new();
    outside.write("private.svg", SVG);
    std::os::unix::fs::symlink(outside.0.join("private.svg"), workspace.0.join("alias.svg"))
        .unwrap();
    let source =
        "# Report\n\n![Outside](alias.svg)\n\n![Remote](https://example.invalid/image.svg)\n";
    workspace.write("document.md", source);
    let checked = report(
        workspace.verify(&["document.md", "--load-local-images"], None),
        source,
        &PdfOptions::default(),
    );
    assert_eq!(
        checked
            .findings
            .iter()
            .filter(|f| f.code == "unresolved_image")
            .count(),
        2
    );
}
