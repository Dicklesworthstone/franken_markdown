//! Native EPUB exports must preserve the same supplied resources as library exports.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::fonts::{FontStyle, body_bytes};
use franken_markdown::{
    FontAssetSlot, FontAssets, FontFamily, HtmlOptions, PdfImageAsset, parse_markdown, render_epub,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><rect width="80" height="40" fill="#123456"/></svg>"##;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fmd-epub-assets-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn export(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .current_dir(dir)
        .args([
            "--no-config",
            "document.md",
            "--to",
            "epub",
            "--out",
            "document.epub",
        ])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_epub_embeds_local_images_supplied_fonts_and_custom_css() {
    let dir = workspace();
    let source = "---\ntitle: Publication\nlang: de\n---\n# Kapitel\n\nCafé **bold**.\n\n![Diagram](figure.svg)\n\n![Repeated](figure.svg)\n";
    let css = ".fmd { font-size: 19px; color: #123456; }\nh1 { font-weight: 700; }\n";
    let font = body_bytes(FontFamily::Serif, FontStyle::Regular);
    std::fs::write(dir.join("document.md"), source).unwrap();
    std::fs::write(dir.join("figure.svg"), SVG).unwrap();
    std::fs::write(dir.join("style.css"), css).unwrap();
    std::fs::write(dir.join("body.ttf"), font).unwrap();
    let args = ["--css", "style.css", "--pdf-font", "body-regular=body.ttf"];
    let output = export(&dir, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "binary output stays out of stdout"
    );
    let actual = std::fs::read(dir.join("document.epub")).unwrap();
    let opts = HtmlOptions {
        title: Some("Publication".into()),
        lang: Some("de".into()),
        custom_css: Some(css.into()),
        font_assets: FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, font.to_vec())
            .unwrap(),
        image_assets: vec![PdfImageAsset::new("figure.svg", SVG.to_vec())],
        ..HtmlOptions::default()
    };
    let expected = render_epub(&parse_markdown(source), &opts).unwrap();
    assert_eq!(
        actual, expected,
        "CLI must forward all publication resources"
    );
    assert!(
        actual
            .windows(b"OEBPS/embedded-fonts.css".len())
            .any(|bytes| bytes == b"OEBPS/embedded-fonts.css")
    );
    assert!(
        actual
            .windows(b"OEBPS/assets/".len())
            .any(|bytes| bytes == b"OEBPS/assets/")
    );
    assert!(export(&dir, &args).status.success());
    assert_eq!(
        actual,
        std::fs::read(dir.join("document.epub")).unwrap(),
        "deterministic native export"
    );
}

#[test]
fn cli_epub_explicit_image_overrides_are_packaged_without_network_access() {
    let dir = workspace();
    let source = "# Supplied asset\n\n![Diagram](virtual.svg)\n";
    std::fs::write(dir.join("document.md"), source).unwrap();
    std::fs::write(dir.join("actual.svg"), SVG).unwrap();
    let output = export(
        &dir,
        &[
            "--pdf-image",
            "virtual.svg=actual.svg",
            "--no-remote-images",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let opts = HtmlOptions {
        image_assets: vec![PdfImageAsset::new("virtual.svg", SVG.to_vec())],
        ..HtmlOptions::default()
    };
    assert_eq!(
        std::fs::read(dir.join("document.epub")).unwrap(),
        render_epub(&parse_markdown(source), &opts).unwrap()
    );
}

#[test]
fn oversized_epub_images_fail_before_replacing_existing_output() {
    let dir = workspace();
    std::fs::write(dir.join("document.md"), "![Diagram](figure.svg)\n").unwrap();
    std::fs::write(dir.join("figure.svg"), SVG).unwrap();
    std::fs::write(dir.join("document.epub"), b"keep existing publication").unwrap();
    let output = export(&dir, &["--max-pdf-image-bytes", "8"]);
    assert_eq!(
        output.status.code(),
        Some(66),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(dir.join("document.epub")).unwrap(),
        b"keep existing publication"
    );
}
