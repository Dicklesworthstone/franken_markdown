//! Coverage-focused contract tests for `fmd` CLI paths that the main contract
//! suite (`tests/cli_contract.rs`) does not reach: the auto-image destination
//! filter zoo, HTML-side auto-asset byte limits, multi-`=` `--pdf-image` spec
//! resolution, derived output paths for stdin/`--text` PDF renders, and the
//! remaining `SOURCE_DATE_EPOCH` / JSON-escape edges.
//!
//! Same conventions as `tests/cli_contract.rs`: spawn the real binary via
//! `env!("CARGO_BIN_EXE_fmd")`, pin exact exit codes, keep stdout-is-data and
//! stderr-is-diagnostics separate, and use unique per-test temp dirs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn fmd_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fmd"));
    cmd.args(args);
    cmd.env_remove("SOURCE_DATE_EPOCH");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

fn fmd_in_dir(args: &[&str], cwd: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .current_dir(cwd)
        .env_remove("SOURCE_DATE_EPOCH")
        .output()
        .unwrap()
}

fn fmd_with_stdin_in_dir(args: &[&str], cwd: &std::path::Path, stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .current_dir(cwd)
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

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "fmd-cli-coverage-{}-{}-{}",
        std::process::id(),
        nanos,
        name
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
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

/// An HTML render whose auto-loaded local image exceeds `--max-pdf-image-bytes`
/// is an input error (66) with the HTML-specific asset label, and no output
/// file is written — the byte cap protects the HTML data-URI path exactly like
/// the PDF path.
#[test]
fn html_auto_loaded_asset_over_byte_limit_is_an_input_error_before_writing() {
    let dir = temp_dir("html-auto-limit");
    fs::write(dir.join("doc.md"), "# Doc\n\n![c](pic.png)\n").unwrap();
    fs::write(dir.join("pic.png"), tiny_rgb_png()).unwrap();
    let out_html = dir.join("out.html");

    let out = fmd_in_dir(
        &[
            "doc.md",
            "--to",
            "html",
            "--out",
            "out.html",
            "--max-pdf-image-bytes",
            "4",
            "--json",
        ],
        &dir,
    );

    assert_eq!(out.status.code(), Some(66), "stderr: {}", text(&out.stderr));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"input_error\""),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("auto HTML image asset pic.png"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("exceeds --max-pdf-image-bytes 4"),
        "stderr: {stderr}"
    );
    assert!(
        !out_html.exists(),
        "an oversized auto asset must fail before writing the HTML output"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The file-input auto-image loader embeds every safe relative PNG destination
/// (including inside headings, blockquotes, lists, tables, emphasis/strong/
/// strikethrough, and links, and query/fragment-suffixed paths) as a data URI,
/// while leaving remote URLs, protocol-relative and absolute paths, parent-dir
/// escapes, unsupported extensions, missing files, and directories untouched.
#[test]
fn auto_image_loading_embeds_safe_destinations_and_skips_unsafe_ones() {
    let dir = temp_dir("auto-image-zoo");
    fs::create_dir_all(dir.join("imgdir.png")).unwrap();
    fs::write(dir.join("pic.png"), tiny_rgb_png()).unwrap();
    let doc = "\
# Zoo ![h](pic.png)

![ok](./pic.png)

![q](pic.png?x=1#f)

![remote](https://example.com/r.png)

![protorel](//cdn.example.com/r.png)

![parent](../escape.png)

![abs](/abs/pic.png)

![jpg](photo.jpg)

![missing](missing.png)

![dirpng](imgdir.png)

![empty]()

> ![bq](pic.png)

- ![li](pic.png)

**![st](pic.png)** *![em](pic.png)* ~~![sk](pic.png)~~

[![lnk](pic.png)](https://example.com/)

| ![th](pic.png) |
| --- |
| ![td](pic.png) |
";
    fs::write(dir.join("doc.md"), doc).unwrap();

    let out = fmd_in_dir(&["doc.md", "--to", "html", "--out", "out.html"], &dir);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", text(&out.stderr));
    let html = fs::read_to_string(dir.join("out.html")).unwrap();

    // 11 embedded copies: heading, ./pic.png, query+fragment variant,
    // blockquote, list item, strong, emphasis, strikethrough, linked image,
    // table header cell, table body cell.
    let embedded = html.matches("data:image/png;base64,").count();
    assert_eq!(
        embedded, 11,
        "expected 11 embedded data URIs, got {embedded}"
    );

    // Unsafe or unresolvable destinations stay as their original src.
    for raw in [
        "src=\"https://example.com/r.png\"",
        "src=\"//cdn.example.com/r.png\"",
        "src=\"../escape.png\"",
        "src=\"/abs/pic.png\"",
        "src=\"photo.jpg\"",
        "src=\"missing.png\"",
        "src=\"imgdir.png\"",
    ] {
        assert!(html.contains(raw), "expected untouched {raw} in output");
    }

    let _ = fs::remove_dir_all(&dir);
}

/// A `--pdf-image` spec whose PATH side itself contains `=` resolves to the
/// split whose path names an existing file when no Markdown destination in the
/// document matches any split. The unrelated document image stays unmapped and
/// surfaces as a human-readable warning line on stderr (not silence), while the
/// PDF still renders (exit 0).
#[test]
fn pdf_image_spec_with_multiple_equals_prefers_the_existing_file_path() {
    let dir = temp_dir("pdf-image-multi-eq");
    fs::write(dir.join("pic.png"), tiny_rgb_png()).unwrap();
    let spec = format!("alpha=beta={}", dir.join("pic.png").display());

    let out = fmd_in_dir(
        &[
            "--text",
            "# T\n\n![x](chart.png)",
            "--to",
            "pdf",
            "--out",
            "out.pdf",
            "--pdf-image",
            &spec,
        ],
        &dir,
    );

    assert_eq!(out.status.code(), Some(0), "stderr: {}", text(&out.stderr));
    assert!(out.stdout.is_empty());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("fmd: warning: image 'chart.png' has no --pdf-image mapping"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("fmd: wrote "), "stderr: {stderr}");
    let pdf = fs::read(dir.join("out.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"), "output must be a PDF");

    let _ = fs::remove_dir_all(&dir);
}

/// `fmd render` with no input argument and no `--text` reads Markdown from
/// stdin and streams the HTML document to stdout (exit 0).
#[test]
fn render_subcommand_with_no_input_argument_reads_stdin() {
    let dir = temp_dir("render-implicit-stdin");
    let out = fmd_with_stdin_in_dir(&["render"], &dir, "# Stdin Implicit\n");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", text(&out.stderr));
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.contains("<!DOCTYPE html>"));
    assert!(stdout.contains("<h1 id=\"stdin-implicit\">Stdin Implicit</h1>"));

    let _ = fs::remove_dir_all(&dir);
}

/// A stdin (`-`) PDF render without `--out` derives `document.pdf` in the
/// current directory — there is no input filename stem to reuse.
#[test]
fn stdin_dash_pdf_without_out_derives_document_pdf_in_cwd() {
    let dir = temp_dir("stdin-derived-pdf");
    let out = fmd_with_stdin_in_dir(&["-", "--to", "pdf"], &dir, "# T\n");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", text(&out.stderr));
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).contains("fmd: wrote document.pdf"));
    let pdf = fs::read(dir.join("document.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));

    let _ = fs::remove_dir_all(&dir);
}

/// A `--text` PDF render without `--out` also derives `document.pdf` in the
/// current directory (the `input.is_none()` side of the stem derivation).
#[test]
fn text_pdf_without_out_derives_document_pdf_in_cwd() {
    let dir = temp_dir("text-derived-pdf");
    let out = fmd_in_dir(&["--text", "# T", "--to", "pdf"], &dir);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", text(&out.stderr));
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).contains("fmd: wrote document.pdf"));
    let pdf = fs::read(dir.join("document.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));

    let _ = fs::remove_dir_all(&dir);
}

/// A whitespace-only or non-digit `SOURCE_DATE_EPOCH` is a usage error (64)
/// for PDF output, in both JSON and human diagnostics forms.
#[test]
fn whitespace_or_nondigit_source_date_epoch_is_a_usage_error() {
    let dir = temp_dir("sde-whitespace");
    let out_pdf = dir.join("out.pdf");
    let out_pdf_s = out_pdf.display().to_string();

    let blank = fmd_with_env(
        &[
            "--text", "# T", "--to", "pdf", "--out", &out_pdf_s, "--json",
        ],
        &[("SOURCE_DATE_EPOCH", "   ")],
    );
    assert_eq!(blank.status.code(), Some(64));
    assert!(blank.stdout.is_empty());
    let stderr = text(&blank.stderr);
    assert!(
        stderr.contains("\"code\":\"usage_error\""),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("SOURCE_DATE_EPOCH must be non-negative decimal seconds"),
        "stderr: {stderr}"
    );

    let nondigit = fmd_with_env(
        &["--text", "# T", "--to", "pdf", "--out", &out_pdf_s],
        &[("SOURCE_DATE_EPOCH", "12x")],
    );
    assert_eq!(nondigit.status.code(), Some(64));
    let stderr = text(&nondigit.stderr);
    assert!(
        stderr.contains("fmd: SOURCE_DATE_EPOCH must be non-negative decimal seconds"),
        "stderr: {stderr}"
    );
    assert!(!out_pdf.exists(), "no PDF may be written on a usage error");

    let _ = fs::remove_dir_all(&dir);
}

/// JSON error envelopes replace non-standard control characters (below U+0020,
/// other than \n \r \t which have their own escapes) with a space so the
/// envelope stays valid single-line JSON.
#[test]
fn json_error_envelope_replaces_control_characters_with_spaces() {
    let dir = temp_dir("json-ctrl-escape");
    let config = dir.join("absent.conf");
    let config_s = config.display().to_string();
    let weird_key = "bad\u{1}key";

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
    assert!(
        stderr.contains("unknown config key `bad key`"),
        "control char must be replaced by a space: {stderr}"
    );
    assert!(
        !stderr.contains('\u{1}'),
        "raw control byte must not appear in the JSON envelope"
    );

    let _ = fs::remove_dir_all(&dir);
}
