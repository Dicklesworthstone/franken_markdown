//! Every local image named by a document remains source material, even if it
//! cannot be loaded yet or an explicit asset supplies its rendered bytes.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "# Chart\n\n![Chart](chart.svg)\n";
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#246"/></svg>"##;
const OVERRIDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#963"/></svg>"##;

fn workspace(source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fmd-image-source-protection-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("document.md"), source).unwrap();
    std::fs::write(dir.join("style.css"), "body { color: black; }\n").unwrap();
    dir
}

fn render(dir: &Path, target: &str, output: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .current_dir(dir)
        .env_remove("SOURCE_DATE_EPOCH")
        .args([
            "--no-config",
            "document.md",
            "--to",
            target,
            "--out",
            output,
            "--css",
            "style.css",
            "--no-remote-images",
            "--json",
        ])
        .args(extra)
        .output()
        .unwrap()
}

fn assert_refused(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(64),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostics.contains("refusing to overwrite render input"),
        "{diagnostics}"
    );
    assert!(
        output.stdout.is_empty(),
        "refusal must not publish stdout data"
    );
}

#[test]
fn every_export_preserves_a_missing_local_image_path() {
    for target in ["html", "pdf", "svg", "epub"] {
        let dir = workspace(SOURCE);
        assert_refused(&render(&dir, target, "chart.svg", &[]));
        assert!(
            !dir.join("chart.svg").exists(),
            "{target} created its own image source"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("document.md")).unwrap(),
            SOURCE
        );
    }
}

#[test]
fn search_index_collision_prevents_both_exports_from_being_published() {
    let dir = workspace(SOURCE);
    assert_refused(&render(
        &dir,
        "both",
        "publication.html",
        &["--search-index", "chart.svg"],
    ));
    for name in ["publication.html", "publication.pdf", "chart.svg"] {
        assert!(!dir.join(name).exists(), "refusal published {name}");
    }
}

#[test]
fn explicit_overrides_do_not_release_the_original_image_path() {
    for original_exists in [false, true] {
        let dir = workspace(SOURCE);
        if original_exists {
            std::fs::write(dir.join("chart.svg"), SVG).unwrap();
        }
        std::fs::write(dir.join("override.svg"), OVERRIDE).unwrap();
        assert_refused(&render(
            &dir,
            "html",
            "chart.svg",
            &["--pdf-image", "chart.svg=override.svg"],
        ));
        if original_exists {
            assert_eq!(std::fs::read_to_string(dir.join("chart.svg")).unwrap(), SVG);
        } else {
            assert!(!dir.join("chart.svg").exists());
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("override.svg")).unwrap(),
            OVERRIDE
        );
    }
}

#[cfg(unix)]
#[test]
fn dangling_image_alias_and_its_future_target_are_both_protected() {
    for output in ["chart.svg", "future.svg"] {
        let dir = workspace(SOURCE);
        std::os::unix::fs::symlink("future.svg", dir.join("chart.svg")).unwrap();
        assert_refused(&render(&dir, "html", output, &[]));
        assert_eq!(
            std::fs::read_link(dir.join("chart.svg")).unwrap(),
            Path::new("future.svg")
        );
        assert!(!dir.join("future.svg").exists());
    }
}

#[cfg(unix)]
#[test]
fn missing_image_identity_resolves_directory_aliases() {
    let dir = workspace("![Chart](images/chart.svg)\n");
    std::fs::create_dir(dir.join("images")).unwrap();
    std::os::unix::fs::symlink("images", dir.join("alias")).unwrap();
    assert_refused(&render(&dir, "html", "alias/chart.svg", &[]));
    assert!(!dir.join("images/chart.svg").exists());
}

#[test]
fn safe_html_images_protect_decoded_paths_without_query_or_fragment() {
    let dir = workspace("<img src=\"chart%20one.svg?width=1#figure\" alt=\"Chart\">\n");
    assert_refused(&render(&dir, "html", "chart one.svg", &[]));
    assert!(!dir.join("chart one.svg").exists());
}

#[test]
fn ordinary_missing_images_and_explicit_overrides_still_render_to_distinct_outputs() {
    let dir = workspace(SOURCE);
    let fallback = render(&dir, "html", "fallback.html", &[]);
    assert!(
        fallback.status.success(),
        "{}",
        String::from_utf8_lossy(&fallback.stderr)
    );
    assert!(
        std::fs::read_to_string(dir.join("fallback.html"))
            .unwrap()
            .contains("chart.svg")
    );
    assert!(!dir.join("chart.svg").exists());

    std::fs::write(dir.join("override.svg"), OVERRIDE).unwrap();
    let supplied = render(
        &dir,
        "html",
        "supplied.html",
        &["--pdf-image", "chart.svg=override.svg"],
    );
    assert!(
        supplied.status.success(),
        "{}",
        String::from_utf8_lossy(&supplied.stderr)
    );
    assert!(
        std::fs::read_to_string(dir.join("supplied.html"))
            .unwrap()
            .contains("data:image/svg+xml;base64,")
    );
    assert!(!dir.join("chart.svg").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("override.svg")).unwrap(),
        OVERRIDE
    );
}

#[test]
fn watch_rebuilds_preserve_missing_image_paths() {
    let dir = workspace(SOURCE);
    let output = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .current_dir(&dir)
        .args([
            "--no-config",
            "watch",
            "document.md",
            "--to",
            "html",
            "--out",
            "chart.svg",
            "--css",
            "style.css",
            "--measure",
            "1",
            "--interval",
            "1",
            "--json",
        ])
        .output()
        .unwrap();
    assert_refused(&output);
    assert!(!dir.join("chart.svg").exists());
    assert!(
        std::fs::read_to_string(dir.join("document.md"))
            .unwrap()
            .starts_with(SOURCE)
    );
}
