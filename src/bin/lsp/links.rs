//! Same-buffer fragment completion and definition navigation.
//!
//! A lexical token is only a candidate. A unique destination probe must become
//! an actual Link in the real parser before we offer edits. Definition requests
//! additionally restore the original destination and require exact AST equality.
//! No source search is mistaken for parser provenance; no URI is ever opened.

use franken_markdown::{Block, Document, Inline, SourceSpan, parse_markdown, parse_markdown_spanned};
use franken_markdown::book::validation::{AnchorKind, ReferenceKind, analyze_document_links};

use super::{Json, LineIndex, MAX_DOCUMENT_BYTES, Position, number, object, position, string};

const MAX_BLOCK_BYTES: usize = 64 * 1024;
const MAX_DESTINATION_BYTES: usize = 8192;
const MAX_COMPLETIONS: usize = 256;
const MAX_ANCHOR_BYTES: usize = 1024;
type Failure = (i32, &'static str);

#[derive(Clone, Copy, Debug)]
struct Token {
    start: usize,
    end: usize,
    fragment: usize,
    angle: bool,
}

fn inlines(block: &Block) -> Option<&[Inline]> {
    match block {
        Block::Paragraph(items) | Block::Heading { inlines: items, .. } => Some(items),
        _ => None,
    }
}
fn inlines_mut(block: &mut Block) -> Option<&mut [Inline]> {
    match block {
        Block::Paragraph(items) | Block::Heading { inlines: items, .. } => Some(items),
        _ => None,
    }
}

// Deliberately conservative: explicit single-line destinations, not reference
// labels, escaped delimiters, image assets, autolinks, or arbitrary bare '#'.
fn url_byte(byte: u8) -> bool {
    !byte.is_ascii_whitespace() && !byte.is_ascii_control()
        && !matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'`' | b'"' | b'\'' | b'\\')
}
fn candidate(source: &str, span: SourceSpan, offset: usize) -> Option<Token> {
    if offset < span.start || offset > span.end { return None; }
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > span.start && url_byte(bytes[start - 1]) { start -= 1; }
    let mut end = offset;
    while end < span.end && url_byte(bytes[end]) { end += 1; }
    let raw = source.get(start..end)?;
    if raw.len() > MAX_DESTINATION_BYTES || !(raw.starts_with('#') || raw.starts_with('?')) { return None; }
    let fragment = start + raw.find('#')?;
    if offset < fragment { return None; }
    let angle = start > span.start && bytes[start - 1] == b'<';
    let mut prefix = start - usize::from(angle);
    while prefix > span.start && matches!(bytes[prefix - 1], b' ' | b'\t') { prefix -= 1; }
    if prefix < span.start + 2 || &bytes[prefix - 2..prefix] != b"](" { return None; }
    Some(Token { start, end, fragment, angle })
}

fn probe_count(items: &[Inline], probe: &str) -> usize {
    items.iter().map(|item| match item {
        Inline::Link { dest, content, .. } => usize::from(dest.as_str() == probe) + probe_count(content, probe),
        Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => probe_count(inner, probe),
        _ => 0,
    }).sum()
}

fn matches_probe(probed: &Document, original: &Document, owner: usize, probe: &str) -> bool {
    probed.blocks.len() == original.blocks.len()
        && probed.blocks.iter().zip(&original.blocks).enumerate()
            .all(|(index, (a, b))| index == owner || a == b)
        && probed.blocks.get(owner).and_then(inlines)
            .is_some_and(|items| probe_count(items, probe) == 1)
}

fn probe_document(source: &str, span: SourceSpan, token: Token, original: &Document, owner: usize,
    completing: bool) -> Option<(Document, String)>
{
    let items = inlines(original.blocks.get(owner)?)?;
    let probe = (0..32).map(|attempt| format!("#fmd-lsp-probe-{attempt}"))
        .find(|probe| !source.contains(probe.as_str()) && probe_count(items, probe) == 0)?;
    let mut amended = String::with_capacity(source.len() + probe.len() + 2);
    amended.push_str(&source[..token.start]);
    amended.push_str(&probe);
    let suffix = amended.len();
    amended.push_str(&source[token.end..]);
    let parsed = parse_markdown(&amended);
    if matches_probe(&parsed, original, owner, &probe) { return Some((parsed, probe)); }
    // The common authoring state '[label](#par' is not a parsed Link yet.
    // Complete it only at the end of this block, then ask the real parser again.
    // Never manufacture a closure across trailing prose, code, or another block.
    if completing && source.get(token.end..span.end)?.trim().is_empty() {
        amended.insert_str(suffix, if token.angle { ">)" } else { ")" });
        let parsed = parse_markdown(&amended);
        if matches_probe(&parsed, original, owner, &probe) { return Some((parsed, probe)); }
    }
    None
}

// A definition must be an EXISTING parsed link at this exact token. Align the
// probe's inline structure with the original, restore just its destination,
// and require equality of the entire document before trusting the location.
fn restore(items: &mut [Inline], original: &[Inline], probe: &str, destination: &mut Option<String>) {
    if items.len() != original.len() { return; }
    for (item, original) in items.iter_mut().zip(original) {
        match (item, original) {
            (Inline::Link { dest, content, .. }, Inline::Link { dest: old, content: old_content, .. }) => {
                if dest.as_str() == probe { *destination = Some(old.clone()); *dest = old.clone(); }
                restore(content, old_content, probe, destination);
            }
            (Inline::Emphasis(items), Inline::Emphasis(old))
            | (Inline::Strong(items), Inline::Strong(old))
            | (Inline::Strikethrough(items), Inline::Strikethrough(old)) => restore(items, old, probe, destination),
            _ => {}
        }
    }
}

fn range(index: &LineIndex, source: &str, span: SourceSpan) -> Json {
    object([
        ("start", position(index.position(source, span.start))),
        ("end", position(index.position(source, span.end))),
    ])
}
fn empty(completing: bool) -> Json {
    if completing { completion_list(Vec::new(), false) } else { Json::Null }
}
fn completion_list(items: Vec<Json>, incomplete: bool) -> Json {
    object([("isIncomplete", Json::Bool(incomplete)), ("items", Json::Array(items))])
}

// Filtering an unfinished URI prefix is not target resolution. Completed
// links always use the core analyzer's publication decoder and ambiguity rules.
fn prefix_text(raw: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'), b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10), _ => None,
        }
    }
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            out.push(hex(*bytes.get(cursor + 1)?)? * 16 + hex(*bytes.get(cursor + 2)?)?);
            cursor += 3;
        } else { out.push(bytes[cursor]); cursor += 1; }
    }
    String::from_utf8(out).ok().filter(|text| !text.chars().any(char::is_control))
}

pub fn request(method: &str, params: &Json, uri: &str, source: &str) -> Result<Json, Failure> {
    let completing = method == "textDocument/completion";
    if !completing && method != "textDocument/definition" { return Err((-32601, "unsupported link request")); }
    if source.len() > MAX_DOCUMENT_BYTES { return Err((-32803, "document exceeds the link navigation budget")); }
    let requested = Position::parse(params.get("position").ok_or((-32602, "missing position"))?)
        .map_err(|reason| (-32602, reason))?;
    let index = LineIndex::new(source);
    let offset = index.offset(source, requested).map_err(|reason| (-32602, reason))?;
    let document = parse_markdown_spanned(source);
    let Some((owner, block)) = document.blocks.iter().enumerate().rev()
        .find(|(_, block)| block.span.start <= offset && offset <= block.span.end)
    else { return Ok(empty(completing)); };
    if inlines(&block.node).is_none() { return Ok(empty(completing)); }
    let span = block.span;
    if span.len() > MAX_BLOCK_BYTES { return Err((-32803, "enclosing block exceeds the link navigation budget")); }
    let Some(token) = candidate(source, span, offset) else { return Ok(empty(completing)); };
    if completing && offset <= token.fragment { return Ok(empty(true)); }
    let (spans, blocks): (Vec<_>, Vec<_>) = document.blocks.into_iter().map(|block| (block.span, block.node)).unzip();
    let original = Document { blocks };
    let Some((mut probed, probe)) = probe_document(source, span, token, &original, owner, completing)
    else { return Ok(empty(completing)); };
    if completing {
        let Some(prefix) = prefix_text(&source[token.fragment + 1..offset]) else {
            return Ok(completion_list(Vec::new(), true));
        };
        let analysis = analyze_document_links(&probed).map_err(|_| (-32803, "link analysis could not complete within its limits"))?;
        let mut items = Vec::new();
        let mut incomplete = false;
        let edit_range = range(&index, source, SourceSpan::new(token.fragment, token.end));
        for anchor in analysis.anchors {
            if anchor.kind != AnchorKind::Heading || anchor.occurrences != 1 || !anchor.id.starts_with(&prefix) { continue; }
            if anchor.id.len() > MAX_ANCHOR_BYTES || items.len() >= MAX_COMPLETIONS { incomplete = true; continue; }
            let label = format!("#{}", anchor.id);
            let detail: String = anchor.title.chars().take(128).collect();
            // The client can match the literal typed URI prefix even when it
            // contains percent escapes; the server already filtered decoded IDs.
            let filter = format!("{}{}", &source[token.fragment..offset], &anchor.id[prefix.len()..]);
            items.push(object([
                ("label", string(&label)), ("kind", number(18)), ("detail", string(&detail)),
                ("filterText", string(&filter)), ("insertTextFormat", number(1)),
                ("textEdit", object([("range", edit_range.clone()), ("newText", string(&label))])),
            ]));
        }
        return Ok(completion_list(items, incomplete));
    }
    let mut destination = None;
    if let Some(items) = probed.blocks.get_mut(owner).and_then(inlines_mut) {
        if let Some(old) = original.blocks.get(owner).and_then(inlines) { restore(items, old, &probe, &mut destination); }
    }
    if probed != original { return Ok(Json::Null); }
    let Some(destination) = destination else { return Ok(Json::Null); };
    let analysis = analyze_document_links(&original).map_err(|_| (-32803, "link analysis could not complete within its limits"))?;
    let Some(reference) = analysis.references.iter().find(|reference| reference.block_index == owner
        && reference.kind == ReferenceKind::Link && reference.destination == destination)
    else { return Ok(Json::Null); };
    if reference.finding.is_some() { return Ok(Json::Null); }
    let target = match reference.target_block_index {
        Some(target) => *spans.get(target).ok_or((-32803, "invalid navigation source owner"))?,
        None => SourceSpan::new(0, 0), // Empty fragment addresses the document root.
    };
    Ok(object([("uri", string(uri)), ("range", range(&index, source, target))]))
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
