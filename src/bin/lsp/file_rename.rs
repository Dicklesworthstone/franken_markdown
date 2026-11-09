//! Versioned Markdown destination edits before client-owned file/folder moves.
//!
//! The normal parser owns every edited token. A second parse must reproduce
//! exactly the intended Link/Image destinations and all other original syntax.
//! The server neither moves files nor changes the authoritative open buffers.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use franken_markdown::parse::{DestinationSpan, destination_spans};
use franken_markdown::{Block, Document, Inline, parse_markdown, transclude};

use super::workspace_links::FileKey;
use super::{
    Buffer, Json, LineIndex, MAX_DOCUMENT_BYTES, MAX_DOCUMENTS, MAX_SESSION_BYTES, object,
    position, string,
};

type Failure = (i32, &'static str);

#[path = "file_rename_paths.rs"]
mod paths;

const MAX_TOKENS: usize = 8192;
const MAX_LINKS: usize = 32768;
const MAX_LINK_BYTES: usize = 1024 * 1024;
const MAX_EDITS: usize = 4096;
const MAX_EDIT_BYTES: usize = 2 * 1024 * 1024;
const MAX_URI_BYTES: usize = 1024 * 1024;
const MAX_PROBE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
enum Role {
    Link,
    Image,
}

#[derive(Clone, Copy, Default)]
struct Roles {
    link: bool,
    image: bool,
}

struct Edit {
    range: Range<usize>,
    text: String,
}

#[derive(Default)]
struct Budget {
    tokens: usize,
    links: usize,
    link_bytes: usize,
    edits: usize,
    edit_bytes: usize,
    uri_bytes: usize,
    source_bytes: usize,
}

fn walk_destinations(
    blocks: &mut [Block],
    visit: &mut impl FnMut(&mut String, Role) -> Result<(), Failure>,
) -> Result<(), Failure> {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                walk_inlines(inlines, visit)?;
            }
            Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => {
                walk_destinations(blocks, visit)?;
            }
            Block::List(list) => {
                for item in &mut list.items {
                    walk_destinations(&mut item.blocks, visit)?;
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
    visit: &mut impl FnMut(&mut String, Role) -> Result<(), Failure>,
) -> Result<(), Failure> {
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => {
                visit(dest, Role::Link)?;
                walk_inlines(content, visit)?;
            }
            Inline::Image { dest, .. } => visit(dest, Role::Image)?,
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => walk_inlines(children, visit)?,
            _ => {}
        }
    }
    Ok(())
}

fn rewrite(
    plan: &paths::Plan,
    origin: &str,
    destination: &str,
    aliases: bool,
    owners: &mut BTreeMap<FileKey, FileKey>,
) -> Result<Option<String>, Failure> {
    if destination.len() > paths::MAX_DESTINATION_BYTES {
        return Err((-32803, "file destination exceeds the rename byte budget"));
    }
    let Some(route) = plan.route(origin, destination, aliases)? else {
        return Ok(None);
    };
    if let Some(previous) = owners.insert(route.after, route.before.clone())
        && previous != route.before
    {
        return Err((
            -32803,
            "file rename would merge distinct destinations used by opened documents",
        ));
    }
    Ok(route.replacement)
}

fn literal_destination(destination: &str) -> String {
    let mut result = String::new();
    for ch in destination.chars() {
        if ch.is_whitespace() || ch.is_control() {
            result.push_str(&format!("&#{};", u32::from(ch)));
        } else {
            if matches!(ch, '\\' | '(' | ')' | '<' | '>' | '&') {
                result.push('\\');
            }
            result.push(ch);
        }
    }
    result
}

fn token_text(source: &str, token: &DestinationSpan, destination: &str) -> Result<String, Failure> {
    let raw = source.get(token.range.clone()).ok_or((
        -32803,
        "file destination has no exact original source range",
    ))?;
    let (angle, body) = if raw.starts_with('<') {
        (
            true,
            raw.strip_prefix('<')
                .and_then(|raw| raw.strip_suffix('>'))
                .ok_or((-32803, "file destination has invalid angle delimiters"))?,
        )
    } else {
        (false, raw)
    };
    let separator = token.dest.find(['?', '#']).unwrap_or(token.dest.len());
    let suffix = &token.dest[separator..];
    // Ordinary query/fragment spelling can stay byte-for-byte intact. If it
    // was entity-encoded or Markdown-escaped, serialize the same decoded value
    // safely; the whole-document proof below checks its exact semantics.
    let body = if body.ends_with(suffix) {
        destination.to_string()
    } else {
        literal_destination(destination)
    };
    Ok(if angle { format!("<{body}>") } else { body })
}

fn apply(source: &str, edits: &[Edit], limit: usize) -> Result<String, Failure> {
    let mut length = source.len();
    let mut previous = 0;
    for edit in edits {
        if edit.range.start < previous || source.get(edit.range.clone()).is_none() {
            return Err((-32803, "file rename source ranges overlap or are invalid"));
        }
        length = length
            .checked_sub(edit.range.len())
            .and_then(|length| length.checked_add(edit.text.len()))
            .ok_or((-32803, "file rename exceeds the document byte budget"))?;
        previous = edit.range.end;
    }
    if length > limit {
        return Err((-32803, "file rename exceeds the document byte budget"));
    }
    let mut result = String::with_capacity(length);
    previous = 0;
    for edit in edits {
        result.push_str(&source[previous..edit.range.start]);
        result.push_str(&edit.text);
        previous = edit.range.end;
    }
    result.push_str(&source[previous..]);
    Ok(result)
}

fn proven_roles(
    source: &str,
    original: &Document,
    existing: &BTreeSet<String>,
    tokens: &[DestinationSpan],
) -> Result<Vec<Roles>, Failure> {
    let mut roles = vec![Roles::default(); tokens.len()];
    if tokens.is_empty() {
        return Ok(roles);
    }
    let prefix = (0..32)
        .map(|attempt| format!("#fmd-file-rename-token-{attempt}-"))
        .find(|prefix| {
            !source.contains(prefix.as_str())
                && !existing
                    .iter()
                    .any(|destination| destination.contains(prefix))
                && !tokens.iter().any(|token| token.dest.contains(prefix))
        })
        .ok_or((-32803, "cannot reserve an unambiguous file rename probe"))?;
    let markers: BTreeMap<_, _> = (0..tokens.len())
        .map(|index| (format!("{prefix}{index}"), index))
        .collect();
    let edits: Vec<_> = tokens
        .iter()
        .enumerate()
        .map(|(index, token)| Edit {
            range: token.range.clone(),
            text: format!("{prefix}{index}"),
        })
        .collect();
    let amended = apply(source, &edits, MAX_PROBE_BYTES)?;
    let mut restored = parse_markdown(&amended);
    walk_destinations(&mut restored.blocks, &mut |destination, role| {
        if let Some(&index) = markers.get(destination) {
            match role {
                Role::Link => roles[index].link = true,
                Role::Image => roles[index].image = true,
            }
            destination.clone_from(&tokens[index].dest);
        }
        Ok(())
    })?;
    if restored != *original {
        return Err((
            -32803,
            "file destination source ownership could not be proven complete",
        ));
    }
    Ok(roles)
}

fn token_rewrite(
    plan: &paths::Plan,
    origin: &str,
    token: &DestinationSpan,
    roles: Roles,
    owners: &mut BTreeMap<FileKey, FileKey>,
) -> Result<Option<String>, Failure> {
    match (roles.link, roles.image) {
        (true, false) => rewrite(plan, origin, &token.dest, true, owners),
        (false, true) => rewrite(plan, origin, &token.dest, false, owners),
        // A shared definition must preserve both roles. An unused definition
        // has no established role, so only an edit valid for both is admitted.
        _ => {
            let link = rewrite(plan, origin, &token.dest, true, owners)?;
            let image = rewrite(plan, origin, &token.dest, false, owners)?;
            if link != image {
                return Err((
                    -32803,
                    "reference destination has incompatible chapter-link and image-resource targets",
                ));
            }
            Ok(link)
        }
    }
}

fn no_active_includes(source: &str) -> Result<(), Failure> {
    if transclude::has_includes(source)
        && transclude::expand_includes_with_limit(
            source,
            &|_, _| Err("file rename does not resolve include sources".to_string()),
            MAX_DOCUMENT_BYTES,
        )
        .is_err()
    {
        // The real expander owns directive/code recognition. Its refusing
        // resolver never reads a file, and cannot invent selected snippet text.
        return Err((
            -32803,
            "file rename cannot maintain active or malformed transclusion directives",
        ));
    }
    Ok(())
}

pub(super) fn request(
    documents: &BTreeMap<String, Buffer>,
    params: &Json,
) -> Result<Json, Failure> {
    if documents.len() > MAX_DOCUMENTS {
        return Err((-32803, "workspace exceeds the file rename document budget"));
    }
    let plan = paths::Plan::new(params, documents)?;
    if plan.is_empty() {
        return Ok(Json::Null);
    }
    let mut source_bytes = 0usize;
    for buffer in documents.values() {
        if !buffer.synchronized {
            return Err((-32801, "file rename requires synchronized open documents"));
        }
        if buffer.text.len() > MAX_DOCUMENT_BYTES
            || buffer.text.len() > MAX_SESSION_BYTES.saturating_sub(source_bytes)
        {
            return Err((-32803, "workspace exceeds the file rename source budget"));
        }
        source_bytes += buffer.text.len();
        no_active_includes(&buffer.text)?;
    }
    let mut budget = Budget::default();
    let mut owners = BTreeMap::new();
    let mut changes = Vec::new();
    for (origin, buffer) in documents {
        let source = &buffer.text;
        let original = parse_markdown(source);
        let mut expected = original.clone();
        let mut existing = BTreeSet::new();
        let mut semantic_change = false;
        walk_destinations(&mut expected.blocks, &mut |destination, role| {
            budget.links += 1;
            budget.link_bytes += destination.len();
            if budget.links > MAX_LINKS || budget.link_bytes > MAX_LINK_BYTES {
                return Err((
                    -32803,
                    "file rename exceeds its semantic destination budget",
                ));
            }
            existing.insert(destination.clone());
            if let Some(replacement) = rewrite(
                &plan,
                origin,
                destination,
                matches!(role, Role::Link),
                &mut owners,
            )? {
                *destination = replacement;
                semantic_change = true;
            }
            Ok(())
        })?;
        let mut edits = Vec::new();
        let tokens = destination_spans(source);
        budget.tokens += tokens.len();
        if budget.tokens > MAX_TOKENS {
            return Err((-32803, "file rename exceeds its source destination budget"));
        }
        let roles = proven_roles(source, &original, &existing, &tokens)?;
        for (token, roles) in tokens.into_iter().zip(roles) {
            if let Some(destination) = token_rewrite(&plan, origin, &token, roles, &mut owners)? {
                let text = token_text(source, &token, &destination)?;
                if source.get(token.range.clone()) != Some(text.as_str()) {
                    budget.edits += 1;
                    budget.edit_bytes += text.len();
                    if budget.edits > MAX_EDITS || budget.edit_bytes > MAX_EDIT_BYTES {
                        return Err((-32803, "file rename exceeds its edit response budget"));
                    }
                    edits.push(Edit {
                        range: token.range,
                        text,
                    });
                }
            }
        }
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let amended = apply(source, &edits, MAX_DOCUMENT_BYTES)?;
        if (semantic_change || !edits.is_empty()) && parse_markdown(&amended) != expected {
            return Err((
                -32803,
                "file rename cannot preserve every affected destination and surrounding Markdown",
            ));
        }
        budget.source_bytes += amended.len();
        if budget.source_bytes > MAX_SESSION_BYTES {
            return Err((-32803, "renamed workspace exceeds the session byte budget"));
        }
        if edits.is_empty() {
            continue;
        }
        budget.uri_bytes += origin.len();
        if budget.uri_bytes > MAX_URI_BYTES {
            return Err((-32803, "file rename exceeds its URI response budget"));
        }
        let index = LineIndex::new(source);
        changes.push(object([
            (
                "textDocument",
                object([
                    // willRenameFiles edits apply BEFORE the actual move.
                    ("uri", string(origin)),
                    ("version", Json::Number(f64::from(buffer.version))),
                ]),
            ),
            (
                "edits",
                Json::Array(
                    edits
                        .into_iter()
                        .map(|edit| {
                            object([
                                (
                                    "range",
                                    object([
                                        (
                                            "start",
                                            position(index.position(source, edit.range.start)),
                                        ),
                                        ("end", position(index.position(source, edit.range.end))),
                                    ]),
                                ),
                                ("newText", string(&edit.text)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]));
    }
    Ok(if changes.is_empty() {
        Json::Null
    } else {
        object([("documentChanges", Json::Array(changes))])
    })
}

pub(super) fn capability() -> Json {
    object([(
        "fileOperations",
        object([(
            "willRename",
            object([(
                "filters",
                Json::Array(vec![object([
                    ("scheme", string("file")),
                    // Resource moves can affect Markdown images as well as
                    // chapter links; folders contain arbitrary open chapters.
                    ("pattern", object([("glob", string("**/*"))])),
                ])]),
            )]),
        )]),
    )])
}

#[cfg(test)]
#[path = "file_rename_tests.rs"]
mod tests;
