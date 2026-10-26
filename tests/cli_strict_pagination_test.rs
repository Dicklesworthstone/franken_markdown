//! Strict page planning reaches native publication and preserves prior outputs.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use franken_markdown::{PdfOptions, render_pdf};

const SOURCE: &str = "# Strict publication\n\nAn ordinary paragraph fits the measured page.\n";
const INFEASIBLE: &str = "A  \nB  \nC";
// Native geometry requires at least 72 pt of body height. At 24 pt type,
// three hard-break lines cannot fit or split into two fragments of >=2 lines.
const TIGHT: &[&str] = &[
    "--page-size",
    "letter",
    "--margin-top-pt",
    "360",
    "--margin-bottom-pt",
    "360",
    "--pdf-base-font-size",
    "24",
];

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-strict-pagination-{}-{}",
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

    fn render(&self, source: &str, path: &str, flags: &[&str]) -> Output {
        self.command()
            .args(["--text", source, "--to", "pdf", "--out", path, "--json"])
            .args(flags)
            .output()
            .unwrap()
    }

    fn tight_render(&self, path: &str, flags: &[&str]) -> Output {
        let flags: Vec<&str> = TIGHT.iter().copied().chain(flags.iter().copied()).collect();
        self.render(INFEASIBLE, path, &flags)
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

fn refused(output: &Output, code: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(70), "{stderr}");
    assert!(stderr.contains(&format!("\"code\":\"{code}\"")), "{stderr}");
    assert!(stderr.contains("strict PDF pagination failed:"), "{stderr}");
    assert!(output.stdout.is_empty());
}

fn strict_options() -> PdfOptions {
    PdfOptions {
        strict_pagination: true,
        ..PdfOptions::default()
    }
}

fn tight_options() -> PdfOptions {
    let mut options = strict_options();
    options.base_font_size = Some(24.0);
    options.theme.page.margins.top_pt = 360.0;
    options.theme.page.margins.bottom_pt = 360.0;
    options
}

#[test]
fn strict_flag_implies_optimal_and_does_not_consume_the_input_path() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("document.md"), SOURCE).unwrap();
    succeeded(&workspace.run(&[
        "--pdf-strict-pagination",
        "document.md",
        "--to",
        "pdf",
        "--out",
        "strict.pdf",
        "--json",
    ]));
    let expected = render_pdf(SOURCE, &strict_options()).unwrap();
    assert_eq!(fs::read(workspace.0.join("strict.pdf")).unwrap(), expected);

    succeeded(&workspace.render(SOURCE, "default.pdf", &[]));
    succeeded(&workspace.render(SOURCE, "disabled.pdf", &["--pdf-strict-pagination=false"]));
    let default = render_pdf(SOURCE, &PdfOptions::default()).unwrap();
    assert_eq!(fs::read(workspace.0.join("default.pdf")).unwrap(), default);
    assert_eq!(fs::read(workspace.0.join("disabled.pdf")).unwrap(), default);
}

#[test]
fn infeasible_strict_render_creates_no_pdf_and_preserves_existing_outputs() {
    let workspace = Workspace::new();
    // This is valid native geometry and the existing fallback preserves content.
    succeeded(&workspace.tight_render("fallback.pdf", &["--pdf-optimal-pagination"]));
    assert!(
        fs::read(workspace.0.join("fallback.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );

    refused(
        &workspace.tight_render("new.pdf", &["--pdf-strict-pagination"]),
        "pagination_infeasible",
    );
    assert!(!workspace.0.join("new.pdf").exists());
    fs::write(workspace.0.join("existing.pdf"), b"prior PDF").unwrap();
    refused(
        &workspace.tight_render("existing.pdf", &["--pdf-strict-pagination"]),
        "pagination_infeasible",
    );
    assert_eq!(
        fs::read(workspace.0.join("existing.pdf")).unwrap(),
        b"prior PDF"
    );

    // Combined publication must not commit its HTML before PDF planning succeeds.
    let output = workspace
        .command()
        .args([
            "--text",
            INFEASIBLE,
            "--to",
            "both",
            "--out",
            "combined.html",
            "--json",
            "--pdf-strict-pagination",
        ])
        .args(TIGHT)
        .output()
        .unwrap();
    refused(&output, "pagination_infeasible");
    assert!(!workspace.0.join("combined.html").exists());
    assert!(!workspace.0.join("combined.pdf").exists());
}

#[test]
fn strict_budget_refusal_is_distinct_from_infeasible_layout() {
    let workspace = Workspace::new();
    // Legal native paper, but its usable body exceeds the bounded exact solver.
    let tall = [
        "--page-size",
        "612x10200",
        "--margin-top-pt",
        "36",
        "--margin-bottom-pt",
        "36",
    ];
    succeeded(&workspace.render("A", "fallback.pdf", &tall));
    let flags: Vec<&str> = tall
        .into_iter()
        .chain(["--pdf-strict-pagination"])
        .collect();
    refused(
        &workspace.render("A", "strict.pdf", &flags),
        "pagination_budget_exceeded",
    );
    assert!(!workspace.0.join("strict.pdf").exists());
}

#[test]
fn configured_strict_policy_round_trips_and_can_be_overridden() {
    let workspace = Workspace::new();
    succeeded(&workspace.run(&["config", "set", "pdf_strict_pagination", "true"]));
    let get = workspace.run(&["config", "get", "pdf_strict_pagination"]);
    succeeded(&get);
    assert_eq!(String::from_utf8(get.stdout).unwrap().trim(), "true");
    let config = fs::read(workspace.0.join("config")).unwrap();
    assert!(String::from_utf8_lossy(&config).contains("pdf_strict_pagination=true\n"));
    let show = workspace.run(&["config", "show", "--json"]);
    succeeded(&show);
    assert!(String::from_utf8_lossy(&show.stdout).contains("\"pdf_strict_pagination\":true"));

    refused(
        &workspace.tight_render("configured.pdf", &[]),
        "pagination_infeasible",
    );
    assert!(!workspace.0.join("configured.pdf").exists());
    succeeded(&workspace.tight_render("disabled.pdf", &["--pdf-strict-pagination=false"]));
    succeeded(&workspace.tight_render("unconfigured.pdf", &["--no-config"]));
    assert_eq!(
        fs::read(workspace.0.join("disabled.pdf")).unwrap(),
        fs::read(workspace.0.join("unconfigured.pdf")).unwrap()
    );
    // A PDF config preference does not prevent HTML publication.
    succeeded(&workspace.run(&["--text", SOURCE, "--to", "html", "--out", "document.html"]));
    let invalid = workspace.run(&[
        "config",
        "set",
        "pdf_strict_pagination",
        "sometimes",
        "--json",
    ]);
    assert_eq!(invalid.status.code(), Some(64));
    assert_eq!(fs::read(workspace.0.join("config")).unwrap(), config);
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
            "watch exited before publication"
        );
        assert!(
            Instant::now() < deadline,
            "watch did not reach the expected publication state"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn strict_watch_keeps_its_last_good_pdf_and_recovers_when_source_is_fixed() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("document.md"), "A").unwrap();
    let log_path = workspace.0.join("watch.log");
    let mut child = WatchChild(
        workspace
            .command()
            .args([
                "watch",
                "--pdf-strict-pagination",
                "document.md",
                "--to",
                "pdf",
                "--out",
                "watched.pdf",
                "--interval",
                "25",
                "--json",
            ])
            .args(TIGHT)
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&log_path).unwrap()))
            .spawn()
            .unwrap(),
    );
    let pdf_path = workspace.0.join("watched.pdf");
    let first = render_pdf("A", &tight_options()).unwrap();
    wait_for_watch(&mut child, || {
        fs::read(&pdf_path).is_ok_and(|bytes| bytes == first)
    });
    fs::write(workspace.0.join("document.md"), INFEASIBLE).unwrap();
    wait_for_watch(&mut child, || {
        fs::read_to_string(&log_path)
            .is_ok_and(|log| log.contains("\"code\":\"pagination_infeasible\""))
    });
    assert_eq!(fs::read(&pdf_path).unwrap(), first);
    fs::write(workspace.0.join("document.md"), "D").unwrap();
    let recovered = render_pdf("D", &tight_options()).unwrap();
    wait_for_watch(&mut child, || {
        fs::read(&pdf_path).is_ok_and(|bytes| bytes == recovered)
    });
}

#[cfg(feature = "batch")]
#[test]
fn batch_uses_configured_strict_policy_and_preserves_prior_output_on_refusal() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("document.md"), INFEASIBLE).unwrap();
    fs::write(workspace.0.join("config"), "pdf_strict_pagination=true\n").unwrap();
    fs::create_dir_all(workspace.0.join("batch-output")).unwrap();
    let output_path = workspace.0.join("batch-output/document.pdf");
    fs::write(&output_path, b"prior batch PDF").unwrap();
    let run = |flags: &[&str]| {
        workspace
            .command()
            .args([
                "batch",
                "document.md",
                "--to",
                "pdf",
                "--out-dir",
                "batch-output",
                "--workers",
                "1",
                "--json",
            ])
            .args(TIGHT)
            .args(flags)
            .output()
            .unwrap()
    };
    let output = run(&[]);
    assert_eq!(
        output.status.code(),
        Some(70),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("strict PDF pagination failed:"));
    assert_eq!(fs::read(&output_path).unwrap(), b"prior batch PDF");
    succeeded(&run(&["--pdf-strict-pagination=false"]));
    let mut expected = tight_options();
    expected.strict_pagination = false;
    assert_eq!(
        fs::read(output_path).unwrap(),
        render_pdf(INFEASIBLE, &expected).unwrap()
    );
}

#[test]
fn native_book_preserves_strict_policy_and_typed_failure_code() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.0.join("book")).unwrap();
    fs::write(workspace.0.join("book/chapter.md"), SOURCE).unwrap();
    fs::write(workspace.0.join("config"), "pdf_strict_pagination=true\n").unwrap();
    let run = |flags: &[&str]| {
        workspace
            .command()
            .args([
                "book",
                "book",
                "--to",
                "pdf",
                "--out",
                "book.pdf",
                "--toc=false",
                "--pdf-page-numbers=false",
                "--json",
            ])
            .args(flags)
            .output()
            .unwrap()
    };
    succeeded(&run(&[]));
    let book = franken_markdown::book::BookRenderer::new(&[franken_markdown::book::BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }])
    .unwrap();
    let mut expected = strict_options();
    expected.title = Some("Strict publication".into());
    assert_eq!(
        fs::read(workspace.0.join("book.pdf")).unwrap(),
        franken_markdown::book::render_book_pdf(book.book(), &expected).unwrap()
    );

    fs::write(workspace.0.join("book.pdf"), b"prior book PDF").unwrap();
    let tall = [
        "--page-size",
        "612x10200",
        "--margin-top-pt",
        "36",
        "--margin-bottom-pt",
        "36",
    ];
    refused(&run(&tall), "pagination_budget_exceeded");
    assert_eq!(
        fs::read(workspace.0.join("book.pdf")).unwrap(),
        b"prior book PDF"
    );
    let disabled: Vec<&str> = tall
        .into_iter()
        .chain(["--pdf-strict-pagination=false"])
        .collect();
    succeeded(&run(&disabled));
    assert!(
        fs::read(workspace.0.join("book.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );
}

#[test]
fn strict_policy_is_discoverable_and_explicit_non_pdf_use_is_rejected() {
    let workspace = Workspace::new();
    for command in ["render", "watch", "book"] {
        let help = workspace.run(&[command, "--help"]);
        succeeded(&help);
        assert!(String::from_utf8_lossy(&help.stdout).contains("--pdf-strict-pagination"));
    }
    #[cfg(feature = "batch")]
    {
        let help = workspace.run(&["batch", "--help"]);
        succeeded(&help);
        assert!(String::from_utf8_lossy(&help.stdout).contains("--pdf-strict-pagination"));
    }
    let capabilities = workspace.run(&["capabilities", "--json"]);
    succeeded(&capabilities);
    assert!(
        String::from_utf8_lossy(&capabilities.stdout)
            .contains("\"strict_pagination_pdf\":\"available_optin_exact_or_error\"")
    );
    for command in [
        vec![
            "--text",
            SOURCE,
            "--to",
            "html",
            "--out",
            "document.html",
            "--pdf-strict-pagination",
            "--json",
        ],
        vec![
            "watch",
            "absent.md",
            "--to",
            "html",
            "--out",
            "document.html",
            "--pdf-strict-pagination",
            "--json",
        ],
        vec![
            "book",
            "absent",
            "--to",
            "html",
            "--out-dir",
            "site",
            "--pdf-strict-pagination",
            "--json",
        ],
    ] {
        let output = workspace.run(&command);
        assert_eq!(
            output.status.code(),
            Some(64),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported_target_option"));
    }
    assert!(!workspace.0.join("document.html").exists());
    assert!(!workspace.0.join("site").exists());
}
