//! Layout engine primitives for the PDF renderer.
//!
//! This module is intentionally small today, but it is no longer just a roadmap
//! note. It owns the deterministic measurement units that the TeX-style
//! paragraph/page builders will use. The PDF writer may serialize final
//! positions as decimal points, but layout decisions should be made with these
//! fixed-point integer units so line breaks do not depend on platform-specific
//! floating point behavior.
//!
//! Roadmap built on these primitives:
//!
//! * **Box / glue / penalty model** — the TeX paragraph representation that
//!   makes high-quality breaking possible.
//! * **Knuth-Plass optimal line breaking** — total-fit minimization of demerits
//!   over the whole paragraph (not greedy), giving even spacing and few
//!   hyphens, with badness/penalty tuning per block type.
//! * **Hyphenation** — Liang's algorithm with TeX hyphenation patterns compiled
//!   to compact deterministic tables.
//! * **Leading and page assembly** — vertical boxes/glue/penalties, widow/orphan
//!   control, keep-with-next headings, and table/code-block breaking.
//! * **Microtypography** — optional punctuation protrusion and tiny font
//!   expansion hooks once the baseline layout is proven.

use crate::ast::Inline;
use crate::line_break::{line_break_class, line_break_opportunities};
use crate::text::Font;
use std::sync::OnceLock;

/// Number of fixed layout units in one PDF point.
///
/// PDF uses points (1/72 inch). `franken_markdown` layout uses milli-points:
/// `1 pt == 1000 LayoutUnit`s. That is small enough for high-quality text
/// fitting, large enough for normal documents to avoid overflow, and fully
/// deterministic across native and WASM targets.
pub const UNITS_PER_POINT: i32 = 1000;

/// A deterministic layout distance stored in milli-points.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayoutUnit(i32);

impl LayoutUnit {
    /// Zero distance.
    pub const ZERO: Self = Self(0);

    /// Construct from raw milli-points.
    #[must_use]
    pub const fn from_milli_points(value: i32) -> Self {
        Self(value)
    }

    /// Construct from whole PDF points.
    #[must_use]
    pub const fn from_points(points: i32) -> Self {
        Self(points.saturating_mul(UNITS_PER_POINT))
    }

    /// Raw milli-point value.
    #[must_use]
    pub const fn milli_points(self) -> i32 {
        self.0
    }

    /// Whole/fractional PDF points as `f32`.
    ///
    /// This is for final output serialization only; layout decisions should use
    /// integer comparisons on [`Self::milli_points`].
    #[must_use]
    pub fn to_points_f32(self) -> f32 {
        self.0 as f32 / UNITS_PER_POINT as f32
    }

    /// Saturating addition.
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Saturating subtraction.
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl core::ops::Add for LayoutUnit {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.saturating_add(rhs)
    }
}

impl core::ops::AddAssign for LayoutUnit {
    fn add_assign(&mut self, rhs: Self) {
        *self = self.saturating_add(rhs);
    }
}

impl core::ops::Sub for LayoutUnit {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        self.saturating_sub(rhs)
    }
}

/// Font size stored in milli-points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontSize {
    milli_points: u32,
}

impl FontSize {
    /// Construct from whole PDF points.
    #[must_use]
    pub const fn from_points(points: u16) -> Self {
        Self {
            milli_points: (points as u32) * (UNITS_PER_POINT as u32),
        }
    }

    /// Construct from milli-points, e.g. `9500` for `9.5pt`.
    #[must_use]
    pub const fn from_milli_points(milli_points: u32) -> Self {
        Self { milli_points }
    }

    /// Raw milli-point value.
    #[must_use]
    pub const fn milli_points(self) -> u32 {
        self.milli_points
    }
}

/// Something that can report glyph advances in PDF text-space units
/// (`1000 == 1em`).
pub trait AdvanceMetrics {
    /// Return the advance width of `ch` in 1/1000 em units.
    fn advance_1000(&self, ch: char) -> u32;
}

impl AdvanceMetrics for Font {
    fn advance_1000(&self, ch: char) -> u32 {
        Font::advance_1000(self, ch)
    }
}

/// Optional pair-positioning metrics in 1/1000 em units.
pub trait PairMetrics: AdvanceMetrics {
    /// Return the kerning / pair-position adjustment between adjacent chars.
    fn kerning_1000(&self, _left: char, _right: char) -> i32 {
        0
    }
}

impl PairMetrics for Font {
    fn kerning_1000(&self, left: char, right: char) -> i32 {
        Font::kerning_1000(self, left, right)
    }
}

/// Convert one 1/1000-em advance to a deterministic layout distance.
#[must_use]
pub fn advance_to_layout_units(advance_1000: u32, size: FontSize) -> LayoutUnit {
    // width_pt = advance_1000 / 1000 * font_size_pt
    // width_mpt = advance_1000 * font_size_mpt / 1000
    let width = (advance_1000 as u128 * size.milli_points() as u128) / 1000;
    LayoutUnit(clamp_u128_to_i32(width))
}

/// Convert a signed 1/1000-em pair adjustment to layout units.
#[must_use]
pub fn adjustment_to_layout_units(adjustment_1000: i32, size: FontSize) -> LayoutUnit {
    let width = (adjustment_1000 as i128 * size.milli_points() as i128) / 1000;
    LayoutUnit(clamp_i128_to_i32(width))
}

/// Measure text by summing per-character advances in deterministic order.
#[must_use]
pub fn measure_text<M: AdvanceMetrics>(metrics: &M, text: &str, size: FontSize) -> LayoutUnit {
    let mut total = LayoutUnit::ZERO;
    for ch in text.chars() {
        total += advance_to_layout_units(metrics.advance_1000(ch), size);
    }
    total
}

/// Measure text with deterministic pair kerning / positioning.
#[must_use]
pub fn measure_text_with_pairs<M: PairMetrics>(
    metrics: &M,
    text: &str,
    size: FontSize,
) -> LayoutUnit {
    let mut total = LayoutUnit::ZERO;
    let mut prev: Option<char> = None;
    for ch in text.chars() {
        if let Some(left) = prev {
            total += adjustment_to_layout_units(metrics.kerning_1000(left, ch), size);
        }
        total += advance_to_layout_units(metrics.advance_1000(ch), size);
        prev = Some(ch);
    }
    total
}

/// Measure text from already-shaped glyph/text advances.
///
/// This exists because future GSUB/GPOS shaping may turn a source substring into
/// a single glyph (ligature) or attach positioning adjustments. The line breaker
/// should not care whether widths came from raw characters or shaped glyph runs.
#[must_use]
pub fn measure_advances<I>(advances_1000: I, size: FontSize) -> LayoutUnit
where
    I: IntoIterator<Item = u32>,
{
    let mut total = LayoutUnit::ZERO;
    for advance in advances_1000 {
        total += advance_to_layout_units(advance, size);
    }
    total
}

/// A very large bad break penalty. TeX conventionally treats `10000` as
/// effectively infinite.
pub const INF_PENALTY: i32 = 10_000;

/// A forced break penalty.
pub const FORCED_BREAK_PENALTY: i32 = -INF_PENALTY;

/// A TeX-style paragraph item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParagraphItem {
    /// Unbreakable shaped text/content.
    Box(TextBox),
    /// Flexible spacing.
    Glue(Glue),
    /// Candidate, discouraged, prohibited, or forced breakpoint.
    Penalty(Penalty),
}

impl ParagraphItem {
    /// Natural item width.
    #[must_use]
    pub const fn width(&self) -> LayoutUnit {
        match self {
            Self::Box(item) => item.width,
            Self::Glue(item) => item.width,
            Self::Penalty(item) => item.width,
        }
    }
}

/// Unbreakable paragraph content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBox {
    /// Plain fallback text for extraction, diagnostics, and simple renderers.
    pub text: String,
    /// Styled text runs carried through to the PDF line/page builders.
    pub runs: StyledText,
    pub width: LayoutUnit,
}

/// Inline text style metadata preserved for PDF layout.
///
/// This is intentionally a compact value type rather than a general CSS model.
/// Markdown only needs a small set of semantic text roles; the PDF builder can
/// map these roles to bundled faces, colors, annotations, and decoration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strikethrough: bool,
    pub link: bool,
}

impl TextStyle {
    /// Unstyled body text.
    pub const BODY: Self = Self {
        bold: false,
        italic: false,
        code: false,
        strikethrough: false,
        link: false,
    };

    #[must_use]
    pub const fn with_bold(self) -> Self {
        Self { bold: true, ..self }
    }

    #[must_use]
    pub const fn with_italic(self) -> Self {
        Self {
            italic: true,
            ..self
        }
    }

    #[must_use]
    pub const fn with_code(self) -> Self {
        Self { code: true, ..self }
    }

    #[must_use]
    pub const fn with_strikethrough(self) -> Self {
        Self {
            strikethrough: true,
            ..self
        }
    }

    #[must_use]
    pub const fn with_link(self) -> Self {
        Self { link: true, ..self }
    }
}

/// A contiguous text segment with one semantic style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledRun {
    pub text: String,
    pub style: TextStyle,
}

/// Markdown inline text after semantic styling has been preserved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyledText {
    pub runs: Vec<StyledRun>,
}

impl StyledText {
    /// Construct unstyled text.
    #[must_use]
    pub fn plain(text: &str) -> Self {
        let mut out = Self::default();
        out.push_text(text, TextStyle::BODY);
        out
    }

    /// Convert Markdown inlines into styled runs.
    #[must_use]
    pub fn from_inlines(inlines: &[Inline]) -> Self {
        let mut out = Self::default();
        push_inline_runs(&mut out, inlines, TextStyle::BODY);
        out
    }

    /// True if there are no non-empty runs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Append text with style, coalescing adjacent equal-style runs.
    pub fn push_text(&mut self, text: &str, style: TextStyle) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.runs.last_mut() {
            if last.style == style {
                last.text.push_str(text);
                return;
            }
        }
        self.runs.push(StyledRun {
            text: text.to_string(),
            style,
        });
    }

    /// Plain-text projection for fallback renderers and copy/search behavior.
    #[must_use]
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for run in &self.runs {
            out.push_str(&run.text);
        }
        out
    }
}

fn push_inline_runs(out: &mut StyledText, inlines: &[Inline], style: TextStyle) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_text(text, style),
            Inline::Emphasis(content) => push_inline_runs(out, content, style.with_italic()),
            Inline::Strong(content) => push_inline_runs(out, content, style.with_bold()),
            Inline::Strikethrough(content) => {
                push_inline_runs(out, content, style.with_strikethrough());
            }
            Inline::Code(text) => out.push_text(text, style.with_code()),
            Inline::Link { content, .. } => push_inline_runs(out, content, style.with_link()),
            Inline::Image { alt, .. } => out.push_text(alt, style),
            Inline::SoftBreak | Inline::HardBreak => out.push_text(" ", style),
            Inline::Html(html) => out.push_text(html, style),
        }
    }
}

/// Flexible space with natural width and stretch/shrink budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glue {
    pub width: LayoutUnit,
    pub stretch: LayoutUnit,
    pub shrink: LayoutUnit,
}

/// Breakpoint cost metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Penalty {
    /// Extra width if this breakpoint is chosen, e.g. a hyphen glyph.
    pub width: LayoutUnit,
    /// Penalty value: positive discourages, negative encourages, `-10000`
    /// forces, `+10000` prohibits.
    pub penalty: i32,
    /// True for discretionary hyphen-style breakpoints so the line breaker can
    /// penalize consecutive flagged breaks.
    pub flagged: bool,
}

/// Convert plain text into a basic box/glue/forced-break paragraph.
///
/// This is the minimal constructor needed for the first Knuth-Plass
/// implementation. Later styled run and hyphenation constructors should produce
/// the same [`ParagraphItem`] stream.
#[must_use]
pub fn paragraph_items_from_text<M: PairMetrics>(
    metrics: &M,
    text: &str,
    size: FontSize,
) -> Vec<ParagraphItem> {
    paragraph_items_from_styled_text(metrics, &StyledText::plain(text), size)
}

/// Convert Markdown inlines into styled paragraph items.
#[must_use]
pub fn paragraph_items_from_inlines<M: PairMetrics>(
    metrics: &M,
    inlines: &[Inline],
    size: FontSize,
) -> Vec<ParagraphItem> {
    paragraph_items_from_styled_text(metrics, &StyledText::from_inlines(inlines), size)
}

/// Convert styled text into a box/glue/forced-break paragraph.
#[must_use]
pub fn paragraph_items_from_styled_text<M: PairMetrics>(
    metrics: &M,
    text: &StyledText,
    size: FontSize,
) -> Vec<ParagraphItem> {
    let mut items = Vec::new();
    let space = measure_text_with_pairs(metrics, " ", size);
    let interword = default_interword_glue(space);
    let mut current = StyledText::default();
    let mut current_plain = String::new();
    let mut current_width = LayoutUnit::ZERO;
    let mut pending_interword = false;

    for run in &text.runs {
        let mut chunk_start = None;
        for (idx, ch) in run.text.char_indices() {
            if is_breakable_whitespace(ch) {
                if let Some(start) = chunk_start.take() {
                    append_styled_word_chunk(
                        metrics,
                        &mut current,
                        &mut current_plain,
                        &mut current_width,
                        &run.text[start..idx],
                        run.style,
                        size,
                    );
                }
                if !current.is_empty() {
                    push_styled_word_box(
                        &mut items,
                        &mut current,
                        &mut current_plain,
                        &mut current_width,
                    );
                    pending_interword = true;
                }
            } else {
                if chunk_start.is_none() {
                    if current.is_empty() && pending_interword {
                        items.push(ParagraphItem::Glue(interword));
                        pending_interword = false;
                    }
                    chunk_start = Some(idx);
                }
            }
        }
        if let Some(start) = chunk_start {
            append_styled_word_chunk(
                metrics,
                &mut current,
                &mut current_plain,
                &mut current_width,
                &run.text[start..],
                run.style,
                size,
            );
        }
    }
    if !current.is_empty() {
        push_styled_word_box(
            &mut items,
            &mut current,
            &mut current_plain,
            &mut current_width,
        );
    }
    items.push(ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: FORCED_BREAK_PENALTY,
        flagged: false,
    }));
    items
}

fn append_styled_word_chunk<M: PairMetrics>(
    metrics: &M,
    current: &mut StyledText,
    current_plain: &mut String,
    current_width: &mut LayoutUnit,
    chunk: &str,
    style: TextStyle,
    size: FontSize,
) {
    if chunk.is_empty() {
        return;
    }
    if let Some((left, right)) = current
        .runs
        .last()
        .filter(|last| last.style == style)
        .and_then(|last| last.text.chars().next_back().zip(chunk.chars().next()))
    {
        *current_width += adjustment_to_layout_units(metrics.kerning_1000(left, right), size);
    }
    *current_width += measure_text_with_pairs(metrics, chunk, size);
    current.push_text(chunk, style);
    current_plain.push_str(chunk);
}

fn push_styled_word_box(
    items: &mut Vec<ParagraphItem>,
    current: &mut StyledText,
    current_plain: &mut String,
    current_width: &mut LayoutUnit,
) {
    items.push(ParagraphItem::Box(TextBox {
        text: std::mem::take(current_plain),
        runs: std::mem::take(current),
        width: *current_width,
    }));
    *current_width = LayoutUnit::ZERO;
}

/// Measure styled text while preserving each run boundary.
///
/// The first implementation uses the same metrics for every style. That is
/// intentional: this layer preserves style semantics without forcing the font
/// subsystem into the core line-breaker API yet. The PDF builder can later map
/// bold/italic/code/link runs to face-specific shaped advances and still feed
/// the resulting boxes into the same paragraph optimizer.
#[must_use]
pub fn measure_styled_text<M: PairMetrics>(
    metrics: &M,
    text: &StyledText,
    size: FontSize,
) -> LayoutUnit {
    let mut total = LayoutUnit::ZERO;
    for run in &text.runs {
        total += measure_text_with_pairs(metrics, &run.text, size);
    }
    total
}

/// Convert plain text into paragraph items with discretionary hyphen penalties.
#[must_use]
pub fn hyphenated_paragraph_items_from_text<M: PairMetrics>(
    metrics: &M,
    hyphenator: &Hyphenator,
    text: &str,
    size: FontSize,
) -> Vec<ParagraphItem> {
    let mut items = Vec::new();
    let mut scratch = ParagraphLayoutScratch::new();
    hyphenated_paragraph_items_from_text_into(
        metrics,
        hyphenator,
        text,
        size,
        &mut scratch,
        &mut items,
    );
    items
}

/// Convert plain text into paragraph items with discretionary hyphen penalties,
/// reusing caller-owned buffers.
///
/// `out` is cleared before use. The scratch workspace is shared with
/// [`break_paragraph_into`] so renderers can reuse one allocation set for item
/// construction and line breaking across all paragraphs in a render call.
pub fn hyphenated_paragraph_items_from_text_into<M: PairMetrics>(
    metrics: &M,
    hyphenator: &Hyphenator,
    text: &str,
    size: FontSize,
    scratch: &mut ParagraphLayoutScratch,
    out: &mut Vec<ParagraphItem>,
) {
    out.clear();
    scratch.hyphen_lower.clear();
    scratch.hyphen_dotted.clear();
    scratch.hyphen_scores.clear();
    scratch.hyphen_points.clear();
    let mut words = breakable_words(text).peekable();
    let space = measure_text_with_pairs(metrics, " ", size);
    let hyphen_width = measure_text_with_pairs(metrics, "-", size);
    while let Some(word) = words.next() {
        if word_contains_cjk(word) {
            push_cjk_word_items(out, metrics, word, size);
        } else {
            hyphenator.hyphenation_points_into_scratch(
                word,
                HyphenationOptions::default(),
                &mut scratch.hyphen_points,
                &mut scratch.hyphen_lower,
                &mut scratch.hyphen_dotted,
                &mut scratch.hyphen_scores,
            );
            push_hyphenated_word_items_from_points(
                out,
                metrics,
                word,
                size,
                hyphen_width,
                &scratch.hyphen_points,
            );
        }
        if words.peek().is_some() {
            out.push(ParagraphItem::Glue(default_interword_glue(space)));
        }
    }
    out.push(ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: FORCED_BREAK_PENALTY,
        flagged: false,
    }));
}

fn push_hyphenated_word_items_from_points<M: PairMetrics>(
    out: &mut Vec<ParagraphItem>,
    metrics: &M,
    word: &str,
    size: FontSize,
    hyphen_width: LayoutUnit,
    points: &[usize],
) {
    if points.is_empty() {
        out.push(ParagraphItem::Box(TextBox {
            text: word.to_string(),
            runs: StyledText::plain(word),
            width: measure_text_with_pairs(metrics, word, size),
        }));
        return;
    }

    let mut start = 0usize;
    for &point in points {
        // Hyphenation points are emitted only for ASCII alphabetic words. For
        // those words, the character offset is also the byte offset; non-ASCII
        // words return no points before this function is called.
        let end = point.min(word.len());
        if end > start {
            let part = &word[start..end];
            out.push(ParagraphItem::Box(TextBox {
                text: part.to_string(),
                runs: StyledText::plain(part),
                width: measure_text_with_pairs(metrics, part, size),
            }));
            out.push(ParagraphItem::Penalty(Penalty {
                width: hyphen_width,
                penalty: 50,
                flagged: true,
            }));
        }
        start = end;
    }
    if start < word.len() {
        let part = &word[start..];
        out.push(ParagraphItem::Box(TextBox {
            text: part.to_string(),
            runs: StyledText::plain(part),
            width: measure_text_with_pairs(metrics, part, size),
        }));
    }
}

fn push_cjk_word_items<M: PairMetrics>(
    out: &mut Vec<ParagraphItem>,
    metrics: &M,
    word: &str,
    size: FontSize,
) {
    let break_points: Vec<usize> = line_break_opportunities(word).collect();
    if break_points.is_empty() {
        out.push(ParagraphItem::Box(TextBox {
            text: word.to_string(),
            runs: StyledText::plain(word),
            width: measure_text_with_pairs(metrics, word, size),
        }));
        return;
    }

    let mut start = 0usize;
    for point in break_points {
        if point <= start {
            continue;
        }
        let part = &word[start..point];
        if part.is_empty() {
            continue;
        }
        out.push(ParagraphItem::Box(TextBox {
            text: part.to_string(),
            runs: StyledText::plain(part),
            width: measure_text_with_pairs(metrics, part, size),
        }));
        out.push(ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: 50,
            flagged: false,
        }));
        start = point;
    }
    if start < word.len() {
        let part = &word[start..];
        out.push(ParagraphItem::Box(TextBox {
            text: part.to_string(),
            runs: StyledText::plain(part),
            width: measure_text_with_pairs(metrics, part, size),
        }));
    }
}

/// True for whitespace where normal Markdown/PDF text layout may break a line.
///
/// Unicode no-break spaces are intentionally treated as word characters. They
/// should stay selectable as their original scalar and must not become ordinary
/// breakable spaces during PDF layout.
#[must_use]
pub(crate) fn is_breakable_whitespace(ch: char) -> bool {
    ch.is_whitespace() && !matches!(ch, '\u{00A0}' | '\u{2007}' | '\u{202F}')
}

fn breakable_words(text: &str) -> BreakableWords<'_> {
    BreakableWords { text, pos: 0 }
}

struct BreakableWords<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Iterator for BreakableWords<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let len = self.text.len();
        while self.pos < len {
            let ch = self.text.get(self.pos..)?.chars().next()?;
            if !is_breakable_whitespace(ch) {
                break;
            }
            self.pos += ch.len_utf8();
        }
        if self.pos >= len {
            return None;
        }
        let start = self.pos;
        while self.pos < len {
            let ch = self.text.get(self.pos..)?.chars().next()?;
            if is_breakable_whitespace(ch) {
                break;
            }
            self.pos += ch.len_utf8();
        }
        self.text.get(start..self.pos)
    }
}

fn word_contains_cjk(word: &str) -> bool {
    word.chars().any(|c| line_break_class(c).is_cjk())
}

/// Default TeX-like interword glue for the first paragraph builder.
#[must_use]
pub fn default_interword_glue(space: LayoutUnit) -> Glue {
    Glue {
        width: space,
        stretch: LayoutUnit::from_milli_points(space.milli_points() / 2),
        shrink: LayoutUnit::from_milli_points(space.milli_points() / 3),
    }
}

/// Hyphenation controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HyphenationOptions {
    /// Minimum characters before the first hyphen.
    pub min_left: usize,
    /// Minimum characters after the last hyphen.
    pub min_right: usize,
}

impl Default for HyphenationOptions {
    fn default() -> Self {
        Self {
            min_left: 2,
            min_right: 3,
        }
    }
}

/// A compiled Liang hyphenation pattern.
#[derive(Debug, Clone, Copy)]
pub struct HyphenPattern {
    letters: &'static str,
    values: &'static [u8],
}

/// A deterministic exception entry. Break positions are character offsets.
#[derive(Debug, Clone, Copy)]
pub struct HyphenException {
    word: &'static str,
    points: &'static [usize],
}

/// Dependency-free Liang-style hyphenator.
#[derive(Debug, Clone, Copy)]
pub struct Hyphenator {
    encoded_patterns: &'static str,
    exceptions: &'static [HyphenException],
}

impl Hyphenator {
    /// English hyphenator. It uses the full TeX `hyph-en-us` pattern set (4938
    /// `\patterns` tokens, embedded from `data/hyph-en-us.patterns`) plus
    /// high-value exceptions for documentation-heavy words.
    #[must_use]
    pub const fn english() -> Self {
        Self {
            encoded_patterns: EN_US_TEX_PATTERNS,
            exceptions: ENGLISH_EXCEPTIONS,
        }
    }

    /// Number of encoded TeX pattern tokens in this hyphenator.
    #[must_use]
    pub fn encoded_pattern_count(&self) -> usize {
        self.encoded_patterns.split_ascii_whitespace().count()
    }

    /// Return legal hyphenation points as character offsets in `word`.
    #[must_use]
    pub fn hyphenation_points(&self, word: &str, opts: HyphenationOptions) -> Vec<usize> {
        if word.len() > opts.min_left.saturating_add(opts.min_right) {
            if let Some(points) = english_exception_points(word) {
                #[cfg(debug_assertions)]
                debug_assert!(english_exception_table_matches_direct_lookup(
                    self.exceptions
                ));

                if opts == HyphenationOptions::default() {
                    return points.to_vec();
                }

                let len = word.len();
                let mut out = Vec::with_capacity(points.len());
                for &point in points {
                    if legal_hyphen_point(point, len, opts) {
                        out.push(point);
                    }
                }
                return out;
            }
        }

        let mut out = Vec::new();
        self.hyphenation_points_into(word, opts, &mut out);
        out
    }

    /// Write legal hyphenation points into a caller-owned buffer.
    ///
    /// `out` is cleared before use. This is the allocation-reuse variant used
    /// by render-call-local layout scratch workspaces.
    pub fn hyphenation_points_into(
        &self,
        word: &str,
        opts: HyphenationOptions,
        out: &mut Vec<usize>,
    ) {
        let mut lower = String::new();
        let mut dotted = Vec::new();
        let mut scores = Vec::new();
        self.hyphenation_points_into_scratch(word, opts, out, &mut lower, &mut dotted, &mut scores);
    }

    fn hyphenation_points_into_scratch(
        &self,
        word: &str,
        opts: HyphenationOptions,
        out: &mut Vec<usize>,
        lower: &mut String,
        dotted: &mut Vec<u8>,
        scores: &mut Vec<u8>,
    ) {
        out.clear();
        lower.clear();
        dotted.clear();
        scores.clear();
        if word.len() <= opts.min_left.saturating_add(opts.min_right) {
            return;
        }

        let mut has_uppercase = false;
        for &byte in word.as_bytes() {
            if !byte.is_ascii_alphabetic() {
                return;
            }
            has_uppercase |= byte.is_ascii_uppercase();
        }
        let normalized_word = if has_uppercase {
            lower.reserve(word.len());
            for byte in word.bytes() {
                lower.push(byte.to_ascii_lowercase() as char);
            }
            lower.as_str()
        } else {
            word
        };

        let len = normalized_word.len();
        if self.extend_exception_points(normalized_word, len, opts, out) {
            return;
        }

        dotted.reserve(len + 2);
        dotted.push(b'.');
        dotted.extend_from_slice(normalized_word.as_bytes());
        dotted.push(b'.');

        scores.resize(dotted.len() + 1, 0);
        english_hyphen_trie().apply(dotted, scores);
        extend_hyphen_points_from_scores(out, scores, len, opts);
    }

    fn extend_exception_points(
        &self,
        normalized_word: &str,
        len: usize,
        opts: HyphenationOptions,
        out: &mut Vec<usize>,
    ) -> bool {
        #[cfg(debug_assertions)]
        debug_assert!(english_exception_table_matches_direct_lookup(
            self.exceptions
        ));

        let Some(points) = english_exception_points(normalized_word) else {
            return false;
        };
        debug_assert!(
            self.exceptions
                .iter()
                .any(|exception| exception.word == normalized_word && exception.points == points)
        );
        out.extend(
            points
                .iter()
                .copied()
                .filter(|&p| legal_hyphen_point(p, len, opts)),
        );
        true
    }
}

#[cfg(debug_assertions)]
fn english_exception_table_matches_direct_lookup(exceptions: &[HyphenException]) -> bool {
    exceptions
        .iter()
        .all(|exception| english_exception_points(exception.word) == Some(exception.points))
}

fn english_exception_points(word: &str) -> Option<&'static [usize]> {
    match word {
        "configuration" => Some(&[3, 6, 7, 9]),
        "deterministic" => Some(&[2, 5, 8]),
        "documentation" => Some(&[3, 5, 8]),
        "hyphenation" => Some(&[2, 6]),
        "implementation" => Some(&[2, 5, 10]),
        "internationalization" => Some(&[2, 5, 7, 11, 13, 16]),
        "optimization" => Some(&[2, 4, 6, 8]),
        "pagination" => Some(&[3, 4, 6]),
        "representation" => Some(&[3, 5, 8, 10]),
        "serialization" => Some(&[2, 4, 6, 9]),
        "typography" => Some(&[2, 5, 7]),
        "visualization" => Some(&[2, 4, 6, 9]),
        _ => None,
    }
}

const EN_US_TEX_PATTERNS: &str = include_str!("../data/hyph-en-us.patterns");

const ENGLISH_EXCEPTIONS: &[HyphenException] = &[
    HyphenException {
        word: "hyphenation",
        points: &[2, 6],
    },
    HyphenException {
        word: "typography",
        points: &[2, 5, 7],
    },
    HyphenException {
        word: "optimization",
        points: &[2, 4, 6, 8],
    },
    HyphenException {
        word: "deterministic",
        points: &[2, 5, 8],
    },
    HyphenException {
        word: "documentation",
        points: &[3, 5, 8],
    },
    HyphenException {
        word: "implementation",
        points: &[2, 5, 10],
    },
    HyphenException {
        word: "pagination",
        points: &[3, 4, 6],
    },
    HyphenException {
        word: "representation",
        points: &[3, 5, 8, 10],
    },
    HyphenException {
        word: "serialization",
        points: &[2, 4, 6, 9],
    },
    HyphenException {
        word: "visualization",
        points: &[2, 4, 6, 9],
    },
    HyphenException {
        word: "configuration",
        points: &[3, 6, 7, 9],
    },
    HyphenException {
        word: "internationalization",
        points: &[2, 5, 7, 11, 13, 16],
    },
];

fn legal_hyphen_point(point: usize, len: usize, opts: HyphenationOptions) -> bool {
    point >= opts.min_left && len.saturating_sub(point) >= opts.min_right
}

fn extend_hyphen_points_from_scores(
    out: &mut Vec<usize>,
    scores: &[u8],
    len: usize,
    opts: HyphenationOptions,
) {
    out.extend(scores.iter().enumerate().filter_map(|(idx, &score)| {
        let point = idx.checked_sub(1)?;
        if score % 2 == 1 && legal_hyphen_point(point, len, opts) {
            Some(point)
        } else {
            None
        }
    }));
}

#[derive(Debug)]
struct HyphenTrie {
    nodes: Vec<HyphenTrieNode>,
    edges: Vec<HyphenTrieEdge>,
    values: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Default)]
struct HyphenTrieNode {
    first_edge: u32,
    edge_count: u16,
    values_start: u32,
    values_len: u8,
}

#[derive(Debug, Clone, Copy)]
struct HyphenTrieEdge {
    byte: u8,
    target: u32,
}

impl HyphenTrie {
    fn apply(&self, word: &[u8], scores: &mut [u8]) {
        for start in 0..word.len() {
            let Some(mut node) = self.child(0, word[start]) else {
                continue;
            };
            self.apply_terminal_values(node, start, scores);
            for &byte in &word[start + 1..] {
                let Some(next) = self.child(node, byte) else {
                    break;
                };
                node = next;
                self.apply_terminal_values(node, start, scores);
            }
        }
    }

    #[inline]
    fn apply_terminal_values(&self, node_idx: u32, start: usize, scores: &mut [u8]) {
        if let Some(values) = self.terminal_values(node_idx) {
            debug_assert!(start + values.len() <= scores.len());
            let score_window = &mut scores[start..start + values.len()];
            for (score, &value) in score_window.iter_mut().zip(values) {
                *score = (*score).max(value);
            }
        }
    }

    fn child(&self, node_idx: u32, byte: u8) -> Option<u32> {
        let node = self.nodes.get(node_idx as usize)?;
        let start = node.first_edge as usize;
        let end = start.saturating_add(node.edge_count as usize);
        let edges = self.edges.get(start..end)?;
        if edges.len() <= 4 {
            for edge in edges {
                if edge.byte == byte {
                    return Some(edge.target);
                }
                if edge.byte > byte {
                    return None;
                }
            }
            return None;
        }
        edges
            .binary_search_by_key(&byte, |edge| edge.byte)
            .ok()
            .and_then(|idx| edges.get(idx).map(|edge| edge.target))
    }

    fn terminal_values(&self, node_idx: u32) -> Option<&[u8]> {
        let node = self.nodes.get(node_idx as usize)?;
        if node.values_len == 0 {
            return None;
        }
        let start = node.values_start as usize;
        let end = start.saturating_add(node.values_len as usize);
        self.values.get(start..end)
    }
}

#[derive(Debug, Default)]
struct BuildHyphenNode {
    children: Vec<(u8, usize)>,
    values: Vec<u8>,
}

fn english_hyphen_trie() -> &'static HyphenTrie {
    static TRIE: OnceLock<HyphenTrie> = OnceLock::new();
    TRIE.get_or_init(|| {
        build_hyphen_trie(
            ENGLISH_STARTER_PATTERNS,
            EN_US_TEX_PATTERNS.split_ascii_whitespace(),
        )
    })
}

fn build_hyphen_trie<'a>(
    starter_patterns: &[HyphenPattern],
    encoded_patterns: impl IntoIterator<Item = &'a str>,
) -> HyphenTrie {
    let mut nodes = vec![BuildHyphenNode::default()];
    for pattern in starter_patterns {
        insert_hyphen_pattern(&mut nodes, pattern.letters.as_bytes(), pattern.values);
    }
    for pattern in encoded_patterns {
        insert_encoded_hyphen_pattern(&mut nodes, pattern);
    }
    flatten_hyphen_trie(nodes)
}

fn insert_encoded_hyphen_pattern(nodes: &mut Vec<BuildHyphenNode>, pattern: &str) {
    let mut letters = Vec::with_capacity(pattern.len());
    let mut values = vec![0u8];
    for byte in pattern.bytes() {
        if byte.is_ascii_digit() {
            if let Some(slot) = values.get_mut(letters.len()) {
                *slot = byte.saturating_sub(b'0');
            }
        } else {
            if letters.len() == 64 {
                return;
            }
            letters.push(byte);
            if values.len() < letters.len() + 1 {
                values.push(0);
            }
        }
    }
    if letters.is_empty() {
        return;
    }
    insert_hyphen_pattern(nodes, &letters, &values);
}

fn insert_hyphen_pattern(nodes: &mut Vec<BuildHyphenNode>, letters: &[u8], values: &[u8]) {
    if letters.is_empty() || values.len() != letters.len() + 1 {
        return;
    }
    let mut node_idx = 0usize;
    for &byte in letters {
        let next_idx = find_or_insert_child(nodes, node_idx, byte);
        node_idx = next_idx;
    }
    merge_hyphen_values(&mut nodes[node_idx].values, values);
}

fn find_or_insert_child(nodes: &mut Vec<BuildHyphenNode>, node_idx: usize, byte: u8) -> usize {
    if let Some((_, child_idx)) = nodes[node_idx]
        .children
        .iter()
        .find(|(existing, _)| *existing == byte)
    {
        return *child_idx;
    }
    let child_idx = nodes.len();
    nodes.push(BuildHyphenNode::default());
    nodes[node_idx].children.push((byte, child_idx));
    child_idx
}

fn merge_hyphen_values(out: &mut Vec<u8>, values: &[u8]) {
    if out.len() < values.len() {
        out.resize(values.len(), 0);
    }
    for (idx, &value) in values.iter().enumerate() {
        if let Some(slot) = out.get_mut(idx) {
            *slot = (*slot).max(value);
        }
    }
}

fn flatten_hyphen_trie(build_nodes: Vec<BuildHyphenNode>) -> HyphenTrie {
    let mut nodes = Vec::with_capacity(build_nodes.len());
    let mut edges = Vec::new();
    let mut values = Vec::new();
    for node in build_nodes {
        let values_start = values.len();
        values.extend_from_slice(&node.values);

        let first_edge = edges.len();
        let mut children = node.children;
        children.sort_unstable_by_key(|(byte, _)| *byte);
        for (byte, target) in children {
            edges.push(HyphenTrieEdge {
                byte,
                target: clamp_usize_to_u32(target),
            });
        }

        nodes.push(HyphenTrieNode {
            first_edge: clamp_usize_to_u32(first_edge),
            edge_count: clamp_usize_to_u16(edges.len().saturating_sub(first_edge)),
            values_start: clamp_usize_to_u32(values_start),
            values_len: clamp_usize_to_u8(values.len().saturating_sub(values_start)),
        });
    }
    HyphenTrie {
        nodes,
        edges,
        values,
    }
}

const ENGLISH_STARTER_PATTERNS: &[HyphenPattern] = &[
    HyphenPattern {
        letters: "tion",
        values: &[0, 0, 0, 4, 0],
    },
    HyphenPattern {
        letters: "ing",
        values: &[0, 0, 4, 0],
    },
    HyphenPattern {
        letters: "ment",
        values: &[0, 0, 0, 4, 0],
    },
    HyphenPattern {
        letters: "able",
        values: &[0, 0, 4, 0, 0],
    },
];

/// Microtypography options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicrotypeOptions {
    /// Enable punctuation protrusion / optical margin alignment.
    pub protrusion: bool,
    /// Maximum font expansion/contraction budget in per-mille of line width.
    /// `20` means up to 2%.
    pub max_expansion_per_mille: u16,
}

impl MicrotypeOptions {
    /// Disabled default: hooks are available but not silently active.
    pub const DISABLED: Self = Self {
        protrusion: false,
        max_expansion_per_mille: 0,
    };

    /// Conservative starting policy for high-quality PDF layout experiments.
    pub const CONSERVATIVE: Self = Self {
        protrusion: true,
        max_expansion_per_mille: 15,
    };
}

impl Default for MicrotypeOptions {
    fn default() -> Self {
        Self::DISABLED
    }
}

/// How far text may visually protrude past the left/right margin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Protrusion {
    pub left: LayoutUnit,
    pub right: LayoutUnit,
}

impl Protrusion {
    /// Total protrusion budget.
    #[must_use]
    pub fn total(self) -> LayoutUnit {
        self.left + self.right
    }
}

/// Compute optical-margin protrusion for a text run.
#[must_use]
pub fn protrusion_for_text(text: &str, size: FontSize, options: MicrotypeOptions) -> Protrusion {
    if !options.protrusion {
        return Protrusion::default();
    }
    let left = text.chars().next().map_or(LayoutUnit::ZERO, |ch| {
        protrusion_amount(left_protrusion_per_mille(ch), size)
    });
    let right = text.chars().next_back().map_or(LayoutUnit::ZERO, |ch| {
        protrusion_amount(right_protrusion_per_mille(ch), size)
    });
    Protrusion { left, right }
}

/// Return the width used for fitting after optical margin protrusion.
#[must_use]
pub fn protruded_fit_width(
    natural_width: LayoutUnit,
    text: &str,
    size: FontSize,
    options: MicrotypeOptions,
) -> LayoutUnit {
    let protrusion = protrusion_for_text(text, size, options).total();
    if natural_width <= LayoutUnit::ZERO || protrusion >= natural_width {
        LayoutUnit::ZERO
    } else {
        natural_width - protrusion
    }
}

/// Maximum deterministic expansion/contraction budget for one line.
#[must_use]
pub fn expansion_budget(line_width: LayoutUnit, options: MicrotypeOptions) -> LayoutUnit {
    let budget =
        (line_width.milli_points() as i128 * options.max_expansion_per_mille as i128) / 1000;
    LayoutUnit(clamp_i128_to_i32(budget))
}

fn protrusion_amount(per_mille: u16, size: FontSize) -> LayoutUnit {
    let amount = (size.milli_points() as u128 * per_mille as u128) / 1000;
    LayoutUnit(clamp_u128_to_i32(amount))
}

const fn left_protrusion_per_mille(ch: char) -> u16 {
    match ch {
        '"' | '\'' | '`' => 350,
        '(' | '[' | '{' => 120,
        '-' | '–' | '—' => 80,
        _ => 0,
    }
}

const fn right_protrusion_per_mille(ch: char) -> u16 {
    match ch {
        '.' | ',' => 550,
        ':' | ';' => 420,
        '!' | '?' => 250,
        '"' | '\'' | '`' => 350,
        ')' | ']' | '}' => 120,
        '-' | '–' | '—' => 80,
        _ => 0,
    }
}

/// One chosen line from the paragraph optimizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineBreak {
    /// First paragraph item on this line.
    pub start: usize,
    /// Exclusive item index for renderable content on this line. A break at
    /// glue excludes the glue; a break at penalty excludes the penalty itself.
    pub end: usize,
    /// First item to consider for the next line.
    pub next: usize,
    /// Natural width before stretch/shrink is applied.
    pub natural_width: LayoutUnit,
    /// TeX-like badness for this line (`0..=10000`).
    pub badness: i32,
    /// Coarse stretch/shrink class used to discourage visually abrupt adjacent
    /// lines.
    pub fitness: FitnessClass,
    /// Cumulative demerits through this line.
    pub demerits: i64,
}

/// Coarse TeX-style line fitness class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitnessClass {
    Tight,
    Decent,
    Loose,
    VeryLoose,
}

#[derive(Debug, Clone, Copy)]
struct BreakCandidate {
    item_index: usize,
    next: usize,
    penalty: i32,
    penalty_width: LayoutUnit,
    flagged: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct BreakCandidateStats {
    has_interior_forced_break: bool,
    has_rewarded_break: bool,
    trailing_forced_width: Option<LayoutUnit>,
}

#[derive(Debug, Clone, Copy)]
struct SegmentMetrics {
    width: LayoutUnit,
    stretch: LayoutUnit,
    shrink: LayoutUnit,
}

#[derive(Debug, Clone, Default)]
struct MetricPrefixes {
    width: Vec<i64>,
    stretch: Vec<i64>,
    shrink: Vec<i64>,
}

impl MetricPrefixes {
    fn rebuild_from_items(&mut self, items: &[ParagraphItem]) {
        self.width.clear();
        self.stretch.clear();
        self.shrink.clear();

        let needed = items.len() + 1;
        self.width.reserve(needed);
        self.stretch.reserve(needed);
        self.shrink.reserve(needed);

        self.width.push(0);
        self.stretch.push(0);
        self.shrink.push(0);

        let mut running_width = 0i64;
        let mut running_stretch = 0i64;
        let mut running_shrink = 0i64;
        for item in items {
            match item {
                ParagraphItem::Box(item) => {
                    running_width += item.width.milli_points() as i64;
                }
                ParagraphItem::Glue(item) => {
                    running_width += item.width.milli_points() as i64;
                    running_stretch += item.stretch.milli_points() as i64;
                    running_shrink += item.shrink.milli_points() as i64;
                }
                ParagraphItem::Penalty(_) => {}
            }
            self.width.push(running_width);
            self.stretch.push(running_stretch);
            self.shrink.push(running_shrink);
        }
    }

    fn segment_metrics(&self, start: usize, candidate: BreakCandidate) -> SegmentMetrics {
        let width = prefix_diff(&self.width, start, candidate.item_index)
            + candidate.penalty_width.milli_points() as i64;
        SegmentMetrics {
            width: LayoutUnit(clamp_i64_to_i32(width)),
            stretch: LayoutUnit(clamp_i64_to_i32(prefix_diff(
                &self.stretch,
                start,
                candidate.item_index,
            ))),
            shrink: LayoutUnit(clamp_i64_to_i32(prefix_diff(
                &self.shrink,
                start,
                candidate.item_index,
            ))),
        }
    }
}

fn prefix_diff(values: &[i64], start: usize, end: usize) -> i64 {
    debug_assert!(start <= end);
    debug_assert!(end < values.len());
    values[end] - values[start]
}

#[derive(Debug, Clone, Copy)]
struct BreakState {
    prev: Option<usize>,
    line: LineBreak,
    flagged: bool,
    fitness: FitnessClass,
}

/// Reusable scratch storage for paragraph layout.
///
/// This is render-call-local by design: callers create one workspace per render
/// job, reuse it for every paragraph in that job, and then drop it. It keeps the
/// core deterministic and WASM-friendly while avoiding per-paragraph allocation
/// churn in the hot line-breaking path.
#[derive(Debug, Default)]
pub struct ParagraphLayoutScratch {
    hyphen_lower: String,
    hyphen_dotted: Vec<u8>,
    hyphen_scores: Vec<u8>,
    hyphen_points: Vec<usize>,
    candidates: Vec<BreakCandidate>,
    forced_prefix: Vec<usize>,
    metrics: MetricPrefixes,
    states: Vec<Option<BreakState>>,
}

impl ParagraphLayoutScratch {
    /// Construct an empty scratch workspace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all live scratch data while retaining allocations for reuse.
    pub fn clear(&mut self) {
        self.hyphen_lower.clear();
        self.hyphen_dotted.clear();
        self.hyphen_scores.clear();
        self.hyphen_points.clear();
        self.candidates.clear();
        self.forced_prefix.clear();
        self.metrics.width.clear();
        self.metrics.stretch.clear();
        self.metrics.shrink.clear();
        self.states.clear();
    }

    /// Report retained capacities for tests and performance proof ledgers.
    #[must_use]
    pub fn capacities(&self) -> ParagraphLayoutScratchCapacities {
        ParagraphLayoutScratchCapacities {
            hyphen_lower_bytes: self.hyphen_lower.capacity(),
            hyphen_dotted_bytes: self.hyphen_dotted.capacity(),
            hyphen_scores: self.hyphen_scores.capacity(),
            hyphen_points: self.hyphen_points.capacity(),
            candidates: self.candidates.capacity(),
            forced_prefixes: self.forced_prefix.capacity(),
            prefix_widths: self.metrics.width.capacity(),
            prefix_stretches: self.metrics.stretch.capacity(),
            prefix_shrinks: self.metrics.shrink.capacity(),
            states: self.states.capacity(),
        }
    }
}

/// Retained scratch-buffer capacities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParagraphLayoutScratchCapacities {
    pub hyphen_lower_bytes: usize,
    pub hyphen_dotted_bytes: usize,
    pub hyphen_scores: usize,
    pub hyphen_points: usize,
    pub candidates: usize,
    pub forced_prefixes: usize,
    pub prefix_widths: usize,
    pub prefix_stretches: usize,
    pub prefix_shrinks: usize,
    pub states: usize,
}

/// Break a paragraph with a first-cut Knuth-Plass-style dynamic program.
///
/// This is intentionally a baseline optimizer, not the final TeX clone:
/// hyphenation, looseness, variable line widths, and emergency stretch all
/// remain separate increments. It already differs materially from greedy
/// wrapping: every legal breakpoint can be paired with every previous legal
/// breakpoint, and the minimum total demerits over the full paragraph is chosen.
#[must_use]
pub fn break_paragraph(items: &[ParagraphItem], line_width: LayoutUnit) -> Vec<LineBreak> {
    let mut scratch = ParagraphLayoutScratch::new();
    let mut out = Vec::new();
    break_paragraph_into(items, line_width, &mut scratch, &mut out);
    out
}

/// Break a paragraph into a caller-owned output buffer using reusable scratch.
///
/// `out` is cleared before use. `scratch` and `out` are separate so renderers can
/// keep one long-lived scratch workspace and decide whether to keep, copy, or
/// immediately consume each paragraph's line breaks.
pub fn break_paragraph_into(
    items: &[ParagraphItem],
    line_width: LayoutUnit,
    scratch: &mut ParagraphLayoutScratch,
    out: &mut Vec<LineBreak>,
) {
    out.clear();
    let candidate_stats = break_candidates_into(items, &mut scratch.candidates);
    if scratch.candidates.is_empty() {
        scratch.forced_prefix.clear();
        scratch.metrics.width.clear();
        scratch.metrics.stretch.clear();
        scratch.metrics.shrink.clear();
        scratch.states.clear();
        return;
    }
    let candidates = &scratch.candidates;
    if !candidate_stats.has_interior_forced_break
        && !candidate_stats.has_rewarded_break
        && let Some(&candidate) = candidates.last()
        && let Some(width) = candidate_stats.trailing_forced_width
        && let Some(line) = trailing_forced_fit_break(candidate, items.len(), width, line_width)
    {
        scratch.forced_prefix.clear();
        scratch.metrics.width.clear();
        scratch.metrics.stretch.clear();
        scratch.metrics.shrink.clear();
        scratch.states.clear();
        out.push(line);
        return;
    }
    scratch.metrics.rebuild_from_items(items);
    if candidate_stats.has_interior_forced_break {
        forced_break_prefixes_into(items, &mut scratch.forced_prefix);
    } else {
        scratch.forced_prefix.clear();
    }

    scratch.states.clear();
    for (j, candidate) in candidates.iter().enumerate() {
        let mut best: Option<BreakState> = None;

        // Predecessors are scanned NEAREST-first (prev_idx descending). Segment
        // width grows monotonically as the start moves earlier (prefix sums), so
        // once an inter-candidate predecessor's line is overfull past its shrink
        // budget, every earlier (wider) predecessor is too — we stop instead of
        // the old unconditional 0..=j scan, which was O(candidates^2) and a
        // practical DoS on a single large paragraph. The result is IDENTICAL: the
        // pruned predecessors are exactly those the old loop rejected as
        // INF-badness, and the `<=` tie-break below (with the reversed order)
        // keeps the same lowest-prev_idx winner the old forward `<` produced.
        for prev_idx in (0..=j).rev() {
            let start = if prev_idx == j {
                0
            } else {
                match candidates.get(prev_idx) {
                    Some(prev_candidate) => prev_candidate.next,
                    None => continue,
                }
            };
            if start > candidate.item_index {
                continue;
            }
            if candidate_stats.has_interior_forced_break
                && forced_break_between(&scratch.forced_prefix, start, candidate.item_index)
            {
                continue;
            }
            // Evaluate the segment BEFORE the reachability check so the pruning
            // decision is purely width-based. The break condition MUST be the
            // monotonic "overfull past max shrink" test — `width - shrink` grows
            // strictly as the start moves earlier (each added item widens it, each
            // added space widens it net of its shrink), so once a line cannot fit
            // even fully shrunk, every earlier (wider) inter-candidate predecessor
            // cannot either. (badness alone is NOT monotonic: an underfull narrow
            // segment can also be INF, and would wrongly stop the scan.) The
            // start = 0 whole-prefix segment is the widest of all, so its overflow
            // says nothing about narrower predecessors — skip it, don't stop.
            let segment = scratch.metrics.segment_metrics(start, *candidate);
            // An INTER-candidate line (prev_idx != j) that cannot fit even fully
            // shrunk is "overfull". Segment width grows monotonically as the start
            // moves earlier, so the first overfull predecessor reached is the
            // least overfull; every earlier one is strictly worse. We keep it
            // SELECTABLE (not illegal) at a large finite demerit — `line_badness`
            // caps at INF_PENALTY, so `line_demerits` charges ~1e8, far above any
            // feasible line — then stop (keeping the scan O(n)). A single too-wide
            // token (a URL, a long identifier) therefore no longer discards the
            // whole paragraph's optimal breaking: it is isolated on one overfull
            // line while the rest stays optimal. Feasible paragraphs are
            // unaffected (a feasible line always wins), and greedy first-fit
            // remains only a true last resort.
            //
            // The prev_idx == j whole-prefix segment is deliberately excluded: it
            // is the widest of all, so admitting it would let the DP cram the
            // entire paragraph onto one maximally-overfull line (all overfull
            // lines share the capped demerit, so fewer lines would win). Its
            // overflow says nothing about the narrower inter-candidate
            // predecessors, so it is neither selectable-when-overfull nor a stop.
            let overfull =
                prev_idx != j && segment.width.saturating_sub(segment.shrink) > line_width;
            let badness = candidate_badness(*candidate, segment, line_width);
            // Underfull-past-stretch lines (INF badness, not overfull) stay illegal
            // — keep scanning toward wider segments.
            if badness >= INF_PENALTY && !overfull {
                continue;
            }
            let prev_state = if prev_idx == j {
                None
            } else {
                match scratch.states[prev_idx] {
                    Some(state) => Some((prev_idx, state)),
                    None => {
                        // No reachable path through this predecessor. For an
                        // overfull line every earlier predecessor is only more
                        // overfull, so stop; otherwise keep scanning.
                        if overfull {
                            break;
                        }
                        continue;
                    }
                }
            };
            let fitness = candidate_fitness(*candidate, segment, line_width);
            let prev_demerits = prev_state.map_or(0, |(_, state)| state.line.demerits);
            let demerits = prev_demerits.saturating_add(line_demerits(
                badness,
                candidate.penalty,
                prev_state.is_some_and(|(_, state)| state.flagged),
                candidate.flagged,
                prev_state.map(|(_, state)| state.fitness),
                fitness,
            ));
            let state = BreakState {
                prev: prev_state.map(|(idx, _)| idx),
                line: LineBreak {
                    start,
                    end: candidate.item_index,
                    next: candidate.next,
                    natural_width: segment.width,
                    badness,
                    fitness,
                    demerits,
                },
                flagged: candidate.flagged,
                fitness,
            };
            if best.is_none_or(|old| state.line.demerits <= old.line.demerits) {
                best = Some(state);
            }
            if overfull {
                // Earlier inter-candidate predecessors are even more overfull
                // (strictly larger demerit) and never win; stop to keep O(n).
                break;
            }
        }
        scratch.states.push(best);
    }

    let Some(mut idx) = scratch.states.len().checked_sub(1) else {
        return;
    };
    if scratch.states[idx].is_none() {
        // True last resort: no path exists even allowing overfull lines (e.g. a
        // forced break makes the last candidate unreachable). Fall back to greedy
        // first-fit rather than emitting nothing.
        greedy_break_paragraph_into(candidates, line_width, &scratch.metrics, out);
        return;
    }
    while let Some(state) = scratch.states[idx] {
        out.push(state.line);
        match state.prev {
            Some(prev) => idx = prev,
            None => break,
        }
    }
    out.reverse();
}

fn trailing_forced_fit_break(
    candidate: BreakCandidate,
    item_count: usize,
    natural_width: LayoutUnit,
    line_width: LayoutUnit,
) -> Option<LineBreak> {
    if candidate.penalty != FORCED_BREAK_PENALTY || candidate.next != item_count {
        return None;
    }
    if natural_width > line_width {
        return None;
    }
    let badness = 0;
    let fitness = FitnessClass::Decent;
    Some(LineBreak {
        start: 0,
        end: candidate.item_index,
        next: candidate.next,
        natural_width,
        badness,
        fitness,
        demerits: line_demerits(
            badness,
            candidate.penalty,
            false,
            candidate.flagged,
            None,
            fitness,
        ),
    })
}

fn forced_break_prefixes_into(items: &[ParagraphItem], out: &mut Vec<usize>) {
    out.clear();
    out.reserve(items.len() + 1);
    let mut count = 0usize;
    out.push(count);
    for item in items {
        if matches!(
            item,
            ParagraphItem::Penalty(Penalty {
                penalty: FORCED_BREAK_PENALTY,
                ..
            })
        ) {
            count = count.saturating_add(1);
        }
        out.push(count);
    }
}

fn forced_break_between(prefix: &[usize], start: usize, end: usize) -> bool {
    let before_start = prefix.get(start).copied().unwrap_or(0);
    let before_end = prefix
        .get(end)
        .copied()
        .or_else(|| prefix.last().copied())
        .unwrap_or(before_start);
    before_end > before_start
}

fn break_candidates_into(
    items: &[ParagraphItem],
    out: &mut Vec<BreakCandidate>,
) -> BreakCandidateStats {
    out.clear();
    out.reserve(items.len());
    let mut stats = BreakCandidateStats::default();
    let mut running_width = 0i64;
    for (idx, item) in items.iter().enumerate() {
        match item {
            ParagraphItem::Box(item) => {
                running_width += item.width.milli_points() as i64;
            }
            ParagraphItem::Glue(item) => {
                running_width += item.width.milli_points() as i64;
                out.push(BreakCandidate {
                    item_index: idx,
                    next: idx + 1,
                    penalty: 0,
                    penalty_width: LayoutUnit::ZERO,
                    flagged: false,
                });
            }
            ParagraphItem::Penalty(p) if p.penalty < INF_PENALTY => {
                let next = idx + 1;
                if p.penalty == FORCED_BREAK_PENALTY {
                    if next < items.len() {
                        stats.has_interior_forced_break = true;
                    } else {
                        stats.trailing_forced_width = Some(LayoutUnit(clamp_i64_to_i32(
                            running_width + p.width.milli_points() as i64,
                        )));
                    }
                } else if p.penalty < 0 {
                    stats.has_rewarded_break = true;
                }
                out.push(BreakCandidate {
                    item_index: idx,
                    next,
                    penalty: p.penalty,
                    penalty_width: p.width,
                    flagged: p.flagged,
                });
            }
            ParagraphItem::Penalty(_) => {}
        }
    }
    stats
}

fn line_badness(metrics: SegmentMetrics, line_width: LayoutUnit) -> i32 {
    let diff = line_width.milli_points() as i64 - metrics.width.milli_points() as i64;
    if diff == 0 {
        return 0;
    }
    let available = if diff > 0 {
        metrics.stretch.milli_points() as i64
    } else {
        metrics.shrink.milli_points() as i64
    };
    if available <= 0 {
        return INF_PENALTY;
    }
    // TeX semantics: glue can stretch past its budget (at cubically growing
    // badness) but can never shrink below width minus shrink. A line that only
    // "fits" by shrinking beyond the budget is overfull: infinitely bad, not
    // merely ugly, otherwise the breaker happily crushes interword spaces
    // toward zero instead of taking a feasible later break.
    if diff < 0 && -diff > available {
        return INF_PENALTY;
    }
    let ratio_milli = (diff.unsigned_abs() as u128).saturating_mul(1000) / available as u128;
    let badness = 100u128
        .saturating_mul(ratio_milli)
        .saturating_mul(ratio_milli)
        .saturating_mul(ratio_milli)
        / 1_000_000_000u128;
    badness.min(INF_PENALTY as u128) as i32
}

fn candidate_badness(
    candidate: BreakCandidate,
    metrics: SegmentMetrics,
    line_width: LayoutUnit,
) -> i32 {
    if candidate.penalty == FORCED_BREAK_PENALTY && metrics.width <= line_width {
        0
    } else {
        line_badness(metrics, line_width)
    }
}

fn line_demerits(
    badness: i32,
    penalty: i32,
    prev_flagged: bool,
    flagged: bool,
    prev_fitness: Option<FitnessClass>,
    fitness: FitnessClass,
) -> i64 {
    let base = (badness as i64 + 1).saturating_pow(2);
    let penalty_cost = if penalty == FORCED_BREAK_PENALTY {
        0
    } else if penalty >= 0 {
        (penalty as i64).saturating_pow(2)
    } else {
        -((penalty as i64).saturating_pow(2))
    };
    let flagged_cost = if prev_flagged && flagged { 10_000 } else { 0 };
    let fitness_cost = if prev_fitness.is_some_and(|prev| fitness_distance(prev, fitness) > 1) {
        3_000
    } else {
        0
    };
    base.saturating_add(penalty_cost)
        .saturating_add(flagged_cost)
        .saturating_add(fitness_cost)
}

fn line_fitness(metrics: SegmentMetrics, line_width: LayoutUnit) -> FitnessClass {
    let diff = line_width.milli_points() as i64 - metrics.width.milli_points() as i64;
    if diff == 0 {
        return FitnessClass::Decent;
    }
    let available = if diff > 0 {
        metrics.stretch.milli_points() as i64
    } else {
        metrics.shrink.milli_points() as i64
    };
    if available <= 0 {
        return FitnessClass::VeryLoose;
    }
    let ratio_milli = diff.saturating_mul(1000) / available;
    if ratio_milli < -500 {
        FitnessClass::Tight
    } else if ratio_milli <= 500 {
        FitnessClass::Decent
    } else if ratio_milli <= 1000 {
        FitnessClass::Loose
    } else {
        FitnessClass::VeryLoose
    }
}

fn candidate_fitness(
    candidate: BreakCandidate,
    metrics: SegmentMetrics,
    line_width: LayoutUnit,
) -> FitnessClass {
    if candidate.penalty == FORCED_BREAK_PENALTY && metrics.width <= line_width {
        FitnessClass::Decent
    } else {
        line_fitness(metrics, line_width)
    }
}

fn fitness_distance(a: FitnessClass, b: FitnessClass) -> i32 {
    fitness_rank(a).abs_diff(fitness_rank(b)) as i32
}

const fn fitness_rank(class: FitnessClass) -> i32 {
    match class {
        FitnessClass::Tight => 0,
        FitnessClass::Decent => 1,
        FitnessClass::Loose => 2,
        FitnessClass::VeryLoose => 3,
    }
}

fn greedy_break_paragraph_into(
    candidates: &[BreakCandidate],
    line_width: LayoutUnit,
    metrics: &MetricPrefixes,
    out: &mut Vec<LineBreak>,
) {
    let mut start = 0usize;
    let mut last_candidate: Option<BreakCandidate> = None;
    for &candidate in candidates {
        let mut segment = metrics.segment_metrics(start, candidate);
        if segment.width > line_width {
            if let Some(prev) = last_candidate {
                let prev_metrics = metrics.segment_metrics(start, prev);
                out.push(LineBreak {
                    start,
                    end: prev.item_index,
                    next: prev.next,
                    natural_width: prev_metrics.width,
                    badness: candidate_badness(prev, prev_metrics, line_width),
                    fitness: candidate_fitness(prev, prev_metrics, line_width),
                    demerits: 0,
                });
                start = prev.next;
                segment = metrics.segment_metrics(start, candidate);
            }
        }
        if candidate.penalty == FORCED_BREAK_PENALTY {
            out.push(LineBreak {
                start,
                end: candidate.item_index,
                next: candidate.next,
                natural_width: segment.width,
                badness: candidate_badness(candidate, segment, line_width),
                fitness: candidate_fitness(candidate, segment, line_width),
                demerits: 0,
            });
            start = candidate.next;
            last_candidate = None;
            continue;
        }
        last_candidate = Some(candidate);
    }
    if let Some(candidate) = last_candidate {
        let metrics = metrics.segment_metrics(start, candidate);
        out.push(LineBreak {
            start,
            end: candidate.item_index,
            next: candidate.next,
            natural_width: metrics.width,
            badness: candidate_badness(candidate, metrics, line_width),
            fitness: candidate_fitness(candidate, metrics, line_width),
            demerits: 0,
        });
    }
}

const fn clamp_u128_to_i32(value: u128) -> i32 {
    if value > i32::MAX as u128 {
        i32::MAX
    } else {
        value as i32
    }
}

const fn clamp_i128_to_i32(value: i128) -> i32 {
    if value > i32::MAX as i128 {
        i32::MAX
    } else if value < i32::MIN as i128 {
        i32::MIN
    } else {
        value as i32
    }
}

const fn clamp_i64_to_i32(value: i64) -> i32 {
    if value > i32::MAX as i64 {
        i32::MAX
    } else if value < i32::MIN as i64 {
        i32::MIN
    } else {
        value as i32
    }
}

const fn clamp_usize_to_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

const fn clamp_usize_to_u16(value: usize) -> u16 {
    if value > u16::MAX as usize {
        u16::MAX
    } else {
        value as u16
    }
}

const fn clamp_usize_to_u8(value: usize) -> u8 {
    if value > u8::MAX as usize {
        u8::MAX
    } else {
        value as u8
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod overfull_selectability_tests {
    //! Real bundled-font metrics (no test doubles): `Font` implements
    //! `AdvanceMetrics`/`PairMetrics`, so these drive the real breaker end to end.
    use super::{
        FontSize, INF_PENALTY, LayoutUnit, SegmentMetrics, break_paragraph, line_badness,
        paragraph_items_from_text,
    };
    use crate::FontFamily;
    use crate::fonts::{FontStyle, load_body};
    use crate::text::Font;

    fn body() -> Font {
        load_body(FontFamily::Sans, FontStyle::Regular).expect("bundled body font parses")
    }

    #[test]
    fn too_wide_token_is_isolated_via_optimal_dp_not_greedy_over_the_paragraph() {
        // A ~100-char single token is far wider than a 200pt line, while the
        // surrounding words fit feasibly. Overfull lines are now selectable, so the
        // words keep their optimal breaking and the token is isolated on its own
        // overfull line — the whole paragraph is NOT dropped to greedy first-fit.
        let font = body();
        let size = FontSize::from_points(10);
        let width = LayoutUnit::from_points(200);
        let token = "W".repeat(100);
        let text = format!("the quick brown fox {token} jumps over the lazy dog");
        let items = paragraph_items_from_text(&font, &text, size);
        let breaks = break_paragraph(&items, width);

        let dbg: Vec<(i32, i32)> = breaks
            .iter()
            .map(|b| (b.badness, b.natural_width.milli_points()))
            .collect();
        assert!(breaks.len() >= 3, "multi-line layout; breaks {dbg:?}");
        // A PHYSICALLY overfull line (natural width past the line) is present — the
        // isolated too-wide token — and it is selectable at the capped demerit;
        // feasible lines coexist, so the paragraph was broken by the DP, not
        // collapsed. (The exact optimal split is pinned by the integration test
        // with fixed StubMetrics.)
        let overfull: Vec<_> = breaks.iter().filter(|b| b.natural_width > width).collect();
        assert!(
            !overfull.is_empty(),
            "expected an overfull line; breaks {dbg:?}"
        );
        assert!(
            overfull.iter().all(|b| b.badness >= INF_PENALTY),
            "overfull lines carry the capped demerit; breaks {dbg:?}"
        );
        assert!(
            breaks.iter().any(|b| b.badness < INF_PENALTY),
            "feasible lines coexist (not greedy-collapsed); breaks {dbg:?}"
        );
    }

    #[test]
    fn too_wide_token_after_an_unbreakable_narrow_word_still_lays_out() {
        // A leading single narrow word cannot form its own (underfull) line, so the
        // candidate after the token reaches an unreachable inter-candidate
        // predecessor — exercising the overfull-unreachable stop. The paragraph
        // must still lay out (greedy last resort) without panicking or losing text.
        let font = body();
        let size = FontSize::from_points(10);
        let width = LayoutUnit::from_points(200);
        let token = "W".repeat(100);
        let items = paragraph_items_from_text(&font, &format!("a {token} b"), size);
        let breaks = break_paragraph(&items, width);
        assert!(!breaks.is_empty(), "must still produce a layout");
    }

    #[test]
    fn feasible_paragraph_never_emits_an_overfull_line() {
        // A plainly breakable paragraph still breaks feasibly (no overfull line),
        // confirming overfull selectability does not perturb normal layout.
        let font = body();
        let size = FontSize::from_points(10);
        let width = LayoutUnit::from_points(400);
        let items = paragraph_items_from_text(&font, "the quick brown fox", size);
        let breaks = break_paragraph(&items, width);
        assert!(breaks.iter().all(|b| b.badness < INF_PENALTY));
    }

    #[test]
    fn line_badness_rejects_shrink_past_available_glue() {
        let over_shrunk = SegmentMetrics {
            width: LayoutUnit::from_points(120),
            stretch: LayoutUnit::ZERO,
            shrink: LayoutUnit::from_points(5),
        };
        assert_eq!(
            line_badness(over_shrunk, LayoutUnit::from_points(100)),
            INF_PENALTY,
            "a line cannot be made feasible by shrinking more than its glue permits"
        );

        let feasible_shrink = SegmentMetrics {
            width: LayoutUnit::from_points(104),
            stretch: LayoutUnit::ZERO,
            shrink: LayoutUnit::from_points(5),
        };
        assert!(
            line_badness(feasible_shrink, LayoutUnit::from_points(100)) < INF_PENALTY,
            "shrinking within the available budget remains a finite badness edge"
        );
    }

    #[test]
    fn clamp_usize_helpers_saturate_on_overflow_and_pass_through_small_values() {
        use super::{clamp_i64_to_i32, clamp_usize_to_u8, clamp_usize_to_u16, clamp_usize_to_u32};
        // On 64-bit hosts usize::MAX exceeds each target's max, exercising the
        // saturating branch; small values pass through unchanged. The asserted
        // results also hold on 32-bit (where usize::MAX == u32::MAX).
        assert_eq!(clamp_usize_to_u32(usize::MAX), u32::MAX);
        assert_eq!(clamp_usize_to_u16(usize::MAX), u16::MAX);
        assert_eq!(clamp_usize_to_u8(usize::MAX), u8::MAX);
        assert_eq!(clamp_usize_to_u32(7), 7);
        assert_eq!(clamp_usize_to_u16(7), 7);
        assert_eq!(clamp_usize_to_u8(7), 7);
        assert_eq!(clamp_i64_to_i32(i64::MAX), i32::MAX);
        assert_eq!(clamp_i64_to_i32(i64::MIN), i32::MIN);
        assert_eq!(clamp_i64_to_i32(7), 7);
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod hyphen_and_break_edge_tests {
    use super::{
        AdvanceMetrics, BreakCandidate, BuildHyphenNode, FORCED_BREAK_PENALTY, FitnessClass,
        FontSize, HyphenPattern, LayoutUnit, PairMetrics, ParagraphItem, Penalty, StyledText,
        TextBox, TextStyle, append_styled_word_chunk, build_hyphen_trie,
        insert_encoded_hyphen_pattern, insert_hyphen_pattern,
        push_hyphenated_word_items_from_points, trailing_forced_fit_break,
    };

    /// Deterministic flat metrics: every char advances 500/1000 em, no kerning.
    struct FlatMetrics;

    impl AdvanceMetrics for FlatMetrics {
        fn advance_1000(&self, _ch: char) -> u32 {
            500
        }
    }

    impl PairMetrics for FlatMetrics {}

    #[test]
    fn append_styled_word_chunk_is_a_no_op_for_an_empty_chunk() {
        let mut current = StyledText::plain("hy");
        let mut current_plain = String::from("hy");
        let mut current_width = LayoutUnit::from_milli_points(10_000);

        append_styled_word_chunk(
            &FlatMetrics,
            &mut current,
            &mut current_plain,
            &mut current_width,
            "",
            TextStyle::BODY,
            FontSize::from_points(10),
        );

        assert_eq!(current_plain, "hy", "empty chunk must not change text");
        assert_eq!(current, StyledText::plain("hy"));
        assert_eq!(
            current_width,
            LayoutUnit::from_milli_points(10_000),
            "empty chunk must not change width (no phantom kerning)"
        );
    }

    #[test]
    fn hyphenated_word_items_skip_duplicate_points_and_a_point_at_word_end() {
        // At 10pt with 500/1000-em advances every char is 5000 milli-points.
        let size = FontSize::from_points(10);
        let hyphen_width = LayoutUnit::from_milli_points(2_500);
        let mut out = Vec::new();

        // Duplicate point (2, 2) must not emit an empty box; a final point at
        // word end (6 == len) must suppress the trailing box.
        push_hyphenated_word_items_from_points(
            &mut out,
            &FlatMetrics,
            "hyphen",
            size,
            hyphen_width,
            &[2, 2, 6],
        );

        // Two boxes and two flagged discretionary penalties, nothing else: the
        // duplicate point emits no empty box and the terminal point emits its
        // penalty but no trailing box.
        let hyphen_penalty = ParagraphItem::Penalty(Penalty {
            width: hyphen_width,
            penalty: 50,
            flagged: true,
        });
        assert_eq!(
            out,
            vec![
                ParagraphItem::Box(TextBox {
                    text: "hy".to_string(),
                    runs: StyledText::plain("hy"),
                    width: LayoutUnit::from_milli_points(10_000),
                }),
                hyphen_penalty.clone(),
                ParagraphItem::Box(TextBox {
                    text: "phen".to_string(),
                    runs: StyledText::plain("phen"),
                    width: LayoutUnit::from_milli_points(20_000),
                }),
                hyphen_penalty,
            ]
        );
    }

    #[test]
    fn hyphen_trie_apply_skips_word_starts_without_a_root_edge() {
        let trie = build_hyphen_trie(
            &[HyphenPattern {
                letters: "ab",
                values: &[0, 9, 0],
            }],
            std::iter::empty::<&str>(),
        );
        let mut scores = [0u8; 4];
        // 'x' has no root edge (continue); the "ab" starting at offset 1 applies
        // its pattern values at that offset.
        trie.apply(b"xab", &mut scores);
        assert_eq!(scores, [0, 0, 9, 0]);
    }

    #[test]
    fn hyphen_pattern_insertion_rejects_malformed_or_oversized_patterns() {
        let mut nodes = vec![BuildHyphenNode::default()];

        // Encoded pattern longer than the 64-letter cap: rejected outright.
        insert_encoded_hyphen_pattern(&mut nodes, &"a".repeat(65));
        assert_eq!(nodes.len(), 1, "oversized pattern must not grow the trie");

        // Digits-only encoded pattern has no letters: rejected.
        insert_encoded_hyphen_pattern(&mut nodes, "5");
        assert_eq!(nodes.len(), 1, "letterless pattern must not grow the trie");

        // Raw insertion guards: empty letters and a values/letters length
        // mismatch (values must be letters.len() + 1) are both rejected.
        insert_hyphen_pattern(&mut nodes, b"", &[0]);
        insert_hyphen_pattern(&mut nodes, b"ab", &[0, 0]);
        assert_eq!(nodes.len(), 1, "malformed patterns must not grow the trie");

        // A well-formed pattern still inserts one node per letter.
        insert_hyphen_pattern(&mut nodes, b"ab", &[0, 1, 0]);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[2].values, vec![0, 1, 0]);
    }

    #[test]
    fn trailing_forced_fit_break_only_accepts_the_paragraph_final_forced_penalty() {
        let width = LayoutUnit::from_points(100);
        let natural = LayoutUnit::from_points(50);

        // Not a forced penalty: no fast-path line.
        let unforced = BreakCandidate {
            item_index: 2,
            next: 3,
            penalty: 0,
            penalty_width: LayoutUnit::ZERO,
            flagged: false,
        };
        assert_eq!(trailing_forced_fit_break(unforced, 3, natural, width), None);

        // Forced but not paragraph-final (next != item_count): no fast path.
        let interior = BreakCandidate {
            penalty: FORCED_BREAK_PENALTY,
            ..unforced
        };
        assert_eq!(trailing_forced_fit_break(interior, 4, natural, width), None);

        // Paragraph-final forced penalty that fits: one Decent zero-badness line.
        let line = trailing_forced_fit_break(interior, 3, natural, width)
            .expect("fitting trailing forced break yields the fast-path line");
        assert_eq!((line.start, line.end, line.next), (0, 2, 3));
        assert_eq!(line.natural_width, natural);
        assert_eq!(line.badness, 0);
        assert_eq!(line.fitness, FitnessClass::Decent);
        assert_eq!(line.demerits, 1, "(badness 0 + 1)^2 with no penalty cost");
    }
}
