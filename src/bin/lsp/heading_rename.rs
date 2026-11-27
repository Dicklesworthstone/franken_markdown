//! Versioned heading-title edits with complete, parser-proven Markdown links.
//!
//! A source token is not a semantic use: reference definitions may be shadowed
//! or shared with images. Bulk probes establish which tokens supply real Links,
//! restore the entire original AST, then the final edits must reproduce exactly
//! the intended heading and link changes. No file or editor buffer is written.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use franken_markdown::book::validation::{
    AnchorKind, DocumentLinkAnalysis, analyze_document_links,
};
use franken_markdown::parse::DestinationSpan;
use franken_markdown::{Block, Document, Inline, SourceSpan};

use super::{
    Buffer, Json, LineIndex, MAX_DOCUMENT_BYTES, MAX_DOCUMENTS, MAX_SESSION_BYTES, Parser,
    Position, object, position, string, workspace_links,
};

type Failure = (i32, &'static str);
const MAX_NAME_BYTES: usize = 1024;
const MAX_TOKENS: usize = 8192;
const MAX_LINKS: usize = 32768;
const MAX_LINK_BYTES: usize = 1024 * 1024;
const MAX_EDITS: usize = 4096;
const MAX_EDIT_BYTES: usize = 2 * 1024 * 1024;
const MAX_URI_BYTES: usize = 1024 * 1024;
const MAX_PROBE_BYTES: usize = 4 * 1024 * 1024;

struct Heading {
    block: usize,
    content: Range<usize>,
    id: String,
    title: String,
    level: u8,
}

struct Edit {
    range: Range<usize>,
    text: String,
}

fn range_json(index: &LineIndex, source: &str, range: &Range<usize>) -> Json {
    object([
        ("start", position(index.position(source, range.start))),
        ("end", position(index.position(source, range.end))),
    ])
}

/// Only proposes a one-line ATX/setext title. A real-parser replacement below
/// must prove that this exact range owns the complete selected heading content.
fn content_candidate(source: &str, span: SourceSpan) -> Option<Range<usize>> {
    let raw = source.get(span.start..span.end)?;
    let raw = raw.trim_end_matches(['\r', '\n']);
    let mut lines = raw.split(['\r', '\n']).filter(|line| !line.is_empty());
    let first = lines.next()?;
    let second = lines.next();
    if lines.next().is_some() {
        return None;
    }
    let first_end = span.start + first.len();
    if second.is_some() {
        let trimmed = first.trim_matches([' ', '\t']);
        let start = span.start + first.len() - first.trim_start_matches([' ', '\t']).len();
        return (!trimmed.is_empty()).then_some(start..start + trimmed.len());
    }
    let indent = first.bytes().take_while(|byte| *byte == b' ').count();
    let hashes = first[indent..]
        .bytes()
        .take_while(|byte| *byte == b'#')
        .count();
    if indent > 3 || !(1..=6).contains(&hashes) {
        return None;
    }
    let mut start = span.start + indent + hashes;
    if !matches!(source.as_bytes().get(start), Some(b' ' | b'\t')) {
        return None;
    }
    let mut end = first_end;
    while end > start && matches!(source.as_bytes()[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    let mut closing = end;
    while closing > start && source.as_bytes()[closing - 1] == b'#' {
        closing -= 1;
    }
    if closing < end && closing > start && matches!(source.as_bytes()[closing - 1], b' ' | b'\t') {
        end = closing;
    }
    while start < end && matches!(source.as_bytes()[start], b' ' | b'\t') {
        start += 1;
    }
    while end > start && matches!(source.as_bytes()[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    Some(start..end)
}

enum HeadingOrigin<'a> {
    Caret(usize),
    Destination(&'a str),
}

type SelectedHeading = (Heading, Document, DocumentLinkAnalysis);

fn select_heading(
    parser: Parser,
    source: &str,
    origin: HeadingOrigin<'_>,
) -> Result<Option<SelectedHeading>, Failure> {
    let parsed = parser.spanned(source);
    let spans: Vec<_> = parsed.blocks.iter().map(|block| block.span).collect();
    let document = parsed.into_document();
    let analyze = || {
        analyze_document_links(&document)
            .map_err(|_| (-32803, "heading anchor analysis exceeds its limits"))
    };
    let (block, analysis) = match origin {
        HeadingOrigin::Caret(offset) => {
            let Some(block) = spans.iter().position(|span| {
                span.contains(offset)
                    || (offset == span.end
                        && (offset == source.len()
                            || matches!(source.as_bytes().get(offset), Some(b'\r' | b'\n'))))
            }) else {
                return Ok(None);
            };
            if !matches!(document.blocks[block], Block::Heading { .. }) {
                return Ok(None);
            }
            (block, analyze()?)
        }
        HeadingOrigin::Destination(destination) => {
            let analysis = analyze()?;
            let Ok(Some(anchor)) = analysis.resolve_fragment(destination) else {
                return Ok(None);
            };
            if anchor.kind != AnchorKind::Heading || anchor.occurrences != 1 {
                return Ok(None);
            }
            (anchor.block_index, analysis)
        }
    };
    let Some(Block::Heading { level, .. }) = document.blocks.get(block) else {
        // A nested heading has a container as its source owner.
        return Ok(None);
    };
    let level = *level;
    let span = spans[block];
    let Some(content) = content_candidate(source, span) else {
        return Ok(None);
    };
    if content.len() > 64 * 1024 {
        return Err((-32803, "heading source exceeds the rename budget"));
    }
    let probe = "fmd-heading-title-source-probe";
    let mut amended = source.to_string();
    amended.replace_range(content.clone(), probe);
    let mut expected = document.clone();
    expected.blocks[block] = Block::Heading {
        level,
        inlines: vec![Inline::Text(probe.to_string())],
    };
    if parser.parse(&amended) != expected {
        return Ok(None);
    }
    let mut anchors = analysis
        .anchors
        .iter()
        .filter(|anchor| anchor.kind == AnchorKind::Heading && anchor.block_index == block);
    let Some(anchor) = anchors.next() else {
        // A colliding footnote can own the merged anchor entry even though
        // the parser proved that this source declaration is a heading.
        return Err((-32803, "heading anchor is unavailable or ambiguous"));
    };
    if anchors.next().is_some() || anchor.occurrences != 1 {
        return Err((-32803, "heading anchor is ambiguous"));
    }
    Ok(Some((
        Heading {
            block,
            content,
            id: anchor.id.clone(),
            title: anchor.title.clone(),
            level,
        },
        document,
        analysis,
    )))
}

fn walk_links(
    blocks: &mut [Block],
    visit: &mut impl FnMut(&mut String) -> Result<(), Failure>,
) -> Result<(), Failure> {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                walk_inline_links(inlines, visit)?;
            }
            Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => {
                walk_links(blocks, visit)?;
            }
            Block::List(list) => {
                for item in &mut list.items {
                    walk_links(&mut item.blocks, visit)?;
                }
            }
            Block::Table(table) => {
                for cell in table.head.iter_mut().chain(table.rows.iter_mut().flatten()) {
                    walk_inline_links(cell, visit)?;
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        walk_inline_links(inlines, visit)?;
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

fn walk_inline_links(
    inlines: &mut [Inline],
    visit: &mut impl FnMut(&mut String) -> Result<(), Failure>,
) -> Result<(), Failure> {
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => {
                visit(dest)?;
                walk_inline_links(content, visit)?;
            }
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => walk_inline_links(children, visit)?,
            _ => {}
        }
    }
    Ok(())
}

fn ensure_unique_uri(documents: &BTreeMap<String, Buffer>, uri: &str) -> Result<(), Failure> {
    let singleton = BTreeMap::from([(
        uri.to_string(),
        Buffer {
            text: String::new(),
            version: 0,
            synchronized: true,
        },
    )]);
    // Opaque buffers have no inferred file identity and remain document-local.
    if workspace_links::resolve(&singleton, uri, uri)?.is_some()
        && workspace_links::resolve(documents, uri, uri)?.is_none()
    {
        return Err((
            -32803,
            "heading rename cannot edit ambiguous file URI aliases",
        ));
    }
    Ok(())
}

struct Target<'a> {
    uri: &'a str,
    id: &'a str,
    analysis: &'a DocumentLinkAnalysis,
    documents: &'a BTreeMap<String, Buffer>,
    singleton: BTreeMap<String, Buffer>,
}

impl Target<'_> {
    fn matches(&self, origin: &str, destination: &str) -> Result<bool, Failure> {
        if !matches!(self.analysis.resolve_fragment(destination), Ok(Some(anchor)) if anchor.id == self.id)
        {
            return Ok(false);
        }
        let destination =
            destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
        if destination.starts_with(['#', '?']) {
            return Ok(origin == self.uri);
        }
        let resolved = workspace_links::resolve(self.documents, origin, destination)?;
        if let Some((uri, _)) = resolved {
            return Ok(uri == self.uri);
        }
        // A target-only match means the full source set made the path ambiguous.
        // Refusing preserves completeness rather than omitting a possible use.
        if workspace_links::resolve(&self.singleton, origin, destination)?.is_some() {
            return Err((-32803, "a heading reference has an ambiguous target path"));
        }
        Ok(false)
    }
}

fn apply(source: &str, edits: &[Edit], limit: usize) -> Result<String, Failure> {
    let mut length = source.len();
    let mut previous = 0;
    for edit in edits {
        if edit.range.start < previous || source.get(edit.range.clone()).is_none() {
            return Err((
                -32803,
                "heading rename source ranges overlap or are invalid",
            ));
        }
        length = length
            .checked_sub(edit.range.len())
            .and_then(|n| n.checked_add(edit.text.len()))
            .ok_or((-32803, "heading rename exceeds the source byte budget"))?;
        previous = edit.range.end;
    }
    if length > limit {
        return Err((-32803, "heading rename exceeds the source byte budget"));
    }
    let mut output = String::with_capacity(length);
    let mut previous = 0;
    for edit in edits {
        output.push_str(&source[previous..edit.range.start]);
        output.push_str(&edit.text);
        previous = edit.range.end;
    }
    output.push_str(&source[previous..]);
    Ok(output)
}

fn probe_tokens(
    source: &str,
    tokens: &[DestinationSpan],
    prefix: &str,
    selected: impl Fn(usize) -> bool,
) -> Result<String, Failure> {
    let edits: Vec<_> = tokens
        .iter()
        .enumerate()
        .filter(|(i, _)| selected(*i))
        .map(|(i, token)| Edit {
            range: token.range.clone(),
            text: format!("{prefix}{i}"),
        })
        .collect();
    apply(source, &edits, MAX_PROBE_BYTES)
}

fn proven_tokens(
    parser: Parser,
    source: &str,
    original: &Document,
    tokens: Vec<DestinationSpan>,
) -> Result<Vec<DestinationSpan>, Failure> {
    if tokens.is_empty() {
        return Ok(tokens);
    }
    let mut existing = BTreeSet::new();
    let mut copy = original.clone();
    walk_links(&mut copy.blocks, &mut |dest| {
        existing.insert(dest.clone());
        Ok(())
    })?;
    let prefix = (0..32)
        .map(|attempt| format!("#fmd-heading-rename-probe-{attempt}-"))
        .find(|prefix| {
            !source.contains(prefix.as_str()) && !existing.iter().any(|dest| dest.contains(prefix))
        })
        .ok_or((-32803, "cannot select an unambiguous heading rename probe"))?;
    let markers: BTreeMap<_, _> = (0..tokens.len())
        .map(|i| (format!("{prefix}{i}"), i))
        .collect();
    let probed = probe_tokens(source, &tokens, &prefix, |_| true)?;
    let mut parsed = parser.parse(&probed);
    let mut selected = BTreeSet::new();
    walk_links(&mut parsed.blocks, &mut |dest| {
        if let Some(index) = markers.get(dest) {
            selected.insert(*index);
        }
        Ok(())
    })?;
    let probed = probe_tokens(source, &tokens, &prefix, |i| selected.contains(&i))?;
    let mut restored = parser.parse(&probed);
    walk_links(&mut restored.blocks, &mut |dest| {
        if let Some(&index) = markers.get(dest) {
            dest.clone_from(&tokens[index].dest);
        }
        Ok(())
    })?;
    if restored != *original {
        return Err((
            -32803,
            "a heading destination is shared with an image or cannot be edited independently",
        ));
    }
    Ok(tokens
        .into_iter()
        .enumerate()
        .filter_map(|(i, token)| selected.contains(&i).then_some(token))
        .collect())
}

fn literal_heading(name: &str) -> Result<String, Failure> {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name.trim() != name
        || name
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}'))
    {
        return Err((
            -32602,
            "newName must be 1..1024 UTF-8 bytes of single-line heading text without outer whitespace",
        ));
    }
    let mut escaped = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    Ok(escaped)
}

fn encoded_fragment(id: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

fn changed_heading(
    original: &Document,
    old_analysis: &DocumentLinkAnalysis,
    heading: &Heading,
    name: &str,
) -> Result<(Document, String), Failure> {
    let mut document = original.clone();
    document.blocks[heading.block] = Block::Heading {
        level: heading.level,
        inlines: vec![Inline::Text(name.to_string())],
    };
    let new_analysis = analyze_document_links(&document)
        .map_err(|_| (-32803, "renamed heading anchor analysis exceeds its limits"))?;
    let selected = |anchor: &&franken_markdown::book::validation::DocumentAnchor| {
        anchor.kind == AnchorKind::Heading && anchor.block_index == heading.block
    };
    let mut anchors = new_analysis.anchors.iter().filter(selected);
    let anchor = anchors
        .next()
        .ok_or((-32803, "renamed heading has no emitted anchor"))?;
    if anchors.next().is_some() || anchor.occurrences != 1 {
        return Err((-32803, "renamed heading collides with an emitted anchor"));
    }
    if old_analysis
        .anchors
        .iter()
        .filter(|a| !selected(a))
        .ne(new_analysis.anchors.iter().filter(|a| !selected(a)))
    {
        return Err((
            -32803,
            "renaming this heading would change other heading or footnote anchors",
        ));
    }
    if old_analysis
        .heading_aliases
        .entries()
        .filter(|(_, id)| *id != heading.id)
        .ne(new_analysis
            .heading_aliases
            .entries()
            .filter(|(_, id)| *id != anchor.id))
    {
        return Err((
            -32803,
            "renaming this heading would change other heading compatibility anchors",
        ));
    }
    Ok((document, anchor.id.clone()))
}

/// Called only for rename/prepareRename after the footnote provider declines.
/// Resolve only a parser-proven fragment token. Image destinations and
/// shadowed reference definitions must never select a heading.
fn linked_heading<'a>(
    parser: Parser,
    uri: &'a str,
    source: &str,
    offset: usize,
    documents: &'a BTreeMap<String, Buffer>,
) -> Result<Option<(&'a str, SelectedHeading, Range<usize>)>, Failure> {
    let tokens = parser.destinations(source);
    if tokens.len() > MAX_TOKENS {
        return Err((
            -32803,
            "source destinations exceed the heading rename budget",
        ));
    }
    let Some(token) = tokens
        .into_iter()
        .find(|token| token.range.contains(&offset))
    else {
        return Ok(None);
    };
    if token.dest.len() > 8192 {
        return Err((-32803, "link exceeds the heading rename budget"));
    }
    let Some(fragment) = token
        .fragment_range(source)
        .filter(|range| range.contains(&offset))
    else {
        return Ok(None);
    };
    let original = parser.parse(source);
    let Some(proven) = proven_tokens(parser, source, &original, vec![token])?.pop() else {
        return Ok(None);
    };
    let dest = proven
        .dest
        .trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    let (target_uri, target_source) = if dest.starts_with(['#', '?']) {
        (uri, source)
    } else if let Some(target) = workspace_links::resolve(documents, uri, dest)? {
        target
    } else {
        return Ok(None);
    };
    if target_source.len() > MAX_DOCUMENT_BYTES {
        return Err((-32803, "document exceeds the heading rename budget"));
    }
    let selected = select_heading(parser, target_source, HeadingOrigin::Destination(dest))?;
    Ok(selected.map(|heading| (target_uri, heading, fragment)))
}

pub(super) fn request(
    parser: Parser,
    method: &str,
    params: &Json,
    uri: &str,
    buffer: &Buffer,
    documents: &BTreeMap<String, Buffer>,
) -> Result<Json, Failure> {
    let source = &buffer.text;
    if !buffer.synchronized {
        return Err((-32801, "document requires full-text resynchronization"));
    }
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err((-32803, "document exceeds the heading rename budget"));
    }
    let requested = Position::parse(params.get("position").ok_or((-32602, "missing position"))?)
        .map_err(|reason| (-32602, reason))?;
    let index = LineIndex::new(source);
    let offset = index
        .offset(source, requested)
        .map_err(|reason| (-32602, reason))?;
    let selection = select_heading(parser, source, HeadingOrigin::Caret(offset))?;
    let (target_uri, (heading, original, analysis), trigger) = if let Some(selected) = selection {
        let trigger = selected.0.content.clone();
        (uri, selected, trigger)
    } else if let Some(linked) = linked_heading(parser, uri, source, offset, documents)? {
        linked
    } else {
        return Ok(Json::Null);
    };
    if documents.len() > MAX_DOCUMENTS {
        return Err((-32803, "workspace exceeds the heading rename budget"));
    }
    let mut session_bytes = 0;
    for document in documents.values() {
        if !document.synchronized {
            return Err((
                -32801,
                "heading rename requires synchronized open documents",
            ));
        }
        if document.text.len() > MAX_DOCUMENT_BYTES
            || document.text.len() > MAX_SESSION_BYTES.saturating_sub(session_bytes)
        {
            return Err((-32803, "workspace exceeds the heading rename budget"));
        }
        session_bytes += document.text.len();
    }
    ensure_unique_uri(documents, uri)?;
    if uri != target_uri {
        ensure_unique_uri(documents, target_uri)?;
    }
    let uri = target_uri;
    if method == "textDocument/prepareRename" {
        return Ok(object([
            ("range", range_json(&index, source, &trigger)),
            ("placeholder", string(&heading.title)),
        ]));
    }
    let name = params
        .get("newName")
        .and_then(Json::as_str)
        .ok_or((-32602, "rename requires string newName"))?;
    let escaped = literal_heading(name)?;
    if name == heading.title {
        return Ok(object([("documentChanges", Json::Array(Vec::new()))]));
    }
    let (renamed, new_id) = changed_heading(&original, &analysis, &heading, name)?;
    let fragment = encoded_fragment(&new_id);
    let target = Target {
        uri,
        id: &heading.id,
        analysis: &analysis,
        documents,
        singleton: BTreeMap::from([(
            uri.to_string(),
            Buffer {
                text: String::new(),
                version: 0,
                synchronized: true,
            },
        )]),
    };
    let mut changes = Vec::new();
    let mut link_count = 0;
    let mut link_bytes = 0;
    let mut token_count = 0;
    let mut edit_count = 0;
    let mut edit_bytes = 0;
    let mut uri_bytes = 0;
    let mut renamed_bytes = 0;
    for (origin, buffer) in documents {
        let source = &buffer.text;
        let document = if origin == uri {
            original.clone()
        } else {
            parser.parse(source)
        };
        let mut expected = if origin == uri {
            renamed.clone()
        } else {
            document.clone()
        };
        let mut affected = false;
        walk_links(&mut expected.blocks, &mut |dest| {
            link_count += 1;
            link_bytes += dest.len();
            if link_count > MAX_LINKS || link_bytes > MAX_LINK_BYTES || dest.len() > 8192 {
                return Err((-32803, "links exceed the heading rename budget"));
            }
            if target.matches(origin, dest)? {
                let separator = dest
                    .find('#')
                    .ok_or((-32803, "heading link has no fragment"))?;
                // Punctuation-only renames can remove an alias while keeping
                // the canonical ID. Stabilize those links too, preserving exact
                // source spellings that already address the final identifier.
                if franken_markdown::anchors::decode_fragment(&dest[separator + 1..]).as_deref()
                    != Some(new_id.as_str())
                {
                    dest.truncate(separator + 1);
                    dest.push_str(&fragment);
                    affected = true;
                }
            }
            Ok(())
        })?;
        let mut edits = Vec::new();
        if origin == uri {
            edits.push(Edit {
                range: heading.content.clone(),
                text: escaped.clone(),
            });
        }
        if affected {
            let mut candidates = Vec::new();
            for token in parser.destinations(source) {
                token_count += 1;
                if token_count > MAX_TOKENS || token.dest.len() > 8192 {
                    return Err((
                        -32803,
                        "source destinations exceed the heading rename budget",
                    ));
                }
                if origin == uri
                    && token.range.start >= heading.content.start
                    && token.range.end <= heading.content.end
                {
                    continue;
                }
                if target.matches(origin, &token.dest)?
                    && token
                        .dest
                        .split_once('#')
                        .and_then(|(_, fragment)| {
                            franken_markdown::anchors::decode_fragment(fragment)
                        })
                        .as_deref()
                        != Some(new_id.as_str())
                {
                    candidates.push(token);
                }
            }
            for token in proven_tokens(parser, source, &document, candidates)? {
                let range = token.fragment_range(source).ok_or((
                    -32803,
                    "heading destination has no independently editable fragment",
                ))?;
                edits.push(Edit {
                    range,
                    text: fragment.clone(),
                });
            }
        }
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        if edits.is_empty() && !affected {
            renamed_bytes += source.len();
            if renamed_bytes > MAX_SESSION_BYTES {
                return Err((-32803, "renamed workspace exceeds the session byte budget"));
            }
            continue;
        }
        let amended = apply(source, &edits, MAX_DOCUMENT_BYTES)?;
        renamed_bytes += amended.len();
        if renamed_bytes > MAX_SESSION_BYTES {
            return Err((-32803, "renamed workspace exceeds the session byte budget"));
        }
        if parser.parse(&amended) != expected {
            return Err((
                -32803,
                "heading rename cannot preserve every Markdown link and surrounding syntax",
            ));
        }
        if edits.is_empty() {
            continue;
        }
        ensure_unique_uri(documents, origin)?;
        edit_count += edits.len();
        edit_bytes += edits.iter().map(|edit| edit.text.len()).sum::<usize>();
        uri_bytes += origin.len();
        if edit_count > MAX_EDITS || edit_bytes > MAX_EDIT_BYTES || uri_bytes > MAX_URI_BYTES {
            return Err((-32803, "heading rename exceeds the response budget"));
        }
        let index = LineIndex::new(source);
        changes.push(object([
            (
                "textDocument",
                object([
                    ("uri", string(origin)),
                    ("version", Json::Number(f64::from(buffer.version))),
                ]),
            ),
            (
                "edits",
                Json::Array(
                    edits
                        .iter()
                        .map(|edit| {
                            object([
                                ("range", range_json(&index, source, &edit.range)),
                                ("newText", string(&edit.text)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]));
    }
    Ok(object([("documentChanges", Json::Array(changes))]))
}

#[cfg(test)]
#[path = "heading_rename_tests.rs"]
mod tests;
