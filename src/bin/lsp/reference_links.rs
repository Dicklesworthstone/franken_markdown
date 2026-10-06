//! Reference-use navigation without a second reference-definition grammar.
//!
//! A bounded bracket scan proposes full, collapsed or shortcut uses. Replacing
//! only the reference suffix with a unique inline destination must preserve the
//! entire parsed AST after restoring the original Link's destination and title.
//! The real parser owns normalization, first-definition precedence and context.

use std::collections::BTreeMap;
use std::ops::Range;

use super::{
    Document, Failure, MAX_BLOCK_BYTES, MAX_DESTINATION_BYTES, ReferenceKind, SourceSpan,
    analyze_document_links, matches_probe, parse_markdown, structure, unique_probe,
};

const MAX_CANDIDATES: usize = 8;

#[derive(Debug, PartialEq, Eq)]
struct Candidate {
    // The reference suffix [id] / [], or an insertion point after [shortcut].
    suffix: Range<usize>,
}

fn candidates(source: &str, span: SourceSpan, offset: usize) -> Result<Vec<Candidate>, Failure> {
    if span.len() > MAX_BLOCK_BYTES {
        return Err((-32803, "reference navigation source block exceeds its budget"));
    }
    if source.get(span.start..span.end).is_none() || offset < span.start || offset > span.end {
        return Ok(Vec::new());
    }
    let bytes = source.as_bytes();
    let mut pairs = BTreeMap::new();
    let mut stack = Vec::new();
    let mut cursor = span.start;
    while cursor < span.end {
        match bytes[cursor] {
            b'\\' if cursor + 1 < span.end && !matches!(bytes[cursor + 1], b'\r' | b'\n') => {
                cursor += 2;
                continue;
            }
            b'\r' | b'\n' => stack.clear(), // No invented multiline source range.
            b'[' => stack.push(cursor),
            b']' => {
                if let Some(open) = stack.pop() {
                    pairs.insert(open, cursor);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    let mut found = Vec::new();
    for (&open, &close) in &pairs {
        // Inline link labels and reference definitions are not reference uses.
        // Code, images, HTML and escaped-context exclusions come from the probe.
        if matches!(bytes.get(close + 1), Some(b'(' | b':')) {
            continue;
        }
        let end = pairs.get(&(close + 1)).map_or(close + 1, |last| last + 1);
        if offset < open || offset >= end {
            continue;
        }
        if end - open > MAX_DESTINATION_BYTES {
            return Err((-32803, "reference use exceeds the 8192-byte navigation budget"));
        }
        if found.len() >= MAX_CANDIDATES {
            return Err((-32803, "reference navigation has too many candidate contexts"));
        }
        found.push(Candidate { suffix: close + 1..end });
    }
    Ok(found)
}

pub(super) fn destination(
    source: &str,
    span: SourceSpan,
    offset: usize,
    original: &Document,
    owner: usize,
) -> Result<Option<String>, Failure> {
    let candidates = candidates(source, span, offset)?;
    if candidates.is_empty() {
        return Ok(None);
    }
    let Some(probe) = unique_probe(source, original) else {
        return Err((-32803, "reference navigation could not allocate a unique source probe"));
    };
    let mut selected = None;
    for candidate in candidates {
        let mut amended = String::with_capacity(source.len() + probe.len() + 2);
        amended.push_str(&source[..candidate.suffix.start]);
        amended.push('(');
        amended.push_str(&probe);
        amended.push(')');
        amended.push_str(&source[candidate.suffix.end..]);
        let mut probed = parse_markdown(&amended);
        if !matches_probe(&probed, original, owner, &probe) {
            continue;
        }
        let analysis = analyze_document_links(&probed)
            .map_err(|_| (-32803, "reference link analysis could not complete within its limits"))?;
        if !analysis.references.iter().any(|reference| {
            reference.block_index == owner
                && reference.kind == ReferenceKind::Link
                && reference.destination == probe
        }) {
            continue;
        }
        let mut destination = None;
        if let (Some(block), Some(old)) = (probed.blocks.get_mut(owner), original.blocks.get(owner)) {
            structure::restore_reference(block, old, &probe, &mut destination);
        }
        if &probed != original {
            continue;
        }
        if let Some(destination) = destination {
            // Equivalent adjacent bracket parses can sometimes produce the
            // same AST. More than one proven context is still ambiguous.
            if selected.is_some() {
                return Ok(None);
            }
            selected = Some(destination);
        }
    }
    Ok(selected)
}

#[cfg(test)]
#[path = "reference_links_tests.rs"]
mod tests;
