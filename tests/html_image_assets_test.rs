//! Host image bytes are portable across HTML, PDF and EPUB asset keys.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, PdfImageAsset, parse_markdown, render_html};

const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 244, 34, 127, 138, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 120, 225, 226, 242, 95,
    101, 226, 142, 6, 0, 19, 235, 4, 93, 101, 80, 222, 158, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
    96, 130,
];
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="20"><rect width="80" height="20" fill="#315ea8"/></svg>"##;

fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        franken_markdown::html::base64_encode(bytes)
    )
}

fn options(destination: &str, bytes: &[u8]) -> HtmlOptions {
    HtmlOptions {
        image_assets: vec![PdfImageAsset::new(destination, bytes.to_vec())],
        ..HtmlOptions::default()
    }
}

#[test]
fn supported_payloads_determine_mime_without_a_matching_filename_extension() {
    let jpeg = include_bytes!("../gh_og_share_image.jpg");
    for (destination, bytes, mime) in [
        ("chart", PNG, "image/png"),
        (
            "https://cdn.example.test/asset?id=7#view",
            SVG,
            "image/svg+xml",
        ),
        ("photo.png", jpeg.as_slice(), "image/jpeg"),
        ("actually.jpeg", SVG, "image/svg+xml"),
    ] {
        let source = format!("![Host asset](<{destination}> \"Caption\")");
        let html = render_html(&source, &options(&format!("  {destination}  "), bytes)).unwrap();
        assert!(
            html.contains(&format!(
                "<img src=\"{}\" alt=\"Host asset\" title=\"Caption\">",
                data_uri(mime, bytes)
            )),
            "{destination} must embed using {mime}"
        );
    }
}

#[test]
fn opaque_assets_work_in_reference_images_tables_and_sanitized_raw_html() {
    let source = "![Reference][figure]\n\n[figure]: asset\n\n| Image |\n| --- |\n| ![Cell](asset) |\n\n<p align=\"center\"><img src=\"asset\" alt=\"Raw\" width=\"80\"></p>\n";
    let html = render_html(source, &options("asset", SVG)).unwrap();
    assert_eq!(html.matches(&data_uri("image/svg+xml", SVG)).count(), 3);
    assert!(html.contains("alt=\"Reference\""));
    assert!(html.contains("alt=\"Cell\""));
    assert!(html.contains("alt=\"Raw\""));
    assert!(!html.contains("src=\"asset\""));
}

#[test]
fn svg_preambles_and_remote_import_stripping_work_for_opaque_keys() {
    let dirty = b"<?xml version='1.0'?>\n<!-- Exported image -->\n<svg xmlns='http://www.w3.org/2000/svg'><style>@import url('https://example.test/font.css'); rect{fill:red}</style><rect width='10' height='10'/></svg>";
    let clean = b"<?xml version='1.0'?>\n<!-- Exported image -->\n<svg xmlns='http://www.w3.org/2000/svg'><style>rect{fill:red}</style><rect width='10' height='10'/></svg>";
    let html = render_html("![Diagram](opaque)", &options("opaque", dirty)).unwrap();
    assert!(html.contains(&data_uri("image/svg+xml", clean)));
    assert!(!html.contains(&data_uri("image/svg+xml", dirty)));
}

#[test]
fn unsupported_payloads_keep_safe_url_fallback_and_unsafe_destinations_stay_blocked() {
    for payload in [
        b"ordinary text".as_slice(),
        b"<html><svg/></html>".as_slice(),
        b"<?xml version='1.0'?><html><svg/></html>".as_slice(),
        b"<svgscript>alert(1)</svgscript>".as_slice(),
    ] {
        let html = render_html(
            "![Fallback](unsupported.svg)",
            &options("unsupported.svg", payload),
        )
        .unwrap();
        assert!(html.contains("<img src=\"unsupported.svg\" alt=\"Fallback\">"));
        assert!(!html.contains("data:image/"));
    }
    for destination in [
        "javascript:alert(1)",
        "vbscript:payload",
        "file:///secret.png",
    ] {
        let source = format!(
            "![Safe alt](<{destination}>)\n\n<img src=\"{destination}\" alt=\"Raw alt\">\n"
        );
        let html = render_html(&source, &options(destination, PNG)).unwrap();
        assert!(!html.contains(&data_uri("image/png", PNG)));
        assert!(!html.contains(&format!("src=\"{destination}\"")));
    }
}

#[test]
fn unused_assets_preserve_default_html_bytes_and_opaque_assets_package_in_epub() {
    let source = "# A document\n\nUnchanged body with a [reference](https://example.com).";
    assert_eq!(
        render_html(source, &options("unused", PNG)).unwrap(),
        render_html(source, &HtmlOptions::default()).unwrap()
    );
    let doc = parse_markdown("# Publication\n\n![Chart](opaque)");
    let epub = franken_markdown::epub::render_epub(&doc, &options("opaque", PNG)).unwrap();
    let resource = b"OEBPS/assets/image-1.png";
    assert!(
        epub.windows(resource.len()).any(|bytes| bytes == resource),
        "EPUB must contain the opaque-key image resource"
    );
    assert_eq!(
        epub,
        franken_markdown::epub::render_epub(&doc, &options("opaque", PNG)).unwrap()
    );
}

#[cfg(feature = "cli")]
mod native {
    use super::*;
    use franken_markdown::{PdfOptions, render_pdf};
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn workspace() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "fmd-html-assets-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("image=asset.dat"), SVG).unwrap();
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
            "status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    struct WatchChild(Child);

    impl Drop for WatchChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn wait_for_html(child: &mut WatchChild, path: &Path, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "watch exited prematurely"
            );
            if fs::read_to_string(path).ok().as_deref() == Some(expected) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "watch never published the expected mapped image"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn watch_rebuilds_when_an_explicit_override_outside_source_dependencies_changes() {
        let root = workspace();
        let source = "# Watched artwork\n\n![Chart](https://cdn.example.test/chart?id=7)";
        let destination = "https://cdn.example.test/chart?id=7";
        fs::write(root.join("source.md"), source).unwrap();
        let mut child = WatchChild(
            command(&root)
                .args([
                    "watch",
                    "source.md",
                    "--to",
                    "html",
                    "--out",
                    "watched.html",
                    "--interval",
                    "25",
                    "--pdf-image",
                    "https://cdn.example.test/chart?id=7=image=asset.dat",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for_html(
            &mut child,
            &root.join("watched.html"),
            &render_html(source, &options(destination, SVG)).unwrap(),
        );
        let replacement = b"<svg xmlns='http://www.w3.org/2000/svg' width='20' height='20'><circle cx='10' cy='10' r='10' fill='red'/></svg>";
        fs::write(root.join("image=asset.dat"), replacement).unwrap();
        wait_for_html(
            &mut child,
            &root.join("watched.html"),
            &render_html(source, &options(destination, replacement)).unwrap(),
        );
        assert_eq!(fs::read_to_string(root.join("source.md")).unwrap(), source);
    }

    #[test]
    fn text_and_stdin_html_embed_explicit_opaque_assets_with_equals_in_keys_and_paths() {
        let root = workspace();
        let source = "# Supplied chart\n\n![Chart](https://cdn.example.test/image?id=7)";
        let destination = "https://cdn.example.test/image?id=7";
        let mapping = "https://cdn.example.test/image?id=7=image=asset.dat";
        let expected = render_html(source, &options(destination, SVG)).unwrap();
        let output = command(&root)
            .args([
                "--text",
                source,
                "--pdf-image",
                mapping,
                "--to",
                "html",
                "--out",
                "-",
            ])
            .output()
            .unwrap();
        succeeded(&output);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);

        let mut child = command(&root)
            .args(["-", "--pdf-image", mapping, "--to", "html", "--out", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        succeeded(&output);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }

    #[test]
    fn file_html_and_combined_output_share_explicit_overrides_over_local_assets() {
        let root = workspace();
        let source = "# Shared artwork\n\n![Chart](chart.svg)\n";
        let local = b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10' fill='red'/></svg>";
        fs::write(root.join("source.md"), source).unwrap();
        fs::write(root.join("chart.svg"), local).unwrap();
        let expected_html = render_html(source, &options("chart.svg", SVG)).unwrap();
        for target in ["html", "both"] {
            let output = command(&root)
                .args([
                    "source.md",
                    "--to",
                    target,
                    "--pdf-image",
                    "chart.svg=image=asset.dat",
                    "--out",
                    "result.html",
                    "--json",
                    "--no-remote-images",
                ])
                .output()
                .unwrap();
            succeeded(&output);
            assert!(output.stdout.is_empty());
            let html = fs::read_to_string(root.join("result.html")).unwrap();
            assert_eq!(html, expected_html);
            assert!(html.contains(&data_uri("image/svg+xml", SVG)));
            assert!(!html.contains(&data_uri("image/svg+xml", local)));
            if target == "both" {
                let opts = PdfOptions {
                    image_assets: vec![PdfImageAsset::new("chart.svg", SVG.to_vec())],
                    ..PdfOptions::default()
                };
                assert_eq!(
                    fs::read(root.join("result.pdf")).unwrap(),
                    render_pdf(source, &opts).unwrap()
                );
            }
        }
    }

    #[test]
    fn html_explicit_mapping_failures_preserve_outputs_and_emit_no_document_data() {
        let root = workspace();
        fs::write(root.join("protected.html"), b"original HTML").unwrap();
        fs::write(root.join("protected.pdf"), b"original PDF").unwrap();
        let source = "# Document\n\n![Chart](asset)";
        for target in ["html", "both"] {
            for (flags, code, hint) in [
                (
                    vec!["--pdf-image", "invalid-mapping"],
                    64,
                    "expected MARKDOWN_DEST=PATH",
                ),
                (vec!["--pdf-image", "asset=missing.dat"], 66, "missing.dat"),
                (
                    vec![
                        "--pdf-image",
                        "asset=image=asset.dat",
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
                        source,
                        "--to",
                        target,
                        "--out",
                        "protected.html",
                        "--json",
                    ])
                    .args(&flags)
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(code),
                    "{target} {flags:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(output.stdout.is_empty());
                assert!(String::from_utf8_lossy(&output.stderr).contains(hint));
                assert_eq!(
                    fs::read(root.join("protected.html")).unwrap(),
                    b"original HTML"
                );
                assert_eq!(
                    fs::read(root.join("protected.pdf")).unwrap(),
                    b"original PDF"
                );
            }
        }
        let output = command(&root)
            .args(["--text", source, "--pdf-image", "asset=missing.dat"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(66));
        assert!(
            output.stdout.is_empty(),
            "an image input error must not stream partial HTML"
        );
    }

    #[test]
    fn html_unsupported_payload_falls_back_without_inventing_an_image_mime() {
        let root = workspace();
        fs::write(root.join("not-image.png"), b"<html><svg/></html>").unwrap();
        let output = command(&root)
            .args([
                "--text",
                "![Fallback](asset)",
                "--pdf-image",
                "asset=not-image.png",
            ])
            .output()
            .unwrap();
        succeeded(&output);
        let html = String::from_utf8(output.stdout).unwrap();
        assert!(html.contains("<img src=\"asset\" alt=\"Fallback\">"));
        assert!(!html.contains("data:image/"));
    }
}
