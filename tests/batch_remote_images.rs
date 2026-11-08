//! Real native batch image fetching and diagnostics, using loopback HTTP only.
#![cfg(feature = "batch")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use franken_markdown::batch::{
    BatchMode, BatchOptions, BatchPlan, OutputFormat, run_batch_blocking,
};
use franken_markdown::{HtmlOptions, PdfOptions};

// Independently encoded 2x2 RGB PNGs, with distinct pixel colors. HTTP serves
// RED; the explicit mapping case supplies BLUE and compares real PDF bytes.
const RED: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x02\x00\x00\x00\xfd\xd4\x9a\x73\x00\x00\x00\x10IDAT\x78\x9c\x63\x38\x21\xc2\x05\x44\x0c\x10\x0a\x00\x1c\x2e\x03\x99\x40\x9c\x9e\xf4\x00\x00\x00\x00IEND\xae\x42\x60\x82";
const BLUE: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x02\x00\x00\x00\xfd\xd4\x9a\x73\x00\x00\x00\x10IDAT\x78\x9c\x63\xe0\x32\xba\x03\x44\x0c\x10\x0a\x00\x1b\x66\x04\x61\x9e\x7f\xab\x2a\x00\x00\x00\x00IEND\xae\x42\x60\x82";

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-batch-remote-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn bytes(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
    }

    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .arg("--no-config")
            .args(args)
            .current_dir(&self.0)
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env("NO_PROXY", "127.0.0.1")
            .env("no_proxy", "127.0.0.1")
            .output()
            .unwrap()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct ImageServer {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    disconnected: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ImageServer {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let disconnected = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&requests);
        let closed = Arc::clone(&disconnected);
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut connections = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        assert!(connections.len() < 128, "unexpected repeated HTTP requests");
                        let seen = Arc::clone(&seen);
                        let closed = Arc::clone(&closed);
                        let stopped = Arc::clone(&stopped);
                        connections.push(std::thread::spawn(move || {
                            serve_image(stream, &seen, &closed, &stopped);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
            for connection in connections {
                connection.join().unwrap();
            }
        });
        Self {
            port,
            requests,
            disconnected,
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    fn wait_disconnected(&self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.disconnected.load(Ordering::Acquire) == 0 {
            assert!(
                Instant::now() < deadline,
                "the cancelled downloader kept its HTTP connection open"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for ImageServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            // Do not mask an earlier assertion with a second panic in Drop.
            let _ = thread.join();
        }
    }
}

fn serve_image(
    mut stream: TcpStream,
    requests: &Mutex<Vec<String>>,
    disconnected: &AtomicU64,
    stopped: &AtomicBool,
) {
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let start = Instant::now();
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    while request.len() < 8192
        && start.elapsed() < Duration::from_secs(2)
        && !stopped.load(Ordering::Acquire)
        && !request.windows(4).any(|part| part == b"\r\n\r\n")
    {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(count) => request.extend_from_slice(&chunk[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
    let request = String::from_utf8_lossy(&request);
    let Some(path) = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return;
    };
    requests.lock().unwrap().push(path.to_owned());
    if path == "/slow.png" {
        // No headers/body yet: only client cancellation/timeout can complete
        // before six seconds. Observe the socket closing, not a mock flag.
        let deadline = Instant::now() + Duration::from_secs(6);
        while Instant::now() < deadline && !stopped.load(Ordering::Acquire) {
            match stream.read(&mut chunk) {
                Ok(0) => {
                    disconnected.fetch_add(1, Ordering::Release);
                    return;
                }
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => {
                    disconnected.fetch_add(1, Ordering::Release);
                    return;
                }
            }
        }
        return;
    }
    if path == "/delayed-missing.png" {
        std::thread::sleep(Duration::from_millis(100));
    }
    if path == "/redirect.png" {
        let _ = stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /figure.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    }
    if path == "/oversized.png" {
        // No Content-Length: the streaming reader must enforce the byte cap.
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(&[42; 4096]);
        return;
    }
    let (status, body): (&str, &[u8]) = match path {
        "/missing.png" | "/delayed-missing.png" => ("404 Not Found", b"not found"),
        "/invalid.png" => ("200 OK", b"this is not a PNG"),
        _ => ("200 OK", RED),
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(header.as_bytes()).is_ok() {
        let _ = stream.write_all(body);
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}; stdout={}; stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn image_count(pdf: &[u8]) -> usize {
    let marker = b"/Subtype /Image";
    pdf.windows(marker.len())
        .filter(|part| *part == marker)
        .count()
}

fn warning_codes(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| {
            line.split_once("\"warning\":\"")?
                .1
                .split('"')
                .next()
                .map(str::to_owned)
        })
        .collect()
}

#[cfg(unix)]
#[test]
fn wget_personal_credential_helpers_do_not_run_during_native_image_fetches() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let wget = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join("wget"))
            .find(|candidate| candidate.is_file())
            .and_then(|candidate| candidate.canonicalize().ok())
    });
    let Some(wget) = wget.filter(|path| {
        Command::new(path)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.stdout.starts_with(b"GNU Wget "))
    }) else {
        eprintln!("GNU wget unavailable; its native credential-helper regression is skipped");
        return;
    };

    let server = ImageServer::start();
    let dir = Directory::new();
    let url = server.url("/figure.png");
    dir.write("source.md", format!("![Figure]({url})\n"));
    // Use the actual GNU wget binary. A private PATH forces its fallback path
    // without replacing curl with a script or changing the test process env.
    dir.write(
        "bin/askpass",
        b"#!/bin/sh\nprintf 'called\\n' >> \"$FMD_ASKPASS_MARKER\"\nprintf credential\n",
    );
    std::fs::set_permissions(
        dir.0.join("bin/askpass"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    symlink(&wget, dir.0.join("bin/wget")).unwrap();
    dir.write("wgetrc", b"use_askpass = askpass\n");
    let configure = |command: &mut Command| {
        command
            .current_dir(&dir.0)
            .env("PATH", dir.0.join("bin"))
            .env("WGETRC", dir.0.join("wgetrc"))
            .env("FMD_ASKPASS_MARKER", dir.0.join("helper-invoked"))
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env("NO_PROXY", "127.0.0.1")
            .env("no_proxy", "127.0.0.1");
    };

    // Establish that this real downloader reads the config and launches the
    // helper without our explicit policy. The helper exits immediately; this
    // regression does not create an orphan or wait for a simulated timeout.
    let mut control = Command::new(&wget);
    configure(&mut control);
    let control = control
        .args([
            "--quiet",
            "--tries=1",
            "--timeout=2",
            "--output-document=-",
            "--",
            &url,
        ])
        .output()
        .unwrap();
    succeeded(&control);
    assert_eq!(control.stdout, RED);
    assert!(!dir.bytes("helper-invoked").is_empty());
    dir.write("helper-invoked", b"");

    for args in [
        vec!["source.md", "--to", "pdf", "--out", "single.pdf", "--json"],
        vec![
            "batch",
            "source.md",
            "--to",
            "pdf",
            "--out-dir",
            "batch",
            "--json",
        ],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        configure(&mut command);
        let output = command.arg("--no-config").args(args).output().unwrap();
        succeeded(&output);
        assert!(warning_codes(&output).is_empty());
        assert!(
            dir.bytes("helper-invoked").is_empty(),
            "native image fetching must not activate ambient credential helpers"
        );
    }
    assert_eq!(image_count(&dir.bytes("single.pdf")), 1);
    assert!(dir.bytes("single.pdf") == dir.bytes("batch/source.pdf"));
    assert_eq!(
        server.requests(),
        ["/figure.png", "/figure.png", "/figure.png"]
    );
}

#[test]
fn batch_remote_images_match_single_output_with_includes_raw_html_and_repeated_urls() {
    let server = ImageServer::start();
    let dir = Directory::new();
    let url = server.url("/figure.png");
    dir.write("input/a.md", "# A\n\n{{#include parts/figure.txt}}\n");
    dir.write(
        "input/parts/figure.txt",
        format!("![Figure]({url})\n\nBefore <img src=\"{url}\" alt=\"Badge\"> after.\n"),
    );
    dir.write(
        "input/b.md",
        format!("# B\n\n| Badge |\n| --- |\n| [![Icon]({url})](https://example.com) |\n"),
    );
    for name in ["a", "b"] {
        succeeded(&dir.run(&[
            &format!("input/{name}.md"),
            "--to",
            "both",
            "--out",
            &format!("single-{name}.html"),
        ]));
    }
    assert_eq!(server.requests(), ["/figure.png", "/figure.png"]);
    let args = [
        "batch",
        "input",
        "--to",
        "both",
        "--out-dir",
        "output",
        "--workers",
        "2",
        "--json",
    ];
    let first = dir.run(&args);
    succeeded(&first);
    assert!(
        warning_codes(&first).is_empty(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(
        server.requests().len(),
        4,
        "one fetch per distinct destination in each document"
    );
    for name in ["a", "b"] {
        let pdf = dir.bytes(&format!("output/{name}.pdf"));
        assert!(image_count(&pdf) > 0, "remote pixels must reach the PDF");
        assert!(
            pdf == dir.bytes(&format!("single-{name}.pdf")),
            "single/batch PDF parity for {name}"
        );
        let html = dir.text(&format!("output/{name}.html"));
        assert!(
            html == dir.text(&format!("single-{name}.html")),
            "single/batch HTML parity for {name}"
        );
        assert!(html.contains(&url));
        assert!(
            !html.contains("data:image/png"),
            "PDF-only fetched assets must not change HTML policy"
        );
    }
    let repeated = dir.run(&args);
    succeeded(&repeated);
    assert_eq!(
        first.stdout, repeated.stdout,
        "fixed bytes produce deterministic receipts"
    );
    assert_eq!(server.requests().len(), 6);
}

#[test]
fn batch_svg_preserves_remote_images_and_uses_the_same_width_and_viewbox_as_single_render() {
    let server = ImageServer::start();
    let dir = Directory::new();
    let url = server.url("/figure.png");
    dir.write(
        "source.md",
        format!("# Poster\n\n![First]({url})\n\n![Second]({url})\n"),
    );
    for (width, rendered_width, adjusted) in [
        ("300", "300.00", false),
        ("900", "900.00", false),
        ("0", "612.00", true),
    ] {
        let single = dir.run(&[
            "source.md",
            "--to",
            "svg",
            "--svg-width-pt",
            width,
            "--out",
            "single.svg",
            "--json",
        ]);
        succeeded(&single);
        let run = dir.run(&[
            "batch",
            "source.md",
            "--to",
            "svg",
            "--svg-width-pt",
            width,
            "--out-dir",
            "out",
            "--json",
        ]);
        succeeded(&run);
        let svg = dir.text("out/source.svg");
        assert!(
            svg == dir.text("single.svg"),
            "ordinary/batch SVG parity at width {width}"
        );
        let root = svg
            .split_once("<svg ")
            .unwrap()
            .1
            .split_once('>')
            .unwrap()
            .0;
        assert!(
            root.contains(&format!("width=\"{rendered_width}pt\"")),
            "{root}"
        );
        assert!(
            root.contains(&format!("viewBox=\"0 0 {rendered_width} ")),
            "{root}"
        );
        assert!(svg.contains(&format!(
            "data:image/png;base64,{}",
            franken_markdown::html::base64_encode(RED)
        )));
        assert_eq!(
            svg.matches("data:image/png;base64,").count(),
            1,
            "repeated references reuse the original image resource"
        );
        let expected = if adjusted {
            vec!["svg_layout_adjusted"]
        } else {
            Vec::new()
        };
        assert_eq!(warning_codes(&run), expected);
        assert_eq!(warning_codes(&single), warning_codes(&run));
    }
    assert_eq!(server.requests(), ["/figure.png"; 6]);
}

#[test]
fn batch_svg_reports_fetch_and_renderer_failures_and_obeys_remote_opt_out() {
    let server = ImageServer::start();
    let dir = Directory::new();
    dir.write(
        "a.md",
        format!("![Missing]({})\n", server.url("/missing.png")),
    );
    dir.write(
        "z.md",
        format!("![Invalid]({})\n", server.url("/invalid.png")),
    );
    let run = dir.run(&[
        "batch",
        "z.md",
        "a.md",
        "--to",
        "svg",
        "--out-dir",
        "out",
        "--workers",
        "2",
        "--json",
    ]);
    succeeded(&run);
    assert_eq!(
        warning_codes(&run),
        [
            "remote_image_fetch_failed",
            "svg_image_missing",
            "svg_image_invalid"
        ]
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    let warnings: Vec<_> = stderr.lines().collect();
    assert!(
        warnings[..2]
            .iter()
            .all(|line| line.contains("\"input\":\"a.md\""))
    );
    assert!(warnings[2].contains("\"input\":\"z.md\""));
    assert_eq!(server.requests().len(), 2);
    assert!(!dir.text("out/a.svg").contains("<image"));
    assert!(!dir.text("out/z.svg").contains("<image"));

    dir.write(
        "offline.md",
        format!("![Figure]({})\n", server.url("/figure.png")),
    );
    let run = dir.run(&[
        "batch",
        "offline.md",
        "--to",
        "svg",
        "--out-dir",
        "out",
        "--no-remote-images",
        "--json",
    ]);
    succeeded(&run);
    assert_eq!(warning_codes(&run), ["svg_image_missing"]);
    assert!(!dir.text("out/offline.svg").contains("<image"));
    assert_eq!(
        server.requests().len(),
        2,
        "SVG opt-out must not contact the server"
    );
}

#[test]
fn explicit_overrides_and_opt_out_never_fetch_even_when_override_bytes_are_invalid() {
    let server = ImageServer::start();
    let dir = Directory::new();
    let url = server.url("/figure.png");
    dir.write("source.md", format!("# Mapped\n\n![Figure]({url})\n"));
    dir.write("blue.png", BLUE);
    let mapping = format!("{url}=blue.png");
    succeeded(&dir.run(&[
        "source.md",
        "--to",
        "pdf",
        "--out",
        "single.pdf",
        "--pdf-image",
        &mapping,
    ]));
    let run = dir.run(&[
        "batch",
        "source.md",
        "--to",
        "pdf",
        "--out-dir",
        "mapped",
        "--pdf-image",
        &mapping,
        "--json",
    ]);
    succeeded(&run);
    assert_eq!(image_count(&dir.bytes("mapped/source.pdf")), 1);
    assert!(dir.bytes("single.pdf") == dir.bytes("mapped/source.pdf"));
    assert!(server.requests().is_empty());

    dir.write("blue.png", b"invalid explicit bytes");
    let run = dir.run(&[
        "batch",
        "source.md",
        "--to",
        "pdf",
        "--out-dir",
        "invalid",
        "--pdf-image",
        &mapping,
        "--json",
    ]);
    succeeded(&run);
    assert_eq!(warning_codes(&run), ["unsupported_image"]);
    assert_eq!(image_count(&dir.bytes("invalid/source.pdf")), 0);
    assert!(
        server.requests().is_empty(),
        "invalid explicit bindings must not authorize a fallback fetch"
    );

    let run = dir.run(&[
        "batch",
        "source.md",
        "--to",
        "pdf",
        "--out-dir",
        "offline",
        "--no-remote-images",
        "--json",
    ]);
    succeeded(&run);
    assert_eq!(warning_codes(&run), ["unresolved_image"]);
    assert_eq!(image_count(&dir.bytes("offline/source.pdf")), 0);
    assert!(server.requests().is_empty());
}

#[test]
fn failed_fetches_and_pdf_warnings_are_reported_in_input_order_without_changing_receipts() {
    let server = ImageServer::start();
    let dir = Directory::new();
    dir.write(
        "a.md",
        format!("![Slow missing]({})\n", server.url("/delayed-missing.png")),
    );
    dir.write(
        "z.md",
        format!("![Fast missing]({})\n", server.url("/missing.png")),
    );
    let args = [
        "batch",
        "z.md",
        "a.md",
        "--to",
        "pdf",
        "--out-dir",
        "out",
        "--workers",
        "2",
        "--json",
    ];
    let first = dir.run(&args);
    succeeded(&first);
    assert_eq!(
        warning_codes(&first),
        [
            "remote_image_fetch_failed",
            "unresolved_image",
            "remote_image_fetch_failed",
            "unresolved_image"
        ]
    );
    let warnings = String::from_utf8_lossy(&first.stderr);
    let lines: Vec<_> = warnings.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(
        lines[..2]
            .iter()
            .all(|line| line.contains("\"input\":\"a.md\""))
    );
    assert!(
        lines[2..]
            .iter()
            .all(|line| line.contains("\"input\":\"z.md\""))
    );
    let receipt = String::from_utf8_lossy(&first.stdout);
    assert_eq!(receipt.lines().count(), 1);
    assert!(receipt.contains("\"ok\":2,\"failed\":0,\"skipped\":0"));
    assert!(!receipt.contains("\"warning\""));
    let second = dir.run(&args);
    succeeded(&second);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stderr, second.stderr);
    assert_eq!(server.requests().len(), 4);
}

#[test]
fn byte_limits_and_invalid_remote_images_keep_fallbacks_and_warn() {
    let server = ImageServer::start();
    let dir = Directory::new();
    let large = server.url("/oversized.png");
    dir.write(
        "large.md",
        format!("![Too large]({large})\n\n![Repeated]({large})\n"),
    );
    dir.write(
        "invalid.md",
        format!("![Invalid]({})\n", server.url("/invalid.png")),
    );
    let run = dir.run(&[
        "batch",
        "large.md",
        "invalid.md",
        "--to",
        "pdf",
        "--out-dir",
        "out",
        "--max-pdf-image-bytes",
        "128",
        "--json",
    ]);
    succeeded(&run);
    assert_eq!(
        warning_codes(&run),
        [
            "unsupported_image",
            "remote_image_fetch_failed",
            "unresolved_image",
            "unresolved_image"
        ]
    );
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|path| *path == "/oversized.png")
            .count(),
        1,
        "failed URLs are deduplicated too"
    );
    assert_eq!(image_count(&dir.bytes("out/large.pdf")), 0);
    assert_eq!(image_count(&dir.bytes("out/invalid.pdf")), 0);
}

#[test]
fn per_image_timeout_reaps_the_connection_then_allows_other_images_to_render() {
    let server = ImageServer::start();
    let dir = Directory::new();
    dir.write(
        "source.md",
        format!(
            "![Slow]({})\n\n![Good]({})\n",
            server.url("/slow.png"),
            server.url("/redirect.png")
        ),
    );
    let started = Instant::now();
    let run = dir.run(&[
        "batch",
        "source.md",
        "--to",
        "pdf",
        "--out-dir",
        "out",
        "--remote-image-timeout-secs",
        "1",
        "--json",
    ]);
    succeeded(&run);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the download must honor its one-second deadline"
    );
    assert_eq!(
        warning_codes(&run),
        ["remote_image_fetch_failed", "unresolved_image"]
    );
    assert_eq!(image_count(&dir.bytes("out/source.pdf")), 1);
    assert_eq!(
        server.requests(),
        ["/slow.png", "/redirect.png", "/figure.png"]
    );
    server.wait_disconnected();
}

#[test]
fn batch_deadline_cancels_image_preparation_and_preserves_both_previous_outputs() {
    let server = ImageServer::start();
    let dir = Directory::new();
    dir.write(
        "a.md",
        format!(
            "# A\n\n![Slow]({})\n\n![Never requested]({})\n",
            server.url("/slow.png"),
            server.url("/figure.png")
        ),
    );
    dir.write("b.md", "# B\n");
    dir.write("out/a.html", b"previous complete HTML");
    dir.write("out/a.pdf", b"previous complete PDF");
    let started = Instant::now();
    let run = dir.run(&[
        "batch",
        "a.md",
        "b.md",
        "--to",
        "both",
        "--out-dir",
        "out",
        "--workers",
        "1",
        "--timeout",
        "1",
        "--remote-image-timeout-secs",
        "20",
        "--json",
    ]);
    assert_eq!(
        run.status.code(),
        Some(70),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "batch cancellation must interrupt the pending fetch"
    );
    let receipt = String::from_utf8_lossy(&run.stdout);
    assert!(receipt.contains("\"ok\":0,\"failed\":0,\"skipped\":2,\"cancelled\":true"));
    assert!(
        warning_codes(&run).is_empty(),
        "cancellation must not be reported as an image fallback"
    );
    assert_eq!(server.requests(), ["/slow.png"]);
    server.wait_disconnected();
    assert_eq!(dir.bytes("out/a.html"), b"previous complete HTML");
    assert_eq!(dir.bytes("out/a.pdf"), b"previous complete PDF");
    assert!(!dir.0.join("out/b.html").exists());
    assert!(!dir.0.join("out/b.pdf").exists());
}

#[test]
fn html_epub_and_public_library_batch_jobs_remain_offline() {
    let server = ImageServer::start();
    let dir = Directory::new();
    dir.write(
        "source.md",
        format!("![Figure]({})\n", server.url("/figure.png")),
    );
    for target in ["html", "epub"] {
        succeeded(&dir.run(&[
            "batch",
            "source.md",
            "--to",
            target,
            "--out-dir",
            target,
            "--json",
        ]));
    }
    let options = BatchOptions {
        html: HtmlOptions::default(),
        pdf: PdfOptions::default(),
        mode: BatchMode::Throughput,
        workers: Some(1),
        mem_budget: None,
        continue_on_error: false,
        timeout_secs: None,
        max_input_bytes: 1024 * 1024,
        max_pdf_image_bytes: 1024 * 1024,
    };
    let receipt = run_batch_blocking(
        BatchPlan {
            inputs: vec![dir.0.join("source.md")],
            format: OutputFormat::Pdf,
            out_dir: Some(dir.0.join("library")),
        },
        &options,
    )
    .unwrap();
    assert_eq!(receipt.ok_count(), 1);
    assert_eq!(image_count(&dir.bytes("library/source.pdf")), 0);
    let receipt = run_batch_blocking(
        BatchPlan {
            inputs: vec![dir.0.join("source.md")],
            format: OutputFormat::Svg,
            out_dir: Some(dir.0.join("library")),
        },
        &options,
    )
    .unwrap();
    assert_eq!(receipt.ok_count(), 1);
    assert!(!dir.text("library/source.svg").contains("<image"));
    assert!(
        server.requests().is_empty(),
        "only explicit CLI PDF authority may fetch images"
    );
}

#[test]
fn input_and_source_protection_failures_do_not_fetch_remote_images() {
    let server = ImageServer::start();
    let dir = Directory::new();
    dir.write(
        "a.md",
        format!(
            "![Remote]({})\n\n{{{{#include missing.txt}}}}\n",
            server.url("/figure.png")
        ),
    );
    let run = dir.run(&[
        "batch",
        "a.md",
        "--to",
        "pdf",
        "--out-dir",
        "bad-input",
        "--json",
    ]);
    assert_eq!(run.status.code(), Some(66));
    assert!(server.requests().is_empty());

    dir.write(
        "a.md",
        format!("![Remote]({})\n", server.url("/figure.png")),
    );
    dir.write("a.pdf", b"source used as another document's include");
    dir.write("b.md", "{{#include a.pdf}}\n");
    let run = dir.run(&[
        "batch",
        "a.md",
        "b.md",
        "--to",
        "pdf",
        "--workers",
        "2",
        "--json",
    ]);
    assert_eq!(run.status.code(), Some(73));
    assert_eq!(
        dir.bytes("a.pdf"),
        b"source used as another document's include"
    );
    assert!(
        server.requests().is_empty(),
        "a refused producer must not fetch before its source guard"
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("refusing to overwrite"));
}

#[test]
fn opted_in_html_epub_and_combined_jobs_embed_the_same_assets_as_single_render() {
    let server = ImageServer::start();
    let dir = Directory::new();
    let url = server.url("/figure.png");
    dir.write("source.md", "# Publication\n\n{{#include figure.txt}}\n");
    dir.write(
        "figure.txt",
        format!("![Figure]({url})\n\n<img src='{url}' alt='Repeated'>\n"),
    );
    for format in ["html", "epub", "both"] {
        let extension = if format == "epub" { "epub" } else { "html" };
        let single = format!("single.{extension}");
        succeeded(&dir.run(&[
            "source.md",
            "--to",
            format,
            "--embed-remote-images",
            "--out",
            &single,
        ]));
        let output = dir.run(&[
            "batch",
            "source.md",
            "--to",
            format,
            "--embed-remote-images",
            "--out-dir",
            "embedded",
            "--json",
        ]);
        succeeded(&output);
        assert!(warning_codes(&output).is_empty());
        assert!(
            dir.bytes(&single) == dir.bytes(&format!("embedded/source.{extension}")),
            "single/batch resource parity for {format}"
        );
        if format == "epub" {
            assert!(
                dir.bytes(&single)
                    .windows(RED.len())
                    .any(|bytes| bytes == RED),
                "EPUB must package the actual PNG"
            );
        } else {
            let html = dir.text(&single);
            assert_eq!(html.matches("data:image/png;base64,").count(), 2);
            assert!(!html.contains("src=\"http://127.0.0.1"));
        }
        if format == "both" {
            assert_eq!(image_count(&dir.bytes("embedded/source.pdf")), 1);
            assert!(dir.bytes("single.pdf") == dir.bytes("embedded/source.pdf"));
        }
    }
    assert_eq!(
        server.requests(),
        ["/figure.png"; 6],
        "one snapshot per document, including combined output"
    );
}

#[test]
fn opted_in_batch_overrides_and_conflicting_flags_do_not_fetch() {
    let server = ImageServer::start();
    let dir = Directory::new();
    let url = server.url("/figure.png?version=7");
    dir.write("source.md", format!("![Figure]({url})\n"));
    dir.write("image=blue.png", BLUE);
    let mapping = format!("{url}=image=blue.png");
    let output = dir.run(&[
        "batch",
        "source.md",
        "--to",
        "both",
        "--out-dir",
        "mapped",
        "--embed-remote-images",
        "--pdf-image",
        &mapping,
    ]);
    succeeded(&output);
    assert!(
        dir.text("mapped/source.html")
            .contains(&franken_markdown::html::base64_encode(BLUE))
    );
    assert_eq!(image_count(&dir.bytes("mapped/source.pdf")), 1);
    let saved = dir.bytes("mapped/source.html");
    let rejected = dir.run(&[
        "batch",
        "source.md",
        "--to",
        "both",
        "--out-dir",
        "mapped",
        "--embed-remote-images",
        "--no-remote-images",
    ]);
    assert_eq!(rejected.status.code(), Some(64));
    assert_eq!(dir.bytes("mapped/source.html"), saved);
    assert!(server.requests().is_empty());
}

#[test]
fn failed_opted_in_combined_fetch_is_not_retried_for_the_pdf_sibling() {
    let server = ImageServer::start();
    let dir = Directory::new();
    let url = server.url("/missing.png");
    dir.write(
        "source.md",
        format!("![Missing]({url})\n\n![Repeated]({url})\n"),
    );
    let output = dir.run(&[
        "batch",
        "source.md",
        "--to",
        "both",
        "--out-dir",
        "out",
        "--embed-remote-images",
        "--json",
    ]);
    succeeded(&output);
    assert_eq!(
        warning_codes(&output)
            .iter()
            .filter(|code| *code == "remote_image_fetch_failed")
            .count(),
        1
    );
    assert_eq!(server.requests(), ["/missing.png"]);
    assert!(dir.text("out/source.html").contains("alt=\"Missing\""));
    assert_eq!(image_count(&dir.bytes("out/source.pdf")), 0);
}

#[test]
fn opted_in_batch_deadlines_cancel_before_html_epub_or_combined_publication() {
    for format in ["html", "epub", "both"] {
        let server = ImageServer::start();
        let dir = Directory::new();
        dir.write(
            "source.md",
            format!(
                "![Slow]({})\n\n![Never requested]({})\n",
                server.url("/slow.png"),
                server.url("/figure.png")
            ),
        );
        let extension = if format == "epub" { "epub" } else { "html" };
        let path = format!("out/source.{extension}");
        dir.write(&path, b"previous complete publication");
        dir.write("out/source.pdf", b"previous complete PDF");
        let started = Instant::now();
        let output = dir.run(&[
            "batch",
            "source.md",
            "--to",
            format,
            "--out-dir",
            "out",
            "--embed-remote-images",
            "--workers",
            "1",
            "--timeout",
            "1",
            "--remote-image-timeout-secs",
            "20",
            "--json",
        ]);
        assert_eq!(
            output.status.code(),
            Some(70),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "pending {format} fetch must be interruptible"
        );
        let receipt = String::from_utf8_lossy(&output.stdout);
        assert!(
            receipt.contains("\"ok\":0,\"failed\":0,\"skipped\":1,\"cancelled\":true"),
            "{receipt}"
        );
        assert!(
            warning_codes(&output).is_empty(),
            "cancellation is not an ordinary image fallback"
        );
        assert_eq!(server.requests(), ["/slow.png"]);
        server.wait_disconnected();
        assert_eq!(dir.bytes(&path), b"previous complete publication");
        assert_eq!(dir.bytes("out/source.pdf"), b"previous complete PDF");
    }
}
