//! Native page budgets apply to actual PDFs before any output is published.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use franken_markdown::{PdfOptions, render_pdf};

const SMALL_PAGE: &[&str] = &[
    "--page-size",
    "144x144",
    "--margin-top-pt",
    "36",
    "--margin-right-pt",
    "36",
    "--margin-bottom-pt",
    "36",
    "--margin-left-pt",
    "36",
];

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-page-budget-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.0.join("config"))
            .env_remove("SOURCE_DATE_EPOCH");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status={:?}; stderr={}; stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

fn refused(output: &Output, code: u8, reason: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(i32::from(code)), "{stderr}");
    assert!(stderr.contains(reason), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(!stderr.contains("\"event\":\"wrote\""), "{stderr}");
}

fn long_source() -> String {
    (0..40).map(|row| format!("Row {row}  \n")).collect()
}

fn small_options() -> PdfOptions {
    let mut options = PdfOptions {
        fit_to_pages: Some(1),
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 144.0;
    options.theme.page.size.height_pt = 144.0;
    options.theme.page.margins = franken_markdown::theme::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    options
}

// Count physical leaf objects through xref offsets, independently of the
// production budget helper's catalog /Pages /Count. Never scan font streams.
fn pages(pdf: &[u8]) -> usize {
    let marker = b"startxref\n";
    let at = pdf
        .windows(marker.len())
        .rposition(|s| s == marker)
        .unwrap();
    let xref: usize = std::str::from_utf8(&pdf[at + marker.len()..])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[xref..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    lines
        .take(count)
        .filter(|line| line.ends_with("n "))
        .filter(|line| {
            let offset: usize = line.split_whitespace().next().unwrap().parse().unwrap();
            let object = &pdf[offset..];
            let header_end = object.iter().position(|byte| *byte == b'\n').unwrap() + 1;
            object[header_end..].starts_with(b"<< /Type /Page ")
        })
        .count()
}

#[test]
fn publication_within_budget_preserves_library_bytes() {
    let workspace = Workspace::new();
    let source = "# Budget\n\nReady.\n";
    let output = workspace.run(&[
        "--text",
        source,
        "--to",
        "pdf",
        "--out",
        "-",
        "--target-pages",
        "1",
        "--json",
    ]);
    succeeded(&output);
    assert_eq!(pages(&output.stdout), 1);
    let options = PdfOptions {
        fit_to_pages: Some(1),
        ..PdfOptions::default()
    };
    assert_eq!(output.stdout, render_pdf(source, &options).unwrap());
}

#[test]
fn unmet_budget_preserves_combined_outputs_and_search_sidecar() {
    let workspace = Workspace::new();
    let source = long_source();
    // The Rust API keeps its existing adaptive behavior. Native publication
    // must reject this complete but over-budget result before touching files.
    let adaptive = render_pdf(&source, &small_options()).unwrap();
    let actual_pages = pages(&adaptive);
    assert!(actual_pages > 1);
    let ordinary = workspace
        .command()
        .args(["--text", &source, "--to", "pdf", "--out", "-"])
        .args(SMALL_PAGE)
        .output()
        .unwrap();
    succeeded(&ordinary);
    let mut unlimited = small_options();
    unlimited.fit_to_pages = None;
    assert_eq!(ordinary.stdout, render_pdf(&source, &unlimited).unwrap());
    for (path, bytes) in [
        ("document.html", b"prior HTML".as_slice()),
        ("document.pdf", b"prior PDF".as_slice()),
        ("search.json", b"prior index".as_slice()),
    ] {
        fs::write(workspace.0.join(path), bytes).unwrap();
    }
    let output = workspace
        .command()
        .args([
            "--text",
            &source,
            "--to",
            "both",
            "--out",
            "document.html",
            "--search-index",
            "search.json",
            "--fit-to-pages",
            "1",
            "--json",
        ])
        .args(SMALL_PAGE)
        .output()
        .unwrap();
    refused(&output, 70, "\"code\":\"page_budget_unmet\"");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(&format!("needs {actual_pages} pages"))
    );
    assert_eq!(
        fs::read(workspace.0.join("document.html")).unwrap(),
        b"prior HTML"
    );
    assert_eq!(
        fs::read(workspace.0.join("document.pdf")).unwrap(),
        b"prior PDF"
    );
    assert_eq!(
        fs::read(workspace.0.join("search.json")).unwrap(),
        b"prior index"
    );
}

#[test]
fn unmet_budget_never_leaks_pdf_or_archival_bytes_to_stdout_or_new_files() {
    let workspace = Workspace::new();
    let source = long_source();
    for archival in [false, true] {
        for destination in ["-", "new.pdf"] {
            let mut command = workspace.command();
            command
                .args([
                    "--text",
                    &source,
                    "--to",
                    "pdf",
                    "--out",
                    destination,
                    "--fit-to-pages",
                    "1",
                    "--json",
                ])
                .args(SMALL_PAGE);
            if archival {
                command.args(["--pdf-a", "2b"]);
            }
            refused(&command.output().unwrap(), 70, "page_budget_unmet");
            assert!(!workspace.0.join("new.pdf").exists());
        }
    }
}

#[test]
fn invalid_native_budgets_fail_before_reading_input_or_starting_watch() {
    let workspace = Workspace::new();
    for subcommand in ["render", "watch"] {
        let zero = workspace.run(&[
            subcommand,
            "missing.md",
            "--to",
            "pdf",
            "--out",
            "new.pdf",
            "--fit-to-pages",
            "0",
            "--json",
        ]);
        refused(&zero, 64, "--fit-to-pages must be at least 1");
        for target in ["html", "svg", "epub", "interactive-html"] {
            let output = workspace.run(&[
                subcommand,
                "missing.md",
                "--to",
                target,
                "--out",
                "new.out",
                "--fit-to-pages",
                "1",
                "--json",
            ]);
            refused(&output, 64, "unsupported_target_option");
        }
    }
    assert!(!workspace.0.join("new.pdf").exists());
    assert!(!workspace.0.join("new.out").exists());
}

struct WatchChild(Child);

impl Drop for WatchChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_watch(child: &mut WatchChild, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if ready() {
            return;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "watch exited unexpectedly"
        );
        assert!(
            Instant::now() < deadline,
            "watch did not reach the expected state"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn watch_keeps_both_last_good_outputs_then_recovers_after_budget_failure() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("document.md"), "Original").unwrap();
    let log_path = workspace.0.join("watch.log");
    let stdout_path = workspace.0.join("watch.stdout");
    let mut child = WatchChild(
        workspace
            .command()
            .args([
                "watch",
                "document.md",
                "--to",
                "both",
                "--out",
                "document.html",
                "--fit-to-pages",
                "1",
                "--interval",
                "25",
                "--json",
            ])
            .args(SMALL_PAGE)
            .stdout(Stdio::from(fs::File::create(&stdout_path).unwrap()))
            .stderr(Stdio::from(fs::File::create(&log_path).unwrap()))
            .spawn()
            .unwrap(),
    );
    let pdf_path = workspace.0.join("document.pdf");
    let html_path = workspace.0.join("document.html");
    let original_pdf = render_pdf("Original", &small_options()).unwrap();
    wait_for_watch(&mut child, || {
        fs::read(&pdf_path).is_ok_and(|bytes| bytes == original_pdf)
    });
    let original_html = fs::read(&html_path).unwrap();
    fs::write(workspace.0.join("document.md"), long_source()).unwrap();
    wait_for_watch(&mut child, || {
        fs::read_to_string(&log_path).is_ok_and(|log| log.contains("page_budget_unmet"))
    });
    assert_eq!(fs::read(&pdf_path).unwrap(), original_pdf);
    assert_eq!(fs::read(&html_path).unwrap(), original_html);
    fs::write(workspace.0.join("document.md"), "Recovered").unwrap();
    let recovered = render_pdf("Recovered", &small_options()).unwrap();
    wait_for_watch(&mut child, || {
        fs::read(&pdf_path).is_ok_and(|bytes| bytes == recovered)
    });
    assert!(fs::read_to_string(html_path).unwrap().contains("Recovered"));
    assert!(fs::read(stdout_path).unwrap().is_empty());
}

#[cfg(feature = "batch")]
#[test]
fn batch_reports_budget_failure_per_job_and_publishes_other_fitting_inputs() {
    let workspace = Workspace::new();
    fs::create_dir(workspace.0.join("inputs")).unwrap();
    fs::create_dir(workspace.0.join("output")).unwrap();
    fs::write(workspace.0.join("inputs/long.md"), long_source()).unwrap();
    fs::write(workspace.0.join("inputs/short.md"), "Fits").unwrap();
    fs::write(workspace.0.join("output/long.html"), "prior HTML").unwrap();
    fs::write(workspace.0.join("output/long.pdf"), "prior PDF").unwrap();
    let output = workspace
        .command()
        .args([
            "batch",
            "inputs",
            "--to",
            "both",
            "--out-dir",
            "output",
            "--workers",
            "1",
            "--continue-on-error",
            "--fit-to-pages",
            "1",
            "--json",
        ])
        .args(SMALL_PAGE)
        .output()
        .unwrap();
    succeeded(&output);
    let receipt = String::from_utf8_lossy(&output.stdout);
    assert!(
        receipt.contains("\"ok\":1,\"failed\":1,\"skipped\":0"),
        "{receipt}"
    );
    assert!(receipt.contains("page_budget_unmet"), "{receipt}");
    assert!(receipt.contains("\"error_kind\":\"render\""), "{receipt}");
    assert_eq!(
        fs::read(workspace.0.join("output/long.html")).unwrap(),
        b"prior HTML"
    );
    assert_eq!(
        fs::read(workspace.0.join("output/long.pdf")).unwrap(),
        b"prior PDF"
    );
    assert_eq!(
        pages(&fs::read(workspace.0.join("output/short.pdf")).unwrap()),
        1
    );
    assert!(
        fs::read_to_string(workspace.0.join("output/short.html"))
            .unwrap()
            .contains("Fits")
    );
}
