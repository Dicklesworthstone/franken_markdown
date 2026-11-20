//! Expand candidate structural tabs before containers remove physical columns.
//!
//! Recognition remains with the block parser. A marker-shaped prefix can also
//! be literal code or a paragraph continuation, so every expansion retains its
//! source tab and literal consumers restore any wholly unconsumed interval.

use std::borrow::Cow;
use std::ops::Range;

pub(super) struct TabExpansion {
    pub source: usize,
    pub expanded: Range<usize>,
}

/// Whitespace before and between potential quote/list markers can acquire
/// structural meaning after an enclosing container strips its own columns.
/// Do not stop at four columns: an indented continuation may contain another
/// list or quote. This conservative scan never decides whether a block exists.
fn prefix_end(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut index = 0;
    loop {
        while matches!(bytes.get(index), Some(b' ' | b'\t')) {
            index += 1;
        }
        let Some(&first) = bytes.get(index) else {
            return index;
        };
        if first == b'>' {
            index += 1;
            continue;
        }
        let width = if matches!(first, b'-' | b'+' | b'*') {
            1
        } else if first.is_ascii_digit() {
            let mut end = index + 1;
            while end - index < 10 && bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end - index > 9 || !matches!(bytes.get(end), Some(b'.' | b')')) {
                return index;
            }
            end - index + 1
        } else {
            return index;
        };
        if !bytes
            .get(index + width)
            .is_none_or(|byte| matches!(byte, b' ' | b'\t'))
        {
            return index;
        }
        index += width;
    }
}

/// All prefix bytes are ASCII, so an expanded byte offset is also the original
/// physical column. Reuse these intervals for literal restoration and source
/// tracking instead of remeasuring a tab after a container has been removed.
pub(super) fn expansions(line: &str) -> impl Iterator<Item = TabExpansion> + '_ {
    let prefix = &line.as_bytes()[..prefix_end(line)];
    let mut column = 0;
    prefix
        .iter()
        .enumerate()
        .filter_map(move |(source, &byte)| {
            let start = column;
            column += if byte == b'\t' { 4 - column % 4 } else { 1 };
            (byte == b'\t').then_some(TabExpansion {
                source,
                expanded: start..column,
            })
        })
}

pub(super) fn expand(line: &str) -> Cow<'_, str> {
    let mut tabs = expansions(line).peekable();
    if tabs.peek().is_none() {
        return Cow::Borrowed(line);
    }
    let mut output = String::with_capacity(line.len());
    let mut copied = 0;
    for tab in tabs {
        output.push_str(&line[copied..tab.source]);
        output.extend(std::iter::repeat_n(' ', tab.expanded.len()));
        copied = tab.source + 1;
    }
    output.push_str(&line[copied..]);
    Cow::Owned(output)
}
