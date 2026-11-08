//! An explicit page boundary in the extended authoring profile.
//!
//! The Markdown parser keeps the comment and its original block span. Only
//! the profile-aware authoring pass promotes a complete standalone marker to
//! `Block::PageBreak`, so CommonMark comments and host-created chapter breaks
//! remain distinguishable without changing the public AST.

pub(crate) const PAGE_BREAK_MARKER: &str = "<!-- pagebreak -->";

/// Match one complete physical comment line, not HTML containing a marker.
/// The parser has already decided whether this is a block or an inline/code
/// literal. A host-created HTML block may retain its one final line ending.
pub(crate) fn is_page_break_marker(html: &str) -> bool {
    let line = html
        .strip_suffix("\r\n")
        .or_else(|| html.strip_suffix('\n'))
        .or_else(|| html.strip_suffix('\r'))
        .unwrap_or(html);
    let indent = line.bytes().take_while(|&byte| byte == b' ').count();
    if indent > 3 {
        return false;
    }
    line[indent..]
        .strip_prefix(PAGE_BREAK_MARKER)
        .is_some_and(|tail| tail.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
}
