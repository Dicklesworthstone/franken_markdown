//! Persistent native dialect defaults must own parsing before includes or output.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "# Profiles\n\n*Cross-line\n: emphasis*\n\nFirst\n: value\n";
const LITERAL: &str = "`Example\n: code\n{{#include missing.txt}}\ntail`\n";
const LINK: &str = "[Cross-line β\r\n: link](#missing)\r\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-profile-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.0.join("config"))
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .args(args)
            .output()
            .unwrap()
    }

    fn bytes(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
    }

    fn profile(&self, name: &str) {
        succeeded(&self.run(&["config", "set", "profile", name]));
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn text(output: Output) -> String {
    succeeded(&output);
    String::from_utf8(output.stdout).unwrap()
}

fn refused(output: &Output, code: i32, detail: &str) {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(detail), "{stderr}");
}

#[test]
fn config_profile_round_trips_without_turning_omission_into_an_explicit_profile() {
    let dir = Workspace::new();
    assert_eq!(text(dir.run(&["config", "get", "profile"])), "legacy\n");
    assert!(text(dir.run(&["config", "show"])).contains("profile: legacy"));
    assert!(text(dir.run(&["config", "show", "--json"])).contains("\"profile\":null"));
    assert!(!dir.0.join("config").exists());

    for profile in ["commonmark-gfm", "gfm-plus"] {
        dir.profile(profile);
        assert_eq!(
            text(dir.run(&["config", "get", "profile"])),
            format!("{profile}\n")
        );
        assert!(
            text(dir.run(&["config", "show", "--json"]))
                .contains(&format!("\"profile\":\"{profile}\""))
        );
        assert!(
            String::from_utf8(dir.bytes("config"))
                .unwrap()
                .contains(&format!("profile={profile}\n"))
        );
    }
    let prior = dir.bytes("config");
    for invalid in ["", "gfm", "gfm_plus", "legacy", "unknown"] {
        refused(
            &dir.run(&["config", "set", "profile", invalid, "--json"]),
            64,
            "profile must be",
        );
        assert_eq!(dir.bytes("config"), prior);
    }
    assert_eq!(
        text(dir.run(&["--no-config", "config", "get", "profile"])),
        "legacy\n"
    );
}

#[test]
fn every_native_format_uses_configured_grammar_and_explicit_flags_win() {
    let dir = Workspace::new();
    dir.write("doc.md", SOURCE);
    dir.profile("commonmark-gfm");
    for (target, files) in [
        ("both", vec!["doc.html", "doc.pdf"]),
        ("epub", vec!["doc.epub"]),
        ("svg", vec!["doc.svg"]),
    ] {
        succeeded(&dir.run(&["doc.md", "--to", target, "--out", files[0]]));
        let configured: Vec<_> = files.iter().map(|name| dir.bytes(name)).collect();
        succeeded(&dir.run(&[
            "--no-config",
            "doc.md",
            "--profile",
            "commonmark-gfm",
            "--to",
            target,
            "--out",
            files[0],
        ]));
        for (name, expected) in files.iter().zip(configured) {
            assert_eq!(dir.bytes(name), expected, "{target}: {name}");
        }
    }
    let ordinary = text(dir.run(&["doc.md", "--out", "-"]));
    assert!(ordinary.contains("<em>Cross-line\n: emphasis</em>"));
    assert!(!ordinary.contains("<dl>"));
    let extended = text(dir.run(&["doc.md", "--profile", "gfm-plus", "--out", "-"]));
    assert!(extended.contains("<dl>"));
    assert_ne!(ordinary, extended);
    assert_eq!(
        text(dir.run(&["--no-config", "doc.md", "--out", "-"])),
        extended
    );
    dir.profile("gfm-plus");
    assert_eq!(text(dir.run(&["doc.md", "--out", "-"])), extended);
    assert_eq!(
        text(dir.run(&["doc.md", "--profile", "commonmark-gfm", "--out", "-"])),
        ordinary
    );
    assert_eq!(dir.bytes("doc.md"), SOURCE.as_bytes());
}

#[test]
fn includes_and_verify_use_the_default_before_deciding_what_is_literal_code() {
    let dir = Workspace::new();
    dir.write("doc.md", "{{#include parts/body.txt}}\n");
    dir.write("parts/body.txt", LITERAL);
    dir.profile("commonmark-gfm");
    succeeded(&dir.run(&["doc.md", "--out", "doc.html"]));
    let original = dir.bytes("doc.html");
    assert!(
        String::from_utf8_lossy(&original)
            .contains("<code>Example : code {{#include missing.txt}} tail</code>")
    );
    let configured = dir.run(&["verify", "doc.md", "--json"]);
    let explicit = dir.run(&[
        "--no-config",
        "verify",
        "doc.md",
        "--profile",
        "commonmark-gfm",
        "--json",
    ]);
    assert!(matches!(configured.status.code(), Some(0 | 1)));
    assert_eq!(configured.status.code(), explicit.status.code());
    assert_eq!(configured.stdout, explicit.stdout);
    assert!(!configured.stdout.is_empty());

    for args in [
        vec![
            "doc.md",
            "--profile",
            "gfm-plus",
            "--out",
            "doc.html",
            "--json",
        ],
        vec!["--no-config", "doc.md", "--out", "doc.html", "--json"],
        vec!["verify", "doc.md", "--profile", "gfm-plus", "--json"],
    ] {
        refused(&dir.run(&args), 66, "missing.txt");
        assert_eq!(dir.bytes("doc.html"), original);
    }
    dir.profile("gfm-plus");
    succeeded(&dir.run(&["doc.md", "--profile", "commonmark-gfm", "--out", "doc.html"]));
    assert_eq!(dir.bytes("doc.html"), original);
    assert_eq!(dir.bytes("parts/body.txt"), LITERAL.as_bytes());
}

#[test]
fn stats_and_diff_share_configured_grammar_with_publication_and_keep_legacy_omission() {
    let dir = Workspace::new();
    dir.write("old.md", "*Term\n: old value*\n");
    dir.write("new.md", "*Term\n: new value*\n");
    for (profile, paragraphs, links, modified) in
        [("commonmark-gfm", 1, 1, 1), ("gfm-plus", 0, 0, 0)]
    {
        dir.profile(profile);
        let stats = text(dir.run(&["stats", "--text", LINK, "--json"]));
        assert!(
            stats.contains(&format!("\"paragraphs\":{paragraphs}")),
            "{stats}"
        );
        assert!(
            stats.contains(&format!("\"links_total\":{links}")),
            "{stats}"
        );
        assert_eq!(
            stats,
            text(dir.run(&[
                "--no-config",
                "stats",
                "--text",
                LINK,
                "--profile",
                profile,
                "--json",
            ]))
        );
        let diff = text(dir.run(&["diff", "old.md", "new.md", "--json"]));
        assert!(
            diff.contains(&format!("\"modified_blocks\":{modified}")),
            "{diff}"
        );
        assert_eq!(
            diff,
            text(dir.run(&[
                "--no-config",
                "diff",
                "old.md",
                "new.md",
                "--profile",
                profile,
                "--json",
            ]))
        );
    }
    let override_stats = text(dir.run(&[
        "stats",
        "--text",
        LINK,
        "--profile",
        "commonmark-gfm",
        "--json",
    ]));
    assert!(override_stats.contains("\"links_total\":1"));
    let override_diff = text(dir.run(&[
        "diff",
        "old.md",
        "new.md",
        "--profile",
        "commonmark-gfm",
        "--json",
    ]));
    assert!(override_diff.contains("\"modified_blocks\":1"));

    let page_break = "<!-- pagebreak -->\n";
    let plus = text(dir.run(&["stats", "--text", page_break, "--json"]));
    assert!(plus.contains("\"characters\":0"), "{plus}");
    dir.write("config", "# No authoring profile is configured.\n");
    assert_eq!(
        text(dir.run(&["stats", "--text", page_break, "--json"])),
        text(dir.run(&["--no-config", "stats", "--text", page_break, "--json"]))
    );
    assert_eq!(
        text(dir.run(&["diff", "old.md", "new.md", "--json"])),
        text(dir.run(&["--no-config", "diff", "old.md", "new.md", "--json"]))
    );
}

#[test]
fn configured_book_profile_owns_included_navigation_and_read_only_checks() {
    let dir = Workspace::new();
    dir.write("book/start.md", "# Start\n\n{{#include parts/shared.md}}\n");
    dir.write("book/end.md", "# End\n");
    dir.write("book/book.toml", "include_only=['parts/shared.md']\n");
    let shared =
        format!("*Term\n: definition*\n\n[Term\n: definition](../end.md#end)\n\n{LITERAL}");
    dir.write("book/parts/shared.md", &shared);
    dir.profile("commonmark-gfm");
    succeeded(&dir.run(&[
        "book",
        "book",
        "--to",
        "html",
        "--out-dir",
        "site",
        "--deny-broken-links",
    ]));
    let original = dir.bytes("site/start.html");
    let html = String::from_utf8_lossy(&original);
    assert!(html.contains("<em>Term\n: definition</em>"));
    assert!(html.contains("href=\"end.html#end\""));
    assert!(html.contains("{{#include missing.txt}}"));
    succeeded(&dir.run(&["book", "book", "--check-links", "--json"]));

    dir.profile("gfm-plus");
    refused(
        &dir.run(&["book", "book", "--check-links", "--json"]),
        66,
        "missing.txt",
    );
    succeeded(&dir.run(&[
        "book",
        "book",
        "--check-links",
        "--profile",
        "commonmark-gfm",
        "--json",
    ]));
    dir.profile("commonmark-gfm");
    dir.write(
        "book/parts/shared.md",
        &shared.replace("#end)", "#missing)"),
    );
    let checked = dir.run(&["book", "book", "--check-links", "--json"]);
    assert_eq!(checked.status.code(), Some(65), "{checked:?}");
    assert!(String::from_utf8_lossy(&checked.stdout).contains("missing_anchor"));
    let denied = dir.run(&[
        "book",
        "book",
        "--to",
        "html",
        "--out-dir",
        "site",
        "--deny-broken-links",
    ]);
    assert_eq!(denied.status.code(), Some(65), "{denied:?}");
    assert_eq!(dir.bytes("site/start.html"), original);
}

#[test]
fn invalid_profile_config_fails_before_inputs_or_outputs_and_no_config_bypasses_it() {
    let dir = Workspace::new();
    dir.write("config", "profile=unsupported\n");
    dir.write("keep.html", "previous publication");
    dir.write("doc.md", SOURCE);
    let mut commands = vec![
        vec!["missing.md", "--out", "keep.html", "--json"],
        vec!["stats", "missing.md", "--json"],
        vec![
            "diff",
            "missing.md",
            "absent.md",
            "--out",
            "keep.html",
            "--json",
        ],
        vec!["verify", "missing.md", "--json"],
        vec!["book", "missing", "--out-dir", "site", "--json"],
        vec!["book", "missing", "--check-links", "--json"],
    ];
    if cfg!(feature = "batch") {
        commands.push(vec!["batch", "missing.md", "--out-dir", "site", "--json"]);
    }
    for args in commands {
        refused(&dir.run(&args), 66, "config_error");
        assert_eq!(dir.bytes("keep.html"), b"previous publication");
        assert!(!dir.0.join("site").exists());
    }
    let html = text(dir.run(&[
        "--no-config",
        "doc.md",
        "--profile",
        "commonmark-gfm",
        "--out",
        "-",
    ]));
    assert!(html.contains("<em>Cross-line\n: emphasis</em>"));
}

#[test]
fn verification_cannot_replace_its_active_profile_config_with_a_link_cache() {
    let dir = Workspace::new();
    dir.write("doc.md", "# Local document\n");
    dir.profile("commonmark-gfm");
    let original = dir.bytes("config");
    #[cfg(not(unix))]
    let caches = ["config"];
    #[cfg(unix)]
    let caches = {
        std::os::unix::fs::symlink(dir.0.join("config"), dir.0.join("alias")).unwrap();
        ["config", "alias"]
    };
    for cache in caches {
        refused(
            &dir.run(&[
                "verify",
                "doc.md",
                "--links",
                "--links-cache",
                cache,
                "--json",
            ]),
            64,
            "refusing to overwrite verify input",
        );
        assert_eq!(dir.bytes("config"), original);
    }
}

#[cfg(feature = "batch")]
#[test]
fn batch_inherits_the_same_profile_for_includes_and_combined_publication() {
    let dir = Workspace::new();
    dir.write("doc.md", "{{#include parts/body.txt}}\n");
    dir.write("parts/body.txt", LITERAL);
    dir.profile("commonmark-gfm");
    let args = [
        "batch",
        "doc.md",
        "--to",
        "both",
        "--out-dir",
        "batch-out",
        "--workers",
        "1",
    ];
    succeeded(&dir.run(&args));
    let html = dir.bytes("batch-out/doc.html");
    let pdf = dir.bytes("batch-out/doc.pdf");
    succeeded(&dir.run(&[
        "--no-config",
        "batch",
        "doc.md",
        "--to",
        "both",
        "--out-dir",
        "batch-out",
        "--workers",
        "1",
        "--profile",
        "commonmark-gfm",
    ]));
    assert_eq!(dir.bytes("batch-out/doc.html"), html);
    assert_eq!(dir.bytes("batch-out/doc.pdf"), pdf);
    assert!(String::from_utf8_lossy(&html).contains("{{#include missing.txt}}"));
}
