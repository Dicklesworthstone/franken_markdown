//! No-network behavioral tests for installer release and checksum resolution.
//!
//! Unix-only: the subject under test is the POSIX `install.sh` run via
//! `bash`, and on windows-latest a bare `bash` resolves to the WSL launcher
//! stub (which fails with "no installed distributions"). Windows installs use
//! `install.ps1` instead, so this test crate is compiled out there.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

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
            .contains("installer version resolution self-test: ok"),
        "self-test should exercise compact, multiline, malformed, and ambiguous release JSON"
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

/// Execute the production resolver without the installer preflight, downloads,
/// locks, or cleanup. The transport stub records which endpoint was requested.
fn resolve_with_response(json: &str, redirect: &str, setup: &str) -> TestResult<(String, String)> {
    let source = fs::read_to_string("install.sh")?;
    let resolver = source
        .split_once("release_tag_is_valid() {")
        .ok_or("missing release parser")?
        .1
        .split_once("\nresolve_version\n")
        .ok_or("missing resolver invocation")?
        .0;
    let script = format!(
        r#"set -euo pipefail
exec 3>&2
release_tag_is_valid() {{{resolver}
OWNER=example
REPO=renderer
VERSION=
VERSION_BARE=
FROM_SOURCE=0
info() {{ :; }}
warn() {{ :; }}
xcurl() {{
  case "$*" in
    *api.github.com*) printf '%s' "$FMD_TEST_RELEASE_JSON"; echo api >&3 ;;
    *) printf '%s' "$FMD_TEST_RELEASE_REDIRECT"; echo redirect >&3 ;;
  esac
}}
{setup}
resolve_version
printf 'version=%s\nbare=%s\nsource=%s\n' "$VERSION" "$VERSION_BARE" "$FROM_SOURCE"
"#
    );
    let output = Command::new("bash")
        .args(["-c", &script])
        .env("FMD_TEST_RELEASE_JSON", json)
        .env("FMD_TEST_RELEASE_REDIRECT", redirect)
        .output()?;
    assert!(
        output.status.success(),
        "release resolution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

#[test]
fn latest_release_resolution_uses_tag_in_compact_json() -> TestResult {
    assert_eq!(
        resolve_with_response(
            r###"{"tag_name":"v0.5.0","assets":[],"body":"## Highlights"}"###,
            "https://github.com/example/renderer/releases/tag/v9.9.9",
            "",
        )?,
        (
            "version=v0.5.0\nbare=0.5.0\nsource=0\n".into(),
            "api\n".into()
        )
    );
    Ok(())
}

#[test]
fn invalid_latest_response_uses_only_a_valid_release_redirect() -> TestResult {
    for json in [
        r#"{"message":"Not Found"}"#,
        r#"{"tag_name":"v1.2.3/../../main"}"#,
        r#"{"tag_name":"v1.2.3","tag_name":"v9.9.9"}"#,
    ] {
        assert_eq!(
            resolve_with_response(
                json,
                "https://github.com/example/renderer/releases/tag/v2.3.4",
                "",
            )?,
            (
                "version=v2.3.4\nbare=2.3.4\nsource=0\n".into(),
                "api\nredirect\n".into()
            )
        );
        for redirect in [
            "https://github.com/example/renderer/releases",
            "https://github.com/example/renderer/releases/tag/v1.2.3/../../main",
        ] {
            assert_eq!(
                resolve_with_response(json, redirect, "")?,
                (
                    "version=\nbare=\nsource=1\n".into(),
                    "api\nredirect\n".into()
                )
            );
        }
    }
    Ok(())
}

#[test]
fn explicit_version_and_source_mode_do_not_resolve_latest() -> TestResult {
    assert_eq!(
        resolve_with_response("", "", "VERSION=v1.2.3; VERSION_BARE=1.2.3",)?,
        (
            "version=v1.2.3\nbare=1.2.3\nsource=0\n".into(),
            String::new()
        )
    );
    assert_eq!(
        resolve_with_response("", "", "FROM_SOURCE=1")?,
        ("version=\nbare=\nsource=1\n".into(), String::new())
    );
    Ok(())
}
