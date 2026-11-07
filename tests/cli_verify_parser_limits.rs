//! The native verifier surfaces bounded parser degradation with source offsets.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn verify(source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "verify", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn parser_work_fallback_is_an_actionable_verification_finding() {
    let hostile = format!("{}leaf{}", "[".repeat(32), "](/target)".repeat(32));
    let source = format!("# Café\r\n\r\n{hostile}\r\n\r\nA final paragraph.\r\n");
    let diagnostic = franken_markdown::parse_markdown_spanned(&source)
        .diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.code == "inline_work_limit")
        .expect("fixture must reach actual parser resource admission");
    let output = verify(&source);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("\"code\":\"inline_work_limit\""),
        "{stdout}"
    );
    assert!(stdout.contains(&format!(
        "source bytes {}..{}",
        diagnostic.span.start, diagnostic.span.end
    )));
    assert!(stdout.contains("A final paragraph."));
    assert!(!stdout.contains("font_load_failed"));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ordinary_verification_keeps_its_existing_report_and_digest() {
    let source = "# A document\n\nAn ordinary paragraph with **emphasis**.\n";
    let output = verify(source);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report = franken_markdown::verify::verify_pdf(
        &franken_markdown::parse_markdown(source),
        &franken_markdown::PdfOptions::default(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        franken_markdown::verify::to_json(&report)
    );
}
