//! Custom font saves and creation must refresh every affected watched export.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SANS: &[u8] = include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
const SERIF: &[u8] = include_bytes!("../fmd-font/fonts/computer-modern/cmunrm.ttf");
const SOURCE: &str = "# Fonts\n\nAn ordinary paragraph uses the supplied body face.\n";

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-watch-fonts-{}-{}",
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
        command.current_dir(&self.0).arg("--no-config");
        command
    }
    fn render(&self, input: &str) -> Vec<u8> {
        let output = self
            .command()
            .args([
                input,
                "--pdf-font",
                "body-regular=custom.ttf",
                "--out",
                "expected.html",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read(self.path("expected.html")).unwrap()
    }
    fn replace_font(&self, bytes: &[u8]) {
        self.write("replacement.ttf", bytes);
        std::fs::rename(self.path("replacement.ttf"), self.path("custom.ttf")).unwrap();
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
    fn start(dir: &Directory, input: &str, output: &str) -> Self {
        let mut child = dir
            .command()
            .args([
                "watch",
                input,
                "--out",
                output,
                "--pdf-font",
                "body-regular=custom.ttf",
                "--interval",
                "10",
                "--json",
            ])
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
        Self { child, events }
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
        if std::fs::read(path).is_ok_and(|actual| actual == expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} never matched a fresh render with the changed font",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn replacing_a_font_rebuilds_without_editing_markdown() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    dir.write("custom.ttf", SANS);
    let watch = Watch::start(&dir, "doc.md", "doc.html");
    watch.event("\"event\":\"watching\"");
    let original = std::fs::read(dir.path("doc.html")).unwrap();
    dir.replace_font(SERIF);
    watch.event("\"event\":\"rebuild\"");
    let expected = dir.render("doc.md");
    assert_ne!(
        original, expected,
        "font replacement must affect the exported face"
    );
    wait_matches(&dir.path("doc.html"), &expected);
    assert_eq!(std::fs::read_to_string(dir.path("doc.md")).unwrap(), SOURCE);
}

#[test]
fn creating_a_missing_font_recovers_the_failed_initial_export() {
    let dir = Directory::new();
    dir.write("doc.md", SOURCE);
    let watch = Watch::start(&dir, "doc.md", "doc.html");
    watch.event("\"event\":\"watching\"");
    assert!(!dir.path("doc.html").exists());
    dir.write("custom.ttf", SANS);
    watch.event("\"event\":\"rebuild\"");
    wait_matches(&dir.path("doc.html"), &dir.render("doc.md"));
    assert_eq!(std::fs::read_to_string(dir.path("doc.md")).unwrap(), SOURCE);
}

#[test]
fn shared_custom_font_changes_refresh_every_directory_export() {
    let dir = Directory::new();
    dir.write("sources/a.md", SOURCE);
    dir.write(
        "sources/nested/b.md",
        "# Another document\n\nThis body also uses the custom font.\n",
    );
    dir.write("custom.ttf", SANS);
    std::fs::create_dir(dir.path("output")).unwrap();
    let watch = Watch::start(&dir, "sources", "output");
    watch.event("\"event\":\"watching\"");
    let original_a = std::fs::read(dir.path("output/a.html")).unwrap();
    let original_b = std::fs::read(dir.path("output/nested/b.html")).unwrap();
    dir.replace_font(SERIF);
    let expected_a = dir.render("sources/a.md");
    let expected_b = dir.render("sources/nested/b.md");
    assert_ne!(original_a, expected_a);
    assert_ne!(original_b, expected_b);
    wait_matches(&dir.path("output/a.html"), &expected_a);
    wait_matches(&dir.path("output/nested/b.html"), &expected_b);
}
