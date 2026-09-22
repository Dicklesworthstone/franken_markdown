//! End-to-end preview parity and recovery across real native renders.
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
struct Preview {
    child: Child,
    events: Receiver<String>,
    port: u16,
}
impl Preview {
    fn start(input: &Path, output: &Path, target: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "watch"])
            .arg(input)
            .arg("--out")
            .arg(output)
            .args(["--to", target, "--serve", "--interval", "10", "--json"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (send, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = send.send(line);
            }
        });
        let mut preview = Self {
            child,
            events,
            port: 0,
        };
        let event = preview.event("\"event\":\"watching\"");
        let suffix = event.split("http://127.0.0.1:").nth(1).unwrap();
        preview.port = suffix.split('/').next().unwrap().parse().unwrap();
        preview
    }
    fn event(&self, expected: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        loop {
            let timeout = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(timeout) {
                Ok(line) if line.contains(expected) => return line,
                Ok(line) => seen.push(line),
                Err(error) => panic!("waiting for {expected}: {error}; events: {seen:?}"),
            }
        }
    }
    fn html(&self) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response.split_once("\r\n\r\n").unwrap().1.to_owned()
    }
    fn wait_html(&self, expected: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let html = self.html();
            if html.contains(expected) {
                return html;
            }
            assert!(
                Instant::now() < deadline,
                "preview never contained {expected}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-watch-preview-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const IMAGE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>";

#[test]
fn preview_matches_export_and_tracks_include_images_and_failed_saves() {
    let dir = Directory::new();
    let source = dir.write(
        "doc.md",
        "---\ntitle: Preview metadata\nlang: fr\ntoc: true\n---\n\n{{#include chapter.txt}}\n",
    );
    let included = dir.write(
        "chapter.txt",
        "# Included chapter\n\n![figure](figure.svg)\n",
    );
    let figure = dir.write("figure.svg", IMAGE);
    let output = dir.path("doc.html");
    let preview = Preview::start(&source, &output, "html");
    let initial = preview.html();
    assert!(initial.contains("<title>Preview metadata</title>"));
    assert!(initial.contains("lang=\"fr\""));
    assert!(initial.contains("Included chapter"));
    assert!(initial.contains("data:image/svg+xml;base64,"));
    assert!(!initial.contains("{{#include"));
    assert_eq!(
        initial.replace(franken_markdown::watch::RELOAD_SNIPPET, ""),
        std::fs::read_to_string(&output).unwrap()
    );

    std::fs::write(&included, "# Updated chapter\n\n![figure](figure.svg)\n").unwrap();
    let updated = preview.wait_html("Updated chapter");
    assert!(!updated.contains("Included chapter"));
    assert_eq!(
        updated.replace(franken_markdown::watch::RELOAD_SNIPPET, ""),
        std::fs::read_to_string(&output).unwrap()
    );

    std::fs::write(&figure, IMAGE.replace("10", "20")).unwrap();
    preview.event("\"event\":\"rebuild\"");
    let deadline = Instant::now() + Duration::from_secs(30);
    while preview.html() == updated {
        assert!(
            Instant::now() < deadline,
            "included image edit did not refresh preview"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let working = preview.html();
    std::fs::write(&included, [0xff]).unwrap();
    preview.event("include_invalid_utf8");
    assert_eq!(
        preview.html(),
        working,
        "bad saves must retain the last valid preview"
    );
    std::fs::write(&included, "# Recovered chapter\n").unwrap();
    preview.wait_html("Recovered chapter");
}

#[test]
fn pdf_watch_html_preview_resolves_the_same_metadata_includes_and_assets() {
    let dir = Directory::new();
    let source = dir.write(
        "doc.md",
        "---\ntitle: PDF preview\nlang: de\n---\n\n{{#include section.md}}\n",
    );
    dir.write("section.md", "# PDF section\n\n![figure](figure.svg)\n");
    dir.write("figure.svg", IMAGE);
    let output = dir.path("doc.pdf");
    let preview = Preview::start(&source, &output, "pdf");
    assert!(std::fs::read(&output).unwrap().starts_with(b"%PDF"));
    let html = preview.html();
    assert!(html.contains("<title>PDF preview</title>"));
    assert!(html.contains("lang=\"de\""));
    assert!(html.contains("PDF section"));
    assert!(html.contains("data:image/svg+xml;base64,"));
}

#[test]
fn includes_obey_the_file_and_total_expansion_budget() {
    let dir = Directory::new();
    let input = dir.write("doc.md", "{{#include part.txt}}\n");
    dir.write("part.txt", "x".repeat(40));
    let output = dir.path("doc.html");
    let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .arg(&input)
        .arg("--out")
        .arg(&output)
        .args(["--no-config", "--max-input-bytes", "32"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(66));
    assert!(String::from_utf8_lossy(&result.stderr).contains("include_oversize"));
    assert!(!output.exists());

    dir.write("doc.md", "{{#include part.txt}}\n{{#include part.txt}}\n");
    dir.write("part.txt", "x".repeat(40));
    let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .arg(&input)
        .arg("--out")
        .arg(&output)
        .args(["--no-config", "--max-input-bytes", "64"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(66));
    assert!(String::from_utf8_lossy(&result.stderr).contains("include_size"));
    assert!(!output.exists());
}
