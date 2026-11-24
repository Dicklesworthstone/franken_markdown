//! Verification's optional link cache is an output, not permission to replace
//! the Markdown being verified. Probes use cached results or local command fixtures.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const URL: &str = "https://example.invalid/reference";
const TTL: &str = "18446744073709551615";

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-verify-cache-safety-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        std::fs::write(self.path(name), bytes).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command.current_dir(&self.0).args(["--no-config", "verify"]);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
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
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cache_record(url: &str) -> String {
    format!("{{\"url\":\"{url}\",\"status\":200,\"ok\":true,\"checked\":0}}\n")
}

fn source_with_cache_example() -> String {
    // This is both valid Markdown and a usable cache under the former bug.
    // Even a regressed executable therefore reproduces data loss offline.
    format!(
        "# Original source\n\n[Reference]({URL})\n\n```json\n{}```\n",
        cache_record(URL)
    )
}

fn assert_refused(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(64),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage_error"), "{stderr}");
    assert!(stderr.contains("refusing to overwrite"), "{stderr}");
    assert!(!stderr.contains("checked 1 external link"), "{stderr}");
}

#[test]
fn verify_refuses_to_replace_its_source_with_link_cache_records() {
    let dir = Directory::new();
    let source = source_with_cache_example();
    dir.write("source.md", &source);
    for destination in ["source.md", "./source.md"] {
        let output = dir.run(&[
            "source.md",
            "--links",
            "--links-cache",
            destination,
            "--links-ttl-secs",
            TTL,
            "--json",
        ]);
        assert_refused(&output);
        dir.assert_contents("source.md", &source);
    }
}

#[cfg(unix)]
#[test]
fn verify_refuses_symlink_hardlink_and_directory_alias_caches() {
    let dir = Directory::new();
    let source = source_with_cache_example();
    dir.write("source.md", &source);
    std::os::unix::fs::symlink("source.md", dir.path("source-link.jsonl")).unwrap();
    std::fs::hard_link(dir.path("source.md"), dir.path("source-hardlink.jsonl")).unwrap();
    std::os::unix::fs::symlink(".", dir.path("alias")).unwrap();
    for destination in [
        "source-link.jsonl",
        "source-hardlink.jsonl",
        "alias/source.md",
    ] {
        let output = dir.run(&[
            "source.md",
            "--links",
            "--links-cache",
            destination,
            "--links-ttl-secs",
            TTL,
            "--json",
        ]);
        assert_refused(&output);
        dir.assert_contents("source.md", &source);
        dir.assert_contents("source-hardlink.jsonl", &source);
    }
    assert!(
        std::fs::symlink_metadata(dir.path("source-link.jsonl"))
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn a_distinct_cache_preserves_its_probe_time_without_modifying_source() {
    let dir = Directory::new();
    let source = format!("# Source\n\n[Reference]({URL}).\n");
    let cache = cache_record(URL);
    dir.write("source.md", &source);
    dir.write("links.jsonl", &cache);
    let output = dir.run(&[
        "source.md",
        "--links",
        "--links-cache",
        "links.jsonl",
        "--links-ttl-secs",
        TTL,
        "--json",
    ]);
    assert!(output.status.success(), "{:?}", output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("checked 1 external link(s), 0 broken")
    );
    dir.assert_contents("links.jsonl", cache);
    dir.assert_contents("source.md", source);
    assert!(std::fs::read_dir(&dir.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".fmd-")
    }));
}

#[test]
fn unused_cache_flags_do_not_write_or_refuse_the_source() {
    let dir = Directory::new();
    let source = "# Source\n\nNo optional link checks requested.\n";
    dir.write("source.md", source);
    let output = dir.run(&["source.md", "--links-cache", "source.md", "--json"]);
    assert!(output.status.success(), "{:?}", output);
    assert!(!output.stdout.is_empty());
    dir.assert_contents("source.md", source);
}

#[test]
fn verification_from_stdin_can_still_reuse_a_distinct_cache() {
    let dir = Directory::new();
    dir.write("links.jsonl", cache_record(URL));
    let source = format!("# Stdin\n\n[Reference]({URL}).\n");
    let mut child = dir
        .command()
        .args([
            "-",
            "--links",
            "--links-cache",
            "links.jsonl",
            "--links-ttl-secs",
            TTL,
            "--json",
        ])
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
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(!output.stdout.is_empty());
    dir.assert_contents("links.jsonl", cache_record(URL));
    assert!(!dir.path("-").exists());
}

#[cfg(unix)]
#[test]
fn interrupted_cache_staging_preserves_the_complete_previous_cache() {
    let dir = Directory::new();
    // A long URL makes the staged cache exceed the child's file-size cap.
    // The cached status avoids any HTTP request in either old or fixed code.
    let url = format!("https://example.invalid/{}", "a".repeat(4096));
    let source = format!("# Source\n\n[Reference]({url}).\n");
    let cache = cache_record(&url);
    dir.write("source.md", &source);
    dir.write("links.jsonl", &cache);
    let output = Command::new("sh")
        .args([
            "-c",
            "ulimit -c 0; ulimit -f 1; exec \"$@\"",
            "fmd-cache-write-limit",
        ])
        .arg(env!("CARGO_BIN_EXE_fmd"))
        .args([
            "--no-config",
            "verify",
            "source.md",
            "--links",
            "--links-cache",
            "links.jsonl",
            "--links-ttl-secs",
            TTL,
            "--json",
        ])
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    dir.assert_contents("links.jsonl", cache);
    dir.assert_contents("source.md", source);
}

#[cfg(unix)]
#[test]
fn link_findings_use_first_hop_status_and_only_probes_advance_the_cache_clock() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
    const ROOT: &str = "https://links.invalid";
    // Both client interfaces run against deterministic responses. Narrow PATH
    // to this directory so a missing client can never reach the network.
    for client in ["curl", "wget"] {
        let dir = Directory::new();
        dir.write(
            client,
            r#"#!/bin/sh
for arg in "$@"; do url=$arg; done
printf '%s\n' "$url" >> probes
case "$url" in
  */expired) status=302 ;;
  */future) status=404 ;;
  *) status=200 ;;
esac
case "$0" in
  */curl) printf '%s' "$status" ;;
  */wget)
    printf '  HTTP/1.1 %s fixture\n' "$status" >&2
    case " $* " in
      *" --max-redirect=0 "*) ;;
      *) if [ "$status" = 302 ]; then printf '  HTTP/1.1 200 destination\n' >&2; fi ;;
    esac ;;
esac
"#,
        );
        std::fs::set_permissions(dir.path(client), std::fs::Permissions::from_mode(0o755)).unwrap();
        dir.write(
            "source.md",
            format!(
                "[Cached]({ROOT}/cached)\n\n[Expired]({ROOT}/expired)\n\n[Future]({ROOT}/future)\n"
            ),
        );
        let start = now();
        let valid = start - 30;
        let record = |name: &str, status: u16, ok: bool, checked: u64| {
            format!(r#"{{"url":"{ROOT}/{name}","status":{status},"ok":{ok},"checked":{checked}}}"#)
        };
        dir.write(
            "links.jsonl",
            [
                record("cached", 301, false, valid), // Legacy HEAD verdict.
                record("expired", 200, true, 1),
                record("future", 200, true, u64::MAX),
            ]
            .join("\n"),
        );
        let verify = || {
            dir.command()
                .args([
                    "source.md",
                    "--links",
                    "--links-cache",
                    "links.jsonl",
                    "--links-ttl-secs",
                    "3600",
                    "--json",
                ])
                .env("PATH", &dir.0)
                .output()
                .unwrap()
        };
        let output = verify();
        assert_eq!(output.status.code(), Some(1), "{client}: {output:?}");
        let report = String::from_utf8(output.stdout).unwrap();
        for (name, status) in [("cached", 301), ("expired", 302)] {
            assert!(
                report.contains(&format!(
                    "external link {ROOT}/{name} redirected (HTTP {status})"
                )),
                "{report}"
            );
        }
        assert!(
            report.contains(&format!("external link {ROOT}/future returned HTTP 404")),
            "{report}"
        );
        let probes = std::fs::read_to_string(dir.path("probes")).unwrap();
        assert_eq!(
            probes.lines().collect::<Vec<_>>(),
            [format!("{ROOT}/expired"), format!("{ROOT}/future")]
        );
        let end = now();
        let cache = std::fs::read_to_string(dir.path("links.jsonl")).unwrap();
        assert!(
            cache
                .lines()
                .any(|line| line == record("cached", 301, true, valid))
        );
        for (name, status, ok) in [("expired", 302, true), ("future", 404, false)] {
            let line = cache
                .lines()
                .find(|line| line.contains(&format!("{ROOT}/{name}")))
                .unwrap();
            let stamp: u64 = line
                .split_once("\"checked\":")
                .unwrap()
                .1
                .trim_end_matches('}')
                .parse()
                .unwrap();
            assert!((start..=end).contains(&stamp));
            assert_eq!(line, record(name, status, ok, stamp));
        }
        let replay = verify();
        assert_eq!(replay.status.code(), Some(1));
        assert_eq!(replay.stdout, report.as_bytes());
        dir.assert_contents("probes", probes);
        dir.assert_contents("links.jsonl", cache);
    }
}
