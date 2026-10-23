//! Every render output must preserve the source files that produced it.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "# Keep this source\n\nUnpublished original text.\n";
const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12"><rect width="12" height="12" fill="red"/></svg>"#;

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-source-preservation-{}-{}",
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
            .env_remove("SOURCE_DATE_EPOCH")
            .env("FMD_CONFIG", self.path("absent-config"));
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command()
            .arg("--no-config")
            .args(args)
            .output()
            .unwrap()
    }

    fn assert_contents(&self, name: &str, expected: impl AsRef<[u8]>) {
        assert_eq!(std::fs::read(self.path(name)).unwrap(), expected.as_ref());
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
        Some(64),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "failed render emitted document data"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn svg_and_search_sidecars_cannot_replace_the_top_level_source() {
    let dir = Directory::new();
    dir.write("source.md", SOURCE);
    dir.write("rendered.html", "previous publication");
    assert_refused(dir.run(&["source.md", "--to", "svg", "--out", "source.md"]));
    dir.assert_contents("source.md", SOURCE);
    assert_refused(dir.run(&[
        "source.md",
        "--out",
        "rendered.html",
        "--search-index",
        "source.md",
        "--json",
    ]));
    dir.assert_contents("source.md", SOURCE);
    dir.assert_contents("rendered.html", "previous publication");
}

#[cfg(unix)]
#[test]
fn svg_and_sidecar_guards_recognize_hardlink_and_symlink_aliases() {
    let dir = Directory::new();
    dir.write("source.md", SOURCE);
    std::fs::hard_link(dir.path("source.md"), dir.path("alias.svg")).unwrap();
    std::os::unix::fs::symlink("source.md", dir.path("alias.json")).unwrap();
    assert_refused(dir.run(&["source.md", "--to", "svg", "--out", "alias.svg"]));
    assert_refused(dir.run(&["source.md", "--search-index", "alias.json"]));
    dir.assert_contents("source.md", SOURCE);
    dir.assert_contents("alias.svg", SOURCE);
    assert!(
        std::fs::symlink_metadata(dir.path("alias.json"))
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn nested_includes_are_preserved_before_either_combined_output_is_written() {
    let dir = Directory::new();
    dir.write("source.md", "# Root\n\n{{#include parts/first.md}}\n");
    dir.write("parts/first.md", "{{#include child.pdf}}\n");
    dir.write("parts/child.pdf", SOURCE);
    dir.write("parts/child.html", "previous publication");
    assert_refused(dir.run(&["source.md", "--to", "both", "--out", "parts/child.html"]));
    dir.assert_contents("parts/child.pdf", SOURCE);
    dir.assert_contents("parts/child.html", "previous publication");
    assert_refused(dir.run(&["source.md", "--search-index", "parts/first.md"]));
    dir.assert_contents("parts/first.md", "{{#include child.pdf}}\n");
}

#[cfg(unix)]
#[test]
fn included_source_aliases_remain_intact() {
    let dir = Directory::new();
    dir.write("source.md", "{{#include alias.md}}\n");
    dir.write("parts/child.md", SOURCE);
    std::os::unix::fs::symlink("parts/child.md", dir.path("alias.md")).unwrap();
    std::fs::hard_link(dir.path("parts/child.md"), dir.path("output.html")).unwrap();
    assert_refused(dir.run(&["source.md", "--out", "output.html"]));
    assert_refused(dir.run(&["source.md", "--search-index", "alias.md"]));
    dir.assert_contents("parts/child.md", SOURCE);
    dir.assert_contents("output.html", SOURCE);
    assert!(
        std::fs::symlink_metadata(dir.path("alias.md"))
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn explicit_stylesheets_and_active_configuration_are_render_inputs() {
    let dir = Directory::new();
    dir.write("source.md", SOURCE);
    dir.write("theme.css", "main { color: navy; }\n");
    assert_refused(dir.run(&["source.md", "--css", "theme.css", "--out", "theme.css"]));
    dir.assert_contents("theme.css", "main { color: navy; }\n");

    let config = "font=serif\ncustom_css=theme.css\n";
    dir.write("active.config", config);
    let output = dir
        .command()
        .env("FMD_CONFIG", dir.path("active.config"))
        .args(["source.md", "--out", "active.config"])
        .output()
        .unwrap();
    assert_refused(output);
    dir.assert_contents("active.config", config);
    let output = dir
        .command()
        .env("FMD_CONFIG", dir.path("active.config"))
        .args(["source.md", "--search-index", "theme.css"])
        .output()
        .unwrap();
    assert_refused(output);
    dir.assert_contents("theme.css", "main { color: navy; }\n");
}

#[test]
fn explicit_and_automatically_loaded_images_are_preserved() {
    let dir = Directory::new();
    dir.write("source.md", "# Chart\n\n![Chart](chart.svg)\n");
    dir.write("chart.svg", SVG);
    assert_refused(dir.run(&["source.md", "--to", "svg", "--out", "chart.svg"]));
    assert_refused(dir.run(&["source.md", "--search-index", "chart.svg"]));
    dir.assert_contents("chart.svg", SVG);

    // The destination itself contains '='; protect the exact mapped host path,
    // using the same resolution that loaded its bytes.
    dir.write(
        "remote.md",
        "![Chart](https://example.invalid/image?key=value)\n",
    );
    dir.write("supplied.asset", SVG);
    assert_refused(dir.run(&[
        "remote.md",
        "--pdf-image",
        "https://example.invalid/image?key=value=supplied.asset",
        "--out",
        "supplied.asset",
    ]));
    dir.assert_contents("supplied.asset", SVG);
}

#[test]
fn caller_supplied_fonts_cannot_be_replaced_by_publication_sidecars() {
    let dir = Directory::new();
    let font = include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
    dir.write("source.md", SOURCE);
    dir.write("body.ttf", font);
    assert_refused(dir.run(&[
        "source.md",
        "--pdf-font",
        "body-regular=body.ttf",
        "--search-index",
        "body.ttf",
    ]));
    dir.assert_contents("body.ttf", font);
}

#[test]
fn colliding_document_and_sidecar_outputs_leave_the_previous_file_untouched() {
    let dir = Directory::new();
    dir.write("source.md", SOURCE);
    dir.write("output.html", "previous publication");
    let result = dir.run(&[
        "source.md",
        "--out",
        "output.html",
        "--search-index",
        "./output.html",
    ]);
    assert_eq!(result.status.code(), Some(73));
    assert!(String::from_utf8_lossy(&result.stderr).contains("duplicate output path"));
    dir.assert_contents("source.md", SOURCE);
    dir.assert_contents("output.html", "previous publication");
}

#[test]
fn distinct_outputs_succeed_and_svg_dash_remains_stdout() {
    let dir = Directory::new();
    dir.write("source.md", "# Root\n\n{{#include parts.md}}\n");
    dir.write("parts.md", "Included body.\n\n![Chart](chart.svg)\n");
    dir.write("chart.svg", SVG);
    dir.write("theme.css", "main { color: navy; }\n");
    let result = dir.run(&[
        "source.md",
        "--css",
        "theme.css",
        "--out",
        "output.html",
        "--search-index",
        "search.json",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        std::fs::read_to_string(dir.path("output.html"))
            .unwrap()
            .contains("Included body.")
    );
    assert!(
        std::fs::read_to_string(dir.path("search.json"))
            .unwrap()
            .contains("Included body.")
    );
    dir.assert_contents("parts.md", "Included body.\n\n![Chart](chart.svg)\n");
    dir.assert_contents("chart.svg", SVG);
    dir.assert_contents("theme.css", "main { color: navy; }\n");
    let result = dir.run(&[
        "source.md",
        "--to",
        "svg",
        "--out",
        "-",
        "--no-remote-images",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("<svg"));
    assert!(!dir.path("-").exists());
}

struct Watch(Child);
impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn watch_uses_the_same_source_and_resource_guard() {
    let dir = Directory::new();
    dir.write("source.md", "# Root\n\n{{#include child.md}}\n");
    dir.write("child.md", SOURCE);
    let mut watch = Watch(
        dir.command()
            .args([
                "--no-config",
                "watch",
                "source.md",
                "--to",
                "svg",
                "--out",
                "child.md",
                "--json",
                "--interval",
                "10",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stderr = watch.0.stderr.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });
    let first = receiver.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(first.contains("refusing to overwrite"), "{first}");
    dir.assert_contents("child.md", SOURCE);
}
