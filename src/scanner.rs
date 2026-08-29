//! Scalar byte/line scanners: small, safe, allocation-free, and portable.
//!
//! [`find_html_text_escape`] and [`find_html_escape`] back the HTML emitter's
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
    find_needles(
        bytes,
        |word| {
            word.to_ne_bytes()
                .iter()
                .any(|&byte| is_markdown_special_byte(byte))
        },
        is_markdown_special_byte,
    )
}

/// Find the first byte that must be escaped in HTML text-node output.
#[must_use]
pub fn find_html_text_escape(bytes: &[u8]) -> Option<usize> {
    find_any_of_3(bytes, b'&', b'<', b'>')
}

/// Find the first byte that must be escaped in HTML attribute output.
#[must_use]
pub fn find_html_escape(bytes: &[u8]) -> Option<usize> {
    find_any_of_4(bytes, b'&', b'<', b'>', b'"')
}

/// Find the first byte that must be escaped in a PDF literal string.
#[must_use]
pub fn find_pdf_escape(bytes: &[u8]) -> Option<usize> {
    find_needles(
        bytes,
        |word| {
            word_contains_byte(word, b'(')
                || word_contains_byte(word, b')')
                || word_contains_byte(word, b'\\')
                || word_contains_byte(word, b'\r')
                || word_contains_byte(word, b'\n')
        },
        is_pdf_escape_byte,
    )
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

const FLAG_PIPE: u8 = 1 << 0;
const FLAG_BACKTICK: u8 = 1 << 1;
const FLAG_TILDE: u8 = 1 << 2;
const FLAG_DASH: u8 = 1 << 3;
const FLAG_COLON: u8 = 1 << 4;
const FLAG_OPEN_ANGLE: u8 = 1 << 5;
const FLAG_AT: u8 = 1 << 6;
const FLAG_OPEN_BRACKET: u8 = 1 << 7;

const LINE_CHAR_FLAGS: [u8; 256] = {
    let mut table = [0u8; 256];
    table[b'|' as usize] = FLAG_PIPE;
    table[b'`' as usize] = FLAG_BACKTICK;
    table[b'~' as usize] = FLAG_TILDE;
    table[b'-' as usize] = FLAG_DASH;
    table[b':' as usize] = FLAG_COLON;
    table[b'<' as usize] = FLAG_OPEN_ANGLE;
    table[b'@' as usize] = FLAG_AT;
    table[b'[' as usize] = FLAG_OPEN_BRACKET;
    table
};

const MARKDOWN_SPECIAL_TABLE: [bool; 256] = {
    let mut table = [false; 256];
    let specials = b"\\\n\r\t#-=*+_`~|[]()<>!&:@0123456789";
    let mut i = 0;
    while i < specials.len() {
        table[specials[i] as usize] = true;
        i += 1;
    }
    table
};

const HTML_ESCAPE_TABLE: [bool; 256] = {
    let mut table = [false; 256];
    let escapes = b"&<>\"";
    let mut i = 0;
    while i < escapes.len() {
        table[escapes[i] as usize] = true;
        i += 1;
    }
    table
};

const PDF_ESCAPE_TABLE: [bool; 256] = {
    let mut table = [false; 256];
    let escapes = b"()\\\r\n";
    let mut i = 0;
    while i < escapes.len() {
        table[escapes[i] as usize] = true;
        i += 1;
    }
    table
};

/// Classify one Markdown source line without allocation.
#[must_use]
pub fn scan_markdown_line(line: &str) -> ParserLineScan {
    let bytes = line.as_bytes();
    let mut accum_flags = 0u8;
    let mut has_reference_colon = false;
    let mut maybe_url_prefix = false;
    let mut first_special_byte = None;
    let mut leading_spaces = 0usize;
    let mut in_leading_spaces = true;
    let mut previous = 0u8;

    for (idx, &byte) in bytes.iter().enumerate() {
        if in_leading_spaces {
            if byte == b' ' {
                leading_spaces += 1;
            } else {
                in_leading_spaces = false;
            }
        }
        accum_flags |= LINE_CHAR_FLAGS[byte as usize];
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

    let first = bytes.get(leading_spaces).copied();
    let indented_as_block = leading_spaces <= 3;
    let marker_tail = bytes.get(leading_spaces..).unwrap_or(&[]);
    let maybe_list_marker = indented_as_block
        && (starts_unordered_list_marker(marker_tail) || starts_ordered_list_marker(marker_tail));
    let first_special_byte =
        first_special_byte.or_else(|| maybe_list_marker.then_some(leading_spaces));

    let contains_pipe = (accum_flags & FLAG_PIPE) != 0;
    let contains_backtick = (accum_flags & FLAG_BACKTICK) != 0;
    let contains_tilde = (accum_flags & FLAG_TILDE) != 0;
    let contains_dash = (accum_flags & FLAG_DASH) != 0;
    let contains_colon = (accum_flags & FLAG_COLON) != 0;
    let contains_open_angle = (accum_flags & FLAG_OPEN_ANGLE) != 0;
    let contains_at = (accum_flags & FLAG_AT) != 0;
    let contains_open_bracket = (accum_flags & FLAG_OPEN_BRACKET) != 0;

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
        maybe_blockquote: indented_as_block && first == Some(b'>'),
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

#[inline(always)]
const fn is_markdown_special_byte(byte: u8) -> bool {
    MARKDOWN_SPECIAL_TABLE[byte as usize]
}

#[inline(always)]
const fn is_html_escape_byte(byte: u8) -> bool {
    HTML_ESCAPE_TABLE[byte as usize]
}

#[inline(always)]
const fn is_pdf_escape_byte(byte: u8) -> bool {
    PDF_ESCAPE_TABLE[byte as usize]
}

fn find_any_of_3(bytes: &[u8], a: u8, b: u8, c: u8) -> Option<usize> {
    find_needles(
        bytes,
        |word| word_contains_any_of_3(word, a, b, c),
        |byte| byte == a || byte == b || byte == c,
    )
}

fn find_any_of_4(bytes: &[u8], a: u8, b: u8, c: u8, d: u8) -> Option<usize> {
    find_needles(
        bytes,
        |word| word_contains_any_of_4(word, a, b, c, d),
        |byte| byte == a || byte == b || byte == c || byte == d,
    )
}

#[inline(always)]
fn word_contains_any_of_3(word: u64, a: u8, b: u8, c: u8) -> bool {
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGHS: u64 = 0x8080_8080_8080_8080;

    let ma = word ^ (ONES * u64::from(a));
    let mb = word ^ (ONES * u64::from(b));
    let mc = word ^ (ONES * u64::from(c));
    let has_a = ma.wrapping_sub(ONES) & !ma;
    let has_b = mb.wrapping_sub(ONES) & !mb;
    let has_c = mc.wrapping_sub(ONES) & !mc;
    (has_a | has_b | has_c) & HIGHS != 0
}

#[inline(always)]
fn word_contains_any_of_4(word: u64, a: u8, b: u8, c: u8, d: u8) -> bool {
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGHS: u64 = 0x8080_8080_8080_8080;

    let ma = word ^ (ONES * u64::from(a));
    let mb = word ^ (ONES * u64::from(b));
    let mc = word ^ (ONES * u64::from(c));
    let md = word ^ (ONES * u64::from(d));
    let has_a = ma.wrapping_sub(ONES) & !ma;
    let has_b = mb.wrapping_sub(ONES) & !mb;
    let has_c = mc.wrapping_sub(ONES) & !mc;
    let has_d = md.wrapping_sub(ONES) & !md;
    (has_a | has_b | has_c | has_d) & HIGHS != 0
}

/// Chunked first-index scan at 32 (AVX2 width), 16 (NEON/SSE2 width), or 8
/// bytes. Uses safe SWAR on copied lanes — no raw pointers, no `unsafe`.
/// LLVM typically lowers the 16/32-byte copies to NEON/AVX2 on those targets.
fn find_needles(
    bytes: &[u8],
    word_matches: impl Fn(u64) -> bool,
    byte_matches: impl Fn(u8) -> bool,
) -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx2") {
            find_by_chunk32(bytes, &word_matches, &byte_matches)
        } else {
            find_by_chunk16(bytes, &word_matches, &byte_matches)
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        find_by_chunk16(bytes, &word_matches, &byte_matches)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        find_by_word_scan(bytes, word_matches, byte_matches)
    }
}

#[cfg(target_arch = "x86_64")]
fn find_by_chunk32(
    bytes: &[u8],
    word_matches: &impl Fn(u64) -> bool,
    byte_matches: &impl Fn(u8) -> bool,
) -> Option<usize> {
    let mut chunks = bytes.chunks_exact(32);
    for (chunk_idx, chunk) in chunks.by_ref().enumerate() {
        if chunk32_hot(chunk, word_matches) {
            let base = chunk_idx * 32;
            // SWAR can theoretically false-positive; do not abort the scan
            // with `None` just because this lane's byte predicate missed.
            if let Some(rel) = chunk.iter().position(|&byte| byte_matches(byte)) {
                return Some(base + rel);
            }
        }
    }
    let base = bytes.len() - chunks.remainder().len();
    find_by_chunk16(chunks.remainder(), word_matches, byte_matches).map(|rel| base + rel)
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn find_by_chunk16(
    bytes: &[u8],
    word_matches: &impl Fn(u64) -> bool,
    byte_matches: &impl Fn(u8) -> bool,
) -> Option<usize> {
    let mut chunks = bytes.chunks_exact(16);
    for (chunk_idx, chunk) in chunks.by_ref().enumerate() {
        if chunk16_hot(chunk, word_matches) {
            let base = chunk_idx * 16;
            if let Some(rel) = chunk.iter().position(|&byte| byte_matches(byte)) {
                return Some(base + rel);
            }
        }
    }
    let base = bytes.len() - chunks.remainder().len();
    find_by_word_scan(chunks.remainder(), word_matches, byte_matches).map(|rel| base + rel)
}

fn find_by_word_scan(
    bytes: &[u8],
    word_matches: impl Fn(u64) -> bool,
    byte_matches: impl Fn(u8) -> bool,
) -> Option<usize> {
    let mut chunks = bytes.chunks_exact(8);
    for (chunk_idx, chunk) in chunks.by_ref().enumerate() {
        let mut lane = [0u8; 8];
        lane.copy_from_slice(chunk);
        if word_matches(u64::from_ne_bytes(lane)) {
            let base = chunk_idx * 8;
            if let Some(rel) = chunk.iter().position(|&byte| byte_matches(byte)) {
                return Some(base + rel);
            }
        }
    }
    let base = bytes.len() - chunks.remainder().len();
    chunks
        .remainder()
        .iter()
        .position(|&byte| byte_matches(byte))
        .map(|rel| base + rel)
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn chunk16_hot(chunk: &[u8], word_matches: impl Fn(u64) -> bool) -> bool {
    let mut lo = [0u8; 8];
    let mut hi = [0u8; 8];
    lo.copy_from_slice(&chunk[..8]);
    hi.copy_from_slice(&chunk[8..16]);
    word_matches(u64::from_ne_bytes(lo)) || word_matches(u64::from_ne_bytes(hi))
}

#[cfg(target_arch = "x86_64")]
fn chunk32_hot(chunk: &[u8], word_matches: impl Fn(u64) -> bool) -> bool {
    chunk16_hot(&chunk[..16], &word_matches) || chunk16_hot(&chunk[16..32], &word_matches)
}

fn word_contains_byte(word: u64, byte: u8) -> bool {
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGHS: u64 = 0x8080_8080_8080_8080;

    let repeated = ONES * u64::from(byte);
    let matches = word ^ repeated;
    matches.wrapping_sub(ONES) & !matches & HIGHS != 0
}

fn maybe_url_prefix_at(bytes: &[u8], idx: usize, byte: u8) -> bool {
    let tail = bytes.get(idx..).unwrap_or(&[]);
    match byte.to_ascii_lowercase() {
        b'w' => starts_with_ignore_ascii_case(tail, b"www."),
        b'h' => {
            starts_with_ignore_ascii_case(tail, b"http://")
                || starts_with_ignore_ascii_case(tail, b"https://")
        }
        _ => false,
    }
}

fn starts_with_ignore_ascii_case(bytes: &[u8], needle: &[u8]) -> bool {
    bytes
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
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
