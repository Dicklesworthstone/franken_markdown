//! PDF renderer with embedded subset fonts.
//!
//! Produces a deterministic, **tiny** PDF that embeds document-specific *subsets*
//! of the bundled IBM Plex / Computer Modern faces (see [`crate::fonts`] +
//! [`crate::text::Font::subset`]). Text is laid out with the faces' real `hmtx`
//! metrics, then written as a composite `Type0` font with `Identity-H` encoding
//! (2-byte glyph ids) and a `CIDFontType2` descendant carrying the subset
//! `FontFile2`. Each face also gets a `ToUnicode` CMap so the text stays
//! selectable / copy-pasteable.
//!
//! Knuth-Plass optimal line breaking (via [`crate::layout::break_paragraph`],
//! with the original greedy wrapper kept as an emergency fallback) + automatic
//! pagination over headings, paragraphs, code blocks, lists, blockquotes, tables
//! (simple), and rules, with styled inline runs (bold / italic / code /
//! bold-italic in their own embedded faces). Inline links are colored +
//! underlined, blockquotes get a subtle gutter bar, and fenced code blocks /
//! inline code get rounded light-gray backgrounds. Focused GPOS kerning, GSUB
//! ligatures, and FlateDecode stream compression are applied today.
//!
//! Pure computation (no `std::fs`, no deps) so it stays WASM / `--no-default-features`
//! clean; the font bytes come from `include_bytes!`, not the filesystem.

use crate::PdfOptions;
use crate::ast::{Block, Document, Inline, List};
use crate::error::Result;
use crate::fonts::{self, FontStyle};
use crate::layout::{
    FontSize, LayoutUnit, ParagraphItem, Penalty, StyledText, TextBox, break_paragraph,
    default_interword_glue, measure_text_with_pairs,
};
use crate::text::{Font, Kerning, Ligatures};
use std::collections::{BTreeMap, BTreeSet};

const PAGE_W: f32 = 612.0; // US Letter, points
const PAGE_H: f32 = 792.0;
const MARGIN: f32 = 72.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;

// Inline-link styling: hyperlink blue (`rg` fill + `RG` underline stroke).
const LINK_COLOR: (f32, f32, f32) = (0.07, 0.20, 0.55);

// Fenced-code panel + inline-code chip backgrounds.
const CODE_PAD_X: f32 = 8.0; // text inset inside a fenced-code line
const PANEL_PAD_V: f32 = 5.0; // vertical breathing room above/below the code
const PANEL_RADIUS: f32 = 4.0;
const PANEL_GRAY: (f32, f32, f32) = (0.960, 0.960, 0.970);
const PANEL_ASCENT_FRAC: f32 = 0.78; // glyph top above baseline (mono)
const PANEL_DESCENT_FRAC: f32 = 0.30; // glyph bottom below baseline
const CHIP_PAD_X: f32 = 2.0;
const CHIP_RADIUS: f32 = 2.5;
const CHIP_GRAY: (f32, f32, f32) = (0.930, 0.930, 0.950);

// Font slots referenced in page Resources as /F1../F5.
const F_BODY: u8 = 1;
const F_BOLD: u8 = 2;
const F_ITALIC: u8 = 3;
const F_MONO: u8 = 4;
const F_BOLDITALIC: u8 = 5;
const SLOTS: [u8; 5] = [F_BODY, F_BOLD, F_ITALIC, F_MONO, F_BOLDITALIC];

/// A positioned run of single-face text within a laid-out line.
struct Seg {
    x: f32,
    slot: u8,
    text: String,
    /// True if this run is part of an inline link (colored + underlined).
    link: bool,
    /// Layout (non-kerned) advance sum, used to size the link underline.
    width: f32,
}

/// One laid-out line: a baseline-aligned row of styled segments, or a rule.
struct Line {
    size: f32,
    gap_after: f32,
    rule: bool,
    /// Left x of a horizontal rule (only meaningful when `rule`).
    rule_x: f32,
    /// For each blockquote enclosing this line: `(quote_id, bar_x)`. `quote_id`
    /// is the out-vec index of the quote's first line; `bar_x` is the stroke x.
    quote_bars: Vec<(usize, f32)>,
    /// Code-panel group: `0` = no background; equal nonzero ids on consecutive
    /// lines share ONE filled rounded rect (a single fenced code block).
    bg: u32,
    segs: Vec<Seg>,
}

/// The source faces resolved from the theme family + the registry.
struct Faces {
    body: Font,
    bold: Font,
    italic: Font,
    bolditalic: Font,
    mono: Font,
}

impl Faces {
    fn load(opts: &PdfOptions) -> Option<Self> {
        let fam = opts.theme.font;
        Some(Self {
            body: fonts::load_body(fam, FontStyle::Regular).ok()?,
            bold: fonts::load_body(fam, FontStyle::Bold).ok()?,
            italic: fonts::load_body(fam, FontStyle::Italic).ok()?,
            bolditalic: fonts::load_body(fam, FontStyle::BoldItalic).ok()?,
            mono: fonts::load_mono(FontStyle::Regular).ok()?,
        })
    }

    fn get(&self, slot: u8) -> &Font {
        match slot {
            F_BOLD => &self.bold,
            F_ITALIC => &self.italic,
            F_BOLDITALIC => &self.bolditalic,
            F_MONO => &self.mono,
            _ => &self.body,
        }
    }

    /// Advance of `c` in 1/1000 em (PDF text space) for the slot's face.
    fn advance(&self, slot: u8, c: char) -> f32 {
        self.get(slot).advance_1000(c) as f32
    }
}

/// Resolve a font slot from inline style flags.
fn slot_of(bold: bool, italic: bool, mono: bool) -> u8 {
    if mono {
        F_MONO
    } else if bold && italic {
        F_BOLDITALIC
    } else if bold {
        F_BOLD
    } else if italic {
        F_ITALIC
    } else {
        F_BODY
    }
}

/// A line-breaking token: a maximal run of non-space chars (a word) or a single
/// inter-word space, each carrying a font slot.
#[derive(Clone)]
struct Tok {
    text: String,
    slot: u8,
    space: bool,
    /// True if this token came from inline link content.
    link: bool,
}

/// Render a document to PDF bytes.
///
/// # Errors
/// Infallible in practice (the bundled fonts always parse); returns [`Result`]
/// to leave room for future validation without a signature change.
pub fn render(doc: &Document, opts: &PdfOptions) -> Result<Vec<u8>> {
    let Some(faces) = Faces::load(opts) else {
        // The bundled fonts are tested to parse, so this is unreachable in
        // practice; emit a valid empty document rather than failing.
        return Ok(empty_pdf());
    };
    let lines = layout(&doc.blocks, opts, &faces);
    Ok(serialize(&lines, opts, &faces))
}

// ---- layout -----------------------------------------------------------------

fn layout(blocks: &[Block], opts: &PdfOptions, faces: &Faces) -> Vec<Line> {
    let mut out = Vec::new();
    let mut next_bg = 0u32;
    layout_blocks(blocks, 0.0, &mut out, &mut next_bg, opts, faces);
    out
}

fn layout_blocks(
    blocks: &[Block],
    indent: f32,
    out: &mut Vec<Line>,
    next_bg: &mut u32,
    opts: &PdfOptions,
    faces: &Faces,
) {
    for block in blocks {
        layout_block(block, indent, out, next_bg, opts, faces);
    }
}

fn layout_block(
    block: &Block,
    indent: f32,
    out: &mut Vec<Line>,
    next_bg: &mut u32,
    opts: &PdfOptions,
    faces: &Faces,
) {
    match block {
        Block::Heading { level, inlines } => {
            let size = match level {
                1 => 24.0,
                2 => 19.0,
                3 => 16.0,
                4 => 13.5,
                5 => 12.0,
                _ => 11.0,
            };
            // Headings render bold; inner emphasis becomes bold-italic.
            let mut toks = Vec::new();
            tokenize(inlines, true, false, false, &mut toks);
            layout_inlines(toks, indent, size, 6.0, faces, out);
        }
        Block::Paragraph(inlines) => {
            let mut toks = Vec::new();
            tokenize(inlines, false, false, false, &mut toks);
            layout_inlines(toks, indent, 11.0, 7.0, faces, out);
        }
        Block::CodeBlock { code, .. } => {
            *next_bg += 1;
            let gid = *next_bg;
            let mut any = false;
            for raw in code.lines() {
                any = true;
                let clipped =
                    clip_to_width(raw, CONTENT_W - indent - CODE_PAD_X, 9.5, F_MONO, faces);
                out.push(Line {
                    size: 9.5,
                    gap_after: 1.5,
                    rule: false,
                    rule_x: 0.0,
                    quote_bars: Vec::new(),
                    bg: gid,
                    segs: vec![Seg {
                        x: MARGIN + indent + CODE_PAD_X,
                        slot: F_MONO,
                        text: clipped,
                        link: false,
                        width: 0.0,
                    }],
                });
            }
            if !any {
                // An empty fence still gets a one-line-tall panel.
                out.push(Line {
                    size: 9.5,
                    gap_after: 1.5,
                    rule: false,
                    rule_x: 0.0,
                    quote_bars: Vec::new(),
                    bg: gid,
                    segs: vec![Seg {
                        x: MARGIN + indent + CODE_PAD_X,
                        slot: F_MONO,
                        text: String::new(),
                        link: false,
                        width: 0.0,
                    }],
                });
            }
            gap(out, 6.0);
        }
        Block::BlockQuote(inner) => {
            let start = out.len();
            layout_blocks(inner, indent + 18.0, out, next_bg, opts, faces);
            let bar_x = MARGIN + indent + 6.0; // sits in the reserved 18pt gutter
            if let Some(lines) = out.get_mut(start..) {
                for line in lines {
                    line.quote_bars.push((start, bar_x)); // `start` = unique quote id
                }
            }
            gap(out, 3.0);
        }
        Block::List(list) => layout_list(list, indent, out, opts, faces),
        Block::Table(table) => {
            // v0: tab-joined rows; header bold, cells body (inline styling TBD).
            let header = table
                .head
                .iter()
                .map(|c| inline_text(c))
                .collect::<Vec<_>>()
                .join("   |   ");
            let mut toks = Vec::new();
            push_text_tokens(&header, F_BOLD, false, &mut toks);
            layout_inlines(toks, indent, 11.0, 2.0, faces, out);
            for row in &table.rows {
                let cells = row
                    .iter()
                    .map(|c| inline_text(c))
                    .collect::<Vec<_>>()
                    .join("   |   ");
                let mut toks = Vec::new();
                push_text_tokens(&cells, F_BODY, false, &mut toks);
                layout_inlines(toks, indent, 11.0, 2.0, faces, out);
            }
            gap(out, 6.0);
        }
        Block::ThematicBreak => {
            out.push(Line {
                size: 6.0,
                gap_after: 8.0,
                rule: true,
                rule_x: MARGIN + indent,
                quote_bars: Vec::new(),
                bg: 0,
                segs: Vec::new(),
            });
        }
        Block::HtmlBlock(html) => {
            if !opts.allow_raw_html {
                let mut toks = Vec::new();
                push_text_tokens(html, F_BODY, false, &mut toks);
                layout_inlines(toks, indent, 11.0, 7.0, faces, out);
            }
        }
    }
}

fn layout_list(list: &List, indent: f32, out: &mut Vec<Line>, _opts: &PdfOptions, faces: &Faces) {
    for (i, item) in list.items.iter().enumerate() {
        let marker = match item.task {
            Some(true) => "[x]".to_string(),
            Some(false) => "[ ]".to_string(),
            None if list.ordered => format!("{}.", list.start + i as u64),
            None => "•".to_string(),
        };
        let mut toks = Vec::new();
        // Marker in the body face, then a space, then the styled item content.
        push_text_tokens(&format!("{marker} "), F_BODY, false, &mut toks);
        for b in &item.blocks {
            match b {
                Block::Paragraph(inl) => tokenize(inl, false, false, false, &mut toks),
                other => push_text_tokens(&block_plain(other), F_BODY, false, &mut toks),
            }
        }
        layout_inlines(toks, indent + 16.0, 11.0, 2.0, faces, out);
    }
    gap(out, 6.0);
}

/// Tokenize inlines into styled line-breaking tokens, tracking inherited style.
///
/// `link` is the inherited link flag; the `Inline::Link` arm forces it `true`
/// for its content so link runs can be colored + underlined at render time.
fn tokenize(inlines: &[Inline], bold: bool, italic: bool, link: bool, out: &mut Vec<Tok>) {
    for inl in inlines {
        match inl {
            Inline::Text(t) => push_text_tokens(t, slot_of(bold, italic, false), link, out),
            Inline::Code(t) => push_text_tokens(t, F_MONO, link, out),
            Inline::Strong(c) => tokenize(c, true, italic, link, out),
            Inline::Emphasis(c) => tokenize(c, bold, true, link, out),
            Inline::Strikethrough(c) => tokenize(c, bold, italic, link, out),
            Inline::Link { content, .. } => tokenize(content, bold, italic, true, out),
            Inline::Image { alt, .. } => {
                push_text_tokens(alt, slot_of(bold, italic, false), link, out);
            }
            Inline::SoftBreak | Inline::HardBreak => out.push(Tok {
                text: " ".to_string(),
                slot: slot_of(bold, italic, false),
                space: true,
                link,
            }),
            Inline::Html(_) => {}
        }
    }
}

/// Split `text` into word + single-space tokens (preserving spaces) with `slot`.
fn push_text_tokens(text: &str, slot: u8, link: bool, out: &mut Vec<Tok>) {
    let mut word = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            if !word.is_empty() {
                out.push(Tok {
                    text: std::mem::take(&mut word),
                    slot,
                    space: false,
                    link,
                });
            }
            out.push(Tok {
                text: " ".to_string(),
                slot,
                space: true,
                link,
            });
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push(Tok {
            text: word,
            slot,
            space: false,
            link,
        });
    }
}

/// Saturating f32-points -> integer milli-point [`LayoutUnit`] (deterministic).
#[inline]
fn lu_from_points_f32(pts: f32) -> LayoutUnit {
    LayoutUnit::from_milli_points((pts * 1000.0).round() as i32)
}

/// f32 point size -> [`FontSize`] (milli-points).
#[inline]
fn font_size_of(size: f32) -> FontSize {
    FontSize::from_milli_points((size * 1000.0).round() as u32)
}

/// Per-slot box width: sum each slot-run via the slot's own face (which already
/// `impl PairMetrics`). Cross-slot kerning is intentionally dropped to match the
/// renderer, which applies GPOS per-segment (per-slot) only.
fn measure_word(runs: &[Tok], fs: FontSize, faces: &Faces) -> LayoutUnit {
    let mut w = LayoutUnit::ZERO;
    for t in runs {
        w += measure_text_with_pairs(faces.get(t.slot), &t.text, fs);
    }
    w
}

/// Build a TeX item stream from styled tokens, plus a parallel token map so each
/// `ParagraphItem` index can be mapped back to the exact tokens (with slots +
/// link flags) that produced it. Words -> `Box`; a single space between two
/// words -> `Glue`; a trailing forced penalty ends the paragraph. Leading,
/// duplicate, and trailing spaces are collapsed for cleaner breakpoints.
fn build_paragraph(
    toks: &[Tok],
    fs: FontSize,
    faces: &Faces,
) -> (Vec<ParagraphItem>, Vec<Vec<Tok>>) {
    let mut items: Vec<ParagraphItem> = Vec::new();
    let mut item_toks: Vec<Vec<Tok>> = Vec::new();
    let mut word: Vec<Tok> = Vec::new();

    let flush_word =
        |items: &mut Vec<ParagraphItem>, item_toks: &mut Vec<Vec<Tok>>, word: &mut Vec<Tok>| {
            if word.is_empty() {
                return;
            }
            let plain: String = word.iter().map(|t| t.text.as_str()).collect();
            let width = measure_word(word, fs, faces);
            items.push(ParagraphItem::Box(TextBox {
                text: plain.clone(),
                runs: StyledText::plain(&plain), // unused by breaker; width is what matters
                width,
            }));
            item_toks.push(std::mem::take(word));
        };

    for tok in toks {
        if tok.space {
            if !word.is_empty() {
                flush_word(&mut items, &mut item_toks, &mut word);
            }
            // Only emit glue *between* two words (collapses runs of spaces).
            if matches!(items.last(), Some(ParagraphItem::Box(_))) {
                let gw = measure_text_with_pairs(faces.get(tok.slot), " ", fs);
                items.push(ParagraphItem::Glue(default_interword_glue(gw)));
                item_toks.push(vec![tok.clone()]);
            }
        } else {
            word.push(tok.clone());
        }
    }
    flush_word(&mut items, &mut item_toks, &mut word);

    items.push(ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: crate::layout::FORCED_BREAK_PENALTY,
        flagged: false,
    }));
    item_toks.push(Vec::new());
    (items, item_toks)
}

/// Optimal-break (Knuth-Plass) styled tokens into baseline lines of positioned
/// segments. Falls back to the greedy wrapper only if the optimizer yields
/// nothing (effectively unreachable given the trailing forced penalty).
fn layout_inlines(
    toks: Vec<Tok>,
    indent: f32,
    size: f32,
    gap_after: f32,
    faces: &Faces,
    out: &mut Vec<Line>,
) {
    let left = MARGIN + indent;
    let fs = font_size_of(size);
    let (items, item_toks) = build_paragraph(&toks, fs, faces);

    // No renderable words -> just advance the vertical gap (old empty behavior).
    if !items.iter().any(|it| matches!(it, ParagraphItem::Box(_))) {
        gap(out, gap_after);
        return;
    }

    let content_w = lu_from_points_f32((CONTENT_W - indent).max(40.0));
    let breaks = break_paragraph(&items, content_w);
    if breaks.is_empty() {
        // Emergency fallback: the optimizer produced nothing.
        layout_inlines_greedy(toks, indent, size, gap_after, faces, out);
        return;
    }

    let n = breaks.len();
    for (i, lb) in breaks.iter().enumerate() {
        let mut line: Vec<Tok> = Vec::new();
        if let Some(range) = item_toks.get(lb.start..lb.end) {
            for group in range {
                line.extend(group.iter().cloned());
            }
        }
        // Drop any trailing space tokens (e.g. an interior glue at line end).
        while line.last().is_some_and(|t| t.space) {
            line.pop();
        }
        let segs = build_segs(&line, left, size, faces);
        out.push(Line {
            size,
            gap_after: if i + 1 == n { gap_after } else { 0.0 },
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            segs,
        });
    }
}

/// The original greedy wrapper, kept as an emergency fallback (and as a
/// regression oracle in tests).
fn layout_inlines_greedy(
    toks: Vec<Tok>,
    indent: f32,
    size: f32,
    gap_after: f32,
    faces: &Faces,
    out: &mut Vec<Line>,
) {
    let left = MARGIN + indent;
    let max = (CONTENT_W - indent).max(40.0);
    let mut lines: Vec<Vec<Tok>> = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut cur_w = 0.0_f32;
    for tok in toks {
        let tw = token_width(&tok, size, faces);
        if tok.space {
            if !cur.is_empty() {
                cur.push(tok);
                cur_w += tw;
            }
        } else {
            if !cur.is_empty() && cur_w + tw > max {
                trim_trailing_spaces(&mut cur, &mut cur_w, size, faces);
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
            }
            cur.push(tok);
            cur_w += tw;
        }
    }
    if !cur.is_empty() {
        trim_trailing_spaces(&mut cur, &mut cur_w, size, faces);
        lines.push(cur);
    }

    if lines.is_empty() {
        gap(out, gap_after);
        return;
    }
    let n = lines.len();
    for (i, line) in lines.into_iter().enumerate() {
        let segs = build_segs(&line, left, size, faces);
        out.push(Line {
            size,
            gap_after: if i + 1 == n { gap_after } else { 0.0 },
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            segs,
        });
    }
}

fn trim_trailing_spaces(cur: &mut Vec<Tok>, cur_w: &mut f32, size: f32, faces: &Faces) {
    while cur.last().is_some_and(|t| t.space) {
        if let Some(t) = cur.pop() {
            *cur_w -= token_width(&t, size, faces);
        }
    }
}

/// Group consecutive same-slot, same-link tokens into positioned segments,
/// accumulating each segment's layout (non-kerned) advance width.
fn build_segs(toks: &[Tok], left: f32, size: f32, faces: &Faces) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut x = left;
    let mut cur: Option<Seg> = None;
    for tok in toks {
        let tw = token_width(tok, size, faces);
        match &mut cur {
            Some(s) if s.slot == tok.slot && s.link == tok.link => {
                s.text.push_str(&tok.text);
                s.width += tw;
            }
            _ => {
                if let Some(s) = cur.take() {
                    segs.push(s);
                }
                cur = Some(Seg {
                    x,
                    slot: tok.slot,
                    text: tok.text.clone(),
                    link: tok.link,
                    width: tw,
                });
            }
        }
        x += tw;
    }
    if let Some(s) = cur {
        segs.push(s);
    }
    segs
}

fn token_width(tok: &Tok, size: f32, faces: &Faces) -> f32 {
    tok.text
        .chars()
        .map(|c| faces.advance(tok.slot, c))
        .sum::<f32>()
        * size
        / 1000.0
}

fn gap(out: &mut [Line], amount: f32) {
    if let Some(last) = out.last_mut() {
        last.gap_after += amount;
    }
}

// ---- pagination + serialization --------------------------------------------

fn serialize(lines: &[Line], opts: &PdfOptions, faces: &Faces) -> Vec<u8> {
    // Which slots actually appear (skip embedding unused faces).
    let mut used_slots: Vec<u8> = SLOTS
        .into_iter()
        .filter(|&s| {
            lines
                .iter()
                .flat_map(|l| l.segs.iter())
                .any(|seg| seg.slot == s && !seg.text.is_empty())
        })
        .collect();
    if used_slots.is_empty() {
        used_slots.push(F_BODY); // always embed at least one face
    }

    // Subset each used face to the characters it renders, and keep the parsed
    // subset (its cmap gives the new glyph ids we encode in the content stream).
    let mut subsets: Vec<EmbeddedFace> = Vec::new();
    for &slot in &used_slots {
        let source = faces.get(slot);
        let lig = source.gsub_ligatures();
        let mut chars: BTreeSet<char> = BTreeSet::new();
        let mut shaped_glyphs: BTreeSet<u16> = BTreeSet::new();
        let mut lig_src_uni: BTreeMap<u16, String> = BTreeMap::new();
        for seg in lines
            .iter()
            .flat_map(|l| l.segs.iter())
            .filter(|seg| seg.slot == slot)
        {
            chars.extend(seg.text.chars());
            let (shaped, ligs) = shape(source, &lig, &seg.text);
            shaped_glyphs.extend(shaped);
            for (g, s) in ligs {
                lig_src_uni.entry(g).or_insert(s);
            }
        }
        let keep: Vec<char> = chars.into_iter().collect();
        // Seed the subset with the chars' glyphs (so the cmap resolves) plus the
        // shaped glyphs (which add ligature glyphs no character maps to).
        let mut seed: Vec<u16> = keep.iter().map(|&c| source.glyph_index(c)).collect();
        seed.extend(shaped_glyphs);
        let Some((bytes, map)) = source.subset_glyphs(&seed, &keep) else {
            return empty_pdf();
        };
        let Ok(font) = Font::parse(bytes.clone()) else {
            return empty_pdf();
        };
        // Re-key ligature ToUnicode entries by the new (subset) glyph id.
        let mut lig_uni: BTreeMap<u16, String> = BTreeMap::new();
        for (src, s) in lig_src_uni {
            if let Some(&new) = map.get(&src) {
                lig_uni.insert(new, s);
            }
        }
        subsets.push(EmbeddedFace {
            slot,
            bytes,
            font,
            kern: source.gpos_kerning(),
            lig,
            map,
            lig_uni,
        });
    }

    // PASS 1 — place every line on a page with its baseline `y` (identical
    // pagination math to the single-pass writer). Backgrounds need each panel's
    // full vertical extent, which is only known once all its lines are placed.
    let top = PAGE_H - MARGIN;
    let mut pages_placed: Vec<Vec<Placed>> = vec![Vec::new()];
    let mut y = top;
    for line in lines {
        let leading = line.size * 1.32;
        if y - leading < MARGIN {
            pages_placed.push(Vec::new());
            y = top;
        }
        y -= leading;
        if let Some(page) = pages_placed.last_mut() {
            page.push(Placed { line, y });
        }
        y -= line.gap_after;
    }

    // PASS 2 — per page: backgrounds (code panels + inline-code chips) FIRST,
    // then text + rules, then blockquote gutter bars.
    let mut pages: Vec<String> = Vec::new();
    for placed in &pages_placed {
        let mut bg = String::new();
        let mut body = String::new();

        // (a) Code panels: maximal runs of equal nonzero `bg` id within the page.
        let mut i = 0;
        while i < placed.len() {
            let Some(first) = placed.get(i) else { break };
            let gid = first.line.bg;
            if gid == 0 {
                i += 1;
                continue;
            }
            let mut j = i;
            while placed.get(j).is_some_and(|p| p.line.bg == gid) {
                j += 1;
            }
            if let (Some(head), Some(tail)) = (placed.get(i), placed.get(j.saturating_sub(1))) {
                let size = head.line.size;
                let x_text = head.line.segs.first().map_or(MARGIN, |s| s.x);
                let x0 = x_text - CODE_PAD_X;
                let x1 = PAGE_W - MARGIN;
                let top_y = head.y + size * PANEL_ASCENT_FRAC + PANEL_PAD_V;
                let bot_y = tail.y - size * PANEL_DESCENT_FRAC - PANEL_PAD_V;
                bg.push_str(&rounded_rect_fill(
                    x0,
                    bot_y,
                    x1,
                    top_y,
                    PANEL_RADIUS,
                    PANEL_GRAY,
                ));
            }
            i = j.max(i + 1);
        }

        // (b) Inline-code chips: F_MONO segs on non-panel, non-rule lines.
        for p in placed {
            if p.line.bg != 0 || p.line.rule {
                continue;
            }
            for seg in &p.line.segs {
                if seg.slot != F_MONO || seg.text.trim().is_empty() {
                    continue;
                }
                let w = text_width(&seg.text, p.line.size, F_MONO, faces);
                let cx0 = seg.x - CHIP_PAD_X;
                let cx1 = seg.x + w + CHIP_PAD_X;
                let cy0 = p.y - p.line.size * 0.26;
                let cy1 = p.y + p.line.size * 0.74;
                bg.push_str(&rounded_rect_fill(
                    cx0,
                    cy0,
                    cx1,
                    cy1,
                    CHIP_RADIUS,
                    CHIP_GRAY,
                ));
            }
        }

        // (c) Text + rules.
        for p in placed {
            let line = p.line;
            let y = p.y;
            if line.rule {
                let x2 = PAGE_W - MARGIN;
                body.push_str(&format!(
                    "0.82 0.82 0.84 RG 0.7 w {x:.2} {yy:.2} m {x2:.2} {yy:.2} l S\n",
                    x = line.rule_x,
                    yy = y + line.size * 0.5,
                ));
            } else {
                for seg in &line.segs {
                    if seg.text.is_empty() {
                        continue;
                    }
                    if let Some(face) = subsets.iter().find(|f| f.slot == seg.slot) {
                        let source = faces.get(seg.slot);
                        let gids: Vec<u16> =
                            seg.text.chars().map(|c| source.glyph_index(c)).collect();
                        let shaped = face.lig.substitute(&gids);
                        if seg.link {
                            let (r, g, b) = LINK_COLOR;
                            body.push_str(&format!("{r:.3} {g:.3} {b:.3} rg\n"));
                        }
                        body.push_str(&format!(
                            "BT /F{f} {s:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm {tj} TJ ET\n",
                            f = seg.slot,
                            s = line.size,
                            x = seg.x,
                            y = y,
                            tj = kerned_tj(&face.map, source, &face.kern, &shaped),
                        ));
                        if seg.link {
                            let (r, g, b) = LINK_COLOR;
                            let uy = y - line.size * 0.12;
                            let uw = (line.size * 0.06).max(0.4);
                            body.push_str(&format!(
                                "{r:.3} {g:.3} {b:.3} RG {uw:.2} w \
                                 {x1:.2} {uy:.2} m {x2:.2} {uy:.2} l S\n0 0 0 rg\n",
                                x1 = seg.x,
                                x2 = seg.x + seg.width,
                            ));
                        }
                    }
                }
            }
        }

        // (d) Blockquote gutter bars: accumulate each quote's vertical extent on
        // this page (keyed by quote id), then stroke one segment per quote.
        let mut quote_acc: BTreeMap<usize, (f32, f32, f32)> = BTreeMap::new();
        for p in placed {
            for &(id, bar_x) in &p.line.quote_bars {
                let top_y = p.y + p.line.size * 0.85;
                let bot_y = p.y - p.line.size * 0.20;
                quote_acc
                    .entry(id)
                    .and_modify(|e| e.2 = bot_y)
                    .or_insert((bar_x, top_y, bot_y));
            }
        }
        flush_quote_bars(&mut body, &mut quote_acc);

        pages.push(format!("{bg}{body}"));
    }
    if pages.is_empty() {
        pages.push(String::new());
    }

    build_pdf(&pages, &subsets, opts)
}

/// A line placed on a page with its computed baseline `y`.
struct Placed<'a> {
    line: &'a Line,
    y: f32,
}

/// Stroke one subtle vertical bar per accumulated blockquote, then clear.
fn flush_quote_bars(content: &mut String, acc: &mut BTreeMap<usize, (f32, f32, f32)>) {
    for (x, top, bot) in acc.values() {
        content.push_str(&format!(
            "0.75 0.75 0.80 RG 2.50 w {x:.2} {top:.2} m {x:.2} {bot:.2} l S\n"
        ));
    }
    acc.clear();
}

/// A light-gray rounded-rectangle fill, color-isolated with `q`/`Q` so the fill
/// color never leaks into following text. Built from 4 lines + 4 cubic Beziers
/// (kappa = 0.5523). Returns an empty string for degenerate rectangles.
fn rounded_rect_fill(x0: f32, y0: f32, x1: f32, y1: f32, r: f32, c: (f32, f32, f32)) -> String {
    if x1 <= x0 || y1 <= y0 {
        return String::new();
    }
    let r = r.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5).max(0.0);
    let k = r * 0.5523; // circle -> bezier magic constant
    let (rc, gc, bc) = c;
    format!(
        "q {rc:.3} {gc:.3} {bc:.3} rg \
         {xa:.2} {y0:.2} m {xb:.2} {y0:.2} l \
         {br1x:.2} {y0:.2} {x1:.2} {br2y:.2} {x1:.2} {ya:.2} c \
         {x1:.2} {yb:.2} l \
         {x1:.2} {tr1y:.2} {tr2x:.2} {y1:.2} {xb:.2} {y1:.2} c \
         {xa:.2} {y1:.2} l \
         {tl1x:.2} {y1:.2} {x0:.2} {tl2y:.2} {x0:.2} {yb:.2} c \
         {x0:.2} {ya:.2} l \
         {x0:.2} {bl1y:.2} {bl2x:.2} {y0:.2} {xa:.2} {y0:.2} c f Q\n",
        xa = x0 + r,
        xb = x1 - r,
        ya = y0 + r,
        yb = y1 - r,
        br1x = x1 - r + k,
        br2y = y0 + r - k,
        tr1y = y1 - r + k,
        tr2x = x1 - r + k,
        tl1x = x0 + r - k,
        tl2y = y1 - r + k,
        bl1y = y0 + r - k,
        bl2x = x0 + r - k,
    )
}

/// A subset face ready to embed.
struct EmbeddedFace {
    slot: u8,
    bytes: Vec<u8>,
    font: Font,
    /// GPOS pair kerning of the SOURCE face (the subset drops GPOS), keyed by
    /// original glyph ids — used to position glyphs in the content stream.
    kern: Kerning,
    /// GSUB ligatures of the SOURCE face, applied to shape content lines.
    lig: Ligatures,
    /// Source glyph id -> subset (renumbered) glyph id.
    map: BTreeMap<u16, u16>,
    /// Subset glyph id -> its source characters, for ligature glyphs that no
    /// character maps to (keeps ligated text selectable via ToUnicode).
    lig_uni: BTreeMap<u16, String>,
}

fn build_pdf(pages: &[String], faces: &[EmbeddedFace], opts: &PdfOptions) -> Vec<u8> {
    let p = pages.len();
    let nf = faces.len();
    let title = opts.title.clone().unwrap_or_default();

    // Object number plan (1-indexed):
    //   1 Catalog, 2 Pages, [3..3+p) Page objs, [3+p..3+2p) content streams,
    //   then per face k: type0, cidfont, descriptor, fontfile, tounicode (5),
    //   then Info (optional).
    let page_obj = |i: usize| 3 + i;
    let content_obj = |i: usize| 3 + p + i;
    let face_base = 3 + 2 * p;
    let type0_obj = |k: usize| face_base + 5 * k;
    let cid_obj = |k: usize| face_base + 5 * k + 1;
    let desc_obj = |k: usize| face_base + 5 * k + 2;
    let file_obj = |k: usize| face_base + 5 * k + 3;
    let touni_obj = |k: usize| face_base + 5 * k + 4;
    let info_obj = face_base + 5 * nf;
    let total_objs = if title.is_empty() {
        info_obj - 1
    } else {
        info_obj
    };

    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = vec![0; total_objs + 1];

    let emit = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, n: usize, body: &str| {
        offsets[n] = buf.len();
        buf.extend_from_slice(format!("{n} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    emit(
        &mut buf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R >>",
    );

    let kids = (0..p)
        .map(|i| format!("{} 0 R", page_obj(i)))
        .collect::<Vec<_>>()
        .join(" ");
    emit(
        &mut buf,
        &mut offsets,
        2,
        &format!("<< /Type /Pages /Count {p} /Kids [ {kids} ] >>"),
    );

    // Shared font resource dict referencing every embedded face's Type0 object.
    let font_res = faces
        .iter()
        .enumerate()
        .map(|(k, f)| format!("/F{} {} 0 R", f.slot, type0_obj(k)))
        .collect::<Vec<_>>()
        .join(" ");
    for i in 0..p {
        emit(
            &mut buf,
            &mut offsets,
            page_obj(i),
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.0} {PAGE_H:.0}] \
                 /Resources << /Font << {font_res} >> >> /Contents {c} 0 R >>",
                c = content_obj(i),
            ),
        );
    }

    for (i, page) in pages.iter().enumerate() {
        offsets[content_obj(i)] = buf.len();
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {len} >>\nstream\n{page}endstream\nendobj\n",
                n = content_obj(i),
                len = page.len(),
            )
            .as_bytes(),
        );
    }

    // Embedded font object groups.
    for (k, face) in faces.iter().enumerate() {
        let psname = subset_psname(k, face.slot);
        let m = FaceMetrics::of(&face.font);

        emit(
            &mut buf,
            &mut offsets,
            type0_obj(k),
            &format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{psname} /Encoding /Identity-H \
                 /DescendantFonts [{cid} 0 R] /ToUnicode {tu} 0 R >>",
                cid = cid_obj(k),
                tu = touni_obj(k),
            ),
        );
        emit(
            &mut buf,
            &mut offsets,
            cid_obj(k),
            &format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{psname} \
                 /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                 /FontDescriptor {desc} 0 R /CIDToGIDMap /Identity /DW 1000 /W [{w}] >>",
                desc = desc_obj(k),
                w = widths_array(&face.font),
            ),
        );
        let italic_angle = if face.slot == F_ITALIC { -12 } else { 0 };
        emit(
            &mut buf,
            &mut offsets,
            desc_obj(k),
            &format!(
                "<< /Type /FontDescriptor /FontName /{psname} /Flags 4 \
                 /FontBBox [{bx0} {by0} {bx1} {by1}] /ItalicAngle {italic_angle} \
                 /Ascent {asc} /Descent {desc} /CapHeight {cap} /StemV 80 /FontFile2 {ff} 0 R >>",
                bx0 = -200,
                by0 = m.descent - 50,
                bx1 = 1100,
                by1 = m.ascent + 50,
                asc = m.ascent,
                desc = m.descent,
                cap = m.cap_height,
                ff = file_obj(k),
            ),
        );
        // FontFile2: FlateDecode-compressed subset font program. /Length1 is the
        // UNCOMPRESSED program length per the PDF spec.
        offsets[file_obj(k)] = buf.len();
        let font_comp = crate::compress::zlib_compress(&face.bytes);
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {clen} /Length1 {olen} /Filter /FlateDecode >>\nstream\n",
                n = file_obj(k),
                clen = font_comp.len(),
                olen = face.bytes.len(),
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&font_comp);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        // ToUnicode CMap (left uncompressed so it stays greppable + tiny).
        let cmap = tounicode_cmap(&face.font, &face.lig_uni);
        offsets[touni_obj(k)] = buf.len();
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {len} >>\nstream\n{cmap}endstream\nendobj\n",
                n = touni_obj(k),
                len = cmap.len(),
            )
            .as_bytes(),
        );
    }

    if !title.is_empty() {
        emit(
            &mut buf,
            &mut offsets,
            info_obj,
            &format!("<< /Title ({}) >>", pdf_escape(&title)),
        );
    }

    // xref + trailer.
    let xref_pos = buf.len();
    let size = total_objs + 1;
    buf.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets.iter().take(total_objs + 1).skip(1) {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    let info = if title.is_empty() {
        String::new()
    } else {
        format!(" /Info {info_obj} 0 R")
    };
    buf.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R{info} >>\nstartxref\n{xref_pos}\n%%EOF\n")
            .as_bytes(),
    );
    buf
}

/// FontDescriptor metrics in 1/1000 em.
struct FaceMetrics {
    ascent: i32,
    descent: i32,
    cap_height: i32,
}

impl FaceMetrics {
    fn of(font: &Font) -> Self {
        let upm = font.units_per_em.max(1) as i32;
        let scale = |v: i32| v * 1000 / upm;
        Self {
            ascent: scale(font.ascent as i32),
            descent: scale(font.descent as i32),
            cap_height: scale((font.ascent as i32 * 7) / 10),
        }
    }
}

/// `/W` widths array `[ 0 [w0 w1 ...] ]` (1/1000 em, indexed by glyph id = CID).
fn widths_array(font: &Font) -> String {
    let upm = font.units_per_em.max(1) as u32;
    let mut s = String::from("0 [");
    for gid in 0..font.num_glyphs {
        let w = font.advance_width(gid) as u32 * 1000 / upm;
        s.push_str(&w.to_string());
        s.push(' ');
    }
    s.push(']');
    s
}

/// Shape `text` with `source`'s ligatures, returning the shaped SOURCE glyph ids
/// and, for each emitted ligature, its source characters (so a `ToUnicode` entry
/// can keep the ligated text selectable).
fn shape(source: &Font, lig: &Ligatures, text: &str) -> (Vec<u16>, Vec<(u16, String)>) {
    let chars: Vec<char> = text.chars().collect();
    let gids: Vec<u16> = chars.iter().map(|&c| source.glyph_index(c)).collect();
    let mut shaped = Vec::with_capacity(gids.len());
    let mut lig_uni = Vec::new();
    let mut ci = 0;
    for (gid, count) in lig.substitute_with_spans(&gids) {
        shaped.push(gid);
        if count > 1 {
            let s: String = chars.get(ci..ci + count).unwrap_or(&[]).iter().collect();
            lig_uni.push((gid, s));
        }
        ci += count;
    }
    (shaped, lig_uni)
}

/// Build a `TJ` array (without the trailing `TJ`) from a pre-shaped SOURCE glyph
/// sequence: each glyph is emitted as its subset id via `map`, with GPOS pair
/// kerning (looked up on the original ids) inserted between glyphs.
fn kerned_tj(map: &BTreeMap<u16, u16>, source: &Font, kern: &Kerning, shaped: &[u16]) -> String {
    let upm = i32::from(source.units_per_em.max(1));
    let mut out = String::from("[<");
    for (i, &g) in shaped.iter().enumerate() {
        out.push_str(&format!("{:04X}", map.get(&g).copied().unwrap_or(0)));
        if let Some(&next) = shaped.get(i + 1) {
            let k = kern.pair(g, next);
            if k != 0 {
                // A TJ number shifts the next glyph left by number/1000 em, so a
                // tightening (negative) kern becomes a positive number.
                let adj = -(i32::from(k) * 1000 / upm);
                out.push_str(&format!(">{adj}<"));
            }
        }
    }
    out.push_str(">]");
    out
}

/// A `ToUnicode` CMap mapping each glyph id back to its character(s), so text
/// stays selectable. Only the glyphs the document uses appear.
fn tounicode_cmap(font: &Font, lig_uni: &BTreeMap<u16, String>) -> String {
    // (gid, UTF-16BE hex) over the chars present in the subset cmap, plus the
    // ligature glyphs (which no character maps to) so ligated text stays
    // selectable.
    let mut entries: Vec<(u16, String)> = Vec::new();
    for cp in 0x20u32..0x2C00 {
        if let Some(c) = char::from_u32(cp) {
            let g = font.glyph_index(c);
            if g != 0 {
                entries.push((g, utf16be_hex(c)));
            }
        }
    }
    for (g, s) in lig_uni {
        entries.push((*g, s.chars().map(utf16be_hex).collect()));
    }
    entries.sort_by_key(|&(g, _)| g);
    entries.dedup_by_key(|(g, _)| *g);

    let mut body = String::new();
    for chunk in entries.chunks(100) {
        body.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (g, hex) in chunk {
            body.push_str(&format!("<{g:04X}> <{hex}>\n"));
        }
        body.push_str("endbfchar\n");
    }
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         {body}endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

fn utf16be_hex(c: char) -> String {
    let mut s = String::new();
    let mut buf = [0u16; 2];
    for u in c.encode_utf16(&mut buf) {
        s.push_str(&format!("{u:04X}"));
    }
    s
}

/// Deterministic subset PostScript name, e.g. `FMDFA1+Embedded`.
fn subset_psname(k: usize, slot: u8) -> String {
    let tag: String = (0..6)
        .map(|i| (b'A' + ((k as u8 + slot + i as u8) % 26)) as char)
        .collect();
    format!("{tag}+Embedded")
}

/// A minimal but valid empty single-page PDF (degenerate fallback).
fn empty_pdf() -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets = [0usize; 4];
    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    for (n, body) in [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        &format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.0} {PAGE_H:.0}] >>"),
    ]
    .into_iter()
    .enumerate()
    {
        offsets[n + 1] = buf.len();
        buf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", n + 1).as_bytes());
    }
    let xref_pos = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes(),
    );
    buf
}

// ---- text helpers -----------------------------------------------------------

fn pdf_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '(' => o.push_str("\\("),
            ')' => o.push_str("\\)"),
            '\\' => o.push_str("\\\\"),
            '\r' => o.push_str("\\r"),
            '\n' => o.push(' '),
            c if (c as u32) < 256 => o.push(c),
            _ => o.push('?'),
        }
    }
    o
}

fn clip_to_width(text: &str, max_width: f32, size: f32, font: u8, faces: &Faces) -> String {
    if text_width(text, size, font, faces) <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut w = 0.0;
    for c in text.chars() {
        let cw = faces.advance(font, c) * size / 1000.0;
        if w + cw > max_width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out
}

fn text_width(s: &str, size: f32, font: u8, faces: &Faces) -> f32 {
    s.chars().map(|c| faces.advance(font, c)).sum::<f32>() * size / 1000.0
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => s.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                s.push_str(&inline_text(c));
            }
            Inline::Link { content, .. } => s.push_str(&inline_text(content)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::Html(_) => {}
        }
    }
    s
}

fn block_plain(block: &Block) -> String {
    match block {
        Block::Paragraph(inl) | Block::Heading { inlines: inl, .. } => inline_text(inl),
        Block::CodeBlock { code, .. } => code.clone(),
        _ => String::new(),
    }
}
