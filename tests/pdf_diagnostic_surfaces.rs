//! Publication adapters must report findings from the PDF they actually emit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::wasm::{WasmRenderOptions, render_pdf};

const SOURCE: &str = concat!(
    "Citation[^n]. [Read the note](#fn-n).\n\n",
    "[^n]: ![Visible alternative](published-missing.png)\n\n",
    "[^n]: ![Shadowed alternative](shadowed-missing.png)\n",
);

#[test]
fn browser_pdf_diagnostics_follow_published_note_ownership() {
    let output = render_pdf(SOURCE, &WasmRenderOptions::default()).unwrap();
    assert!(output.bytes.starts_with(b"%PDF-"));
    let resource_warnings: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("has no --pdf-image mapping"))
        .collect();
    assert_eq!(resource_warnings.len(), 1, "{:?}", output.diagnostics);
    let warning = resource_warnings[0];
    assert!(warning.message.contains("published-missing.png"));
    assert!(!warning.message.contains("shadowed-missing.png"));
    assert_eq!(warning.severity, "warning");
    assert_eq!((warning.start, warning.end), (0, 0));
    assert!(
        warning.code.is_none(),
        "retain the browser diagnostic schema"
    );
}

#[cfg(feature = "cli")]
#[test]
fn native_pdf_and_archival_stdout_report_only_published_resources() {
    for archival in [false, true] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_fmd"));
        command.env("SOURCE_DATE_EPOCH", "1700000000").args([
            "--no-config",
            "--text",
            SOURCE,
            "--to",
            "pdf",
            "--out",
            "-",
            "--no-remote-images",
            "--json",
        ]);
        if archival {
            command.args(["--pdf-a", "2b"]);
        }
        let output = command.output().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{stderr}");
        assert!(output.stdout.starts_with(b"%PDF-"));
        assert!(output.stdout.ends_with(b"%%EOF\n"));
        let warnings: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains("\"warning\":\"unresolved_image\""))
            .collect();
        assert_eq!(warnings.len(), 1, "{stderr}");
        assert!(warnings[0].contains("published-missing.png"));
        assert!(!stderr.contains("shadowed-missing.png"), "{stderr}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("\"event\":\"warning\""));
    }
}
