//! Scalar byte/line scanners: small, safe, allocation-free, and portable.
//!
//! [`find_html_escape`] backs the HTML emitter's `escape_text`/`escape_attr`
//! bulk-copy escaping in production. The remaining line classifiers
//! ([`scan_markdown_line`], [`scan_table_or_fence_candidate`]) are the
//! behavioral reference a future, explicitly-approved SIMD acceleration island
//! must match exactly (see AGENTS.md on the SIMD/font-parsing island policy).
//! Either way these routines define exact, testable behavior.

/// Scalar, allocation-free Markdown line classification.
///
/// This is the reference oracle for future SIMD scanners: every flag is
/// conservative. A flag may be true when the expensive parser detector later
/// says "not actually this construct", but it must not be false when that
/// detector could succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParserLineScan {
    /// The line contains an ASCII pipe byte.
    pub contains_pipe: bool,
    /// The line contains an ASCII backtick byte.
    pub contains_backtick: bool,
    /// The line contains an ASCII tilde byte.
    pub contains_tilde: bool,
    /// The line may start an ATX heading.
    pub maybe_heading_marker: bool,
    /// The line may start a list item.
    pub maybe_list_marker: bool,
    /// The line may start or contain an HTML/autolink opener.
    pub maybe_html: bool,
    /// The line may be a link reference definition.
    pub maybe_reference: bool,
    /// The line may be a pipe-table delimiter row.
    pub maybe_table_delimiter: bool,
    /// The line may contain an inline autolink or bare URL.
    pub maybe_autolink: bool,
    /// The line may start a fenced code block.
    pub maybe_fence: bool,
    /// The line may start a blockquote.
    pub maybe_blockquote: bool,
    /// The line may be a thematic break.
    pub maybe_thematic_break: bool,
    /// The line may be a setext heading underline.
    pub maybe_setext_underline: bool,
    /// Byte offset of the first parser-significant ASCII byte.
    pub first_special_byte: Option<usize>,
}

/// Result of a byte-level scanner pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ByteCandidateScan {
    /// First byte that can start a Markdown construct.
    pub first_markdown_special: Option<usize>,
    /// First byte that must be escaped in HTML text/attribute contexts.
    pub first_html_escape: Option<usize>,
    /// First byte that must be escaped in PDF literal strings.
    pub first_pdf_escape: Option<usize>,
}

/// ASCII whitespace classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WhitespaceScan {
    /// First ASCII whitespace byte.
    pub first_ascii_whitespace: Option<usize>,
    /// The input contains at least one ASCII space.
    pub contains_space: bool,
    /// The input contains at least one tab.
    pub contains_tab: bool,
    /// The input contains at least one carriage return.
    pub contains_cr: bool,
    /// The input contains at least one line feed.
    pub contains_lf: bool,
    /// True only when every byte is ASCII whitespace and the input is non-empty.
    pub all_ascii_whitespace: bool,
}

/// Candidate flags needed by table/fence scanners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TableFenceCandidateScan {
    /// The line contains a pipe byte.
    pub contains_pipe: bool,
    /// The line contains a backtick byte.
    pub contains_backtick: bool,
    /// The line contains a tilde byte.
    pub contains_tilde: bool,
    /// The line may be a GFM table delimiter.
    pub maybe_table_delimiter: bool,
    /// The line may start a fenced code block.
    pub maybe_fence: bool,
}

/// Find the first byte that could matter to Markdown parsing.
#[must_use]
pub fn find_any_special_byte(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|&byte| is_markdown_special_byte(byte))
}

/// Find the first byte that must be escaped in HTML text/attribute output.
#[must_use]
pub fn find_html_escape(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|&byte| is_html_escape_byte(byte))
}

/// Find the first byte that must be escaped in a PDF literal string.
#[must_use]
pub fn find_pdf_escape(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|&byte| is_pdf_escape_byte(byte))
}

/// Run the shared byte candidate scanners in one scalar pass.
#[must_use]
pub fn scan_byte_candidates(bytes: &[u8]) -> ByteCandidateScan {
    let mut scan = ByteCandidateScan::default();
    for (idx, &byte) in bytes.iter().enumerate() {
        if scan.first_markdown_special.is_none() && is_markdown_special_byte(byte) {
            scan.first_markdown_special = Some(idx);
        }
        if scan.first_html_escape.is_none() && is_html_escape_byte(byte) {
            scan.first_html_escape = Some(idx);
        }
        if scan.first_pdf_escape.is_none() && is_pdf_escape_byte(byte) {
            scan.first_pdf_escape = Some(idx);
        }
        if scan.first_markdown_special.is_some()
            && scan.first_html_escape.is_some()
            && scan.first_pdf_escape.is_some()
        {
            break;
        }
    }
    scan
}

/// Classify ASCII whitespace in one scalar pass.
#[must_use]
pub fn classify_ascii_whitespace(bytes: &[u8]) -> WhitespaceScan {
    let mut scan = WhitespaceScan {
        all_ascii_whitespace: !bytes.is_empty(),
        ..WhitespaceScan::default()
    };
    for (idx, &byte) in bytes.iter().enumerate() {
        match byte {
            b' ' => {
                scan.contains_space = true;
                scan.first_ascii_whitespace.get_or_insert(idx);
            }
            b'\t' => {
                scan.contains_tab = true;
                scan.first_ascii_whitespace.get_or_insert(idx);
            }
            b'\r' => {
                scan.contains_cr = true;
                scan.first_ascii_whitespace.get_or_insert(idx);
            }
            b'\n' => {
                scan.contains_lf = true;
                scan.first_ascii_whitespace.get_or_insert(idx);
            }
            _ => scan.all_ascii_whitespace = false,
        }
    }
    scan
}

/// Classify one Markdown source line without allocation.
#[must_use]
pub fn scan_markdown_line(line: &str) -> ParserLineScan {
    let bytes = line.as_bytes();
    let mut contains_pipe = false;
    let mut contains_backtick = false;
    let mut contains_tilde = false;
    let mut contains_dash = false;
    let mut contains_colon = false;
    let mut contains_open_angle = false;
    let mut contains_at = false;
    let mut contains_open_bracket = false;
    let mut has_reference_colon = false;
    let mut maybe_url_prefix = false;
    let mut first_special_byte = None;
    let mut previous = 0u8;

    for (idx, &byte) in bytes.iter().enumerate() {
        match byte {
            b'|' => contains_pipe = true,
            b'`' => contains_backtick = true,
            b'~' => contains_tilde = true,
            b'-' => contains_dash = true,
            b':' => contains_colon = true,
            b'<' => contains_open_angle = true,
            b'@' => contains_at = true,
            b'[' => contains_open_bracket = true,
            _ => {}
        }
        if previous == b']' && byte == b':' {
            has_reference_colon = true;
        }
        if !maybe_url_prefix && maybe_url_prefix_at(bytes, idx, byte) {
            maybe_url_prefix = true;
        }
        if first_special_byte.is_none() && is_markdown_special_byte(byte) {
            first_special_byte = Some(idx);
        }
        previous = byte;
    }

    let leading_spaces = leading_spaces_bytes(bytes);
    let first = bytes.get(leading_spaces).copied();
    let indented_as_block = leading_spaces <= 3;
    let marker_tail = bytes.get(leading_spaces..).unwrap_or(&[]);
    let maybe_list_marker = indented_as_block
        && (starts_unordered_list_marker(marker_tail) || starts_ordered_list_marker(marker_tail));
    let first_special_byte =
        first_special_byte.or_else(|| maybe_list_marker.then_some(leading_spaces));

    ParserLineScan {
        contains_pipe,
        contains_backtick,
        contains_tilde,
        maybe_heading_marker: indented_as_block && first == Some(b'#'),
        maybe_list_marker,
        maybe_html: contains_open_angle,
        maybe_reference: leading_spaces <= 3 && has_reference_colon && contains_open_bracket,
        maybe_table_delimiter: contains_pipe || contains_dash || contains_colon,
        maybe_autolink: contains_open_angle || contains_at || maybe_url_prefix,
        maybe_fence: indented_as_block && matches!(first, Some(b'`' | b'~')),
        maybe_blockquote: indented_as_block && line.trim_start().as_bytes().first() == Some(&b'>'),
        maybe_thematic_break: indented_as_block && matches!(first, Some(b'-' | b'*' | b'_')),
        maybe_setext_underline: indented_as_block && matches!(first, Some(b'=' | b'-')),
        first_special_byte,
    }
}

/// Classify table/fence candidates in one scalar pass.
#[must_use]
pub fn scan_table_or_fence_candidate(line: &str) -> TableFenceCandidateScan {
    let line_scan = scan_markdown_line(line);
    TableFenceCandidateScan {
        contains_pipe: line_scan.contains_pipe,
        contains_backtick: line_scan.contains_backtick,
        contains_tilde: line_scan.contains_tilde,
        maybe_table_delimiter: line_scan.maybe_table_delimiter,
        maybe_fence: line_scan.maybe_fence,
    }
}

const fn is_markdown_special_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'\\'
            | b'\n'
            | b'\r'
            | b'\t'
            | b'#'
            | b'-'
            | b'='
            | b'*'
            | b'+' // `+` is a CommonMark bullet-list marker, like `-` and `*`
            | b'_'
            | b'`'
            | b'~'
            | b'|'
            | b'['
            | b']'
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'!'
            | b'&'
            | b':'
            | b'@'
            | b'0'..=b'9'
    )
}

const fn is_html_escape_byte(byte: u8) -> bool {
    matches!(byte, b'&' | b'<' | b'>' | b'"')
}

const fn is_pdf_escape_byte(byte: u8) -> bool {
    matches!(byte, b'(' | b')' | b'\\' | b'\r' | b'\n')
}

fn leading_spaces_bytes(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|&&byte| byte == b' ').count()
}

fn maybe_url_prefix_at(bytes: &[u8], idx: usize, byte: u8) -> bool {
    match byte {
        b'w' => bytes[idx..].starts_with(b"www."),
        b'h' => bytes[idx..].starts_with(b"http"),
        b':' => bytes[idx..].starts_with(b"://"),
        _ => false,
    }
}

fn starts_unordered_list_marker(bytes: &[u8]) -> bool {
    let Some((&marker, rest)) = bytes.split_first() else {
        return false;
    };
    matches!(marker, b'-' | b'*' | b'+') && marker_has_padding(rest)
}

fn starts_ordered_list_marker(bytes: &[u8]) -> bool {
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || digits > 9 {
        return false;
    }
    let Some((&marker, rest)) = bytes.get(digits..).and_then(|tail| tail.split_first()) else {
        return false;
    };
    matches!(marker, b'.' | b')') && marker_has_padding(rest)
}

fn marker_has_padding(bytes: &[u8]) -> bool {
    bytes
        .first()
        .is_none_or(|byte| matches!(byte, b' ' | b'\t'))
}
