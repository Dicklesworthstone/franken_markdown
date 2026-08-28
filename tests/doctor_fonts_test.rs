//! y5i9.1: `fmd doctor fonts --corpus` process-boundary contract.
//!
//! Every assertion prints `check=… outcome=` on stderr.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn fmd(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .env_remove("SOURCE_DATE_EPOCH")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn log(id: &str, subject: &str, outcome: &str) {
    eprintln!("check={id} subject={subject} outcome={outcome}");
}

fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
    if ok {
        log(id, subject, "PASS");
    } else {
        log(id, subject, "FAIL");
        panic!("{id} `{subject}`: {detail}");
    }
}

fn tmp(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "fmd-doctor-fonts-cli-{}-{}-{name}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn doctor_health_json_still_works() {
    let out = fmd(&["doctor", "--json"]);
    let stdout = text(&out.stdout);
    assert_ok(
        "health-exit",
        "0",
        out.status.success(),
        &format!("status={:?}", out.status),
    );
    assert_ok(
        "health-json",
        "ok",
        stdout.contains("\"ok\":true") && stdout.contains("\"tool\":\"fmd\""),
        &stdout,
    );
    assert_ok(
        "health-not-fonts",
        "command",
        !stdout.contains("doctor fonts"),
        &stdout,
    );
}

#[test]
fn fonts_without_corpus_is_usage() {
    let out = fmd(&["doctor", "fonts"]);
    let code = out.status.code().unwrap_or(255);
    let stderr = text(&out.stderr);
    assert_ok(
        "usage-exit",
        "64-or-2",
        code == 64 || code == 2,
        &format!("code={code} stderr={stderr}"),
    );
}

#[test]
fn empty_corpus_is_covered() {
    let dir = tmp("empty");
    let out = fmd(&[
        "doctor",
        "fonts",
        "--corpus",
        dir.to_str().unwrap(),
        "--json",
    ]);
    let stdout = text(&out.stdout);
    assert_ok(
        "empty-exit",
        "0",
        out.status.success(),
        &format!("status={:?} stdout={stdout}", out.status),
    );
    assert_ok(
        "empty-gaps",
        "false",
        stdout.contains("\"files\":0") && stdout.contains("\"gaps\":false"),
        &stdout,
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn latin_corpus_is_covered() {
    let dir = tmp("latin");
    fs::write(dir.join("a.md"), "# Hello\n\nA paragraph.\n").unwrap();
    let out = fmd(&[
        "doctor",
        "fonts",
        "--corpus",
        dir.to_str().unwrap(),
        "--json",
    ]);
    let stdout = text(&out.stdout);
    assert_ok(
        "latin-exit",
        "0",
        out.status.success(),
        &format!("status={:?} stdout={stdout}", out.status),
    );
    assert_ok(
        "latin-gaps",
        "false",
        stdout.contains("\"gaps\":false"),
        &stdout,
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn han_fixture_reports_gaps_and_hints() {
    let dir = tmp("han");
    fs::write(dir.join("cjk.md"), "# 你好\n\n世界\n").unwrap();
    let out = fmd(&[
        "doctor",
        "fonts",
        "--corpus",
        dir.to_str().unwrap(),
        "--json",
    ]);
    let code = out.status.code().unwrap_or(255);
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert_ok(
        "han-exit",
        "1",
        code == 1,
        &format!("code={code} stdout={stdout} stderr={stderr}"),
    );
    assert_ok(
        "han-gaps",
        "true",
        stdout.contains("\"gaps\":true") && stdout.contains("\"script\":\"han\""),
        &stdout,
    );
    assert_ok(
        "han-hint",
        "CURATED_RANGES",
        stdout.contains("CURATED_RANGES"),
        &stdout,
    );
    assert_ok("han-sample", "cjk.md", stdout.contains("cjk.md:"), &stdout);
    let trimmed = stdout.trim_end();
    assert_ok(
        "han-json-stdout-pure",
        "stdout",
        trimmed.starts_with('{') && !trimmed.contains('\n'),
        &stdout,
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn walk_is_filesystem_order_independent() {
    let dir = tmp("order");
    fs::write(dir.join("b.md"), "b").unwrap();
    fs::write(dir.join("a.md"), "a").unwrap();
    let nested = dir.join("z");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("c.md"), "c").unwrap();
    let first = fmd(&[
        "doctor",
        "fonts",
        "--corpus",
        dir.to_str().unwrap(),
        "--json",
    ]);
    let second = fmd(&[
        "doctor",
        "fonts",
        "--corpus",
        dir.to_str().unwrap(),
        "--json",
    ]);
    let a = text(&first.stdout);
    let b = text(&second.stdout);
    assert_ok(
        "order-stable",
        "json",
        a == b && first.status == second.status,
        "two walks must match",
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_corpus_is_input_error() {
    let out = fmd(&[
        "doctor",
        "fonts",
        "--corpus",
        "/no/such/fmd/doctor-fonts-corpus",
        "--json",
    ]);
    let code = out.status.code().unwrap_or(255);
    let stderr = text(&out.stderr);
    assert_ok(
        "missing-exit",
        "66",
        code == 66,
        &format!("code={code} stderr={stderr}"),
    );
    assert_ok(
        "missing-json",
        "input_error",
        stderr.contains("\"code\":\"input_error\""),
        &stderr,
    );
}
