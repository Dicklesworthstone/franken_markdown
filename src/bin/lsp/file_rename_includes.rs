//! Include ownership for file moves over the client's complete opened capture.
//! The shared expander owns syntax, snippets, cycles and exact source origins.
//! Before/after expanded ASTs must agree in one post-move destination space.

use std::borrow::Cow;
use std::cell::Cell;

use super::super::workspace_links::{addressed, file_uri};
use super::*;
use transclude::{ExpandedDocument, IncludeTargetSpan};

// Both sides of every opened root share these limits. Repeated includes also
// charge the complete resolver payload before cloning a selected source.
const MAX_EXPANDED_BYTES: usize = 2 * MAX_SESSION_BYTES;
const MAX_RESOLVED_BYTES: usize = 2 * MAX_SESSION_BYTES;
const MAX_EXPANDED_TOKENS: usize = 2 * MAX_TOKENS;
const MAX_EXPANDED_LINKS: usize = 2 * MAX_LINKS;

pub(super) fn inventory(parser: Parser, source: &str) -> Result<Vec<IncludeTargetSpan>, Failure> {
    transclude::include_target_spans(source, parser.profile()).map_err(|_| {
        (
            -32803,
            "file rename cannot inspect malformed or over-budget transclusion directives",
        )
    })
}

pub(super) fn without_directives<'a>(
    source: &'a str,
    tokens: &[IncludeTargetSpan],
) -> Result<Cow<'a, str>, Failure> {
    if tokens.is_empty() {
        return Ok(Cow::Borrowed(source));
    }
    let mut visible = source.as_bytes().to_vec();
    for token in tokens {
        let line = visible
            .get_mut(token.line.clone())
            .ok_or((-32803, "include directive has no exact source line"))?;
        for byte in line {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(visible)
        .map(Cow::Owned)
        .map_err(|_| (-32803, "include directive has invalid source boundaries"))
}

pub(super) fn edits(
    plan: &paths::Plan,
    origin: &str,
    tokens: &[IncludeTargetSpan],
    owners: &mut BTreeMap<FileKey, FileKey>,
    budget: &mut Budget,
    edits: &mut Vec<Edit>,
) -> Result<(), Failure> {
    for token in tokens {
        let route = plan.include_route(origin, &token.path)?;
        if let Some(previous) = owners.insert(route.after, route.before.clone())
            && previous != route.before
        {
            return Err((
                -32803,
                "file rename would capture a different include source",
            ));
        }
        if let Some(path) = route.replacement {
            let text = transclude::include_path_token(&path).map_err(|_| {
                (
                    -32803,
                    "renamed include path cannot be represented literally",
                )
            })?;
            budget.edits += 1;
            budget.edit_bytes += text.len();
            if budget.edits > MAX_EDITS || budget.edit_bytes > MAX_EDIT_BYTES {
                return Err((-32803, "file rename exceeds its edit response budget"));
            }
            edits.push(Edit {
                range: token.range.clone(),
                text,
            });
        }
    }
    Ok(())
}

struct Source<'a> {
    uri: String,
    text: &'a str,
}

#[derive(Default)]
struct ProofBudget {
    expanded_bytes: usize,
    resolved_bytes: Cell<usize>,
    tokens: usize,
    links: usize,
    link_bytes: usize,
}

fn expand(
    parser: Parser,
    root: &Source<'_>,
    sources: &BTreeMap<FileKey, Source<'_>>,
    budget: &mut ProofBudget,
) -> Result<ExpandedDocument, Failure> {
    let expanded = transclude::expand_includes_mapped_with_limit_and_profile(
        root.text,
        &root.uri,
        &|path, origin| {
            let key = paths::include_key(origin, path).map_err(|(_, reason)| reason.to_string())?;
            let source = sources.get(&key).ok_or_else(|| {
                "include source must be explicitly opened and synchronized".to_string()
            })?;
            let used = budget.resolved_bytes.get();
            if source.text.len() > MAX_RESOLVED_BYTES.saturating_sub(used) {
                return Err("include source capture exceeds the request read budget".into());
            }
            budget.resolved_bytes.set(used + source.text.len());
            Ok(Some((source.text.to_string(), source.uri.clone())))
        },
        MAX_DOCUMENT_BYTES.min(MAX_EXPANDED_BYTES.saturating_sub(budget.expanded_bytes)),
        parser.profile(),
    )
    .map_err(|_| {
        (
            -32803,
            "file rename needs a complete, valid, bounded opened transclusion graph",
        )
    })?;
    budget.expanded_bytes += expanded.text.len();
    Ok(expanded)
}

/// Validate every covered mapping segment, not just the token's endpoints.
/// Source offsets count bytes; the last byte of a Unicode scalar need not be
/// a character boundary, and matching outer origins cannot authorize a token
/// assembled through a different source in the middle.
pub(super) fn token_origin<'a>(
    expanded: &'a ExpandedDocument,
    range: &Range<usize>,
) -> Result<&'a str, Failure> {
    let failure = (
        -32803,
        "expanded destination crosses incompatible source origins",
    );
    if range.is_empty() || expanded.text.get(range.clone()).is_none() {
        return Err(failure);
    }
    let mut index = expanded
        .spans
        .partition_point(|span| span.expanded.end <= range.start);
    let first = expanded.spans.get(index).ok_or(failure)?;
    let source_id = first.source_id;
    let mut original_offset = first
        .original
        .start
        .checked_add(
            range
                .start
                .checked_sub(first.expanded.start)
                .ok_or(failure)?,
        )
        .ok_or(failure)?;
    let mut position = range.start;
    while position < range.end {
        let span = expanded.spans.get(index).ok_or(failure)?;
        if span.generated
            || span.source_id != source_id
            || span.expanded.start > position
            || span.expanded.end <= position
            || (position != range.start && span.expanded.start != position)
            || span.original.end.checked_sub(span.original.start)
                != span.expanded.end.checked_sub(span.expanded.start)
            || span
                .original
                .start
                .checked_add(position - span.expanded.start)
                != Some(original_offset)
        {
            return Err(failure);
        }
        let end = span.expanded.end.min(range.end);
        original_offset = original_offset.checked_add(end - position).ok_or(failure)?;
        position = end;
        index += 1;
    }
    expanded
        .sources
        .get(source_id)
        .map(String::as_str)
        .ok_or(failure)
}

fn canonical(
    parser: Parser,
    expanded: &ExpandedDocument,
    root: &str,
    plan: &paths::Plan,
    after_move: bool,
    budget: &mut ProofBudget,
    owners: &mut BTreeMap<FileKey, FileKey>,
) -> Result<Document, Failure> {
    let mut original = parser.parse(&expanded.text);
    let mut existing = BTreeSet::new();
    walk_destinations(&mut original.blocks, &mut |destination, _| {
        if destination.len() > paths::MAX_DESTINATION_BYTES {
            return Err((-32803, "file destination exceeds the rename byte budget"));
        }
        budget.links += 1;
        budget.link_bytes += destination.len();
        if budget.links > MAX_EXPANDED_LINKS || budget.link_bytes > 2 * MAX_LINK_BYTES {
            return Err((
                -32803,
                "expanded includes exceed the semantic destination budget",
            ));
        }
        existing.insert(destination.clone());
        Ok(())
    })?;
    let tokens = parser.destinations(&expanded.text);
    budget.tokens += tokens.len();
    if budget.tokens > MAX_EXPANDED_TOKENS {
        return Err((
            -32803,
            "expanded includes exceed the source destination budget",
        ));
    }
    let ProvenTokens {
        mut document,
        markers,
        ..
    } = proven_tokens(parser, &expanded.text, &original, &existing, &tokens)?;
    walk_destinations(&mut document.blocks, &mut |destination, role| {
        let Some(&index) = markers.get(destination) else {
            if addressed(root, destination).is_some() {
                return Err((
                    -32803,
                    "expanded file destination has no proven source token",
                ));
            }
            return Ok(());
        };
        let token = &tokens[index];
        if token.range.is_empty() {
            destination.clone_from(&token.dest);
            return Ok(());
        }
        let origin = token_origin(expanded, &token.range)?;
        let aliases = matches!(role, Role::Link);
        if !after_move {
            // Selected snippets can expose destinations hidden by code in the
            // complete source. Claim their original-to-moved identity through
            // the same ledger as ordinary links, images and include sources.
            rewrite(plan, origin, &token.dest, aliases, owners)?;
        }
        *destination = plan
            .canonical_destination(origin, &token.dest, aliases, after_move)?
            .unwrap_or_else(|| token.dest.clone());
        Ok(())
    })?;
    Ok(document)
}

pub(super) fn verify_expanded(
    parser: Parser,
    documents: &BTreeMap<String, Buffer>,
    amended: &BTreeMap<&str, String>,
    plan: &paths::Plan,
    owners: &mut BTreeMap<FileKey, FileKey>,
) -> Result<(), Failure> {
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    for (origin, buffer) in documents {
        let Some((key, false)) = file_uri(origin) else {
            if !inventory(parser, &buffer.text)?.is_empty() {
                return Err((
                    -32803,
                    "include rename requires an opened source with a file URI",
                ));
            }
            continue;
        };
        let uri = plan.moved_uri(origin)?;
        let (new_key, _) = file_uri(&uri).ok_or((
            -32803,
            "include rename cannot represent the moved source capture",
        ))?;
        before.insert(
            key,
            Source {
                uri: origin.clone(),
                text: &buffer.text,
            },
        );
        let text = amended.get(origin.as_str()).ok_or((
            -32803,
            "include rename is missing a proposed source capture",
        ))?;
        after.insert(new_key, Source { uri, text });
    }
    let mut budget = ProofBudget::default();
    for source in before.values() {
        if inventory(parser, source.text)?.is_empty() {
            continue;
        }
        let after_uri = plan.moved_uri(&source.uri)?;
        let (after_key, _) = file_uri(&after_uri).ok_or((
            -32803,
            "include rename cannot identify the proposed expansion root",
        ))?;
        let replacement = after.get(&after_key).ok_or((
            -32803,
            "include rename is missing the proposed expansion root",
        ))?;
        let original = expand(parser, source, &before, &mut budget)?;
        let updated = expand(parser, replacement, &after, &mut budget)?;
        let expected = canonical(
            parser,
            &original,
            &source.uri,
            plan,
            false,
            &mut budget,
            owners,
        )?;
        let actual = canonical(
            parser,
            &updated,
            &replacement.uri,
            plan,
            true,
            &mut budget,
            owners,
        )?;
        if expected != actual {
            return Err((
                -32803,
                "file rename cannot preserve every selected include context",
            ));
        }
    }
    Ok(())
}
