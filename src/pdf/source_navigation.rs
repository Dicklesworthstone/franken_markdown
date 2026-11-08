//! Actual destinations for immutable source headings and prepared page notes.
//!
//! Source identity survives line wrapping, repeated layout passes and note
//! placement. It is used for lookup only: pointer values never enter output,
//! ordering, or the crate-private result. Generated headings have no source match.

use std::collections::BTreeMap;

use crate::{Block, Document, PdfASettings, PdfOptions, Result};

use super::{OutlineEntry, PdfEmitOptions};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Destination {
    pub page_index: usize,
    pub y: f32,
}

impl Destination {
    /// Use the writer's numeric formatting, with a page object read from the
    /// completed PDF's page tree rather than an assumed object-number formula.
    pub(crate) fn append_pdf(self, out: &mut Vec<u8>, page_object: usize) {
        out.push(b'[');
        super::append_pdf_object_ref_bytes(out, page_object);
        out.extend_from_slice(b" /XYZ null ");
        super::append_pdf_num_bytes(out, self.y);
        out.extend_from_slice(b" null]");
    }
}

#[derive(Default)]
pub(crate) struct SourceNavigation {
    /// Depth-first heading ordinals in the supplied prepared document, including
    /// note bodies. Empty/unpainted headings have no emitted destination.
    pub headings: BTreeMap<usize, Destination>,
    /// Prepared note IDs emitted at the page foot or in the endnote fallback.
    /// These entries are not bookmarks and must not match a heading with the
    /// same spelling as the renderer's internal note namespace.
    pub notes: BTreeMap<String, Destination>,
    /// First linked citations, keyed by the same prepared note IDs as `notes`.
    /// The original body takes precedence over any note-to-note citation even
    /// when page-bottom placement paints the latter on an earlier page.
    pub note_references: BTreeMap<String, Destination>,
}

/// Keep the same renderer result for ordinary, reported and profiled source
/// navigation. Binding annotations changes neither pagination nor page paint.
pub(crate) struct NavigationRender {
    pub bytes: Vec<u8>,
    pub navigation: SourceNavigation,
    pub stages: Vec<super::PdfStageSummary>,
    pub pagination: super::PdfPaginationReport,
    pub warnings: Vec<super::RenderWarning>,
}

pub(crate) fn render_complete(
    doc: &Document,
    options: &PdfOptions,
    settings: PdfASettings,
    emit: PdfEmitOptions,
    profiled: bool,
) -> Result<NavigationRender> {
    render_complete_with_diagnostics(doc, options, settings, emit, profiled, true, false)
}

/// Book emission can retain the actual writer provenance and fidelity findings
/// while avoiding source-destination collection when no links require binding.
/// Diagnostics reuse loaded faces and measured lines; they never rerender.
pub(crate) fn render_complete_with_diagnostics(
    doc: &Document,
    options: &PdfOptions,
    settings: PdfASettings,
    emit: PdfEmitOptions,
    profiled: bool,
    collect_navigation: bool,
    collect_warnings: bool,
) -> Result<NavigationRender> {
    let rendered = super::render_inner(
        doc,
        options,
        settings,
        profiled,
        emit,
        collect_navigation,
        collect_warnings,
    )?;
    Ok(NavigationRender {
        bytes: rendered.bytes,
        navigation: rendered.navigation,
        stages: rendered.stages,
        pagination: rendered.pagination,
        warnings: rendered.warnings,
    })
}

pub(super) fn collect(doc: &Document, outlines: &[OutlineEntry]) -> SourceNavigation {
    fn walk(blocks: &[Block], headings: &mut BTreeMap<usize, usize>) {
        for block in blocks {
            match block {
                Block::Heading { .. } => {
                    let ordinal = headings.len();
                    headings.insert(std::ptr::from_ref(block) as usize, ordinal);
                }
                Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                    walk(inner, headings);
                }
                Block::List(list) => {
                    for item in &list.items {
                        walk(&item.blocks, headings);
                    }
                }
                _ => {}
            }
        }
    }
    let mut identities = BTreeMap::new();
    walk(&doc.blocks, &mut identities);
    let mut navigation = SourceNavigation::default();
    for outline in outlines {
        let destination = Destination {
            page_index: outline.page_index,
            y: outline.y,
        };
        if let Some(ordinal) = outline.source_identity.and_then(|id| identities.get(&id)) {
            navigation.headings.insert(*ordinal, destination);
        }
        if outline.level == 0
            && outline.source_identity.is_none()
            && outline.id.starts_with(crate::footnotes::PAGE_NOTE_PREFIX)
        {
            navigation.notes.insert(outline.id.clone(), destination);
        }
        if outline.level == 0
            && outline.source_identity.is_none()
            && let Some(number) = outline
                .id
                .strip_prefix(crate::footnotes::PAGE_NOTE_REFERENCE_PREFIX)
        {
            navigation.note_references.insert(
                format!("{}{number}", crate::footnotes::PAGE_NOTE_PREFIX),
                destination,
            );
        }
    }
    navigation
}
