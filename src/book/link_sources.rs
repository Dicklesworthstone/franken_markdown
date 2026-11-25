//! Source ownership retained separately from the public, source-less link AST.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::{Json, LinkReport, check_book_links_inner, invalid};
use crate::book::BookRenderer;
use crate::book::source_locations::{BookSourceLocation, SourceMap, SourceMapBudget};
use crate::parse::NavigationCapture;
use crate::span::{SourceSpan, SpannedDocument};
use crate::{Block, Document, Inline, Result};

const MAX_METADATA: usize = 65_536;
pub(super) const MAX_PROVENANCE_BYTES: usize = 2 * 1024 * 1024;
// Optional parser-decoded spellings are independent of source-coordinate
// admission. Exhaustion never removes an otherwise valid navigation location.
pub(crate) const MAX_PARSED_DESTINATION_BYTES: usize = 1024 * 1024;
const MAX_PARSED_DESTINATION: usize = 8192;

/// Precision of the parser-owned expanded range projected into original files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkSourcePrecision {
    /// The URL token supplying this actual link, including a reference
    /// definition's token when the link uses a full/collapsed/shortcut label.
    Destination,
    /// Only the enclosing top-level syntax block has proven ownership.
    Block,
}

/// Original-source attribution is navigation metadata, never edit authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSourceLocation {
    pub precision: LinkSourcePrecision,
    pub source: BookSourceLocation,
}

/// The ordinary complete link report with a separately bounded source sidecar.
/// Existing public `LinkFinding`, `ChapterLinks`, and `LinkReport` literals and
/// the source-less checker retain their construction and serialization shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedLinkReport {
    report: LinkReport,
    locations: Vec<Vec<LinkSourceLocation>>,
    json: String,
}

impl LocatedLinkReport {
    #[must_use]
    pub fn report(&self) -> &LinkReport {
        &self.report
    }

    #[must_use]
    pub fn source_location(&self, chapter: usize, finding: usize) -> Option<&LinkSourceLocation> {
        self.locations.get(chapter)?.get(finding)
    }

    /// Cached schema-v1 JSON: unchanged 4 MiB base admission and an independent
    /// 2 MiB optional source allowance. Exhausted provenance is explicit.
    #[must_use]
    pub fn to_json(&self) -> String {
        self.json.clone()
    }
}

impl BookRenderer {
    /// Check the retained AST and locate findings in original selected sources.
    /// Reuses the publication walk and parser-owned token identities, without
    /// parsing, source-text searches, asset reads, or a second renderer.
    ///
    /// # Errors
    /// Same complete-link-report admission as [`Self::validate_links`]. Missing
    /// or exhausted optional source metadata never rejects an admitted report.
    pub fn validate_links_with_sources(&self) -> Result<LocatedLinkReport> {
        check_with_sources(self.book(), self.source_diagnostics().link_sources())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChapterLinkSources {
    length: usize,
    blocks: Vec<SourceSpan>,
    destinations: Vec<Option<Range<usize>>>,
    /// Captured in structural order before chapter URL canonicalization.
    parsed_destinations: Vec<Option<String>>,
    /// The allowance that discarded captured values; repeated admission cannot
    /// recover them. A workspace must recapture before changing this allowance.
    parsed_truncation_allowance: Option<usize>,
    map: SourceMap,
    reason: Option<&'static str>,
}

impl ChapterLinkSources {
    pub(crate) fn capture(
        parsed: &SpannedDocument,
        capture: &NavigationCapture,
        path: &str,
    ) -> Self {
        let mut destinations = Vec::new();
        let mut parsed_destinations = Vec::new();
        let mut parsed_bytes = 0usize;
        let mut complete = parsed.blocks.len() <= MAX_METADATA;
        if complete {
            for block in &parsed.blocks {
                if !walk_blocks(std::slice::from_ref(&block.node), 0, &mut |node| {
                    if destinations.len() == MAX_METADATA {
                        return false;
                    }
                    destinations.push(match &node {
                        Node::Link(_, destination) => capture.link_range(destination),
                        Node::Html(html, index, _) => capture.html_range(html, *index),
                        Node::Note(_, _) => None,
                    });
                    parsed_destinations.push(node.destination().and_then(|destination| {
                        if destination.len() > MAX_PARSED_DESTINATION
                            || destination.len()
                                > MAX_PARSED_DESTINATION_BYTES.saturating_sub(parsed_bytes)
                        {
                            return None;
                        }
                        parsed_bytes += destination.len();
                        Some(destination.to_owned())
                    }));
                    true
                }) {
                    complete = false;
                    break;
                }
            }
        }
        Self {
            length: parsed.source_len,
            blocks: if complete {
                parsed.blocks.iter().map(|block| block.span).collect()
            } else {
                Vec::new()
            },
            destinations: if complete { destinations } else { Vec::new() },
            parsed_destinations: if complete {
                parsed_destinations
            } else {
                Vec::new()
            },
            parsed_truncation_allowance: None,
            map: SourceMap::identity(path, parsed.source_len),
            reason: (!complete).then_some("provenance_limit"),
        }
    }

    fn unavailable(length: usize) -> Self {
        Self {
            length,
            blocks: Vec::new(),
            destinations: Vec::new(),
            parsed_destinations: Vec::new(),
            parsed_truncation_allowance: None,
            map: SourceMap::unavailable(length, "not_captured"),
            reason: Some("not_captured"),
        }
    }

    /// Apply existing coordinate admission before spending the independent
    /// parsed-target allowance. Discarded chapters must not consume that pool.
    pub(crate) fn admit_metadata(&mut self, units: &mut usize, parsed_bytes: &mut usize) {
        let count = self.blocks.len().saturating_add(self.destinations.len());
        if count > MAX_METADATA.saturating_sub(*units) {
            self.blocks.clear();
            self.destinations.clear();
            self.parsed_destinations.clear();
            self.reason = Some("provenance_limit");
        } else {
            *units += count;
        }
        self.admit_parsed_destinations(parsed_bytes);
    }

    fn admit_parsed_destinations(&mut self, used: &mut usize) {
        let allowance = MAX_PARSED_DESTINATION_BYTES.saturating_sub(*used);
        for destination in &mut self.parsed_destinations {
            if let Some(value) = destination {
                if value.len() > MAX_PARSED_DESTINATION_BYTES.saturating_sub(*used) {
                    self.parsed_truncation_allowance.get_or_insert(allowance);
                    *destination = None;
                } else {
                    *used += value.len();
                }
            }
        }
    }

    pub(crate) fn needs_parsed_destination_refresh(&self, used: usize) -> bool {
        self.reason.is_none()
            && self.parsed_truncation_allowance.is_some_and(|previous| {
                previous != MAX_PARSED_DESTINATION_BYTES.saturating_sub(used)
            })
    }

    fn location(&self, owner: usize, index: Option<usize>) -> LinkSourceLocation {
        let exact = index
            .and_then(|index| self.destinations.get(index))
            .and_then(Clone::clone);
        let precision = if exact.is_some() {
            LinkSourcePrecision::Destination
        } else {
            LinkSourcePrecision::Block
        };
        let range = exact.or_else(|| self.blocks.get(owner).map(|span| span.start..span.end));
        let source = match (self.reason, range) {
            (Some(reason), _) => BookSourceLocation::Unavailable(reason),
            (None, Some(range)) => self.map.locate(range),
            _ => BookSourceLocation::Unavailable("ownership_unavailable"),
        };
        LinkSourceLocation { precision, source }
    }

    pub(super) fn indices(&self, doc: &Document) -> Option<BTreeMap<NodeKey, usize>> {
        self.indices_for_blocks(&doc.blocks)
    }

    fn indices_for_blocks(&self, blocks: &[Block]) -> Option<BTreeMap<NodeKey, usize>> {
        if self.reason.is_some() {
            return None;
        }
        let mut indices = BTreeMap::new();
        let mut next = 0;
        let complete = walk_blocks(blocks, 0, &mut |node| {
            if next >= self.destinations.len() || indices.insert(node.key(), next).is_some() {
                return false;
            }
            next += 1;
            true
        });
        (complete && next == self.destinations.len()).then_some(indices)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct BookLinkSources {
    chapters: Vec<ChapterLinkSources>,
}

impl BookLinkSources {
    pub(crate) fn new(mut chapters: Vec<ChapterLinkSources>) -> Self {
        let mut units = 0;
        let mut parsed_bytes = 0usize;
        let mut maps = SourceMapBudget::default();
        for chapter in &mut chapters {
            chapter.admit_metadata(&mut units, &mut parsed_bytes);
            let placeholder = SourceMap::unavailable(chapter.length, "not_captured");
            chapter.map = maps.admit(std::mem::replace(&mut chapter.map, placeholder));
        }
        Self { chapters }
    }

    pub(crate) fn unavailable(lengths: impl Iterator<Item = usize>) -> Self {
        Self::new(lengths.map(ChapterLinkSources::unavailable).collect())
    }

    pub(crate) fn chapter(&self, index: usize) -> &ChapterLinkSources {
        &self.chapters[index]
    }

    pub(crate) fn with_maps(mut self, maps: &[SourceMap]) -> Result<Self> {
        if maps.len() != self.chapters.len() {
            return Err(invalid("source provenance does not match chapter count"));
        }
        for (chapter, map) in self.chapters.iter_mut().zip(maps) {
            if chapter.length != map.len() {
                return Err(invalid("source provenance does not match chapter bytes"));
            }
            chapter.map = map.clone();
        }
        Ok(Self::new(self.chapters))
    }
}

/// Parser-owned references, including dormant notes retained by PDF endnotes.
pub(crate) struct PdfReferenceSource {
    pub destination: String,
    pub parsed_destination: Option<String>,
    pub note: bool,
    /// A note citation outside an existing link can own a generated return
    /// action. Nested citations retain their enclosing link instead.
    pub linked_note: bool,
    pub html: bool,
    pub source: LinkSourceLocation,
}

/// Use the original AST to project source ranges; normalized PDF inventories
/// pass None and are matched by destination/kind occurrences conservatively.
pub(crate) fn pdf_references(
    blocks: &[Block],
    sources: Option<&ChapterLinkSources>,
) -> Result<Vec<PdfReferenceSource>> {
    let indices = sources.and_then(|sources| sources.indices_for_blocks(blocks));
    let mut references = Vec::new();
    let mut bytes = 0usize;
    let mut source_bytes = 0usize;
    // PDF publishes the first definition of each source ID, including
    // otherwise dormant notes. Keep structural source indices unchanged, but
    // do not inventory references in duplicate bodies that are never painted.
    let mut retained_notes = Some(BTreeSet::new());
    for (owner, block) in blocks.iter().enumerate() {
        let complete = walk_blocks_with_notes(
            std::slice::from_ref(block),
            0,
            &mut retained_notes,
            &mut |node| {
                let index = indices
                    .as_ref()
                    .and_then(|indices| indices.get(&node.key()).copied());
                let (destination, note, linked_note, html) = match node {
                    Node::Link(_, destination) => (Cow::Borrowed(destination), false, false, false),
                    Node::Note(Inline::FootnoteRef { id }, linked) => {
                        (Cow::Borrowed(id.as_str()), true, linked, false)
                    }
                    Node::Note(_, _) => return true,
                    Node::Html(_, _, destination) => (Cow::Owned(destination), false, false, true),
                };
                if references.len() >= 16_384
                    || destination.len() > 8192
                    || destination.len() > (1024 * 1024usize).saturating_sub(bytes)
                {
                    return false;
                }
                bytes += destination.len();
                let mut source = sources.map_or_else(
                    || LinkSourceLocation {
                        precision: LinkSourcePrecision::Block,
                        source: BookSourceLocation::Unavailable("ownership_unavailable"),
                    },
                    |sources| sources.location(owner, index),
                );
                let retained = match &source.source {
                    BookSourceLocation::Mapped(ranges) => ranges
                        .iter()
                        .map(|range| range.path.len().saturating_add(32))
                        .sum::<usize>(),
                    BookSourceLocation::Unavailable(_) => 0,
                };
                if retained > MAX_PROVENANCE_BYTES.saturating_sub(source_bytes) {
                    source.source = BookSourceLocation::Unavailable("provenance_limit");
                } else {
                    source_bytes += retained;
                }
                let parsed_destination = sources
                    .and_then(|sources| {
                        index.and_then(|index| sources.parsed_destinations.get(index))
                    })
                    .and_then(Clone::clone);
                references.push(PdfReferenceSource {
                    destination: destination.into_owned(),
                    parsed_destination,
                    note,
                    linked_note,
                    html,
                    source,
                });
                true
            },
        );
        if !complete {
            return Err(crate::RenderError::InvalidInput(
                "book_pdf_verification_limit: reference inventory exceeds depth or report limits"
                    .into(),
            ));
        }
    }
    Ok(references)
}

/// These are temporary keys in a borrowed, immutable AST. Persisted metadata
/// contains only deterministic structural ordinals and source ranges. Cloning
/// a book or canonicalizing destinations therefore cannot stale its identities.
pub(super) type NodeKey = (u8, usize, usize);
enum Node<'a> {
    Link(&'a Inline, &'a str),
    Note(&'a Inline, bool),
    Html(&'a str, usize, String),
}
impl Node<'_> {
    fn key(&self) -> NodeKey {
        match self {
            Self::Link(node, _) | Self::Note(node, _) => inline_key(node),
            Self::Html(html, index, _) => html_key(html, *index),
        }
    }

    fn destination(&self) -> Option<&str> {
        match self {
            Self::Link(_, destination) => Some(destination),
            Self::Note(Inline::FootnoteRef { id }, _) => Some(id),
            Self::Note(_, _) => None,
            Self::Html(_, _, destination) => Some(destination),
        }
    }
}
pub(super) fn inline_key(node: &Inline) -> NodeKey {
    (0, std::ptr::from_ref(node) as usize, 0)
}
pub(super) fn html_key(html: &str, index: usize) -> NodeKey {
    (1, html.as_ptr() as usize, index)
}

fn walk_html<'a>(html: &'a str, visit: &mut impl FnMut(Node<'a>) -> bool) -> bool {
    crate::safe_html::url_spans(html)
        .filter(|span| span.kind == crate::safe_html::UrlKind::Link)
        .enumerate()
        .all(|(index, span)| visit(Node::Html(html, index, span.destination)))
}
fn walk_inlines<'a>(
    inlines: &'a [Inline],
    depth: usize,
    enclosing_link: bool,
    visit: &mut impl FnMut(Node<'a>) -> bool,
) -> bool {
    if depth > 128 {
        return false;
    }
    inlines.iter().all(|inline| match inline {
        Inline::Link { dest, content, .. } => {
            visit(Node::Link(inline, dest)) && walk_inlines(content, depth + 1, true, visit)
        }
        Inline::FootnoteRef { .. } => visit(Node::Note(inline, !enclosing_link)),
        Inline::Html(html) => walk_html(html, visit),
        Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
            walk_inlines(inner, depth + 1, enclosing_link, visit)
        }
        _ => true,
    })
}
fn walk_blocks<'a>(
    blocks: &'a [Block],
    depth: usize,
    visit: &mut impl FnMut(Node<'a>) -> bool,
) -> bool {
    walk_blocks_with_notes(blocks, depth, &mut None, visit)
}

fn walk_blocks_with_notes<'a>(
    blocks: &'a [Block],
    depth: usize,
    retained_notes: &mut Option<BTreeSet<&'a str>>,
    visit: &mut impl FnMut(Node<'a>) -> bool,
) -> bool {
    if depth > 128 {
        return false;
    }
    blocks.iter().all(|block| match block {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            walk_inlines(inlines, depth + 1, false, visit)
        }
        Block::HtmlBlock(html) => walk_html(html, visit),
        Block::BlockQuote(inner) => walk_blocks_with_notes(inner, depth + 1, retained_notes, visit),
        Block::FootnoteDefinition { id, blocks: inner } => {
            if retained_notes
                .as_mut()
                .is_some_and(|notes| !notes.insert(id.as_str()))
            {
                true
            } else {
                walk_blocks_with_notes(inner, depth + 1, retained_notes, visit)
            }
        }
        Block::List(list) => list
            .items
            .iter()
            .all(|item| walk_blocks_with_notes(&item.blocks, depth + 1, retained_notes, visit)),
        Block::Table(table) => table
            .head
            .iter()
            .chain(table.rows.iter().flatten())
            .all(|cell| walk_inlines(cell, depth + 1, false, visit)),
        Block::DefinitionList(items) => items.iter().all(|item| {
            item.terms
                .iter()
                .chain(&item.definitions)
                .all(|inlines| walk_inlines(inlines, depth + 1, false, visit))
        }),
        _ => true,
    })
}

pub(crate) fn check_with_sources(
    book: &crate::book::Book,
    sources: &BookLinkSources,
) -> Result<LocatedLinkReport> {
    let (report, origins) = check_book_links_inner(book, Some(sources))?;
    let mut locations = Vec::with_capacity(report.chapters.len());
    let mut metadata = Vec::with_capacity(report.chapters.len());
    let mut remaining = report.finding_count();
    let (mut paths, mut ranges, mut bytes) = (0usize, 0usize, 0usize);
    for (chapter_index, chapter) in report.chapters.iter().enumerate() {
        let mut chapter_locations = Vec::with_capacity(chapter.findings.len());
        let mut chapter_metadata = Vec::with_capacity(chapter.findings.len());
        for &(owner, index) in &origins[chapter_index] {
            let mut location = sources.chapter(chapter_index).location(owner, index);
            if let BookSourceLocation::Mapped(items) = &location.source {
                let size = items.iter().map(|item| item.path.len()).sum::<usize>();
                if size > (256 * 1024usize).saturating_sub(paths)
                    || items.len() > 16_384usize.saturating_sub(ranges)
                {
                    location.source = BookSourceLocation::Unavailable("report_provenance_limit");
                } else {
                    paths += size;
                    ranges += items.len();
                }
            }
            remaining -= 1;
            let mut json = location_json(&location)?;
            if json.len() + 10 > MAX_PROVENANCE_BYTES.saturating_sub(bytes + remaining * 128) {
                location.source = BookSourceLocation::Unavailable("report_provenance_limit");
                json = location_json(&location)?;
            }
            bytes += json.len() + 10;
            chapter_metadata.push(json);
            chapter_locations.push(location);
        }
        metadata.push(chapter_metadata);
        locations.push(chapter_locations);
    }
    let json = report.to_json_with_sources(Some(&metadata))?;
    Ok(LocatedLinkReport {
        report,
        locations,
        json,
    })
}

fn location_json(location: &LinkSourceLocation) -> Result<String> {
    let mut json = Json::new();
    match &location.source {
        BookSourceLocation::Unavailable(reason) => {
            json.raw("{\"status\":\"unavailable\",\"reason\":")?;
            json.string(reason)?;
            json.raw("}")?;
        }
        BookSourceLocation::Mapped(ranges) => {
            json.raw("{\"status\":\"mapped\",\"precision\":")?;
            json.string(match location.precision {
                LinkSourcePrecision::Destination => "destination",
                LinkSourcePrecision::Block => "block",
            })?;
            json.raw(",\"ranges\":[")?;
            for (index, range) in ranges.iter().enumerate() {
                if index != 0 {
                    json.raw(",")?;
                }
                json.raw("{\"path\":")?;
                json.string(&range.path)?;
                json.number("start", range.start)?;
                json.number("end", range.end)?;
                json.raw(",\"kind\":")?;
                json.string(range.kind.name())?;
                json.raw("}")?;
            }
            json.raw("]}")?;
        }
    }
    Ok(json.0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::book::{BookInput, build_book};

    #[test]
    fn missing_exact_tokens_are_explicit_block_ranges_not_fabricated_destinations() {
        let source = "one [a](#missing) and [b](#other)\n";
        let parsed = crate::parse_markdown_spanned(source);
        let sources = BookLinkSources::new(vec![ChapterLinkSources::capture(
            &parsed,
            &NavigationCapture::default(),
            "start.md",
        )]);
        let book = build_book(&[BookInput {
            path: "start.md".into(),
            source: source.into(),
        }])
        .unwrap();
        let report = check_with_sources(&book, &sources).unwrap();
        assert_eq!(report.report().finding_count(), 2);
        for index in 0..2 {
            let location = report.source_location(0, index).unwrap();
            assert_eq!(location.precision, LinkSourcePrecision::Block);
            let BookSourceLocation::Mapped(ranges) = &location.source else {
                panic!("missing block provenance")
            };
            assert_eq!(ranges.len(), 1);
            assert_eq!((ranges[0].start, ranges[0].end), (0, source.len() - 1));
        }
        assert!(report.to_json().contains("\"precision\":\"block\""));
        let absent = BookLinkSources::unavailable(std::iter::once(source.len()));
        let absent = check_with_sources(&book, &absent).unwrap();
        assert_eq!(absent.report(), report.report());
        assert_eq!(
            absent.source_location(0, 0).unwrap().source,
            BookSourceLocation::Unavailable("not_captured")
        );
    }

    #[test]
    fn optional_link_metadata_does_not_consume_the_original_base_json_budget() {
        let mut report = LinkReport {
            chapters: vec![super::super::ChapterLinks {
                path: "a.md".into(),
                checked: 1,
                external: 0,
                unchecked: 0,
                findings: vec![super::super::LinkFinding {
                    code: "missing_anchor",
                    destination: "#missing".into(),
                    message: "Missing anchor.",
                }],
            }],
        };
        let base = report.to_json().unwrap().len();
        report.chapters[0].path = format!(
            "{}a.md",
            "x".repeat(super::super::MAX_REPORT_BYTES - base - 8)
        );
        let metadata = vec![vec![
            location_json(&LinkSourceLocation {
                precision: LinkSourcePrecision::Block,
                source: BookSourceLocation::Unavailable("provenance_limit"),
            })
            .unwrap(),
        ]];
        let json = report.to_json_with_sources(Some(&metadata)).unwrap();
        assert!(json.len() > super::super::MAX_REPORT_BYTES);
        assert!(json.len() <= super::super::MAX_REPORT_BYTES + MAX_PROVENANCE_BYTES);
        report.chapters[0].path.insert_str(0, "sixteen_more_byte");
        assert!(
            report
                .to_json_with_sources(Some(&metadata))
                .unwrap_err()
                .to_string()
                .contains("report exceeds 4 MiB")
        );
    }

    #[test]
    fn exact_capture_preserves_normal_parse_ast_diagnostics_and_literal_fallbacks() {
        for source in [
            "# Head\n\n[broken]:\n\n```rust\n[not a link](#missing)\n",
            "[outer [inner](#x)](#y) **[ok](#z)**\n\n> [r]: <missing.md>\n>\n> [ref][r]\n",
            "| a | b |\n|---|---|\n| [x](#missing) | `![x](#hidden)` |\n\nTerm\n: [x](#missing)\n",
            "# 😀\r\n\r\n[x](#nul\0id)\r\n\r\n<div><a href='#missing'>x</a></div>\r\n",
        ] {
            for profile in [
                None,
                Some(crate::Profile::CommonMarkGfm),
                Some(crate::Profile::GfmPlus),
            ] {
                let ordinary = crate::parse_markdown_spanned_with_profile(source, profile);
                let (captured, _) = crate::parse::parse_book_document(source, profile);
                assert_eq!(ordinary, captured, "source={source:?}, profile={profile:?}");
            }
        }
    }
}
