//! Chapter-scoped PDF navigation. Source anchors use the existing heading-ID
//! implementation, while destinations come from the actual emitted outlines.
//! Neither title matching nor a prediction of line wrapping/page positions is
//! used to bind links. Footnotes may move without losing their source scope.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::book::{Book, paths};
use crate::search_index::{EntryKind, build_search_index};
use crate::{Block, Document, Inline, PdfOptions, RenderError, Result, render_pdf_document};

#[path = "pdf_link_wire.rs"]
mod wire;

const PREFIX: &str = "fmd-book-pdf-nav/";

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("book_pdf_navigation: {message}"))
}

#[cfg(feature = "cli")]
pub(super) fn page_count(bytes: &[u8]) -> Option<u64> {
    wire::page_count(bytes)
}

pub(super) fn render(book: &Book, document: Document, options: &PdfOptions) -> Result<Vec<u8>> {
    let plan = prepare(book, document, options)?;
    if plan.targets.is_empty() {
        return render_pdf_document(&plan.document, options);
    }
    let origins = crate::footnotes::heading_origins_for_pdf(&plan.document);
    let prepared = crate::footnotes::for_pdf(&plan.document);
    let mut headings = Vec::new();
    collect_headings(&prepared.blocks, &mut headings);
    if headings.len() != origins.len() {
        return Err(invalid("footnote preparation and heading provenance disagree"));
    }
    let mut emitted = BTreeMap::new();
    let mut heading_count = 0usize;
    for (heading, origin) in headings.iter().zip(origins) {
        let Block::Heading { inlines, .. } = heading else { return Err(invalid("not a heading")); };
        if !has_text(inlines) { continue; }
        if let Some(origin) = origin { emitted.insert(origin, heading_count); }
        heading_count += 1;
    }
    let targets = plan.targets.iter().map(|(uri, source)| {
        (uri.clone(), source.and_then(|source| emitted.get(&source).copied()))
    }).collect();
    let bytes = render_pdf_document(&prepared, options)?;
    wire::bind(bytes, &targets, heading_count, options.toc)
}

struct Plan {
    document: Document,
    /// Temporary URI -> source heading ordinal; None means unresolved.
    targets: BTreeMap<String, Option<usize>>,
}

#[derive(Debug, PartialEq, Eq)]
enum LocalTarget {
    Start(usize),
    Heading(usize, String),
    Unresolved,
}

fn local_target(
    source_index: usize,
    source: &str,
    dest: &str,
    known: &BTreeMap<String, usize>,
) -> Option<LocalTarget> {
    let dest = dest.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    let (chapter, suffix) = if dest.starts_with(['#', '?']) {
        (source_index, dest.to_string())
    } else {
        let (path, suffix) = paths::destination(source, dest)?;
        (*known.get(&path)?, suffix)
    };
    let fragment = suffix.split_once('#').map(|(_, fragment)| {
        fragment.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control())
    });
    Some(match fragment {
        None | Some("") => LocalTarget::Start(chapter),
        Some(fragment) => match decode_fragment(fragment) {
            Some(fragment) => LocalTarget::Heading(chapter, fragment),
            None => LocalTarget::Unresolved,
        },
    })
}

fn decode_fragment(fragment: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let mut bytes = Vec::with_capacity(fragment.len());
    let source = fragment.as_bytes();
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%' {
            bytes.push(hex(*source.get(index + 1)?)? * 16 + hex(*source.get(index + 2)?)?);
            index += 3;
        } else {
            bytes.push(source[index]);
            index += 1;
        }
    }
    let fragment = String::from_utf8(bytes).ok()?;
    (!fragment.chars().any(char::is_control)).then_some(fragment)
}

/// Assemble source boundaries from chapter lengths, never by searching for
/// PageBreak blocks (chapters can contain their own explicit page breaks).
fn chapter_ranges(book: &Book, document: &Document) -> Result<Vec<Range<usize>>> {
    let mut ranges = Vec::with_capacity(book.chapters.len());
    let mut start = 0usize;
    for (index, chapter) in book.chapters.iter().enumerate() {
        if index > 0 {
            if document.blocks.get(start) != Some(&Block::PageBreak) {
                return Err(invalid("chapter boundary differs from the assembled document"));
            }
            start = start.checked_add(1).ok_or_else(|| invalid("chapter size overflow"))?;
        }
        let end = start.checked_add(chapter.doc.blocks.len())
            .filter(|end| *end <= document.blocks.len())
            .ok_or_else(|| invalid("chapter size exceeds the assembled document"))?;
        ranges.push(start..end);
        start = end;
    }
    if start != document.blocks.len() { return Err(invalid("unaccounted chapter blocks")); }
    Ok(ranges)
}

fn prepare(book: &Book, mut document: Document, options: &PdfOptions) -> Result<Plan> {
    let sources: Vec<String> = book.chapters.iter().map(|chapter| {
        paths::source_path(&chapter.path).ok_or_else(|| invalid("invalid chapter path"))
    }).collect::<Result<_>>()?;
    let known: BTreeMap<String, usize> = sources.iter().cloned().enumerate()
        .map(|(index, source)| (source, index)).collect();
    if known.len() != sources.len() { return Err(invalid("duplicate chapter path")); }
    let ranges = chapter_ranges(book, &document)?;
    let mut starts = BTreeSet::new();
    let mut reserved = BTreeSet::new();
    let mut candidates = 0usize;
    for (index, range) in ranges.iter().enumerate() {
        walk_links(&mut document.blocks[range.clone()], &mut |dest| {
            reserve_nonce(dest.as_bytes(), &mut reserved);
            if let Some(target) = local_target(index, &sources[index], dest, &known) {
                candidates += 1;
                if let LocalTarget::Start(chapter) = target { starts.insert(chapter); }
            }
        });
    }
    if candidates == 0 {
        return Ok(Plan { document, targets: BTreeMap::new() });
    }
    // SVG-hosted links can also become URI annotations. Reserve prefixes that
    // occur in supplied bytes as well as source links before generating ours.
    for asset in &options.image_assets {
        for (start, window) in asset.bytes.windows(PREFIX.len()).enumerate() {
            if window == PREFIX.as_bytes() { reserve_nonce(&asset.bytes[start..], &mut reserved); }
        }
    }
    let nonce = (0..=reserved.len()).find(|candidate| !reserved.contains(candidate))
        .ok_or_else(|| invalid("temporary link namespace exhausted"))?;
    let prefix = format!("{PREFIX}{nonce}/");

    // A bare chapter link must include leading prose and empty chapters, not
    // jump to some later heading. Add a chapter-title heading only when such
    // a chapter is actually targeted and has no heading at its start.
    let insert_heading: Vec<bool> = ranges.iter().enumerate().map(|(index, range)| {
        starts.contains(&index) && leading_heading(&document.blocks[range.clone()]).is_none()
    }).collect();
    let mut original = document.blocks.into_iter();
    let mut blocks = Vec::with_capacity(original.len().saturating_add(starts.len()));
    let mut ranges = Vec::with_capacity(book.chapters.len());
    for (index, chapter) in book.chapters.iter().enumerate() {
        if index > 0 {
            blocks.push(original.next().ok_or_else(|| invalid("missing chapter break"))?);
        }
        let start = blocks.len();
        if insert_heading[index] {
            let title = if chapter.title.trim().is_empty() { &sources[index] } else { &chapter.title };
            blocks.push(Block::Heading { level: 1, inlines: vec![Inline::Text(title.clone())] });
        }
        for _ in 0..chapter.doc.blocks.len() {
            blocks.push(original.next().ok_or_else(|| invalid("missing chapter block"))?);
        }
        ranges.push(start..blocks.len());
    }
    if original.next().is_some() { return Err(invalid("extra chapter block")); }
    document = Document { blocks };

    let mut aliases = Vec::with_capacity(book.chapters.len());
    let mut landings = Vec::with_capacity(book.chapters.len());
    let mut base = 0usize;
    for (index, chapter) in book.chapters.iter().enumerate() {
        let mut source_headings = Vec::new();
        collect_headings(&chapter.doc.blocks, &mut source_headings);
        let source_order: Vec<usize> = crate::footnotes::heading_origins_for_pdf(&chapter.doc)
            .into_iter().flatten().collect();
        // Reuse the renderer-aligned ID implementation; only heading content
        // is cloned. Generated chapter/Notes headings do not consume source IDs.
        let headings = source_order.iter().map(|&ordinal| {
            source_headings.get(ordinal).map(|block| (**block).clone())
                .ok_or_else(|| invalid("source heading ordinal is invalid"))
        }).collect::<Result<Vec<_>>>()?;
        let index_entries = build_search_index(&Document { blocks: headings });
        let entries: Vec<_> = index_entries.entries.into_iter()
            .filter(|entry| entry.kind == EntryKind::Heading).collect();
        if entries.len() != source_order.len() { return Err(invalid("heading index drift")); }
        let shift = usize::from(insert_heading[index]);
        let mut map = BTreeMap::new();
        for (entry, ordinal) in entries.into_iter().zip(source_order) {
            map.insert(entry.anchor, base + shift + ordinal);
        }
        aliases.push(map);
        let chapter_blocks = &document.blocks[ranges[index].clone()];
        let mut assembled = Vec::new();
        collect_headings(chapter_blocks, &mut assembled);
        let landing = leading_heading(chapter_blocks).and_then(|first| {
            assembled.iter().position(|heading| std::ptr::eq(*heading, first))
        }).map(|ordinal| base + ordinal);
        landings.push(landing);
        base = base.checked_add(assembled.len()).ok_or_else(|| invalid("heading count overflow"))?;
    }

    let mut tokens: BTreeMap<Option<usize>, String> = BTreeMap::new();
    for (index, range) in ranges.into_iter().enumerate() {
        walk_links(&mut document.blocks[range], &mut |dest| {
            let Some(target) = local_target(index, &sources[index], dest, &known) else { return; };
            let target = match target {
                LocalTarget::Start(chapter) => landings.get(chapter).copied().flatten(),
                LocalTarget::Heading(chapter, fragment) => aliases.get(chapter)
                    .and_then(|map| map.get(&fragment)).copied(),
                LocalTarget::Unresolved => None,
            };
            if let Some(token) = tokens.get(&target) {
                dest.clone_from(token);
            } else {
                // Reserve ample ASCII space for an explicit /XYZ destination;
                // these URI bytes are never part of the visible text layer.
                let token = format!("{prefix}{:020}/{}", tokens.len(), "0".repeat(128));
                tokens.insert(target, token.clone());
                *dest = token;
            }
        });
    }
    let targets = tokens.into_iter().map(|(target, token)| (token, target)).collect();
    Ok(Plan { document, targets })
}

fn reserve_nonce(bytes: &[u8], reserved: &mut BTreeSet<usize>) {
    let Some(rest) = bytes.strip_prefix(PREFIX.as_bytes()) else { return; };
    let length = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    if length == 0 || rest.get(length) != Some(&b'/') { return; }
    if let Some(nonce) = std::str::from_utf8(&rest[..length]).ok().and_then(|s| s.parse().ok()) {
        reserved.insert(nonce);
    }
}

fn leading_heading(blocks: &[Block]) -> Option<&Block> {
    let first = blocks.iter().find(|block| {
        !matches!(block, Block::FootnoteDefinition { .. } | Block::PageBreak)
    })?;
    match first {
        Block::Heading { inlines, .. } if has_text(inlines) => Some(first),
        _ => None,
    }
}

fn has_text(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| match inline {
        Inline::Text(text) | Inline::Code(text) | Inline::Html(text)
        | Inline::Math(text) | Inline::DisplayMath(text) => {
            text.chars().any(|ch| !crate::layout::is_breakable_whitespace(ch))
        }
        Inline::Image { alt, .. } => alt.chars().any(|ch| !crate::layout::is_breakable_whitespace(ch)),
        Inline::FootnoteRef { .. } => true,
        Inline::Emphasis(content) | Inline::Strong(content) | Inline::Strikethrough(content)
        | Inline::Link { content, .. } => has_text(content),
        Inline::SoftBreak | Inline::HardBreak => false,
    })
}

fn collect_headings<'a>(blocks: &'a [Block], out: &mut Vec<&'a Block>) {
    for block in blocks {
        match block {
            Block::Heading { .. } => out.push(block),
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                collect_headings(inner, out);
            }
            Block::List(list) => {
                for item in &list.items { collect_headings(&item.blocks, out); }
            }
            _ => {}
        }
    }
}

fn walk_links(blocks: &mut [Block], visit: &mut impl FnMut(&mut String)) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => walk_inline_links(inlines, visit),
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => walk_links(inner, visit),
            Block::List(list) => {
                for item in &mut list.items { walk_links(&mut item.blocks, visit); }
            }
            Block::Table(table) => {
                for cell in &mut table.head { walk_inline_links(cell, visit); }
                for row in &mut table.rows {
                    for cell in row { walk_inline_links(cell, visit); }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for content in item.terms.iter_mut().chain(&mut item.definitions) {
                        walk_inline_links(content, visit);
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk_inline_links(inlines: &mut [Inline], visit: &mut impl FnMut(&mut String)) {
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => { visit(dest); walk_inline_links(content, visit); }
            Inline::Emphasis(content) | Inline::Strong(content) | Inline::Strikethrough(content) => {
                walk_inline_links(content, visit);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::book::{BookInput, book_pdf_document, build_book};

    fn book(chapters: &[(&str, &str)]) -> Book {
        build_book(&chapters.iter().map(|(path, source)| BookInput {
            path: (*path).into(), source: (*source).into(),
        }).collect::<Vec<_>>()).unwrap()
    }

    fn resolved(plan: &mut Plan) -> Vec<Option<usize>> {
        let mut out = Vec::new();
        walk_links(&mut plan.document.blocks, &mut |dest| {
            if let Some(target) = plan.targets.get(dest) { out.push(*target); }
        });
        out
    }

    #[test]
    fn duplicate_titles_keep_local_and_cross_chapter_scope() {
        let book = book(&[
            ("guide/one.md", "# Same\n\n[self](#same) [next](two.md#same)\n\n## Same\n\n[second](#same-2)"),
            ("guide/two.md", "# Same\n\n[here](#same) [back](one.md#same-2)"),
        ]);
        let original = book.chapters[0].doc.clone();
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), vec![Some(0), Some(2), Some(1), Some(2), Some(1)]);
        assert_eq!(book.chapters[0].doc, original);
    }

    #[test]
    fn bare_chapter_links_get_real_landings_without_renaming_source_anchors() {
        let book = book(&[
            ("one.md", "# One\n\n[chapter](two.md) [detail](two.md#details)"),
            ("two.md", "Leading prose.\n\n## Details\n\n[local](#details)"),
        ]);
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), vec![Some(1), Some(2), Some(2)]);
        assert_eq!(plan.document.blocks.iter().filter(|block| matches!(block, Block::Heading { .. })).count(), 3);
    }

    #[test]
    fn unknown_fragment_never_falls_back_to_another_chapters_heading() {
        let book = book(&[("one.md", "# Hidden"), ("two.md", "# Other\n\n[missing](#hidden)")]);
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), vec![None]);
    }

    #[test]
    fn paths_fragments_queries_and_external_urls_are_classified_separately() {
        let known = BTreeMap::from([("guide/start.md".into(), 0), ("space name.md".into(), 1)]);
        assert_eq!(local_target(0, "guide/start.md", "../space%20name.md?edition=2#%61lpha", &known), Some(LocalTarget::Heading(1, "alpha".into())));
        assert_eq!(local_target(0, "guide/start.md", "?edition=2#alpha", &known), Some(LocalTarget::Heading(0, "alpha".into())));
        for url in ["#bad%", "#bad%GG", "#bad%00", "#%FF"] {
            assert_eq!(local_target(0, "guide/start.md", url, &known), Some(LocalTarget::Unresolved));
        }
        for url in ["https://example.org/#alpha", "//example.org/page", "../../escape.md", "missing.md#alpha"] {
            assert_eq!(local_target(0, "guide/start.md", url, &known), None);
        }
        assert_eq!(decode_fragment("%2561lpha"), Some("%61lpha".into()));
    }

    #[test]
    fn external_only_books_preserve_the_exact_original_assembly() {
        let book = book(&[("one.md", "Prose [external](https://example.org)."), ("two.md", "Other prose.")]);
        let document = book_pdf_document(&book);
        let plan = prepare(&book, document.clone(), &PdfOptions::default()).unwrap();
        assert_eq!(plan.document, document);
        assert!(plan.targets.is_empty());
    }

    #[test]
    fn literal_suffix_collisions_use_the_existing_heading_id_algorithm() {
        let book = book(&[("one.md", "# A\n\n# A-2\n\n# A\n\n[a](#a) [literal](#a-2) [duplicate](#a-3)")]);
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), vec![Some(0), Some(1), Some(2)]);
    }
}
