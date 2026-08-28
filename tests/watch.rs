//! Filesystem and CLI contract tests for `fmd watch` (j3e0.1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use franken_markdown::watch::{
    ChangeKind, FakeClock, PollWatcher, collect_watch_paths, referenced_local_paths,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn log_check(id: &str, subject: &str, ok: bool) {
    eprintln!(
        "check id={id} subject={subject} outcome={}",
        if ok { "PASS" } else { "FAIL" }
    );
    assert!(ok, "{id}: {subject}");
}

fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fmd-watch-it-{tag}-{}-{}",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fmd(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn rename_into_place_then_css_extra_are_watched() {
    let dir = fresh_dir("rename-css");
    let md = dir.join("doc.md");
    let css = dir.join("t.css");
    let tmp = dir.join("doc.md.tmp");
    std::fs::write(&md, "# a\n").unwrap();
    std::fs::write(&css, "body{}\n").unwrap();
    let clock = FakeClock::new();
    let paths = collect_watch_paths(&md, &[css.clone()]);
    let mut w = PollWatcher::new(paths, Duration::ZERO, clock);
    std::fs::write(&tmp, "# b\n").unwrap();
    std::fs::rename(&tmp, &md).unwrap();
    let events = w.poll();
    log_check(
        "j3e0.1.it.rename",
        "markdown atomic save fires",
        events
            .iter()
            .any(|e| e.path == md && e.kind == ChangeKind::Modified),
    );
    std::fs::write(&css, "body{color:red}\n").unwrap();
    let events = w.poll();
    log_check(
        "j3e0.1.it.css",
        "css write fires",
        events
            .iter()
            .any(|e| e.path == css && e.kind == ChangeKind::Modified),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn watch_referenced_png_next_to_markdown_is_collected() {
    let dir = fresh_dir("img");
    let png = dir.join("hero.png");
    std::fs::write(&png, b"png").unwrap();
    let found = referenced_local_paths("![h](hero.png)\n", &dir);
    log_check(
        "j3e0.1.it.img",
        "local png dest is collected",
        found == [png],
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn watch_help_mentions_interval() {
    let out = fmd(&["watch", "--help"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    log_check(
        "j3e0.1.cli.help",
        "--interval in watch help",
        out.status.success() && stdout.contains("--interval"),
    );
}

#[test]
fn watch_refuses_stdin() {
    let out = fmd(&["watch", "-", "--out", "x.html"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    log_check(
        "j3e0.1.cli.stdin",
        "stdin path is a usage error",
        !out.status.success() && stderr.contains("stdin"),
    );
}

#[test]
fn watch_help_mentions_serve() {
    let out = fmd(&["watch", "--help"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    log_check(
        "xjld.cli.help.serve",
        "--serve in watch help",
        out.status.success() && stdout.contains("--serve"),
    );
}

#[test]
fn watch_serve_loopback_index_injects_reload_snippet() {
    let dir = fresh_dir("serve");
    let md = dir.join("doc.md");
    let html = dir.join("doc.html");
    std::fs::write(&md, "# ServeMe\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args([
            "watch",
            md.to_str().unwrap(),
            "--out",
            html.to_str().unwrap(),
            "--serve",
            "--interval",
            "50",
            "--no-config",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let port = match wait_for_preview_port(stderr, Duration::from_secs(30)) {
        Some(port) => port,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&dir);
            panic!("preview URL never appeared on stderr");
        }
    };
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut body = Vec::new();
    let _ = stream.read_to_end(&mut body);
    let text = String::from_utf8_lossy(&body);
    log_check(
        "xjld.serve.index.snippet",
        "preview HTML carries EventSource reload snippet",
        text.contains("EventSource('/events')"),
    );
    log_check(
        "xjld.serve.index.body",
        "preview HTML contains the document heading",
        text.contains("ServeMe"),
    );
    let written = std::fs::read_to_string(&html).unwrap_or_default();
    log_check(
        "xjld.serve.out.no_snippet",
        "--out file must not include the preview script",
        !written.contains("EventSource"),
    );
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

fn wait_for_preview_port(stderr: impl Read + Send + 'static, timeout: Duration) -> Option<u16> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if let Some(port) = parse_preview_port(&line) {
                let _ = tx.send(port);
                break;
            }
        }
    });
    rx.recv_timeout(timeout).ok()
}

fn parse_preview_port(line: &str) -> Option<u16> {
    let marker = "http://127.0.0.1:";
    let rest = line.split(marker).nth(1)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok().filter(|port| *port != 0)
}

#[test]
fn watch_help_mentions_measure() {
    let out = fmd(&["watch", "--help"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    log_check(
        "j3e0.3.cli.help.measure",
        "--measure in watch help",
        out.status.success() && stdout.contains("--measure"),
    );
}

#[test]
fn watch_measure_zero_is_usage_error() {
    let dir = fresh_dir("measure-zero");
    let md = dir.join("doc.md");
    let html = dir.join("doc.html");
    std::fs::write(&md, "# x\n").unwrap();
    let out = fmd(&[
        "watch",
        md.to_str().unwrap(),
        "--out",
        html.to_str().unwrap(),
        "--measure",
        "0",
        "--no-config",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    log_check(
        "j3e0.3.cli.measure.zero",
        "--measure 0 is a usage error",
        !out.status.success() && stderr.contains("--measure"),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn watch_measure_tiny_doc_emits_p95_under_budget() {
    let dir = fresh_dir("measure");
    let md = dir.join("doc.md");
    let html = dir.join("doc.html");
    std::fs::write(&md, "# MeasureMe\n\nhello\n").unwrap();
    let out = fmd(&[
        "watch",
        md.to_str().unwrap(),
        "--out",
        html.to_str().unwrap(),
        "--serve",
        "--measure",
        "5",
        "--interval",
        "1",
        "--no-config",
        "--json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    log_check(
        "j3e0.3.cli.measure.exit",
        "tiny-doc measure exits 0",
        out.status.success(),
    );
    log_check(
        "j3e0.3.cli.measure.samples",
        "five sample events",
        stderr.matches("\"event\":\"sample\"").count() == 5,
    );
    log_check(
        "j3e0.3.cli.measure.summary",
        "measure summary with pass verdict",
        stderr.contains("\"event\":\"measure\"") && stderr.contains("\"verdict\":\"pass\""),
    );
    log_check(
        "j3e0.3.cli.measure.stdout",
        "stdout stays empty",
        out.stdout.is_empty(),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn watch_measure_fifty_headings_p95_under_150ms() {
    let dir = fresh_dir("measure-50");
    let md = dir.join("doc.md");
    let html = dir.join("doc.html");
    let mut src = String::from("# Watch latency fixture\n\n");
    for i in 1..=50 {
        src.push_str(&format!(
            "# Heading {i}\n\nParagraph {i} with enough words to give the HTML renderer a real page of work to do on every rebuild.\n\n"
        ));
    }
    std::fs::write(&md, src).unwrap();
    let out = fmd(&[
        "watch",
        md.to_str().unwrap(),
        "--out",
        html.to_str().unwrap(),
        "--serve",
        "--measure",
        "21",
        "--interval",
        "1",
        "--no-config",
        "--json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    log_check(
        "j3e0.3.fifty.samples",
        "21 sample events",
        stderr.matches("\"event\":\"sample\"").count() == 21,
    );
    log_check(
        "j3e0.3.fifty.exit",
        "p95 <= 150ms (process exit 0)",
        out.status.success(),
    );
    log_check(
        "j3e0.3.fifty.verdict",
        "measure summary verdict pass",
        stderr.contains("\"verdict\":\"pass\""),
    );
    let _ = std::fs::remove_dir_all(&dir);
}
