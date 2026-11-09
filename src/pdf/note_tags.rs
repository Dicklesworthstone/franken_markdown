//! Source-owned PDF 1.7 footnote structure, independent of page placement.

use super::{Document, Inline};
pub(super) use crate::footnotes::paged_semantics::Mark;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Context {
    pub inlines: Arc<BTreeMap<usize, Mark>>,
    pub bodies: BTreeMap<usize, u32>,
    pub source_owned: bool,
}

impl Context {
    pub fn bind(input: &impl crate::footnotes::PdfDocumentInput) -> Self {
        let Some(semantics) = input.pdf_note_semantics() else {
            return Self::default();
        };
        let document: &Document = input.document();
        Self {
            inlines: Arc::new(semantics.bind_inlines(document)),
            bodies: semantics.bind_bodies(document),
            source_owned: true,
        }
    }
}

pub(super) fn note_id(number: u32) -> String {
    format!("fmd-note-{number}")
}

pub(super) fn inline_mark(inlines: &BTreeMap<usize, Mark>, inline: &Inline) -> Option<Mark> {
    inlines.get(&(std::ptr::from_ref(inline) as usize)).copied()
}

/// Alert layout temporarily owns a cloned leading paragraph. Rebind only
/// those cloned nodes, then discard the overrides before that storage dies.
pub(super) fn cloned_inline_marks(
    source: &[Inline],
    cloned: &[Inline],
    links: &super::LinkIntern,
) -> BTreeMap<usize, Mark> {
    fn visit(
        source: &[Inline],
        cloned: &[Inline],
        links: &super::LinkIntern,
        marks: &mut BTreeMap<usize, Mark>,
    ) {
        for (source, cloned) in source.iter().zip(cloned) {
            if let Some(mark) = links.note_mark(source) {
                marks.insert(std::ptr::from_ref(cloned) as usize, mark);
            }
            match (source, cloned) {
                (Inline::Emphasis(source), Inline::Emphasis(cloned))
                | (Inline::Strong(source), Inline::Strong(cloned))
                | (Inline::Strikethrough(source), Inline::Strikethrough(cloned))
                | (
                    Inline::Link {
                        content: source, ..
                    },
                    Inline::Link {
                        content: cloned, ..
                    },
                ) => visit(source, cloned, links, marks),
                _ => {}
            }
        }
    }
    let mut marks = BTreeMap::new();
    visit(source, cloned, links, &mut marks);
    marks
}

pub(super) fn table_has_marks(table: &super::Table, marks: &BTreeMap<usize, Mark>) -> bool {
    fn marked(items: &[Inline], marks: &BTreeMap<usize, Mark>) -> bool {
        items.iter().any(|inline| {
            inline_mark(marks, inline).is_some()
                || match inline {
                    Inline::Emphasis(inner)
                    | Inline::Strong(inner)
                    | Inline::Strikethrough(inner)
                    | Inline::Link { content: inner, .. } => marked(inner, marks),
                    _ => false,
                }
        })
    }
    !marks.is_empty()
        && table
            .head
            .iter()
            .chain(table.rows.iter().flatten())
            .any(|cell| marked(cell, marks))
}

/// Sparse byte ranges in the existing text. `None` is legacy layout with no
/// source provenance; an empty boxed slice is known ordinary source content
/// and allocates nothing. Neither state changes shaping or line breaking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Spans(Option<Box<[Span]>>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
    mark: Mark,
}

impl Spans {
    pub fn tokens(tokens: &[super::LineTok]) -> Self {
        let mut result = Self::default();
        let mut offset = 0;
        for token in tokens {
            result.append(&token.tok.note_spans, offset);
            offset += super::token_visible_text(&token.tok).len();
        }
        result
    }
    pub fn whole(text: &str, mark: Mark) -> Self {
        if text.is_empty() {
            Self::from_vec(Vec::new())
        } else {
            Self(Some(
                vec![Span {
                    start: 0,
                    end: text.len(),
                    mark,
                }]
                .into_boxed_slice(),
            ))
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.as_deref().is_none_or(<[Span]>::is_empty)
    }

    pub fn is_known(&self) -> bool {
        self.0.is_some()
    }

    pub fn make_known(&mut self) {
        if self.0.is_none() {
            self.0 = Some(Box::default());
        }
    }

    pub fn linked_reference(&self) -> Option<u32> {
        self.0.as_deref()?.iter().find_map(|span| match span.mark {
            Mark::Reference {
                number,
                linked: true,
                ..
            } => Some(number),
            _ => None,
        })
    }

    pub fn is_navigation(&self) -> bool {
        self.0.as_deref().is_some_and(|spans| {
            spans.iter().any(|span| {
                matches!(
                    span.mark,
                    Mark::Reference { linked: true, .. } | Mark::Label(_)
                )
            })
        })
    }

    fn uniform(&self, len: usize) -> Option<Mark> {
        match self.0.as_deref()? {
            [span] if span.start == 0 && span.end == len => Some(span.mark),
            _ => None,
        }
    }

    pub fn at(&self, byte: usize) -> Option<Mark> {
        self.0
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|span| span.start <= byte && byte < span.end)
            .map(|span| span.mark)
    }

    pub fn slice(&self, start: usize, end: usize) -> Self {
        let Some(source) = self.0.as_deref() else {
            return Self::default();
        };
        let spans = source
            .iter()
            .filter_map(|span| {
                let low = span.start.max(start);
                let high = span.end.min(end);
                (low < high).then_some(Span {
                    start: low.saturating_sub(start),
                    end: high.saturating_sub(start),
                    mark: span.mark,
                })
            })
            .collect::<Vec<_>>();
        Self::from_vec(spans)
    }

    pub fn append(&mut self, other: &Self, offset: usize) {
        let Some(other) = other.0.as_deref() else {
            return;
        };
        let mut spans = self.0.take().map_or_else(Vec::new, Vec::from);
        for span in other {
            let start = span.start.saturating_add(offset);
            let end = span.end.saturating_add(offset);
            if let Some(last) = spans.last_mut()
                && last.end == start
                && last.mark == span.mark
            {
                last.end = end;
            } else {
                spans.push(Span {
                    start,
                    end,
                    mark: span.mark,
                });
            }
        }
        *self = Self::from_vec(spans);
    }

    /// Scalar fallback can replace a superscript with its base digit. The
    /// existing fallback keeps scalar order/cardinality; project source roles
    /// onto its new UTF-8 byte offsets rather than copying stale byte ranges.
    pub fn fallback_runs(&self, source: &str, runs: &[(u8, String)]) -> Vec<Self> {
        if self.is_empty() {
            return (0..runs.len()).map(|_| self.clone()).collect();
        }
        let mut source = source.char_indices();
        runs.iter()
            .map(|(_, text)| {
                let mut spans = Vec::<Span>::new();
                for (start, ch) in text.char_indices() {
                    let mark = source.next().and_then(|(index, _)| self.at(index));
                    if let Some(mark) = mark {
                        let end = start + ch.len_utf8();
                        if let Some(last) = spans.last_mut()
                            && last.end == start
                            && last.mark == mark
                        {
                            last.end = end;
                        } else {
                            spans.push(Span { start, end, mark });
                        }
                    }
                }
                Self::from_vec(spans)
            })
            .collect()
    }

    fn from_vec(spans: Vec<Span>) -> Self {
        Self(Some(spans.into_boxed_slice()))
    }
}

/// Writer state is borrowed for one existing shaped segment. No layout or
/// token boundary is changed merely to insert a semantic marked-content span.
pub(super) struct Draw<'a, 'f> {
    pub body: &'a mut String,
    pub annots: &'a mut Vec<super::LinkAnnotation>,
    pub current_fill: &'a mut super::Fill,
    pub marks: &'a mut Vec<super::StructMark>,
    pub next_mcid: &'a mut usize,
    pub subsets: &'a [super::EmbeddedFace<'f>],
    pub subset_lookup: &'a super::EmbeddedFaceLookup,
    pub faces: &'a super::Faces,
    pub shaped_cache: &'a super::ShapedRunCache,
    pub palette: &'a super::Palette,
}

fn path_for(base: &[super::SElem], mark: Option<Mark>) -> super::SmallPath {
    use super::{SElem, SKey, SmallPath};
    let link = base.last().copied().filter(|element| element.tag == "Link");
    let mut path = match mark {
        Some(Mark::Label(number)) => {
            // The generated number is a direct Note child; the following
            // paragraph/list/table retains its own ordinary block role.
            let end = base
                .iter()
                .position(|element| element.key == SKey::Note(number))
                .map_or(base.len(), |index| index + 1);
            let mut path = SmallPath::from_slice(&base[..end]);
            path.push(SElem {
                key: SKey::NoteLabel(number),
                tag: "Lbl",
            });
            path
        }
        Some(Mark::Reference {
            occurrence,
            linked: true,
            ..
        }) => {
            let end = base.len() - usize::from(link.is_some());
            let mut path = SmallPath::from_slice(&base[..end]);
            path.push(SElem {
                key: SKey::NoteReference(occurrence),
                tag: "Reference",
            });
            path
        }
        Some(Mark::Reference {
            occurrence,
            linked: false,
            ..
        }) => {
            let mut path = SmallPath::from_slice(base);
            path.push(SElem {
                key: SKey::NoteReference(occurrence),
                tag: "Reference",
            });
            return path;
        }
        None => return SmallPath::from_slice(base),
    };
    if let Some(link) = link {
        path.push(link);
    }
    path
}

fn begin(
    draw: &mut Draw<'_, '_>,
    base: &[super::SElem],
    mark: Option<Mark>,
    repeated: bool,
) -> usize {
    let path = path_for(base, mark);
    let tag = path.last().map_or("P", |element| element.tag);
    let mcid = *draw.next_mcid;
    super::append_table_cell_content_begin(draw.body, tag, mcid, repeated);
    draw.marks.push(super::StructMark {
        mcid,
        path,
        alt: None,
        bbox: None,
    });
    *draw.next_mcid += 1;
    mcid
}

pub(super) fn draw(
    mut draw: Draw<'_, '_>,
    seg: &super::Seg,
    size: f32,
    y: f32,
    base: &[super::SElem],
    repeated: bool,
) {
    if let Some(mark) = seg.note_spans.uniform(seg.text.len()) {
        let owner = begin(&mut draw, base, Some(mark), repeated);
        super::draw_seg(
            draw.body,
            draw.annots,
            draw.current_fill,
            owner,
            seg,
            size,
            y,
            draw.subsets,
            draw.subset_lookup,
            draw.faces,
            draw.shaped_cache,
            draw.palette,
        );
        draw.body.push_str("EMC\n");
        return;
    }
    let Some(slot) = super::pdf_font_slot_index(seg.slot) else {
        return;
    };
    let Some(embedded) = draw.subset_lookup.get(draw.subsets, seg.slot) else {
        return;
    };
    let face = draw.faces.face(seg.slot);
    let fallback;
    let run = if let Some(run) =
        super::shaped_run_lookup(&draw.shaped_cache[slot], &seg.text, seg.text_hash)
    {
        run
    } else {
        fallback = super::shape_run(face, &seg.text);
        &fallback
    };
    let ranges = glyph_sources(face, &seg.text, run);
    debug_assert_eq!(ranges.len(), run.glyphs.len());
    let mut groups = Vec::<Group>::new();
    for (index, source) in ranges.into_iter().enumerate() {
        let mark = seg.note_spans.at(source.start);
        if let Some(last) = groups.last_mut()
            && last.mark == mark
        {
            last.glyphs.end = index + 1;
            last.source.end = last.source.end.max(source.end);
        } else {
            groups.push(Group {
                glyphs: index..index + 1,
                source,
                mark,
            });
        }
    }
    if seg.fill != *draw.current_fill {
        super::append_rgb_fill_operator(draw.body, super::fill_rgb(seg.fill, draw.palette));
        *draw.current_fill = seg.fill;
    }
    draw.body.push_str("BT /F");
    super::append_decimal_u64_string(draw.body, u64::from(seg.slot));
    draw.body.push(' ');
    super::append_pdf_fixed2(draw.body, size);
    draw.body.push_str(" Tf ");
    // Positioned runs already carry expansion in their absolute matrices.
    if run.positioned.is_none() {
        if seg.expansion_permille != 0 {
            super::append_pdf_fixed2(draw.body, 100.0 + f32::from(seg.expansion_permille) / 10.0);
            draw.body.push_str(" Tz ");
        }
        draw.body.push_str("1 0 0 1 ");
        super::append_pdf_fixed2(draw.body, seg.x);
        draw.body.push(' ');
        super::append_pdf_fixed2(draw.body, y);
        draw.body.push_str(" Tm ");
    }
    let expansion = 1.0 + f32::from(seg.expansion_permille) / 1000.0;
    let font_size = super::font_size_of(size);
    let mut positioned_pen = super::LayoutUnit::ZERO;
    let mut legacy_pen = 0.0f32;
    let mut owners = Vec::with_capacity(groups.len());
    for group in &groups {
        let owner = begin(&mut draw, base, group.mark, repeated);
        // Per-cluster replacement text preserves source spelling even when a
        // composed/ligated run crosses a semantic boundary.
        super::append_actual_text_start(draw.body, &seg.text[group.source.clone()]);
        let x0 = seg.x
            + if run.positioned.is_some() {
                positioned_pen.to_points_f32()
            } else {
                legacy_pen
            } * expansion;
        if let Some(positioned) = &run.positioned {
            super::text_positioning::append_glyphs_range(
                draw.body,
                positioned,
                &embedded.map_lookup,
                super::SvgTextMatrix {
                    a: expansion,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    x: seg.x,
                    y,
                    size,
                },
                0,
                0,
                group.glyphs.clone(),
                positioned_pen,
            );
            for glyph in &positioned.glyphs[group.glyphs.clone()] {
                positioned_pen += positioned.measure(glyph.advance, font_size);
            }
        } else {
            draw.body.push_str("[<");
            let upm = i32::from(face.font.units_per_em.max(1));
            for index in group.glyphs.clone() {
                let glyph = run.glyphs[index];
                super::append_hex_u16(
                    draw.body,
                    embedded
                        .map_lookup
                        .get(usize::from(glyph))
                        .copied()
                        .unwrap_or(0),
                );
                let kern = run.glyphs.get(index + 1).map_or(0, |&next| {
                    i32::from(face.kern.pair(glyph, next)) * 1000 / upm
                });
                legacy_pen += (super::glyph_advance_1000(&face.font, glyph) as f32 + kern as f32)
                    * size
                    / 1000.0;
                // Keep the boundary pair adjustment inside this TJ. The next
                // piece continues the same text matrix without reshaping.
                if kern != 0 {
                    draw.body.push('>');
                    super::append_i32_string(draw.body, -kern);
                    draw.body.push('<');
                }
            }
            draw.body.push_str(">] TJ ");
        }
        draw.body.push_str("EMC\nEMC\n");
        let x1 = seg.x
            + if run.positioned.is_some() {
                positioned_pen.to_points_f32()
            } else {
                legacy_pen
            } * expansion;
        owners.push((owner, x0, x1));
    }
    if run.positioned.is_none() && seg.expansion_permille != 0 {
        draw.body.push_str("100 Tz ");
    }
    draw.body.push_str("ET\n");
    // Decorations retain their complete original geometry. Annotation pieces
    // acquire the same logical owner as their marked text; linked labels and
    // each repeated citation therefore have valid Link/OBJR back-references.
    let mut annotations = Vec::new();
    draw.body.push_str("/Artifact BMC\n");
    super::draw_seg_decorations(
        draw.body,
        &mut annotations,
        draw.current_fill,
        0,
        seg,
        size,
        y,
        draw.palette,
        run.positioned
            .as_ref()
            .map_or((0.0, 0.0), |run| run.ink(font_size)),
    );
    draw.body.push_str("EMC\n");
    for annotation in annotations {
        let last = owners.len().saturating_sub(1);
        for (index, &(owner, x0, x1)) in owners.iter().enumerate() {
            let mut piece = annotation.clone();
            piece.rect.x0 = if index == 0 { annotation.rect.x0 } else { x0 };
            piece.rect.x1 = if index == last {
                annotation.rect.x1
            } else {
                x1
            };
            piece.owner_mcid = Some(owner);
            if piece.rect.x1 > piece.rect.x0 {
                draw.annots.push(piece);
            }
        }
    }
}

struct Group {
    glyphs: std::ops::Range<usize>,
    source: std::ops::Range<usize>,
    mark: Option<Mark>,
}

/// Recover source clusters for the already selected glyphs. The ordinary
/// shaper's substitution counts and the strict shaper's retained ranges use
/// the same composed string; neither path is shaped in smaller role runs.
fn glyph_sources(
    face: &super::Face,
    text: &str,
    run: &super::ShapedRun,
) -> Vec<std::ops::Range<usize>> {
    let mut units = Vec::new();
    let mut glyph_text = String::with_capacity(text.len());
    let mut original = 0;
    for cluster in super::text_composition::source_clusters(text) {
        let composed = super::text_composition::glyph_text(cluster, &face.font);
        let changed = matches!(&composed, std::borrow::Cow::Owned(_));
        for (offset, ch) in composed.char_indices() {
            let source = if changed {
                original..original + cluster.len()
            } else {
                original + offset..original + offset + ch.len_utf8()
            };
            let start = glyph_text.len();
            glyph_text.push(ch);
            units.push((start..glyph_text.len(), source));
        }
        original += cluster.len();
    }
    if let Some(positioned) = &run.positioned {
        return positioned
            .glyphs
            .iter()
            .map(|glyph| {
                let first = units.partition_point(|(range, _)| range.end <= glyph.source_start);
                let last = units.partition_point(|(range, _)| range.start < glyph.source_end);
                units[first].1.start..units[last - 1].1.end
            })
            .collect();
    }
    let raw = glyph_text
        .chars()
        .map(|ch| face.font.glyph_index(ch))
        .collect::<Vec<_>>();
    let shaped = face.lig.substitute_with_spans(&raw);
    let mut at = 0;
    shaped
        .into_iter()
        .map(|(_, count)| {
            let source = units[at].1.start..units[at + count - 1].1.end;
            at += count;
            source
        })
        .collect()
}
