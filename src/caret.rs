//! Span caret renderer for stderr diagnostics.
//!
//! Pure: given source bytes, a [`SourceSpan`], and style options, emit a
//! rustc-shaped block (gutter, context, caret run). No filesystem or default
//! environment reads on the `--no-default-features` path. Color/env policy is
//! explicit [`ColorMode`]; [`ColorMode::from_env`] exists only on the CLI
//! feature.

use crate::span::{DiagnosticSeverity, ParseDiagnostic, SourceSpan};

const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const BOLD: &str = "\x1b[1m";
const ELLIPSIS: &str = "…";

/// How ANSI color is decided for a caret block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Color only when the caller says the sink is a TTY.
    Auto,
    /// Always emit ANSI.
    Always,
    /// Never emit ANSI (`NO_COLOR`, `CI`, `TERM=dumb`, `--no-color`).
    Never,
}

impl ColorMode {
    /// Resolve the process environment into a mode.
    ///
    /// Order: `NO_COLOR` (any value) and `TERM=dumb` and `CI` (any value) and
    /// `CLICOLOR=0` force [`Never`]. `CLICOLOR_FORCE` non-empty and not `"0"`
    /// forces [`Always`]. Otherwise [`Auto`].
    #[cfg(feature = "cli")]
    #[must_use]
    pub fn from_env() -> Self {
        fn nonempty(name: &str) -> bool {
            std::env::var_os(name).is_some_and(|v| !v.is_empty())
        }
        if nonempty("NO_COLOR") {
            return Self::Never;
        }
        if std::env::var_os("TERM").is_some_and(|t| t == "dumb") {
            return Self::Never;
        }
        if nonempty("CI") {
            return Self::Never;
        }
        if std::env::var_os("CLICOLOR").is_some_and(|v| v == "0") {
            return Self::Never;
        }
        if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0" && !v.is_empty()) {
            return Self::Always;
        }
        Self::Auto
    }

    /// Whether this mode plus TTY-ness should emit ANSI.
    #[must_use]
    pub fn enabled(self, is_tty: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => is_tty,
        }
    }
}

/// Layout knobs for one caret render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaretStyle {
    /// Emit ANSI colors.
    pub color: bool,
    /// Terminal width in columns. `None` disables elision.
    pub columns: Option<usize>,
    /// Extra source lines above and below the span line.
    pub context_lines: usize,
}

impl Default for CaretStyle {
    fn default() -> Self {
        Self {
            color: false,
            columns: None,
            context_lines: 1,
        }
    }
}

/// Render one diagnostic as a caret block.
#[must_use]
pub fn render_caret(
    source: &str,
    span: SourceSpan,
    file: Option<&str>,
    message: &str,
    severity: DiagnosticSeverity,
    style: CaretStyle,
) -> String {
    let lines = line_ranges(source);
    let (line_idx, col) = locate(source, &lines, span.start);
    let line_no = line_idx.saturating_add(1);
    let mut out = String::new();
    write_header(
        &mut out,
        file,
        line_no,
        col.saturating_add(1),
        severity,
        message,
        style.color,
    );
    out.push('\n');
    let ctx = style.context_lines;
    let start_i = line_idx.saturating_sub(ctx);
    let end_i = (line_idx.saturating_add(ctx) + 1).min(lines.len().max(1));
    let gutter = digits(end_i.max(1));
    if lines.is_empty() {
        write_source_line(&mut out, 1, "", gutter, style);
        write_caret_line(&mut out, 0, 1, gutter, severity, style);
        return out;
    }
    for (i, &(a, b)) in lines.iter().enumerate().take(end_i).skip(start_i) {
        let text = source.get(a..b).unwrap_or("");
        write_source_line(&mut out, i + 1, text, gutter, style);
        if i == line_idx {
            let caret_cols = caret_width(text, span, a);
            let start_col = byte_to_col(text, span.start.saturating_sub(a));
            write_caret_line(&mut out, start_col, caret_cols, gutter, severity, style);
        }
    }
    out
}

/// Render a [`ParseDiagnostic`] against `source`.
#[must_use]
pub fn render_parse_diagnostic(
    diag: &ParseDiagnostic,
    source: &str,
    file: Option<&str>,
    style: CaretStyle,
) -> String {
    render_caret(source, diag.span, file, &diag.message, diag.severity, style)
}

/// Display column (0-based) of `byte_off` inside `line`.
#[must_use]
pub fn byte_to_col(line: &str, byte_off: usize) -> usize {
    let mut col = 0usize;
    for (i, ch) in line.char_indices() {
        if i >= byte_off {
            return col;
        }
        col = col.saturating_add(display_width(ch));
    }
    col
}

/// Terminal display width of `ch`: 0 combining, 2 East-Asian/fullwidth, else 1.
#[must_use]
pub fn display_width(ch: char) -> usize {
    if is_combining(ch) {
        0
    } else if is_wide(ch) {
        2
    } else {
        1
    }
}

fn is_combining(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x07EB..=0x07F3
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
    )
}

fn is_wide(ch: char) -> bool {
    let c = ch as u32;
    (0x1100..=0x115F).contains(&c)
        || (0x2329..=0x232A).contains(&c)
        || ((0x2E80..=0xA4CF).contains(&c) && c != 0x303F)
        || (0xAC00..=0xD7A3).contains(&c)
        || (0xF900..=0xFAFF).contains(&c)
        || (0xFE10..=0xFE19).contains(&c)
        || (0xFE30..=0xFE6F).contains(&c)
        || (0xFF00..=0xFF60).contains(&c)
        || (0xFFE0..=0xFFE6).contains(&c)
}

fn line_ranges(source: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let b = source.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'\n' {
            let mut end = i;
            if end > start && b[end - 1] == b'\r' {
                end -= 1;
            }
            out.push((start, end));
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < b.len() || out.is_empty() {
        out.push((start, b.len()));
    }
    out
}

fn locate(source: &str, lines: &[(usize, usize)], byte: usize) -> (usize, usize) {
    let byte = byte.min(source.len());
    if lines.is_empty() {
        return (0, 0);
    }
    for (i, &(a, b)) in lines.iter().enumerate() {
        let last = i + 1 == lines.len();
        if byte < b || (byte == b && last) || (byte >= a && byte < b) || (byte == b && !last) {
            let rel = byte.saturating_sub(a).min(b.saturating_sub(a));
            let text = source.get(a..b).unwrap_or("");
            return (i, byte_to_col(text, rel));
        }
    }
    let (a, b) = lines[lines.len() - 1];
    let text = source.get(a..b).unwrap_or("");
    (lines.len() - 1, byte_to_col(text, text.len()))
}

fn caret_width(line: &str, span: SourceSpan, line_start: usize) -> usize {
    let lo = span.start.max(line_start);
    let hi = span.end.min(line_start.saturating_add(line.len()));
    if hi <= lo {
        return 1;
    }
    let start_col = byte_to_col(line, lo - line_start);
    let end_col = byte_to_col(line, hi - line_start);
    end_col.saturating_sub(start_col).max(1)
}

fn digits(n: usize) -> usize {
    if n == 0 { 1 } else { n.to_string().len() }
}

fn write_header(
    out: &mut String,
    file: Option<&str>,
    line: usize,
    col: usize,
    severity: DiagnosticSeverity,
    message: &str,
    color: bool,
) {
    if let Some(f) = file {
        out.push_str(f);
        out.push(':');
    }
    push_usize(out, line);
    out.push(':');
    push_usize(out, col);
    out.push_str(": ");
    let label = match severity {
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    };
    if color {
        out.push_str(BOLD);
        out.push_str(severity_color(severity));
        out.push_str(label);
        out.push_str(RESET);
    } else {
        out.push_str(label);
    }
    out.push_str(": ");
    out.push_str(message);
}

fn severity_color(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Warning => YELLOW,
        DiagnosticSeverity::Error => RED,
    }
}

fn write_source_line(
    out: &mut String,
    line_no: usize,
    text: &str,
    gutter: usize,
    style: CaretStyle,
) {
    pad_gutter(out, line_no, gutter);
    if style.color {
        out.push_str(CYAN);
        out.push_str(" | ");
        out.push_str(RESET);
    } else {
        out.push_str(" | ");
    }
    let shown = elide_line(text, 0, style.columns, gutter);
    out.push_str(&shown);
    out.push('\n');
}

fn write_caret_line(
    out: &mut String,
    start_col: usize,
    width: usize,
    gutter: usize,
    severity: DiagnosticSeverity,
    style: CaretStyle,
) {
    for _ in 0..gutter {
        out.push(' ');
    }
    if style.color {
        out.push_str(CYAN);
        out.push_str(" | ");
        out.push_str(RESET);
        out.push_str(severity_color(severity));
    } else {
        out.push_str(" | ");
    }
    let (skip, take, prefix) = elide_window(start_col, width, style.columns, gutter);
    if prefix {
        out.push_str(ELLIPSIS);
    }
    for _ in 0..skip {
        out.push(' ');
    }
    for _ in 0..take {
        out.push('^');
    }
    if style.color {
        out.push_str(RESET);
    }
    out.push('\n');
}

fn elide_line(text: &str, _caret_col: usize, columns: Option<usize>, gutter: usize) -> String {
    let Some(cols) = columns else {
        return text.to_owned();
    };
    let budget = cols.saturating_sub(gutter.saturating_add(3)).max(8);
    let width: usize = text.chars().map(display_width).sum();
    if width <= budget {
        return text.to_owned();
    }
    // Keep the left of the line; suffix ellipsis. Caret alignment uses the
    // same window via [`elide_window`].
    let mut acc = 0usize;
    let mut out = String::new();
    let ell_w = display_width('…');
    for ch in text.chars() {
        let w = display_width(ch);
        if acc.saturating_add(w).saturating_add(ell_w) > budget {
            out.push('…');
            break;
        }
        out.push(ch);
        acc = acc.saturating_add(w);
    }
    out
}

fn elide_window(
    start_col: usize,
    width: usize,
    columns: Option<usize>,
    gutter: usize,
) -> (usize, usize, bool) {
    let Some(cols) = columns else {
        return (start_col, width, false);
    };
    let budget = cols.saturating_sub(gutter.saturating_add(3)).max(8);
    let ell_w = 1usize;
    if start_col.saturating_add(width) + ell_w <= budget {
        return (start_col, width, false);
    }
    // Drop left columns so the caret stays visible; signal prefix ellipsis.
    let keep = width.min(budget.saturating_sub(ell_w).max(1));
    (0, keep, start_col > 0)
}

fn pad_gutter(out: &mut String, line_no: usize, gutter: usize) {
    let n = line_no.to_string();
    for _ in 0..gutter.saturating_sub(n.len()) {
        out.push(' ');
    }
    out.push_str(&n);
}

fn push_usize(out: &mut String, n: usize) {
    out.push_str(&n.to_string());
}

#[cfg(test)]
mod tests {
    use super::{byte_to_col, display_width};

    #[test]
    fn combining_mark_is_zero_width() {
        assert_eq!(display_width('\u{0301}'), 0);
        assert_eq!(byte_to_col("e\u{0301}x", 0), 0);
        // 'e' (1 byte) + combining acute (2 bytes) occupy one column.
        assert_eq!(byte_to_col("e\u{0301}x", "e\u{0301}".len()), 1);
    }

    #[test]
    fn cjk_is_two_columns() {
        assert_eq!(display_width('中'), 2);
        assert_eq!(byte_to_col("a中b", 0), 0);
        assert_eq!(byte_to_col("a中b", 1), 1);
        assert_eq!(byte_to_col("a中b", "a中".len()), 3);
    }
}
