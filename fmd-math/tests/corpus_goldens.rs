//! The tier-1 corpus parse goldens (franken_manim G0-4).
//!
//! The harvested 3b1b corpus is **private** (licensing, §15.3 of the
//! franken_manim plan): its strings live outside this repository. When the
//! `FMD_MATH_CORPUS` environment variable points at a harvest
//! `tex_corpus.jsonl`, this suite parses all ~9.3k distinct strings and
//! asserts the G0-4 contract on every one:
//!
//! - a string whose constructs are all parse-supported must parse
//!   end-to-end in its recorded mode;
//! - a string containing unsupported vocabulary must fail with a precise
//!   [`fmd_math::MathError::UnsupportedCommand`] naming one of its own
//!   unsupported constructs — never garbage, never a false success.
//!
//! Without the variable the suite is a no-op (vacuously green), so public
//! CI stays honest while the consuming repo runs the real goldens.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{ConstructStatus, construct_status, parse, parse_text};

/// One corpus entry.
struct Entry {
    mode: String,
    text: String,
    constructs: Vec<String>,
    count: u64,
}

#[test]
fn corpus_parse_outcomes_match_the_tiers() {
    let Ok(path) = std::env::var("FMD_MATH_CORPUS") else {
        eprintln!("FMD_MATH_CORPUS not set; corpus goldens skipped");
        return;
    };
    let data = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read corpus at {path}: {e}"));
    #[cfg(feature = "bundled-faces")]
    let engine = match fmd_math::Engine::bundled() {
        Ok(e) => Some(e),
        Err(e) => panic!("bundled faces: {e}"),
    };
    #[cfg(not(feature = "bundled-faces"))]
    let engine: Option<fmd_math::Engine> = None;
    let mut total = 0_u64;
    let mut parsed = 0_u64;
    let mut occurrences = 0_u64;
    let mut occ_parsed = 0_u64;
    let mut laid = 0_u64;
    let mut occ_laid = 0_u64;
    let mut layout_fail_tally: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    for (lineno, line) in data.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry =
            parse_entry(line).unwrap_or_else(|| panic!("line {}: bad corpus JSON", lineno + 1));
        total += 1;
        occurrences += entry.count;
        let expected_ok = entry
            .constructs
            .iter()
            .all(|c| construct_status(c) == ConstructStatus::Supported);
        let result = if entry.mode == "text" {
            parse_text(&entry.text)
        } else {
            parse(&entry.text)
        };
        match (expected_ok, &result) {
            (true, Ok(_)) => {
                parsed += 1;
                occ_parsed += entry.count;
                // The layout plane: parse-covered strings must either lay
                // out or fail with a precise named error (a layout-pending
                // construct or an unmapped character) — never panic, never
                // a structural fault appearing only at layout time.
                if let Some(engine) = engine.as_ref() {
                    let laid_result = if entry.mode == "text" {
                        engine.typeset_text(&entry.text)
                    } else {
                        engine.typeset(&entry.text, fmd_math::Style::Display)
                    };
                    match laid_result {
                        Ok(_) => {
                            laid += 1;
                            occ_laid += entry.count;
                        }
                        Err(fmd_math::MathError::UnsupportedCommand { name, .. }) => {
                            *layout_fail_tally.entry(name).or_insert(0) += entry.count;
                        }
                        Err(fmd_math::MathError::UnmappedChar { ch, .. }) => {
                            *layout_fail_tally
                                .entry(format!("char:U+{:04X}", ch as u32))
                                .or_insert(0) += entry.count;
                        }
                        Err(other) => failures.push(format!(
                            "line {}: parse-covered string failed layout structurally: {other}",
                            lineno + 1
                        )),
                    }
                }
            }
            (true, Err(e)) => failures.push(format!(
                "line {}: expected parse, got: {e}\n  mode={} constructs={:?}",
                lineno + 1,
                entry.mode,
                entry.constructs
            )),
            (false, Ok(_)) => {
                // Stricter than required: if the tier table says a construct
                // is unsupported but we parsed the string anyway, the
                // support map and the parser disagree — fix the map.
                failures.push(format!(
                    "line {}: parsed despite unsupported constructs {:?}",
                    lineno + 1,
                    entry
                        .constructs
                        .iter()
                        .filter(|c| construct_status(c) != ConstructStatus::Supported)
                        .collect::<Vec<_>>()
                ));
            }
            (false, Err(e)) => {
                // The failure must be a named unsupported-construct error
                // for one of the entry's own unsupported constructs.
                if let Some(name) = e.unsupported_construct() {
                    let known = entry.constructs.iter().any(|c| c == name);
                    if known && construct_status(name) != ConstructStatus::Supported {
                        parsed += 0;
                    } else {
                        failures.push(format!(
                            "line {}: failed on `{name}`, which is not an unsupported \
                             construct of the entry ({:?})",
                            lineno + 1,
                            entry.constructs
                        ));
                    }
                } else {
                    failures.push(format!(
                        "line {}: expected a named unsupported-construct error, got: {e}",
                        lineno + 1
                    ));
                }
            }
        }
    }
    assert!(total > 9_000, "corpus looks truncated: {total} entries");
    eprintln!(
        "corpus goldens: {total} strings, {parsed} parsed \
         ({:.3}% unique-string / {:.3}% occurrence-weighted parse coverage)",
        100.0 * parsed as f64 / total as f64,
        100.0 * occ_parsed as f64 / occurrences as f64,
    );
    if engine.is_some() {
        eprintln!(
            "layout coverage: {laid} laid out ({:.3}% unique-string / {:.3}% \
             occurrence-weighted)",
            100.0 * laid as f64 / total as f64,
            100.0 * occ_laid as f64 / occurrences as f64,
        );
        let mut ranked: Vec<(&String, &u64)> = layout_fail_tally.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (name, count) in ranked.iter().take(20) {
            eprintln!("  layout-pending {name}: {count} occurrences");
        }
    }
    assert!(
        failures.is_empty(),
        "{} corpus mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ── A minimal JSON-object reader for the harvest lines (zero-dep) ────────

fn parse_entry(line: &str) -> Option<Entry> {
    let mut mode = None;
    let mut text = None;
    let mut constructs = None;
    let mut count = None;
    let bytes = line.as_bytes();
    let mut i = skip_ws(bytes, 0);
    if bytes.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    loop {
        i = skip_ws(bytes, i);
        match bytes.get(i) {
            Some(b'}') => break,
            Some(b',') => {
                i += 1;
                continue;
            }
            Some(b'"') => {}
            _ => return None,
        }
        let (key, ni) = read_string(line, i)?;
        i = skip_ws(bytes, ni);
        if bytes.get(i) != Some(&b':') {
            return None;
        }
        i = skip_ws(bytes, i + 1);
        match key.as_str() {
            "mode" => {
                let (v, ni) = read_string(line, i)?;
                mode = Some(v);
                i = ni;
            }
            "text" => {
                let (v, ni) = read_string(line, i)?;
                text = Some(v);
                i = ni;
            }
            "constructs" => {
                let (v, ni) = read_string_array(line, i)?;
                constructs = Some(v);
                i = ni;
            }
            "count" => {
                let (v, ni) = read_number(bytes, i)?;
                count = Some(v);
                i = ni;
            }
            _ => {
                i = skip_value(line, i)?;
            }
        }
    }
    Some(Entry {
        mode: mode?,
        text: text?,
        constructs: constructs?,
        count: count?,
    })
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while matches!(bytes.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    i
}

/// Read a JSON string starting at the opening quote; returns the decoded
/// value and the index just past the closing quote.
fn read_string(s: &str, start: usize) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut out = String::new();
    let mut i = start + 1;
    loop {
        let rest = s.get(i..)?;
        let mut chars = rest.char_indices();
        let (_, c) = chars.next()?;
        match c {
            '"' => return Some((out, i + 1)),
            '\\' => {
                let (_, esc) = chars.next()?;
                i += 1 + esc.len_utf8();
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let hex = s.get(i..i + 4)?;
                        let cp = u32::from_str_radix(hex, 16).ok()?;
                        i += 4;
                        if (0xD800..0xDC00).contains(&cp) {
                            // Surrogate pair.
                            if s.get(i..i + 2)? != "\\u" {
                                return None;
                            }
                            let lo = u32::from_str_radix(s.get(i + 2..i + 6)?, 16).ok()?;
                            i += 6;
                            let combined =
                                0x10000 + ((cp - 0xD800) << 10) + (lo.checked_sub(0xDC00)?);
                            out.push(char::from_u32(combined)?);
                        } else {
                            out.push(char::from_u32(cp)?);
                        }
                    }
                    _ => return None,
                }
                continue;
            }
            other => {
                out.push(other);
                i += other.len_utf8();
            }
        }
    }
}

fn read_string_array(s: &str, start: usize) -> Option<(Vec<String>, usize)> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut out = Vec::new();
    let mut i = skip_ws(bytes, start + 1);
    if bytes.get(i) == Some(&b']') {
        return Some((out, i + 1));
    }
    loop {
        let (v, ni) = read_string(s, i)?;
        out.push(v);
        i = skip_ws(bytes, ni);
        match bytes.get(i) {
            Some(b',') => i = skip_ws(bytes, i + 1),
            Some(b']') => return Some((out, i + 1)),
            _ => return None,
        }
    }
}

fn read_number(bytes: &[u8], start: usize) -> Option<(u64, usize)> {
    let mut i = start;
    let mut val: u64 = 0;
    let mut any = false;
    while let Some(d) = bytes.get(i).copied().filter(u8::is_ascii_digit) {
        val = val.checked_mul(10)?.checked_add(u64::from(d - b'0'))?;
        i += 1;
        any = true;
    }
    any.then_some((val, i))
}

/// Skip any JSON value (used for the fields we don't consume).
fn skip_value(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    match bytes.get(start)? {
        b'"' => read_string(s, start).map(|(_, i)| i),
        b'[' => {
            let mut depth = 0_usize;
            let mut i = start;
            loop {
                match bytes.get(i)? {
                    b'"' => {
                        i = read_string(s, i)?.1;
                    }
                    b'[' => {
                        depth += 1;
                        i += 1;
                    }
                    b']' => {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    _ => i += 1,
                }
            }
        }
        _ => {
            let mut i = start;
            while !matches!(bytes.get(i), None | Some(b',' | b'}' | b']')) {
                i += 1;
            }
            Some(i)
        }
    }
}
