//! Lossless PDF footnote preparation over the shared document AST.
//!
//! The PDF layout engine consumes ordinary blocks and inline text. Resolve
//! references once, move complete note bodies to the end, and leave their
//! internal block structure intact. Note-reference cycles are graph edges,
//! not recursive expansion: every definition is emitted at most once.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Block, Document, Inline};

#[path = "footnotes/endnote_navigation.rs"]
mod endnote_navigation;
pub use endnote_navigation::{EndnoteTarget, PreparedEndnotes};
#[path = "footnotes/root_origins.rs"]
mod root_origins;
pub(crate) use root_origins::paged_root_origins;

/// Internal provenance for a document whose page notes have been prepared.
///
/// A source `Document` can contain any footnote identifier and any leading
/// text. Neither a destination-shaped ID nor a matching superscript proves
/// that its notes have already been numbered. Only this wrapper carries that
/// fact; borrowing it for another internal pass never applies the transform
/// again. The source AST remains unchanged and no marker enters output.
pub(crate) struct PreparedPdfDocument<'a>(Cow<'a, Document>);

impl PreparedPdfDocument<'_> {
    pub(crate) fn into_owned(self) -> PreparedPdfDocument<'static> {
        PreparedPdfDocument(Cow::Owned(self.0.into_owned()))
    }
}

impl std::ops::Deref for PreparedPdfDocument<'_> {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.0
    }
}

impl AsRef<Document> for PreparedPdfDocument<'_> {
    fn as_ref(&self) -> &Document {
        &self.0
    }
}

/// Explicit source/prepared distinction for private preparation and warning
/// passes. Every public `Document` input is source, even when its contents
/// happen to be identical to an internal prepared representation.
pub(crate) trait PdfDocumentInput {
    fn document(&self) -> &Document;

    fn is_pdf_prepared(&self) -> bool {
        false
    }
}

impl PdfDocumentInput for Document {
    fn document(&self) -> &Document {
        self
    }
}

impl PdfDocumentInput for PreparedPdfDocument<'_> {
    fn document(&self) -> &Document {
        self
    }

    fn is_pdf_prepared(&self) -> bool {
        true
    }
}

/// Typeset references as superscript numerals (`claim¹`, the way a printed
/// document marks notes) while retaining every supported block in each note. Number referenced notes by first use in the
/// body, then traverse note-to-note references in that order. Definitions
/// without references follow in source order, preserving the legacy policy
/// that an explicitly supplied definition is not silently discarded.
///
/// Duplicate definitions use the first definition. Undefined references stay
/// visible as `[^id]`, never the misleading `[0]`. A document with no footnote
/// syntax is borrowed, avoiding a whole-AST clone on the ordinary PDF path.
pub(crate) fn for_pdf(doc: &Document) -> Cow<'_, Document> {
    prepare(doc, false, false)
}

/// Prefix of the fragment a page-bottom note reference links to, and of the
/// id its prepared definition carries (`fmd-fn-3` for note 3).
pub(crate) const PAGE_NOTE_PREFIX: &str = "fmd-fn-";

/// Private destination of a prepared note's first linked citation.
pub(crate) const PAGE_NOTE_REFERENCE_PREFIX: &str = "fmd-fnref-";

/// [`for_pdf`] for the PDF page builder's bottom-of-page notes. References
/// become `#fmd-fn-N` links around the superscript numeral, and each note
/// body stays a `FootnoteDefinition` with id `fmd-fn-N` at the end of the
/// document, in note order, already labeled. The PDF layout places each note
/// at the foot of the page carrying its first body reference, and prints any
/// note it cannot place there (unreferenced, referenced only from another
/// note, or too tall) in a trailing Notes section exactly like [`for_pdf`].
pub(crate) fn for_pdf_paged(doc: &impl PdfDocumentInput) -> PreparedPdfDocument<'_> {
    prepare_paged(doc, false)
}

/// Page notes with optional return links on their existing superscript labels.
/// The writer resolves these after final pagination; preparation cannot know
/// the citing page or whether a label has a rendered citation to return to.
pub(crate) fn for_pdf_paged_with_backlinks(
    doc: &impl PdfDocumentInput,
    backlinks: bool,
) -> PreparedPdfDocument<'_> {
    if backlinks {
        prepare_paged(doc, true)
    } else {
        for_pdf_paged(doc)
    }
}

fn prepare_paged(doc: &impl PdfDocumentInput, backlinks: bool) -> PreparedPdfDocument<'_> {
    PreparedPdfDocument(if doc.is_pdf_prepared() {
        Cow::Borrowed(doc.document())
    } else {
        prepare(doc.document(), true, backlinks)
    })
}

/// Original definition ID to the ID carried by its prepared page note. Book
/// navigation resolves source anchors through this map, then uses the note's
/// actual emitted destination; first-use order and endnote fallback are never
/// inferred from definition order or estimated page positions.
///
/// Use the same HTML lowering, definition collection and numbering as
/// [`for_pdf_paged`]. Already prepared notes retain their existing IDs.
pub(crate) fn page_note_ids(doc: &impl PdfDocumentInput) -> BTreeMap<String, String> {
    let lowered = crate::safe_html::lower(doc.document());
    let mut notes = Notes::default();
    notes.collect(&lowered.blocks);
    if doc.is_pdf_prepared() {
        return notes
            .indices
            .keys()
            .map(|id| ((*id).to_string(), (*id).to_string()))
            .collect();
    }
    let numbering = number_notes(&lowered, &notes);
    notes
        .indices
        .into_iter()
        .map(|(id, index)| {
            (
                id.to_string(),
                format!("{PAGE_NOTE_PREFIX}{}", numbering.numbers[index]),
            )
        })
        .collect()
}

fn prepare(doc: &Document, paged: bool, backlinks: bool) -> Cow<'_, Document> {
    // Paged output cannot pass markup through: lower the safe HTML subset to
    // native nodes first (idempotent, borrowed when there is no raw HTML).
    match crate::safe_html::lower(doc) {
        Cow::Borrowed(doc) => endnotes_for_pdf(doc, paged, backlinks),
        Cow::Owned(lowered) => {
            Cow::Owned(endnotes_for_pdf(&lowered, paged, backlinks).into_owned())
        }
    }
}

fn endnotes_for_pdf(doc: &Document, paged: bool, backlinks: bool) -> Cow<'_, Document> {
    let mut notes = Notes::default();
    notes.collect(&doc.blocks);
    if notes.definitions.is_empty() && !notes.has_reference {
        return Cow::Borrowed(doc);
    }
    let mut numbering = number_notes(doc, &notes);
    numbering.paged = paged;
    Cow::Owned(assemble_notes(doc, &notes, &numbering, backlinks))
}

fn assemble_notes(
    doc: &Document,
    notes: &Notes<'_>,
    numbering: &Numbering,
    backlinks: bool,
) -> Document {
    let paged = numbering.paged;
    let mut blocks = rewrite_blocks(&doc.blocks, notes, numbering);
    if !notes.definitions.is_empty() && !paged {
        blocks.push(Block::Heading {
            level: 2,
            inlines: vec![Inline::Text("Notes".to_string())],
        });
        for &index in &numbering.order {
            let mut body = rewrite_blocks(notes.definitions[index], notes, numbering);
            let label = superscript(numbering.numbers[index]);
            match body.first_mut() {
                Some(Block::Paragraph(inlines)) if paragraph_accepts_note_label(inlines) => {
                    inlines.insert(0, Inline::Text(format!("{label} ")));
                }
                _ => {
                    // A code fence, table, list, image, equation, or heading
                    // is not a paragraph to flatten. Give it a separate note
                    // label and preserve the original blocks underneath it.
                    body.insert(0, Block::Paragraph(vec![Inline::Text(label)]));
                }
            }
            blocks.extend(body);
        }
    }
    if paged {
        let prepared_notes: Vec<_> = numbering
            .order
            .iter()
            .map(|&index| {
                (
                    numbering.numbers[index],
                    rewrite_blocks(notes.definitions[index], notes, numbering),
                )
            })
            .collect();
        let mut references = BTreeSet::new();
        if backlinks {
            references.extend(page_note_references(&blocks));
            for (_, body) in &prepared_notes {
                references.extend(page_note_references(body));
            }
        }
        for (number, mut body) in prepared_notes {
            let backlinks =
                backlinks && u32::try_from(number).is_ok_and(|number| references.contains(&number));
            let label = superscript(number);
            match body.first_mut() {
                Some(Block::Paragraph(inlines)) if paragraph_accepts_note_label(inlines) => {
                    if backlinks {
                        inlines.insert(0, Inline::Text(" ".to_string()));
                        inlines.insert(0, page_note_backlink(number, label));
                    } else {
                        inlines.insert(0, Inline::Text(format!("{label} ")));
                    }
                }
                _ => body.insert(
                    0,
                    Block::Paragraph(vec![if backlinks {
                        page_note_backlink(number, label)
                    } else {
                        Inline::Text(label)
                    }]),
                ),
            }
            blocks.push(Block::FootnoteDefinition {
                id: format!("{PAGE_NOTE_PREFIX}{number}"),
                blocks: body,
            });
        }
    }
    Document { blocks }
}

/// Keep figure-only paragraphs intact. Prefixing their first image with text
/// changes a standalone figure or gallery into prose, making PDF choose
/// inline-image sizing. This decision uses only source structure, so missing
/// resources and custom assets follow the same preparation path.
fn paragraph_accepts_note_label(inlines: &[Inline]) -> bool {
    if crate::toc::marker_depth(inlines).is_some() {
        return false;
    }
    let mut pending = vec![inlines];
    let mut has_image = false;
    while let Some(inlines) = pending.pop() {
        for inline in inlines {
            match inline {
                Inline::Image { .. } => has_image = true,
                Inline::Text(text) if text.trim().is_empty() => {}
                Inline::SoftBreak | Inline::HardBreak => {}
                Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                    pending.push(inner);
                }
                Inline::Link { content, .. } => pending.push(content),
                _ => return true,
            }
        }
    }
    !has_image
}

fn page_note_backlink(number: usize, label: String) -> Inline {
    Inline::Link {
        dest: format!("#{PAGE_NOTE_REFERENCE_PREFIX}{number}"),
        title: None,
        content: vec![Inline::Text(label)],
    }
}

/// Prepared note links that can supply return destinations. Nested note marks
/// are already plain text, so an enclosing ordinary link cannot manufacture a
/// return target. The anchor audit uses the same inventory as label preparation.
pub(crate) fn page_note_references(blocks: &[Block]) -> BTreeSet<u32> {
    fn inlines(items: &[Inline], references: &mut BTreeSet<u32>) {
        for inline in items {
            match inline {
                Inline::Link { dest, content, .. } if !content.is_empty() => {
                    if let Some(number) = dest
                        .strip_prefix('#')
                        .and_then(crate::anchors::decode_fragment)
                        .and_then(|id| id.strip_prefix(PAGE_NOTE_PREFIX)?.parse::<u32>().ok())
                        .filter(|&number| number != 0 && number != u32::MAX)
                    {
                        references.insert(number);
                    }
                }
                Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                    inlines(inner, references);
                }
                _ => {}
            }
        }
    }
    fn walk(blocks: &[Block], references: &mut BTreeSet<u32>) {
        for block in blocks {
            match block {
                Block::Paragraph(items) | Block::Heading { inlines: items, .. } => {
                    inlines(items, references)
                }
                Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                    walk(inner, references)
                }
                Block::List(list) => {
                    for item in &list.items {
                        walk(&item.blocks, references);
                    }
                }
                Block::Table(table) => {
                    for cell in table.head.iter().chain(table.rows.iter().flatten()) {
                        inlines(cell, references);
                    }
                }
                Block::DefinitionList(items) => {
                    for item in items {
                        for content in item.terms.iter().chain(&item.definitions) {
                            inlines(content, references);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut references = BTreeSet::new();
    walk(blocks, &mut references);
    references
}

fn number_notes(doc: &Document, notes: &Notes<'_>) -> Numbering {
    let mut numbering = Numbering {
        numbers: vec![0; notes.definitions.len()],
        order: Vec::with_capacity(notes.definitions.len()),
        paged: false,
    };
    references_in_blocks(&doc.blocks, &mut |id| numbering.reference(notes, id));
    let mut visited = 0;
    numbering.follow_references(notes, &mut visited);
    for index in 0..notes.definitions.len() {
        numbering.assign(index);
        numbering.follow_references(notes, &mut visited);
    }
    numbering
}

/// Source-heading ordinal for each heading in `for_pdf(doc)`, in output
/// order. `None` denotes the generated Notes heading. Source ordinals include
/// headings inside definitions, including duplicates that are later omitted.
///
/// Book navigation uses this sidecar instead of matching titles: two chapters
/// may contain identical headings, and referenced note bodies can move into a
/// different order. The same definition collection and numbering code drives
/// both preparation and provenance. Nothing is embedded in the visible AST.
pub(crate) fn heading_origins_for_pdf(doc: &Document) -> Vec<Option<usize>> {
    heading_origins(doc, false)
}

/// Source-heading ordinals after page-note preparation. The layout engine may
/// subsequently move note bodies or generate a fallback Notes heading; callers
/// must use emitted source destinations, not assume physical bookmark order.
pub(crate) fn heading_origins_for_pdf_paged(doc: &Document) -> Vec<Option<usize>> {
    heading_origins(doc, true)
}

fn heading_origins(doc: &Document, paged: bool) -> Vec<Option<usize>> {
    let mut original = BTreeMap::new();
    visit_headings(&doc.blocks, true, &mut |block| {
        let ordinal = original.len();
        // Pointer identity is used only for lookup against this immutable AST.
        // Addresses and map iteration order never enter the returned sidecar.
        original.insert(std::ptr::from_ref(block), ordinal);
    });
    let mut notes = Notes::default();
    notes.collect(&doc.blocks);
    let numbering = number_notes(doc, &notes);
    let mut origins = Vec::with_capacity(original.len().saturating_add(1));
    visit_headings(&doc.blocks, false, &mut |block| {
        origins.push(original.get(&std::ptr::from_ref(block)).copied());
    });
    if !notes.definitions.is_empty() {
        if !paged {
            origins.push(None);
        }
        for index in numbering.order {
            visit_headings(notes.definitions[index], false, &mut |block| {
                origins.push(original.get(&std::ptr::from_ref(block)).copied());
            });
        }
    }
    origins
}

fn visit_headings(blocks: &[Block], definitions: bool, visit: &mut impl FnMut(&Block)) {
    for block in blocks {
        match block {
            Block::Heading { .. } => visit(block),
            Block::BlockQuote(inner) => visit_headings(inner, definitions, visit),
            Block::List(list) => {
                for item in &list.items {
                    visit_headings(&item.blocks, definitions, visit);
                }
            }
            Block::FootnoteDefinition { blocks, .. } if definitions => {
                visit_headings(blocks, definitions, visit);
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct Notes<'a> {
    indices: BTreeMap<&'a str, usize>,
    definitions: Vec<&'a [Block]>,
    has_reference: bool,
}

impl<'a> Notes<'a> {
    fn collect(&mut self, blocks: &'a [Block]) {
        for block in blocks {
            match block {
                Block::FootnoteDefinition { id, blocks } => {
                    if !self.indices.contains_key(id.as_str()) {
                        let index = self.definitions.len();
                        self.indices.insert(id, index);
                        self.definitions.push(blocks);
                        self.collect(blocks);
                    }
                }
                Block::BlockQuote(inner) => self.collect(inner),
                Block::List(list) => {
                    for item in &list.items {
                        self.collect(&item.blocks);
                    }
                }
                _ => references_in_block(block, &mut |_| self.has_reference = true),
            }
        }
    }
}

struct Numbering {
    numbers: Vec<usize>,
    order: Vec<usize>,
    /// References link to their page-bottom note (see [`for_pdf_paged`]).
    paged: bool,
}

impl Numbering {
    fn assign(&mut self, index: usize) {
        if self.numbers[index] == 0 {
            self.numbers[index] = self.order.len() + 1;
            self.order.push(index);
        }
    }

    fn reference(&mut self, notes: &Notes<'_>, id: &str) {
        if let Some(&index) = notes.indices.get(id) {
            self.assign(index);
        }
    }

    fn follow_references(&mut self, notes: &Notes<'_>, visited: &mut usize) {
        while *visited < self.order.len() {
            let index = self.order[*visited];
            *visited += 1;
            references_in_blocks(notes.definitions[index], &mut |id| {
                self.reference(notes, id)
            });
        }
    }
}

/// Definitions do not appear in normal flow. Their references are traversed
/// separately when their turn arrives in the note queue.
fn references_in_blocks(blocks: &[Block], visit: &mut impl FnMut(&str)) {
    for block in blocks {
        references_in_block(block, visit);
    }
}

fn references_in_block(block: &Block, visit: &mut impl FnMut(&str)) {
    match block {
        Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
            references_in_inlines(inlines, visit);
        }
        Block::BlockQuote(inner) => references_in_blocks(inner, visit),
        Block::List(list) => {
            for item in &list.items {
                references_in_blocks(&item.blocks, visit);
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                references_in_inlines(cell, visit);
            }
            for row in &table.rows {
                for cell in row {
                    references_in_inlines(cell, visit);
                }
            }
        }
        Block::DefinitionList(items) => {
            for item in items {
                for inlines in item.terms.iter().chain(&item.definitions) {
                    references_in_inlines(inlines, visit);
                }
            }
        }
        Block::FootnoteDefinition { .. }
        | Block::CodeBlock { .. }
        | Block::ThematicBreak
        | Block::HtmlBlock(_)
        | Block::MathBlock(_)
        | Block::PageBreak => {}
    }
}

fn references_in_inlines(inlines: &[Inline], visit: &mut impl FnMut(&str)) {
    for inline in inlines {
        match inline {
            Inline::FootnoteRef { id } => visit(id),
            Inline::Emphasis(content)
            | Inline::Strong(content)
            | Inline::Strikethrough(content)
            | Inline::Link { content, .. } => references_in_inlines(content, visit),
            _ => {}
        }
    }
}

fn rewrite_blocks(blocks: &[Block], notes: &Notes<'_>, numbering: &Numbering) -> Vec<Block> {
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        let rewritten = match block {
            Block::FootnoteDefinition { .. } => continue,
            Block::Paragraph(inlines) => {
                Block::Paragraph(rewrite_inlines(inlines, notes, numbering))
            }
            Block::Heading { level, inlines } => Block::Heading {
                level: *level,
                inlines: rewrite_inlines(inlines, notes, numbering),
            },
            Block::BlockQuote(inner) => Block::BlockQuote(rewrite_blocks(inner, notes, numbering)),
            Block::List(list) => {
                let mut list = list.clone();
                for item in &mut list.items {
                    item.blocks = rewrite_blocks(&item.blocks, notes, numbering);
                }
                Block::List(list)
            }
            Block::Table(table) => {
                let mut table = table.clone();
                for cell in &mut table.head {
                    *cell = rewrite_inlines(cell, notes, numbering);
                }
                for row in &mut table.rows {
                    for cell in row {
                        *cell = rewrite_inlines(cell, notes, numbering);
                    }
                }
                Block::Table(table)
            }
            Block::DefinitionList(items) => {
                let mut items = items.clone();
                for item in &mut items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        *inlines = rewrite_inlines(inlines, notes, numbering);
                    }
                }
                Block::DefinitionList(items)
            }
            other => other.clone(),
        };
        out.push(rewritten);
    }
    out
}

/// A note number in Unicode superscript digits (every bundled body face and
/// the SVG/PDF fallback carry them).
fn superscript(number: usize) -> String {
    const DIGITS: [char; 10] = [
        '\u{2070}', '\u{B9}', '\u{B2}', '\u{B3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}',
        '\u{2078}', '\u{2079}',
    ];
    number
        .to_string()
        .bytes()
        .map(|digit| DIGITS[usize::from(digit - b'0')])
        .collect()
}

fn rewrite_inlines(inlines: &[Inline], notes: &Notes<'_>, numbering: &Numbering) -> Vec<Inline> {
    rewrite_inlines_in(inlines, notes, numbering, numbering.paged)
}

/// `link_refs` is false inside an existing link: links do not nest.
fn rewrite_inlines_in(
    inlines: &[Inline],
    notes: &Notes<'_>,
    numbering: &Numbering,
    link_refs: bool,
) -> Vec<Inline> {
    let rewrite_inlines = |content: &[Inline], notes: &Notes<'_>, numbering: &Numbering| {
        rewrite_inlines_in(content, notes, numbering, link_refs)
    };
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::FootnoteRef { id } => match notes.indices.get(id.as_str()) {
                Some(&index) if link_refs => {
                    let number = numbering.numbers[index];
                    Inline::Link {
                        dest: format!("#{PAGE_NOTE_PREFIX}{number}"),
                        title: None,
                        content: vec![Inline::Text(superscript(number))],
                    }
                }
                Some(&index) => Inline::Text(superscript(numbering.numbers[index])),
                None => Inline::Text(format!("[^{id}]")),
            },
            Inline::Emphasis(content) => {
                Inline::Emphasis(rewrite_inlines(content, notes, numbering))
            }
            Inline::Strong(content) => Inline::Strong(rewrite_inlines(content, notes, numbering)),
            Inline::Strikethrough(content) => {
                Inline::Strikethrough(rewrite_inlines(content, notes, numbering))
            }
            Inline::Link {
                dest,
                title,
                content,
            } => Inline::Link {
                dest: dest.clone(),
                title: title.clone(),
                content: rewrite_inlines_in(content, notes, numbering, false),
            },
            other => other.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Align, DefinitionItem, List, ListItem, Table};

    fn text(value: &str) -> Inline {
        Inline::Text(value.to_string())
    }
    fn reference(id: &str) -> Inline {
        Inline::FootnoteRef { id: id.to_string() }
    }
    fn paragraph(value: &str) -> Block {
        Block::Paragraph(vec![text(value)])
    }
    fn definition(id: &str, blocks: Vec<Block>) -> Block {
        Block::FootnoteDefinition {
            id: id.to_string(),
            blocks,
        }
    }
    fn document(blocks: Vec<Block>) -> Document {
        Document { blocks }
    }

    #[test]
    fn note_free_document_is_borrowed_including_literal_footnote_text() {
        let doc = document(vec![
            paragraph("literal [^id]"),
            Block::CodeBlock {
                lang: None,
                code: "[^id]: not a parsed definition".into(),
            },
        ]);
        assert!(matches!(for_pdf(&doc), Cow::Borrowed(_)));
    }

    #[test]
    fn single_paragraph_note_keeps_its_shape() {
        let doc = document(vec![
            Block::Paragraph(vec![text("Body"), reference("a")]),
            definition("a", vec![paragraph("Citation")]),
        ]);
        assert_eq!(
            for_pdf(&doc).as_ref(),
            &document(vec![
                Block::Paragraph(vec![text("Body"), text("¹")]),
                Block::Heading {
                    level: 2,
                    inlines: vec![text("Notes")]
                },
                Block::Paragraph(vec![text("¹ "), text("Citation")]),
            ])
        );
    }

    #[test]
    fn first_figure_notes_preserve_blocks_and_optional_return_labels() -> Result<(), String> {
        let image = Inline::Image {
            dest: "plot.svg".into(),
            title: Some("Original title".into()),
            alt: "Evidence plot".into(),
        };
        let linked = Inline::Link {
            dest: "https://example.com/evidence".into(),
            title: None,
            content: vec![Inline::Strong(vec![Inline::Emphasis(vec![image.clone()])])],
        };
        for inlines in [
            vec![image.clone()],
            vec![linked.clone()],
            vec![Inline::Strikethrough(vec![image.clone()])],
            vec![
                image.clone(),
                text(" \t"),
                Inline::SoftBreak,
                linked,
                Inline::HardBreak,
                image.clone(),
            ],
        ] {
            let figure = Block::Paragraph(inlines);
            let doc = document(vec![
                Block::Paragraph(vec![text("Evidence"), reference("plot")]),
                definition("plot", vec![figure.clone()]),
            ]);
            let original = doc.clone();
            let endnotes = for_pdf(&doc);
            assert_eq!(endnotes.blocks.len(), 4);
            assert_eq!(endnotes.blocks[2], paragraph("¹"));
            assert_eq!(endnotes.blocks[3], figure);
            for backlinks in [false, true] {
                let prepared = for_pdf_paged_with_backlinks(&doc, backlinks);
                let Some(Block::FootnoteDefinition { id, blocks }) = prepared.blocks.get(1) else {
                    return Err(format!(
                        "missing prepared figure note: {:?}",
                        prepared.as_ref()
                    ));
                };
                assert_eq!(id, "fmd-fn-1");
                assert_eq!(blocks.len(), 2);
                assert_eq!(blocks[1], figure);
                let label = if backlinks {
                    Inline::Link {
                        dest: "#fmd-fnref-1".into(),
                        title: None,
                        content: vec![text("¹")],
                    }
                } else {
                    text("¹")
                };
                assert_eq!(blocks[0], Block::Paragraph(vec![label]));
                assert!(matches!(
                    for_pdf_paged_with_backlinks(&prepared, backlinks).0,
                    Cow::Borrowed(_)
                ));
            }
            assert_eq!(doc, original);
        }
        Ok(())
    }

    #[test]
    fn mixed_image_prose_keeps_its_inline_note_label() -> Result<(), String> {
        let prose = vec![
            text("See "),
            Inline::Image {
                dest: "plot.svg".into(),
                title: None,
                alt: "Evidence plot".into(),
            },
            text(" for evidence."),
        ];
        let doc = document(vec![
            Block::Paragraph(vec![reference("plot")]),
            definition("plot", vec![Block::Paragraph(prose.clone())]),
        ]);
        let mut expected = vec![text("¹ ")];
        expected.extend(prose.clone());
        let endnotes = for_pdf(&doc);
        assert_eq!(endnotes.blocks.len(), 3);
        assert_eq!(endnotes.blocks[2], Block::Paragraph(expected.clone()));
        let paged = for_pdf_paged(&doc);
        let Some(Block::FootnoteDefinition { blocks, .. }) = paged.blocks.get(1) else {
            return Err(format!("missing prepared prose note: {:?}", paged.as_ref()));
        };
        assert_eq!(blocks, &[Block::Paragraph(expected)]);
        let linked = for_pdf_paged_with_backlinks(&doc, true);
        let Some(Block::FootnoteDefinition { blocks, .. }) = linked.blocks.get(1) else {
            return Err(format!("missing linked prose note: {:?}", linked.as_ref()));
        };
        let mut expected = vec![
            Inline::Link {
                dest: "#fmd-fnref-1".into(),
                title: None,
                content: vec![text("¹")],
            },
            text(" "),
        ];
        expected.extend(prose);
        assert_eq!(blocks, &[Block::Paragraph(expected)]);
        Ok(())
    }

    #[test]
    fn note_numbers_are_superscript_numerals() {
        assert_eq!(superscript(1), "\u{B9}");
        assert_eq!(superscript(10), "\u{B9}\u{2070}");
        assert_eq!(superscript(2345), "\u{B2}\u{B3}\u{2074}\u{2075}");
        assert_eq!(superscript(6789), "\u{2076}\u{2077}\u{2078}\u{2079}");
    }

    #[test]
    fn numbering_follows_references_not_definition_order() {
        let doc = document(vec![
            definition("a", vec![paragraph("A")]),
            definition("b", vec![paragraph("B")]),
            Block::Paragraph(vec![reference("b"), reference("a"), reference("b")]),
        ]);
        let transformed = for_pdf(&doc);
        assert_eq!(
            transformed.blocks[0],
            Block::Paragraph(vec![text("¹"), text("²"), text("¹")])
        );
        assert_eq!(
            transformed.blocks[2],
            Block::Paragraph(vec![text("¹ "), text("B")])
        );
        assert_eq!(
            transformed.blocks[3],
            Block::Paragraph(vec![text("² "), text("A")])
        );
    }

    #[test]
    fn every_rich_note_block_survives_in_order_and_sources_are_unchanged() {
        let rich = vec![
            Block::CodeBlock {
                lang: Some("rust".into()),
                code: "let evidence = 42;".into(),
            },
            Block::BlockQuote(vec![paragraph("Quoted evidence")]),
            Block::List(List {
                ordered: true,
                start: 3,
                tight: false,
                items: vec![ListItem {
                    task: Some(true),
                    blocks: vec![paragraph("List evidence")],
                }],
            }),
            Block::Table(Table {
                align: vec![Align::Right],
                head: vec![vec![text("Measure")]],
                rows: vec![vec![vec![text("42")]]],
            }),
            Block::MathBlock("x^2".into()),
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![text("Term")]],
                definitions: vec![vec![text("Meaning")]],
            }]),
            Block::Paragraph(vec![Inline::Image {
                dest: "plot.svg".into(),
                title: None,
                alt: "Plot".into(),
            }]),
            Block::HtmlBlock("<aside>Raw evidence</aside>".into()),
            Block::Heading {
                level: 3,
                inlines: vec![text("Appendix")],
            },
            Block::ThematicBreak,
            Block::PageBreak,
            paragraph("Last evidence"),
        ];
        let doc = document(vec![
            Block::Paragraph(vec![reference("rich")]),
            definition("rich", rich.clone()),
        ]);
        let original = doc.clone();
        // Endnote rewriting alone must keep every block, raw HTML included;
        // `for_pdf` additionally lowers that HTML (see `safe_html`).
        let result = endnotes_for_pdf(&doc, false, false);
        assert_eq!(result.blocks[2], paragraph("¹"));
        assert_eq!(&result.blocks[3..], rich.as_slice());
        let prepared = for_pdf(&doc);
        assert_eq!(prepared.blocks[10], paragraph("Raw evidence"));
        assert_eq!(doc, original);
    }

    #[test]
    fn references_inside_tables_and_definition_lists_are_resolved() {
        let doc = document(vec![
            Block::Table(Table {
                align: vec![Align::Left],
                head: vec![vec![reference("b")]],
                rows: vec![vec![vec![Inline::Strong(vec![reference("a")])]]],
            }),
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![reference("a")]],
                definitions: vec![vec![Inline::Link {
                    dest: "https://example.com".into(),
                    title: Some("Link".into()),
                    content: vec![reference("b")],
                }]],
            }]),
            definition("a", vec![paragraph("A")]),
            definition("b", vec![paragraph("B")]),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(
            result.blocks[0],
            Block::Table(Table {
                align: vec![Align::Left],
                head: vec![vec![text("¹")]],
                rows: vec![vec![vec![Inline::Strong(vec![text("²")])]]]
            })
        );
        assert_eq!(
            result.blocks[1],
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![text("²")]],
                definitions: vec![vec![Inline::Link {
                    dest: "https://example.com".into(),
                    title: Some("Link".into()),
                    content: vec![text("¹")]
                }]],
            }])
        );
    }

    #[test]
    fn nested_reference_cycles_emit_each_note_once_without_recursive_expansion() {
        let doc = document(vec![
            Block::Paragraph(vec![reference("a")]),
            definition("a", vec![Block::Paragraph(vec![text("A"), reference("b")])]),
            definition("b", vec![Block::Paragraph(vec![text("B"), reference("a")])]),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(result.blocks.len(), 4);
        assert_eq!(
            result.blocks[2],
            Block::Paragraph(vec![text("¹ "), text("A"), text("²")])
        );
        assert_eq!(
            result.blocks[3],
            Block::Paragraph(vec![text("² "), text("B"), text("¹")])
        );
    }

    #[test]
    fn first_definition_wins_and_unreferenced_notes_are_not_lost() {
        let doc = document(vec![
            definition("unused", vec![paragraph("Unreferenced evidence")]),
            Block::Paragraph(vec![reference("a")]),
            definition("a", vec![paragraph("First definition")]),
            definition("a", vec![paragraph("Duplicate definition")]),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(result.blocks.len(), 4);
        assert_eq!(
            result.blocks[2],
            Block::Paragraph(vec![text("¹ "), text("First definition")])
        );
        assert_eq!(
            result.blocks[3],
            Block::Paragraph(vec![text("² "), text("Unreferenced evidence")])
        );
    }

    #[test]
    fn source_note_ids_are_not_preparation_markers() {
        for id in [
            "fmd-fn-evidence",
            "fmd-fn-1",
            "fmd-fn-0",
            "fmd-fn-01",
            "fmd-fn-4294967295",
            "fmd-fn-4294967296",
        ] {
            let source = document(vec![
                paragraph("Body"),
                definition(id, vec![paragraph("Evidence")]),
            ]);
            let expected = document(vec![
                paragraph("Body"),
                definition(
                    "fmd-fn-1",
                    vec![Block::Paragraph(vec![text("¹ "), text("Evidence")])],
                ),
            ]);
            for backlinks in [false, true] {
                let prepared = for_pdf_paged_with_backlinks(&source, backlinks);
                assert_eq!(prepared.as_ref(), &expected, "{id}, backlinks={backlinks}");
                assert_eq!(
                    page_note_ids(&source).get(id),
                    Some(&"fmd-fn-1".to_string())
                );
                assert!(matches!(
                    for_pdf_paged_with_backlinks(&prepared, backlinks).0,
                    Cow::Borrowed(_)
                ));
                assert_eq!(
                    page_note_ids(&prepared).get("fmd-fn-1"),
                    Some(&"fmd-fn-1".to_string())
                );
            }
        }
    }

    #[test]
    fn matching_authored_superscripts_are_not_preparation_provenance() {
        for body in [
            "¹ Original text",
            "¹ **Original text**",
            "¹",
            "² Original text",
        ] {
            let authored = crate::parse_markdown(&format!("[^fmd-fn-1]: {body}\n"));
            let ordinary = crate::parse_markdown(&format!("[^evidence]: {body}\n"));
            for backlinks in [false, true] {
                let prepared = for_pdf_paged_with_backlinks(&authored, backlinks);
                assert_eq!(
                    prepared.as_ref(),
                    for_pdf_paged_with_backlinks(&ordinary, backlinks).as_ref(),
                    "author text {body:?} must survive numbering"
                );
                let reused = for_pdf_paged_with_backlinks(&prepared, backlinks);
                assert!(matches!(&reused.0, Cow::Borrowed(_)));
                assert!(std::ptr::eq(reused.as_ref(), prepared.as_ref()));
            }
        }
    }

    #[test]
    fn page_note_return_labels_require_a_real_linked_citation() -> Result<(), String> {
        let doc = crate::parse_markdown(
            "Claim[^one]. [Ordinary link[^hidden]](https://example.com)\n\n[^one]: First cites another[^two].\n\n[^two]: Second note.\n\n[^hidden]: Nested reference.\n\n[^unused]: Unreferenced note.\n",
        );
        let original = doc.clone();
        let ids = page_note_ids(&doc);
        let prepared = for_pdf_paged_with_backlinks(&doc, true);
        let ordinary = for_pdf_paged(&doc);
        for label in ["one", "two", "hidden", "unused"] {
            let id = ids
                .get(label)
                .ok_or_else(|| format!("missing prepared note ID for {label}"))?;
            let blocks = prepared
                .blocks
                .iter()
                .find_map(|block| match block {
                    Block::FootnoteDefinition { id: actual, blocks } if actual == id => {
                        Some(blocks)
                    }
                    _ => None,
                })
                .ok_or_else(|| format!("missing prepared note {label} ({id})"))?;
            let Some(Block::Paragraph(inlines)) = blocks.first() else {
                return Err(format!(
                    "note {label} ({id}) must start with a numbered paragraph: {blocks:?}"
                ));
            };
            if matches!(label, "one" | "two") {
                let Some(Inline::Link { dest, .. }) = inlines.first() else {
                    return Err(format!(
                        "note {label} ({id}) must start with a linked return label: {inlines:?}"
                    ));
                };
                let number = id
                    .strip_prefix(PAGE_NOTE_PREFIX)
                    .ok_or_else(|| format!("invalid prepared note ID for {label}: {id}"))?;
                assert_eq!(dest, &format!("#{PAGE_NOTE_REFERENCE_PREFIX}{number}"));
            } else {
                assert!(
                    matches!(inlines.first(), Some(Inline::Text(_))),
                    "{label} has no rendered citation"
                );
            }
            let ordinary_blocks = ordinary
                .blocks
                .iter()
                .find_map(|block| match block {
                    Block::FootnoteDefinition { id: actual, blocks } if actual == id => {
                        Some(blocks)
                    }
                    _ => None,
                })
                .ok_or_else(|| format!("missing ordinary note {label} ({id})"))?;
            let Some(Block::Paragraph(ordinary_inlines)) = ordinary_blocks.first() else {
                return Err(format!(
                    "ordinary note {label} ({id}) must start with a numbered paragraph: {ordinary_blocks:?}"
                ));
            };
            assert_eq!(
                crate::html::inlines_to_plain(inlines),
                crate::html::inlines_to_plain(ordinary_inlines)
            );
            assert_eq!(&blocks[1..], &ordinary_blocks[1..]);
        }
        assert_eq!(doc, original);
        assert!(matches!(
            for_pdf_paged_with_backlinks(&prepared, true).0,
            Cow::Borrowed(_)
        ));
        Ok(())
    }

    #[test]
    fn undefined_references_remain_literal_instead_of_zero() {
        let doc = document(vec![Block::Paragraph(vec![reference("missing")])]);
        assert_eq!(
            for_pdf(&doc).as_ref(),
            &document(vec![paragraph("[^missing]")])
        );
    }

    #[test]
    fn nested_definitions_are_moved_once_and_continuation_paragraphs_keep_their_text() {
        let doc = document(vec![
            Block::Paragraph(vec![reference("outer")]),
            definition(
                "outer",
                vec![
                    paragraph("First"),
                    paragraph("Continuation"),
                    definition("inner", vec![paragraph("Nested definition")]),
                    Block::Paragraph(vec![reference("inner")]),
                ],
            ),
        ]);
        let result = for_pdf(&doc);
        assert_eq!(result.blocks.len(), 6);
        assert_eq!(result.blocks[3], paragraph("Continuation"));
        assert_eq!(result.blocks[4], Block::Paragraph(vec![text("²")]));
        assert_eq!(
            result.blocks[5],
            Block::Paragraph(vec![text("² "), text("Nested definition")])
        );
        assert!(
            matches!(for_pdf(result.as_ref()), Cow::Borrowed(_)),
            "preparation is idempotent"
        );
    }

    fn heading(value: &str) -> Block {
        Block::Heading {
            level: 2,
            inlines: vec![text(value)],
        }
    }

    #[test]
    fn heading_provenance_follows_note_queue_not_equal_title_matching() {
        let doc = document(vec![
            definition("a", vec![heading("Same")]),
            heading("Same"),
            definition("b", vec![heading("Same")]),
            Block::Paragraph(vec![reference("b"), reference("a")]),
        ]);
        assert_eq!(
            heading_origins_for_pdf(&doc),
            vec![Some(1), None, Some(2), Some(0)]
        );
        assert_eq!(
            heading_origins_for_pdf_paged(&doc),
            vec![Some(1), Some(2), Some(0)]
        );
        let paged = for_pdf_paged(&doc);
        let mut paged_count = 0;
        visit_headings(&paged.blocks, true, &mut |_| paged_count += 1);
        assert_eq!(paged_count, heading_origins_for_pdf_paged(&doc).len());
        let prepared = for_pdf(&doc);
        let mut count = 0;
        visit_headings(&prepared.blocks, true, &mut |_| count += 1);
        assert_eq!(count, heading_origins_for_pdf(&doc).len());
    }

    #[test]
    fn heading_provenance_omits_duplicate_definition_bodies() {
        let doc = document(vec![
            definition("a", vec![heading("First")]),
            definition("a", vec![heading("Discarded")]),
            Block::BlockQuote(vec![heading("Body")]),
        ]);
        assert_eq!(heading_origins_for_pdf(&doc), vec![Some(2), None, Some(0)]);
        assert_eq!(heading_origins_for_pdf_paged(&doc), vec![Some(2), Some(0)]);
    }

    #[test]
    fn heading_provenance_preserves_nested_note_free_source_order() {
        let doc = document(vec![heading("A"), Block::BlockQuote(vec![heading("B")])]);
        assert_eq!(heading_origins_for_pdf(&doc), vec![Some(0), Some(1)]);
        assert!(matches!(for_pdf(&doc), Cow::Borrowed(_)));
    }
}
