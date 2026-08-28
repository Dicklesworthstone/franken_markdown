//! Filesystem and CLI contract tests for `fmd watch` (j3e0.1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;
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
