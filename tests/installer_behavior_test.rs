//! No-network behavioral tests for installer checksum parsing.
//!
//! Unix-only: the subject under test is the POSIX `install.sh` run via
//! `bash`, and on windows-latest a bare `bash` resolves to the WSL launcher
//! stub (which fails with "no installed distributions"). Windows installs use
//! `install.ps1` instead, so this test crate is compiled out there.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("fmd-{label}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn unix_installer_checksum_self_test_is_no_network_and_no_install() -> TestResult {
    let temp = TempDir::new("installer-behavior")?;
    let dest = temp.path().join("bin");
    fs::create_dir_all(&dest)?;

    let output = Command::new("bash")
        .arg("install.sh")
        .arg("--from-source")
        .arg("--dest")
        .arg(&dest)
        .arg("--force")
        .arg("--quiet")
        .arg("--no-gum")
        .env("FMD_INSTALLER_CHECKSUM_SELF_TEST", "1")
        .env("NO_COLOR", "1")
        .env(
            "ARTIFACT_URL",
            "http://127.0.0.1:9/should-not-be-used.tar.gz",
        )
        .output()?;

    assert!(
        output.status.success(),
        "installer checksum self-test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("installer checksum self-test: ok"),
        "self-test should report success on stdout"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Checksum verification explicitly skipped by CHECKSUM=SKIP"),
        "self-test should exercise the visible CHECKSUM=SKIP warning path"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("installer archive path self-test: ok"),
        "self-test should exercise archive member path validation"
    );
    assert!(
        !dest.join("fmd").exists(),
        "checksum self-test must not install a binary"
    );

    Ok(())
}
