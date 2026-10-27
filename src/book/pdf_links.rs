//! Chapter-scoped PDF navigation. Source anchors use the existing heading-ID
//! implementation, while destinations come from the actual emitted outlines.
//! Neither title matching nor a prediction of line wrapping/page positions is
//! used to bind links. Footnotes may move without losing their source scope.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::book::{Book, BookRenderer, paths};
use crate::search_index::{EntryKind, build_search_index};
use crate::{
    Block, Document, Inline, PdfASettings, PdfOptions, RenderError, Result, render_pdf_document,
    render_pdf_document_pdfa,
};

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
    render_plan(plan, options)
}

/// Inspect the same assembled, navigation-prepared document that is rendered.
/// Chapter-relative images, synthesized chapter landings and moved note bodies
/// must all use their actual rendering context when diagnosing fallbacks.
#[cfg(feature = "cli")]
pub(super) fn render_report(
    book: &Book,
    document: Document,
    options: &PdfOptions,
    settings: PdfASettings,
) -> Result<(Vec<u8>, Vec<crate::RenderWarning>)> {
    let plan = prepare_configured(book, document, options, settings)?;
    let warnings = crate::render_warnings(&crate::footnotes::for_pdf(&plan.document), options);
    let bytes = render_plan_configured(plan, options, settings)?;
    Ok((bytes, warnings))
}

impl BookRenderer {
    /// Render the retained book with its current options and explicit PDF/A settings.
    ///
    /// Uses the same chapter-local images, citations, landing headings and bound
    /// PDF destinations as [`BookRenderer::render_pdf`]. `PdfASettings::OFF`
    /// preserves ordinary output. PDF/A adds the existing writer's identification
    /// and output intent; it is not an independent conformance certification.
    /// No source is reparsed, and no settings or assets are changed by this call.
    ///
    /// # Errors
    /// Propagates book, asset, font, renderer and navigation-binding failures.
    /// Strict PDF/A checks the assembled source before footnote transformation;
    /// forbidden URI actions cannot hide in unpublished notes. Strict mode with
    /// an `Off` profile is rejected rather than silently ignored.
    pub fn render_pdf_pdfa(&self, settings: PdfASettings) -> Result<Vec<u8>> {
        self.render_pdf_with_options(&self.options().pdf_options(), settings)
    }

    /// Render this parsed book with caller-borrowed PDF options and PDF/A settings.
    ///
    /// Unlike the retained-options convenience methods, this accepts every
    /// [`PdfOptions`] field and borrows image/font payloads without first cloning
    /// them into the retained options. It never updates `self.options()`.
    /// Chapter-local navigation still goes through the book's shared pipeline;
    /// a merged-document render is not an equivalent replacement.
    ///
    /// # Errors
    /// See [`BookRenderer::render_pdf_pdfa`].
    pub fn render_pdf_with_options(
        &self,
        options: &PdfOptions,
        settings: PdfASettings,
    ) -> Result<Vec<u8>> {
        validate_settings(settings)?;
        let document = super::book_pdf_document_with_assets(self.book(), &options.image_assets)?;
        let plan = prepare_configured(self.book(), document, options, settings)?;
        render_plan_configured(plan, options, settings)
    }
}

fn prepare_configured(
    book: &Book,
    document: Document,
    options: &PdfOptions,
    settings: PdfASettings,
) -> Result<Plan> {
    validate_settings(settings)?;
    if settings.strict {
        // Navigation may move/drop footnote bodies. Validate the original
        // assembled source so dormant notes cannot conceal forbidden actions.
        crate::validate_doc_pdfa_strict(&document)?;
    }
    prepare(book, document, options)
}

fn validate_settings(settings: PdfASettings) -> Result<()> {
    if settings.strict && !settings.mode.is_a2b() {
        return Err(crate::pdfa::pdfa_reject(
            "pdf_a_strict_without_profile",
            "strict PDF/A requires the 2b profile",
        ));
    }
    Ok(())
}

fn render_plan_configured(
    plan: Plan,
    options: &PdfOptions,
    settings: PdfASettings,
) -> Result<Vec<u8>> {
    if settings.mode.is_a2b() {
        render_plan_with(plan, options, |document, options| {
            render_pdf_document_pdfa(document, options, settings)
        })
    } else {
        render_plan(plan, options)
    }
}

fn render_plan(plan: Plan, options: &PdfOptions) -> Result<Vec<u8>> {
    render_plan_with(plan, options, render_pdf_document)
}

// Keep heading provenance and annotation binding identical for both writers.
// The emitter runs exactly once, before binding into its reserved URI space.
fn render_plan_with(
    plan: Plan,
    options: &PdfOptions,
    emit: impl FnOnce(&Document, &PdfOptions) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    if plan.targets.is_empty() {
        return emit(&plan.document, options);
    }
    let origins = crate::footnotes::heading_origins_for_pdf(&plan.document);
    let prepared = crate::footnotes::for_pdf(&plan.document);
    let mut headings = Vec::new();
    collect_headings(&prepared.blocks, &mut headings);
    if headings.len() != origins.len() {
        return Err(invalid(
            "footnote preparation and heading provenance disagree",
        ));
    }
    let mut emitted = BTreeMap::new();
    let mut heading_count = 0usize;
    for (heading, origin) in headings.iter().zip(origins) {
        let Block::Heading { inlines, .. } = heading else {
            return Err(invalid("not a heading"));
        };
        if !has_text(inlines) {
            continue;
        }
        if let Some(origin) = origin {
            emitted.insert(origin, heading_count);
        }
        heading_count += 1;
    }
    let targets = plan
        .targets
        .iter()
        .map(|(uri, source)| {
            (
                uri.clone(),
                source.and_then(|source| emitted.get(&source).copied()),
            )
        })
        .collect();
    let bytes = emit(&prepared, options)?;
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
    crate::anchors::decode_fragment(fragment)
}

/// Assemble source boundaries from chapter lengths, never by searching for
/// PageBreak blocks (chapters can contain their own explicit page breaks).
fn chapter_ranges(book: &Book, document: &Document) -> Result<Vec<Range<usize>>> {
    let mut ranges = Vec::with_capacity(book.chapters.len());
    let mut start = 0usize;
    for (index, chapter) in book.chapters.iter().enumerate() {
        if index > 0 {
            if document.blocks.get(start) != Some(&Block::PageBreak) {
                return Err(invalid(
                    "chapter boundary differs from the assembled document",
                ));
            }
            start = start
                .checked_add(1)
                .ok_or_else(|| invalid("chapter size overflow"))?;
        }
        let end = start
            .checked_add(chapter.doc.blocks.len())
            .filter(|end| *end <= document.blocks.len())
            .ok_or_else(|| invalid("chapter size exceeds the assembled document"))?;
        ranges.push(start..end);
        start = end;
    }
    if start != document.blocks.len() {
        return Err(invalid("unaccounted chapter blocks"));
    }
    Ok(ranges)
}

/// Lower the already assembled chapters independently, after their original
/// boundaries have been validated. Lowering can remove a comment or expand
/// one HTML block into several blocks, so source lengths cannot index the
/// result. Consume the merged blocks to retain scoped notes and resolved
/// images, and carry explicit ranges past authored PageBreak blocks.
fn normalize_chapters(
    document: Document,
    ranges: &[Range<usize>],
) -> Result<(Document, Vec<Range<usize>>)> {
    let mut original = document.blocks.into_iter();
    let mut blocks = Vec::with_capacity(original.len());
    let mut normalized_ranges = Vec::with_capacity(ranges.len());
    for (index, range) in ranges.iter().enumerate() {
        if index > 0 {
            blocks.push(
                original
                    .next()
                    .ok_or_else(|| invalid("missing chapter break"))?,
            );
        }
        let chapter = Document {
            blocks: original.by_ref().take(range.len()).collect(),
        };
        if chapter.blocks.len() != range.len() {
            return Err(invalid("missing chapter block"));
        }
        let normalized = match crate::safe_html::lower(&chapter) {
            Cow::Borrowed(_) => chapter,
            Cow::Owned(lowered) => lowered,
        };
        let start = blocks.len();
        blocks.extend(normalized.blocks);
        normalized_ranges.push(start..blocks.len());
    }
    if original.next().is_some() {
        return Err(invalid("extra chapter block"));
    }
    Ok((Document { blocks }, normalized_ranges))
}

/// Map source Markdown heading ordinals to their positions after HTML
/// lowering. Raw HTML headings become PDF bookmarks, but HTML publication
/// does not assign them Markdown IDs: they must not consume collision
/// suffixes or capture a Markdown link to a later identically named heading.
fn normalized_heading_positions(blocks: &[Block]) -> (Vec<usize>, usize) {
    fn walk(blocks: &[Block], positions: &mut Vec<usize>, count: &mut usize) {
        for block in blocks {
            match block {
                Block::Heading { .. } => {
                    positions.push(*count);
                    *count += 1;
                }
                Block::HtmlBlock(_) => {
                    let source = Document {
                        blocks: vec![block.clone()],
                    };
                    let lowered = crate::safe_html::lower(&source);
                    let mut headings = Vec::new();
                    collect_headings(&lowered.blocks, &mut headings);
                    *count += headings.len();
                }
                Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                    walk(inner, positions, count);
                }
                Block::List(list) => {
                    for item in &list.items {
                        walk(&item.blocks, positions, count);
                    }
                }
                _ => {}
            }
        }
    }
    let mut positions = Vec::new();
    let mut count = 0;
    walk(blocks, &mut positions, &mut count);
    (positions, count)
}

fn prepare(book: &Book, document: Document, options: &PdfOptions) -> Result<Plan> {
    let sources: Vec<String> = book
        .chapters
        .iter()
        .map(|chapter| {
            paths::source_path(&chapter.path).ok_or_else(|| invalid("invalid chapter path"))
        })
        .collect::<Result<_>>()?;
    let known: BTreeMap<String, usize> = sources
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, source)| (source, index))
        .collect();
    if known.len() != sources.len() {
        return Err(invalid("duplicate chapter path"));
    }
    let original_ranges = chapter_ranges(book, &document)?;
    let (mut document, ranges) = normalize_chapters(document, &original_ranges)?;
    let mut starts = BTreeSet::new();
    let mut reserved = BTreeSet::new();
    let mut candidates = 0usize;
    for (index, range) in ranges.iter().enumerate() {
        walk_links(&mut document.blocks[range.clone()], &mut |dest| {
            reserve_nonce(dest.as_bytes(), &mut reserved);
            if let Some(target) = local_target(index, &sources[index], dest, &known) {
                candidates += 1;
                if let LocalTarget::Start(chapter) = target {
                    starts.insert(chapter);
                }
            }
        });
    }
    if candidates == 0 {
        return Ok(Plan {
            document,
            targets: BTreeMap::new(),
        });
    }
    // SVG-hosted links can also become URI annotations. Reserve prefixes that
    // occur in supplied bytes as well as source links before generating ours.
    for asset in &options.image_assets {
        for (start, window) in asset.bytes.windows(PREFIX.len()).enumerate() {
            if window == PREFIX.as_bytes() {
                reserve_nonce(&asset.bytes[start..], &mut reserved);
            }
        }
    }
    let nonce = (0..=reserved.len())
        .find(|candidate| !reserved.contains(candidate))
        .ok_or_else(|| invalid("temporary link namespace exhausted"))?;
    let prefix = format!("{PREFIX}{nonce}/");

    // A bare chapter link must include leading prose and empty chapters, not
    // jump to some later heading. Add a chapter-title heading only when such
    // a chapter is actually targeted and has no heading at its start.
    let insert_heading: Vec<bool> = ranges
        .iter()
        .enumerate()
        .map(|(index, range)| {
            starts.contains(&index) && leading_heading(&document.blocks[range.clone()]).is_none()
        })
        .collect();
    let mut original = document.blocks.into_iter();
    let mut blocks = Vec::with_capacity(original.len().saturating_add(starts.len()));
    let mut assembled_ranges = Vec::with_capacity(book.chapters.len());
    for (index, chapter) in book.chapters.iter().enumerate() {
        if index > 0 {
            blocks.push(
                original
                    .next()
                    .ok_or_else(|| invalid("missing chapter break"))?,
            );
        }
        let start = blocks.len();
        if insert_heading[index] {
            let title = if chapter.title.trim().is_empty() {
                &sources[index]
            } else {
                &chapter.title
            };
            blocks.push(Block::Heading {
                level: 1,
                inlines: vec![Inline::Text(title.clone())],
            });
        }
        for _ in 0..ranges[index].len() {
            blocks.push(
                original
                    .next()
                    .ok_or_else(|| invalid("missing chapter block"))?,
            );
        }
        assembled_ranges.push(start..blocks.len());
    }
    if original.next().is_some() {
        return Err(invalid("extra chapter block"));
    }
    document = Document { blocks };

    let mut aliases = Vec::with_capacity(book.chapters.len());
    let mut landings = Vec::with_capacity(book.chapters.len());
    let mut base = 0usize;
    for (index, chapter) in book.chapters.iter().enumerate() {
        let (positions, normalized_count) = normalized_heading_positions(&chapter.doc.blocks);
        let mut source_headings = Vec::new();
        collect_headings(&chapter.doc.blocks, &mut source_headings);
        let source_order: Vec<usize> = crate::footnotes::heading_origins_for_pdf(&chapter.doc)
            .into_iter()
            .flatten()
            .collect();
        // Reuse the renderer-aligned ID implementation; only heading content
        // is cloned. Generated chapter/Notes headings do not consume source IDs.
        let headings = source_order
            .iter()
            .map(|&ordinal| {
                source_headings
                    .get(ordinal)
                    .map(|block| (**block).clone())
                    .ok_or_else(|| invalid("source heading ordinal is invalid"))
            })
            .collect::<Result<Vec<_>>>()?;
        let index_entries = build_search_index(&Document { blocks: headings });
        let entries: Vec<_> = index_entries
            .entries
            .into_iter()
            .filter(|entry| entry.kind == EntryKind::Heading)
            .collect();
        if entries.len() != source_order.len() {
            return Err(invalid("heading index drift"));
        }
        let shift = usize::from(insert_heading[index]);
        let mut map = BTreeMap::new();
        for (entry, ordinal) in entries.into_iter().zip(source_order) {
            let normalized = positions
                .get(ordinal)
                .ok_or_else(|| invalid("normalized heading ordinal is invalid"))?;
            map.insert(entry.anchor, base + shift + *normalized);
        }
        aliases.push(map);
        let chapter_blocks = &document.blocks[assembled_ranges[index].clone()];
        let mut assembled = Vec::new();
        collect_headings(chapter_blocks, &mut assembled);
        if assembled.len() != normalized_count + shift {
            return Err(invalid("HTML lowering and heading provenance disagree"));
        }
        let landing = leading_heading(chapter_blocks)
            .and_then(|first| {
                assembled
                    .iter()
                    .position(|heading| std::ptr::eq(*heading, first))
            })
            .map(|ordinal| base + ordinal);
        landings.push(landing);
        base = base
            .checked_add(assembled.len())
            .ok_or_else(|| invalid("heading count overflow"))?;
    }

    let mut tokens: BTreeMap<Option<usize>, String> = BTreeMap::new();
    for (index, range) in assembled_ranges.into_iter().enumerate() {
        walk_links(&mut document.blocks[range], &mut |dest| {
            let Some(target) = local_target(index, &sources[index], dest, &known) else {
                return;
            };
            let target = match target {
                LocalTarget::Start(chapter) => landings.get(chapter).copied().flatten(),
                LocalTarget::Heading(chapter, fragment) => aliases
                    .get(chapter)
                    .and_then(|map| map.get(&fragment))
                    .copied(),
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
    let targets = tokens
        .into_iter()
        .map(|(target, token)| (token, target))
        .collect();
    Ok(Plan { document, targets })
}

fn reserve_nonce(bytes: &[u8], reserved: &mut BTreeSet<usize>) {
    let Some(rest) = bytes.strip_prefix(PREFIX.as_bytes()) else {
        return;
    };
    let length = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    if length == 0 || rest.get(length) != Some(&b'/') {
        return;
    }
    if let Some(nonce) = std::str::from_utf8(&rest[..length])
        .ok()
        .and_then(|s| s.parse().ok())
    {
        reserved.insert(nonce);
    }
}

fn leading_heading(blocks: &[Block]) -> Option<&Block> {
    let first = blocks
        .iter()
        .find(|block| !matches!(block, Block::FootnoteDefinition { .. } | Block::PageBreak))?;
    match first {
        Block::Heading { inlines, .. } if has_text(inlines) => Some(first),
        _ => None,
    }
}

fn has_text(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| match inline {
        Inline::Text(text)
        | Inline::Code(text)
        | Inline::Html(text)
        | Inline::Math(text)
        | Inline::DisplayMath(text) => text
            .chars()
            .any(|ch| !crate::layout::is_breakable_whitespace(ch)),
        Inline::Image { alt, .. } => alt
            .chars()
            .any(|ch| !crate::layout::is_breakable_whitespace(ch)),
        Inline::FootnoteRef { .. } => true,
        Inline::Emphasis(content)
        | Inline::Strong(content)
        | Inline::Strikethrough(content)
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
                for item in &list.items {
                    collect_headings(&item.blocks, out);
                }
            }
            _ => {}
        }
    }
}

fn walk_links(blocks: &mut [Block], visit: &mut impl FnMut(&mut String)) {
    crate::safe_html::visit_destinations(blocks, &mut |dest, kind| {
        if kind == crate::safe_html::UrlKind::Link {
            visit(dest);
        }
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::book::{BookInput, book_pdf_document, build_book};

    fn book(chapters: &[(&str, &str)]) -> Book {
        build_book(
            &chapters
                .iter()
                .map(|(path, source)| BookInput {
                    path: (*path).into(),
                    source: (*source).into(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn resolved(plan: &mut Plan) -> Vec<Option<usize>> {
        let mut out = Vec::new();
        walk_links(&mut plan.document.blocks, &mut |dest| {
            if let Some(target) = plan.targets.get(dest) {
                out.push(*target);
            }
        });
        out
    }

    #[test]
    fn duplicate_titles_keep_local_and_cross_chapter_scope() {
        let book = book(&[
            (
                "guide/one.md",
                "# Same\n\n[self](#same) [next](two.md#same)\n\n## Same\n\n[second](#same-2)",
            ),
            (
                "guide/two.md",
                "# Same\n\n[here](#same) [back](one.md#same-2)",
            ),
        ]);
        let original = book.chapters[0].doc.clone();
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(
            resolved(&mut plan),
            vec![Some(0), Some(2), Some(1), Some(2), Some(1)]
        );
        assert_eq!(book.chapters[0].doc, original);
    }

    #[test]
    fn raw_html_anchors_share_chapter_scope_without_mutating_the_html_source() {
        let book = book(&[
            (
                "guide/one.md",
                "# Same\n\nInline <a href='two.md?print=1&amp;x=2#same'>next</a>.\n\n<div><a href='#same'>self</a><a href='two.md#absent'>missing</a><a href='https://example.org/'>web</a></div>\n",
            ),
            ("guide/two.md", "# Same\n\n<a href='one.md#same'>back</a>\n"),
        ]);
        let originals: Vec<_> = book
            .chapters
            .iter()
            .map(|chapter| chapter.doc.clone())
            .collect();
        let html =
            crate::render_html_document(&book.chapters[0].doc, &crate::HtmlOptions::default())
                .unwrap();
        let document = book_pdf_document(&book);
        let mut plan = prepare(&book, document, &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), [Some(1), Some(0), None, Some(0)]);
        let mut external = 0;
        walk_links(&mut plan.document.blocks, &mut |dest| {
            external += usize::from(dest == "https://example.org/");
        });
        assert_eq!(external, 1);
        assert_eq!(
            book.chapters
                .iter()
                .map(|chapter| &chapter.doc)
                .collect::<Vec<_>>(),
            originals.iter().collect::<Vec<_>>()
        );
        assert_eq!(
            crate::render_html_document(&book.chapters[0].doc, &crate::HtmlOptions::default())
                .unwrap(),
            html
        );
    }

    #[test]
    fn links_removed_by_pdf_lowering_never_insert_a_chapter_title() {
        for source in [
            "<pre><code><a href='two.md'>example</a></code></pre>",
            "Before <code><a href='two.md'>example</a></code> after.",
            "Before <kbd><a href='two.md'>example</a></kbd> after.",
            "Before <samp><a href='two.md'>example</a></samp> after.",
            "Before <tt><a href='two.md'>example</a></tt> after.",
            "Before <code>[example](two.md)</code> after.",
            "<a href='two.md'/>",
        ] {
            let book = book(&[
                ("one.md", source),
                ("two.md", "Target chapter begins with prose."),
            ]);
            let options = PdfOptions::default();
            let document = book_pdf_document(&book);
            let expected = render_pdf_document(&document, &options).unwrap();
            let plan = prepare(&book, document, &options).unwrap();
            assert!(plan.targets.is_empty(), "dormant link in {source}");
            let mut headings = Vec::new();
            collect_headings(&plan.document.blocks, &mut headings);
            assert!(headings.is_empty(), "no generated heading for {source}");
            assert_eq!(render_plan(plan, &options).unwrap(), expected);
        }
    }

    fn assert_bound_links(bytes: &[u8], count: usize) {
        let pdf = String::from_utf8_lossy(bytes);
        assert!(!pdf.contains(PREFIX));
        let links: Vec<_> = pdf.split("/Subtype /Link").skip(1).collect();
        assert_eq!(links.len(), count);
        for link in links {
            let dictionary = link.split_once("endobj").unwrap().0;
            assert!(
                dictionary.contains("/Dest ["),
                "bound page destination: {dictionary}"
            );
            assert!(!dictionary.contains("/S /URI"));
        }
    }

    #[test]
    fn raw_html_headings_and_links_render_with_matching_pdf_provenance() {
        let book = book(&[
            (
                "one.md",
                "<h1>Raw heading</h1>\n\n<a href='two.md#target'>Next</a>\n\n> <div><h2>Nested raw heading</h2></div>\n",
            ),
            ("two.md", "# Target\n\n<a href='one.md'>Back</a>\n"),
        ]);
        let options = PdfOptions::default();
        let mut plan = prepare(&book, book_pdf_document(&book), &options).unwrap();
        assert_eq!(resolved(&mut plan), [Some(2), Some(0)]);
        assert_bound_links(&render_plan(plan, &options).unwrap(), 2);
    }

    #[test]
    fn raw_html_bookmarks_do_not_capture_markdown_heading_ids() {
        let book = book(&[
            ("one.md", "# Source\n\n<a href='two.md#same'>Next</a>\n"),
            (
                "two.md",
                "<h1>Same</h1>\n\n# Same\n\n<a href='#same'>Self</a>\n",
            ),
        ]);
        let options = PdfOptions::default();
        let mut plan = prepare(&book, book_pdf_document(&book), &options).unwrap();
        // The raw heading is PDF ordinal 1; both source links address the
        // native Markdown heading, whose chapter-local HTML ID is still same.
        assert_eq!(resolved(&mut plan), [Some(2), Some(2)]);
        assert_bound_links(&render_plan(plan, &options).unwrap(), 2);
    }

    #[test]
    fn normalization_retains_merged_notes_assets_and_authored_page_breaks() {
        let mut book = book(&[
            (
                "guide/one.md",
                "# One\n\nReference[^n].\n\n<div><img src='../image.svg' alt='asset'><a href='../two.md#target'>Next</a></div>\n\n[^n]: <div><h2>Raw note heading</h2><a href='../two.md#target'>Note link</a></div>\n",
            ),
            (
                "two.md",
                "# Target\n\nAnother[^n].\n\n[^n]: Separate note.\n",
            ),
        ]);
        book.chapters[0].doc.blocks.insert(1, Block::PageBreak);
        let options = PdfOptions {
            image_assets: vec![crate::PdfImageAsset::new(
                "image.svg",
                br#"<svg width="20" height="10"><rect width="20" height="10"/></svg>"#.to_vec(),
            )],
            ..PdfOptions::default()
        };
        let document =
            crate::book::book_pdf_document_with_assets(&book, &options.image_assets).unwrap();
        let plan = prepare(&book, document, &options).unwrap();
        assert_eq!(
            plan.document
                .blocks
                .iter()
                .filter(|block| **block == Block::PageBreak)
                .count(),
            2
        );
        let notes: Vec<_> = plan
            .document
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::FootnoteDefinition { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(notes, ["fmd-book-1-n", "fmd-book-2-n"]);
        let mut asset_keys = Vec::new();
        let mut inspected = plan.document.clone();
        crate::safe_html::visit_destinations(&mut inspected.blocks, &mut |dest, kind| {
            if kind == crate::safe_html::UrlKind::Image {
                asset_keys.push(dest.clone());
            }
        });
        assert_eq!(asset_keys, ["image.svg"]);
        assert_bound_links(&render_plan(plan, &options).unwrap(), 2);
    }

    #[test]
    fn bare_chapter_links_get_real_landings_without_renaming_source_anchors() {
        let book = book(&[
            (
                "one.md",
                "# One\n\n[chapter](two.md) [detail](two.md#details)",
            ),
            (
                "two.md",
                "Leading prose.\n\n## Details\n\n[local](#details)",
            ),
        ]);
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), vec![Some(1), Some(2), Some(2)]);
        assert_eq!(
            plan.document
                .blocks
                .iter()
                .filter(|block| matches!(block, Block::Heading { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn unknown_fragment_never_falls_back_to_another_chapters_heading() {
        let book = book(&[
            ("one.md", "# Hidden"),
            ("two.md", "# Other\n\n[missing](#hidden)"),
        ]);
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), vec![None]);
    }

    #[test]
    fn paths_fragments_queries_and_external_urls_are_classified_separately() {
        let known = BTreeMap::from([("guide/start.md".into(), 0), ("space name.md".into(), 1)]);
        assert_eq!(
            local_target(
                0,
                "guide/start.md",
                "../space%20name.md?edition=2#%61lpha",
                &known
            ),
            Some(LocalTarget::Heading(1, "alpha".into()))
        );
        assert_eq!(
            local_target(0, "guide/start.md", "?edition=2#alpha", &known),
            Some(LocalTarget::Heading(0, "alpha".into()))
        );
        for url in ["#bad%", "#bad%GG", "#bad%00", "#%FF"] {
            assert_eq!(
                local_target(0, "guide/start.md", url, &known),
                Some(LocalTarget::Unresolved)
            );
        }
        for url in [
            "https://example.org/#alpha",
            "//example.org/page",
            "../../escape.md",
            "missing.md#alpha",
        ] {
            assert_eq!(local_target(0, "guide/start.md", url, &known), None);
        }
        assert_eq!(decode_fragment("%2561lpha"), Some("%61lpha".into()));
    }

    #[test]
    fn external_only_books_preserve_the_exact_original_assembly() {
        let book = book(&[
            ("one.md", "Prose [external](https://example.org)."),
            ("two.md", "Other prose."),
        ]);
        let document = book_pdf_document(&book);
        let plan = prepare(&book, document.clone(), &PdfOptions::default()).unwrap();
        assert_eq!(plan.document, document);
        assert!(plan.targets.is_empty());
    }

    #[test]
    fn literal_suffix_collisions_use_the_existing_heading_id_algorithm() {
        let book = book(&[(
            "one.md",
            "# A\n\n# A-2\n\n# A\n\n[a](#a) [literal](#a-2) [duplicate](#a-3)",
        )]);
        let mut plan = prepare(&book, book_pdf_document(&book), &PdfOptions::default()).unwrap();
        assert_eq!(resolved(&mut plan), vec![Some(0), Some(1), Some(2)]);
    }
}
