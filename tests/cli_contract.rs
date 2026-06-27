//! Contract tests for the `fmd` command-line surface.
//!
//! These deliberately execute the real binary instead of calling `cli::main`
//! directly: agent ergonomics regressions usually show up at the process
//! boundary (stdout/stderr, exit codes, argument normalization, help text).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn fmd(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .output()
        .unwrap()
}

fn fmd_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fmd"));
    cmd.args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

fn fmd_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn temp_file(name: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "fmd-cli-contract-{}-{}-{}.{}",
        std::process::id(),
        nanos,
        name,
        ext
    ))
}

#[test]
fn bare_invocation_prints_help_and_exits_successfully() {
    let out = fmd(&[]);

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.contains("First tries that work:"));
    assert!(stdout.contains("fmd README.md"));
    assert!(stdout.contains("fmd capabilities --json"));
    assert!(stdout.contains("fmd robot-docs guide"));
}

#[test]
fn discovery_surfaces_are_json_data_on_stdout() {
    let capabilities = fmd(&["capabilities", "--json"]);
    assert!(capabilities.status.success());
    assert!(capabilities.stderr.is_empty());
    let stdout = text(&capabilities.stdout);
    assert!(stdout.contains("\"tool\":\"fmd\""));
    assert!(stdout.contains("\"contract_version\":\"0.1.0\""));
    assert!(stdout.contains("\"64\":\"usage error\""));
    assert!(stdout.contains("\"robot_triage\":\"available\""));

    let doctor = fmd(&["doctor", "--json"]);
    assert!(doctor.status.success());
    assert!(doctor.stderr.is_empty());
    let stdout = text(&doctor.stdout);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"html\":\"available\""));
    assert!(stdout.contains("\"license\":\"LicenseRef-MIT-OpenAI-Anthropic-Rider\""));

    let triage = fmd(&["--robot-triage"]);
    assert!(triage.status.success());
    assert!(triage.stderr.is_empty());
    let stdout = text(&triage.stdout);
    assert!(stdout.contains("\"quick_ref\""));
    assert!(stdout.contains("\"recommended_next_actions\""));
    assert!(stdout.contains("fmd README.md --out README.html"));
}

#[test]
fn render_text_status_keeps_stdout_data_and_stderr_diagnostics_split() {
    let out_path = temp_file("render-text", "html");
    let out_path_s = out_path.display().to_string();
    let out = fmd(&[
        "--text",
        "# Hello\n\nA **strong** paragraph.",
        "--out",
        &out_path_s,
        "--json",
    ]);

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"ok\":true"));
    assert!(stderr.contains("\"event\":\"wrote\""));
    assert!(stderr.contains("\"format\":\"html\""));

    let html = fs::read_to_string(&out_path).unwrap();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<h1 id=\"hello\">Hello</h1>"));
    assert!(html.contains("<strong>strong</strong>"));

    let _ = fs::remove_file(out_path);
}

#[test]
fn first_try_file_and_stdin_renders_work() {
    let input_path = temp_file("input", "md");
    let file_out_path = temp_file("file-render", "html");
    fs::write(&input_path, "# File\n\nRendered from disk.").unwrap();

    let input_path_s = input_path.display().to_string();
    let file_out_path_s = file_out_path.display().to_string();
    let file_out = fmd(&[&input_path_s, "--out", &file_out_path_s]);

    assert!(file_out.status.success());
    assert!(file_out.stdout.is_empty());
    assert!(text(&file_out.stderr).contains("fmd: wrote"));
    let file_html = fs::read_to_string(&file_out_path).unwrap();
    assert!(file_html.contains("<h1 id=\"file\">File</h1>"));

    let stdin_out_path = temp_file("stdin-render", "html");
    let stdin_out_path_s = stdin_out_path.display().to_string();
    let stdin_out = fmd_with_stdin(&["-", "--out", &stdin_out_path_s], "# Stdin");

    assert!(stdin_out.status.success());
    assert!(stdin_out.stdout.is_empty());
    assert!(text(&stdin_out.stderr).contains("fmd: wrote"));
    let stdin_html = fs::read_to_string(&stdin_out_path).unwrap();
    assert!(stdin_html.contains("<h1 id=\"stdin\">Stdin</h1>"));

    let _ = fs::remove_file(input_path);
    let _ = fs::remove_file(file_out_path);
    let _ = fs::remove_file(stdin_out_path);
}

#[test]
fn pdf_render_writes_valid_mvp_pdf_and_json_status_to_stderr() {
    let out_path = temp_file("mvp-pdf", "pdf");
    let out_path_s = out_path.display().to_string();
    let out = fmd(&[
        "--text",
        "# Hello",
        "--to",
        "pdf",
        "--out",
        &out_path_s,
        "--json",
    ]);

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"ok\":true"));
    assert!(stderr.contains("\"event\":\"wrote\""));
    assert!(stderr.contains("\"format\":\"pdf\""));

    let pdf = fs::read(&out_path).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.7\n"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    assert!(pdf.windows(b"startxref".len()).any(|w| w == b"startxref"));
    assert!(
        pdf.windows(b"/Type /Catalog".len())
            .any(|w| w == b"/Type /Catalog")
    );
    assert!(pdf.len() > 500);

    let _ = fs::remove_file(out_path);
}

#[test]
fn parse_errors_use_documented_exit_code_and_teaching_hint() {
    let out = fmd(&["--definitely-not-a-real-flag"]);

    assert_eq!(out.status.code(), Some(64));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("unexpected argument"));
    assert!(stderr.contains("fmd: try `fmd --help`"));
    assert!(stderr.contains("fmd capabilities --json"));
}

#[test]
fn common_json_typos_are_inferred_before_parsing() {
    let out_path = temp_file("json-typo", "html");
    let out_path_s = out_path.display().to_string();
    let out = fmd(&["--jason", "--text", "# Typo", "--out", &out_path_s]);

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"ok\":true"));
    assert!(stderr.contains("\"event\":\"wrote\""));

    let html = fs::read_to_string(&out_path).unwrap();
    assert!(html.contains("<h1 id=\"typo\">Typo</h1>"));

    let _ = fs::remove_file(out_path);
}

#[test]
fn help_honors_no_color_ci_and_dumb_terminal_expectations() {
    let out = fmd_with_env(
        &["--help"],
        &[("NO_COLOR", "1"), ("CI", "true"), ("TERM", "dumb")],
    );

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.contains("Usage: fmd"));
    assert!(!stdout.contains('\u{1b}'));
}
