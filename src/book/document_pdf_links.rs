//! Standalone PDF links to the public note IDs used by HTML and EPUB.
//!
//! Ordinary note links must not become citations: doing so would move page
//! notes, change their first return destination, and paint link text as a
//! superscript. Reuse the book writer's temporary actions and final-destination
//! binding, without inventing a chapter path or resolving host-owned file URLs.

use super::{PREFIX, Target, reserve_link_nonce, walk_links, wire};
use crate::book::validation::{AnchorKind, footnote_anchors};
use crate::pdf::source_navigation::{self, NavigationRender};
use crate::{Block, Document, Inline, PdfASettings, PdfEmitOptions, PdfOptions, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Return `None` for documents without a link to an emitted public note anchor.
/// Their ordinary borrowed transformation/render path stays unchanged.
pub(crate) fn render(
    source: &Document,
    options: &PdfOptions,
    settings: PdfASettings,
    emit: PdfEmitOptions,
    profiled: bool,
) -> Result<Option<NavigationRender>> {
    let mut candidate = false;
    visit_links(&source.blocks, &mut |destination| {
        candidate |= note_fragment(destination).is_some();
    });
    if !candidate {
        return Ok(None);
    }
    let source = crate::apply_profile(source, options.profile);
    let aliases: BTreeMap<_, _> = footnote_anchors(&source)
        .into_iter()
        .map(|anchor| {
            let target = if anchor.occurrences != 1 {
                // A source heading and note sharing an HTML ID are ambiguous.
                // Neither the generated heading slug nor the note may win.
                None
            } else {
                match anchor.kind {
                    AnchorKind::Footnote => Some(Target::Footnote(anchor.title)),
                    AnchorKind::FootnoteBackreference => {
                        Some(Target::FootnoteReference(anchor.title))
                    }
                    AnchorKind::Heading => None,
                }
            };
            (anchor.id, target)
        })
        .collect();
    let mut addressed = false;
    let mut reserved = BTreeSet::new();
    visit_links(&source.blocks, &mut |destination| {
        reserve_link_nonce(destination, &mut reserved);
        addressed |= note_fragment(destination).is_some_and(|id| aliases.contains_key(&id));
    });
    if !addressed {
        // Unreferenced definitions do not publish HTML anchors. In particular,
        // an explicit link alone cannot make a note into a body citation.
        return Ok(None);
    }
    let nonce = (0..=reserved.len())
        .find(|candidate| !reserved.contains(candidate))
        .ok_or_else(|| super::invalid("temporary link namespace exhausted"))?;
    let prefix = format!("{PREFIX}{nonce}/");
    let mut document = crate::safe_html::lower(&source).into_owned();
    let note_ids = crate::footnotes::page_note_ids(&document);
    let mut tokens = BTreeMap::<Option<Target>, String>::new();
    walk_links(&mut document.blocks, &mut |destination| {
        let Some(target) = note_fragment(destination).and_then(|id| aliases.get(&id)) else {
            return;
        };
        if let Some(token) = tokens.get(target) {
            destination.clone_from(token);
        } else {
            // The binder replaces this action in place after pagination.
            let token = format!("{prefix}{:020}/{}", tokens.len(), "0".repeat(128));
            tokens.insert(target.clone(), token.clone());
            *destination = token;
        }
    });
    let prepared =
        crate::footnotes::for_pdf_paged_with_backlinks(&document, options.footnote_backlinks);
    let mut rendered =
        source_navigation::render_complete(&prepared, options, settings, emit, profiled)?;
    let targets = tokens
        .into_iter()
        .map(|(target, token)| {
            let destination = match target {
                Some(Target::Footnote(id)) => note_ids
                    .get(&id)
                    .and_then(|prepared| rendered.navigation.notes.get(prepared))
                    .copied(),
                Some(Target::FootnoteReference(id)) => note_ids
                    .get(&id)
                    .and_then(|prepared| rendered.navigation.note_references.get(prepared))
                    .copied(),
                Some(Target::Heading(_)) | None => None,
            };
            (token, destination)
        })
        .collect();
    rendered.bytes = wire::bind(rendered.bytes, &targets)?;
    Ok(Some(rendered))
}

fn note_fragment(destination: &str) -> Option<String> {
    let destination =
        destination.trim_matches(|ch: char| ch.is_ascii_whitespace() || ch.is_control());
    if !destination.starts_with(['#', '?']) {
        return None;
    }
    let (_, fragment) = destination.split_once('#')?;
    if !fragment.contains('%') && !fragment.starts_with("fn-") && !fragment.starts_with("fnref-") {
        return None;
    }
    let fragment = crate::anchors::decode_fragment(fragment)?;
    (fragment.starts_with("fn-") || fragment.starts_with("fnref-")).then_some(fragment)
}

/// Inspect borrowed source links, including the same safe HTML URL grammar
/// that lowering consumes. Plain documents allocate no replacement AST.
fn visit_links(blocks: &[Block], visit: &mut impl FnMut(&str)) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                visit_inlines(inlines, visit);
            }
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                visit_links(inner, visit);
            }
            Block::List(list) => {
                for item in &list.items {
                    visit_links(&item.blocks, visit);
                }
            }
            Block::Table(table) => {
                for cell in table.head.iter().chain(table.rows.iter().flatten()) {
                    visit_inlines(cell, visit);
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for inlines in item.terms.iter().chain(&item.definitions) {
                        visit_inlines(inlines, visit);
                    }
                }
            }
            Block::HtmlBlock(html) => visit_html(html, visit),
            _ => {}
        }
    }
}

fn visit_inlines(inlines: &[Inline], visit: &mut impl FnMut(&str)) {
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => {
                visit(dest);
                visit_inlines(content, visit);
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                visit_inlines(inner, visit);
            }
            Inline::Html(html) => visit_html(html, visit),
            _ => {}
        }
    }
}

fn visit_html(html: &str, visit: &mut impl FnMut(&str)) {
    for span in crate::safe_html::url_spans(html) {
        if span.kind == crate::safe_html::UrlKind::Link {
            visit(&span.destination);
        }
    }
}
