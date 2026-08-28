//! Compile TeX hyphenation pattern files into the compact token modules
//! embedded by `Hyphenator` (bead 38re.1).
//!
//! Reads TUG `hyph-utf8` `.tex` sources (or already-tokenized `.pat.txt`),
//! strips comments, extracts `\patterns{...}`, and writes deterministic
//! one-token-per-line files under `data/`. Stderr is the size report
//! (token count, bytes, trie nodes/edges/values); stdout stays empty so
//! agents can treat it as a data-free compiler.
//!
//! ```text
//! cargo run --example gen_hyphen_patterns -- \
//!     --src /path/to/hyph-utf8/patterns --out data
//! ```
//!
//! Without `--src`, reports sizes of the committed `data/hyph-*.patterns`
//! files (no write). Output is deterministic for a given source tree.

use franken_markdown::layout::{HyphenLang, Hyphenator};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

struct LangSpec {
    lang: HyphenLang,
    stem: &'static str,
    hyphenator: fn() -> Hyphenator,
    /// Generous committed-file ceiling. German's TeX set is ~272 KiB of
    /// tokens; the others stay well under 100 KiB. Ceilings are documented
    /// so a silent upstream dump cannot balloon the binary unnoticed.
    max_bytes: usize,
}

const LANGS: &[LangSpec] = &[
    LangSpec {
        lang: HyphenLang::German,
        stem: "hyph-de-1996",
        hyphenator: Hyphenator::german,
        max_bytes: 320_000,
    },
    LangSpec {
        lang: HyphenLang::French,
        stem: "hyph-fr",
        hyphenator: Hyphenator::french,
        max_bytes: 20_000,
    },
    LangSpec {
        lang: HyphenLang::Dutch,
        stem: "hyph-nl",
        hyphenator: Hyphenator::dutch,
        max_bytes: 120_000,
    },
    LangSpec {
        lang: HyphenLang::Spanish,
        stem: "hyph-es",
        hyphenator: Hyphenator::spanish,
        max_bytes: 60_000,
    },
];

fn main() -> ExitCode {
    let started = Instant::now();
    let args: Vec<String> = env::args().skip(1).collect();
    let mut src: Option<PathBuf> = None;
    let mut out = PathBuf::from("data");
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--src" => {
                i += 1;
                src = args.get(i).map(PathBuf::from);
            }
            "--out" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    out = PathBuf::from(p);
                }
            }
            "--help" | "-h" => {
                let _ = writeln!(
                    io::stderr(),
                    "gen_hyphen_patterns [--src DIR] [--out DIR]\n  \
                     Compile TeX hyphenation patterns into data/hyph-*.patterns"
                );
                return ExitCode::SUCCESS;
            }
            other => {
                let _ = writeln!(io::stderr(), "error: unknown flag {other} (try --help)");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let wrote = if let Some(src_dir) = src.as_deref() {
        if let Err(err) = compile_from_src(src_dir, &out) {
            let _ = writeln!(io::stderr(), "error: {err}");
            return ExitCode::from(1);
        }
        true
    } else {
        false
    };

    let mut failed = 0u32;
    for spec in LANGS {
        match report_lang(spec, &out, wrote) {
            Ok(()) => {}
            Err(err) => {
                failed += 1;
                let _ = writeln!(
                    io::stderr(),
                    "check={} subject={} outcome=FAIL err={err}",
                    spec.stem,
                    spec.lang.as_str()
                );
            }
        }
    }
    let _ = writeln!(
        io::stderr(),
        "phase=hyphen-compile langs={} failed={failed} elapsed_ms={}",
        LANGS.len(),
        started.elapsed().as_millis()
    );
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn compile_from_src(src: &Path, out: &Path) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;
    for spec in LANGS {
        let tex = src.join("tex").join(format!("{}.tex", spec.stem));
        let pat = src.join("txt").join(format!("{}.pat.txt", spec.stem));
        let tokens = if tex.is_file() {
            let raw =
                fs::read_to_string(&tex).map_err(|e| format!("read {}: {e}", tex.display()))?;
            extract_tex_patterns(&raw)?
        } else if pat.is_file() {
            let raw =
                fs::read_to_string(&pat).map_err(|e| format!("read {}: {e}", pat.display()))?;
            tokenize_pattern_body(&raw)
        } else {
            return Err(format!(
                "missing {} (looked for {} and {})",
                spec.stem,
                tex.display(),
                pat.display()
            ));
        };
        let dest = out.join(format!("{}.patterns", spec.stem));
        let mut body = tokens.join("\n");
        body.push('\n');
        fs::write(&dest, body.as_bytes()).map_err(|e| format!("write {}: {e}", dest.display()))?;
        let _ = writeln!(
            io::stderr(),
            "phase=write lang={} path={} tokens={} bytes={}",
            spec.lang.as_str(),
            dest.display(),
            tokens.len(),
            body.len()
        );
    }
    Ok(())
}

fn report_lang(spec: &LangSpec, out: &Path, wrote: bool) -> Result<(), String> {
    let path = out.join(format!("{}.patterns", spec.stem));
    let readme = out.join(format!("{}.README.md", spec.stem));
    let bytes = fs::metadata(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len() as usize;
    let text = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let token_count = text.split_ascii_whitespace().count();
    let hyphenator = (spec.hyphenator)();
    let live_count = hyphenator.encoded_pattern_count();
    let has_license = readme.is_file()
        && fs::read_to_string(&readme)
            .map(|s| s.contains("licence") || s.contains("license") || s.contains("MIT"))
            .unwrap_or(false);

    let size_ok = bytes <= spec.max_bytes;
    // After a write the process still sees the *compiled-in* pattern set, so
    // skip the live-count equality check in that mode.
    let count_ok = wrote || live_count == token_count;
    let outcome = if size_ok && count_ok && has_license {
        "PASS"
    } else {
        "FAIL"
    };
    let _ = writeln!(
        io::stderr(),
        "check=size lang={} tokens={} bytes={} ceiling={} license={} outcome={outcome}",
        spec.lang.as_str(),
        token_count,
        bytes,
        spec.max_bytes,
        has_license
    );
    if !size_ok {
        return Err(format!(
            "{} is {bytes} bytes, ceiling {}",
            spec.stem, spec.max_bytes
        ));
    }
    if !count_ok {
        return Err(format!(
            "{} hyphenator sees {live_count} tokens, file has {token_count}",
            spec.stem
        ));
    }
    if !has_license {
        return Err(format!("{} README missing MIT licence notice", spec.stem));
    }
    Ok(())
}

/// Pull the `\patterns{...}` body out of a hyph-utf8 `.tex` file.
fn extract_tex_patterns(src: &str) -> Result<Vec<String>, String> {
    let Some(start_kw) = src.find("\\patterns") else {
        return Err("no \\patterns in TeX source".into());
    };
    let rest = &src[start_kw..];
    let Some(brace) = rest.find('{') else {
        return Err("\\patterns has no opening brace".into());
    };
    let body = &rest[brace + 1..];
    let mut depth = 1i32;
    let mut end = None;
    for (idx, ch) in body.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| "\\patterns body is unclosed".to_string())?;
    Ok(tokenize_pattern_body(&body[..end]))
}

fn tokenize_pattern_body(body: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for line in body.lines() {
        let trimmed = match line.find('%') {
            Some(idx) => &line[..idx],
            None => line,
        };
        for tok in trimmed.split_ascii_whitespace() {
            if !tok.is_empty() {
                tokens.push(tok.to_string());
            }
        }
    }
    tokens
}
