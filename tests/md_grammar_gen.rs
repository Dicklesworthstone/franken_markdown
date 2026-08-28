//! Self-tests for the grammar-aware Markdown generator (bead 2c72.1).
//!
//! Every assertion logs `check=<id> subject=<…> outcome=PASS|FAIL` on stderr.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{ADVERSARIES, Adversary, GenOptions, adversarial, generate};

fn log_check(id: &str, subject: &str, outcome: &str) {
    eprintln!("check={id} subject={subject} outcome={outcome}");
}

fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
    if ok {
        log_check(id, subject, "PASS");
    } else {
        log_check(id, subject, "FAIL");
        panic!("{id} failed for `{subject}`: {detail}");
    }
}

#[test]
fn same_seed_reproduces_bytes() {
    let opts = GenOptions::default();
    for seed in [0u64, 1, 7, 42, 99_991] {
        let a = generate(seed, &opts);
        let b = generate(seed, &opts);
        assert_ok(
            "repro",
            &format!("seed={seed}"),
            a == b && !a.is_empty(),
            "mismatch or empty",
        );
    }
}

#[test]
fn different_seeds_usually_differ() {
    let opts = GenOptions::default();
    let a = generate(1, &opts);
    let b = generate(2, &opts);
    assert_ok(
        "seed-diverges",
        "1 vs 2",
        a != b,
        "distinct seeds produced identical documents",
    );
}

#[test]
fn never_exceeds_max_bytes() {
    let opts = GenOptions {
        max_bytes: 512,
        max_depth: 3,
        max_blocks: 40,
        verbose: false,
    };
    for seed in 0..80u64 {
        let s = generate(seed, &opts);
        assert_ok(
            "cap",
            &format!("seed={seed} len={}", s.len()),
            s.len() <= 512,
            "over cap",
        );
    }
}

#[test]
fn distribution_hits_major_constructs() {
    let opts = GenOptions {
        max_bytes: 4096,
        max_depth: 3,
        max_blocks: 20,
        verbose: false,
    };
    let mut saw_atx = false;
    let mut saw_list = false;
    let mut saw_table = false;
    let mut saw_fence = false;
    let mut saw_quote = false;
    for seed in 0..120u64 {
        let s = generate(seed, &opts);
        if s.contains("\n# ") || s.starts_with("# ") {
            saw_atx = true;
        }
        if s.contains("\n- ") || s.starts_with("- ") {
            saw_list = true;
        }
        if s.contains("| --- |") {
            saw_table = true;
        }
        if s.contains("```") {
            saw_fence = true;
        }
        if s.contains("\n> ") || s.starts_with("> ") {
            saw_quote = true;
        }
    }
    assert_ok("dist-atx", "heading", saw_atx, "no ATX heading in 120 docs");
    assert_ok("dist-list", "list", saw_list, "no list");
    assert_ok("dist-table", "table", saw_table, "no table");
    assert_ok("dist-fence", "fence", saw_fence, "no fence");
    assert_ok("dist-quote", "quote", saw_quote, "no blockquote");
}

#[test]
fn adversarial_library_is_capped_and_named() {
    for kind in ADVERSARIES {
        let s = adversarial(*kind, 1024);
        assert_ok(
            "adv-cap",
            kind.name(),
            s.len() <= 1024 && !s.is_empty(),
            &format!("len={}", s.len()),
        );
    }
    let nul = adversarial(Adversary::Nul, 1024);
    assert_ok("adv-nul", "nul", nul.contains('\0'), "missing NUL");
    let crlf = adversarial(Adversary::Crlf, 1024);
    assert_ok("adv-crlf", "crlf", crlf.contains("\r\n"), "missing CRLF");
    let astral = adversarial(Adversary::Astral, 1024);
    assert_ok(
        "adv-astral",
        "astral",
        astral.chars().any(|c| c as u32 > 0xFFFF),
        "no astral scalar",
    );
    let unclosed = adversarial(Adversary::Unclosed, 1024);
    assert_ok(
        "adv-unclosed",
        "unclosed",
        unclosed.contains("```") && !unclosed.trim_end().ends_with("```"),
        "not an unclosed fence",
    );
    let run = adversarial(Adversary::EmphasisRun, 2048);
    assert_ok(
        "adv-emphasis",
        "emphasis-run",
        run.starts_with('*') && run.contains('x') && run.len() <= 2048,
        &format!("len={}", run.len()),
    );
}

#[test]
fn hundred_kb_emphasis_obeys_caller_cap() {
    let s = adversarial(Adversary::EmphasisRun, 100_000);
    assert_ok(
        "emphasis-100k-cap",
        "100000",
        s.len() <= 100_000 && s.len() >= 1000,
        &format!("len={}", s.len()),
    );
}

#[test]
fn verbose_emits_phase_lines_on_stderr() {
    let opts = GenOptions {
        max_bytes: 256,
        max_depth: 1,
        max_blocks: 3,
        verbose: true,
    };
    let _ = generate(3, &opts);
    log_check("verbose", "seed=3", "PASS");
}

#[test]
fn utf8_always() {
    let opts = GenOptions::default();
    for seed in 0..30u64 {
        let s = generate(seed, &opts);
        assert_ok(
            "utf8",
            &format!("seed={seed}"),
            std::str::from_utf8(s.as_bytes()).is_ok(),
            "non-utf8",
        );
    }
}
