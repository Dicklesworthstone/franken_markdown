//! Physical quote frames shared by prose, lists, tables, code and figures.
//!
//! Ordinary quotes retain their 18pt gutter. When nesting exhausts that space,
//! retain four body ems where the containing width permits, along with enough
//! space for indivisible glyphs and native block gutters. Deeper quotes can
//! share a border lane. The logical quote IDs
//! remain distinct even when the decorative bars occupy the same coordinates.

use super::*;

const GUTTER: f32 = 18.0;
const TEXT_TO_BAR: f32 = 12.0;
const READABLE_EMS: f32 = 4.0;
const EDGE_RESERVE: f32 = 0.025;

#[derive(Default)]
pub(super) struct State {
    depth: usize,
    framed: bool,
    body_left: f32,
    blocks: HashMap<usize, f32>,
    lists: HashMap<usize, f32>,
}

fn identity<T>(value: &T) -> usize {
    std::ptr::from_ref(value) as usize
}

/// A glyph-sized column technically fits but turns ordinary words into a
/// vertical run of letters. Reserve a short readable measure before compressing
/// decorative nesting; a larger glyph or native block inset still wins.
fn framed_minimum(minimum: f32, cx: &LayoutCx<'_>) -> f32 {
    minimum
        .max(READABLE_EMS * cx.type_scale.body)
        .min(cx.page.content_w)
}

/// Descendants may reclaim their own indentation, but cannot reclaim space
/// through their containing quote's bar. Preserve each block's native inset
/// and at least one complete source cluster at its requested type size.
pub(super) fn block_indent(block: &Block, indent: f32, cx: &LayoutCx<'_>) -> f32 {
    if cx.quote_geometry.depth == 0 || !cx.quote_geometry.framed {
        return indent;
    }
    let minimum = cx
        .quote_geometry
        .blocks
        .get(&identity(block))
        .copied()
        .unwrap_or(cx.type_scale.body);
    indent
        .max(0.0)
        .min((cx.page.content_w - framed_minimum(minimum, cx)).max(0.0))
}

/// A list's hanging gutter must also leave space for its real children.
/// Outside quotes, retain the existing list geometry byte for byte.
pub(super) fn list_geometry(
    list: &List,
    indent: f32,
    marker_col: f32,
    size: f32,
    cx: &LayoutCx<'_>,
) -> list_geometry::Geometry {
    let width = cx.page.content_w;
    let ordinary = list_geometry::geometry(width, indent, marker_col, size);
    if cx.quote_geometry.depth == 0 || !cx.quote_geometry.framed {
        return ordinary;
    }
    let minimum = framed_minimum(
        cx.quote_geometry
            .lists
            .get(&identity(list))
            .copied()
            .unwrap_or(size)
            .max(size),
        cx,
    );
    if ordinary.hanging && ordinary.content_indent + minimum <= width {
        return ordinary;
    }
    let indent = indent
        .max(0.0)
        .min((width - marker_col.max(minimum)).max(0.0));
    list_geometry::Geometry {
        indent,
        content_indent: indent,
        hanging: false,
    }
}

pub(super) fn layout(
    block: &Block,
    inner: &[Block],
    indent: f32,
    out: &mut Vec<Line>,
    cx: &mut LayoutCx<'_>,
) {
    if cx.quote_geometry.depth == 0 {
        // One source-tree pass per outer quote. Nested quotes reuse this plan;
        // a later footnote layout builds a fresh plan at its own type sizes.
        let mut state = State {
            body_left: cx.page.left,
            ..State::default()
        };
        let width = plan_block(block, cx, &mut state, !cx.list_stack.is_empty());
        // Keep the historical global-indent arithmetic when every native
        // gutter fits. Besides avoiding needless layout changes, this retains
        // exact bytes at fractional margins where regrouping f32 additions
        // can change an emitted coordinate by a hundredth of a point.
        state.framed = width.natural > cx.page.content_w - indent;
        cx.quote_geometry = state;
    }
    let page = cx.page;
    let (child_indent, bar_x) = if cx.quote_geometry.framed {
        let minimum = framed_minimum(cx.quote_geometry.blocks[&identity(block)], cx);
        let content_indent = (indent + GUTTER)
            .max(0.0)
            .min((page.content_w - minimum).max(0.0));
        let content_left = page.left + content_indent;
        let bar_x = (content_left - TEXT_TO_BAR).max(cx.quote_geometry.body_left + QUOTE_BG_PAD_X);
        cx.page.left = content_left;
        cx.page.content_w = page.content_w - content_indent;
        (0.0, bar_x)
    } else {
        (indent + GUTTER, page.left + indent + 6.0)
    };
    cx.quote_geometry.depth += 1;

    // Allocate before descending: source identity and nesting order are
    // independent of whether several borders share one physical lane.
    let quote_id = cx.alloc_flow() as usize;
    let start = out.len();
    gap(out, 4.0);
    if let Some(parts) = crate::ast::alert_parts(inner) {
        let group = cx.alloc_flow();
        let mut title_toks = Vec::new();
        push_text_tokens(parts.label, F_BOLD, false, None, &mut title_toks);
        apply_symbol_fallback(&mut title_toks, cx.faces);
        layout_inlines(
            title_toks,
            child_indent,
            cx.type_scale.body,
            3.0,
            out,
            cx,
            FlowSpec {
                group,
                kind: FlowKind::Heading,
            },
        );
        if !parts.leading.is_empty() {
            layout_blocks(
                &[Block::Paragraph(parts.leading.to_vec())],
                child_indent,
                out,
                cx,
            );
        }
        layout_blocks(parts.rest, child_indent, out, cx);
    } else {
        layout_blocks(inner, child_indent, out, cx);
    }
    for line in &mut out[start..] {
        line.quote_bars.push((quote_id, bar_x));
    }
    gap(out, 3.0);
    cx.page = page;
    cx.quote_geometry.depth -= 1;
    if cx.quote_geometry.depth == 0 {
        cx.quote_geometry.blocks.clear();
        cx.quote_geometry.lists.clear();
    }
}

#[derive(Clone, Copy)]
struct Width {
    minimum: f32,
    natural: f32,
}

impl Width {
    fn new(minimum: f32) -> Self {
        let minimum = minimum.max(1.0);
        Self {
            minimum,
            natural: minimum,
        }
    }

    fn include(&mut self, other: Self) {
        self.minimum = self.minimum.max(other.minimum);
        self.natural = self.natural.max(other.natural);
    }
}

fn plan_blocks(blocks: &[Block], cx: &LayoutCx<'_>, state: &mut State, in_list: bool) -> Width {
    blocks.iter().fold(Width::new(1.0), |mut width, block| {
        width.include(plan_block(block, cx, state, in_list));
        width
    })
}

fn plan_block(block: &Block, cx: &LayoutCx<'_>, state: &mut State, in_list: bool) -> Width {
    let mut width = match block {
        Block::Paragraph(inlines) => {
            let minimum = inline_minimum(inlines, cx.type_scale.body, false, cx);
            Width::new(if toc_marker_depth(inlines).is_some() {
                minimum.max(MIN_CONTENT_DIM.max(4.0 * cx.type_scale.body))
            } else {
                minimum
            })
        }
        Block::Heading { level, inlines } => {
            let minimum = inline_minimum(
                inlines,
                cx.type_scale.h[(*level as usize).saturating_sub(1).min(5)],
                true,
                cx,
            );
            Width::new(if cx.automatic_toc {
                minimum.max(MIN_CONTENT_DIM.max(4.0 * cx.type_scale.body))
            } else {
                minimum
            })
        }
        Block::CodeBlock { code, .. } => {
            let size = cx.type_scale.code;
            let numbers = if cx.opts.code_line_numbers {
                code_line_number_column_width(
                    code.lines().count().max(1).to_string().len(),
                    size,
                    cx.faces,
                    &cx.width_cache,
                )
            } else {
                0.0
            };
            Width::new(
                CODE_PAD_X + numbers + fenced_code::widest_cluster(code, size, cx.faces) + 0.05,
            )
        }
        Block::BlockQuote(inner) => {
            let mut width = plan_blocks(inner, cx, state, in_list);
            if let Some(parts) = crate::ast::alert_parts(inner) {
                width.include(Width::new(text_minimum(
                    parts.label,
                    cx.type_scale.body,
                    F_BOLD,
                    cx,
                )));
            }
            width.natural += GUTTER;
            width
        }
        Block::List(list) => {
            let size = cx.type_scale.body;
            let mut width = Width::new(size);
            for item in &list.items {
                width.include(plan_blocks(&item.blocks, cx, state, true));
                if let Some(checked) = item.task {
                    width.include(Width::new(
                        text_width_cached(
                            if checked { "[x]" } else { "[ ]" },
                            size,
                            F_BODY,
                            cx.faces,
                            &cx.width_cache,
                        ) + EDGE_RESERVE,
                    ));
                }
            }
            let (_, marker_col) = list_marker_layouts(list, cx.faces, size, &cx.width_cache);
            width.natural += marker_col + size;
            state.lists.insert(identity(list), width.minimum);
            width
        }
        Block::Table(table) => {
            let mut cell = TABLE_MIN_COL_WIDTH;
            for inlines in &table.head {
                cell = cell.max(inline_minimum(inlines, cx.type_scale.table, true, cx));
            }
            for inlines in table.rows.iter().flatten() {
                cell = cell.max(inline_minimum(inlines, cx.type_scale.table, false, cx));
            }
            let spacing = spacing::Policy::new(cx.opts);
            Width::new(spacing.table_gutter(cx.type_scale.table, cx.type_scale.table) + cell)
        }
        Block::DefinitionList(items) => Width::new(items.iter().fold(1.0_f32, |minimum, item| {
            let terms = item
                .terms
                .iter()
                .map(|inlines| inline_minimum(inlines, cx.type_scale.body, true, cx))
                .fold(1.0, f32::max);
            let definitions = item
                .definitions
                .iter()
                .map(|inlines| 14.0 + inline_minimum(inlines, cx.type_scale.body, false, cx))
                .fold(1.0, f32::max);
            minimum.max(terms).max(definitions)
        })),
        Block::HtmlBlock(text) => Width::new(text_minimum(text, 11.0, F_BODY, cx)),
        // Valid formulas fit their containing measure. Their source fallback
        // has a 12pt inset and uses the ordinary 11pt monospace face.
        Block::MathBlock(text) => Width::new(12.0 + text_minimum(text, 11.0, F_MONO, cx)),
        Block::ThematicBreak | Block::PageBreak | Block::FootnoteDefinition { .. } => {
            Width::new(1.0)
        }
    };
    if in_list {
        // Natural indentation must survive the same em clamps as real list
        // layout, including a list outside this quote's planned subtree. A
        // narrow glyph alone can fit while its heading is moved back through
        // the quote border. These admission reserves do not enlarge the
        // measured minimum used by the compressed frame.
        let reserve = match block {
            Block::Heading { level, .. } => {
                cx.type_scale.h[(*level as usize).saturating_sub(1).min(5)]
            }
            Block::DefinitionList(items)
                if items.iter().any(|item| !item.definitions.is_empty()) =>
            {
                14.0 + cx.type_scale.body
            }
            // Raw-source fallbacks call layout_inlines at their fixed 11pt
            // size; math also adds its own 12pt text inset.
            Block::HtmlBlock(_) => cx.type_scale.body.max(11.0),
            Block::MathBlock(_) => cx.type_scale.body.max(12.0 + 11.0),
            Block::FootnoteDefinition { .. } | Block::PageBreak => 0.0,
            _ => cx.type_scale.body,
        };
        width.natural = width.natural.max(reserve);
    }
    state.blocks.insert(identity(block), width.minimum);
    width
}

fn inline_minimum(inlines: &[Inline], size: f32, bold: bool, cx: &LayoutCx<'_>) -> f32 {
    let mut toks = Vec::new();
    // Use the same source/style/cluster interpretation as real layout without
    // changing the document's interned link order during measurement.
    tokenize(
        inlines,
        bold,
        false,
        false,
        None,
        &mut LinkIntern::default(),
        &mut toks,
    );
    token_minimum(toks, size, cx)
}

fn text_minimum(text: &str, size: f32, slot: u8, cx: &LayoutCx<'_>) -> f32 {
    let toks = vec![Tok {
        text: text.to_owned(),
        slot,
        space: false,
        hard_break: false,
        literal: false,
        list_label: false,
        link: None,
        strike: false,
    }];
    token_minimum(toks, size, cx)
}

fn token_minimum(mut toks: Vec<Tok>, size: f32, cx: &LayoutCx<'_>) -> f32 {
    apply_symbol_fallback(&mut toks, cx.faces);
    toks.iter().fold(1.0_f32, |minimum, tok| {
        if tok.space || tok.hard_break {
            minimum
        } else if is_inline_object(tok.slot) {
            // Figures and formulas fit their eventual containing measure;
            // keep at least one text em instead of collapsing it to a hairline.
            minimum.max(size)
        } else {
            text_composition::source_clusters(&tok.text)
                .map(|cluster| {
                    text_width_cached(cluster, size, tok.slot, cx.faces, &cx.width_cache)
                        + EDGE_RESERVE
                })
                .fold(minimum, f32::max)
        }
    })
}
