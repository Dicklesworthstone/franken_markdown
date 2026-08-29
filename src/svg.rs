//! Standalone vector-SVG ("poster") backend.
//!
//! Renders a parsed Markdown document to a single vertical-flow SVG page of
//! fixed width ([`SvgOptions::max_width_pt`]) whose height grows to fit the
//! content. Every glyph is real vector outline data — the output contains no
//! `<text>` elements — so it is resolution-independent, needs no fonts at
//! view time, and is byte-identical for a fixed input.
//!
//! Glyph strategy: each unique `(face, glyph id)` outline is emitted once in
//! `<defs>` as `<path id="gN" d="…">`, normalized to a 100 pt em with the
//! font's y-up coordinates flipped to SVG's y-down, and quantized to the
//! 0.01 determinism grid (integer round-trip, no float formatting). Use
//! sites place instances with
//! `<use href="#gN" transform="translate(x y) scale(size/100)" fill="…"/>`.
//! `transform` is used instead of the `x`/`y` attributes because those
//! cannot scale — per-glyph dedup across the heading/body/code size ladder
//! requires scaling at the use site.
//!
//! TrueType outlines are quadratic and [`outline`] already resolves implied
//! on-curve midpoints into `Segment::Line` / `Segment::Quad` chains, so the
//! path compiler is a direct 1:1 mapping: contour start → `M`, line → `L`,
//! quad → `Q`, close → `Z`.
//!
//! Deliberate scope cuts (the poster is a display artifact, not a print
//! engine): greedy word wrap on raw `hmtx` advances (no Knuth-Plass, no
//! kerning, no ligatures); inline links are coloured but not underlined;
//! inline/display math renders its TeX source in the mono face; raw HTML
//! blocks/inline HTML are skipped; footnote definitions are skipped in flow
//! (matching the AST contract); images render their alt text.
//!
//! Light palette only: a standalone vector artifact cannot honour the
//! `prefers-color-scheme` policy behind [`Theme::dark_mode`], so the poster
//! always uses [`Theme::colors`].

use std::collections::BTreeMap;

use franken_markdown::ast::{Align, Block, Document, Inline, List, Table};
use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::text::Font;
use franken_markdown::text::outline::Segment;
use franken_markdown::theme::{Theme, ThemeColors, TypeScale};

/// Def path data is normalized to a 100 pt em, so use-site scale factors are
/// simply `size / 100` (0.11 for 11 pt body) and 0.01-quantized def
/// coordinates carry 1/10 000-em precision.
const DEF_EM_PT: f64 = 100.0;

/// Face slots in `Poster::faces`.
const SLOT_BODY: usize = 0; // + style index (regular/bold/italic/bold-italic)
const SLOT_MONO: usize = 4;
const SLOT_SYMBOL: usize = 5;
const SLOT_COUNT: usize = 6;

/// Options for the SVG poster backend.
#[derive(Debug, Clone)]
pub struct SvgOptions {
    /// Visual theme (palette, page margins, font family).
    pub theme: Theme,
    /// Page width in points; the height grows to fit the content.
    /// Non-finite or absurdly narrow values fall back to US Letter width.
    pub max_width_pt: f32,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            max_width_pt: 612.0,
        }
    }
}

/// Render statistics from [`render_svg_with_report`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SvgReport {
    /// Characters skipped because no bundled face maps them.
    pub glyphs_missing: usize,
    /// Glyph instances emitted as `<use>` elements.
    pub glyphs_drawn: usize,
    /// Unique glyph outlines emitted as `<path>` defs.
    pub paths_emitted: usize,
}

/// Render `doc` to a standalone vector-SVG poster, discarding statistics.
#[must_use]
pub fn render_svg(doc: &Document, opts: &SvgOptions) -> Vec<u8> {
    render_svg_with_report(doc, opts).0
}

/// Render `doc` to a standalone vector-SVG poster plus render statistics.
#[must_use]
pub fn render_svg_with_report(doc: &Document, opts: &SvgOptions) -> (Vec<u8>, SvgReport) {
    Poster::new(opts).render(doc)
}

// ---------------------------------------------------------------------------
// Deterministic number formatting (integer round-trip; no float printing).
// ---------------------------------------------------------------------------

/// Quantize to the 0.01 grid with two fixed decimals ("12.00", "-3.45").
/// Rounds through an integer so output never carries float noise; negative
/// zero collapses to "0.00".
fn q2(v: f64) -> String {
    let q = (v * 100.0).round() as i64;
    let a = q.unsigned_abs();
    let sign = if q < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", a / 100, a % 100)
}

/// Quantize to 1/10 000 with trailing zeros trimmed (for scale factors).
fn trim4(v: f64) -> String {
    let q = (v * 10_000.0).round() as i64;
    let a = q.unsigned_abs();
    let sign = if q < 0 { "-" } else { "" };
    let int = a / 10_000;
    let mut frac = a % 10_000;
    if frac == 0 {
        return format!("{sign}{int}");
    }
    let mut width = 4;
    while frac % 10 == 0 {
        frac /= 10;
        width -= 1;
    }
    format!("{}{}.{:0w$}", sign, int, frac, w = width)
}

/// XML double-quoted attribute escaping: `&`, `<`, `>`, `"`, `'`.
fn esc_attr(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
}

// ---------------------------------------------------------------------------
// Palette and paint ops.
// ---------------------------------------------------------------------------

/// Palette slots; resolved against [`ThemeColors`] at emission time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    Fg,
    FgMuted,
    BgSubtle,
    Border,
    BorderMuted,
    CodeBg,
    QuoteFg,
    QuoteBar,
    Accent,
}

impl Ink {
    fn hex(self, colors: &ThemeColors) -> &str {
        match self {
            Self::Fg => &colors.fg,
            Self::FgMuted => &colors.fg_muted,
            Self::BgSubtle => &colors.bg_subtle,
            Self::Border => &colors.border,
            Self::BorderMuted => &colors.border_muted,
            Self::CodeBg => &colors.code_bg,
            Self::QuoteFg => &colors.quote_fg,
            Self::QuoteBar => &colors.quote_bar,
            Self::Accent => &colors.accent,
        }
    }
}

/// One paint operation, in document (painter's) order.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: Ink,
        stroke: Option<Ink>,
    },
    Rule {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        ink: Ink,
        w: f64,
    },
    Glyph {
        slot: usize,
        gid: u16,
        x: f64,
        y: f64,
        size: f64,
        ink: Ink,
    },
}

// ---------------------------------------------------------------------------
// Inline flattening and greedy wrapping.
// ---------------------------------------------------------------------------

/// Style carried by a flattened run of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RStyle {
    bold: bool,
    italic: bool,
    mono: bool,
    strike: bool,
    ink: Ink,
}

impl RStyle {
    const BODY: Self = Self {
        bold: false,
        italic: false,
        mono: false,
        strike: false,
        ink: Ink::Fg,
    };

    fn font_style(self) -> FontStyle {
        FontStyle::new(self.bold, self.italic)
    }
}

/// A flattened inline piece: styled text or a forced line break.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String, RStyle),
    Break,
}

/// One wrapped word with its measured width.
#[derive(Debug, Clone)]
struct Word {
    text: String,
    style: RStyle,
    w: f64,
}

// ---------------------------------------------------------------------------
// The poster painter.
// ---------------------------------------------------------------------------

struct Poster {
    /// Body regular/bold/italic/bold-italic, mono, symbol fallback. `None`
    /// only if a bundled face failed to parse (the registry tests make that
    /// unreachable in practice); unresolvable glyphs count as missing.
    faces: [Option<Font>; SLOT_COUNT],
    colors: ThemeColors,
    scale: TypeScale,
    line_height: f64,
    width: f64,
    margin_bottom: f64,
    ops: Vec<Op>,
    /// Top of the next block, in pt from the page top.
    y: f64,
    top: f64,
    missing: usize,
}

impl Poster {
    fn new(opts: &SvgOptions) -> Self {
        let theme = &opts.theme;
        let width = if opts.max_width_pt.is_finite() && opts.max_width_pt >= 144.0 {
            f64::from(opts.max_width_pt)
        } else {
            612.0
        };
        let family = theme.font;
        let load = |style: FontStyle| fonts::load_body(family, style).ok();
        Self {
            faces: [
                load(FontStyle::Regular),
                load(FontStyle::Bold),
                load(FontStyle::Italic),
                load(FontStyle::BoldItalic),
                fonts::load_mono(FontStyle::Regular).ok(),
                fonts::load_symbol().ok(),
            ],
            colors: theme.colors.clone(),
            scale: TypeScale::default(),
            line_height: f64::from(theme.spacing.line_height),
            width,
            margin_bottom: f64::from(theme.page.margins.bottom_pt),
            ops: Vec::new(),
            y: f64::from(theme.page.margins.top_pt),
            top: f64::from(theme.page.margins.top_pt),
            missing: 0,
        }
    }

    fn content_left(&self) -> f64 {
        // Margins come from the theme page contract, clamped so a narrow
        // poster keeps a usable measure.
        72.0_f64.min(self.width / 4.0)
    }

    fn content_right(&self) -> f64 {
        self.width - self.content_left()
    }

    fn render(mut self, doc: &Document) -> (Vec<u8>, SvgReport) {
        let l = self.content_left();
        let r = self.content_right();
        for block in &doc.blocks {
            self.block(block, l, r, false);
        }
        let height = self.y + self.margin_bottom;
        self.emit(height)
    }

    // -- glyph resolution / measurement ------------------------------------

    /// Map `ch` to `(face slot, glyph id)`: the styled primary face first,
    /// then the bundled symbol fallback. `(slot, 0)` means unmapped.
    fn resolve(&self, ch: char, st: RStyle) -> (usize, u16) {
        let slot = if st.mono {
            SLOT_MONO
        } else {
            SLOT_BODY + style_index(st.font_style())
        };
        if let Some(font) = &self.faces[slot] {
            let gid = font.glyph_index(ch);
            if gid != 0 {
                return (slot, gid);
            }
        }
        if slot != SLOT_SYMBOL
            && let Some(font) = &self.faces[SLOT_SYMBOL]
        {
            let gid = font.glyph_index(ch);
            if gid != 0 {
                return (SLOT_SYMBOL, gid);
            }
        }
        (slot, 0)
    }

    fn advance(&self, slot: usize, ch: char, size: f64) -> f64 {
        self.faces[slot]
            .as_ref()
            .map_or(0.0, |font| f64::from(font.advance_1000(ch)) * size / 1000.0)
    }

    /// Width of `text` at `size` under `st` (silent; never counts missing).
    fn measure(&self, text: &str, st: RStyle, size: f64) -> f64 {
        text.chars()
            .map(|ch| self.advance(self.resolve(ch, st).0, ch, size))
            .sum()
    }

    fn space_width(&self, st: RStyle, size: f64) -> f64 {
        self.advance(self.resolve(' ', st).0, ' ', size)
    }

    /// Draw `text` with its left edge at `x`, baseline at `baseline`; returns
    /// the pen position after the last glyph. Unmapped characters are
    /// skipped and counted; whitespace still advances.
    fn draw_text(&mut self, x: f64, baseline: f64, text: &str, st: RStyle, size: f64) -> f64 {
        let mut pen = x;
        for ch in text.chars() {
            let (slot, gid) = self.resolve(ch, st);
            let adv = self.advance(slot, ch, size);
            if gid == 0 {
                if !ch.is_whitespace() {
                    self.missing += 1;
                }
            } else if !ch.is_whitespace() {
                self.ops.push(Op::Glyph {
                    slot,
                    gid,
                    x: pen,
                    y: baseline,
                    size,
                    ink: st.ink,
                });
            }
            pen += adv;
        }
        if st.strike && pen > x {
            self.ops.push(Op::Rule {
                x1: x,
                y1: baseline - size * 0.28,
                x2: pen,
                y2: baseline - size * 0.28,
                ink: st.ink,
                w: (size * 0.05).max(0.5),
            });
        }
        pen
    }

    // -- inline flattening and wrapping -------------------------------------

    fn flatten(&self, inlines: &[Inline], st: RStyle, out: &mut Vec<Piece>) {
        for inline in inlines {
            match inline {
                Inline::Text(s) => out.push(Piece::Text(s.clone(), st)),
                Inline::Emphasis(inner) => self.flatten(inner, RStyle { italic: true, ..st }, out),
                Inline::Strong(inner) => self.flatten(inner, RStyle { bold: true, ..st }, out),
                Inline::Strikethrough(inner) => {
                    self.flatten(inner, RStyle { strike: true, ..st }, out);
                }
                Inline::Code(s) => out.push(Piece::Text(s.clone(), RStyle { mono: true, ..st })),
                Inline::Link { content, .. } => {
                    self.flatten(
                        content,
                        RStyle {
                            ink: Ink::Accent,
                            ..st
                        },
                        out,
                    );
                }
                Inline::Image { alt, .. } => {
                    let label = if alt.is_empty() {
                        "[image]".to_string()
                    } else {
                        format!("[{alt}]")
                    };
                    out.push(Piece::Text(
                        label,
                        RStyle {
                            ink: Ink::FgMuted,
                            ..st
                        },
                    ));
                }
                Inline::SoftBreak => out.push(Piece::Text(" ".to_string(), st)),
                Inline::HardBreak => out.push(Piece::Break),
                Inline::Html(_) => {}
                Inline::FootnoteRef { id } => out.push(Piece::Text(
                    format!("[^{id}]"),
                    RStyle {
                        ink: Ink::FgMuted,
                        ..st
                    },
                )),
                Inline::Math(s) | Inline::DisplayMath(s) => out.push(Piece::Text(
                    s.clone(),
                    RStyle {
                        mono: true,
                        ink: Ink::FgMuted,
                        ..st
                    },
                )),
            }
        }
    }

    /// Greedy word wrap of flattened pieces to `width` at `size`.
    fn wrap(&self, pieces: &[Piece], size: f64, width: f64) -> Vec<Vec<Word>> {
        let mut lines: Vec<Vec<Word>> = Vec::new();
        let mut cur: Vec<Word> = Vec::new();
        let mut cur_w = 0.0;
        let mut space_w = 0.0;
        let mut flush = |cur: &mut Vec<Word>, cur_w: &mut f64| {
            if !cur.is_empty() {
                lines.push(std::mem::take(cur));
            }
            *cur_w = 0.0;
        };
        for piece in pieces {
            match piece {
                Piece::Break => flush(&mut cur, &mut cur_w),
                Piece::Text(text, st) => {
                    for word in text.split_whitespace() {
                        let w = self.measure(word, *st, size);
                        let gap = if cur.is_empty() { 0.0 } else { space_w };
                        if !cur.is_empty() && cur_w + gap + w > width {
                            flush(&mut cur, &mut cur_w);
                        }
                        if !cur.is_empty() {
                            cur_w += space_w;
                        }
                        space_w = self.space_width(*st, size);
                        cur_w += w;
                        cur.push(Word {
                            text: word.to_string(),
                            style: *st,
                            w,
                        });
                    }
                }
            }
        }
        flush(&mut cur, &mut cur_w);
        lines
    }

    /// Draw one wrapped line starting at `x`, returning nothing; words are
    /// separated by single spaces measured in the preceding word's style.
    fn draw_words(&mut self, words: &[Word], x: f64, baseline: f64, size: f64) {
        let mut pen = x;
        for (i, word) in words.iter().enumerate() {
            if i > 0 {
                let prev = words[i - 1].style;
                pen += self.space_width(prev, size);
            }
            pen = self.draw_text(pen, baseline, &word.text, word.style, size);
        }
    }

    /// Total width of a wrapped line (for table cell alignment).
    fn words_width(&self, words: &[Word], size: f64) -> f64 {
        let mut total = 0.0;
        for (i, word) in words.iter().enumerate() {
            if i > 0 {
                total += self.space_width(words[i - 1].style, size);
            }
            total += word.w;
        }
        total
    }

    // -- block painters ------------------------------------------------------

    fn block(&mut self, block: &Block, l: f64, r: f64, quote: bool) {
        match block {
            Block::Heading { level, inlines } => self.heading(*level, inlines, l, r, quote),
            Block::Paragraph(inlines) => self.paragraph(inlines, l, r, quote),
            Block::CodeBlock { code, .. } => self.code_panel(code, l, r),
            Block::MathBlock(src) => self.code_panel(src, l, r),
            Block::BlockQuote(inner) => self.blockquote(inner, l, r),
            Block::List(list) => self.list(list, l, r, quote),
            Block::Table(table) => self.table(table, l, r),
            Block::ThematicBreak => {
                self.y += 4.0;
                self.ops.push(Op::Rule {
                    x1: l,
                    y1: self.y,
                    x2: r,
                    y2: self.y,
                    ink: Ink::Border,
                    w: 0.75,
                });
                self.y += 12.0;
            }
            // Raw HTML has no vector representation; footnote definitions are
            // collected by emitters that support notes (the poster does not).
            Block::HtmlBlock(_) | Block::FootnoteDefinition { .. } => {}
            Block::DefinitionList(items) => {
                let body_size = f64::from(self.scale.body);
                for item in items {
                    for term in &item.terms {
                        let mut pieces = Vec::new();
                        self.flatten(
                            term,
                            RStyle {
                                bold: true,
                                ..RStyle::BODY
                            },
                            &mut pieces,
                        );
                        self.text_lines(&pieces, body_size, l, r, quote, body_size * 0.4);
                    }
                    for def in &item.definitions {
                        let mut pieces = Vec::new();
                        self.flatten(def, RStyle::BODY, &mut pieces);
                        self.text_lines(&pieces, body_size, l + 18.0, r, quote, body_size * 0.4);
                    }
                }
                self.y += body_size * 0.4;
            }
            Block::PageBreak => {}
        }
    }

    fn default_ink(quote: bool) -> Ink {
        if quote { Ink::QuoteFg } else { Ink::Fg }
    }

    /// Lay out flattened pieces as wrapped lines at `size`; returns after
    /// adding `gap_after` below the last line.
    fn text_lines(
        &mut self,
        pieces: &[Piece],
        size: f64,
        l: f64,
        r: f64,
        _quote: bool, // ink already carried by run styles
        gap_after: f64,
    ) {
        let leading = size * self.line_height;
        let lines = self.wrap(pieces, size, r - l);
        for line in &lines {
            let baseline = self.y + size * 0.85;
            self.draw_words(line, l, baseline, size);
            self.y += leading;
        }
        self.y += gap_after;
    }

    fn heading(&mut self, level: u8, inlines: &[Inline], l: f64, r: f64, quote: bool) {
        let idx = usize::from(level.clamp(1, 6) - 1);
        let size = f64::from(self.scale.h[idx]);
        if self.y > self.top + 0.01 {
            self.y += size * 0.8;
        }
        let ink = Self::default_ink(quote);
        let mut pieces = Vec::new();
        self.flatten(
            inlines,
            RStyle {
                bold: true,
                ink,
                ..RStyle::BODY
            },
            &mut pieces,
        );
        let leading = size * 1.3;
        let lines = self.wrap(&pieces, size, r - l);
        for line in &lines {
            let baseline = self.y + size * 0.85;
            self.draw_words(line, l, baseline, size);
            self.y += leading;
        }
        if level == 1 {
            let rule_y = self.y - leading * 0.35;
            self.ops.push(Op::Rule {
                x1: l,
                y1: rule_y,
                x2: r,
                y2: rule_y,
                ink: Ink::BorderMuted,
                w: 0.75,
            });
        }
        self.y += size * 0.3;
    }

    fn paragraph(&mut self, inlines: &[Inline], l: f64, r: f64, quote: bool) {
        let size = f64::from(self.scale.body);
        let ink = Self::default_ink(quote);
        let mut pieces = Vec::new();
        self.flatten(
            inlines,
            RStyle {
                ink,
                ..RStyle::BODY
            },
            &mut pieces,
        );
        self.text_lines(&pieces, size, l, r, quote, size * 0.7);
    }

    /// Fenced code and math blocks: mono face on a themed panel.
    fn code_panel(&mut self, code: &str, l: f64, r: f64) {
        let size = f64::from(self.scale.code);
        let leading = size * 1.45;
        let pad = 8.0;
        let inset = 12.0;
        let lines: Vec<&str> = code.lines().collect();
        let h = lines.len() as f64 * leading + 2.0 * pad;
        self.ops.push(Op::Rect {
            x: l,
            y: self.y,
            w: r - l,
            h,
            fill: Ink::CodeBg,
            stroke: Some(Ink::BorderMuted),
        });
        let st = RStyle {
            mono: true,
            ..RStyle::BODY
        };
        let mut ly = self.y + pad;
        for line in lines {
            let baseline = ly + size * 0.8;
            let expanded = line.replace('\t', "    ");
            self.draw_text(l + inset, baseline, &expanded, st, size);
            ly += leading;
        }
        self.y += h + f64::from(self.scale.body) * 0.7;
    }

    fn blockquote(&mut self, inner: &[Block], l: f64, r: f64) {
        let start = self.y;
        let inner_l = l + 14.0;
        for block in inner {
            self.block(block, inner_l, r, true);
        }
        let h = (self.y - start).max(f64::from(self.scale.body));
        self.ops.push(Op::Rect {
            x: l,
            y: start,
            w: 3.0,
            h,
            fill: Ink::QuoteBar,
            stroke: None,
        });
        self.y += f64::from(self.scale.body) * 0.3;
    }

    fn list(&mut self, list: &List, l: f64, r: f64, quote: bool) {
        let size = f64::from(self.scale.body);
        let marker_w = 18.0;
        // Plex/CM both map U+2022; fall back to '-' if a face ever does not.
        let bullet = self.faces[SLOT_BODY].as_ref().map_or("-", |f| {
            if f.glyph_index('•') != 0 {
                "•"
            } else {
                "-"
            }
        });
        let ink = Self::default_ink(quote);
        for (i, item) in list.items.iter().enumerate() {
            let marker = if let Some(checked) = item.task {
                if checked {
                    "[x]".to_string()
                } else {
                    "[ ]".to_string()
                }
            } else if list.ordered {
                format!("{}.", list.start + i as u64)
            } else {
                bullet.to_string()
            };
            let mark_top = self.y;
            for block in &item.blocks {
                self.block(block, l + marker_w, r, quote);
            }
            let baseline = mark_top + size * 0.85;
            self.draw_text(
                l,
                baseline,
                &marker,
                RStyle {
                    ink,
                    ..RStyle::BODY
                },
                size,
            );
            if !list.tight {
                self.y += 4.0;
            }
        }
        self.y += size * 0.3;
    }

    fn table(&mut self, table: &Table, l: f64, r: f64) {
        let ncols = table
            .align
            .len()
            .max(table.head.len())
            .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
        if ncols == 0 {
            return;
        }
        let size = f64::from(self.scale.table);
        let leading = size * 1.35;
        let pad_x = 6.0;
        let pad_y = 4.0;
        let avail = r - l;

        // Column widths: natural single-line width (capped), scaled down
        // proportionally when the table would overflow the measure.
        let mut natural = vec![size; ncols];
        let consider = |col: usize, cells: &[Inline], bold: bool, natural: &mut Vec<f64>| {
            let mut pieces = Vec::new();
            self.flatten(
                cells,
                RStyle {
                    bold,
                    ..RStyle::BODY
                },
                &mut pieces,
            );
            let text: String = pieces
                .iter()
                .filter_map(|p| match p {
                    Piece::Text(t, _) => Some(t.as_str()),
                    Piece::Break => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            let w = self.measure(
                &text,
                RStyle {
                    bold,
                    ..RStyle::BODY
                },
                size,
            ) + 2.0 * pad_x;
            natural[col] = natural[col].max(w.min(avail * 0.6));
        };
        for (c, cell) in table.head.iter().enumerate().take(ncols) {
            consider(c, cell, true, &mut natural);
        }
        for row in &table.rows {
            for (c, cell) in row.iter().enumerate().take(ncols) {
                consider(c, cell, false, &mut natural);
            }
        }
        let total: f64 = natural.iter().sum();
        let widths: Vec<f64> = if total > avail {
            natural.iter().map(|w| w * avail / total).collect()
        } else {
            natural
        };

        let aligns: Vec<Align> = (0..ncols)
            .map(|c| table.align.get(c).copied().unwrap_or(Align::None))
            .collect();

        let draw_row = |poster: &mut Self, cells: &[Vec<Inline>], bold: bool, top: f64| -> f64 {
            let st = RStyle {
                bold,
                ..RStyle::BODY
            };
            let wrapped: Vec<Vec<Vec<Word>>> = (0..ncols)
                .map(|c| {
                    let mut pieces = Vec::new();
                    if let Some(cell) = cells.get(c) {
                        poster.flatten(cell, st, &mut pieces);
                    }
                    poster.wrap(&pieces, size, (widths[c] - 2.0 * pad_x).max(size))
                })
                .collect();
            let max_lines = wrapped.iter().map(Vec::len).max().unwrap_or(0).max(1);
            let row_h = max_lines as f64 * leading + 2.0 * pad_y;
            let mut cx = l;
            for (c, cell_lines) in wrapped.iter().enumerate() {
                let cell_w = widths[c] - 2.0 * pad_x;
                for (li, line) in cell_lines.iter().enumerate() {
                    let line_w = poster.words_width(line, size);
                    let x = match aligns[c] {
                        Align::Center => cx + pad_x + (cell_w - line_w) / 2.0,
                        Align::Right => cx + pad_x + (cell_w - line_w),
                        Align::None | Align::Left => cx + pad_x,
                    };
                    let baseline = top + pad_y + li as f64 * leading + size * 0.8;
                    poster.draw_words(line, x, baseline, size);
                }
                cx += widths[c];
            }
            row_h
        };

        let table_top = self.y;
        // Header row on a subtle stripe.
        let header_h = draw_row(self, &table.head, true, table_top);
        self.ops.push(Op::Rect {
            x: l,
            y: table_top,
            w: avail,
            h: header_h,
            fill: Ink::BgSubtle,
            stroke: None,
        });
        // Repaint header text above the stripe.
        let header_h2 = draw_row(self, &table.head, true, table_top);
        debug_assert_eq!(header_h, header_h2);
        let mut row_top = table_top + header_h;
        let mut bottoms = vec![row_top];
        for row in &table.rows {
            let h = draw_row(self, row, false, row_top);
            row_top += h;
            bottoms.push(row_top);
        }
        let table_bottom = row_top;

        // Grid: horizontal separators, column separators, outer border.
        for &by in &bottoms {
            self.ops.push(Op::Rule {
                x1: l,
                y1: by,
                x2: r,
                y2: by,
                ink: Ink::BorderMuted,
                w: 0.5,
            });
        }
        let mut cx = l;
        for w in widths.iter().take(ncols.saturating_sub(1)) {
            cx += w;
            self.ops.push(Op::Rule {
                x1: cx,
                y1: table_top,
                x2: cx,
                y2: table_bottom,
                ink: Ink::BorderMuted,
                w: 0.5,
            });
        }
        self.ops.push(Op::Rule {
            x1: l,
            y1: table_top,
            x2: r,
            y2: table_top,
            ink: Ink::Border,
            w: 0.75,
        });
        self.ops.push(Op::Rule {
            x1: l,
            y1: table_bottom,
            x2: r,
            y2: table_bottom,
            ink: Ink::Border,
            w: 0.75,
        });
        self.y = table_bottom + f64::from(self.scale.body) * 0.7;
    }

    // -- emission ------------------------------------------------------------

    /// Compile one glyph's contours to SVG path data in the normalized
    /// 100 pt em (y flipped). `None` for blank glyphs (space) or outlines
    /// that fail to decode.
    fn glyph_path(font: &Font, gid: u16) -> Option<String> {
        let outline = font.glyph_outline(gid).ok()?;
        if outline.contours.is_empty() {
            return None;
        }
        let k = DEF_EM_PT / f64::from(font.units_per_em);
        let mut d = String::new();
        for contour in &outline.contours {
            d.push('M');
            d.push_str(&q2(contour.start.x * k));
            d.push(' ');
            d.push_str(&q2(-contour.start.y * k));
            for seg in &contour.segments {
                match seg {
                    Segment::Line { to } => {
                        d.push('L');
                        d.push_str(&q2(to.x * k));
                        d.push(' ');
                        d.push_str(&q2(-to.y * k));
                    }
                    Segment::Quad { ctrl, to } => {
                        d.push('Q');
                        d.push_str(&q2(ctrl.x * k));
                        d.push(' ');
                        d.push_str(&q2(-ctrl.y * k));
                        d.push(' ');
                        d.push_str(&q2(to.x * k));
                        d.push(' ');
                        d.push_str(&q2(-to.y * k));
                    }
                }
            }
            d.push('Z');
        }
        Some(d)
    }

    fn emit(self, height: f64) -> (Vec<u8>, SvgReport) {
        // Collect unique glyph defs in sorted (slot, gid) order: BTreeMap
        // iteration keeps def ids stable for a fixed input.
        let mut defs: BTreeMap<(usize, u16), Option<String>> = BTreeMap::new();
        for op in &self.ops {
            if let Op::Glyph { slot, gid, .. } = op {
                defs.entry((*slot, *gid)).or_insert_with(|| {
                    self.faces[*slot]
                        .as_ref()
                        .and_then(|font| Self::glyph_path(font, *gid))
                });
            }
        }
        let ids: BTreeMap<(usize, u16), usize> = defs
            .iter()
            .filter(|(_, path)| path.is_some())
            .enumerate()
            .map(|(i, (key, _))| (*key, i))
            .collect();

        let mut out = String::with_capacity(64 * 1024);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"");
        out.push_str(&q2(self.width));
        out.push_str("pt\" height=\"");
        out.push_str(&q2(height));
        out.push_str("pt\" viewBox=\"0 0 ");
        out.push_str(&q2(self.width));
        out.push(' ');
        out.push_str(&q2(height));
        out.push_str("\">\n");

        // Page background.
        out.push_str("<rect width=\"");
        out.push_str(&q2(self.width));
        out.push_str("\" height=\"");
        out.push_str(&q2(height));
        out.push_str("\" fill=\"");
        esc_attr(&self.colors.bg, &mut out);
        out.push_str("\"/>\n");

        out.push_str("<defs>\n");
        for (key, path) in &defs {
            if let (Some(path), Some(id)) = (path, ids.get(key)) {
                out.push_str("<path id=\"g");
                out.push_str(&id.to_string());
                out.push_str("\" d=\"");
                out.push_str(path);
                out.push_str("\"/>\n");
            }
        }
        out.push_str("</defs>\n");

        let mut drawn = 0;
        for op in &self.ops {
            match op {
                Op::Rect {
                    x,
                    y,
                    w,
                    h,
                    fill,
                    stroke,
                } => {
                    out.push_str("<rect x=\"");
                    out.push_str(&q2(*x));
                    out.push_str("\" y=\"");
                    out.push_str(&q2(*y));
                    out.push_str("\" width=\"");
                    out.push_str(&q2(*w));
                    out.push_str("\" height=\"");
                    out.push_str(&q2(*h));
                    out.push_str("\" fill=\"");
                    esc_attr(fill.hex(&self.colors), &mut out);
                    out.push('"');
                    if let Some(stroke) = stroke {
                        out.push_str(" stroke=\"");
                        esc_attr(stroke.hex(&self.colors), &mut out);
                        out.push_str("\" stroke-width=\"0.5\"");
                    }
                    out.push_str("/>\n");
                }
                Op::Rule {
                    x1,
                    y1,
                    x2,
                    y2,
                    ink,
                    w,
                } => {
                    out.push_str("<line x1=\"");
                    out.push_str(&q2(*x1));
                    out.push_str("\" y1=\"");
                    out.push_str(&q2(*y1));
                    out.push_str("\" x2=\"");
                    out.push_str(&q2(*x2));
                    out.push_str("\" y2=\"");
                    out.push_str(&q2(*y2));
                    out.push_str("\" stroke=\"");
                    esc_attr(ink.hex(&self.colors), &mut out);
                    out.push_str("\" stroke-width=\"");
                    out.push_str(&q2(*w));
                    out.push_str("\"/>\n");
                }
                Op::Glyph {
                    slot,
                    gid,
                    x,
                    y,
                    size,
                    ink,
                } => {
                    let key = (*slot, *gid);
                    let Some(id) = ids.get(&key) else {
                        continue; // outline failed to decode; no def to reference
                    };
                    out.push_str("<use href=\"#g");
                    out.push_str(&id.to_string());
                    out.push_str("\" transform=\"translate(");
                    out.push_str(&q2(*x));
                    out.push(' ');
                    out.push_str(&q2(*y));
                    out.push_str(") scale(");
                    out.push_str(&trim4(*size / DEF_EM_PT));
                    out.push_str(")\" fill=\"");
                    esc_attr(ink.hex(&self.colors), &mut out);
                    out.push_str("\"/>\n");
                    drawn += 1;
                }
            }
        }
        out.push_str("</svg>\n");

        let report = SvgReport {
            glyphs_missing: self.missing,
            glyphs_drawn: drawn,
            paths_emitted: ids.len(),
        };
        (out.into_bytes(), report)
    }
}

/// `FontStyle` → body face slot offset.
fn style_index(style: FontStyle) -> usize {
    match style {
        FontStyle::Regular => 0,
        FontStyle::Bold => 1,
        FontStyle::Italic => 2,
        FontStyle::BoldItalic => 3,
    }
}
