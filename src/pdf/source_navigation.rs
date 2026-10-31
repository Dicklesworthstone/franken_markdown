//! Actual destinations for immutable source headings, including note bodies.
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
}

pub(crate) fn render(
    doc: &Document,
    options: &PdfOptions,
    settings: PdfASettings,
    emit: PdfEmitOptions,
) -> Result<(Vec<u8>, SourceNavigation)> {
    let rendered = super::render_inner(doc, options, settings, false, emit, true)?;
    Ok((rendered.bytes, rendered.navigation))
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
    }
    navigation
}
