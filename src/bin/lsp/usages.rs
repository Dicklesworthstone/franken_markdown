//! Heading references and highlights over explicitly opened, synchronized text.
//! Locations are enclosing parser source blocks, not guessed inline offsets.

use std::collections::BTreeMap;

use franken_markdown::Block;
use franken_markdown::book::validation::DocumentLinkAnalysis;

use super::super::{MAX_DOCUMENTS, MAX_SESSION_BYTES};
use super::{
    AnchorKind, Buffer, Document, Failure, Json, LineIndex, MAX_BLOCK_BYTES, MAX_DESTINATION_BYTES,
    MAX_DOCUMENT_BYTES, Position, ReferenceKind, SourceSpan, analyze_document_links, candidate,
    number, object, parse_markdown_spanned, prefix_text, probe_document, range, references, string,
    structure, workspace_links,
};

const MAX_RESULTS: usize = 4096;
const MAX_REFERENCE_WORK: usize = 32_768;
const MAX_RESULT_URI_BYTES: usize = 1024 * 1024;

struct Snapshot {
    document: Document,
    spans: Vec<SourceSpan>,
    analysis: DocumentLinkAnalysis,
}

impl Snapshot {
    fn parse(source: &str) -> Result<Self, Failure> {
        if source.len() > MAX_DOCUMENT_BYTES {
            return Err((-32803, "document exceeds the reference search budget"));
        }
        let (spans, blocks) = parse_markdown_spanned(source)
            .blocks
            .into_iter()
            .map(|block| (block.span, block.node))
            .unzip();
        let document = Document { blocks };
        let analysis = DocumentLinkAnalysis::with_file_references(&document).map_err(|_| {
            (
                -32803,
                "reference analysis could not complete within its limits",
            )
        })?;
        Ok(Self {
            document,
            spans,
            analysis,
        })
    }

    fn owner(&self, offset: usize) -> Option<usize> {
        self.spans
            .iter()
            .rposition(|span| span.start <= offset && offset <= span.end)
    }
}

struct Target<'a> {
    uri: &'a str,
    id: String,
    span: SourceSpan,
}

// Reuse definition navigation's parser probes, structural restoration and
// publication check. A lexical match alone never selects the symbol to search.
fn destination_at(
    source: &str,
    snapshot: &Snapshot,
    offset: usize,
) -> Result<Option<String>, Failure> {
    let Some(owner) = snapshot.owner(offset) else {
        return Ok(None);
    };
    let original = &snapshot.document;
    let block = &original.blocks[owner];
    if !structure::supported(block) {
        return Ok(None);
    }
    let span = snapshot.spans[owner];
    if span.len() > MAX_BLOCK_BYTES {
        return Err((-32803, "enclosing block exceeds the link navigation budget"));
    }
    let Some(token) = candidate(source, span, offset) else {
        return references::destination(source, span, offset, original, owner);
    };
    let Some((mut probed, probe)) = probe_document(source, span, token, original, owner, false)
    else {
        return Ok(None);
    };
    let analysis = analyze_document_links(&probed)
        .map_err(|_| (-32803, "link analysis could not complete within its limits"))?;
    if !analysis.references.iter().any(|reference| {
        reference.kind == ReferenceKind::Link
            && reference.block_index == owner
            && reference.destination == probe
    }) {
        return Ok(None);
    }
    let mut destination = None;
    structure::restore(&mut probed.blocks[owner], block, &probe, &mut destination);
    if probed != *original {
        return Ok(None);
    }
    Ok(destination)
}

fn addressed_target<'a>(
    documents: &'a BTreeMap<String, Buffer>,
    origin: &'a str,
    snapshot: &Snapshot,
    destination: &str,
) -> Result<Option<Target<'a>>, Failure> {
    let destination = destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    let Some((_, fragment)) = destination.split_once('#') else {
        return Ok(None);
    };
    let Some(id) = prefix_text(fragment).filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    let remote;
    let (uri, target) = if destination.starts_with(['#', '?']) {
        (origin, snapshot)
    } else {
        let Some((uri, source)) = workspace_links::resolve(documents, origin, destination)? else {
            return Ok(None);
        };
        remote = Snapshot::parse(source)?;
        (uri, &remote)
    };
    let Ok(index) = target
        .analysis
        .anchors
        .binary_search_by(|anchor| anchor.id.cmp(&id))
    else {
        return Ok(None);
    };
    let anchor = &target.analysis.anchors[index];
    if anchor.kind != AnchorKind::Heading || anchor.occurrences != 1 {
        return Ok(None);
    }
    let span = *target
        .spans
        .get(anchor.block_index)
        .ok_or((-32803, "invalid heading source owner"))?;
    Ok(Some(Target { uri, id, span }))
}

fn select_target<'a>(
    documents: &'a BTreeMap<String, Buffer>,
    uri: &'a str,
    source: &str,
    snapshot: &Snapshot,
    requested: Position,
) -> Result<Option<Target<'a>>, Failure> {
    let offset = LineIndex::new(source)
        .offset(source, requested)
        .map_err(|reason| (-32602, reason))?;
    if let Some(destination) = destination_at(source, snapshot, offset)? {
        return addressed_target(documents, uri, snapshot, &destination);
    }
    let Some(owner) = snapshot.owner(offset) else {
        return Ok(None);
    };
    // A nested container can own several headings. Without independent source
    // spans, a caret there cannot identify one; selecting a verified link can.
    if !matches!(
        snapshot.document.blocks.get(owner),
        Some(Block::Heading { .. })
    ) {
        return Ok(None);
    }
    let mut anchors = snapshot
        .analysis
        .anchors
        .iter()
        .filter(|anchor| anchor.kind == AnchorKind::Heading && anchor.block_index == owner);
    let Some(anchor) = anchors.next() else {
        return Ok(None);
    };
    if anchors.next().is_some() || anchor.occurrences != 1 {
        return Ok(None);
    }
    if anchor.id.len() > MAX_DESTINATION_BYTES {
        return Err((-32803, "heading ID exceeds the reference search budget"));
    }
    Ok(Some(Target {
        uri,
        id: anchor.id.clone(),
        span: snapshot.spans[owner],
    }))
}

#[derive(Default)]
struct Locations<'a> {
    entries: BTreeMap<(&'a str, usize, usize), bool>,
    uri_bytes: usize,
}

impl<'a> Locations<'a> {
    fn add(&mut self, uri: &'a str, span: SourceSpan, declaration: bool) -> Result<(), Failure> {
        let key = (uri, span.start, span.end);
        if let Some(existing) = self.entries.get_mut(&key) {
            *existing |= declaration;
            return Ok(());
        }
        if self.entries.len() >= MAX_RESULTS
            || uri.len() > MAX_RESULT_URI_BYTES.saturating_sub(self.uri_bytes)
        {
            return Err((-32803, "reference results exceed the response budget"));
        }
        self.uri_bytes += uri.len();
        self.entries.insert(key, declaration);
        Ok(())
    }
}

pub(super) fn request(
    method: &str,
    params: &Json,
    uri: &str,
    documents: &BTreeMap<String, Buffer>,
) -> Result<Json, Failure> {
    let highlights = method == "textDocument/documentHighlight";
    if !highlights && method != "textDocument/references" {
        return Err((-32601, "unsupported reference request"));
    }
    let include_declaration = if highlights {
        true
    } else {
        params
            .get("context")
            .and_then(|context| context.get("includeDeclaration"))
            .and_then(Json::as_bool)
            .ok_or((
                -32602,
                "references requires context.includeDeclaration boolean",
            ))?
    };
    let requested = Position::parse(params.get("position").ok_or((-32602, "missing position"))?)
        .map_err(|reason| (-32602, reason))?;
    let (uri, buffer) = documents
        .get_key_value(uri)
        .ok_or((-32602, "document is not open"))?;
    if !buffer.synchronized {
        return Err((-32801, "document requires full-text resynchronization"));
    }
    let mut source_bytes = 0usize;
    if documents.len() > MAX_DOCUMENTS {
        return Err((-32803, "too many open documents for reference search"));
    }
    for (key, document) in documents {
        if key.len() > 8192
            || document.text.len() > MAX_DOCUMENT_BYTES
            || document.text.len() > MAX_SESSION_BYTES.saturating_sub(source_bytes)
        {
            return Err((-32803, "workspace exceeds the reference search budget"));
        }
        source_bytes += document.text.len();
    }
    let snapshot = Snapshot::parse(&buffer.text)?;
    let Some(target) = select_target(documents, uri, &buffer.text, &snapshot, requested)? else {
        return Ok(Json::Array(Vec::new()));
    };
    // An unsynchronized buffer might contain another use. Never return a
    // deceptively complete workspace result from the last accepted revision.
    if !highlights && documents.values().any(|document| !document.synchronized) {
        return Err((
            -32801,
            "reference search requires synchronized open documents",
        ));
    }
    let mut found = Locations::default();
    let mut work = 0usize;
    for (origin, document) in documents {
        if highlights && origin != uri {
            continue;
        }
        let other;
        let current = if origin == uri {
            &snapshot
        } else {
            other = Snapshot::parse(&document.text)?;
            &other
        };
        work += current.analysis.references.len();
        if work > MAX_REFERENCE_WORK {
            return Err((
                -32803,
                "workspace reference count exceeds the search budget",
            ));
        }
        for reference in &current.analysis.references {
            if reference.kind != ReferenceKind::Link || reference.finding.is_some() {
                continue;
            }
            let destination = reference
                .destination
                .trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
            let Some((_, fragment)) = destination.split_once('#') else {
                continue;
            };
            if prefix_text(fragment).as_deref() != Some(target.id.as_str()) {
                continue;
            }
            let matches = if destination.starts_with(['#', '?']) {
                origin == target.uri
            } else {
                workspace_links::resolve(documents, origin, destination)?
                    .is_some_and(|(resolved, _)| resolved == target.uri)
            };
            if matches {
                let span = *current
                    .spans
                    .get(reference.block_index)
                    .ok_or((-32803, "invalid reference source owner"))?;
                found.add(origin, span, false)?;
            }
        }
    }
    if include_declaration && (!highlights || target.uri == uri) {
        found.add(target.uri, target.span, true)?;
    }
    let mut indices = BTreeMap::new();
    let mut result = Vec::with_capacity(found.entries.len());
    for ((owner, start, end), declaration) in found.entries {
        let source = &documents
            .get(owner)
            .ok_or((-32803, "reference owner is no longer open"))?
            .text;
        let index = indices
            .entry(owner)
            .or_insert_with(|| LineIndex::new(source));
        let location = range(index, source, SourceSpan::new(start, end));
        result.push(if highlights {
            object([
                ("range", location),
                ("kind", number(if declaration { 3 } else { 2 })),
            ])
        } else {
            object([("uri", string(owner)), ("range", location)])
        });
    }
    Ok(Json::Array(result))
}

#[cfg(test)]
#[path = "usages_tests.rs"]
mod tests;
