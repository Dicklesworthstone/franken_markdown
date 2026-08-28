//! Corpus glyph-coverage auditor (`fmd doctor fonts --corpus`).
//!
//! Classifies Markdown codepoints against every bundled face plus the Noto
//! Sans Math fallback. JSON is hand-built (no serde) and key order is
//! deterministic. Directory walking matches batch: depth-bounded, no symlink
//! follow, case-insensitive `.md`/`.markdown`, sorted paths.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::text::{Font, bundled::ALL_FACES};

const MAX_COLLECT_DIR_DEPTH: usize = 128;
const TOP_UNCOVERED: usize = 20;
const FALLBACK_FACE: &str = "noto-sans-math-symbols";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Script {
    Arabic,
    Cyrillic,
    Devanagari,
    Greek,
    Han,
    Hangul,
    Hebrew,
    Hiragana,
    Katakana,
    Latin,
    Other,
    Symbols,
    Thai,
}

impl Script {
    fn as_str(self) -> &'static str {
        match self {
            Self::Arabic => "arabic",
            Self::Cyrillic => "cyrillic",
            Self::Devanagari => "devanagari",
            Self::Greek => "greek",
            Self::Han => "han",
            Self::Hangul => "hangul",
            Self::Hebrew => "hebrew",
            Self::Hiragana => "hiragana",
            Self::Katakana => "katakana",
            Self::Latin => "latin",
            Self::Other => "other",
            Self::Symbols => "symbols",
            Self::Thai => "thai",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Covered,
    FallbackCovered,
    Uncovered,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Covered => "covered",
            Self::FallbackCovered => "fallback-covered",
            Self::Uncovered => "uncovered",
        }
    }

    fn worse(self, other: Self) -> Self {
        use Verdict::*;
        match (self, other) {
            (Uncovered, _) | (_, Uncovered) => Uncovered,
            (FallbackCovered, _) | (_, FallbackCovered) => FallbackCovered,
            _ => Covered,
        }
    }
}

#[derive(Clone, Copy)]
struct Block {
    name: &'static str,
    start: u32,
    end: u32,
}

const BLOCKS: &[Block] = &[
    Block {
        name: "Basic Latin",
        start: 0x0000,
        end: 0x007F,
    },
    Block {
        name: "Latin-1 Supplement",
        start: 0x0080,
        end: 0x00FF,
    },
    Block {
        name: "Latin Extended-A",
        start: 0x0100,
        end: 0x017F,
    },
    Block {
        name: "Latin Extended-B",
        start: 0x0180,
        end: 0x024F,
    },
    Block {
        name: "Greek and Coptic",
        start: 0x0370,
        end: 0x03FF,
    },
    Block {
        name: "Cyrillic",
        start: 0x0400,
        end: 0x04FF,
    },
    Block {
        name: "Hebrew",
        start: 0x0590,
        end: 0x05FF,
    },
    Block {
        name: "Arabic",
        start: 0x0600,
        end: 0x06FF,
    },
    Block {
        name: "Devanagari",
        start: 0x0900,
        end: 0x097F,
    },
    Block {
        name: "Thai",
        start: 0x0E00,
        end: 0x0E7F,
    },
    Block {
        name: "Hiragana",
        start: 0x3040,
        end: 0x309F,
    },
    Block {
        name: "Katakana",
        start: 0x30A0,
        end: 0x30FF,
    },
    Block {
        name: "CJK Unified Ideographs",
        start: 0x4E00,
        end: 0x9FFF,
    },
    Block {
        name: "Hangul Syllables",
        start: 0xAC00,
        end: 0xD7AF,
    },
    Block {
        name: "General Punctuation",
        start: 0x2000,
        end: 0x206F,
    },
    Block {
        name: "Arrows",
        start: 0x2190,
        end: 0x21FF,
    },
    Block {
        name: "Mathematical Operators",
        start: 0x2200,
        end: 0x22FF,
    },
    Block {
        name: "Miscellaneous Symbols",
        start: 0x2600,
        end: 0x26FF,
    },
    Block {
        name: "CJK Compatibility Ideographs",
        start: 0xF900,
        end: 0xFAFF,
    },
];

fn script_of(cp: u32) -> Script {
    match cp {
        0x0000..=0x024F | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F => Script::Latin,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        0x0400..=0x052F => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F => Script::Arabic,
        0x0900..=0x097F => Script::Devanagari,
        0x0E00..=0x0E7F => Script::Thai,
        0x3040..=0x309F => Script::Hiragana,
        0x30A0..=0x30FF => Script::Katakana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2A6DF => Script::Han,
        0x1100..=0x11FF | 0xAC00..=0xD7AF => Script::Hangul,
        0x2000..=0x27FF | 0x1D400..=0x1D7FF => Script::Symbols,
        _ => Script::Other,
    }
}

fn block_of(cp: u32) -> Block {
    for block in BLOCKS {
        if cp >= block.start && cp <= block.end {
            return *block;
        }
    }
    Block {
        name: "Other",
        start: cp,
        end: cp,
    }
}

fn skip_char(ch: char) -> bool {
    ch.is_whitespace() || ch.is_control()
}

struct ParsedFace {
    font: Font,
    fallback: bool,
}

fn faces() -> &'static [ParsedFace] {
    static FACES: OnceLock<Vec<ParsedFace>> = OnceLock::new();
    FACES.get_or_init(|| {
        ALL_FACES
            .iter()
            .filter_map(|(name, bytes)| {
                Font::parse(bytes.to_vec()).ok().map(|font| ParsedFace {
                    font,
                    fallback: *name == FALLBACK_FACE,
                })
            })
            .collect()
    })
}

fn classify_char(ch: char) -> Verdict {
    let mut fallback = false;
    for face in faces() {
        if face.font.glyph_index(ch) == 0 {
            continue;
        }
        if face.fallback {
            fallback = true;
        } else {
            return Verdict::Covered;
        }
    }
    if fallback {
        Verdict::FallbackCovered
    } else {
        Verdict::Uncovered
    }
}

#[derive(Clone)]
struct Hit {
    count: u64,
    sample: String,
}

#[derive(Default)]
struct ScriptAcc {
    codepoints: u64,
    covered: u64,
    fallback_covered: u64,
    uncovered: u64,
}

struct BlockAcc {
    name: &'static str,
    start: u32,
    end: u32,
    codepoints: u64,
    verdict: Verdict,
}

/// Stable report produced by a corpus scan.
pub struct CoverageReport {
    files: usize,
    codepoints: u64,
    scripts: BTreeMap<Script, ScriptAcc>,
    blocks: BTreeMap<u32, BlockAcc>,
    uncovered: BTreeMap<char, Hit>,
}

impl CoverageReport {
    fn new() -> Self {
        Self {
            files: 0,
            codepoints: 0,
            scripts: BTreeMap::new(),
            blocks: BTreeMap::new(),
            uncovered: BTreeMap::new(),
        }
    }

    fn ingest(&mut self, text: &str, path: &str) {
        self.files += 1;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_no = line_idx + 1;
            let line = line.strip_suffix('\r').unwrap_or(line);
            for ch in line.chars() {
                if skip_char(ch) {
                    continue;
                }
                self.codepoints += 1;
                let verdict = classify_char(ch);
                let script = script_of(ch as u32);
                let acc = self.scripts.entry(script).or_default();
                acc.codepoints += 1;
                match verdict {
                    Verdict::Covered => acc.covered += 1,
                    Verdict::FallbackCovered => acc.fallback_covered += 1,
                    Verdict::Uncovered => {
                        acc.uncovered += 1;
                        let sample = format!("{path}:{line_no}");
                        self.uncovered
                            .entry(ch)
                            .and_modify(|hit| hit.count += 1)
                            .or_insert(Hit { count: 1, sample });
                    }
                }
                let block = block_of(ch as u32);
                self.blocks
                    .entry(block.start)
                    .and_modify(|acc| {
                        acc.codepoints += 1;
                        acc.verdict = acc.verdict.worse(verdict);
                    })
                    .or_insert(BlockAcc {
                        name: block.name,
                        start: block.start,
                        end: block.end,
                        codepoints: 1,
                        verdict,
                    });
            }
        }
    }

    fn has_gaps(&self) -> bool {
        self.uncovered.values().any(|h| h.count > 0)
    }

    fn hints(&self) -> Vec<String> {
        let mut cps: Vec<u32> = self.uncovered.keys().map(|c| *c as u32).collect();
        cps.sort_unstable();
        let mut out = Vec::new();
        let mut i = 0;
        while i < cps.len() {
            let start = cps[i];
            let mut end = start;
            i += 1;
            while i < cps.len() && cps[i] == end + 1 {
                end = cps[i];
                i += 1;
            }
            out.push(format!(
                "add {}-{} to CURATED_RANGES",
                fmt_cp(start),
                fmt_cp(end)
            ));
        }
        out
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"ok\":true,\"tool\":\"fmd\",\"command\":\"doctor fonts\",");
        out.push_str(&format!("\"files\":{},", self.files));
        out.push_str(&format!("\"codepoints\":{},", self.codepoints));
        out.push_str(&format!(
            "\"gaps\":{},",
            if self.has_gaps() { "true" } else { "false" }
        ));
        out.push_str("\"scripts\":[");
        for (i, (script, acc)) in self.scripts.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"script\":\"{}\",\"codepoints\":{},\"covered\":{},\"fallback_covered\":{},\"uncovered\":{},\"coverage_percent\":{}}}",
                script.as_str(),
                acc.codepoints,
                acc.covered,
                acc.fallback_covered,
                acc.uncovered,
                percent(acc.covered + acc.fallback_covered, acc.codepoints)
            ));
        }
        out.push_str("],\"ranges\":[");
        for (i, acc) in self.blocks.values().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"block\":\"{}\",\"first\":\"{}\",\"last\":\"{}\",\"verdict\":\"{}\",\"codepoints\":{}}}",
                json_escape(acc.name),
                fmt_cp(acc.start),
                fmt_cp(acc.end),
                acc.verdict.as_str(),
                acc.codepoints
            ));
        }
        out.push_str("],\"uncovered\":[");
        let mut hits: Vec<(char, &Hit)> = self.uncovered.iter().map(|(c, h)| (*c, h)).collect();
        hits.sort_by(|a, b| {
            b.1.count
                .cmp(&a.1.count)
                .then_with(|| (a.0 as u32).cmp(&(b.0 as u32)))
        });
        hits.truncate(TOP_UNCOVERED);
        for (i, (ch, hit)) in hits.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"cp\":\"{}\",\"char\":\"{}\",\"count\":{},\"sample\":\"{}\"}}",
                fmt_cp(*ch as u32),
                json_escape(&ch.to_string()),
                hit.count,
                json_escape(&hit.sample)
            ));
        }
        out.push_str("],\"hints\":[");
        let hints = self.hints();
        for (i, hint) in hints.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{}\"", json_escape(hint)));
        }
        out.push_str("]}");
        out
    }

    pub fn to_human(&self) -> String {
        let mut lines = Vec::new();
        lines.push("fmd doctor fonts".to_string());
        lines.push(format!("  files: {}", self.files));
        lines.push(format!("  codepoints: {}", self.codepoints));
        lines.push(format!(
            "  gaps: {}",
            if self.has_gaps() { "yes" } else { "no" }
        ));
        for (script, acc) in &self.scripts {
            lines.push(format!(
                "  {}: {}% ({}/{} covered, {} fallback, {} uncovered)",
                script.as_str(),
                percent(acc.covered + acc.fallback_covered, acc.codepoints),
                acc.covered,
                acc.codepoints,
                acc.fallback_covered,
                acc.uncovered
            ));
        }
        for hint in self.hints() {
            lines.push(format!("  hint: {hint}"));
        }
        lines.join("\n")
    }
}

fn percent(n: u64, d: u64) -> String {
    if d == 0 {
        return "100.0".to_string();
    }
    let tenths = n.saturating_mul(1000).saturating_add(d / 2) / d;
    format!("{}.{}", tenths / 10, tenths % 10)
}

fn fmt_cp(cp: u32) -> String {
    if cp <= 0xFFFF {
        format!("U+{cp:04X}")
    } else {
        format!("U+{cp:06X}")
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

/// Expand a corpus path the same way batch does: recurse directories,
/// skip symlinks, depth-cap, sort, dedup.
pub fn expand_corpus(root: &Path) -> Result<Vec<PathBuf>, String> {
    let meta =
        std::fs::metadata(root).map_err(|e| format!("cannot read {}: {e}", root.display()))?;
    let mut out = Vec::new();
    if meta.is_dir() {
        collect_dir(root, &mut out, 0)?;
    } else if is_markdown(root) {
        out.push(root.to_path_buf());
    } else {
        return Err(format!(
            "{} is not a Markdown file or directory of Markdown files",
            root.display()
        ));
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn collect_dir(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<(), String> {
    if depth >= MAX_COLLECT_DIR_DEPTH {
        return Err(format!(
            "directory nesting exceeds maximum depth {MAX_COLLECT_DIR_DEPTH}; skipped {}",
            dir.display()
        ));
    }
    let read_dir = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read directory {}: {e}", dir.display()))?;
    let mut entries: Vec<PathBuf> = read_dir.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for entry in entries {
        if entry.is_symlink() {
            continue;
        }
        if entry.is_dir() {
            collect_dir(&entry, out, depth + 1)?;
        } else if is_markdown(&entry) {
            out.push(entry);
        }
    }
    Ok(())
}

/// Scan expanded Markdown files into a coverage report.
pub fn audit_files(paths: &[PathBuf]) -> Result<CoverageReport, String> {
    let mut report = CoverageReport::new();
    for path in paths {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let sample = path.to_string_lossy();
        report.ingest(&text, &sample);
    }
    Ok(report)
}

/// True when the corpus has at least one uncovered graphic codepoint.
pub fn report_has_gaps(report: &CoverageReport) -> bool {
    report.has_gaps()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn log(id: &str, subject: &str, outcome: &str) {
        eprintln!("check={id} subject={subject} outcome={outcome}");
    }

    fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
        if ok {
            log(id, subject, "PASS");
        } else {
            log(id, subject, "FAIL");
            panic!("{id} `{subject}`: {detail}");
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "fmd-doctor-fonts-{}-{}-{name}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn latin_hello_is_fully_covered() {
        let mut report = CoverageReport::new();
        report.ingest("Hello", "a.md:n/a");
        let latin = report.scripts.get(&Script::Latin).expect("latin");
        assert_ok(
            "latin-covered",
            "Hello",
            latin.uncovered == 0 && latin.covered == 5,
            &format!("covered={} uncovered={}", latin.covered, latin.uncovered),
        );
        assert_ok(
            "latin-no-gaps",
            "Hello",
            !report.has_gaps(),
            "latin-only corpus must be gap-free",
        );
    }

    #[test]
    fn han_is_uncovered_on_bundled_faces() {
        let mut report = CoverageReport::new();
        report.ingest("你好", "cjk.md");
        let han = report.scripts.get(&Script::Han).expect("han");
        assert_ok(
            "han-uncovered",
            "你好",
            han.uncovered == 2 && han.covered == 0,
            &format!("covered={} uncovered={}", han.covered, han.uncovered),
        );
        let json = report.to_json();
        assert_ok(
            "han-json-gap",
            "gaps",
            json.contains("\"gaps\":true") && json.contains("\"script\":\"han\""),
            &json,
        );
        assert_ok(
            "han-hint",
            "CURATED_RANGES",
            report.hints().iter().any(|h| h.contains("CURATED_RANGES")),
            &report.hints().join(";"),
        );
    }

    #[test]
    fn json_is_byte_stable_across_calls() {
        let mut a = CoverageReport::new();
        a.ingest("Hello 你好", "z.md");
        let mut b = CoverageReport::new();
        b.ingest("Hello 你好", "z.md");
        let ja = a.to_json();
        let jb = b.to_json();
        assert_ok(
            "json-stable",
            "digest",
            ja == jb,
            "to_json must be deterministic",
        );
        assert_ok(
            "json-shape",
            "keys",
            ja.contains("\"scripts\":")
                && ja.contains("\"ranges\":")
                && ja.contains("\"uncovered\":"),
            &ja,
        );
    }

    #[test]
    fn expand_corpus_sorts_and_skips_non_markdown() {
        let dir = tmp("order");
        fs::write(dir.join("b.md"), "b").unwrap();
        fs::write(dir.join("a.md"), "a").unwrap();
        fs::write(dir.join("skip.txt"), "nope").unwrap();
        let nested = dir.join("sub");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("c.MD"), "c").unwrap();
        let got = expand_corpus(&dir).unwrap();
        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_ok(
            "walk-order",
            "a.md,b.md,c.MD",
            names == ["a.md", "b.md", "c.MD"],
            &names.join(","),
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_dir_is_gap_free() {
        let dir = tmp("empty");
        let paths = expand_corpus(&dir).unwrap();
        let report = audit_files(&paths).unwrap();
        assert_ok(
            "empty-files",
            "0",
            report.files == 0 && !report.has_gaps(),
            &format!("files={}", report.files),
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
