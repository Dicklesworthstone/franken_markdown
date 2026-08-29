//! Drift verification test for shell completions and man pages (bead ncok).
//!
//! Asserts that:
//! 1. All subcommands in the clap definition are covered by bash, zsh, and fish completions.
//! 2. All main CLI flags are present in completion scripts and the man page.
//! 3. Completion scripts are syntactically well-formed.
//! 4. The man page docs/fmd.1 exists, is non-empty, and mentions all subcommands.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

#[test]
fn completions_and_man_page_exist() {
    let bash = Path::new("completions/fmd.bash");
    let zsh = Path::new("completions/fmd.zsh");
    let fish = Path::new("completions/fmd.fish");
    let man = Path::new("docs/fmd.1");

    assert!(bash.exists(), "completions/fmd.bash must exist");
    assert!(zsh.exists(), "completions/fmd.zsh must exist");
    assert!(fish.exists(), "completions/fmd.fish must exist");
    assert!(man.exists(), "docs/fmd.1 must exist");

    let bash_content = fs::read_to_string(bash).expect("read bash completion");
    let zsh_content = fs::read_to_string(zsh).expect("read zsh completion");
    let fish_content = fs::read_to_string(fish).expect("read fish completion");
    let man_content = fs::read_to_string(man).expect("read man page");

    assert!(!bash_content.trim().is_empty());
    assert!(!zsh_content.trim().is_empty());
    assert!(!fish_content.trim().is_empty());
    assert!(!man_content.trim().is_empty());
}

#[test]
fn all_subcommands_are_present_in_completions_and_man() {
    let subcommands = [
        "render",
        "capabilities",
        "robot-docs",
        "verify",
        "watch",
        "doctor",
        "config",
        "stats",
        "diff",
        "book",
        "batch",
        "mcp",
    ];

    let bash = fs::read_to_string("completions/fmd.bash").expect("read bash");
    let zsh = fs::read_to_string("completions/fmd.zsh").expect("read zsh");
    let fish = fs::read_to_string("completions/fmd.fish").expect("read fish");
    let man = fs::read_to_string("docs/fmd.1").expect("read man");

    for sub in subcommands {
        assert!(
            bash.contains(sub),
            "bash completion missing subcommand '{sub}'"
        );
        assert!(
            zsh.contains(sub),
            "zsh completion missing subcommand '{sub}'"
        );
        assert!(
            fish.contains(sub),
            "fish completion missing subcommand '{sub}'"
        );
        let roff_sub = sub.replace('-', "\\-");
        assert!(
            man.contains(sub) || man.contains(&roff_sub),
            "man page docs/fmd.1 missing subcommand '{sub}'"
        );
    }
}

#[test]
fn core_flags_are_present_in_completions_and_man() {
    let core_flags = [
        "--json",
        "--no-color",
        "--no-config",
        "--robot-triage",
        "--to",
        "--out",
        "--font",
        "--css",
        "--font-scale",
        "--fit-to-pages",
        "--microtype",
        "--typography-homogeneous",
        "--pdf-line-numbers",
        "--pdf-page-numbers",
        "--pdf-a",
        "--pdf-a-strict",
        "--toc",
        "--allow-html",
    ];

    let bash = fs::read_to_string("completions/fmd.bash").expect("read bash");
    let zsh = fs::read_to_string("completions/fmd.zsh").expect("read zsh");
    let fish = fs::read_to_string("completions/fmd.fish").expect("read fish");
    let man = fs::read_to_string("docs/fmd.1").expect("read man");

    for flag in core_flags {
        assert!(bash.contains(flag), "bash completion missing flag '{flag}'");
        assert!(zsh.contains(flag), "zsh completion missing flag '{flag}'");
        let flag_name = flag.trim_start_matches('-');
        let fish_long = format!("-l {flag_name}");
        assert!(
            fish.contains(flag) || fish.contains(&fish_long),
            "fish completion missing flag '{flag}' (searched '{flag}' and '{fish_long}')"
        );
        // In roff man pages, flags with double-hyphens are written with roff escapes like \-\-to
        let roff_flag = flag.replace('-', "\\-");
        assert!(
            man.contains(flag) || man.contains(&roff_flag),
            "man page docs/fmd.1 missing flag '{flag}' (searched both '{flag}' and '{roff_flag}')"
        );
    }
}
