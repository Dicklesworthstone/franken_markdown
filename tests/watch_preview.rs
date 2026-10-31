//! End-to-end preview parity and recovery across real native renders.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
        Self::start_with_options(input, output, target, &[])
    }
    fn start_with_options(input: &Path, output: &Path, target: &str, options: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "watch"])
            .arg(input)
            .arg("--out")
            .arg(output)
            .args(["--to", target, "--serve", "--interval", "10", "--json"])
            .args(options)
            .env("NO_PROXY", "127.0.0.1")
            .env("no_proxy", "127.0.0.1")
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

/// Serve a real remote-image response without contacting the internet. Accept
/// and request reads are bounded; Drop stops and joins the owning thread even
/// when a render assertion fails before both expected requests arrive.
struct ImageServer {
    port: u16,
    served: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ImageServer {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let served = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let served_count = Arc::clone(&served);
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => return,
                };
                if stream
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .is_err()
                    || stream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .is_err()
                {
                    continue;
                }
                let deadline = Instant::now() + Duration::from_secs(2);
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while request.len() < 8192
                    && Instant::now() < deadline
                    && !stopped.load(Ordering::Acquire)
                    && !request.windows(4).any(|bytes| bytes == b"\r\n\r\n")
                {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(count) => request.extend_from_slice(&chunk[..count]),
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                }
                if !request.starts_with(b"GET /figure.svg HTTP/")
                    || !request.windows(4).any(|bytes| bytes == b"\r\n\r\n")
                {
                    continue;
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{IMAGE}",
                    IMAGE.len()
                );
                if stream.write_all(response.as_bytes()).is_ok() {
                    served_count.fetch_add(1, Ordering::Release);
                }
            }
        });
        Self {
            port,
            served,
            stop,
            thread: Some(thread),
        }
    }

    fn served(&self) -> u64 {
        self.served.load(Ordering::Acquire)
    }
}

impl Drop for ImageServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

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
fn serving_combined_output_preserves_the_html_export_asset_policy() {
    let dir = Directory::new();
    let server = ImageServer::start();
    let url = format!("http://127.0.0.1:{}/figure.svg", server.port);
    let source = dir.write(
        "combined.md",
        format!("# Remote figure\n\n![Figure]({url})\n"),
    );
    let ordinary_path = dir.path("ordinary.html");
    let ordinary = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .arg("--no-config")
        .arg(&source)
        .args(["--to", "both", "--remote-image-timeout-secs", "2", "--out"])
        .arg(&ordinary_path)
        .env("NO_PROXY", "127.0.0.1")
        .env("no_proxy", "127.0.0.1")
        .output()
        .unwrap();
    assert!(ordinary.status.success(), "{:?}", ordinary);
    assert!(ordinary.stdout.is_empty());
    assert_eq!(
        server.served(),
        1,
        "ordinary PDF must fetch the SVG successfully"
    );
    let diagnostics = String::from_utf8_lossy(&ordinary.stderr);
    assert!(
        !diagnostics.contains("remote_image_fetch_failed"),
        "{diagnostics}"
    );
    assert!(!diagnostics.contains("unresolved_image"), "{diagnostics}");
    let expected = std::fs::read_to_string(&ordinary_path).unwrap();
    assert!(expected.contains(&format!("src=\"{url}\"")));

    let watched_path = dir.path("watched.html");
    let preview = Preview::start_with_options(
        &source,
        &watched_path,
        "both",
        &["--remote-image-timeout-secs", "2"],
    );
    assert_eq!(
        server.served(),
        2,
        "watch PDF must fetch the SVG exactly once"
    );
    assert!(
        std::fs::read(watched_path.with_extension("pdf"))
            .unwrap()
            .starts_with(b"%PDF")
    );
    let actual = std::fs::read_to_string(&watched_path).unwrap();
    assert!(
        actual == expected,
        "--serve changed the combined HTML export"
    );
    let served = preview
        .html()
        .replace(franken_markdown::watch::RELOAD_SNIPPET, "");
    assert!(
        served == expected,
        "preview must contain the actual combined HTML export"
    );
}

#[test]
fn non_html_watch_previews_apply_the_same_explicit_options_as_html_exports() {
    let dir = Directory::new();
    let source = dir.write(
        "styled.md",
        "---\ntitle: Frontmatter title\nlang: de\ntoc_depth: 6\n---\n\n\
         # Chapter\n\n## Detail\n\nTerm\n: Definition\n\n\
         <custom-note data-preview=\"kept\">Raw preview</custom-note>\n",
    );
    let font =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fmd-font/fonts/test-shaping/FmdShaping.ttf");
    let font_mapping = format!("body-regular={}", font.display());
    let options = [
        "--title",
        "Explicit preview title",
        "--lang",
        "fr",
        "--toc",
        "--toc-depth",
        "1",
        "--font",
        "serif",
        "--font-scale",
        "1.25",
        "--profile",
        "commonmark-gfm",
        "--allow-html",
        "--html-font-format",
        "ttf",
        "--font-embedding",
        "full",
        "--pdf-font",
        &font_mapping,
    ];
    let expected = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .arg("--no-config")
        .arg(&source)
        .args(["--to", "html", "--out", "-"])
        .args(options)
        .output()
        .unwrap();
    assert!(expected.status.success(), "{:?}", expected);
    let expected = String::from_utf8(expected.stdout).unwrap();
    assert!(expected.contains("<title>Explicit preview title</title>"));
    assert!(expected.contains("lang=\"fr\""));
    assert!(expected.contains("data-preview=\"kept\""));
    assert!(expected.contains("data:font/ttf;base64,"));
    assert!(!expected.contains("<dl>"));

    for target in ["pdf", "svg", "epub"] {
        let output = dir.path(&format!("styled.{target}"));
        let preview = Preview::start_with_options(&source, &output, target, &options);
        assert!(output.metadata().unwrap().len() > 0);
        let actual = preview
            .html()
            .replace(franken_markdown::watch::RELOAD_SNIPPET, "");
        assert_eq!(
            actual, expected,
            "{target} preview changed the selected HTML options"
        );
    }
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
