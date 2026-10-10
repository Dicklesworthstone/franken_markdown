//! Native running logos use explicit assets, including assets absent from Markdown.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use franken_markdown::{
    PdfImageAsset, PdfOptions, PdfRunningImage, PdfRunningImagePosition, render_pdf,
    zlib_decompress,
};

// Valid two-pixel RGBA PNG: one opaque and one translucent pixel, including CRCs.
const LOGO_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 244, 34, 127, 138, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 120, 225, 226, 242, 95,
    101, 226, 142, 6, 0, 19, 235, 4, 93, 101, 80, 222, 158, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
    96, 130,
];
const SOURCE: &str = "# Branding report\n\nThe source contains no image reference.\n";

fn workspace() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-running-image-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("logo.png"), LOGO_PNG).unwrap();
    root
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
    command
        .current_dir(root)
        .env("FMD_CONFIG", root.join("absent-config"))
        .env_remove("SOURCE_DATE_EPOCH");
    command
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}: stderr={} stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stdout.is_empty(),
        "PDF output must not leak to stdout"
    );
}

fn rendered(root: &Path, source: &str, args: &[&str]) -> Vec<u8> {
    let output = command(root)
        .args([
            "--text",
            source,
            "--to",
            "pdf",
            "--out",
            "result.pdf",
            "--json",
        ])
        .args(args)
        .output()
        .unwrap();
    succeeded(&output);
    fs::read(root.join("result.pdf")).unwrap()
}

fn logo(dest: &str, position: PdfRunningImagePosition, height_pt: Option<u16>) -> PdfRunningImage {
    PdfRunningImage {
        dest: dest.into(),
        position,
        height_pt,
    }
}

fn png_options() -> PdfOptions {
    PdfOptions {
        image_assets: vec![PdfImageAsset::new("brand", LOGO_PNG.to_vec())],
        ..PdfOptions::default()
    }
}

fn content_streams(pdf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    // Page streams can be raw when compression would make them larger. Follow
    // the actual page references and /Length, excluding fonts and image data.
    for reference in String::from_utf8_lossy(pdf).split("/Contents ").skip(1) {
        let number: usize = reference
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let header = format!("\n{number} 0 obj\n");
        let start = pdf
            .windows(header.len())
            .position(|bytes| bytes == header.as_bytes())
            .unwrap()
            + header.len();
        let object = &pdf[start..];
        let stream = object
            .windows(8)
            .position(|bytes| bytes == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..stream]).unwrap();
        let length: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let raw = &object[stream + 8..stream + 8 + length];
        let decoded;
        let bytes = if dictionary.contains("/Filter /FlateDecode") {
            decoded = zlib_decompress(raw, 64 << 20).expect("valid compressed page stream");
            decoded.as_slice()
        } else {
            assert!(
                !dictionary.contains("/Filter"),
                "unexpected page stream filter"
            );
            raw
        };
        let text = std::str::from_utf8(bytes).unwrap().to_owned();
        assert!(
            text.contains("BT") && text.contains(" Tf"),
            "expected body text on each page"
        );
        out.push(text);
    }
    assert!(!out.is_empty(), "expected actual PDF page content");
    out
}

fn long_doc() -> String {
    format!(
        "# Branding report\n\n{}",
        "The report retains all its body text and [working reference](https://example.com/report). Its paragraphs fill several pages while the shared logo stays in the margins.\n\n".repeat(70)
    )
}

#[test]
fn header_logo_without_markdown_reference_defaults_to_left_and_embeds_alpha() {
    let root = workspace();
    let args = [
        "--pdf-image",
        "brand=logo.png",
        "--pdf-header-image",
        "brand",
    ];
    let pdf = rendered(&root, SOURCE, &args);
    let mut opts = png_options();
    opts.running.header.image = Some(logo("brand", PdfRunningImagePosition::Left, None));
    assert_eq!(pdf, render_pdf(SOURCE, &opts).unwrap());
    let explicit_left = [args.as_slice(), &["--pdf-header-image-position", "left"]].concat();
    assert_eq!(pdf, rendered(&root, SOURCE, &explicit_left));

    let structure = String::from_utf8_lossy(&pdf);
    assert!(
        structure.contains("/SMask"),
        "the translucent logo keeps its alpha mask"
    );
    assert_eq!(structure.matches("/Subtype /Image").count(), 2);
    assert!(!structure.contains("/S /Figure"), "a logo is decorative");
    let streams = content_streams(&pdf);
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0].matches("/Subtype /Header").count(), 1);
    assert_eq!(streams[0].matches(" Do").count(), 1);
}

#[test]
fn both_bands_share_one_asset_with_rules_page_counters_and_deterministic_output() {
    let root = workspace();
    let source = long_doc();
    let args = [
        "--pdf-image",
        "brand=logo.png",
        "--pdf-header-image",
        "brand",
        "--pdf-header-image-position",
        "right",
        "--pdf-header-image-height-pt",
        "18",
        "--pdf-footer-image",
        "brand",
        "--pdf-footer-image-position",
        "left",
        "--pdf-footer-image-height-pt",
        "12",
        "--pdf-header-left",
        "{title}",
        "--pdf-footer-center",
        "{page} / {pages}",
        "--pdf-page-numbers",
        "--pdf-header-rule",
        "--pdf-footer-rule",
    ];
    let pdf = rendered(&root, &source, &args);
    assert_eq!(pdf, rendered(&root, &source, &args));
    let mut opts = png_options();
    opts.page_numbers = true;
    opts.running.header.image = Some(logo("brand", PdfRunningImagePosition::Right, Some(18)));
    opts.running.header.left = Some("{title}".into());
    opts.running.header.rule = true;
    opts.running.footer.image = Some(logo("brand", PdfRunningImagePosition::Left, Some(12)));
    opts.running.footer.center = Some("{page} / {pages}".into());
    opts.running.footer.rule = true;
    assert_eq!(pdf, render_pdf(&source, &opts).unwrap());

    let structure = String::from_utf8_lossy(&pdf);
    assert_eq!(
        structure.matches("/Subtype /Image").count(),
        2,
        "one image and one alpha mask for all pages/bands"
    );
    assert!(structure.contains("/URI (https://example.com/report)"));
    let streams = content_streams(&pdf);
    assert!(streams.len() > 1);
    assert_eq!(
        streams.len(),
        content_streams(&render_pdf(&source, &PdfOptions::default()).unwrap()).len()
    );
    for stream in streams {
        assert_eq!(stream.matches("/Subtype /Header").count(), 1);
        assert_eq!(stream.matches("/Subtype /Footer").count(), 1);
        assert_eq!(stream.matches(" Do").count(), 2);
        assert!(stream.matches(" S\n").count() >= 2, "both rules must draw");
    }
}

#[test]
fn skip_first_page_suppresses_logo_and_page_counter_together() {
    let root = workspace();
    let args = [
        "--pdf-image",
        "brand=logo.png",
        "--pdf-footer-image",
        "brand",
        "--pdf-page-numbers",
        "--pdf-running-skip-first",
    ];
    let one_page = rendered(&root, SOURCE, &args);
    assert_eq!(
        one_page,
        render_pdf(SOURCE, &PdfOptions::default()).unwrap()
    );
    assert!(!String::from_utf8_lossy(&one_page).contains("/Subtype /Image"));
    let pages = content_streams(&rendered(&root, &long_doc(), &args));
    assert!(pages.len() > 1);
    assert!(!pages[0].contains("/Pagination"));
    assert!(!pages[0].contains(" Do"));
    for stream in &pages[1..] {
        assert_eq!(stream.matches("/Subtype /Footer").count(), 1);
        assert_eq!(stream.matches(" Do").count(), 1);
    }
}

#[test]
fn svg_logo_keys_and_paths_with_equals_resolve_without_source_references() {
    let root = workspace();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="20" viewBox="0 0 80 20"><a href="https://example.com/logo"><rect width="80" height="20" fill="#315ea8"/></a></svg>"##;
    fs::write(root.join("logo=dark.svg"), svg).unwrap();
    let args = [
        "--pdf-image",
        "brand=dark=logo=dark.svg",
        "--pdf-footer-image",
        "brand=dark",
        "--pdf-footer-image-position",
        "right",
        "--pdf-footer-image-height-pt",
        "65535",
    ];
    let pdf = rendered(&root, SOURCE, &args);
    let mut opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("brand=dark", svg.to_vec())],
        ..PdfOptions::default()
    };
    opts.running.footer.image = Some(logo(
        "brand=dark",
        PdfRunningImagePosition::Right,
        Some(u16::MAX),
    ));
    assert_eq!(pdf, render_pdf(SOURCE, &opts).unwrap());
    assert!(!String::from_utf8_lossy(&pdf).contains("https://example.com/logo"));
    assert!(content_streams(&pdf)[0].contains("/Subtype /Footer"));
    assert_ne!(pdf, render_pdf(SOURCE, &PdfOptions::default()).unwrap());

    let output = command(&root)
        .args([
            "--text",
            SOURCE,
            "--to",
            "pdf",
            "--pdf-footer-image",
            "brand=dark",
            "--pdf-image",
            "brand=dark=missing=logo.svg",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(66));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("PDF image asset brand=dark from missing=logo.svg"),
        "{stderr}"
    );
}

#[test]
fn invalid_logo_flags_fail_before_input_and_preserve_existing_outputs() {
    let root = workspace();
    fs::write(root.join("protected.pdf"), b"keep PDF").unwrap();
    let cases: &[&[&str]] = &[
        &["--pdf-header-image-position", "right"],
        &["--pdf-footer-image-position", "left"],
        &["--pdf-header-image-height-pt", "12"],
        &["--pdf-footer-image-height-pt", "12"],
        &[
            "--pdf-header-image",
            "brand",
            "--pdf-footer-image-height-pt",
            "12",
        ],
        &[
            "--pdf-header-image",
            "brand",
            "--pdf-header-image-position",
            "center",
        ],
        &[
            "--pdf-footer-image",
            "brand",
            "--pdf-footer-image-height-pt",
            "0",
        ],
        &[
            "--pdf-header-image",
            "brand",
            "--pdf-header-image-height-pt",
            "65536",
        ],
        &[
            "--pdf-header-image",
            "brand",
            "--pdf-header-image-height-pt",
            "1.5",
        ],
        &[
            "--pdf-footer-image",
            "brand",
            "--pdf-footer-image-height-pt",
            "-1",
        ],
        &["--pdf-header-image", "   "],
    ];
    for flags in cases {
        let output = command(&root)
            .args([
                "absent.md",
                "--to",
                "pdf",
                "--out",
                "protected.pdf",
                "--json",
            ])
            .args(*flags)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(64),
            "{flags:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read(root.join("protected.pdf")).unwrap(), b"keep PDF");
    }
}

#[test]
fn unsupported_targets_reject_explicit_logos_before_reading_input() {
    let root = workspace();
    fs::write(root.join("protected.out"), b"keep output").unwrap();
    for target in ["html", "svg", "epub", "interactive-html"] {
        for image in ["--pdf-header-image", "--pdf-footer-image"] {
            let output = command(&root)
                .args([
                    "absent.md",
                    "--to",
                    target,
                    "--out",
                    "protected.out",
                    image,
                    "brand",
                    "--json",
                ])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(64));
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("unsupported_target_option"), "{stderr}");
            assert!(stderr.contains("--to pdf or --to both"), "{stderr}");
            assert!(output.stdout.is_empty());
            assert_eq!(
                fs::read(root.join("protected.out")).unwrap(),
                b"keep output"
            );
        }
    }
}

#[test]
fn missing_invalid_and_oversized_logo_assets_fail_with_actionable_diagnostics() {
    let root = workspace();
    fs::write(root.join("broken.png"), b"not an image").unwrap();
    fs::write(root.join("protected.html"), b"keep HTML").unwrap();
    fs::write(root.join("protected.pdf"), b"keep PDF").unwrap();
    for (extra, code, hint) in [
        (vec![], 70, "--pdf-image"),
        (vec!["--pdf-image", "brand=broken.png"], 70, "brand"),
        (vec!["--pdf-image", "brand=absent.png"], 66, "absent.png"),
        (
            vec![
                "--pdf-image",
                "brand=logo.png",
                "--max-pdf-image-bytes",
                "4",
            ],
            66,
            "--max-pdf-image-bytes",
        ),
    ] {
        let output = command(&root)
            .args([
                "--text",
                SOURCE,
                "--to",
                "both",
                "--out",
                "protected.html",
                "--pdf-header-image",
                "brand",
                "--json",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(hint), "{stderr}");
        assert!(stderr.contains("brand"), "{stderr}");
        assert_eq!(fs::read(root.join("protected.html")).unwrap(), b"keep HTML");
        assert_eq!(fs::read(root.join("protected.pdf")).unwrap(), b"keep PDF");
    }
}

struct WatchChild(Child);

impl Drop for WatchChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_pdf(child: &mut WatchChild, path: &Path, expected: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if fs::read(path).is_ok_and(|bytes| bytes == expected) {
            return;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "watch exited before rendering"
        );
        assert!(
            Instant::now() < deadline,
            "watch never produced the expected branded PDF"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn watch_rebuilds_when_a_logo_absent_from_markdown_changes() {
    let root = workspace();
    fs::write(root.join("report.md"), SOURCE).unwrap();
    let first = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#315ea8"/></svg>"##;
    let second = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#b83040"/></svg>"##;
    fs::write(root.join("logo.svg"), first).unwrap();
    let mut opts = PdfOptions {
        image_assets: vec![PdfImageAsset::new("brand", first.to_vec())],
        ..PdfOptions::default()
    };
    opts.running.header.image = Some(logo("brand", PdfRunningImagePosition::Right, Some(18)));
    let mut child = WatchChild(
        command(&root)
            .args([
                "watch",
                "report.md",
                "--to",
                "pdf",
                "--out",
                "watched.pdf",
                "--interval",
                "25",
                "--pdf-header-image",
                "brand",
                "--pdf-header-image-position",
                "right",
                "--pdf-header-image-height-pt",
                "18",
                "--pdf-image",
                "brand=logo.svg",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for_pdf(
        &mut child,
        &root.join("watched.pdf"),
        &render_pdf(SOURCE, &opts).unwrap(),
    );
    fs::write(root.join("logo.svg"), second).unwrap();
    opts.image_assets = vec![PdfImageAsset::new("brand", second.to_vec())];
    wait_for_pdf(
        &mut child,
        &root.join("watched.pdf"),
        &render_pdf(SOURCE, &opts).unwrap(),
    );
    assert_eq!(fs::read_to_string(root.join("report.md")).unwrap(), SOURCE);
}

#[test]
fn unsupported_logo_target_exits_watch_before_creating_a_polling_loop() {
    let root = workspace();
    let mut child = WatchChild(
        command(&root)
            .args([
                "watch",
                "absent.md",
                "--to",
                "html",
                "--out",
                "absent.html",
                "--pdf-footer-image",
                "brand",
                "--json",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "unsupported target started a watch loop"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let mut stderr = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(status.code(), Some(64), "{stderr}");
    assert!(stderr.contains("unsupported_target_option"));
    assert!(!stderr.contains("\"event\":\"watching\""));
    assert!(!root.join("absent.html").exists());
}

#[test]
fn capabilities_advertise_running_images_and_help_lists_native_controls() {
    let root = workspace();
    let output = command(&root)
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let capabilities = String::from_utf8_lossy(&output.stdout);
    assert!(capabilities.contains("\"pdf_running_content\":\"available_text_and_images_v1\""));
    let running = capabilities
        .split("\"pdf_running_content\":{")
        .nth(1)
        .unwrap();
    assert!(running.contains("\"images\":true"));
    for subcommand in ["render", "watch"] {
        let output = command(&root)
            .args([subcommand, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8_lossy(&output.stdout);
        for flag in [
            "--pdf-header-image",
            "--pdf-header-image-position",
            "--pdf-header-image-height-pt",
            "--pdf-footer-image",
            "--pdf-footer-image-position",
            "--pdf-footer-image-height-pt",
        ] {
            assert!(help.contains(flag), "{subcommand} help missing {flag}");
        }
    }
}

#[cfg(feature = "batch")]
#[test]
fn batch_renders_branding_from_shared_explicit_assets_and_rejects_other_targets() {
    let root = workspace();
    fs::write(root.join("report.md"), SOURCE).unwrap();
    let output = command(&root)
        .args([
            "batch",
            "report.md",
            "--workers",
            "1",
            "--to",
            "pdf",
            "--out-dir",
            "out",
            "--pdf-image",
            "brand=logo.png",
            "--pdf-header-image",
            "brand",
            "--pdf-header-image-position",
            "right",
            "--pdf-header-image-height-pt",
            "18",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut opts = png_options();
    opts.running.header.image = Some(logo("brand", PdfRunningImagePosition::Right, Some(18)));
    assert_eq!(
        fs::read(root.join("out/report.pdf")).unwrap(),
        render_pdf(SOURCE, &opts).unwrap()
    );

    let output = command(&root)
        .args([
            "batch",
            "absent.md",
            "--to",
            "html",
            "--out-dir",
            "untouched",
            "--pdf-header-image",
            "brand",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported_target_option"));
    assert!(!root.join("untouched").exists());
}
