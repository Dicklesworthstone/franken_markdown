//! Parser-verified fragment completion and same/cross-buffer navigation.
//!
//! A lexical token is only a candidate. A unique destination probe must become
//! an actual Link in the real parser before we offer edits. Definition requests
//! additionally restore the original destination and require exact AST equality.
//! No source search is mistaken for parser provenance; no URI is ever opened.

use franken_markdown::book::validation::{AnchorKind, ReferenceKind, analyze_document_links};
use franken_markdown::{Document, SourceSpan, parse_markdown, parse_markdown_spanned};

use super::{
    Buffer, Json, LineIndex, MAX_DOCUMENT_BYTES, Position, number, object, position, string,
    workspace_links,
};
use std::collections::BTreeMap;

#[path = "link_structure.rs"]
mod structure;
#[path = "reference_links.rs"]
mod references;

const MAX_BLOCK_BYTES: usize = 64 * 1024;
const MAX_DESTINATION_BYTES: usize = 8192;
const MAX_COMPLETIONS: usize = 256;
const MAX_ANCHOR_BYTES: usize = 1024;
type Failure = (i32, &'static str);

#[derive(Clone, Copy, Debug)]
struct Token {
    start: usize,
    end: usize,
    fragment: Option<usize>,
    angle: bool,
}

// Deliberately conservative: explicit single-line destinations, not reference
// labels, escaped delimiters, image assets, autolinks, or arbitrary bare '#'.
fn url_byte(byte: u8) -> bool {
    !byte.is_ascii_whitespace()
        && !byte.is_ascii_control()
        && !matches!(
            byte,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'`' | b'"' | b'\'' | b'\\'
        )
}
fn candidate(source: &str, span: SourceSpan, offset: usize) -> Option<Token> {
    if offset < span.start || offset > span.end {
        return None;
    }
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > span.start && url_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < span.end && url_byte(bytes[end]) {
        end += 1;
    }
    let raw = source.get(start..end)?;
    if raw.is_empty() || raw.len() > MAX_DESTINATION_BYTES {
        return None;
    }
    let fragment = raw.find('#').map(|index| start + index);
    let angle = start > span.start && bytes[start - 1] == b'<';
    let mut prefix = start - usize::from(angle);
    while prefix > span.start && matches!(bytes[prefix - 1], b' ' | b'\t') {
        prefix -= 1;
    }
    if prefix < span.start + 2 || &bytes[prefix - 2..prefix] != b"](" {
        return None;
    }
    Some(Token {
        start,
        end,
        fragment,
        angle,
    })
}

fn matches_probe(probed: &Document, original: &Document, owner: usize, probe: &str) -> bool {
    probed.blocks.len() == original.blocks.len()
        && probed
            .blocks
            .iter()
            .zip(&original.blocks)
            .enumerate()
            .all(|(index, (a, b))| index == owner || a == b)
        && probed
            .blocks
            .get(owner)
            .is_some_and(|block| structure::count_block(block, probe) == 1)
}

fn unique_probe(source: &str, original: &Document) -> Option<String> {
    (0..32)
        .map(|attempt| format!("#fmd-lsp-probe-{attempt}"))
        .find(|probe| {
            !source.contains(probe.as_str()) && structure::count(&original.blocks, probe) == 0
        })
}

fn probe_document(
    source: &str,
    span: SourceSpan,
    token: Token,
    original: &Document,
    owner: usize,
    completing: bool,
) -> Option<(Document, String)> {
    let probe = unique_probe(source, original)?;
    let mut amended = String::with_capacity(source.len() + probe.len() + 2);
    amended.push_str(&source[..token.start]);
    amended.push_str(&probe);
    let suffix = amended.len();
    amended.push_str(&source[token.end..]);
    let parsed = parse_markdown(&amended);
    if matches_probe(&parsed, original, owner, &probe) {
        return Some((parsed, probe));
    }
    // The common authoring state '[label](#par' is not a parsed Link yet.
    // Complete it only at the end of this block, then ask the real parser again.
    // Never manufacture a closure across trailing prose, code, or another block.
    if completing && source.get(token.end..span.end)?.trim().is_empty() {
        amended.insert_str(suffix, if token.angle { ">)" } else { ")" });
        let parsed = parse_markdown(&amended);
        if matches_probe(&parsed, original, owner, &probe) {
            return Some((parsed, probe));
        }
    }
    None
}

fn range(index: &LineIndex, source: &str, span: SourceSpan) -> Json {
    object([
        ("start", position(index.position(source, span.start))),
        ("end", position(index.position(source, span.end))),
    ])
}
fn empty(completing: bool) -> Json {
    if completing {
        completion_list(Vec::new(), false)
    } else {
        Json::Null
    }
}
fn completion_list(items: Vec<Json>, incomplete: bool) -> Json {
    object([
        ("isIncomplete", Json::Bool(incomplete)),
        ("items", Json::Array(items)),
    ])
}

// Filtering an unfinished URI prefix is not target resolution. Completed
// links always use the core analyzer's publication decoder and ambiguity rules.
fn prefix_text(raw: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            out.push(hex(*bytes.get(cursor + 1)?)? * 16 + hex(*bytes.get(cursor + 2)?)?);
            cursor += 3;
        } else {
            out.push(bytes[cursor]);
            cursor += 1;
        }
    }
    String::from_utf8(out)
        .ok()
        .filter(|text| !text.chars().any(char::is_control))
}

#[cfg(test)]
fn request(method: &str, params: &Json, uri: &str, source: &str) -> Result<Json, Failure> {
    request_resolving(method, params, uri, source, |_| Ok(None))
}

pub fn request_in_workspace(
    method: &str,
    params: &Json,
    uri: &str,
    source: &str,
    documents: &BTreeMap<String, Buffer>,
) -> Result<Json, Failure> {
    request_resolving(method, params, uri, source, |destination| {
        workspace_links::resolve(documents, uri, destination)
    })
}

fn request_resolving<'a>(
    method: &str,
    params: &Json,
    uri: &str,
    source: &str,
    resolve: impl Fn(&str) -> Result<Option<(&'a str, &'a str)>, Failure>,
) -> Result<Json, Failure> {
    let completing = method == "textDocument/completion";
    if !completing && method != "textDocument/definition" {
        return Err((-32601, "unsupported link request"));
    }
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err((-32803, "document exceeds the link navigation budget"));
    }
    let requested = Position::parse(params.get("position").ok_or((-32602, "missing position"))?)
        .map_err(|reason| (-32602, reason))?;
    let index = LineIndex::new(source);
    let offset = index
        .offset(source, requested)
        .map_err(|reason| (-32602, reason))?;
    let document = parse_markdown_spanned(source);
    let Some((owner, block)) = document
        .blocks
        .iter()
        .enumerate()
        .rev()
        .find(|(_, block)| block.span.start <= offset && offset <= block.span.end)
    else {
        return Ok(empty(completing));
    };
    if !structure::supported(&block.node) {
        return Ok(empty(completing));
    }
    let span = block.span;
    if span.len() > MAX_BLOCK_BYTES {
        return Err((-32803, "enclosing block exceeds the link navigation budget"));
    }
    let token = candidate(source, span, offset);
    let (spans, blocks): (Vec<_>, Vec<_>) = document
        .blocks
        .into_iter()
        .map(|block| (block.span, block.node))
        .unzip();
    let original = Document { blocks };
    let definition_source = DefinitionSource { uri, source, document: &original, spans: &spans, owner };
    let Some(token) = token else {
        if !completing {
            if let Some(destination) = references::destination(source, span, offset, &original, owner)? {
                return definition_source.resolve(&destination, &resolve);
            }
        }
        return Ok(empty(completing));
    };
    // Reference labels are never edited by fragment completion.
    let fragment = token.fragment.unwrap_or(token.end);
    if completing && (token.fragment.is_none() || offset <= fragment) {
        return Ok(empty(true));
    }
    let Some((mut probed, probe)) =
        probe_document(source, span, token, &original, owner, completing)
    else {
        return Ok(empty(completing));
    };
    // A parsed link inside an unreferenced note is not emitted. Require the
    // unique probe in the renderer's reference walk before granting either
    // completion edits or cross-file navigation. Admission also precedes the
    // recursive structural restoration below.
    let mut analysis = analyze_document_links(&probed)
        .map_err(|_| (-32803, "link analysis could not complete within its limits"))?;
    if !analysis.references.iter().any(|reference| {
        reference.block_index == owner
            && reference.kind == ReferenceKind::Link
            && reference.destination == probe
    }) {
        return Ok(empty(completing));
    }
    let mut destination = None;
    if let (Some(block), Some(old)) = (probed.blocks.get_mut(owner), original.blocks.get(owner)) {
        structure::restore(block, old, &probe, &mut destination);
    }
    if destination.is_some() && probed != original {
        // Restoring a URL must not conceal changes to any sibling, container
        // metadata, label or title, even for a completion request.
        return Ok(empty(completing));
    }
    if !completing {
        return match destination {
            Some(destination) => definition_source.resolve(&destination, &resolve),
            None => Ok(Json::Null),
        };
    }
    // Existing links use the parser-decoded destination (including entities).
    // An unfinished completion has only its verified lexical URL available.
    let address = destination.as_deref().unwrap_or(&source[token.start..token.end]);
    if !address.is_empty() && !address.starts_with(['#', '?']) {
        let Some((_, target_source)) = resolve(address)? else {
            return Ok(empty(completing));
        };
        if target_source.len() > MAX_DOCUMENT_BYTES {
            return Err((-32803, "target document exceeds the link navigation budget"));
        }
        // Targets are analyzed as authored: injecting a synthetic reference
        // could incorrectly make an unreferenced footnote become published.
        analysis = analyze_document_links(&parse_markdown(target_source))
            .map_err(|_| (-32803, "target link analysis could not complete within its limits"))?;
    }
    let Some(prefix) = prefix_text(&source[fragment + 1..offset]) else {
        return Ok(completion_list(Vec::new(), true));
    };
    let mut items = Vec::new();
    let mut incomplete = false;
    let edit_range = range(&index, source, SourceSpan::new(fragment, token.end));
    for anchor in analysis.anchors {
        if anchor.kind != AnchorKind::Heading
            || anchor.occurrences != 1
            || !anchor.id.starts_with(&prefix)
        {
            continue;
        }
        if anchor.id.len() > MAX_ANCHOR_BYTES || items.len() >= MAX_COMPLETIONS {
            incomplete = true;
            continue;
        }
        let label = format!("#{}", anchor.id);
        let detail: String = anchor.title.chars().take(128).collect();
        // The client can match the literal typed URI prefix even when it
        // contains percent escapes; the server already filtered decoded IDs.
        let filter = format!(
            "{}{}",
            &source[fragment..offset],
            &anchor.id[prefix.len()..]
        );
        items.push(object([
            ("label", string(&label)),
            ("kind", number(18)),
            ("detail", string(&detail)),
            ("filterText", string(&filter)),
            ("insertTextFormat", number(1)),
            (
                "textEdit",
                object([("range", edit_range.clone()), ("newText", string(&label))]),
            ),
        ]));
    }
    Ok(completion_list(items, incomplete))
}

struct DefinitionSource<'a> {
    uri: &'a str,
    source: &'a str,
    document: &'a Document,
    spans: &'a [SourceSpan],
    owner: usize,
}

impl DefinitionSource<'_> {
    fn resolve<'a>(
        &self,
        destination: &str,
        resolve: &impl Fn(&str) -> Result<Option<(&'a str, &'a str)>, Failure>,
    ) -> Result<Json, Failure> {
        if !destination.is_empty() && !destination.starts_with(['#', '?']) {
            let Some((uri, source)) = resolve(destination)? else {
                return Ok(Json::Null);
            };
            if source.len() > MAX_DOCUMENT_BYTES {
                return Err((-32803, "target document exceeds the link navigation budget"));
            }
            return remote_definition(uri, source, destination);
        }
        let analysis = analyze_document_links(self.document)
            .map_err(|_| (-32803, "link analysis could not complete within its limits"))?;
        let Some(reference) = analysis.references.iter().find(|reference| {
            reference.block_index == self.owner
                && reference.kind == ReferenceKind::Link
                && reference.destination == destination
        }) else {
            return Ok(Json::Null);
        };
        if reference.finding.is_some() {
            return Ok(Json::Null);
        }
        let target = match reference.target_block_index {
            Some(target) => *self.spans.get(target)
                .ok_or((-32803, "invalid navigation source owner"))?,
            None => SourceSpan::new(0, 0),
        };
        Ok(object([
            ("uri", string(self.uri)),
            ("range", range(&LineIndex::new(self.source), self.source, target)),
        ]))
    }
}

// Resolve against the target's own emitted anchors and source owners, not
// headings merged across buffers. IDs and ambiguity come from the core analyzer.
fn remote_definition(uri: &str, source: &str, destination: &str) -> Result<Json, Failure> {
    let index = LineIndex::new(source);
    let fragment = destination.split_once('#').map_or("", |(_, fragment)| fragment);
    let target = if fragment.is_empty() {
        SourceSpan::new(0, 0)
    } else {
        let Some(id) = prefix_text(fragment) else {
            return Ok(Json::Null);
        };
        let document = parse_markdown_spanned(source);
        let (spans, blocks): (Vec<_>, Vec<_>) = document
            .blocks
            .into_iter()
            .map(|block| (block.span, block.node))
            .unzip();
        let analysis = analyze_document_links(&Document { blocks })
            .map_err(|_| (-32803, "target link analysis could not complete within its limits"))?;
        let Some(anchor) = analysis.anchors.iter().find(|anchor| anchor.id == id) else {
            return Ok(Json::Null);
        };
        if anchor.occurrences != 1 {
            return Ok(Json::Null);
        }
        *spans.get(anchor.block_index).ok_or((-32803, "invalid target source owner"))?
    };
    Ok(object([
        ("uri", string(uri)),
        ("range", range(&index, source, target)),
    ]))
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "nested_links_tests.rs"]
mod nested_tests;
