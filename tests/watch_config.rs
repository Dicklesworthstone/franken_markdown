//! Native config and its current stylesheet remain live watch inputs.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "# Live configuration\n\nA document with **styled** body text.\n";

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-watch-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.path("config"));
        command
    }

    fn render(&self, input: &str, extra: &[&str]) -> Vec<u8> {
        let output = self
            .command()
            .arg(input)
            .args(extra)
            .args(["--out", "-"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fresh render failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
        output.stdout
    }

    fn replace_config(&self, contents: &str) {
        self.write("replacement-config", contents);
        std::fs::rename(self.path("replacement-config"), self.path("config")).unwrap();
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Watch {
    child: Child,
    events: Receiver<String>,
}

impl Watch {
    fn start(dir: &Directory, input: &str, output: &str, extra: &[&str]) -> Self {
        let mut child = dir
            .command()
            .args([
                "watch",
                input,
                "--out",
                output,
                "--interval",
                "10",
                "--json",
            ])
            .args(extra)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        });
        let watch = Self { child, events };
        watch.event("\"event\":\"watching\"");
        watch
    }

    fn event(&self, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        loop {
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) if line.contains(expected) => return,
                Ok(line) => seen.push(line),
                Err(error) => panic!("waiting for {expected}: {error}; events: {seen:?}"),
            }
        }
    }

    fn assert_no_rebuild(&self) {
        // Used only after the initial watching event or a completed write,
        // with no pending source changes. Twenty poll windows cover retirement
        // of an input without tying correctness to render latency.
        let deadline = Instant::now() + Duration::from_millis(200);
        while let Ok(line) = self
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            assert!(
                !line.contains("\"event\":\"rebuild\"")
                    && !line.contains("\"event\":\"watched_rebuild\""),
                "retired/disabled config input caused a rebuild: {line}"
            );
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_matches(path: &Path, expected: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if std::fs::read(path).is_ok_and(|bytes| bytes == expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} never matched a fresh render with the current configuration",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn config_creation_edit_replacement_and_removal_refresh_pdf_without_source_edits() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    let extra = ["--to", "pdf", "--no-remote-images"];
    let original = dir.render("doc.md", &extra);
    let watch = Watch::start(&dir, "doc.md", "doc.pdf", &extra);
    wait_matches(&dir.path("doc.pdf"), &original);

    dir.write("config", "page_size=a4\nfont_scale=125%\n");
    watch.event("\"event\":\"wrote\"");
    let created = dir.render("doc.md", &extra);
    assert_ne!(original, created);
    wait_matches(&dir.path("doc.pdf"), &created);

    dir.write("config", "page_size=a5\nfont_scale=125%\n");
    watch.event("\"event\":\"wrote\"");
    let edited = dir.render("doc.md", &extra);
    assert_ne!(created, edited);
    wait_matches(&dir.path("doc.pdf"), &edited);

    dir.replace_config("page_size=a4\nfont=serif\n");
    watch.event("\"event\":\"wrote\"");
    let replaced = dir.render("doc.md", &extra);
    assert_ne!(edited, replaced);
    wait_matches(&dir.path("doc.pdf"), &replaced);

    std::fs::rename(dir.path("config"), dir.path("removed-config")).unwrap();
    watch.event("\"event\":\"wrote\"");
    wait_matches(&dir.path("doc.pdf"), &original);
    assert_eq!(std::fs::read_to_string(dir.path("doc.md")).unwrap(), SOURCE);
}

#[test]
fn selecting_missing_css_tracks_its_creation_and_retires_the_previous_stylesheet() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    dir.write("one.css", "body { color: #123456; }\n");
    dir.write("config", "custom_css=one.css\n");
    let watch = Watch::start(&dir, "doc.md", "doc.html", &[]);
    let original = std::fs::read(dir.path("doc.html")).unwrap();

    dir.replace_config("custom_css=two.css\n");
    watch.event("reading stylesheet two.css");
    assert_eq!(std::fs::read(dir.path("doc.html")).unwrap(), original);

    dir.write("two.css", "body { color: #654321; }\n");
    watch.event("\"event\":\"wrote\"");
    let new_css = dir.render("doc.md", &[]);
    assert_ne!(original, new_css);
    wait_matches(&dir.path("doc.html"), &new_css);

    dir.write("one.css", "body { color: red; }\n");
    watch.assert_no_rebuild();
    assert_eq!(std::fs::read(dir.path("doc.html")).unwrap(), new_css);

    dir.write("two.css", "body { color: blue; }\n");
    watch.event("\"event\":\"wrote\"");
    wait_matches(&dir.path("doc.html"), &dir.render("doc.md", &[]));
}

#[test]
fn invalid_config_preserves_output_and_dependencies_until_config_repair() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    dir.write("one.css", "body { color: #123456; }\n");
    dir.write("config", "custom_css=one.css\n");
    let watch = Watch::start(&dir, "doc.md", "doc.html", &[]);
    let original = std::fs::read(dir.path("doc.html")).unwrap();

    dir.replace_config("font=invalid\ncustom_css=unowned.css\n");
    watch.event("\"code\":\"config_error\"");
    assert_eq!(std::fs::read(dir.path("doc.html")).unwrap(), original);

    // The failed config must not retire the previous good dependency graph.
    dir.write("one.css", "body { color: #abcdef; }\n");
    watch.event("\"code\":\"config_error\"");
    assert_eq!(std::fs::read(dir.path("doc.html")).unwrap(), original);

    dir.replace_config("custom_css=one.css\nfont=serif\n");
    watch.event("\"event\":\"wrote\"");
    let repaired = dir.render("doc.md", &[]);
    assert_ne!(original, repaired);
    wait_matches(&dir.path("doc.html"), &repaired);
}

#[test]
fn config_and_css_changes_refresh_every_directory_export_and_protect_new_inputs() {
    let dir = Directory::new();
    dir.write("sources/a.md", SOURCE);
    dir.write(
        "sources/nested/b.md",
        "# Second\n\nAnother watched document.\n",
    );
    dir.write("config", "font_scale=100%\n");
    std::fs::create_dir(dir.path("output")).unwrap();
    let watch = Watch::start(&dir, "sources", "output", &[]);
    let first = std::fs::read(dir.path("output/a.html")).unwrap();

    dir.replace_config("font_scale=125%\n");
    let expected_a = dir.render("sources/a.md", &[]);
    let expected_b = dir.render("sources/nested/b.md", &[]);
    assert_ne!(first, expected_a);
    wait_matches(&dir.path("output/a.html"), &expected_a);
    wait_matches(&dir.path("output/nested/b.html"), &expected_b);

    // This output has become the shared stylesheet. Protect it before any
    // document is published, while allowing unrelated exports to continue.
    dir.replace_config("custom_css=output/a.html\n");
    watch.event("refusing to overwrite watched input");
    assert_eq!(
        std::fs::read(dir.path("output/a.html")).unwrap(),
        expected_a
    );
    wait_matches(
        &dir.path("output/nested/b.html"),
        &dir.render("sources/nested/b.md", &[]),
    );

    dir.write("repaired.css", "body { color: #135724; }\n");
    dir.replace_config("custom_css=repaired.css\n");
    wait_matches(&dir.path("output/a.html"), &dir.render("sources/a.md", &[]));
    wait_matches(
        &dir.path("output/nested/b.html"),
        &dir.render("sources/nested/b.md", &[]),
    );
    dir.write("repaired.css", "body { color: #246813; }\n");
    wait_matches(&dir.path("output/a.html"), &dir.render("sources/a.md", &[]));
    wait_matches(
        &dir.path("output/nested/b.html"),
        &dir.render("sources/nested/b.md", &[]),
    );
}

#[test]
fn explicit_css_stays_authoritative_when_configured_stylesheets_change() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    dir.write("explicit.css", "body { color: #123456; }\n");
    dir.write("ignored.css", "body { color: red; }\n");
    dir.write("config", "custom_css=ignored.css\n");
    let extra = ["--css", "explicit.css"];
    let watch = Watch::start(&dir, "doc.md", "doc.html", &extra);

    dir.replace_config("font_scale=125%\ncustom_css=missing.css\n");
    watch.event("\"event\":\"wrote\"");
    wait_matches(&dir.path("doc.html"), &dir.render("doc.md", &extra));

    dir.write("ignored.css", "body { color: blue; }\n");
    dir.write("missing.css", "body { color: green; }\n");
    watch.assert_no_rebuild();

    dir.write("explicit.css", "body { color: #abcdef; }\n");
    watch.event("\"event\":\"wrote\"");
    wait_matches(&dir.path("doc.html"), &dir.render("doc.md", &extra));
}

#[test]
fn no_config_neither_reads_nor_watches_the_configured_path() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    dir.write("config", "font=invalid\n");
    let extra = ["--no-config"];
    let watch = Watch::start(&dir, "doc.md", "doc.html", &extra);
    let original = dir.render("doc.md", &extra);
    wait_matches(&dir.path("doc.html"), &original);

    dir.replace_config("font=serif\nfont_scale=125%\n");
    watch.assert_no_rebuild();
    assert_eq!(std::fs::read(dir.path("doc.html")).unwrap(), original);
    dir.write(
        "doc.md",
        "# Changed\n\nThe source still triggers a rebuild.\n",
    );
    watch.event("\"event\":\"wrote\"");
    wait_matches(&dir.path("doc.html"), &dir.render("doc.md", &extra));
}
