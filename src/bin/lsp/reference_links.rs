//! Reference-use navigation without a second reference-definition grammar.
//!
//! A bounded bracket scan proposes full, collapsed or shortcut uses. Replacing
//! only the reference suffix with a unique inline destination must preserve the
//! entire parsed AST after restoring the original Link's destination and title.
//! An adjacent preceding bracket may need a removable comment boundary so it
//! cannot consume the new inline link's label as its own reference suffix.
//! The real parser owns normalization, first-definition precedence and context.

use franken_markdown::{Block, Inline};
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
    // Boundary before this link's label when it immediately follows a bracket.
    preceding: Option<usize>,
}

fn candidates(source: &str, span: SourceSpan, offset: usize) -> Result<Vec<Candidate>, Failure> {
    if span.len() > MAX_BLOCK_BYTES {
        return Err((
            -32803,
            "reference navigation source block exceeds its budget",
        ));
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
            return Err((
                -32803,
                "reference use exceeds the 8192-byte navigation budget",
            ));
        }
        if found.len() >= MAX_CANDIDATES {
            return Err((
                -32803,
                "reference navigation has too many candidate contexts",
            ));
        }
        found.push(Candidate {
            suffix: close + 1..end,
            preceding: (open > span.start && bytes[open - 1] == b']').then_some(open),
        });
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
        return Err((
            -32803,
            "reference navigation could not allocate a unique source probe",
        ));
    };
    let mut selected = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if let Some(destination) =
            prove_candidate(source, candidate, original, owner, &probe, None)?
        {
            // Equivalent adjacent bracket parses can sometimes produce the
            // same AST. More than one proven context is still ambiguous.
            if selected.is_some() {
                return Ok(None);
            }
            selected = Some((index, destination));
        }
    }
    let Some((selected_index, destination)) = selected else {
        return Ok(None);
    };
    let boundary = format!("<!--{probe}-->");
    for (index, candidate) in candidates.iter().enumerate() {
        // [x][x](probe) can be swallowed by the earlier full reference. A
        // comment lets the parser resolve the preceding [x] on its own.
        // This extra proof can only veto an ambiguous ordinary match, never
        // grant navigation from an image or another unproven source context.
        if index != selected_index
            && candidate.preceding.is_some()
            && prove_candidate(source, candidate, original, owner, &probe, Some(&boundary))?
                .is_some()
        {
            return Ok(None);
        }
    }
    Ok(Some(destination))
}

fn prove_candidate(
    source: &str,
    candidate: &Candidate,
    original: &Document,
    owner: usize,
    probe: &str,
    boundary: Option<&str>,
) -> Result<Option<String>, Failure> {
    let mut amended =
        String::with_capacity(source.len() + probe.len() + boundary.map_or(0, str::len) + 2);
    if let (Some(at), Some(boundary)) = (candidate.preceding, boundary) {
        amended.push_str(&source[..at]);
        amended.push_str(boundary);
        amended.push_str(&source[at..candidate.suffix.start]);
    } else {
        amended.push_str(&source[..candidate.suffix.start]);
    }
    amended.push('(');
    amended.push_str(probe);
    amended.push(')');
    amended.push_str(&source[candidate.suffix.end..]);
    let mut probed = parse_markdown(&amended);
    if let Some(boundary) = boundary
        && probed
            .blocks
            .get_mut(owner)
            .map(|block| remove_boundary(block, boundary))
            != Some(1)
    {
        return Ok(None);
    }
    if !matches_probe(&probed, original, owner, probe) {
        return Ok(None);
    }
    let analysis = analyze_document_links(&probed).map_err(|_| {
        (
            -32803,
            "reference link analysis could not complete within its limits",
        )
    })?;
    if !analysis.references.iter().any(|reference| {
        reference.block_index == owner
            && reference.kind == ReferenceKind::Link
            && reference.destination == probe
    }) {
        return Ok(None);
    }
    let mut destination = None;
    if let (Some(block), Some(old)) = (probed.blocks.get_mut(owner), original.blocks.get(owner)) {
        structure::restore_reference(block, old, probe, &mut destination);
    }
    Ok((&probed == original).then_some(destination).flatten())
}

/// Remove only the injected raw HTML node, never text that resembles it. The
/// unique probe makes the marker absent from the original source, and callers
/// require exactly one removal before validating the entire parsed document.
fn remove_boundary(block: &mut Block, boundary: &str) -> usize {
    match block {
        Block::Paragraph(items) | Block::Heading { inlines: items, .. } => {
            remove_inline_boundary(items, boundary)
        }
        Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => blocks
            .iter_mut()
            .map(|block| remove_boundary(block, boundary))
            .sum(),
        Block::List(list) => list
            .items
            .iter_mut()
            .flat_map(|item| &mut item.blocks)
            .map(|block| remove_boundary(block, boundary))
            .sum(),
        Block::Table(table) => table
            .head
            .iter_mut()
            .chain(table.rows.iter_mut().flatten())
            .map(|items| remove_inline_boundary(items, boundary))
            .sum(),
        Block::DefinitionList(items) => items
            .iter_mut()
            .flat_map(|item| item.terms.iter_mut().chain(&mut item.definitions))
            .map(|items| remove_inline_boundary(items, boundary))
            .sum(),
        _ => 0,
    }
}

fn remove_inline_boundary(items: &mut Vec<Inline>, boundary: &str) -> usize {
    let mut removed = 0;
    items.retain_mut(|item| match item {
        Inline::Html(html) if html == boundary => {
            removed += 1;
            false
        }
        Inline::Emphasis(items)
        | Inline::Strong(items)
        | Inline::Strikethrough(items)
        | Inline::Link { content: items, .. } => {
            removed += remove_inline_boundary(items, boundary);
            true
        }
        _ => true,
    });
    removed
}

#[cfg(test)]
#[path = "reference_links_tests.rs"]
mod tests;
