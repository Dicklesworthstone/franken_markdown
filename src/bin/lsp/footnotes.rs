//! Exact, parser-proven footnote symbols in an opened Markdown buffer.
//!
//! Raw markers are candidates only. Two bulk probe parses identify the actual
//! FootnoteRef/FootnoteDefinition nodes and then restore their IDs. The complete
//! restored AST and occurrence count must equal the original. This excludes
//! lookalikes in code, image alt text, URLs, titles and HTML without maintaining
//! another Markdown grammar, or parsing the whole document once per marker.

use std::collections::BTreeMap;

use franken_markdown::book::validation::{
    AnchorKind, DocumentAnchor, DocumentLinkAnalysis, analyze_document_links,
};
use franken_markdown::{Block, Document, Inline, SourceSpan, parse_markdown};

use super::{
    Buffer, Json, LineIndex, MAX_DOCUMENT_BYTES, MAX_DOCUMENTS, MAX_SESSION_BYTES, Position,
    number, object, position, string, workspace_links,
};

type Failure = (i32, &'static str);
const MAX_CANDIDATES: usize = 8192;
const MAX_SYMBOLS: usize = 4096;
const MAX_PROBE_BYTES: usize = 4 * 1024 * 1024;
const MAX_LINKS: usize = 32768;
const MAX_LINK_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Definition,
    Reference,
}

struct Candidate {
    marker: SourceSpan,
    label: SourceSpan,
    id: String,
}

struct Note {
    candidate: Candidate,
    kind: Kind,
    order: usize,
}

struct Inventory {
    document: Document,
    notes: Vec<Note>,
}

enum Symbol<'a> {
    Note(&'a mut String, Kind),
    Link(&'a str),
}

fn walk_blocks(
    blocks: &mut [Block],
    visit: &mut impl FnMut(Symbol<'_>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                walk_inlines(inlines, visit)?;
            }
            Block::BlockQuote(blocks) => walk_blocks(blocks, visit)?,
            Block::FootnoteDefinition { id, blocks } => {
                visit(Symbol::Note(id, Kind::Definition))?;
                walk_blocks(blocks, visit)?;
            }
            Block::List(list) => {
                for item in &mut list.items {
                    walk_blocks(&mut item.blocks, visit)?;
                }
            }
            Block::Table(table) => {
                for cell in table.head.iter_mut().chain(table.rows.iter_mut().flatten()) {
                    walk_inlines(cell, visit)?;
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        walk_inlines(inlines, visit)?;
                    }
                }
            }
            Block::CodeBlock { .. }
            | Block::HtmlBlock(_)
            | Block::MathBlock(_)
            | Block::ThematicBreak
            | Block::PageBreak => {}
        }
    }
    Ok(())
}

fn walk_inlines(
    inlines: &mut [Inline],
    visit: &mut impl FnMut(Symbol<'_>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    for inline in inlines {
        match inline {
            Inline::FootnoteRef { id } => visit(Symbol::Note(id, Kind::Reference))?,
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => walk_inlines(children, visit)?,
            Inline::Link { dest, content, .. } => {
                visit(Symbol::Link(dest))?;
                walk_inlines(content, visit)?;
            }
            Inline::Text(_)
            | Inline::Code(_)
            | Inline::Image { .. }
            | Inline::Html(_)
            | Inline::Math(_)
            | Inline::DisplayMath(_)
            | Inline::SoftBreak
            | Inline::HardBreak => {}
        }
    }
    Ok(())
}

fn candidates(source: &str) -> Result<Vec<Candidate>, Failure> {
    let mut result = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("[^") {
        let start = offset + relative;
        offset = start + 2;
        // The actual parser caps labels at 128 UTF-8 bytes. Bound each lexical
        // scan too, including malformed sequences without a closing bracket.
        let bytes = source.as_bytes();
        let limit = (offset + 129).min(bytes.len());
        let Some(close) = bytes[offset..limit].iter().position(|&byte| byte == b']') else {
            continue;
        };
        let end = offset + close;
        let Some(id) = source.get(offset..end) else {
            continue;
        };
        if id.is_empty()
            || id
                .chars()
                .any(|c| c.is_whitespace() || c.is_ascii_control() || c == '[')
        {
            continue;
        }
        if result.len() == MAX_CANDIDATES {
            return Err((
                -32803,
                "footnote source candidates exceed the navigation budget",
            ));
        }
        result.push(Candidate {
            marker: SourceSpan::new(start, end + 1),
            label: SourceSpan::new(offset, end),
            id: id.to_string(),
        });
        offset = end + 1;
    }
    Ok(result)
}

fn candidate_at_position(source: &str, offset: usize) -> Result<bool, Failure> {
    // A complete marker occupies at most 131 bytes. Keep the initial dispatch
    // check local so unrelated heading/link requests never pay a whole-file
    // candidate budget, even in documents full of footnote-shaped literals.
    let mut start = offset.saturating_sub(131);
    while !source.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = offset.saturating_add(131).min(source.len());
    while !source.is_char_boundary(end) {
        end += 1;
    }
    Ok(candidates(&source[start..end])?
        .iter()
        .any(|candidate| candidate.marker.contains(offset - start)))
}

fn probed_source(
    source: &str,
    candidates: &[Candidate],
    prefix: &str,
    admitted: impl Fn(usize) -> bool,
) -> Result<String, Failure> {
    let mut output = String::with_capacity(source.len());
    let mut previous = 0;
    for (index, candidate) in candidates.iter().enumerate() {
        if admitted(index) {
            output.push_str(&source[previous..candidate.label.start]);
            output.push_str(&format!("{prefix}{index}"));
            previous = candidate.label.end;
            if output.len() > MAX_PROBE_BYTES {
                return Err((-32803, "footnote probe exceeds the navigation budget"));
            }
        }
    }
    output.push_str(&source[previous..]);
    if output.len() > MAX_PROBE_BYTES {
        return Err((-32803, "footnote probe exceeds the navigation budget"));
    }
    Ok(output)
}

impl Inventory {
    fn parse(source: &str, candidates: Vec<Candidate>) -> Result<Self, Failure> {
        let mut document = parse_markdown(source);
        let mut count = 0;
        walk_blocks(&mut document.blocks, &mut |symbol| {
            if matches!(symbol, Symbol::Note(_, _)) {
                count += 1;
                if count > MAX_SYMBOLS {
                    return Err((-32803, "footnote symbols exceed the navigation budget"));
                }
            }
            Ok(())
        })?;
        if count == 0 {
            return Ok(Self {
                document,
                notes: Vec::new(),
            });
        }
        let prefix = (0..32)
            .map(|attempt| format!("fmd-lsp-note-{attempt}-"))
            .find(|prefix| !source.contains(prefix.as_str()))
            .ok_or((-32803, "cannot reserve an unambiguous footnote probe"))?;
        let probes: BTreeMap<_, _> = (0..candidates.len())
            .map(|index| (format!("{prefix}{index}"), index))
            .collect();
        let mut kinds = vec![None; candidates.len()];
        {
            let amended = probed_source(source, &candidates, &prefix, |_| true)?;
            let mut probed = parse_markdown(&amended);
            walk_blocks(&mut probed.blocks, &mut |symbol| {
                if let Symbol::Note(id, kind) = symbol
                    && let Some(&index) = probes.get(id.as_str())
                    && kinds[index].replace(kind).is_some()
                {
                    return Err((-32803, "footnote probe has ambiguous source ownership"));
                }
                Ok(())
            })?;
        }
        // The first pass can change unrelated lexical lookalikes. Reparse ONLY
        // admitted note tokens, then require exact restoration of all semantics.
        let amended = probed_source(source, &candidates, &prefix, |index| kinds[index].is_some())?;
        let mut restored = parse_markdown(&amended);
        let mut orders = vec![None; candidates.len()];
        let mut mapped = 0;
        walk_blocks(&mut restored.blocks, &mut |symbol| {
            if let Symbol::Note(id, kind) = symbol
                && let Some(&index) = probes.get(id.as_str())
            {
                if kinds[index] != Some(kind) || orders[index].replace(mapped).is_some() {
                    return Err((-32803, "footnote probe changed semantic ownership"));
                }
                *id = candidates[index].id.clone();
                mapped += 1;
            }
            Ok(())
        })?;
        if restored != document
            || mapped != count
            || kinds.iter().filter(|kind| kind.is_some()).count() != count
        {
            return Err((
                -32803,
                "footnote source mapping could not be proven complete",
            ));
        }
        let notes = candidates
            .into_iter()
            .zip(kinds)
            .zip(orders)
            .filter_map(|((candidate, kind), order)| {
                Some(Note {
                    candidate,
                    kind: kind?,
                    order: order?,
                })
            })
            .collect();
        Ok(Self { document, notes })
    }
}

fn range(index: &LineIndex, source: &str, span: SourceSpan) -> Json {
    object([
        ("start", position(index.position(source, span.start))),
        ("end", position(index.position(source, span.end))),
    ])
}

fn location(uri: &str, index: &LineIndex, source: &str, span: SourceSpan) -> Json {
    object([("uri", string(uri)), ("range", range(index, source, span))])
}

/// A footnote rename cannot safely rewrite ordinary Markdown links with the
/// marker-only provenance above. Refuse when any opened buffer targets its
/// generated anchor, including links in currently dormant note bodies. Never
/// guess at an unopened file or alter a different document's note namespace.
fn check_rename_links(
    documents: &BTreeMap<String, Buffer>,
    uri: &str,
    document: &mut Document,
    id: &str,
) -> Result<usize, Failure> {
    if documents.len() > MAX_DOCUMENTS {
        return Err((-32803, "workspace exceeds the footnote rename budget"));
    }
    // Use the renderer's fragment decoder; this synthetic inventory only tests
    // the spelling of a guarded ID, never grants navigation or edit authority.
    let guarded = DocumentLinkAnalysis {
        anchors: vec![DocumentAnchor {
            id: format!("fn-{id}"),
            block_index: 0,
            title: String::new(),
            kind: AnchorKind::Footnote,
            occurrences: 1,
        }],
        references: Vec::new(),
        external: 0,
        unchecked: 0,
    };
    // A singleton inventory asks whether the selected normalized URI is a
    // possible target. The navigation resolver deliberately returns None for
    // ambiguous opened aliases, which must never become permission to rename.
    let target = BTreeMap::from([(
        uri.to_string(),
        Buffer {
            text: String::new(),
            version: 0,
            synchronized: true,
        },
    )]);
    let mut source_bytes = 0;
    let mut links = 0;
    let mut link_bytes = 0;
    for (origin, buffer) in documents {
        if !buffer.synchronized {
            return Err((
                -32801,
                "footnote rename requires synchronized open documents",
            ));
        }
        if buffer.text.len() > MAX_DOCUMENT_BYTES
            || buffer.text.len() > MAX_SESSION_BYTES.saturating_sub(source_bytes)
        {
            return Err((-32803, "workspace exceeds the footnote rename budget"));
        }
        source_bytes += buffer.text.len();
        let mut other;
        let current = if origin == uri {
            &mut *document
        } else {
            other = parse_markdown(&buffer.text);
            &mut other
        };
        walk_blocks(&mut current.blocks, &mut |symbol| {
            let Symbol::Link(destination) = symbol else {
                return Ok(());
            };
            links += 1;
            link_bytes += destination.len();
            if links > MAX_LINKS || link_bytes > MAX_LINK_BYTES || destination.len() > 8192 {
                return Err((-32803, "links exceed the footnote rename budget"));
            }
            if !matches!(guarded.resolve_fragment(destination), Ok(Some(_))) {
                return Ok(());
            }
            let destination =
                destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
            let targets_note = if destination.starts_with(['#', '?']) {
                origin == uri || workspace_links::resolve(&target, origin, origin)?.is_some()
            } else {
                workspace_links::resolve(&target, origin, destination)?.is_some()
            };
            if targets_note {
                return Err((
                    -32803,
                    "footnote has ordinary anchor links; update those links before renaming its label",
                ));
            }
            Ok(())
        })?;
    }
    Ok(source_bytes)
}

fn valid_new_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    // The parser, not a divergent LSP label normalizer, defines accepted IDs.
    parse_markdown(&format!("[^{name}]\n\n[^{name}]: note\n"))
        == Document {
            blocks: vec![
                Block::Paragraph(vec![Inline::FootnoteRef {
                    id: name.to_string(),
                }]),
                Block::FootnoteDefinition {
                    id: name.to_string(),
                    blocks: vec![Block::Paragraph(vec![Inline::Text("note".to_string())])],
                },
            ],
        }
}

fn rename(
    uri: &str,
    buffer: &Buffer,
    inventory: &Inventory,
    selected: &str,
    name: &str,
    index: &LineIndex,
    session_bytes: usize,
) -> Result<Json, Failure> {
    let source = &buffer.text;
    let other_buffer_bytes = session_bytes
        .checked_sub(source.len())
        .ok_or((-32803, "workspace exceeds the footnote rename budget"))?;
    let byte_budget = MAX_DOCUMENT_BYTES.min(MAX_SESSION_BYTES.saturating_sub(other_buffer_bytes));
    if !valid_new_name(name) {
        return Err((
            -32602,
            "newName must be a parser-valid footnote label of 1..128 UTF-8 bytes",
        ));
    }
    if name != selected && inventory.notes.iter().any(|note| note.candidate.id == name) {
        return Err((-32803, "new footnote label already exists in this document"));
    }
    let mut amended = source.to_string();
    let mut edits = Vec::new();
    if name != selected {
        for note in inventory
            .notes
            .iter()
            .rev()
            .filter(|note| note.candidate.id == selected)
        {
            let span = note.candidate.label;
            if amended.len() - span.len() + name.len() > byte_budget {
                return Err((
                    -32803,
                    "renamed document exceeds the document or session byte budget",
                ));
            }
            amended.replace_range(span.start..span.end, name);
            edits.push(object([
                ("range", range(index, source, span)),
                ("newText", string(name)),
            ]));
        }
        edits.reverse();
    }
    let mut expected = inventory.document.clone();
    walk_blocks(&mut expected.blocks, &mut |symbol| {
        if let Symbol::Note(id, _) = symbol
            && id == selected
        {
            *id = name.to_string();
        }
        Ok(())
    })?;
    if parse_markdown(&amended) != expected {
        return Err((
            -32803,
            "new footnote label changes surrounding Markdown syntax",
        ));
    }
    if name != selected {
        let analysis = analyze_document_links(&expected).map_err(|_| {
            (
                -32803,
                "renamed footnote anchor analysis exceeds its limits",
            )
        })?;
        let anchor_id = format!("fn-{name}");
        if analysis.anchors.iter().any(|anchor| {
            anchor.id == anchor_id
                && (anchor.kind != AnchorKind::Footnote || anchor.occurrences != 1)
        }) {
            return Err((
                -32803,
                "new footnote label collides with an emitted document anchor",
            ));
        }
    }
    Ok(object([(
        "documentChanges",
        Json::Array(vec![object([
            (
                "textDocument",
                object([
                    ("uri", string(uri)),
                    ("version", Json::Number(f64::from(buffer.version))),
                ]),
            ),
            ("edits", Json::Array(edits)),
        ])]),
    )]))
}

/// None lets the existing heading/link provider handle non-footnote positions.
/// Footnotes are document-local, exact and case-sensitive, including dormant or
/// unresolved references. Results use label-only ranges, never enclosing blocks.
pub(super) fn request(
    method: &str,
    params: &Json,
    uri: &str,
    buffer: &Buffer,
    documents: &BTreeMap<String, Buffer>,
) -> Result<Option<Json>, Failure> {
    let source = &buffer.text;
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err((-32803, "document exceeds the footnote navigation budget"));
    }
    if !buffer.synchronized {
        return Err((-32801, "document requires full-text resynchronization"));
    }
    let requested = Position::parse(params.get("position").ok_or((-32602, "missing position"))?)
        .map_err(|reason| (-32602, reason))?;
    let index = LineIndex::new(source);
    let offset = index
        .offset(source, requested)
        .map_err(|reason| (-32602, reason))?;
    if !candidate_at_position(source, offset)? {
        return Ok(None);
    }
    let candidates = candidates(source)?;
    let mut inventory = Inventory::parse(source, candidates)?;
    let Some(selected) = inventory
        .notes
        .iter()
        .find(|note| note.candidate.marker.contains(offset))
    else {
        return Ok(None);
    };
    let selected_id = selected.candidate.id.clone();
    let selected_range = selected.candidate.label;
    let result = match method {
        "textDocument/definition" => inventory
            .notes
            .iter()
            .filter(|note| note.kind == Kind::Definition && note.candidate.id == selected_id)
            .min_by_key(|note| note.order)
            .map_or(Json::Null, |note| {
                location(uri, &index, source, note.candidate.label)
            }),
        "textDocument/references" | "textDocument/documentHighlight" => {
            let highlights = method == "textDocument/documentHighlight";
            let declarations = highlights
                || params
                    .get("context")
                    .and_then(|context| context.get("includeDeclaration"))
                    .and_then(Json::as_bool)
                    .ok_or((
                        -32602,
                        "references requires context.includeDeclaration boolean",
                    ))?;
            Json::Array(
                inventory
                    .notes
                    .iter()
                    .filter(|note| {
                        note.candidate.id == selected_id
                            && (declarations || note.kind == Kind::Reference)
                    })
                    .map(|note| {
                        if highlights {
                            object([
                                ("range", range(&index, source, note.candidate.label)),
                                (
                                    "kind",
                                    number(if note.kind == Kind::Definition { 3 } else { 2 }),
                                ),
                            ])
                        } else {
                            location(uri, &index, source, note.candidate.label)
                        }
                    })
                    .collect(),
            )
        }
        "textDocument/prepareRename" | "textDocument/rename" => {
            let session_bytes =
                check_rename_links(documents, uri, &mut inventory.document, &selected_id)?;
            if method == "textDocument/prepareRename" {
                object([
                    ("range", range(&index, source, selected_range)),
                    ("placeholder", string(&selected_id)),
                ])
            } else {
                let name = params
                    .get("newName")
                    .and_then(Json::as_str)
                    .ok_or((-32602, "rename requires string newName"))?;
                rename(
                    uri,
                    buffer,
                    &inventory,
                    &selected_id,
                    name,
                    &index,
                    session_bytes,
                )?
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

#[cfg(test)]
#[path = "footnotes_tests.rs"]
mod tests;
