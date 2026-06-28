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
        .env_remove("SOURCE_DATE_EPOCH")
        .output()
        .unwrap()
}

fn fmd_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fmd"));
    cmd.args(args);
    cmd.env_remove("SOURCE_DATE_EPOCH");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

fn fmd_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .env_remove("SOURCE_DATE_EPOCH")
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
    assert!(stdout.contains("fmd config show --json"));
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
    assert!(stdout.contains("\"native_config\":\"available\""));
    assert!(stdout.contains("\"shared_theme_model\":\"structured_v1\""));
    assert!(stdout.contains("\"input_size_limit\":\"available\""));
    assert!(stdout.contains("\"pdf\":\"available_v0_embedded_subset_fonts\""));
    assert!(stdout.contains("\"font_subsetting_pdf\":\"available\""));
    assert!(stdout.contains("\"embedded_subset_fonts_pdf\":\"available\""));
    assert!(stdout.contains("\"gpos_kerning_pdf\":\"available_focused\""));
    assert!(stdout.contains("\"gsub_ligatures_pdf\":\"available_focused\""));
    assert!(stdout.contains("\"pdf_code_line_numbers\":\"available\""));
    assert!(stdout.contains("\"pdf_metadata\":\"available\""));
    assert!(stdout.contains("\"source_date_epoch_pdf\":\"available\""));
    assert!(stdout.contains("\"tagged_pdf\":\"available_v0\""));
    assert!(stdout.contains("\"stream_compression_pdf\":\"available\""));
    assert!(stdout.contains("--pdf-line-numbers"));
    assert!(stdout.contains("--author"));
    assert!(stdout.contains("\"knuth_plass_pdf\":\"planned\""));
    assert!(stdout.contains("\"theme_model\":{\"status\":\"structured_v1\""));
    assert!(!stdout.contains("available_v0_base14"));

    let doctor = fmd(&["doctor", "--json"]);
    assert!(doctor.status.success());
    assert!(doctor.stderr.is_empty());
    let stdout = text(&doctor.stdout);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"html\":\"available\""));
    assert!(stdout.contains("\"pdf\":\"available_v0_embedded_subset_fonts\""));
    assert!(stdout.contains("\"theme_model\":{\"status\":\"structured_v1\""));
    assert!(stdout.contains("\"license\":\"LicenseRef-MIT-OpenAI-Anthropic-Rider\""));
    assert!(!stdout.contains("available_v0_base14"));

    let triage = fmd(&["--robot-triage"]);
    assert!(triage.status.success());
    assert!(triage.stderr.is_empty());
    let stdout = text(&triage.stdout);
    assert!(stdout.contains("\"quick_ref\""));
    assert!(stdout.contains("\"recommended_next_actions\""));
    assert!(stdout.contains("\"pdf\":\"available_v0_embedded_subset_fonts\""));
    assert!(stdout.contains("fmd README.md --out README.html"));
    assert!(!stdout.contains("available_v0_base14"));
}

#[test]
fn robot_docs_describe_current_pdf_capability_without_stale_base14_claims() {
    let out = fmd(&["robot-docs", "guide"]);

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.contains("embedded per-document font subsets"));
    assert!(stdout.contains("focused GPOS kerning"));
    assert!(stdout.contains("GSUB ligatures"));
    assert!(stdout.contains("--pdf-line-numbers"));
    assert!(stdout.contains("--author"));
    assert!(stdout.contains("--max-input-bytes"));
    assert!(stdout.contains("SOURCE_DATE_EPOCH"));
    assert!(stdout.contains("tagged-PDF structure tree v0"));
    assert!(stdout.contains("Knuth-Plass paragraph layout"));
    assert!(!stdout.contains("base-14"));
    assert!(!stdout.contains("Base-14"));
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
fn render_refuses_inputs_over_the_configured_byte_limit() {
    let raw = "123456789";
    let text_out = fmd(&[
        "--text",
        raw,
        "--max-input-bytes",
        "8",
        "--out",
        "ignored.html",
        "--json",
    ]);
    assert_eq!(text_out.status.code(), Some(66));
    assert!(text_out.stdout.is_empty());
    let stderr = text(&text_out.stderr);
    assert!(stderr.contains("\"code\":\"input_error\""));
    assert!(stderr.contains("--max-input-bytes 8"));
    assert!(!stderr.contains(raw));

    let input_path = temp_file("too-large-input", "md");
    fs::write(&input_path, raw).unwrap();
    let input_path_s = input_path.display().to_string();
    let file_out = fmd(&[&input_path_s, "--max-input-bytes", "8", "--json"]);
    assert_eq!(file_out.status.code(), Some(66));
    assert!(file_out.stdout.is_empty());
    let stderr = text(&file_out.stderr);
    assert!(stderr.contains("\"code\":\"input_error\""));
    assert!(stderr.contains("input file"));
    assert!(stderr.contains("--max-input-bytes 8"));
    assert!(!stderr.contains(raw));

    let stdin_out = fmd_with_stdin(&["-", "--max-input-bytes", "8", "--json"], raw);
    assert_eq!(stdin_out.status.code(), Some(66));
    assert!(stdin_out.stdout.is_empty());
    let stderr = text(&stdin_out.stderr);
    assert!(stderr.contains("\"code\":\"input_error\""));
    assert!(stderr.contains("stdin input"));
    assert!(stderr.contains("--max-input-bytes 8"));
    assert!(!stderr.contains(raw));

    let _ = fs::remove_file(input_path);
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
fn pdf_render_accepts_agent_discoverable_line_numbers_flag() {
    let out_path = temp_file("numbered-pdf", "pdf");
    let out_path_s = out_path.display().to_string();
    let out = fmd(&[
        "--text",
        "```text\nalpha\nbeta\n```",
        "--to",
        "pdf",
        "--pdf-line-numbers",
        "--title",
        "Numbered PDF",
        "--author",
        "FMD Tests",
        "--out",
        &out_path_s,
        "--json",
    ]);

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).contains("\"format\":\"pdf\""));

    let pdf = fs::read(&out_path).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(
        pdf_text.contains("0.431 0.467 0.506 rg"),
        "line numbers should render in muted syntax/comment color"
    );
    assert!(pdf_text.contains("/Title (Numbered PDF)"));
    assert!(pdf_text.contains("/Author (FMD Tests)"));

    let _ = fs::remove_file(out_path);
}

#[test]
fn pdf_render_honors_source_date_epoch_metadata() {
    let out_path = temp_file("source-date-pdf", "pdf");
    let out_path_s = out_path.display().to_string();
    let out = fmd_with_env(
        &[
            "--text",
            "# Dated\n\nBody.",
            "--to",
            "pdf",
            "--out",
            &out_path_s,
            "--json",
        ],
        &[("SOURCE_DATE_EPOCH", "1700000000")],
    );

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let pdf = fs::read(&out_path).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(pdf_text.contains("/CreationDate (D:20231114221320Z)"));
    assert!(pdf_text.contains("/ModDate (D:20231114221320Z)"));

    let _ = fs::remove_file(out_path);
}

#[test]
fn invalid_source_date_epoch_fails_before_partial_both_output() {
    let base_path = temp_file("bad-source-date", "out");
    let base_path_s = base_path.display().to_string();
    let html_path = base_path.with_extension("html");
    let pdf_path = base_path.with_extension("pdf");
    let out = fmd_with_env(
        &[
            "--text",
            "# Dated\n\nBody.",
            "--to",
            "both",
            "--out",
            &base_path_s,
            "--json",
        ],
        &[("SOURCE_DATE_EPOCH", "not-a-date")],
    );

    assert_eq!(out.status.code(), Some(64));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"code\":\"usage_error\""));
    assert!(stderr.contains("SOURCE_DATE_EPOCH"));
    assert!(stderr.contains("decimal seconds"));
    assert!(
        !html_path.exists(),
        "invalid PDF metadata env should fail before writing HTML in --to both mode"
    );
    assert!(
        !pdf_path.exists(),
        "invalid PDF metadata env should not create a PDF"
    );

    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(html_path);
    let _ = fs::remove_file(pdf_path);
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
