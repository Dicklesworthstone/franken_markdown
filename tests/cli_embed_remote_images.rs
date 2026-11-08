//! Native self-contained publications use explicit network authority and one
//! image snapshot. All requests stay on a real loopback HTTP server.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use franken_markdown::{
    HtmlOptions, PdfImageAsset, PdfOptions, parse_markdown, render_epub, render_pdf,
};

// Independently encoded 2x2 RGB PNGs. Changing responses expose accidental
// repeat fetches that would give HTML and PDF different versions of a figure.
const RED: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x02\x00\x00\x00\xfd\xd4\x9a\x73\x00\x00\x00\x10IDAT\x78\x9c\x63\x38\x21\xc2\x05\x44\x0c\x10\x0a\x00\x1c\x2e\x03\x99\x40\x9c\x9e\xf4\x00\x00\x00\x00IEND\xae\x42\x60\x82";
const BLUE: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x02\x00\x00\x00\xfd\xd4\x9a\x73\x00\x00\x00\x10IDAT\x78\x9c\x63\xe0\x32\xba\x03\x44\x0c\x10\x0a\x00\x1b\x66\x04\x61\x9e\x7f\xab\x2a\x00\x00\x00\x00IEND\xae\x42\x60\x82";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-embed-remote-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.0.join(path)).unwrap()
    }

    fn text(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap()
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .arg("--no-config")
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env("NO_PROXY", "127.0.0.1")
            .env("no_proxy", "127.0.0.1");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
}

struct Server {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&requests);
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("loopback listener: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while request.len() < 8192 && !request.windows(4).any(|bytes| bytes == b"\r\n\r\n")
                {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => request.extend_from_slice(&buffer[..count]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let Some(path) = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                else {
                    continue;
                };
                let occurrence = {
                    let mut requests = seen.lock().unwrap();
                    requests.push(path.to_owned());
                    requests.iter().filter(|request| *request == path).count()
                };
                if path == "/oversized.png" {
                    // No Content-Length: the streaming reader must cap bytes.
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                    let _ = stream.write_all(&[42; 4096]);
                    continue;
                }
                let (status, body) = if path == "/missing.png" {
                    ("404 Not Found", b"missing".as_slice())
                } else if path == "/changing.png" && occurrence > 1 {
                    ("200 OK", BLUE)
                } else {
                    ("200 OK", RED)
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if stream.write_all(header.as_bytes()).is_ok() {
                    let _ = stream.write_all(body);
                }
            }
        });
        Self {
            base,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn image_uri(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        franken_markdown::html::base64_encode(bytes)
    )
}

fn stored_pngs(bytes: &[u8]) -> Vec<&[u8]> {
    let mut images = Vec::new();
    let mut at = 0;
    while bytes.get(at..at + 4) == Some(b"PK\x03\x04".as_slice()) {
        let word =
            |offset| u16::from_le_bytes(bytes[at + offset..at + offset + 2].try_into().unwrap());
        let size = u32::from_le_bytes(bytes[at + 18..at + 22].try_into().unwrap()) as usize;
        assert_eq!(word(6) & 8, 0, "known-size ZIP members");
        let name_end = at + 30 + usize::from(word(26));
        let name = std::str::from_utf8(&bytes[at + 30..name_end]).unwrap();
        let start = name_end + usize::from(word(28));
        if name.ends_with(".png") {
            assert_eq!(word(8), 0, "PNG is stored without recompression");
            images.push(&bytes[start..start + size]);
        }
        at = start + size;
    }
    assert_eq!(bytes.get(at..at + 4), Some(b"PK\x01\x02".as_slice()));
    images
}

#[test]
fn ordinary_html_epub_and_book_publications_keep_offline_defaults() {
    let server = Server::start();
    let workspace = Workspace::new();
    let source = format!(
        "# Offline default\n\n![Figure]({})\n",
        server.url("/figure.png")
    );
    workspace.write("document.md", &source);
    workspace.write("book/chapter.md", &source);
    for format in ["html", "epub"] {
        succeeded(&workspace.run(&[
            "document.md",
            "--to",
            format,
            "--out",
            &format!("document.{format}"),
        ]));
    }
    assert!(
        workspace
            .text("document.html")
            .contains(&server.url("/figure.png"))
    );
    assert!(stored_pngs(&workspace.bytes("document.epub")).is_empty());
    for format in ["html", "pdf", "epub"] {
        succeeded(&workspace.run(&[
            "book",
            "book",
            "--to",
            format,
            "--out-dir",
            &format!("default-{format}"),
        ]));
    }
    assert!(
        server.requests().is_empty(),
        "no opt-in means no book or HTML/EPUB requests"
    );
}

#[test]
fn html_embeds_included_raw_html_table_and_footnote_images_once() {
    let server = Server::start();
    let workspace = Workspace::new();
    let url = server.url("/figure.png?version=7&size=2");
    workspace.write("document.md", "# Portable\n\n{{#include part.md}}\n\nEvidence[^note].\n\n[^note]: ![Supporting figure](PLACEHOLDER)\n".replace("PLACEHOLDER", &url));
    workspace.write("part.md", format!("![Figure]({url})\n\n<img src='{}' alt='Raw'>\n\n| Figure |\n| --- |\n| [![Linked]({url})](https://example.com/details) |\n", url.replace('&', "&amp;")));
    let output = workspace.run(&[
        "document.md",
        "--embed-remote-images",
        "--out",
        "portable.html",
        "--json",
    ]);
    succeeded(&output);
    let html = workspace.text("portable.html");
    assert_eq!(html.matches(&image_uri(RED)).count(), 4);
    assert!(!html.contains("src=\"http://127.0.0.1"));
    assert!(html.contains("href=\"https://example.com/details\""));
    assert_eq!(server.requests(), ["/figure.png?version=7&size=2"]);
}

#[test]
fn combined_output_shares_one_snapshot_even_when_the_server_changes() {
    let server = Server::start();
    let workspace = Workspace::new();
    let url = server.url("/changing.png");
    let source = format!("# Shared snapshot\n\n![First]({url})\n\n![Repeated]({url})\n");
    workspace.write("document.md", &source);
    succeeded(&workspace.run(&[
        "document.md",
        "--to",
        "both",
        "--embed-remote-images",
        "--out",
        "combined.html",
    ]));
    assert_eq!(server.requests(), ["/changing.png"]);
    assert_eq!(
        workspace
            .text("combined.html")
            .matches(&image_uri(RED))
            .count(),
        2
    );
    let options = PdfOptions {
        image_assets: vec![PdfImageAsset::new(url, RED.to_vec())],
        metadata_epoch_seconds: Some(1700000000),
        ..PdfOptions::default()
    };
    assert!(
        workspace.bytes("combined.pdf") == render_pdf(&source, &options).unwrap(),
        "PDF must use the same red pixels as HTML"
    );
}

#[test]
fn epub_contains_real_remote_bytes_and_remains_deterministic_for_fixed_responses() {
    let server = Server::start();
    let workspace = Workspace::new();
    let url = server.url("/figure.png");
    let source = format!("# Packaged\n\n![Figure]({url})\n\n![Again]({url})\n");
    workspace.write("document.md", &source);
    let args = [
        "document.md",
        "--to",
        "epub",
        "--embed-remote-images",
        "--out",
        "portable.epub",
    ];
    succeeded(&workspace.run(&args));
    let bytes = workspace.bytes("portable.epub");
    assert_eq!(stored_pngs(&bytes), [RED]);
    let options = HtmlOptions {
        image_assets: vec![PdfImageAsset::new(url, RED.to_vec())],
        ..HtmlOptions::default()
    };
    assert!(
        bytes == render_epub(&parse_markdown(&source), &options).unwrap(),
        "native publication must contain the same complete resources as the byte-only API"
    );
    succeeded(&workspace.run(&args));
    assert!(bytes == workspace.bytes("portable.epub"));
    assert_eq!(server.requests(), ["/figure.png", "/figure.png"]);
}

#[test]
fn explicit_query_url_overrides_win_without_authorizing_network_fallback() {
    let server = Server::start();
    let workspace = Workspace::new();
    let url = server.url("/figure.png?version=7");
    let source = format!("# Mapped\n\n![Figure]({url})\n");
    workspace.write("document.md", &source);
    workspace.write("book/chapter.md", &source);
    workspace.write("image=blue.png", BLUE);
    let mapping = format!("{url}=image=blue.png");
    for format in ["html", "both", "epub"] {
        let output = if format == "epub" {
            "mapped.epub"
        } else {
            "mapped.html"
        };
        succeeded(&workspace.run(&[
            "document.md",
            "--to",
            format,
            "--embed-remote-images",
            "--pdf-image",
            &mapping,
            "--out",
            output,
        ]));
        if format == "epub" {
            assert_eq!(stored_pngs(&workspace.bytes(output)), [BLUE]);
        } else {
            assert!(workspace.text(output).contains(&image_uri(BLUE)));
        }
    }
    succeeded(&workspace.run(&[
        "book",
        "book",
        "--to",
        "epub",
        "--embed-remote-images",
        "--pdf-image",
        &mapping,
        "--out",
        "book.epub",
        "--json",
    ]));
    assert_eq!(stored_pngs(&workspace.bytes("book.epub")), [BLUE]);
    workspace.write("image=blue.png", b"invalid explicit bytes");
    succeeded(&workspace.run(&[
        "document.md",
        "--to",
        "epub",
        "--embed-remote-images",
        "--pdf-image",
        &mapping,
        "--out",
        "invalid.epub",
    ]));
    assert!(stored_pngs(&workspace.bytes("invalid.epub")).is_empty());
    assert!(
        server.requests().is_empty(),
        "invalid explicit bytes still own their destination"
    );
}

#[test]
fn failed_or_oversized_fetches_warn_once_per_url_and_keep_document_content() {
    let server = Server::start();
    let workspace = Workspace::new();
    let missing = server.url("/missing.png");
    let oversized = server.url("/oversized.png");
    let source = format!(
        "# Retained\n\nReadable body.\n\n![Missing]({missing})\n\n![Repeated]({missing})\n\n![Large]({oversized})\n"
    );
    workspace.write("document.md", &source);
    for format in ["html", "epub", "both"] {
        let path = if format == "epub" {
            "fallback.epub"
        } else {
            "fallback.html"
        };
        let output = workspace.run(&[
            "document.md",
            "--to",
            format,
            "--embed-remote-images",
            "--max-pdf-image-bytes",
            "128",
            "--out",
            path,
            "--json",
        ]);
        succeeded(&output);
        assert_eq!(
            String::from_utf8_lossy(&output.stderr)
                .matches("\"warning\":\"remote_image_fetch_failed\"")
                .count(),
            2
        );
        if format == "epub" {
            assert!(stored_pngs(&workspace.bytes(path)).is_empty());
            assert!(
                workspace.bytes(path)
                    == render_epub(&parse_markdown(&source), &HtmlOptions::default()).unwrap()
            );
        } else {
            assert!(workspace.text(path).contains("Readable body."));
            assert!(workspace.text(path).contains("alt=\"Missing\""));
        }
    }
    assert_eq!(
        server.requests(),
        [
            "/missing.png",
            "/oversized.png",
            "/missing.png",
            "/oversized.png",
            "/missing.png",
            "/oversized.png"
        ]
    );
}

#[test]
fn book_exports_fetch_once_across_chapters_and_remove_stale_missing_image_warnings() {
    let server = Server::start();
    let workspace = Workspace::new();
    let url = server.url("/figure.png");
    workspace.write("book/a.md", format!("# First\n\n![Figure]({url})\n"));
    workspace.write(
        "book/b.md",
        format!("# Second\n\n<img src='{url}' alt='Repeated'>\n"),
    );
    for format in ["both", "epub"] {
        let output = workspace.run(&[
            "book",
            "book",
            "--to",
            format,
            "--embed-remote-images",
            "--out-dir",
            "publication",
            "--json",
        ]);
        succeeded(&output);
        let receipt = String::from_utf8_lossy(&output.stdout);
        assert!(receipt.contains("\"images\":1"), "{receipt}");
        assert!(!receipt.contains("image_not_loaded"), "{receipt}");
        assert!(!receipt.contains("remote_image_fetch_failed"), "{receipt}");
    }
    assert!(
        workspace
            .text("publication/a.html")
            .contains(&image_uri(RED))
    );
    assert!(
        workspace
            .text("publication/b.html")
            .contains(&image_uri(RED))
    );
    assert!(
        String::from_utf8_lossy(&workspace.bytes("publication/book.pdf"))
            .contains("/Subtype /Image")
    );
    assert_eq!(
        stored_pngs(&workspace.bytes("publication/book.epub")),
        [RED]
    );
    assert_eq!(server.requests(), ["/figure.png", "/figure.png"]);
}

#[test]
fn conflicting_network_flags_fail_before_fetching_or_replacing_output() {
    let server = Server::start();
    let workspace = Workspace::new();
    let source = format!("![Figure]({})\n", server.url("/figure.png"));
    workspace.write("document.md", &source);
    workspace.write("book/chapter.md", &source);
    workspace.write("existing.html", b"previous publication");
    for args in [
        vec![
            "document.md",
            "--out",
            "existing.html",
            "--embed-remote-images",
            "--no-remote-images",
        ],
        vec![
            "book",
            "book",
            "--to",
            "epub",
            "--out",
            "existing.epub",
            "--embed-remote-images",
            "--no-remote-images",
        ],
        vec!["book", "book", "--check-links", "--embed-remote-images"],
        vec!["book", "book", "--remote-image-timeout-secs", "1"],
    ] {
        let output = workspace.run(&args);
        assert_eq!(
            output.status.code(),
            Some(64),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(workspace.bytes("existing.html"), b"previous publication");
    assert!(!workspace.0.join("existing.epub").exists());
    assert!(server.requests().is_empty());
}

struct Watch(Child);
impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn watch_rebuilds_embed_a_fresh_snapshot_after_source_changes() {
    let server = Server::start();
    let workspace = Workspace::new();
    let source = format!("# First\n\n![Figure]({})\n", server.url("/changing.png"));
    workspace.write("document.md", &source);
    let mut child = Watch(
        workspace
            .command()
            .args([
                "watch",
                "document.md",
                "--out",
                "watched.html",
                "--embed-remote-images",
                "--interval",
                "25",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let wait = |child: &mut Watch, expected: &[u8]| {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !fs::read_to_string(workspace.0.join("watched.html"))
            .is_ok_and(|html| html.contains(&image_uri(expected)))
        {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "watch exited before publication"
            );
            assert!(
                Instant::now() < deadline,
                "watch did not embed the expected figure"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&mut child, RED);
    workspace.write("document.md", source.replace("# First", "# Updated"));
    wait(&mut child, BLUE);
    assert_eq!(server.requests(), ["/changing.png", "/changing.png"]);
}
