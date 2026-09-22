//! Directory watch renders complete trees and keeps every affected output fresh.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-watch-directory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
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
    fn start(input: &Path, output: &Path, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "watch"])
            .arg(input)
            .arg("--out")
            .arg(output)
            .args(["--interval", "10", "--json"])
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
        Self { child, events }
    }
    fn event(&self, expected: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        loop {
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) if line.contains(expected) => return line,
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

fn wait_contains(path: &Path, content: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let contents = std::fs::read_to_string(path).unwrap_or_default();
        if contents.contains(content) {
            return contents;
        }
        assert!(
            Instant::now() < deadline,
            "{} never contained {content}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn initial_export_preserves_relative_tree_and_initializes_preview() {
    let dir = Directory::new();
    dir.write("sources/a.md", "# First chapter\n");
    dir.write("sources/nested/b.markdown", "# Nested chapter\n");
    let out = dir.path("output");
    std::fs::create_dir(&out).unwrap();
    let watch = Watch::start(&dir.path("sources"), &out, &["--serve"]);
    let event = watch.event("\"event\":\"watching\"");
    assert!(out.join("a.html").is_file());
    assert!(out.join("nested/b.html").is_file());
    assert!(!dir.path("sources/a.html").exists());
    assert!(!dir.path("sources/nested/b.html").exists());
    let port: u16 = event
        .split("http://127.0.0.1:")
        .nth(1)
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("First chapter"));
    assert!(response.contains("EventSource('/events')"));
}

#[test]
fn simultaneous_edits_shared_styles_and_new_files_rebuild_all_needed_outputs() {
    let dir = Directory::new();
    let first = dir.write("sources/a.md", "# Alpha original\n");
    let second = dir.write("sources/b.md", "# Beta original\n");
    let css = dir.write("theme.css", "body{color:red}\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let watch = Watch::start(
        &dir.path("sources"),
        &output,
        &["--css", css.to_str().unwrap()],
    );
    watch.event("\"event\":\"watching\"");
    std::fs::write(&first, "# Alpha changed\n").unwrap();
    std::fs::write(&second, "# Beta changed\n").unwrap();
    wait_contains(&output.join("a.html"), "Alpha changed");
    wait_contains(&output.join("b.html"), "Beta changed");
    std::fs::write(&css, "body{color:purple}\n").unwrap();
    wait_contains(&output.join("a.html"), "color:purple");
    wait_contains(&output.join("b.html"), "color:purple");
    dir.write("sources/new/c.md", "# Added chapter\n");
    wait_contains(&output.join("new/c.html"), "Added chapter");
    std::fs::remove_file(&first).unwrap();
    watch.event("watch_source_removed");
    assert!(
        output.join("a.html").exists(),
        "source deletion must not delete a user's export"
    );
    dir.write("sources/a.md", "# Returned chapter\n");
    wait_contains(&output.join("a.html"), "Returned chapter");
}

#[test]
fn directory_pdf_outputs_have_pdf_names_and_all_inputs_render() {
    let dir = Directory::new();
    dir.write("sources/a.md", "# Alpha\n");
    dir.write("sources/nested/b.md", "# Beta\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let watch = Watch::start(&dir.path("sources"), &output, &["--to", "pdf"]);
    watch.event("\"event\":\"watching\"");
    for path in [output.join("a.pdf"), output.join("nested/b.pdf")] {
        assert!(std::fs::read(path).unwrap().starts_with(b"%PDF"));
    }
    assert!(!output.join("a.html").exists());
}

#[test]
fn a_shared_include_change_rebuilds_each_dependent_document() {
    let dir = Directory::new();
    dir.write("sources/a.md", "# Alpha\n\n{{#include shared.txt}}\n");
    dir.write("sources/b.md", "# Beta\n\n{{#include shared.txt}}\n");
    let included = dir.write("sources/shared.txt", "Shared original paragraph.\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let watch = Watch::start(&dir.path("sources"), &output, &[]);
    watch.event("\"event\":\"watching\"");
    std::fs::write(&included, "Shared replacement paragraph.\n").unwrap();
    for output in [output.join("a.html"), output.join("b.html")] {
        let html = wait_contains(&output, "Shared replacement paragraph.");
        assert!(!html.contains("Shared original paragraph."));
    }
}

#[test]
fn colliding_sources_and_directory_measure_are_refused_before_rendering() {
    let dir = Directory::new();
    dir.write("sources/same.md", "# First\n");
    dir.write("sources/same.markdown", "# Second\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "watch"])
        .arg(dir.path("sources"))
        .arg("--out")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&result.stderr).contains("same output"));
    assert!(std::fs::read_dir(&output).unwrap().next().is_none());
    let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "watch"])
        .arg(dir.path("sources"))
        .arg("--out")
        .arg(&output)
        .args(["--measure", "1"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--measure"));
}
