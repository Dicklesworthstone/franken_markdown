//! Ranked heading search over current synchronized editor buffers.
//! No workspace crawl, filesystem fallback, retained stale index, or regex engine.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use franken_markdown::book::validation::{AnchorKind, analyze_document_links};
use franken_markdown::{Document, SourceSpan, parse_markdown_spanned};

use super::{
    Buffer, Json, LineIndex, MAX_DOCUMENT_BYTES, MAX_DOCUMENTS, MAX_SESSION_BYTES,
    Position, number, object, position, string, text_field,
};

const MAX_RESULTS: usize = 256;
const MAX_QUERY_BYTES: usize = 256;
const MAX_QUERY_TERMS: usize = 16;
const MAX_HEADING_BYTES: usize = 4096;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
type Failure = (i32, &'static str);

struct Hit<'a> {
    rank: u8,
    folded: String,
    title: String,
    anchor: String,
    uri: &'a str,
    span: SourceSpan,
    start: Position,
    end: Position,
}

impl Ord for Hit<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.rank, &self.folded, self.uri, self.span.start, self.span.end, &self.anchor,
        ).cmp(&(
            other.rank, &other.folded, other.uri, other.span.start, other.span.end, &other.anchor,
        ))
    }
}
impl PartialOrd for Hit<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Hit<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Hit<'_> {}

fn rank(title: &str, anchor: &str, uri: &str, query: &str, terms: &[&str]) -> Option<u8> {
    if !terms.iter().all(|term| title.contains(*term) || anchor.contains(*term) || uri.contains(*term)) {
        return None;
    }
    Some(if title == query {
        0
    } else if title.starts_with(query) {
        1
    } else if title.contains(query) {
        2
    } else {
        3
    })
}

pub(super) fn request(documents: &BTreeMap<String, Buffer>, params: &Json) -> Result<Json, Failure> {
    let query = text_field(params, "query").map_err(|reason| (-32602, reason))?;
    if query.len() > MAX_QUERY_BYTES || query.chars().any(|ch| ch.is_control() && !ch.is_whitespace()) {
        return Err((-32602, "symbol query exceeds 256 bytes or contains controls"));
    }
    let query = query.trim().to_lowercase();
    let terms: Vec<_> = query.split_whitespace().collect();
    if terms.len() > MAX_QUERY_TERMS {
        return Err((-32602, "symbol query exceeds 16 terms"));
    }
    if documents.len() > MAX_DOCUMENTS {
        return Err((-32803, "workspace symbol document budget exceeded"));
    }
    let mut total = 0usize;
    for (uri, buffer) in documents {
        if uri.len() > 8192 || buffer.text.len() > MAX_DOCUMENT_BYTES {
            return Err((-32803, "workspace symbol document budget exceeded"));
        }
        total = total.checked_add(buffer.text.len())
            .filter(|total| *total <= MAX_SESSION_BYTES)
            .ok_or((-32803, "workspace symbol session budget exceeded"))?;
    }

    // The heap retains only the best suggestions, not a JSON object (including
    // a URI copy) for every heading. Later documents can displace earlier hits.
    let mut best: BinaryHeap<Hit<'_>> = BinaryHeap::new();
    for (uri, buffer) in documents {
        if !buffer.synchronized {
            continue;
        }
        let parsed = parse_markdown_spanned(&buffer.text);
        let (spans, blocks): (Vec<_>, Vec<_>) = parsed.blocks.into_iter()
            .map(|block| (block.span, block.node)).unzip();
        let analysis = analyze_document_links(&Document { blocks })
            .map_err(|_| (-32803, "workspace symbol analysis exceeded its limits"))?;
        if analysis.anchors.len() > super::navigation::MAX_NAVIGATION_ITEMS {
            return Err((-32803, "workspace symbol heading budget exceeded"));
        }
        let folded_uri = uri.to_lowercase();
        let index = LineIndex::new(&buffer.text);
        for anchor in analysis.anchors {
            if anchor.kind != AnchorKind::Heading || anchor.occurrences != 1 {
                continue;
            }
            if anchor.title.len() > MAX_HEADING_BYTES || anchor.id.len() > MAX_HEADING_BYTES {
                return Err((-32803, "workspace symbol heading text exceeds 4096 bytes"));
            }
            let folded = anchor.title.to_lowercase();
            let Some(rank) = rank(&folded, &anchor.id.to_lowercase(), &folded_uri, &query, &terms) else {
                continue;
            };
            let span = *spans.get(anchor.block_index)
                .ok_or((-32803, "workspace symbol source owner is invalid"))?;
            let hit = Hit {
                rank, folded, title: anchor.title, anchor: anchor.id, uri, span,
                start: index.position(&buffer.text, span.start),
                end: index.position(&buffer.text, span.end),
            };
            if best.len() < MAX_RESULTS {
                best.push(hit);
            } else if best.peek().is_some_and(|worst| hit.cmp(worst) == Ordering::Less) {
                best.pop();
                best.push(hit);
            }
        }
    }

    let mut results = Vec::with_capacity(best.len());
    let mut response_bytes = 2usize;
    for hit in best.into_sorted_vec() {
        let title = if hit.title.is_empty() { "Untitled section" } else { &hit.title };
        // Bound worst-case JSON escaping BEFORE constructing the response.
        // containerName carries the exact buffer identity and canonical anchor.
        let text_bytes = title.len() + hit.uri.len() * 2 + hit.anchor.len() + 2;
        response_bytes = response_bytes.checked_add(text_bytes * 6 + 512)
            .filter(|total| *total <= MAX_RESPONSE_BYTES)
            .ok_or((-32803, "symbol response exceeds 4 MiB; narrow the query"))?;
        let range = object([
            ("start", position(hit.start)),
            ("end", position(hit.end)),
        ]);
        results.push(object([
            ("name", string(title)),
            ("kind", number(15)), // SymbolKind::String, also used by documentSymbol.
            ("containerName", string(&format!("{} #{}", hit.uri, hit.anchor))),
            ("location", object([("uri", string(hit.uri)), ("range", range)])),
        ]));
    }
    Ok(Json::Array(results))
}

#[cfg(test)]
#[path = "workspace_symbols_tests.rs"]
mod tests;
