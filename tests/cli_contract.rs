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

fn fmd_in_dir(args: &[&str], cwd: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .current_dir(cwd)
        .env_remove("SOURCE_DATE_EPOCH")
        .output()
        .unwrap()
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

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "fmd-cli-contract-{}-{}-{}",
        std::process::id(),
        nanos,
        name
    ))
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn tiny_rgb_png() -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);

    let rows = [
        0, // filter type 0
        0xE8, 0x44, 0x44, 0x24, 0x91, 0xB8,
    ];
    let idat = franken_markdown::compress::zlib_compress(&rows);

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

#[test]
fn bare_invocation_prints_help_and_exits_successfully() {
    let out = fmd(&[]);

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.contains("First tries that work:"));
    assert!(stdout.contains("fmd README.md"));
    assert!(stdout.contains("fmd --text '# Hello' --out - > hello.html"));
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
    // The advertised `version` must track the package version (no manual drift).
    assert!(
        stdout.contains(&format!("\"version\":\"{}\"", env!("CARGO_PKG_VERSION"))),
        "capabilities `version` must equal CARGO_PKG_VERSION"
    );
    assert!(stdout.contains("\"contract_version\":\"0.1.0\""));
    assert!(stdout.contains("\"64\":\"usage error\""));
    assert!(stdout.contains("\"robot_triage\":\"available\""));
    assert!(stdout.contains("\"native_config\":\"available\""));
    assert!(stdout.contains("\"shared_theme_model\":\"structured_v1\""));
    assert!(stdout.contains("\"input_size_limit\":\"available\""));
    assert!(stdout.contains("\"html_stdout_dash\":\"available\""));
    assert!(stdout.contains("\"pdf_stdout_dash\":\"refused_usage_error\""));
    assert!(stdout.contains("\"pdf\":\"available_v0_embedded_subset_fonts\""));
    assert!(stdout.contains("\"font_subsetting_pdf\":\"available\""));
    assert!(stdout.contains("\"embedded_subset_fonts_pdf\":\"available\""));
    assert!(stdout.contains("\"gpos_kerning_pdf\":\"available_focused\""));
    assert!(stdout.contains("\"gsub_ligatures_pdf\":\"available_focused\""));
    assert!(stdout.contains("\"pdf_code_line_numbers\":\"available\""));
    assert!(stdout.contains("\"pdf_metadata\":\"available\""));
    assert!(stdout.contains("\"source_date_epoch_pdf\":\"available\""));
    assert!(stdout.contains("\"tagged_pdf\":\"available_hierarchical_accessible\""));
    assert!(stdout.contains("\"stream_compression_pdf\":\"available\""));
    assert!(stdout.contains("\"pdf_image_assets\":\"available_png_v0\""));
    assert!(stdout.contains("--pdf-line-numbers"));
    assert!(stdout.contains("--pdf-image"));
    assert!(stdout.contains("--author"));
    assert!(stdout.contains("fmd --text '# Hello' --out - > hello.html"));
    assert!(stdout.contains("\"knuth_plass_pdf\":\"available\""));
    assert!(stdout.contains("\"page_builder_pdf\":\"available_v0_keep_widow\""));
    assert!(stdout.contains("\"hyphenation_pdf\":\"available_discretionary_body_paragraphs\""));
    assert!(stdout.contains("\"pdf_justification\":\"available_body_paragraphs\""));
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
    assert!(stdout.contains("--pdf-image"));
    assert!(stdout.contains("--author"));
    assert!(stdout.contains("--max-input-bytes"));
    assert!(stdout.contains("fmd --text '# Hello' --out - > hello.html"));
    assert!(stdout.contains("`--out -` writes HTML document data to stdout only"));
    assert!(stdout.contains("PDF and --to both require a real output path"));
    assert!(stdout.contains("SOURCE_DATE_EPOCH"));
    assert!(stdout.contains("hierarchical accessible tagged-PDF structure tree"));
    assert!(stdout.contains("Knuth-Plass paragraph layout"));
    assert!(stdout.contains("deterministic discretionary hyphenation"));
    assert!(stdout.contains("glue justification for body paragraphs"));
    assert!(stdout.contains("basic keep/widow page building"));
    assert!(stdout.contains("deeper page-builder polish is still planned"));
    assert!(!stdout.contains(
        "Knuth-Plass paragraph layout, hyphenation, and page-builder polish are still planned"
    ));
    assert!(
        !stdout.contains("TeX/Liang hyphenation and deeper page-builder polish are still planned")
    );
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
fn html_out_dash_writes_document_data_to_stdout_without_dash_file() {
    let cwd = temp_dir("out-dash-html");
    fs::create_dir_all(&cwd).unwrap();

    let out = fmd_in_dir(&["--text", "# Stdout\n\nBody.", "--out", "-"], &cwd);

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.starts_with("<!DOCTYPE html>"));
    assert!(stdout.contains("<h1 id=\"stdout\">Stdout</h1>"));
    assert!(
        !cwd.join("-").exists(),
        "`--out -` for HTML stdout must not create a literal dash file"
    );
}

#[test]
fn pdf_out_dash_is_rejected_before_creating_literal_dash_file() {
    let cwd = temp_dir("out-dash-pdf");
    fs::create_dir_all(&cwd).unwrap();

    let out = fmd_in_dir(
        &["--text", "# PDF", "--to", "pdf", "--out", "-", "--json"],
        &cwd,
    );

    assert_eq!(out.status.code(), Some(64));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"code\":\"usage_error\""));
    assert!(stderr.contains("`--out -` writes HTML to stdout only"));
    assert!(
        !cwd.join("-").exists(),
        "PDF refusal must not create a literal dash file"
    );
}

#[test]
fn both_out_dash_is_rejected_before_creating_dash_derived_files() {
    let cwd = temp_dir("out-dash-both");
    fs::create_dir_all(&cwd).unwrap();

    let out = fmd_in_dir(
        &["--text", "# Both", "--to", "both", "--out", "-", "--json"],
        &cwd,
    );

    assert_eq!(out.status.code(), Some(64));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"code\":\"usage_error\""));
    assert!(stderr.contains("PDF and --to both require a real output path"));
    assert!(!cwd.join("-").exists());
    assert!(!cwd.join("-.html").exists());
    assert!(!cwd.join("-.pdf").exists());
}

#[test]
fn pdf_and_both_without_out_derive_paths_from_the_input_filename() {
    // mwm.8: PDF/both cannot stream to stdout, but omitting --out is NOT an
    // error — the output path is derived from the input stem (file input) or
    // `document.*` (stdin/--text). This pins the documented default behavior.
    let cwd = temp_dir("default-out-derive");
    fs::create_dir_all(&cwd).unwrap();
    fs::write(cwd.join("report.md"), "# Title\n\nbody\n").unwrap();

    // File input + --to pdf, no --out -> <stem>.pdf.
    let pdf = fmd_in_dir(&["report.md", "--to", "pdf", "--json"], &cwd);
    assert_eq!(
        pdf.status.code(),
        Some(0),
        "pdf without --out should succeed"
    );
    assert!(cwd.join("report.pdf").exists(), "should derive report.pdf");
    assert!(
        !cwd.join("document.pdf").exists(),
        "must not fall back to document.pdf for a file input"
    );
    let pdf_err = text(&pdf.stderr);
    assert!(
        pdf_err.contains("report.pdf"),
        "stderr should report the derived output path; got: {pdf_err}"
    );

    // File input + --to both, no --out -> sibling .html and .pdf.
    let both = fmd_in_dir(&["report.md", "--to", "both", "--json"], &cwd);
    assert_eq!(both.status.code(), Some(0));
    assert!(
        cwd.join("report.html").exists() && cwd.join("report.pdf").exists(),
        "both should derive sibling report.html and report.pdf"
    );

    // --text + --to pdf, no --out -> document.pdf (no input stem available).
    let text_pdf = fmd_in_dir(&["--text", "# Hi", "--to", "pdf", "--json"], &cwd);
    assert_eq!(text_pdf.status.code(), Some(0));
    assert!(
        cwd.join("document.pdf").exists(),
        "stdin/--text without --out should derive document.pdf"
    );
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
fn pdf_render_accepts_local_image_assets() {
    let image_path = temp_file("tiny-image", "png");
    let out_path = temp_file("image-pdf", "pdf");
    fs::write(&image_path, tiny_rgb_png()).unwrap();

    let mapping = format!("images/tiny.png={}", image_path.display());
    let out_path_s = out_path.display().to_string();
    let out = fmd(&[
        "--text",
        "![Tiny chart](images/tiny.png)",
        "--to",
        "pdf",
        "--pdf-image",
        &mapping,
        "--out",
        &out_path_s,
        "--json",
    ]);

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).contains("\"format\":\"pdf\""));

    let pdf = fs::read(&out_path).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("/ColorSpace /DeviceRGB"));
    assert!(pdf_text.contains("/XObject << /Im1 "));
    assert!(pdf_text.contains("/Im1 Do"));
    assert!(pdf_text.contains("/S /Figure"));
    assert!(pdf_text.contains("/Alt (Tiny chart)"));

    let _ = fs::remove_file(image_path);
    let _ = fs::remove_file(out_path);
}

#[test]
fn pdf_image_asset_mapping_allows_equals_in_markdown_destination() {
    let image_path = temp_file("query-image", "png");
    let equals_image_path = temp_file("query-image-with-equals", "png").with_file_name(format!(
        "fmd-cli-contract-{}-image=asset.png",
        std::process::id()
    ));
    let out_path = temp_file("query-image-pdf", "pdf");
    let equals_out_path = temp_file("equals-path-image-pdf", "pdf");
    fs::write(&image_path, tiny_rgb_png()).unwrap();
    fs::write(&equals_image_path, tiny_rgb_png()).unwrap();

    let destination = "https://cdn.example.test/chart.png?version=1";
    let mapping = format!("{destination}={}", image_path.display());
    let out_path_s = out_path.display().to_string();
    let markdown = format!("![Versioned chart]({destination})");
    let out = fmd(&[
        "--text",
        &markdown,
        "--to",
        "pdf",
        "--pdf-image",
        &mapping,
        "--out",
        &out_path_s,
        "--json",
    ]);

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let pdf = fs::read(&out_path).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("/Alt (Versioned chart)"));

    let equals_mapping = format!("images/local.png={}", equals_image_path.display());
    let equals_out_path_s = equals_out_path.display().to_string();
    let equals_out = fmd(&[
        "--text",
        "![Local asset](images/local.png)",
        "--to",
        "pdf",
        "--pdf-image",
        &equals_mapping,
        "--out",
        &equals_out_path_s,
        "--json",
    ]);

    assert!(equals_out.status.success());
    assert!(equals_out.stdout.is_empty());
    let pdf = fs::read(&equals_out_path).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(pdf_text.contains("/Subtype /Image"));
    assert!(pdf_text.contains("/Alt (Local asset)"));

    let _ = fs::remove_file(image_path);
    let _ = fs::remove_file(equals_image_path);
    let _ = fs::remove_file(out_path);
    let _ = fs::remove_file(equals_out_path);
}

#[test]
fn pdf_image_asset_errors_are_stable_and_prevent_partial_both_output() {
    let base_path = temp_file("bad-image-asset", "out");
    let base_path_s = base_path.display().to_string();
    let html_path = base_path.with_extension("html");
    let pdf_path = base_path.with_extension("pdf");
    let missing = temp_file("missing-image", "png");
    let mapping = format!("images/missing.png={}", missing.display());

    let out = fmd(&[
        "--text",
        "# With image\n\n![Missing](images/missing.png)",
        "--to",
        "both",
        "--pdf-image",
        &mapping,
        "--out",
        &base_path_s,
        "--json",
    ]);

    assert_eq!(out.status.code(), Some(66));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"code\":\"input_error\""));
    assert!(stderr.contains("reading PDF image asset images/missing.png"));
    assert!(
        !html_path.exists(),
        "PDF asset preflight should fail before writing HTML in --to both mode"
    );
    assert!(
        !pdf_path.exists(),
        "PDF asset preflight should not create a PDF"
    );

    let pdf_path_s = pdf_path.display().to_string();
    let bad_spec = fmd(&[
        "--text",
        "![Bad](bad.png)",
        "--to",
        "pdf",
        "--pdf-image",
        "bad.png",
        "--out",
        &pdf_path_s,
        "--json",
    ]);
    assert_eq!(bad_spec.status.code(), Some(66));
    let stderr = text(&bad_spec.stderr);
    assert!(stderr.contains("\"code\":\"input_error\""));
    assert!(stderr.contains("expected MARKDOWN_DEST=PATH"));

    let oversized_path = temp_file("oversized-image", "png");
    fs::write(&oversized_path, tiny_rgb_png()).unwrap();
    let oversized_mapping = format!("images/oversized.png={}", oversized_path.display());
    let oversized = fmd(&[
        "--text",
        "![Oversized](images/oversized.png)",
        "--to",
        "pdf",
        "--pdf-image",
        &oversized_mapping,
        "--max-pdf-image-bytes",
        "4",
        "--out",
        &pdf_path_s,
        "--json",
    ]);
    assert_eq!(oversized.status.code(), Some(66));
    let stderr = text(&oversized.stderr);
    assert!(stderr.contains("\"code\":\"input_error\""));
    assert!(stderr.contains("PDF image asset images/oversized.png"));
    assert!(stderr.contains("exceeds --max-pdf-image-bytes 4"));

    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(html_path);
    let _ = fs::remove_file(pdf_path);
    let _ = fs::remove_file(oversized_path);
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

// ---------------------------------------------------------------------------
// Render argument-validation, flag-normalization, and discovery branches that
// the first wave of contract tests did not reach. Everything below drives the
// real binary via real argv, real temp files, and per-child environments.
// ---------------------------------------------------------------------------

/// `--font` selects the body family for one render (covers the FontArg ->
/// FontFamily conversion and the `theme.with_font` overlay in run_render).
#[test]
fn font_flag_overrides_body_family_for_a_single_render() {
    let serif_path = temp_file("font-serif", "html");
    let serif_path_s = serif_path.display().to_string();
    let serif = fmd(&[
        "--text",
        "# Serif\n\nBody.",
        "--font",
        "serif",
        "--out",
        &serif_path_s,
    ]);
    assert!(serif.status.success());
    let serif_html = fs::read_to_string(&serif_path).unwrap();
    assert!(
        serif_html.contains("Source Serif 4"),
        "--font serif should select the serif family"
    );

    let sans_path = temp_file("font-sans", "html");
    let sans_path_s = sans_path.display().to_string();
    let sans = fmd(&[
        "--text",
        "# Sans\n\nBody.",
        "--font",
        "sans",
        "--out",
        &sans_path_s,
    ]);
    assert!(sans.status.success());
    let sans_html = fs::read_to_string(&sans_path).unwrap();
    assert!(
        sans_html.contains("Inter"),
        "--font sans should select the sans family"
    );
    assert!(!sans_html.contains("Source Serif 4"));

    let _ = fs::remove_file(serif_path);
    let _ = fs::remove_file(sans_path);
}

/// A `--css` stylesheet that cannot be read is an input error (exit 66), not a
/// silent fallback to the default theme.
#[test]
fn missing_custom_stylesheet_is_an_input_error() {
    let css = temp_file("missing-style", "css");
    let _ = fs::remove_file(&css);
    let css_s = css.display().to_string();
    let out_path = temp_file("css-fail", "html");
    let out_path_s = out_path.display().to_string();

    let out = fmd(&[
        "--text",
        "# X",
        "--css",
        &css_s,
        "--out",
        &out_path_s,
        "--json",
    ]);

    assert_eq!(out.status.code(), Some(66));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"input_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reading stylesheet"), "stderr: {stderr}");
    assert!(
        !out_path.exists(),
        "a stylesheet failure must not write an HTML file"
    );
}

/// Writing HTML to a path whose parent directory does not exist is an output
/// error (exit 73), reported on stderr without emitting document data.
#[test]
fn html_write_to_unwritable_path_is_an_output_error() {
    let dir = temp_dir("html-write-fail");
    let _ = fs::remove_dir_all(&dir);
    let target = dir.join("out.html");
    let target_s = target.display().to_string();

    let out = fmd(&["--text", "# X", "--out", &target_s, "--json"]);

    assert_eq!(out.status.code(), Some(73));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"output_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("writing"), "stderr: {stderr}");
    assert!(!target.exists());
}

/// The PDF writer hits the same output-error path (exit 73) when its
/// destination directory is missing.
#[test]
fn pdf_write_to_unwritable_path_is_an_output_error() {
    let dir = temp_dir("pdf-write-fail");
    let _ = fs::remove_dir_all(&dir);
    let target = dir.join("out.pdf");
    let target_s = target.display().to_string();

    let out = fmd(&["--text", "# X", "--to", "pdf", "--out", &target_s, "--json"]);

    assert_eq!(out.status.code(), Some(73));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"output_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("writing"), "stderr: {stderr}");
    assert!(!target.exists());
}

/// With no `--out` and the default HTML target, document data streams to stdout
/// (no file is created and stderr stays clean).
#[test]
fn html_without_out_streams_to_stdout() {
    let cwd = temp_dir("html-default-stdout");
    fs::create_dir_all(&cwd).unwrap();

    let out = fmd_in_dir(&["--text", "# Plain\n\nBody."], &cwd);

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.starts_with("<!DOCTYPE html>"));
    assert!(stdout.contains("<h1 id=\"plain\">Plain</h1>"));
    // No --out means nothing is written next to the working directory.
    assert!(fs::read_dir(&cwd).unwrap().next().is_none());

    let _ = fs::remove_dir_all(&cwd);
}

/// `--to both --out <path>` swaps the extension per format, deriving sibling
/// `.html` and `.pdf` files from the given output path.
#[test]
fn both_target_with_out_swaps_extension_per_format() {
    let dir = temp_dir("both-extension");
    fs::create_dir_all(&dir).unwrap();
    let base = dir.join("doc.out");
    let base_s = base.display().to_string();

    let out = fmd(&[
        "--text",
        "# Both\n\nBody.",
        "--to",
        "both",
        "--out",
        &base_s,
        "--json",
    ]);

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("\"format\":\"html\""), "stderr: {stderr}");
    assert!(stderr.contains("\"format\":\"pdf\""), "stderr: {stderr}");
    assert!(
        dir.join("doc.html").exists(),
        "both should derive doc.html from the --out stem"
    );
    assert!(
        dir.join("doc.pdf").exists(),
        "both should derive doc.pdf from the --out stem"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `doctor` prints a human report by default and a JSON envelope when the
/// global `--json` flag precedes the subcommand.
#[test]
fn doctor_prints_human_report_and_honors_global_json() {
    let human = fmd(&["doctor"]);
    assert!(human.status.success());
    assert!(human.stderr.is_empty());
    let stdout = text(&human.stdout);
    assert!(stdout.contains("fmd doctor"));
    assert!(stdout.contains("html: available"));
    assert!(stdout.contains("theme model: structured v1"));
    assert!(stdout.contains("cli dependency: clap"));
    assert!(stdout.contains("wasm posture: core builds with --no-default-features"));
    assert!(stdout.contains("license: MIT with OpenAI/Anthropic rider"));

    let global = fmd(&["--json", "doctor"]);
    assert!(global.status.success());
    let stdout = text(&global.stdout);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"license\":\"LicenseRef-MIT-OpenAI-Anthropic-Rider\""));
}

/// `robot-docs` with no explicit subcommand defaults to the guide.
#[test]
fn robot_docs_defaults_to_the_guide_subcommand() {
    let out = fmd(&["robot-docs"]);
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    assert!(text(&out.stdout).contains("fmd agent guide"));
}

/// Every remaining `--json` typo normalizes to `--json` before parsing, just
/// like the already-tested `--jason`.
#[test]
fn additional_json_typos_are_inferred_before_parsing() {
    for typo in ["--jsno", "--jsoon", "--json=true"] {
        let out_path = temp_file("json-typo-variant", "html");
        let out_path_s = out_path.display().to_string();
        let out = fmd(&[typo, "--text", "# Typo", "--out", &out_path_s]);
        assert!(out.status.success(), "{typo} should normalize to --json");
        assert!(out.stdout.is_empty());
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains("\"event\":\"wrote\""),
            "{typo} should emit JSON write status; got: {stderr}"
        );
        let _ = fs::remove_file(out_path);
    }
}

/// The colour-spelling and `--color=never` typos normalize to `--no-color`,
/// which is an accepted global flag (output is already plain).
#[test]
fn color_flag_typos_normalize_to_no_color() {
    for typo in ["--no-colour", "--colour=never", "--color=never"] {
        let out = fmd(&[typo, "capabilities", "--json"]);
        assert!(out.status.success(), "{typo} should be accepted");
        assert!(out.stderr.is_empty(), "{typo} should not warn");
        assert!(
            text(&out.stdout).contains("\"tool\":\"fmd\""),
            "{typo} should still run capabilities"
        );
    }
}

/// An input error emitted without `--json` uses the plain `fmd: <msg>` human
/// form on stderr rather than a JSON envelope.
#[test]
fn input_error_without_json_uses_human_readable_stderr() {
    let out = fmd(&["--text", "123456789", "--max-input-bytes", "4"]);

    assert_eq!(out.status.code(), Some(66));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(stderr.starts_with("fmd: "), "stderr: {stderr}");
    assert!(
        stderr.contains("exceeds --max-input-bytes 4"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("\"code\""), "stderr: {stderr}");
}

/// A `--pdf-image` spec with a blank destination or a blank path is a distinct,
/// stable input error (exercises both blank-side branches of the spec parser).
#[test]
fn pdf_image_spec_rejects_blank_destination_and_blank_path() {
    let img = temp_file("blank-side-image", "png");
    fs::write(&img, tiny_rgb_png()).unwrap();
    let blank_dest_pdf = temp_file("blank-dest-pdf", "pdf");
    let blank_dest_pdf_s = blank_dest_pdf.display().to_string();
    let blank_dest_mapping = format!("={}", img.display());

    let blank_dest = fmd(&[
        "--text",
        "# X",
        "--to",
        "pdf",
        "--pdf-image",
        &blank_dest_mapping,
        "--out",
        &blank_dest_pdf_s,
        "--json",
    ]);
    assert_eq!(blank_dest.status.code(), Some(66));
    let stderr = text(&blank_dest.stderr);
    assert!(
        stderr.contains("\"code\":\"input_error\""),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("MARKDOWN_DEST must not be blank"),
        "stderr: {stderr}"
    );

    let blank_path_pdf = temp_file("blank-path-pdf", "pdf");
    let blank_path_pdf_s = blank_path_pdf.display().to_string();
    let blank_path = fmd(&[
        "--text",
        "# X",
        "--to",
        "pdf",
        "--pdf-image",
        "images/chart.png=",
        "--out",
        &blank_path_pdf_s,
        "--json",
    ]);
    assert_eq!(blank_path.status.code(), Some(66));
    let stderr = text(&blank_path.stderr);
    assert!(
        stderr.contains("\"code\":\"input_error\""),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("PATH must not be blank"),
        "stderr: {stderr}"
    );

    let _ = fs::remove_file(img);
    let _ = fs::remove_file(blank_dest_pdf);
    let _ = fs::remove_file(blank_path_pdf);
}

/// A digits-only but out-of-range `SOURCE_DATE_EPOCH` is a usage error (exit
/// 64) that fails before any PDF is written.
#[test]
fn source_date_epoch_overflow_is_a_usage_error() {
    let out_path = temp_file("sde-overflow-pdf", "pdf");
    let _ = fs::remove_file(&out_path);
    let out_path_s = out_path.display().to_string();
    let out = fmd_with_env(
        &[
            "--text",
            "# Dated",
            "--to",
            "pdf",
            "--out",
            &out_path_s,
            "--json",
        ],
        &[("SOURCE_DATE_EPOCH", "999999999999999999999999")],
    );

    assert_eq!(out.status.code(), Some(64));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"usage_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("too large"), "stderr: {stderr}");
    assert!(!out_path.exists());
}

// ---------------------------------------------------------------------------
// Config-subcommand human-readable output and resolution/error branches not
// reached by the JSON-only config contract suite. All use isolated FMD_CONFIG
// paths so the real user config is never read or written.
// ---------------------------------------------------------------------------

/// Render that loads a malformed native config reports a config error (exit 66)
/// instead of rendering with partial state.
#[test]
fn render_with_malformed_config_is_a_config_error() {
    let config = temp_file("render-bad-config", "conf");
    fs::write(&config, "this line has no equals sign\n").unwrap();
    let config_s = config.display().to_string();
    let out_path = temp_file("render-bad-config-out", "html");
    let out_path_s = out_path.display().to_string();

    let out = fmd_with_env(
        &["--text", "# X", "--out", &out_path_s, "--json"],
        &[("FMD_CONFIG", config_s.as_str())],
    );

    assert_eq!(out.status.code(), Some(66));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"config_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reading config"), "stderr: {stderr}");
    assert!(!out_path.exists());

    let _ = fs::remove_file(config);
}

/// `config show` (no `--json`) prints the resolved keys in human form.
#[test]
fn config_show_human_readable_lists_resolved_keys() {
    let config = temp_file("show-human", "conf");
    let _ = fs::remove_file(&config);
    let config_s = config.display().to_string();

    let out = fmd_with_env(&["config", "show"], &[("FMD_CONFIG", config_s.as_str())]);

    assert!(out.status.success());
    let stdout = text(&out.stdout);
    assert!(stdout.contains("fmd config"));
    assert!(stdout.contains(&format!("path: {config_s}")));
    assert!(stdout.contains("font: sans"));
    assert!(stdout.contains("dark_mode: auto"));
    assert!(stdout.contains("page_size: letter"));
}

/// `config get <key>` (no `--json`) prints just the resolved value.
#[test]
fn config_get_human_readable_prints_bare_value() {
    let config = temp_file("get-human", "conf");
    let _ = fs::remove_file(&config);
    let config_s = config.display().to_string();

    let out = fmd_with_env(
        &["config", "get", "font"],
        &[("FMD_CONFIG", config_s.as_str())],
    );

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    assert_eq!(text(&out.stdout).trim(), "sans");
}

/// `config path` (no `--json`) prints exactly the resolved config path.
#[test]
fn config_path_human_readable_prints_resolved_path() {
    let config = temp_file("path-human", "conf");
    let config_s = config.display().to_string();

    let out = fmd_with_env(&["config", "path"], &[("FMD_CONFIG", config_s.as_str())]);

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    assert_eq!(text(&out.stdout).trim(), config_s);
}

/// `config get` against a config path that is a directory surfaces a config
/// read error (exit 66), mirroring the `config show` directory case.
#[test]
fn config_get_reports_config_error_when_path_is_a_directory() {
    let dir = temp_dir("get-dir-config");
    fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.display().to_string();

    let out = fmd_with_env(
        &["config", "get", "font", "--json"],
        &[("FMD_CONFIG", dir_s.as_str())],
    );

    assert_eq!(out.status.code(), Some(66));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"config_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reading config"), "stderr: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

/// `config set` against a malformed existing config fails while loading it
/// (exit 66) and never rewrites the file.
#[test]
fn config_set_reports_config_error_when_existing_config_is_malformed() {
    let config = temp_file("set-bad-config", "conf");
    fs::write(&config, "garbage without an equals\n").unwrap();
    let config_s = config.display().to_string();

    let out = fmd_with_env(
        &["config", "set", "font", "serif", "--json"],
        &[("FMD_CONFIG", config_s.as_str())],
    );

    assert_eq!(out.status.code(), Some(66));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"config_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reading config"), "stderr: {stderr}");
    // The malformed file is left untouched.
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        "garbage without an equals\n"
    );

    let _ = fs::remove_file(config);
}

/// JSON envelopes escape control and quote characters: an unknown `config get`
/// key carrying every escaped class flows through the CLI's json_escape.
#[test]
fn json_envelopes_escape_control_and_quote_characters() {
    let config = temp_file("json-escape-config", "conf");
    let _ = fs::remove_file(&config);
    let config_s = config.display().to_string();
    let weird_key = "a\"b\\c\nd\re\tf";

    let out = fmd_with_env(
        &["config", "get", weird_key, "--json"],
        &[("FMD_CONFIG", config_s.as_str())],
    );

    assert_eq!(out.status.code(), Some(64));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"usage_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("a\\\"b"), "escaped quote missing: {stderr}");
    assert!(
        stderr.contains("b\\\\c"),
        "escaped backslash missing: {stderr}"
    );
    assert!(
        stderr.contains("c\\nd"),
        "escaped newline missing: {stderr}"
    );
    assert!(
        stderr.contains("d\\re"),
        "escaped carriage return missing: {stderr}"
    );
    assert!(stderr.contains("e\\tf"), "escaped tab missing: {stderr}");
}

// ---------------------------------------------------------------------------
// Unix-only environment edge cases. These need a child process environment that
// cannot be expressed as UTF-8 `&str`, or filesystem permissions that the
// crate's forbid-unsafe rules keep out of the in-process tests.
// ---------------------------------------------------------------------------

/// A non-UTF-8 `SOURCE_DATE_EPOCH` is a usage error (exit 64).
#[cfg(unix)]
#[test]
fn non_utf8_source_date_epoch_is_a_usage_error() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let out_path = temp_file("sde-non-utf8-pdf", "pdf");
    let _ = fs::remove_file(&out_path);
    let out_path_s = out_path.display().to_string();

    let out = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args([
            "--text",
            "# Dated",
            "--to",
            "pdf",
            "--out",
            &out_path_s,
            "--json",
        ])
        .env("SOURCE_DATE_EPOCH", OsStr::from_bytes(&[0x66, 0xff, 0xfe]))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(64));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"usage_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("must be UTF-8"), "stderr: {stderr}");
    assert!(!out_path.exists());
}

/// `config set` reports a config write error (exit 73) when the target config
/// directory is read-only. Skipped when DAC permissions are not enforced for
/// this process (e.g. running as root), so the assertion is never a false claim.
#[cfg(unix)]
#[test]
fn config_set_save_error_when_target_directory_is_read_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("ro-config-save");
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();

    let probe = dir.join(".probe");
    let perms_enforced = fs::write(&probe, b"x").is_err();

    if perms_enforced {
        let config = dir.join("config");
        let config_s = config.display().to_string();
        let out = fmd_with_env(
            &["config", "set", "font", "serif", "--json"],
            &[("FMD_CONFIG", config_s.as_str())],
        );
        assert_eq!(out.status.code(), Some(73));
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains("\"code\":\"config_error\""),
            "stderr: {stderr}"
        );
        assert!(stderr.contains("writing config"), "stderr: {stderr}");
        assert!(
            !config.exists(),
            "a failed save must not leave a config file"
        );
    } else {
        let _ = fs::remove_file(&probe);
    }

    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
    let _ = fs::remove_dir_all(&dir);
}
