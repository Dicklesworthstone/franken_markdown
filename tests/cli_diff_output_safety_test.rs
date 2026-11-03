//! Diff publications must never consume either revision or truncate a report
//! before its replacement has been written successfully.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const OLD: &str = "# Before\n\nOriginal unpublished text.\n";
const NEW: &str = "# After\n\nRevised unpublished text.\n";
const PREVIOUS_REPORT: &str = "An earlier report that must survive failed publication.\n";

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-diff-output-safety-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let dir = Self(path);
        dir.write("old.md", OLD);
        dir.write("new.md", NEW);
        dir
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        std::fs::write(self.path(name), bytes).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.path("settings.conf"));
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command()
            .arg("--no-config")
            .args(args)
            .output()
            .unwrap()
    }

    fn assert_contents(&self, name: &str, expected: impl AsRef<[u8]>) {
        let actual = std::fs::read(self.path(name)).unwrap();
        assert!(
            actual == expected.as_ref(),
            "{name} differs: actual {} bytes, expected {} bytes",
            actual.len(),
            expected.as_ref().len()
        );
    }

    fn assert_sources(&self) {
        self.assert_contents("old.md", OLD);
        self.assert_contents("new.md", NEW);
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_refused(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(64),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"));
}

#[test]
fn diff_refuses_both_revision_paths_without_touching_source_bytes() {
    let dir = Directory::new();
    for destination in ["old.md", "new.md", "./old.md", "./new.md"] {
        assert_refused(&dir.run(&["diff", "old.md", "new.md", "--out", destination]));
        dir.assert_sources();
    }
}

#[cfg(unix)]
#[test]
fn diff_refuses_file_and_directory_aliases_of_either_revision() {
    let dir = Directory::new();
    std::os::unix::fs::symlink("old.md", dir.path("old-link.html")).unwrap();
    std::os::unix::fs::symlink("new.md", dir.path("new-link.html")).unwrap();
    std::fs::hard_link(dir.path("old.md"), dir.path("old-hardlink.html")).unwrap();
    std::fs::hard_link(dir.path("new.md"), dir.path("new-hardlink.html")).unwrap();
    std::os::unix::fs::symlink(".", dir.path("alias")).unwrap();

    for destination in [
        "old-link.html",
        "new-link.html",
        "old-hardlink.html",
        "new-hardlink.html",
        "alias/old.md",
        "alias/new.md",
    ] {
        assert_refused(&dir.run(&["diff", "old.md", "new.md", "--out", destination]));
        dir.assert_sources();
    }
    for name in ["old-link.html", "new-link.html"] {
        assert!(
            std::fs::symlink_metadata(dir.path(name))
                .unwrap()
                .is_symlink()
        );
    }
    dir.assert_contents("old-hardlink.html", OLD);
    dir.assert_contents("new-hardlink.html", NEW);
}

#[test]
fn diff_preserves_the_active_config_and_its_missing_destination() {
    let dir = Directory::new();
    // Missing config means default options, not permission to create an HTML
    // file where subsequent invocations will expect configuration.
    let output = dir
        .command()
        .args(["diff", "old.md", "new.md", "--out", "settings.conf"])
        .output()
        .unwrap();
    assert_refused(&output);
    assert!(!dir.path("settings.conf").exists());

    let config = "font=serif\n";
    dir.write("settings.conf", config);
    let output = dir
        .command()
        .args(["diff", "old.md", "new.md", "--out", "settings.conf"])
        .output()
        .unwrap();
    assert_refused(&output);
    dir.assert_contents("settings.conf", config);
    dir.assert_sources();

    // --no-config does not reserve an unrelated, unused path.
    assert!(
        dir.run(&["diff", "old.md", "new.md", "--out", "settings.conf"])
            .status
            .success()
    );
    dir.assert_sources();
}

#[cfg(unix)]
#[test]
fn diff_preserves_aliases_of_its_active_configuration() {
    let dir = Directory::new();
    dir.write("settings.conf", "font=serif\n");
    std::os::unix::fs::symlink("settings.conf", dir.path("config-link.html")).unwrap();
    std::fs::hard_link(dir.path("settings.conf"), dir.path("config-hardlink.html")).unwrap();
    for destination in ["config-link.html", "config-hardlink.html"] {
        let output = dir
            .command()
            .args(["diff", "old.md", "new.md", "--out", destination])
            .output()
            .unwrap();
        assert_refused(&output);
    }
    dir.assert_contents("settings.conf", "font=serif\n");
    assert!(
        std::fs::symlink_metadata(dir.path("config-link.html"))
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn diff_replaces_a_distinct_report_with_the_complete_stdout_document() {
    let dir = Directory::new();
    let stdout = dir.run(&["diff", "old.md", "new.md"]);
    assert!(stdout.status.success());
    assert!(String::from_utf8_lossy(&stdout.stdout).contains("diff-inline"));
    dir.write("report.html", PREVIOUS_REPORT);
    let output = dir.run(&["diff", "old.md", "new.md", "--out", "report.html"]);
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stdout.is_empty());
    // The stdout helper adds a terminal newline after the complete document;
    // files retain exactly the rendered bytes, as before staged publication.
    dir.assert_contents("report.html", stdout.stdout.strip_suffix(b"\n").unwrap());
    dir.assert_sources();
    assert!(std::fs::read_dir(&dir.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".fmd-")
    }));
}

#[test]
fn diff_input_and_output_failures_preserve_sources_and_previous_reports() {
    let dir = Directory::new();
    dir.write("report.html", PREVIOUS_REPORT);
    dir.write("invalid.md", [0xff]);
    let output = dir.run(&["diff", "old.md", "invalid.md", "--out", "report.html"]);
    assert_eq!(output.status.code(), Some(66));
    dir.assert_contents("report.html", PREVIOUS_REPORT);

    let output = dir.run(&["diff", "old.md", "new.md", "--out", "absent/report.html"]);
    assert_eq!(output.status.code(), Some(73));
    assert!(!dir.path("absent").exists());
    std::fs::create_dir(dir.path("directory.html")).unwrap();
    dir.write("directory.html/keep", PREVIOUS_REPORT);
    let output = dir.run(&["diff", "old.md", "new.md", "--out", "directory.html"]);
    assert_eq!(output.status.code(), Some(73));
    dir.assert_contents("directory.html/keep", PREVIOUS_REPORT);
    dir.assert_sources();
}

#[cfg(unix)]
#[test]
fn interrupted_diff_staging_cannot_truncate_the_previous_report() {
    let dir = Directory::new();
    dir.write("report.html", PREVIOUS_REPORT);
    // Limit the child's writes, not its inputs. The real generated HTML is
    // larger than one file-size block, so this deterministically interrupts
    // publication after bytes have been written. Direct fs::write would
    // already have truncated report.html; staging leaves it untouched.
    let output = Command::new("sh")
        .args([
            "-c",
            "ulimit -c 0; ulimit -f 1; exec \"$@\"",
            "fmd-diff-write-limit",
        ])
        .arg(env!("CARGO_BIN_EXE_fmd"))
        .args([
            "--no-config",
            "diff",
            "old.md",
            "new.md",
            "--out",
            "report.html",
        ])
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    dir.assert_contents("report.html", PREVIOUS_REPORT);
    dir.assert_sources();
}

#[test]
fn diff_keeps_stdin_stdout_and_json_output_semantics() {
    let dir = Directory::new();
    let mut child = dir
        .command()
        .args(["--no-config", "diff", "-", "new.md", "--out", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(OLD.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("diff-inline"));
    assert!(!dir.path("-").exists());

    // JSON reports have always used stdout, even when --out is also passed.
    let output = dir.run(&["diff", "old.md", "new.md", "--json", "--out", "new.md"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"schema\":\"fmd-diff-v1\""));
    dir.assert_sources();
}
