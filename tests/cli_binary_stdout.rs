//! Execute the real CLI at its binary-output boundary. A stdout export must be
//! exactly the same finished document as a file export, never a status envelope
//! or a partially rendered archive.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE: &str = "# Portable résumé\n\nA **document** with [a link](https://example.com) and a note.[^n]\n\n[^n]: Preserved footnote.\n";

fn scratch(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "fmd-binary-stdout-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn command(cwd: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
    command
        .current_dir(cwd)
        .arg("--no-config")
        .env("SOURCE_DATE_EPOCH", "1700000000")
        .env("NO_COLOR", "1");
    command
}

fn run(cwd: &Path, args: &[&str], input: Option<&str>) -> Output {
    let mut child = command(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(mut stdin) = child.stdin.take()
        && let Some(input) = input
    {
        stdin.write_all(input.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_binary(format: &str, bytes: &[u8]) {
    if format == "pdf" {
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    } else {
        assert!(bytes.starts_with(b"PK\x03\x04"));
        // The EPUB mimetype must be the first stored ZIP entry; the central
        // directory and its end record must survive publication without a newline.
        assert!(bytes.windows(20).any(|w| w == b"application/epub+zip"));
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
    }
}

#[test]
fn file_stdin_and_raw_text_export_the_exact_same_binary_to_stdout_and_disk() {
    for format in ["pdf", "epub"] {
        for source in ["file", "stdin", "text"] {
            let cwd = scratch(&format!("{format}-{source}"));
            fs::write(cwd.join("source.md"), SOURCE).unwrap();
            let (input_args, stdin) = match source {
                "file" => (vec!["source.md"], None),
                "stdin" => (vec!["-"], Some(SOURCE)),
                _ => (vec!["--text", SOURCE], None),
            };
            let destination = format!("export.{format}");
            let mut args = input_args.clone();
            args.extend(["--to", format, "--out", &destination]);
            let file = run(&cwd, &args, stdin);
            success(&file);
            assert!(file.stdout.is_empty());
            let expected = fs::read(cwd.join(&destination)).unwrap();
            assert_binary(format, &expected);

            let mut args = input_args;
            args.extend(["--to", format, "--out", "-", "--json"]);
            let stdout = run(&cwd, &args, stdin);
            success(&stdout);
            assert_eq!(stdout.stdout, expected, "{format} from {source}");
            assert!(!cwd.join("-").exists());
            assert!(!cwd.join(format!("document.{format}")).exists());
            assert!(!cwd.join(format!("source.{format}")).exists());
            assert!(!String::from_utf8_lossy(&stdout.stderr).contains("\"event\":\"wrote\""));
        }
    }
}

#[test]
fn binary_stdout_keeps_local_includes_images_and_pdfa_metadata() {
    let cwd = scratch("resources");
    fs::write(cwd.join("source.md"), "# Assets\n\n{{#include part.md}}\n").unwrap();
    fs::write(
        cwd.join("part.md"),
        "Included text.\n\n![Local figure](figure.svg)\n",
    )
    .unwrap();
    fs::write(
        cwd.join("figure.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#3b82f6"/></svg>"##,
    )
    .unwrap();
    for format in ["pdf", "epub"] {
        let destination = format!("asset-export.{format}");
        let mut common = vec!["source.md", "--to", format];
        if format == "pdf" {
            common.extend([
                "--pdf-a",
                "2b",
                "--title",
                "Archival source",
                "--author",
                "Example",
            ]);
        }
        let mut file_args = common.clone();
        file_args.extend(["--out", &destination]);
        let file = run(&cwd, &file_args, None);
        success(&file);
        let mut stream_args = common;
        stream_args.extend(["--out", "-", "--json"]);
        let stdout = run(&cwd, &stream_args, None);
        success(&stdout);
        assert_eq!(stdout.stdout, fs::read(cwd.join(&destination)).unwrap());
        if format == "pdf" {
            assert!(String::from_utf8_lossy(&stdout.stdout).contains("pdfaid:part"));
            assert!(String::from_utf8_lossy(&stdout.stderr).contains("\"event\":\"pdf_a\""));
        }
    }
}

#[test]
fn pdf_warnings_stay_on_stderr_without_contaminating_the_binary() {
    let cwd = scratch("warnings");
    let output = run(
        &cwd,
        &[
            "--text",
            "# Missing image\n\n![Description](missing.png)",
            "--to",
            "pdf",
            "--out",
            "-",
            "--json",
        ],
        None,
    );
    success(&output);
    assert_binary("pdf", &output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"warning\":\"unresolved_image\""),
        "{stderr}"
    );
    assert!(
        stderr
            .lines()
            .all(|line| line.starts_with('{') && line.ends_with('}'))
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("\"event\":\"warning\""));
}

#[test]
fn incompatible_multiple_documents_are_refused_before_reading_input() {
    let cwd = scratch("multiple");
    for target in ["both", "pdf", "epub", "svg"] {
        let mut args = vec!["missing.md", "--to", target, "--out", "-", "--json"];
        if target != "both" {
            args.push("--interactive-html");
        }
        let output = run(&cwd, &args, None);
        assert_eq!(output.status.code(), Some(64));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("writes one document"));
    }
    assert_eq!(fs::read_dir(&cwd).unwrap().count(), 0);
}

#[test]
fn render_and_sidecar_failures_leave_binary_stdout_empty() {
    let cwd = scratch("failed-publication");
    let output = run(
        &cwd,
        &[
            "--text",
            SOURCE,
            "--to",
            "pdf",
            "--out",
            "-",
            "--pdf-split-footnotes",
            "--pdf-optimal-pagination",
            "--json",
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(70));
    assert!(output.stdout.is_empty());

    fs::write(cwd.join("blocked"), "not a directory").unwrap();
    for format in ["pdf", "epub"] {
        let output = run(
            &cwd,
            &[
                "--text",
                SOURCE,
                "--to",
                format,
                "--out",
                "-",
                "--search-index",
                "blocked/index.json",
                "--json",
            ],
            None,
        );
        assert_eq!(
            output.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert_eq!(
            fs::read_to_string(cwd.join("blocked")).unwrap(),
            "not a directory"
        );
    }
}

#[test]
fn search_index_can_accompany_binary_stdout_without_becoming_document_bytes() {
    let cwd = scratch("sidecar");
    for format in ["pdf", "epub"] {
        let output = run(
            &cwd,
            &[
                "--text",
                SOURCE,
                "--to",
                format,
                "--out",
                "-",
                "--search-index",
                "index.json",
                "--json",
            ],
            None,
        );
        success(&output);
        assert_binary(format, &output.stdout);
        assert!(
            fs::read_to_string(cwd.join("index.json"))
                .unwrap()
                .contains("fmd-search-index-v1")
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("\"format\":\"search-index\""));
    }
}

#[cfg(unix)]
#[test]
fn early_pipe_closure_is_a_clean_exit_for_binary_formats() {
    let cwd = scratch("closed-consumer");
    for format in ["pdf", "epub"] {
        let mut child = command(&cwd)
            .args(["--text", SOURCE, "--to", format, "--out", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let output = child.wait_with_output().unwrap();
        success(&output);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn stdout_write_failures_return_74_and_report_json_on_stderr() {
    let cwd = scratch("failed-consumer");
    for format in ["pdf", "epub"] {
        let output = command(&cwd)
            .args(["--text", SOURCE, "--to", format, "--out", "-", "--json"])
            .stdout(Stdio::from(
                fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            ))
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(74));
        assert!(String::from_utf8_lossy(&output.stderr).contains("\"code\":\"output_error\""));
        assert!(output.stdout.is_empty());
    }
}
