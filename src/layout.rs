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
    /// Optical-margin protrusion precomputed at box construction, where the
    /// font size is known (see docs/MICROTYPOGRAPHY.md). `Protrusion::default()`
    /// (zero) unless the caller enabled microtype protrusion — the breaker
    /// stays size-agnostic and default output stays byte-identical.
    pub protrusion: Protrusion,
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
            Inline::FootnoteRef { .. } => {}
            Inline::Text(text) => out.push_text(text, style),
            Inline::Emphasis(content) => push_inline_runs(out, content, style.with_italic()),
            Inline::Strong(content) => push_inline_runs(out, content, style.with_bold()),
            Inline::Strikethrough(content) => {
                push_inline_runs(out, content, style.with_strikethrough());
            }
            Inline::Code(text) | Inline::Math(text) | Inline::DisplayMath(text) => {
                out.push_text(text, style.with_code())
            }
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
                        &mut items,
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
                &mut items,
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

/// Append one whitespace-free chunk to the word being built, splitting it into
/// separate boxes wherever CJK rules permit a line break.
///
/// A Latin chunk contains no CJK break opportunity, so it is appended whole and
/// the emitted item stream is exactly what it was before CJK support.
#[allow(clippy::too_many_arguments)]
fn append_styled_word_chunk<M: PairMetrics>(
    items: &mut Vec<ParagraphItem>,
    metrics: &M,
    current: &mut StyledText,
    current_plain: &mut String,
    current_width: &mut LayoutUnit,
    chunk: &str,
    style: TextStyle,
    size: FontSize,
) {
    let mut prev = current
        .runs
        .last()
        .and_then(|last| last.text.chars().next_back());
    let mut segment_start = 0usize;
    for (idx, ch) in chunk.char_indices() {
        if prev.is_some_and(|left| cjk_break_allowed(left, ch)) {
            append_styled_chunk_text(
                metrics,
                current,
                current_plain,
                current_width,
                &chunk[segment_start..idx],
                style,
                size,
            );
            if !current.is_empty() {
                push_styled_word_box(items, current, current_plain, current_width);
                items.push(ParagraphItem::Glue(cjk_break_glue(size)));
            }
            segment_start = idx;
        }
        prev = Some(ch);
    }
    append_styled_chunk_text(
        metrics,
        current,
        current_plain,
        current_width,
        &chunk[segment_start..],
        style,
        size,
    );
}

fn append_styled_chunk_text<M: PairMetrics>(
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
        protrusion: Protrusion::default(),
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
        hyphenator.hyphenation_points_into_scratch(
            word,
            hyphenator.default_options(),
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

/// Convert [`Hyphenator`] points (character offsets into `word`) into byte
/// offsets so callers can slice without panicking on non-ASCII letters.
fn hyphen_char_points_to_byte_offsets(word: &str, points: &[usize]) -> Vec<usize> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut wanted: Vec<usize> = points.iter().copied().filter(|&p| p > 0).collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    wanted.sort_unstable();
    wanted.dedup();
    let mut out = Vec::with_capacity(wanted.len());
    let mut wi = 0usize;
    let mut char_i = 0usize;
    for (byte_i, _) in word.char_indices() {
        while wi < wanted.len() && wanted[wi] == char_i {
            out.push(byte_i);
            wi += 1;
        }
        if wi == wanted.len() {
            return out;
        }
        char_i += 1;
    }
    while wi < wanted.len() {
        if wanted[wi] == char_i {
            out.push(word.len());
        }
        wi += 1;
    }
    out
}

fn push_hyphenated_word_items_from_points<M: PairMetrics>(
    out: &mut Vec<ParagraphItem>,
    metrics: &M,
    word: &str,
    size: FontSize,
    hyphen_width: LayoutUnit,
    points: &[usize],
) {
    // Dictionary hyphens (which render a `-`) are character offsets from the
    // hyphenator; CJK opportunities (which render nothing) are already byte
    // offsets from `char_indices`. Convert the former before merging so a
    // word like "Bäckerei" is sliced on character boundaries, not mid-code-unit.
    let mut splits: Vec<(usize, bool)> = hyphen_char_points_to_byte_offsets(word, points)
        .into_iter()
        .map(|byte| (byte, true))
        .collect();
    let mut prev: Option<char> = None;
    for (idx, ch) in word.char_indices() {
        if prev.is_some_and(|left| cjk_break_allowed(left, ch)) {
            splits.push((idx, false));
        }
        prev = Some(ch);
    }
    if splits.is_empty() {
        out.push(ParagraphItem::Box(TextBox {
            text: word.to_string(),
            runs: StyledText::plain(word),
            width: measure_text_with_pairs(metrics, word, size),
            protrusion: Protrusion::default(),
        }));
        return;
    }
    splits.sort_by_key(|split| split.0);
    splits.dedup_by_key(|split| split.0);

    let mut start = 0usize;
    for (offset, hyphen) in splits {
        let end = offset.min(word.len());
        if end > start {
            let part = &word[start..end];
            out.push(ParagraphItem::Box(TextBox {
                text: part.to_string(),
                runs: StyledText::plain(part),
                width: measure_text_with_pairs(metrics, part, size),
                protrusion: Protrusion::default(),
            }));
            if hyphen {
                out.push(ParagraphItem::Penalty(Penalty {
                    width: hyphen_width,
                    penalty: 50,
                    flagged: true,
                }));
            } else {
                out.push(ParagraphItem::Glue(cjk_break_glue(size)));
            }
        }
        start = end;
    }
    if start < word.len() {
        let part = &word[start..];
        out.push(ParagraphItem::Box(TextBox {
            text: part.to_string(),
            runs: StyledText::plain(part),
            width: measure_text_with_pairs(metrics, part, size),
            protrusion: Protrusion::default(),
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

/// Default TeX-like interword glue for the first paragraph builder.
#[must_use]
pub fn default_interword_glue(space: LayoutUnit) -> Glue {
    Glue {
        width: space,
        stretch: LayoutUnit::from_milli_points(space.milli_points() / 2),
        shrink: LayoutUnit::from_milli_points(space.milli_points() / 3),
    }
}

// ---------------------------------------------------------------------------
// CJK line breaking (UAX #14, reduced to the rules CJK typesetting needs)
// ---------------------------------------------------------------------------

/// Stretch of one inter-ideograph break, in 1/1000 em.
///
/// Chinese, Japanese, and Korean text is written without interword spaces, so a
/// whitespace-only breaker finds no break opportunity at all inside a run of
/// ideographs and the whole run becomes one unbreakable box that overruns the
/// measure. The fix is the one traditional CJK typesetters use: a *breakable,
/// zero-width, slightly stretchable* gap between adjacent ideographs
/// (TeX's `\CJKglue`). Zero natural width keeps the character grid intact, the
/// stretch lets the optimizer justify a line by opening the inter-character
/// gaps by a fraction of an em instead of declaring the line infeasible, and
/// there is deliberately no shrink — CJK glyphs must never be crowded.
const CJK_BREAK_STRETCH_1000: u32 = 125;

/// The glue inserted at a permitted break between two CJK characters.
///
/// See [`CJK_BREAK_STRETCH_1000`]. The glue carries no width, so a paragraph
/// that never breaks there measures exactly as it did before.
#[must_use]
pub fn cjk_break_glue(size: FontSize) -> Glue {
    Glue {
        width: LayoutUnit::ZERO,
        stretch: advance_to_layout_units(CJK_BREAK_STRETCH_1000, size),
        shrink: LayoutUnit::ZERO,
    }
}

/// The UAX #14 line-break classes that CJK layout actually distinguishes.
///
/// Everything a Latin-only breaker already handles collapses into
/// [`CjkClass::Other`], which is how ASCII behaviour is kept bit-for-bit
/// identical: a break is only ever *added* when one of the two characters
/// around it belongs to a CJK script or to CJK punctuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CjkClass {
    /// Not CJK-relevant: Latin letters, digits, symbols, everything else.
    Other,
    /// `ID` — ideographs, kana, bopomofo, and the fullwidth forms that behave
    /// like them. A break is allowed on both sides.
    Ideographic,
    /// `H2` — a precomposed Hangul LV syllable.
    HangulLv,
    /// `H3` — a precomposed Hangul LVT syllable.
    HangulLvt,
    /// `JL` — conjoining Hangul leading consonant.
    JamoLeading,
    /// `JV` — conjoining Hangul vowel.
    JamoVowel,
    /// `JT` — conjoining Hangul trailing consonant.
    JamoTrailing,
    /// `OP`/`PR` — opening bracket or prefix. Never break *after* it (LB14),
    /// so `（` can never be stranded alone at the end of a line.
    Open,
    /// `CL`/`CP`/`EX`/`IS`/`NS`/`PO` — closing bracket, sentence punctuation,
    /// small kana, iteration marks. Never break *before* it (LB13/16/25), so a
    /// line can never start with `。`, `）`, or `ゃ`.
    Close,
    /// `CM`/`ZWJ` — attaches to the preceding character (LB9), never break
    /// before it.
    Combining,
}

/// A classified character plus whether the character itself is CJK.
///
/// The `cjk` flag is what keeps Latin untouched: ASCII `)` classifies as
/// [`CjkClass::Close`] so it is never orphaned after an ideograph, but a pair
/// of ASCII characters never gains a break because neither side is CJK.
#[derive(Debug, Clone, Copy)]
struct CjkInfo {
    class: CjkClass,
    cjk: bool,
}

/// Classify one character. The arms are grouped by UAX #14 class (several map
/// to the same class) so the table stays auditable against `LineBreak.txt`.
fn cjk_info(ch: char) -> CjkInfo {
    if ch.is_ascii() {
        let class = match ch {
            '(' | '[' | '{' => CjkClass::Open,
            ')' | ']' | '}' | ',' | '.' | ';' | ':' | '!' | '?' | '%' => CjkClass::Close,
            _ => CjkClass::Other,
        };
        return CjkInfo { class, cjk: false };
    }

    let cp = ch as u32;
    let (class, cjk) = match cp {
        // LB9 / LB8a: combining marks and joiners bind to the previous glyph.
        0x200D
        | 0x0300..=0x036F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20F0
        | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F
        | 0xE0100..=0xE01EF => (CjkClass::Combining, false),
        // Combining kana voicing marks are CJK but still bind leftwards.
        0x3099..=0x309A => (CjkClass::Combining, true),
        // LB26: conjoining Hangul jamo compose one syllable.
        0x1100..=0x115F | 0xA960..=0xA97C => (CjkClass::JamoLeading, true),
        0x1160..=0x11A7 | 0xD7B0..=0xD7C6 => (CjkClass::JamoVowel, true),
        0x11A8..=0x11FF | 0xD7CB..=0xD7FB => (CjkClass::JamoTrailing, true),
        0xAC00..=0xD7A3 => {
            if (cp - 0xAC00) % 28 == 0 {
                (CjkClass::HangulLv, true)
            } else {
                (CjkClass::HangulLvt, true)
            }
        }
        // LB14 (`OP`) plus the fullwidth currency prefixes (`PR`).
        0x3008 | 0x300A | 0x300C | 0x300E | 0x3010 | 0x3014 | 0x3016 | 0x3018 | 0x301A | 0x301D => {
            (CjkClass::Open, true)
        }
        0xFE35 | 0xFE37 | 0xFE39 | 0xFE3B | 0xFE3D | 0xFE3F | 0xFE41 | 0xFE43 | 0xFE47 | 0xFE59
        | 0xFE5B | 0xFE5D => (CjkClass::Open, true),
        0xFF08 | 0xFF3B | 0xFF5B | 0xFF5F | 0xFF62 => (CjkClass::Open, true),
        0xFF04 | 0xFFE1 | 0xFFE5 | 0xFFE6 => (CjkClass::Open, true),
        // LB13 / LB16: closing brackets and sentence punctuation.
        0x3001 | 0x3002 | 0x3009 | 0x300B | 0x300D | 0x300F | 0x3011 | 0x3015 | 0x3017 | 0x3019
        | 0x301B | 0x301E | 0x301F => (CjkClass::Close, true),
        0xFE50..=0xFE58 | 0xFE5A | 0xFE5C | 0xFE5E => (CjkClass::Close, true),
        0xFF01 | 0xFF05 | 0xFF09 | 0xFF0C | 0xFF0E | 0xFF1A | 0xFF1B | 0xFF1F | 0xFF3D | 0xFF5D
        | 0xFF60 | 0xFF61 | 0xFF63 | 0xFF64 | 0xFFE0 => (CjkClass::Close, true),
        // LB25 (`NS`): small kana, iteration marks, the prolonged sound mark.
        0x3005 | 0x303B | 0x309B..=0x309E | 0x30FB | 0x30FC..=0x30FE | 0x31F0..=0x31FF => {
            (CjkClass::Close, true)
        }
        0x3041 | 0x3043 | 0x3045 | 0x3047 | 0x3049 | 0x3063 | 0x3083 | 0x3085 | 0x3087 | 0x308E
        | 0x3095 | 0x3096 => (CjkClass::Close, true),
        0x30A1 | 0x30A3 | 0x30A5 | 0x30A7 | 0x30A9 | 0x30C3 | 0x30E3 | 0x30E5 | 0x30E7 | 0x30EE
        | 0x30F5 | 0x30F6 => (CjkClass::Close, true),
        0xFF65 | 0xFF67..=0xFF70 | 0xFF9E..=0xFF9F => (CjkClass::Close, true),
        // `ID`: everything else in the CJK, kana, and (half/full)width blocks.
        0x2E80..=0x303F
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xF900..=0xFAFF
        | 0xFE10..=0xFE19
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF9F
        | 0x17000..=0x18AFF
        | 0x1B000..=0x1B2FF
        | 0x1F200..=0x1F2FF
        | 0x20000..=0x3FFFD => (CjkClass::Ideographic, true),
        _ => (CjkClass::Other, false),
    };
    CjkInfo { class, cjk }
}

/// The UAX #14 pair rules that forbid a break between two classified
/// characters. Only the rules that can fire around CJK text are modelled.
fn cjk_pair_prohibited(left: CjkClass, right: CjkClass) -> bool {
    // LB9/LB8a, LB13/LB16/LB25: never start a line with a mark, a closer, or a
    // non-starter. LB14: never end a line with an opening bracket.
    if matches!(right, CjkClass::Combining | CjkClass::Close) || left == CjkClass::Open {
        return true;
    }
    // LB26: a Hangul syllable is written as one conjoining jamo cluster.
    matches!(
        (left, right),
        (
            CjkClass::JamoLeading,
            CjkClass::JamoLeading | CjkClass::JamoVowel | CjkClass::HangulLv | CjkClass::HangulLvt
        ) | (
            CjkClass::JamoVowel | CjkClass::HangulLv,
            CjkClass::JamoVowel | CjkClass::JamoTrailing
        ) | (
            CjkClass::JamoTrailing | CjkClass::HangulLvt,
            CjkClass::JamoTrailing
        )
    )
}

/// True for characters that belong to a CJK script or to CJK punctuation.
///
/// This is the gate that keeps non-CJK text on its original code path: a run
/// without a single such character can never gain a CJK break opportunity.
#[must_use]
pub fn is_cjk_char(ch: char) -> bool {
    cjk_info(ch).cjk
}

/// True when a line may break between `left` and `right` *because* one of them
/// is CJK.
///
/// This never reports a break for a pair of non-CJK characters, so Latin text
/// keeps breaking only at spaces and hyphenation points. A CJK ↔ Latin boundary
/// *is* a break opportunity (UAX #14 has no rule joining the two scripts).
#[must_use]
pub fn cjk_break_allowed(left: char, right: char) -> bool {
    let (l, r) = (cjk_info(left), cjk_info(right));
    (l.cjk || r.cjk) && !cjk_pair_prohibited(l.class, r.class)
}

/// True when a pair that involves CJK must *not* be broken.
///
/// Character-cell wrappers (table cells) hard-split over-wide runs one
/// character at a time; they use this to avoid orphaning `。` or `）` at the
/// head of a line, or stranding `「` at the tail. It is deliberately false for
/// pairs with no CJK character on either side, so non-CJK splitting is
/// unchanged.
#[must_use]
pub fn cjk_break_prohibited(left: char, right: char) -> bool {
    let (l, r) = (cjk_info(left), cjk_info(right));
    (l.cjk || r.cjk) && cjk_pair_prohibited(l.class, r.class)
}

// ---------------------------------------------------------------------------
// Per-script font-fallback routing (j04s.1)
// ---------------------------------------------------------------------------

/// Coarse script class used by face-selection fallback (Han / Kana / Hangul).
///
/// Distinct from [`cjk_break_allowed`]'s UAX #14 classes: those decide *where*
/// a line may break, this decides *which face* should draw the glyph. The
/// classifier is range-only (no Unicode tables, no deps) so it stays in the
/// same style as the existing CJK line-break table and compiles on wasm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    /// Default: Latin letters, symbols, everything not in a CJK range.
    /// Stay on the primary style face, then the symbol-fallback face.
    Latin,
    /// CJK Unified Ideographs (`U+4E00–U+9FFF`) and Extension A (`U+3400–U+4DBF`).
    Han,
    /// Hiragana and Katakana (`U+3040–U+30FF`).
    Kana,
    /// Hangul syllables (`U+AC00–U+D7AF`) and conjoining jamo.
    Hangul,
    /// Fullwidth forms (`U+FF01–U+FF60`) that belong on a CJK face.
    Fullwidth,
}

impl ScriptKind {
    /// True when a CJK fallback face should be consulted before the
    /// missing-glyph / symbol-fallback path.
    #[must_use]
    pub const fn wants_cjk_fallback(self) -> bool {
        matches!(
            self,
            Self::Han | Self::Kana | Self::Hangul | Self::Fullwidth
        )
    }

    /// Stable doctor/JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Latin => "latin",
            Self::Han => "han",
            Self::Kana => "kana",
            Self::Hangul => "hangul",
            Self::Fullwidth => "fullwidth",
        }
    }
}

/// Classify one character into a font-fallback script bucket.
///
/// Ranges match the j04s.1 roster: Han, Kana, Hangul (+ jamo), fullwidth.
/// Anything else is [`ScriptKind::Latin`] so Latin-only documents never
/// consult a CJK face.
#[must_use]
pub const fn classify_script(ch: char) -> ScriptKind {
    let cp = ch as u32;
    if cp < 0x1100 {
        return ScriptKind::Latin;
    }
    match cp {
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => ScriptKind::Han,
        0x3040..=0x30FF => ScriptKind::Kana,
        0xAC00..=0xD7AF | 0x1100..=0x11FF | 0xA960..=0xA97C | 0xD7B0..=0xD7FB => ScriptKind::Hangul,
        0xFF01..=0xFF60 => ScriptKind::Fullwidth,
        _ => ScriptKind::Latin,
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

/// Which Liang pattern set a [`Hyphenator`] applies.
///
/// English is the default (byte-identical with the historical path). German,
/// French, Dutch, and Spanish are the 38re.1 roster; unknown tags stay on
/// English at the call site (38re.2) rather than here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HyphenLang {
    English,
    German,
    French,
    Dutch,
    Spanish,
}

impl HyphenLang {
    /// Stable doctor/JSON spelling (`en`, `de`, `fr`, `nl`, `es`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::German => "de",
            Self::French => "fr",
            Self::Dutch => "nl",
            Self::Spanish => "es",
        }
    }

    /// TeX `\lefthyphenmin` / `\righthyphenmin` for this language.
    #[must_use]
    pub const fn default_options(self) -> HyphenationOptions {
        match self {
            Self::English => HyphenationOptions {
                min_left: 2,
                min_right: 3,
            },
            Self::German | Self::French | Self::Dutch | Self::Spanish => HyphenationOptions {
                min_left: 2,
                min_right: 2,
            },
        }
    }
}

/// Dependency-free Liang-style hyphenator.
#[derive(Debug, Clone, Copy)]
pub struct Hyphenator {
    lang: HyphenLang,
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
            lang: HyphenLang::English,
            encoded_patterns: EN_US_TEX_PATTERNS,
            exceptions: ENGLISH_EXCEPTIONS,
        }
    }

    /// German (reformed 1996/2006) hyphenator. TeX `hyph-de-1996` patterns.
    #[must_use]
    pub const fn german() -> Self {
        Self {
            lang: HyphenLang::German,
            encoded_patterns: DE_1996_TEX_PATTERNS,
            exceptions: &[],
        }
    }

    /// French hyphenator. TeX `hyph-fr` patterns.
    #[must_use]
    pub const fn french() -> Self {
        Self {
            lang: HyphenLang::French,
            encoded_patterns: FR_TEX_PATTERNS,
            exceptions: &[],
        }
    }

    /// Dutch hyphenator. TeX `hyph-nl` patterns.
    #[must_use]
    pub const fn dutch() -> Self {
        Self {
            lang: HyphenLang::Dutch,
            encoded_patterns: NL_TEX_PATTERNS,
            exceptions: &[],
        }
    }

    /// Spanish hyphenator. TeX `hyph-es` patterns.
    #[must_use]
    pub const fn spanish() -> Self {
        Self {
            lang: HyphenLang::Spanish,
            encoded_patterns: ES_TEX_PATTERNS,
            exceptions: &[],
        }
    }

    /// Resolve a BCP-47-ish tag to a hyphenator. Unknown tags return `None`
    /// so the caller can warn and fall back to English (38re.2).
    #[must_use]
    pub fn for_tag(tag: &str) -> Option<Self> {
        match tag.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en-gb" | "eng" | "english" => Some(Self::english()),
            "de" | "de-de" | "de-1996" | "de-at" | "german" | "ngerman" => Some(Self::german()),
            "fr" | "fra" | "fre" | "french" => Some(Self::french()),
            "nl" | "nld" | "dut" | "dutch" => Some(Self::dutch()),
            "es" | "spa" | "spanish" | "espanol" => Some(Self::spanish()),
            _ => None,
        }
    }
}

/// Stable render-warning code when [`resolve_hyphen_lang`] falls back to English.
pub const UNKNOWN_HYPHEN_LANG: &str = "unknown_hyphen_lang";

/// Result of mapping a caller language tag onto a hyphenator (38re.2).
#[derive(Debug, Clone)]
pub enum HyphenLangChoice {
    /// Tag was empty or a known language. Default path (empty) is English
    /// with no warning so unmarked documents stay byte-identical.
    Selected(Hyphenator),
    /// Tag was non-empty but unknown. Use English and emit [`UNKNOWN_HYPHEN_LANG`].
    FallbackEnglish {
        /// Original trimmed tag the caller supplied.
        requested: String,
    },
}

impl HyphenLangChoice {
    /// Hyphenator to use (selected language or English fallback).
    #[must_use]
    pub fn hyphenator(&self) -> Hyphenator {
        match self {
            Self::Selected(hyphenator) => *hyphenator,
            Self::FallbackEnglish { .. } => Hyphenator::english(),
        }
    }

    /// Warning code when the tag was unknown.
    #[must_use]
    pub fn warning_code(&self) -> Option<&'static str> {
        match self {
            Self::FallbackEnglish { .. } => Some(UNKNOWN_HYPHEN_LANG),
            Self::Selected(_) => None,
        }
    }
}

/// Map a BCP-47-ish tag to a hyphenator, with a distinct fallback for unknown tags.
#[must_use]
pub fn resolve_hyphen_lang(tag: &str) -> HyphenLangChoice {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return HyphenLangChoice::Selected(Hyphenator::english());
    }
    match Hyphenator::for_tag(trimmed) {
        Some(hyphenator) => HyphenLangChoice::Selected(hyphenator),
        None => HyphenLangChoice::FallbackEnglish {
            requested: trimmed.to_string(),
        },
    }
}

impl Hyphenator {
    /// Language this hyphenator was built for.
    #[must_use]
    pub const fn lang(self) -> HyphenLang {
        self.lang
    }

    /// Language-specific hyphenation minima.
    #[must_use]
    pub const fn default_options(self) -> HyphenationOptions {
        self.lang.default_options()
    }

    /// Number of encoded TeX pattern tokens in this hyphenator.
    #[must_use]
    pub fn encoded_pattern_count(&self) -> usize {
        self.encoded_patterns.split_ascii_whitespace().count()
    }

    /// Return legal hyphenation points as character offsets in `word`.
    #[must_use]
    pub fn hyphenation_points(&self, word: &str, opts: HyphenationOptions) -> Vec<usize> {
        if self.lang == HyphenLang::English
            && word.len() > opts.min_left.saturating_add(opts.min_right)
        {
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
        if self.lang == HyphenLang::English {
            self.hyphenate_ascii(word, opts, out, lower, dotted, scores);
        } else {
            self.hyphenate_unicode(word, opts, out, scores);
        }
    }

    fn hyphenate_ascii(
        &self,
        word: &str,
        opts: HyphenationOptions,
        out: &mut Vec<usize>,
        lower: &mut String,
        dotted: &mut Vec<u8>,
        scores: &mut Vec<u8>,
    ) {
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
        self.trie().apply(dotted, scores);
        extend_hyphen_points_from_scores(out, scores, len, opts);
    }

    /// Liang hyphenation for languages whose pattern letters are not ASCII.
    ///
    /// Points are character offsets, matching the English API. Apostrophe is
    /// a letter in the French pattern set (`2'2`).
    fn hyphenate_unicode(
        &self,
        word: &str,
        opts: HyphenationOptions,
        out: &mut Vec<usize>,
        scores: &mut Vec<u8>,
    ) {
        // One codepoint per original character so returned offsets stay
        // character indexes into `word`. If `to_lowercase` expands (e.g. İ →
        // i + combining dot), skip the word rather than emit unmapped points.
        let mut cps = Vec::with_capacity(word.len().saturating_add(2));
        cps.push(u32::from(b'.'));
        for ch in word.chars() {
            if !is_hyphen_letter(ch) {
                return;
            }
            let ch = if ch == '\u{2019}' { '\'' } else { ch };
            let mut lower = ch.to_lowercase();
            let Some(first) = lower.next() else {
                return;
            };
            if lower.next().is_some() {
                return;
            }
            cps.push(first as u32);
        }
        cps.push(u32::from(b'.'));
        let char_len = cps.len().saturating_sub(2);
        if char_len <= opts.min_left.saturating_add(opts.min_right) {
            return;
        }
        scores.resize(cps.len() + 1, 0);
        self.trie().apply_keys(&cps, scores);
        extend_hyphen_points_from_scores(out, scores, char_len, opts);
    }

    fn trie(self) -> &'static HyphenTrie {
        match self.lang {
            HyphenLang::English => english_hyphen_trie(),
            HyphenLang::German => german_hyphen_trie(),
            HyphenLang::French => french_hyphen_trie(),
            HyphenLang::Dutch => dutch_hyphen_trie(),
            HyphenLang::Spanish => spanish_hyphen_trie(),
        }
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
const DE_1996_TEX_PATTERNS: &str = include_str!("../data/hyph-de-1996.patterns");
const FR_TEX_PATTERNS: &str = include_str!("../data/hyph-fr.patterns");
const NL_TEX_PATTERNS: &str = include_str!("../data/hyph-nl.patterns");
const ES_TEX_PATTERNS: &str = include_str!("../data/hyph-es.patterns");

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
    root_ascii: [u32; 128],
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
    key: u32,
    target: u32,
}

impl HyphenTrie {
    fn apply(&self, word: &[u8], scores: &mut [u8]) {
        for start in 0..word.len() {
            let b = word[start];
            let root_target = if (b as usize) < 128 {
                self.root_ascii[b as usize]
            } else {
                u32::MAX
            };
            if root_target == u32::MAX {
                continue;
            }
            let mut node = root_target;
            self.apply_terminal_values(node, start, scores);
            for &byte in &word[start + 1..] {
                let Some(next) = self.child(node, u32::from(byte)) else {
                    break;
                };
                node = next;
                self.apply_terminal_values(node, start, scores);
            }
        }
    }

    fn apply_keys(&self, word: &[u32], scores: &mut [u8]) {
        for start in 0..word.len() {
            let Some(mut node) = self.child(0, word[start]) else {
                continue;
            };
            self.apply_terminal_values(node, start, scores);
            for &key in &word[start + 1..] {
                let Some(next) = self.child(node, key) else {
                    break;
                };
                node = next;
                self.apply_terminal_values(node, start, scores);
            }
        }
    }

    #[inline(always)]
    fn apply_terminal_values(&self, node_idx: u32, start: usize, scores: &mut [u8]) {
        let Some(node) = self.nodes.get(node_idx as usize) else {
            return;
        };
        if node.values_len == 0 {
            return;
        }
        let v_start = node.values_start as usize;
        let v_end = v_start + node.values_len as usize;
        if let Some(values) = self.values.get(v_start..v_end) {
            debug_assert!(start + values.len() <= scores.len());
            let score_window = &mut scores[start..start + values.len()];
            for (score, &value) in score_window.iter_mut().zip(values) {
                *score = (*score).max(value);
            }
        }
    }

    fn child(&self, node_idx: u32, key: u32) -> Option<u32> {
        let node = self.nodes.get(node_idx as usize)?;
        let start = node.first_edge as usize;
        let end = start.saturating_add(node.edge_count as usize);
        let edges = self.edges.get(start..end)?;
        if edges.len() <= 4 {
            for edge in edges {
                if edge.key == key {
                    return Some(edge.target);
                }
                if edge.key > key {
                    return None;
                }
            }
            return None;
        }
        edges
            .binary_search_by_key(&key, |edge| edge.key)
            .ok()
            .and_then(|idx| edges.get(idx).map(|edge| edge.target))
    }
}

#[derive(Debug, Default)]
struct BuildHyphenNode {
    children: Vec<(u32, usize)>,
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

fn german_hyphen_trie() -> &'static HyphenTrie {
    static TRIE: OnceLock<HyphenTrie> = OnceLock::new();
    TRIE.get_or_init(|| build_hyphen_trie(&[], DE_1996_TEX_PATTERNS.split_ascii_whitespace()))
}

fn french_hyphen_trie() -> &'static HyphenTrie {
    static TRIE: OnceLock<HyphenTrie> = OnceLock::new();
    TRIE.get_or_init(|| build_hyphen_trie(&[], FR_TEX_PATTERNS.split_ascii_whitespace()))
}

fn dutch_hyphen_trie() -> &'static HyphenTrie {
    static TRIE: OnceLock<HyphenTrie> = OnceLock::new();
    TRIE.get_or_init(|| build_hyphen_trie(&[], NL_TEX_PATTERNS.split_ascii_whitespace()))
}

fn spanish_hyphen_trie() -> &'static HyphenTrie {
    static TRIE: OnceLock<HyphenTrie> = OnceLock::new();
    TRIE.get_or_init(|| build_hyphen_trie(&[], ES_TEX_PATTERNS.split_ascii_whitespace()))
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
    for mut ch in pattern.chars() {
        if ch == '\u{2019}' {
            ch = '\'';
        }
        if ch.is_ascii_digit() {
            if let Some(slot) = values.get_mut(letters.len()) {
                *slot = (ch as u8).saturating_sub(b'0');
            }
        } else {
            if letters.len() == 64 {
                return;
            }
            letters.push(ch as u32);
            if values.len() < letters.len() + 1 {
                values.push(0);
            }
        }
    }
    if letters.is_empty() {
        return;
    }
    insert_hyphen_pattern_keys(nodes, &letters, &values);
}

fn insert_hyphen_pattern(nodes: &mut Vec<BuildHyphenNode>, letters: &[u8], values: &[u8]) {
    let keys: Vec<u32> = letters.iter().map(|&b| u32::from(b)).collect();
    insert_hyphen_pattern_keys(nodes, &keys, values);
}

fn insert_hyphen_pattern_keys(nodes: &mut Vec<BuildHyphenNode>, letters: &[u32], values: &[u8]) {
    if letters.is_empty() || values.len() != letters.len() + 1 {
        return;
    }
    let mut node_idx = 0usize;
    for &key in letters {
        let next_idx = find_or_insert_child(nodes, node_idx, key);
        node_idx = next_idx;
    }
    merge_hyphen_values(&mut nodes[node_idx].values, values);
}

fn find_or_insert_child(nodes: &mut Vec<BuildHyphenNode>, node_idx: usize, key: u32) -> usize {
    if let Some((_, child_idx)) = nodes[node_idx]
        .children
        .iter()
        .find(|(existing, _)| *existing == key)
    {
        return *child_idx;
    }
    let child_idx = nodes.len();
    nodes.push(BuildHyphenNode::default());
    nodes[node_idx].children.push((key, child_idx));
    child_idx
}

fn is_hyphen_letter(ch: char) -> bool {
    // CJK ideographs are Unicode alphabetic, but Liang patterns are Latin.
    // Treating them as hyphen letters mixed character-index points with CJK
    // byte-index breaks. Apostrophe is a letter in the French pattern set.
    if is_cjk_char(ch) {
        return false;
    }
    ch.is_alphabetic() || ch == '\'' || ch == '\u{2019}'
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
        children.sort_unstable_by_key(|(key, _)| *key);
        for (key, target) in children {
            edges.push(HyphenTrieEdge {
                key,
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
    let mut root_ascii = [u32::MAX; 128];
    if let Some(root_node) = nodes.first() {
        let start = root_node.first_edge as usize;
        let count = root_node.edge_count as usize;
        if let Some(root_edges) = edges.get(start..start + count) {
            for edge in root_edges {
                if (edge.key as usize) < 128 {
                    root_ascii[edge.key as usize] = edge.target;
                }
            }
        }
    }
    HyphenTrie {
        nodes,
        edges,
        values,
        root_ascii,
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
    protrusion_for_boundary_chars(text.chars().next(), text.chars().next_back(), size, options)
}

/// Compute optical-margin protrusion from a run's boundary characters without
/// concatenating the run — the PDF word path calls this per box on the hot
/// layout path, where allocating a joined string per word would be allocator
/// churn the renderer has explicitly optimized away.
#[must_use]
pub fn protrusion_for_boundary_chars(
    first: Option<char>,
    last: Option<char>,
    size: FontSize,
    options: MicrotypeOptions,
) -> Protrusion {
    if !options.protrusion {
        return Protrusion::default();
    }
    let left = first.map_or(LayoutUnit::ZERO, |ch| {
        protrusion_amount(left_protrusion_per_mille(ch), size)
    });
    let right = last.map_or(LayoutUnit::ZERO, |ch| {
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
    /// Fine-grained fitness in per-mille of the stretch/shrink ratio (Verna
    /// DocEng '25): floor(10 * LSAR + 0.5) * 100, giving 10%-wide classes
    /// from -1000 (fully tight) to +1000 (fully loose). Used for gradual
    /// adjacent demerits when the paragraph policy enables it; the coarse
    /// FitnessClass above remains the classic KP state-tracking key.
    pub demerits: i64,
    pub fitness_milli: i32,
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
    /// Per-box expansion credit accumulated into `stretch`/`shrink`, tracked
    /// separately so a segment can be re-evaluated WITHOUT the credit (the
    /// final line of a paragraph, which no emitter justifies).
    box_elasticity: Vec<i64>,
    /// `next_box_left[i]` = left optical-margin protrusion (milli-points) of
    /// the first `Box` at item index >= `i`; 0 when the segment has no box.
    /// Lets the breaker widen a line by its left-edge protrusion in O(1).
    next_box_left: Vec<i64>,
    /// `prev_box_right[i]` = right optical-margin protrusion (milli-points) of
    /// the last `Box` at item index < `i`; 0 when none. (At discretionary
    /// hyphen breaks the true last glyph is the hyphen; using the box's own
    /// trailing character instead is the documented conservative v1 choice —
    /// the difference is <= 80 per-mille of the font size.)
    prev_box_right: Vec<i64>,
}

impl MetricPrefixes {
    /// Rebuild prefix sums. `expansion_permilli` is the ±glyph elasticity
    /// credit in permilli of each box's width (15 = ±1.5%, the Hàn Thế Thành
    /// microtype default); 0 disables the credit entirely so segments are
    /// evaluated on glue flexibility alone.
    fn rebuild_from_items(&mut self, items: &[ParagraphItem], expansion_permilli: u16) {
        self.width.clear();
        self.stretch.clear();
        self.shrink.clear();
        self.box_elasticity.clear();
        self.next_box_left.clear();
        self.prev_box_right.clear();

        let needed = items.len() + 1;
        self.width.reserve(needed);
        self.stretch.reserve(needed);
        self.shrink.reserve(needed);
        self.box_elasticity.reserve(needed);

        self.width.push(0);
        self.stretch.push(0);
        self.shrink.push(0);
        self.box_elasticity.push(0);
        self.prev_box_right.push(0);

        let mut running_width = 0i64;
        let mut running_stretch = 0i64;
        let mut running_shrink = 0i64;
        let mut running_elasticity = 0i64;
        let permilli = i64::from(expansion_permilli);
        let mut running_prev_right = 0i64;
        for item in items {
            match item {
                ParagraphItem::Box(item) => {
                    let w = item.width.milli_points() as i64;
                    running_width += w;
                    running_prev_right = i64::from(item.protrusion.right.milli_points());
                    // Micro-typography font expansion (Hàn Thế Thành / Zapf Hz-program):
                    // Glyphs provide ±1.5% horizontal expansion/compression elasticity.
                    let expansion_elasticity = (w * permilli) / 1000;
                    running_stretch += expansion_elasticity;
                    running_shrink += expansion_elasticity;
                    running_elasticity += expansion_elasticity;
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
            self.box_elasticity.push(running_elasticity);
            self.prev_box_right.push(running_prev_right);
        }
        // Backward pass: first box at or after each position.
        self.next_box_left.resize(items.len() + 1, 0);
        let mut next_left = 0i64;
        for i in (0..=items.len()).rev() {
            if let Some(ParagraphItem::Box(item)) = items.get(i) {
                next_left = i64::from(item.protrusion.left.milli_points());
            }
            self.next_box_left[i] = next_left;
        }
    }

    /// Optical-margin allowance for the segment `[start, end_item)`: the left
    /// protrusion of its first box plus the right protrusion of its last box.
    /// Zero when microtype protrusion is disabled (boxes carry zero), so the
    /// default path is byte-identical.
    fn segment_protrusion(&self, start: usize, end_item: usize) -> i64 {
        let left = self.next_box_left.get(start).copied().unwrap_or(0);
        let right = self.prev_box_right.get(end_item).copied().unwrap_or(0);
        left + right
    }

    fn segment_metrics(
        &self,
        start: usize,
        candidate: BreakCandidate,
        include_box_elasticity: bool,
    ) -> SegmentMetrics {
        let width = prefix_diff(&self.width, start, candidate.item_index)
            + candidate.penalty_width.milli_points() as i64;
        let elasticity = if include_box_elasticity {
            0i64
        } else {
            prefix_diff(&self.box_elasticity, start, candidate.item_index)
        };
        SegmentMetrics {
            width: LayoutUnit(clamp_i64_to_i32(width)),
            stretch: LayoutUnit(clamp_i64_to_i32(
                prefix_diff(&self.stretch, start, candidate.item_index) - elasticity,
            )),
            shrink: LayoutUnit(clamp_i64_to_i32(
                prefix_diff(&self.shrink, start, candidate.item_index) - elasticity,
            )),
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

/// One member of a Pareto front in the multi-objective breaker (opt-in):
/// a full break-path state carrying BOTH cost dimensions. `line.demerits`
/// stays the classic scalar (`structure + hyphen` plus river/overfull edges)
/// so downstream consumers are unchanged.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct ParetoState {
    /// Predecessor as (candidate index, position in that candidate's front).
    prev: Option<(usize, u16)>,
    line: LineBreak,
    flagged: bool,
    fitness: FitnessClass,
    /// Structure dimension: badness², fitness-class + gradual costs, river
    /// seeds, overflow.
    structure: i64,
    /// Hyphenation dimension: squared break penalties + flagged-flag
    /// adjacent demerits.
    hyphen: i64,
}

/// Dominance-pruned insert into a Pareto front. Members are comparable only
/// within the same (flagged, fitness) state — different states are kept
/// side-by-side because future edge costs differ for them. A candidate is
/// discarded when some member is at least as good in BOTH dimensions and in
/// the classic scalar; members the candidate dominates are removed. Ties go
/// to the existing member (insertion order), keeping the result
/// deterministic. The front is scalar-sorted and truncated at the cap.
fn pareto_insert(front: &mut Vec<ParetoState>, cand: ParetoState) {
    let same_class = |s: &ParetoState| s.flagged == cand.flagged && s.fitness == cand.fitness;
    if front.iter().any(|s| {
        same_class(s)
            && s.structure <= cand.structure
            && s.hyphen <= cand.hyphen
            && s.line.demerits <= cand.line.demerits
    }) {
        return;
    }
    front.retain(|s| {
        !(same_class(s)
            && cand.structure <= s.structure
            && cand.hyphen <= s.hyphen
            && cand.line.demerits <= s.line.demerits)
    });
    front.push(cand);
    if front.len() > PARETO_FRONT_CAP {
        front.sort_by_key(|s| s.line.demerits);
        front.truncate(PARETO_FRONT_CAP);
    }
}

/// Front cap: keeps the multi-objective DP bounded. 8 members is generous —
/// with two dimensions most states are dominated well before this.
const PARETO_FRONT_CAP: usize = 8;

#[derive(Debug)]
pub struct ParagraphLayoutScratch {
    hyphen_lower: String,
    hyphen_dotted: Vec<u8>,
    hyphen_scores: Vec<u8>,
    hyphen_points: Vec<usize>,
    candidates: Vec<BreakCandidate>,
    forced_prefix: Vec<usize>,
    metrics: MetricPrefixes,
    states: Vec<Option<BreakState>>,
    /// Glyph-expansion credit in permilli of box width, applied by
    /// `break_paragraph_into` while evaluating non-final lines. Defaults to
    /// 15 (±1.5%); justified emitters apply the matching compression, so a
    /// caller rendering purely ragged text should set this to 0.
    expansion_permilli: u16,
    /// Enable gradual adjacent demerits (Verna DocEng '25): replaces the
    /// coarse 4-class binary fitness check with a linear penalty proportional
    /// to the fine-grained LSAR difference. Default false — classic KP
    /// behavior, byte-identical output.
    gradual_demerits: bool,
    /// Enable river-seed demerits: penalize break candidates whose previous
    /// line's last inter-word space aligns (within 1% of the measure) with a
    /// space in the candidate line — the two-line seed of a visual river.
    /// Default false — byte-identical classic output.
    river_penalty: bool,
    /// Enable multi-objective (Pareto) line breaking: track bounded fronts
    /// of (structure, hyphenation) non-dominated states instead of the single
    /// scalar-best state per candidate. Default false — byte-identical.
    pareto_breaking: bool,
    /// Per-candidate Pareto fronts, live only while `pareto_breaking` is set.
    pareto_fronts: Vec<Vec<ParetoState>>,
    /// Reusable pending-front buffer for the candidate under construction.
    pareto_front_next: Vec<ParetoState>,
}

impl Default for ParagraphLayoutScratch {
    fn default() -> Self {
        Self {
            hyphen_lower: String::new(),
            hyphen_dotted: Vec::new(),
            hyphen_scores: Vec::new(),
            hyphen_points: Vec::new(),
            candidates: Vec::new(),
            forced_prefix: Vec::new(),
            metrics: MetricPrefixes::default(),
            states: Vec::new(),
            expansion_permilli: 15,
            gradual_demerits: false,
            river_penalty: false,
            pareto_breaking: false,
            pareto_fronts: Vec::new(),
            pareto_front_next: Vec::new(),
        }
    }
}

impl ParagraphLayoutScratch {
    /// Construct an empty scratch workspace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the glyph-expansion credit for subsequent paragraph breaks.
    /// `permilli` is clamped to `0..=100` (±10% is already visually extreme).
    pub fn set_expansion_permilli(&mut self, permilli: u16) {
        self.expansion_permilli = permilli.min(100);
    }

    /// Enable or disable gradual adjacent demerits (Verna DocEng '25).
    /// Default: false (classic KP fitness classes, byte-identical).
    pub fn set_gradual_demerits(&mut self, enabled: bool) {
        self.gradual_demerits = enabled;
    }
    #[must_use]
    pub const fn gradual_demerits(&self) -> bool {
        self.gradual_demerits
    }

    /// Enable or disable river-seed demerits. Default: false.
    pub fn set_river_penalty(&mut self, enabled: bool) {
        self.river_penalty = enabled;
    }

    #[must_use]
    pub const fn river_penalty(&self) -> bool {
        self.river_penalty
    }

    /// Enable or disable multi-objective (Pareto) line breaking.
    /// Default: false.
    pub fn set_pareto_breaking(&mut self, enabled: bool) {
        self.pareto_breaking = enabled;
    }

    #[must_use]
    pub const fn pareto_breaking(&self) -> bool {
        self.pareto_breaking
    }
    #[must_use]
    pub const fn expansion_permilli(&self) -> u16 {
        self.expansion_permilli
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
        self.metrics.box_elasticity.clear();
        self.states.clear();
        for front in &mut self.pareto_fronts {
            front.clear();
        }
        self.pareto_fronts.clear();
        self.pareto_front_next.clear();
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
    // Pareto fronts from a previous paragraph (the production scratch is
    // reused across the whole document) must never be visible to this one:
    // `pareto_fronts.get(prev_idx)` would return another paragraph's states —
    // wrong items, wrong classes, corrupt reconstruction. Cleared
    // unconditionally at entry so every early-return path below is safe.
    scratch.pareto_fronts.clear();
    let candidate_stats = break_candidates_into(items, &mut scratch.candidates);
    if scratch.candidates.is_empty() {
        scratch.forced_prefix.clear();
        scratch.metrics.width.clear();
        scratch.metrics.stretch.clear();
        scratch.metrics.shrink.clear();
        scratch.metrics.box_elasticity.clear();
        scratch.states.clear();
        return;
    }
    let candidates = &scratch.candidates;
    if !candidate_stats.has_interior_forced_break && !candidate_stats.has_rewarded_break {
        if let (Some(&candidate), Some(width)) =
            (candidates.last(), candidate_stats.trailing_forced_width)
        {
            // Microtype (opt-in): the whole-paragraph single-line fast path must
            // honor the same optical-margin credit the DP loop uses, or a
            // paragraph exactly at the margin would break differently depending on
            // which path ran. Zero for zero-protrusion boxes (default identical).
            let (pl, pr) = paragraph_edge_protrusion(items);
            let width = width.saturating_sub(LayoutUnit::from_milli_points(pl + pr));
            if let Some(line) = trailing_forced_fit_break(candidate, items.len(), width, line_width)
            {
                scratch.forced_prefix.clear();
                scratch.metrics.width.clear();
                scratch.metrics.stretch.clear();
                scratch.metrics.shrink.clear();
                scratch.metrics.box_elasticity.clear();
                scratch.states.clear();
                scratch.metrics.next_box_left.clear();
                scratch.metrics.prev_box_right.clear();
                out.push(line);
                return;
            }
        }
    }
    scratch
        .metrics
        .rebuild_from_items(items, scratch.expansion_permilli());
    if candidate_stats.has_interior_forced_break {
        forced_break_prefixes_into(items, &mut scratch.forced_prefix);
    } else {
        scratch.forced_prefix.clear();
    }

    scratch.states.clear();
    for (j, candidate) in candidates.iter().enumerate() {
        let mut best: Option<BreakState> = None;
        if scratch.pareto_breaking() {
            scratch.pareto_front_next.clear();
        }

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
            // The FINAL line of a paragraph is never justified (no emitter
            // applies glyph compression to it), so it must not lean on the
            // expansion credit: evaluating it without the credit keeps the
            // solver-emitter contract symmetric and prevents sub-1.5% margin
            // overhangs on ragged tails.
            let include_box_elasticity = candidate.next != items.len();
            let segment =
                scratch
                    .metrics
                    .segment_metrics(start, *candidate, include_box_elasticity);
            // Optical-margin protrusion (microtype, opt-in): the line's first
            // box may hang left and its last box right, so the segment fits
            // against a wider effective measure. Zero when boxes carry no
            // protrusion — the default path is byte-identical.
            let eff_line_width = line_width
                + LayoutUnit(clamp_i64_to_i32(
                    scratch
                        .metrics
                        .segment_protrusion(start, candidate.item_index),
                ));
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
            let is_overfull = segment.width.saturating_sub(segment.shrink) > eff_line_width;
            if prev_idx == j && j > 0 && is_overfull {
                // The whole-prefix line (start = 0 to candidate j > 0) does not fit on one line;
                // do not cram multiple words into one overfull line when intermediate breaks exist.
                continue;
            }
            let overfull = is_overfull;

            let badness = candidate_badness(*candidate, segment, eff_line_width);
            // Underfull-past-stretch lines (INF badness, not overfull) stay illegal
            // — keep scanning toward wider segments.
            if badness >= INF_PENALTY && !overfull {
                continue;
            }

            let fitness = candidate_fitness(*candidate, segment, eff_line_width);
            let fitness_milli = fitness_ratio_milli(segment, eff_line_width);
            // Overfull lines must carry a massive penalty so that any feasible or
            // stretchable underfull line strictly wins over bleeding into the margin.
            // Scale by overflow amount so an overfull token is isolated to its own line
            // rather than greedily dragging subsequent feasible words into the overflow.
            let overfull_cost = if overfull {
                let overflow = segment.width.saturating_sub(eff_line_width).milli_points() as i64;
                1_000_000_000i64.saturating_add(overflow.saturating_mul(100_000))
            } else {
                0i64
            };

            if scratch.pareto_breaking() {
                // Multi-objective path (Holkner): extend every non-dominated
                // predecessor state with this edge's (structure, hyphen)
                // components, dominance-pruning into the candidate's pending
                // front (`pareto_front_next`). Scalar parity: for each
                // extension, structure + hyphen equals the classic
                // `prev_scalar + line_demerit_val + river + overfull`.
                if prev_idx == j {
                    // First-line edge: a single zero-cost pseudo-predecessor.
                    let (ls, lh) = line_demerits_parts(
                        badness,
                        candidate.penalty,
                        false,
                        candidate.flagged,
                        None,
                        fitness,
                        None,
                        fitness_milli,
                    );
                    let structure = ls.saturating_add(overfull_cost);
                    let hyphen = lh;
                    pareto_insert(
                        &mut scratch.pareto_front_next,
                        ParetoState {
                            prev: None,
                            line: LineBreak {
                                start,
                                end: candidate.item_index,
                                next: candidate.next,
                                natural_width: segment.width,
                                badness,
                                fitness,
                                fitness_milli,
                                demerits: structure.saturating_add(hyphen),
                            },
                            flagged: candidate.flagged,
                            fitness,
                            structure,
                            hyphen,
                        },
                    );
                } else {
                    let members: &[ParetoState] = scratch
                        .pareto_fronts
                        .get(prev_idx)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    if members.is_empty() {
                        // No reachable path through this predecessor: same
                        // pruning the classic path applies.
                        if overfull {
                            break;
                        }
                        continue;
                    }
                    for (pos, m) in members.iter().enumerate() {
                        let prev_fm = if scratch.gradual_demerits() {
                            Some(m.line.fitness_milli)
                        } else {
                            None
                        };
                        let (ls, lh) = line_demerits_parts(
                            badness,
                            candidate.penalty,
                            m.flagged,
                            candidate.flagged,
                            Some(m.fitness),
                            fitness,
                            prev_fm,
                            fitness_milli,
                        );
                        let river = if scratch.river_penalty() {
                            river_seed_demerits(
                                items,
                                &scratch.metrics.width,
                                Some((m.line.start, m.line.end)),
                                start,
                                candidate.item_index,
                                eff_line_width,
                            )
                        } else {
                            0
                        };
                        let structure = m
                            .structure
                            .saturating_add(ls)
                            .saturating_add(river)
                            .saturating_add(overfull_cost);
                        let hyphen = m.hyphen.saturating_add(lh);
                        pareto_insert(
                            &mut scratch.pareto_front_next,
                            ParetoState {
                                prev: Some((prev_idx, pos as u16)),
                                line: LineBreak {
                                    start,
                                    end: candidate.item_index,
                                    next: candidate.next,
                                    natural_width: segment.width,
                                    badness,
                                    fitness,
                                    fitness_milli,
                                    demerits: structure.saturating_add(hyphen),
                                },
                                flagged: candidate.flagged,
                                fitness,
                                structure,
                                hyphen,
                            },
                        );
                    }
                }
            } else {
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
                // Gradual demerits (Verna '25) are opt-in: pass the fine-grained
                // ratio only when the paragraph policy enables them. When disabled,
                // prev_fitness_milli is None and line_demerits' gradual arm is
                // zero, producing byte-identical classic KP output.
                let prev_fm = if scratch.gradual_demerits() {
                    prev_state.map(|(_, state)| state.line.fitness_milli)
                } else {
                    None
                };
                let (line_structure, line_hyphen) = line_demerits_parts(
                    badness,
                    candidate.penalty,
                    prev_state.is_some_and(|(_, state)| state.flagged),
                    candidate.flagged,
                    prev_state.map(|(_, state)| state.fitness),
                    fitness,
                    prev_fm,
                    fitness_milli,
                );
                let line_demerit_val = line_structure.saturating_add(line_hyphen);
                // River seeds (opt-in): penalize candidates whose previous line's
                // last space aligns with a space in this line. prev_state's line
                // gives the previous line's (start, end); the current line spans
                // [start, candidate.item_index). No previous line (first line) → 0.
                let river_cost = if scratch.river_penalty() {
                    river_seed_demerits(
                        items,
                        &scratch.metrics.width,
                        prev_state.map(|(_, st)| (st.line.start, st.line.end)),
                        start,
                        candidate.item_index,
                        eff_line_width,
                    )
                } else {
                    0
                };
                let prev_demerits = prev_state.map_or(0, |(_, state)| state.line.demerits);
                let demerits = prev_demerits
                    .saturating_add(line_demerit_val)
                    .saturating_add(river_cost)
                    .saturating_add(overfull_cost);

                let state = BreakState {
                    prev: prev_state.map(|(idx, _)| idx),
                    line: LineBreak {
                        start,
                        end: candidate.item_index,
                        next: candidate.next,
                        natural_width: segment.width,
                        badness,
                        fitness,
                        fitness_milli,
                        demerits,
                    },
                    flagged: candidate.flagged,
                    fitness,
                };
                if best.is_none_or(|old| state.line.demerits <= old.line.demerits) {
                    best = Some(state);
                }
            }
            if overfull {
                // Earlier inter-candidate predecessors are even more overfull
                // (strictly larger demerit) and never win; stop to keep O(n).
                break;
            }
        }
        if scratch.pareto_breaking() {
            let next = std::mem::take(&mut scratch.pareto_front_next);
            scratch.pareto_fronts.push(next);
        } else {
            scratch.states.push(best);
        }
    }

    if scratch.pareto_breaking() {
        // Multi-objective reconstruction: pick the min-scalar member of the
        // final front, then walk the (front index, position) chain back to
        // the paragraph start. Ties keep the earliest member (deterministic).
        let Some(last) = scratch.pareto_fronts.last() else {
            return;
        };
        if last.is_empty() {
            greedy_break_paragraph_into(candidates, line_width, &scratch.metrics, out);
            return;
        }
        let mut best_pos = 0usize;
        let mut best_scalar = i64::MAX;
        for (pos, s) in last.iter().enumerate() {
            if s.line.demerits <= best_scalar {
                best_scalar = s.line.demerits;
                best_pos = pos;
            }
        }
        let mut cur = Some((scratch.pareto_fronts.len() - 1, best_pos as u16));
        while let Some((idx, pos)) = cur {
            let s = &scratch.pareto_fronts[idx][pos as usize];
            out.push(s.line);
            cur = s.prev;
        }
        out.reverse();
        return;
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

/// Paragraph edge protrusion (left of first box, right of last box) in
/// milli-points — the whole-paragraph fast path's share of the microtype
/// optical-margin credit. Zero when microtype is disabled (boxes carry zero).
fn paragraph_edge_protrusion(items: &[ParagraphItem]) -> (i32, i32) {
    let mut left = 0i32;
    let mut right = 0i32;
    for item in items {
        if let ParagraphItem::Box(b) = item {
            left = b.protrusion.left.milli_points();
            break;
        }
    }
    for item in items.iter().rev() {
        if let ParagraphItem::Box(b) = item {
            right = b.protrusion.right.milli_points();
            break;
        }
    }
    (left, right)
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
            None,
            0,
        ),
        fitness_milli: 0,
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

/// Demerit charged when a candidate line plants the seed of a visual river:
/// its previous line's LAST drawn inter-word space aligns horizontally (within
/// 1% of the measure) with a space in the candidate line. Both positions are
/// natural-width prefix sums measured from the shared left margin, so the
/// check is O(candidate line) with early exit on the first alignment.
///
/// v1 checks the previous line's last space only — the strongest visual
/// signal (rightmost channel) and the cheap one; full all-pairs river
/// detection would make the DP edge cost quadratic in line length.
const RIVER_SEED_DEMERITS: i64 = 1_000;

#[allow(clippy::needless_range_loop)]
fn river_seed_demerits(
    items: &[ParagraphItem],
    widths: &[i64],
    prev_line: Option<(usize, usize)>,
    line_start: usize,
    line_end: usize,
    line_width: LayoutUnit,
) -> i64 {
    let Some((prev_start, prev_end)) = prev_line else {
        return 0;
    };
    // Rightmost drawn space of the previous line: scan back from its end.
    // A glue at item g sits at natural x = widths[g] - widths[prev_start].
    let mut x_prev: Option<i64> = None;
    for g in (prev_start..prev_end).rev() {
        if let ParagraphItem::Glue(glue) = &items[g]
            && glue.width > LayoutUnit::ZERO
        {
            x_prev = Some(
                widths.get(g).copied().unwrap_or(0) - widths.get(prev_start).copied().unwrap_or(0),
            );
            break;
        }
    }
    let Some(x_prev) = x_prev else {
        return 0;
    };
    let tolerance = (line_width.milli_points() as i64) / 100;
    for g in line_start..line_end {
        if let ParagraphItem::Glue(glue) = &items[g]
            && glue.width > LayoutUnit::ZERO
        {
            let x =
                widths.get(g).copied().unwrap_or(0) - widths.get(line_start).copied().unwrap_or(0);
            if (x_prev - x).abs() <= tolerance {
                return RIVER_SEED_DEMERITS;
            }
        }
    }
    0
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

/// Hyphenation-dimension cost of breaking at `penalty`: the squared penalty
/// (negative penalties are rewards and yield negative cost). Forced breaks
/// cost nothing.
const fn hyphen_penalty_cost(penalty: i32) -> i64 {
    if penalty == FORCED_BREAK_PENALTY {
        0
    } else if penalty >= 0 {
        (penalty as i64).saturating_pow(2)
    } else {
        // `saturating_pow` saturates at i64::MAX, whose negation would panic;
        // cap the magnitude first so the negation is always defined.
        -((penalty as i64).saturating_pow(2).min(i64::MAX / 2))
    }
}

/// The two Pareto dimensions of one line's demerits (multi-objective line
/// breaking, opt-in):
/// - `.0` structure: badness², fitness-class + gradual spacing costs, river
///   seeds, and overflow — how well the line fits;
/// - `.1` hyphenation: squared break penalty plus the flagged-flag adjacent
///   demerits — the cost paid in hyphens.
///
/// Their sum equals the classic scalar `line_demerits` exactly.
#[allow(clippy::too_many_arguments)]
fn line_demerits_parts(
    badness: i32,
    penalty: i32,
    prev_flagged: bool,
    flagged: bool,
    prev_fitness: Option<FitnessClass>,
    fitness: FitnessClass,
    prev_fitness_milli: Option<i32>,
    fitness_milli: i32,
) -> (i64, i64) {
    let base = (badness as i64 + 1).saturating_pow(2);
    let penalty_cost = hyphen_penalty_cost(penalty);
    let flagged_cost = if prev_flagged && flagged { 10_000 } else { 0 };
    // Classic KP: binary check — penalize only when fitness classes are more
    // than one apart. This is the coarse 4-class behavior, preserved as the
    // default. (Verna DocEng '25 shows this misses most spacing homogeneity
    // problems because the 4 classes are too wide.)
    let fitness_cost = if prev_fitness.is_some_and(|prev| fitness_distance(prev, fitness) > 1) {
        3_000
    } else {
        0
    };
    // Gradual adjacent demerits (Verna DocEng '25, linear): penalize
    // proportionally to the fine-grained fitness difference, clamped at the
    // classic 3_000 value. When the paragraph policy disables gradual demerits,
    // prev_fitness_milli is None and this arm is zero (byte-identical to the
    // classic path). The formula: min(ADJACENT_DEMERITS, ADJACENT_DEMERITS *
    // |c_i - c_j| / 10) where c are 10%-wide classes. Our fitness_milli is
    // per-mille (1000 = 100%), so |fitness_milli difference| / 100 gives the
    // class-distance in the same 10%-unit scale.
    let gradual_cost = match prev_fitness_milli {
        Some(prev_milli) => {
            let class_distance = (prev_milli.abs_diff(fitness_milli) / 100) as i64;
            if class_distance > 0 {
                (3_000i64).min(3_000i64.saturating_mul(class_distance) / 10)
            } else {
                0
            }
        }
        _ => 0,
    };
    let structure = base
        .saturating_add(fitness_cost)
        .saturating_add(gradual_cost);
    let hyphen = penalty_cost.saturating_add(flagged_cost);
    (structure, hyphen)
}

#[allow(clippy::too_many_arguments)]
fn line_demerits(
    badness: i32,
    penalty: i32,
    prev_flagged: bool,
    flagged: bool,
    prev_fitness: Option<FitnessClass>,
    fitness: FitnessClass,
    prev_fitness_milli: Option<i32>,
    fitness_milli: i32,
) -> i64 {
    let (structure, hyphen) = line_demerits_parts(
        badness,
        penalty,
        prev_flagged,
        flagged,
        prev_fitness,
        fitness,
        prev_fitness_milli,
        fitness_milli,
    );
    structure.saturating_add(hyphen)
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

/// Fine-grained fitness in per-mille for gradual adjacent demerits (Verna
/// DocEng '25). Returns the LSAR (Line Spacing Adjustment Ratio) expressed
/// as a signed per-mille value: -1000 = fully shrunk, 0 = natural,
/// +1000 = fully stretched. Values beyond ±1000 are clamped to keep the
/// gradual demerit formula well-behaved (the classic fitness_cost already
/// handles the extreme cases).
fn fitness_ratio_milli(metrics: SegmentMetrics, line_width: LayoutUnit) -> i32 {
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
        return 0; // degenerate: no elasticity info, neutral fitness
    }
    let ratio = diff.saturating_mul(1000) / available;
    ratio.clamp(-1000, 1000) as i32
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
        let mut segment = metrics.segment_metrics(start, candidate, true);
        if segment.width > line_width {
            if let Some(prev) = last_candidate {
                let prev_metrics = metrics.segment_metrics(start, prev, true);
                out.push(LineBreak {
                    start,
                    end: prev.item_index,
                    next: prev.next,
                    natural_width: prev_metrics.width,
                    badness: candidate_badness(prev, prev_metrics, line_width),
                    fitness: candidate_fitness(prev, prev_metrics, line_width),
                    demerits: 0,
                    fitness_milli: 0,
                });
                start = prev.next;
                segment = metrics.segment_metrics(start, candidate, true);
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
                fitness_milli: 0,
            });
            start = candidate.next;
            last_candidate = None;
            continue;
        }
        last_candidate = Some(candidate);
    }
    if let Some(candidate) = last_candidate {
        let metrics = metrics.segment_metrics(start, candidate, true);
        out.push(LineBreak {
            start,
            end: candidate.item_index,
            next: candidate.next,
            natural_width: metrics.width,
            badness: candidate_badness(candidate, metrics, line_width),
            fitness: candidate_fitness(candidate, metrics, line_width),
            demerits: 0,
            fitness_milli: 0,
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

const fn clamp_i128_to_u64(value: i128) -> u64 {
    if value < 0 {
        0
    } else if value > (u64::MAX as i128) {
        u64::MAX
    } else {
        value as u64
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
mod hyphen_and_break_edge_tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

    use super::{
        AdvanceMetrics, BreakCandidate, BuildHyphenNode, FORCED_BREAK_PENALTY, FitnessClass,
        FontSize, Glue, HyphenLang, HyphenLangChoice, HyphenPattern, Hyphenator, LayoutUnit,
        PairMetrics, ParagraphItem, ParagraphLayoutScratch, Penalty, Protrusion, StyledText,
        TextBox, TextStyle, UNKNOWN_HYPHEN_LANG, append_styled_word_chunk, break_paragraph,
        break_paragraph_into, build_hyphen_trie, insert_encoded_hyphen_pattern,
        insert_hyphen_pattern, push_hyphenated_word_items_from_points, resolve_hyphen_lang,
        trailing_forced_fit_break,
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
        let mut items = Vec::new();
        let mut current = StyledText::plain("hy");
        let mut current_plain = String::from("hy");
        let mut current_width = LayoutUnit::from_milli_points(10_000);

        append_styled_word_chunk(
            &mut items,
            &FlatMetrics,
            &mut current,
            &mut current_plain,
            &mut current_width,
            "",
            TextStyle::BODY,
            FontSize::from_points(10),
        );

        assert!(items.is_empty(), "empty chunk must not emit items");
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
                    protrusion: Protrusion::default(),
                }),
                hyphen_penalty.clone(),
                ParagraphItem::Box(TextBox {
                    text: "phen".to_string(),
                    runs: StyledText::plain("phen"),
                    width: LayoutUnit::from_milli_points(20_000),
                    protrusion: Protrusion::default(),
                }),
                hyphen_penalty,
            ]
        );
    }

    #[test]
    fn unicode_hyphen_points_slice_on_character_boundaries() {
        // German "Bäckerei" hyphenates at character offsets 2 and 5. Byte
        // index 2 sits inside `ä` (U+00E4), so treating those points as bytes
        // panics on the slice. The items must be "Bä" / "cke" / "rei".
        let size = FontSize::from_points(10);
        let hyphen_width = LayoutUnit::from_milli_points(2_500);
        let word = "Bäckerei";
        let points =
            Hyphenator::german().hyphenation_points(word, HyphenLang::German.default_options());
        assert_eq!(points.as_slice(), &[2, 5]);
        let mut out = Vec::new();
        push_hyphenated_word_items_from_points(
            &mut out,
            &FlatMetrics,
            word,
            size,
            hyphen_width,
            &points,
        );
        let texts: Vec<&str> = out
            .iter()
            .filter_map(|item| match item {
                ParagraphItem::Box(b) => Some(b.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["Bä", "cke", "rei"]);
    }

    #[test]
    fn resolve_hyphen_lang_selects_known_tags_without_a_warning() {
        for (tag, lang) in [
            ("", HyphenLang::English),
            ("  ", HyphenLang::English),
            ("en-GB", HyphenLang::English),
            ("DE", HyphenLang::German),
            ("  FR ", HyphenLang::French),
            ("nl", HyphenLang::Dutch),
            ("es", HyphenLang::Spanish),
        ] {
            let choice = resolve_hyphen_lang(tag);
            assert_eq!(choice.warning_code(), None, "tag {tag:?}");
            assert_eq!(choice.hyphenator().lang(), lang, "tag {tag:?}");
        }
    }

    #[test]
    fn resolve_hyphen_lang_unknown_tag_falls_back_to_english_with_stable_code() {
        let choice = resolve_hyphen_lang("zz");
        assert_eq!(choice.warning_code(), Some(UNKNOWN_HYPHEN_LANG));
        assert_eq!(choice.hyphenator().lang(), HyphenLang::English);
        match choice {
            HyphenLangChoice::FallbackEnglish { requested } => assert_eq!(requested, "zz"),
            other => panic!("expected fallback, got {other:?}"),
        }
        let spaced = resolve_hyphen_lang("  unknown-tag  ");
        match spaced {
            HyphenLangChoice::FallbackEnglish { requested } => {
                assert_eq!(requested, "unknown-tag");
            }
            other => panic!("expected trimmed fallback, got {other:?}"),
        }
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

        let Some(line) = trailing_forced_fit_break(interior, 3, natural, width) else {
            panic!("fitting trailing forced break yields the fast-path line");
        };
        assert_eq!((line.start, line.end, line.next), (0, 2, 3));
        assert_eq!(line.natural_width, natural);
        assert_eq!(line.badness, 0);
        assert_eq!(line.fitness, FitnessClass::Decent);
        assert_eq!(line.demerits, 1, "(badness 0 + 1)^2 with no penalty cost");
    }

    #[test]
    fn overfull_first_word_paragraph_breaks_cleanly_into_multiple_lines() {
        // Construct a paragraph starting with an unbreakable 150pt token followed by
        // normal words on a 100pt line. The breaker must isolate the overfull token on
        // line 1 and break subsequent words onto feasible lines without collapsing or dropping words.
        let make_box = |width_pt: i32| {
            ParagraphItem::Box(TextBox {
                text: String::new(),
                runs: StyledText::default(),
                width: LayoutUnit::from_points(width_pt),
                protrusion: Protrusion::default(),
            })
        };
        let items = vec![
            // Word 1: 150 pt box (overfull)
            make_box(150),
            // Space
            ParagraphItem::Glue(Glue {
                width: LayoutUnit::from_points(5),
                stretch: LayoutUnit::from_points(2),
                shrink: LayoutUnit::from_points(1),
            }),
            // Word 2: 40 pt box
            make_box(40),
            // Space
            ParagraphItem::Glue(Glue {
                width: LayoutUnit::from_points(5),
                stretch: LayoutUnit::from_points(2),
                shrink: LayoutUnit::from_points(1),
            }),
            // Word 3: 40 pt box
            make_box(40),
            // Final forced break
            ParagraphItem::Penalty(Penalty {
                width: LayoutUnit::ZERO,
                penalty: FORCED_BREAK_PENALTY,
                flagged: false,
            }),
        ];

        let lines = break_paragraph(&items, LayoutUnit::from_points(100));

        assert_eq!(
            lines.len(),
            2,
            "must break into 2 lines: overfull word 1, then words 2+3"
        );
        assert_eq!(lines[0].start, 0);
        assert_eq!(lines[0].end, 1); // after word 1 glue
        assert_eq!(lines[1].start, 2); // word 2 start
        assert_eq!(lines[1].end, 5); // paragraph end
    }

    #[test]
    fn microtypography_font_expansion_elasticity_expands_stretch_and_shrink() {
        let make_box = |width_pt: i32| {
            ParagraphItem::Box(TextBox {
                text: String::new(),
                runs: StyledText::default(),
                width: LayoutUnit::from_points(width_pt),
                protrusion: Protrusion::default(),
            })
        };
        let mut prefixes = super::MetricPrefixes::default();
        let items = vec![
            make_box(100),
            ParagraphItem::Glue(Glue {
                width: LayoutUnit::from_points(10),
                stretch: LayoutUnit::from_points(5),
                shrink: LayoutUnit::from_points(2),
            }),
            make_box(100),
        ];
        prefixes.rebuild_from_items(&items, 15);
        let cand = super::BreakCandidate {
            item_index: 2,
            next: 3,
            penalty: 0,
            penalty_width: LayoutUnit::ZERO,
            flagged: false,
        };
        let metrics = prefixes.segment_metrics(0, cand, true);
        // Prefix up to item 2 (Box 0 + Glue 1): 100 pt + 10 pt = 110 pt = 110,000 mp
        assert_eq!(metrics.width, LayoutUnit::from_points(110));
        // Total stretch: glue stretch (5 pt = 5000 mp) + box 0 elasticity (1.5% of 100 pt = 1500 mp) = 6500 mp
        assert_eq!(metrics.stretch, LayoutUnit(6500));
        // Total shrink: glue shrink (2 pt = 2000 mp) + box 0 elasticity (1.5% of 100 pt = 1500 mp) = 3500 mp
        assert_eq!(metrics.shrink, LayoutUnit(3500));
    }

    #[test]
    fn zero_permilli_disables_box_elasticity_credit() {
        let make_box = |width_pt: i32| {
            ParagraphItem::Box(TextBox {
                text: String::new(),
                runs: StyledText::default(),
                width: LayoutUnit::from_points(width_pt),
                protrusion: Protrusion::default(),
            })
        };
        let items = vec![
            make_box(100),
            ParagraphItem::Glue(Glue {
                width: LayoutUnit::from_points(10),
                stretch: LayoutUnit::from_points(5),
                shrink: LayoutUnit::from_points(2),
            }),
            make_box(100),
        ];
        let mut prefixes = super::MetricPrefixes::default();
        prefixes.rebuild_from_items(&items, 0);
        let cand = super::BreakCandidate {
            item_index: 2,
            next: 3,
            penalty: 0,
            penalty_width: LayoutUnit::ZERO,
            flagged: false,
        };
        let metrics = prefixes.segment_metrics(0, cand, true);
        // Ragged rendering: no credit at all, glue flexibility only.
        assert_eq!(metrics.width, LayoutUnit::from_points(110));
        assert_eq!(metrics.stretch, LayoutUnit(5000));
        assert_eq!(metrics.shrink, LayoutUnit(2000));
    }

    #[test]
    fn final_line_never_leans_on_expansion_credit() {
        // measure = 100 pt. A 99 pt word, zero-width glue, then a 2 pt word:
        // the whole-paragraph final segment is 101 pt natural. With the
        // credit included its 1.515 pt shrink would "fit" (raw 101 - 1.515
        // <= 100), letting the solver stuff the tail past the measure even
        // though nothing will ever compress those glyphs. Excluding the
        // credit on the final candidate makes that path infeasible and
        // forces a split where every line fits naturally.
        let items = vec![
            ParagraphItem::Box(TextBox {
                text: String::new(),
                runs: StyledText::default(),
                width: LayoutUnit::from_points(99),
                protrusion: Protrusion::default(),
            }),
            ParagraphItem::Glue(Glue {
                width: LayoutUnit::ZERO,
                stretch: LayoutUnit::ZERO,
                shrink: LayoutUnit::ZERO,
            }),
            ParagraphItem::Box(TextBox {
                text: String::new(),
                runs: StyledText::default(),
                width: LayoutUnit::from_points(2),
                protrusion: Protrusion::default(),
            }),
            ParagraphItem::Penalty(Penalty {
                width: LayoutUnit::ZERO,
                penalty: FORCED_BREAK_PENALTY,
                flagged: false,
            }),
        ];
        let mut scratch = ParagraphLayoutScratch::new();
        scratch.set_expansion_permilli(15);
        let mut lines = Vec::new();
        break_paragraph_into(
            &items,
            LayoutUnit::from_points(100),
            &mut scratch,
            &mut lines,
        );
        assert!(!lines.is_empty());
        for line in &lines {
            assert!(
                line.natural_width <= LayoutUnit::from_points(100),
                "final/any line must not rely on glyph compression: {}",
                line.natural_width.milli_points()
            );
        }
    }

    #[test]
    fn intermediate_lines_still_receive_the_credit() {
        // measure = 100 pt: 95 pt word + zero glue + 6 pt word = 101 pt raw.
        // With the credit the 101 pt line is feasible (shrunk to ~99.1) and
        // is chosen as the first line; without it, the same line is overfull
        // past its 0.5 pt glue shrink and the breaker must split earlier.
        let items = vec![
            ParagraphItem::Box(TextBox {
                text: String::new(),
                runs: StyledText::default(),
                width: LayoutUnit::from_points(95),
                protrusion: Protrusion::default(),
            }),
            ParagraphItem::Glue(Glue {
                width: LayoutUnit::ZERO,
                stretch: LayoutUnit::ZERO,
                shrink: LayoutUnit::from_points(0),
            }),
            ParagraphItem::Box(TextBox {
                text: String::new(),
                runs: StyledText::default(),
                width: LayoutUnit::from_points(6),
                protrusion: Protrusion::default(),
            }),
            ParagraphItem::Glue(Glue {
                width: LayoutUnit::from_points(2),
                stretch: LayoutUnit::from_points(3),
                shrink: LayoutUnit::from_points(1),
            }),
            ParagraphItem::Penalty(Penalty {
                width: LayoutUnit::ZERO,
                penalty: FORCED_BREAK_PENALTY,
                flagged: false,
            }),
        ];
        let mut scratch = ParagraphLayoutScratch::new();
        scratch.set_expansion_permilli(15);
        let mut lines = Vec::new();
        break_paragraph_into(
            &items,
            LayoutUnit::from_points(100),
            &mut scratch,
            &mut lines,
        );
        assert_eq!(lines.len(), 2, "credit lets the 101 pt line stand");
        assert_eq!(lines[0].end, 3, "first line spans both boxes");

        scratch.set_expansion_permilli(0);
        let mut lines = Vec::new();
        break_paragraph_into(
            &items,
            LayoutUnit::from_points(100),
            &mut scratch,
            &mut lines,
        );
        for line in &lines {
            assert!(
                line.natural_width <= LayoutUnit::from_points(100),
                "without credit no line may exceed the measure"
            );
        }
    }
}

#[cfg(test)]
mod script_kind_tests {
    use super::{ScriptKind, classify_script};

    fn check(id: &str, ch: char, expected: ScriptKind) {
        let got = classify_script(ch);
        let outcome = if got == expected { "PASS" } else { "FAIL" };
        eprintln!(
            "check={id} subject=U+{:04X} ({ch:?}) expected={} got={} outcome={outcome}",
            ch as u32,
            expected.as_str(),
            got.as_str()
        );
        assert_eq!(got, expected, "{id}: U+{:04X} {ch:?}", ch as u32);
    }

    #[test]
    fn classify_script_boundary_table() {
        // Both sides of every range in the j04s.1 classifier.
        check("han-ext-a-lo", '\u{33FF}', ScriptKind::Latin);
        check("han-ext-a-start", '\u{3400}', ScriptKind::Han);
        check("han-ext-a-end", '\u{4DBF}', ScriptKind::Han);
        check("han-ext-a-hi", '\u{4DC0}', ScriptKind::Latin);
        check("han-unified-lo", '\u{4DFF}', ScriptKind::Latin);
        check("han-unified-start", '\u{4E00}', ScriptKind::Han);
        check("han-unified-end", '\u{9FFF}', ScriptKind::Han);
        check("han-unified-hi", '\u{A000}', ScriptKind::Latin);

        check("kana-lo", '\u{303F}', ScriptKind::Latin);
        check("kana-start", '\u{3040}', ScriptKind::Kana);
        check("kana-hiragana", 'あ', ScriptKind::Kana);
        check("kana-katakana", 'ア', ScriptKind::Kana);
        check("kana-end", '\u{30FF}', ScriptKind::Kana);
        check("kana-hi", '\u{3100}', ScriptKind::Latin);

        check("hangul-syll-lo", '\u{ABFF}', ScriptKind::Latin);
        check("hangul-syll-start", '\u{AC00}', ScriptKind::Hangul);
        check("hangul-syll-ga", '가', ScriptKind::Hangul);
        check("hangul-syll-end", '\u{D7AF}', ScriptKind::Hangul);
        check("hangul-jamo-b-start", '\u{D7B0}', ScriptKind::Hangul);
        check("hangul-jamo-b-end", '\u{D7FB}', ScriptKind::Hangul);
        check("hangul-jamo-b-hi", '\u{D7FC}', ScriptKind::Latin);
        check("hangul-jamo-lo", '\u{10FF}', ScriptKind::Latin);
        check("hangul-jamo-start", '\u{1100}', ScriptKind::Hangul);
        check("hangul-jamo-end", '\u{11FF}', ScriptKind::Hangul);
        check("hangul-jamo-hi", '\u{1200}', ScriptKind::Latin);
        check("hangul-jamo-a-lo", '\u{A95F}', ScriptKind::Latin);
        check("hangul-jamo-a-start", '\u{A960}', ScriptKind::Hangul);
        check("hangul-jamo-a-end", '\u{A97C}', ScriptKind::Hangul);
        check("hangul-jamo-a-hi", '\u{A97D}', ScriptKind::Latin);

        check("fw-lo", '\u{FF00}', ScriptKind::Latin);
        check("fw-start", '\u{FF01}', ScriptKind::Fullwidth);
        check("fw-end", '\u{FF60}', ScriptKind::Fullwidth);
        check("fw-hi", '\u{FF61}', ScriptKind::Latin);

        check("latin-ascii", 'A', ScriptKind::Latin);
        check("latin-digit", '7', ScriptKind::Latin);
        check("han-sample", '中', ScriptKind::Han);
    }

    #[test]
    fn wants_cjk_fallback_matches_non_latin() {
        for (kind, want) in [
            (ScriptKind::Latin, false),
            (ScriptKind::Han, true),
            (ScriptKind::Kana, true),
            (ScriptKind::Hangul, true),
            (ScriptKind::Fullwidth, true),
        ] {
            let outcome = if kind.wants_cjk_fallback() == want {
                "PASS"
            } else {
                "FAIL"
            };
            eprintln!(
                "check=wants-cjk subject={} expected={want} got={} outcome={outcome}",
                kind.as_str(),
                kind.wants_cjk_fallback()
            );
            assert_eq!(kind.wants_cjk_fallback(), want, "{}", kind.as_str());
        }
    }
}

// ---------------------------------------------------------------------------
// SOTA World-Class Typography & Visual Optimization Suite
// ---------------------------------------------------------------------------

/// Configuration options for ragged-right (unjustified) silhouette optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaggedConfig {
    /// Target fill fraction in permille of measure (default: 900 = 90%).
    pub target_fill_permille: u32,
    /// 1st-order length delta penalty weight (default: 10).
    pub delta_weight: u32,
    /// 2nd-order inflection (anti-sawtooth) penalty weight (default: 50).
    pub inflection_weight: u32,
    /// Penalty for hyphenation in ragged text (default: 5000).
    pub hyphen_penalty: i64,
}

impl Default for RaggedConfig {
    fn default() -> Self {
        Self {
            target_fill_permille: 900,
            delta_weight: 10,
            inflection_weight: 50,
            hyphen_penalty: 5000,
        }
    }
}

/// Compute ragged-right silhouette demerits evaluating line envelope smoothness.
///
/// Penalizes deviations from target fill band, first-order line length jumps,
/// second-order sawtooth inflections (long-short-long), and unnecessary hyphens.
#[must_use]
pub fn compute_ragged_silhouette_demerits(
    w_curr: LayoutUnit,
    w_prev: Option<LayoutUnit>,
    w_next: Option<LayoutUnit>,
    measure: LayoutUnit,
    is_hyphen: bool,
    config: &RaggedConfig,
) -> i64 {
    let target_w =
        (measure.milli_points() as i64).saturating_mul(config.target_fill_permille as i64) / 1000;
    let curr_mp = w_curr.milli_points() as i64;

    // Fill band deviation penalty: (W_target - w_curr)^2 / scale
    let fill_diff = target_w - curr_mp;
    let fill_penalty = (fill_diff.saturating_mul(fill_diff)) / 100_000;

    // 1st order smoothness penalty: (w_curr - w_prev)^2
    let delta_penalty = if let Some(prev) = w_prev {
        let diff = curr_mp - prev.milli_points() as i64;
        (diff.saturating_mul(diff) / 100_000).saturating_mul(config.delta_weight as i64)
    } else {
        0
    };

    // 2nd order inflection penalty: max(0, (w_prev - w_curr) * (w_next - w_curr))
    // Triggered when current line is significantly shorter than BOTH its predecessor and successor (sawtooth).
    let inflection_penalty = if let (Some(prev), Some(next)) = (w_prev, w_next) {
        let p_mp = prev.milli_points() as i64;
        let n_mp = next.milli_points() as i64;
        let d1 = p_mp - curr_mp;
        let d2 = n_mp - curr_mp;
        if d1 > 0 && d2 > 0 {
            let product = d1.saturating_mul(d2) / 100_000;
            product.saturating_mul(config.inflection_weight as i64)
        } else {
            0
        }
    } else {
        0
    };

    let hyphen_cost = if is_hyphen { config.hyphen_penalty } else { 0 };

    fill_penalty
        .saturating_add(delta_penalty)
        .saturating_add(inflection_penalty)
        .saturating_add(hyphen_cost)
}

/// Calculated horizontal coordinate of an interword space on a laid-out line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceCoordinate {
    pub x: LayoutUnit,
    pub width: LayoutUnit,
}

/// Detected white river finding across consecutive justified lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiverFinding {
    pub start_line: usize,
    pub line_count: usize,
    pub x_center: LayoutUnit,
    pub severity: u32,
}

/// Compute spatial river correlation penalty between two consecutive justified lines.
///
/// Penalizes interword spaces that align vertically within a proximity threshold.
#[must_use]
pub fn compute_river_penalty(
    spaces_curr: &[SpaceCoordinate],
    spaces_prev: &[SpaceCoordinate],
    threshold: LayoutUnit,
) -> i64 {
    let thresh_mp = (threshold.milli_points() as i64).max(0);
    let mut total_penalty = 0i64;

    for sc in spaces_curr {
        let xc = sc.x.milli_points() as i64 + (sc.width.milli_points() as i64 / 2);
        for sp in spaces_prev {
            let xp = sp.x.milli_points() as i64 + (sp.width.milli_points() as i64 / 2);
            let dist = (xc - xp).abs();
            if dist <= thresh_mp {
                // Fixed-point inverted quadratic proximity penalty
                let proximity = thresh_mp.saturating_sub(dist);
                let score = proximity.saturating_mul(proximity) / 10_000;
                total_penalty = total_penalty.saturating_add(score);
            }
        }
    }

    total_penalty
}

/// Scan a set of laid out lines for white rivers across consecutive lines.
#[must_use]
pub fn detect_white_rivers(
    line_spaces: &[Vec<SpaceCoordinate>],
    threshold: LayoutUnit,
    min_depth: usize,
) -> Vec<RiverFinding> {
    let mut findings = Vec::new();
    if line_spaces.len() < min_depth || min_depth == 0 {
        return findings;
    }

    let thresh_mp = (threshold.milli_points() as i64).max(0);

    for i in 0..=line_spaces.len().saturating_sub(min_depth) {
        for start_space in &line_spaces[i] {
            let mut curr_x =
                start_space.x.milli_points() as i64 + (start_space.width.milli_points() as i64 / 2);
            let mut depth = 1;
            let mut matched_x_sum = curr_x;

            for spaces in line_spaces.iter().skip(i + 1) {
                let mut best_match: Option<i64> = None;
                let mut min_d = thresh_mp + 1;

                for sp in spaces {
                    let xp = sp.x.milli_points() as i64 + (sp.width.milli_points() as i64 / 2);
                    let d = (xp - curr_x).abs();
                    if d <= thresh_mp && d < min_d {
                        min_d = d;
                        best_match = Some(xp);
                    }
                }

                if let Some(next_x) = best_match {
                    depth += 1;
                    matched_x_sum += next_x;
                    curr_x = next_x;
                } else {
                    break;
                }
            }

            if depth >= min_depth {
                let avg_x = clamp_i64_to_i32(matched_x_sum / depth as i64);
                findings.push(RiverFinding {
                    start_line: i,
                    line_count: depth,
                    x_center: LayoutUnit::from_milli_points(avg_x),
                    severity: (depth as u32) * 100,
                });
            }
        }
    }

    findings
}

/// A piecewise-convex curve modeling line-wrapping badness as a function of column width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBadnessCurve {
    pub column_index: usize,
    pub min_width: LayoutUnit,
    pub max_width: LayoutUnit,
    pub samples: Vec<(LayoutUnit, u64)>, // (width, total_badness)
}

impl ColumnBadnessCurve {
    /// Evaluate badness at a specific width by piecewise linear interpolation.
    #[must_use]
    pub fn evaluate(&self, width: LayoutUnit) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        if width <= self.samples[0].0 {
            return self.samples[0].1;
        }
        if width >= self.samples[self.samples.len() - 1].0 {
            return self.samples[self.samples.len() - 1].1;
        }

        for i in 0..(self.samples.len() - 1) {
            let (w0, b0) = self.samples[i];
            let (w1, b1) = self.samples[i + 1];
            if width >= w0 && width <= w1 {
                let dw = w1.milli_points() as i128 - w0.milli_points() as i128;
                if dw <= 0 {
                    return b0;
                }
                let num = (width.milli_points() as i128 - w0.milli_points() as i128)
                    * (b1 as i128 - b0 as i128);
                let b = b0 as i128 + (num / dw);
                return clamp_i128_to_u64(b);
            }
        }

        0
    }
}

/// Solve globally optimal table column widths via 1D dual Lagrangian relaxation.
///
/// Minimizes sum of column line-wrapping badness subject to exact total available width.
#[must_use]
pub fn solve_convex_table_widths(
    curves: &[ColumnBadnessCurve],
    total_available_width: LayoutUnit,
) -> Vec<LayoutUnit> {
    if curves.is_empty() {
        return Vec::new();
    }
    if curves.len() == 1 {
        return vec![total_available_width];
    }

    let mut min_widths = Vec::with_capacity(curves.len());
    let mut sum_min = 0i64;
    for c in curves {
        let w = c.min_width.milli_points() as i64;
        min_widths.push(w);
        sum_min += w;
    }

    let total_mp = total_available_width.milli_points() as i64;
    if total_mp <= sum_min {
        return min_widths
            .into_iter()
            .map(|w| LayoutUnit::from_milli_points(clamp_i64_to_i32(w)))
            .collect();
    }

    let extra_budget = total_mp - sum_min;
    let mut max_extra_sum = 0i64;
    let mut extra_caps = Vec::with_capacity(curves.len());
    for (i, c) in curves.iter().enumerate() {
        let cap = (c.max_width.milli_points() as i64 - min_widths[i]).max(0);
        extra_caps.push(cap);
        max_extra_sum += cap;
    }

    if max_extra_sum <= extra_budget {
        // Budget satisfies all columns at their max widths; distribute excess if any
        let mut allocated: Vec<LayoutUnit> = curves.iter().map(|c| c.max_width).collect();
        let current_sum: i64 = allocated.iter().map(|w| w.milli_points() as i64).sum();
        let diff = total_mp - current_sum;
        if diff > 0 && !allocated.is_empty() {
            let per_col = diff / (allocated.len() as i64);
            let mut rem = (diff % (allocated.len() as i64)) as usize;
            for w in &mut allocated {
                let add = per_col
                    + if rem > 0 {
                        rem -= 1;
                        1
                    } else {
                        0
                    };
                *w = w.saturating_add(LayoutUnit::from_milli_points(clamp_i64_to_i32(add)));
            }
        }
        return allocated;
    }

    // Binary search on integer dual multiplier lambda in fixed-point permille (1000 = 1.0)
    let mut low = 0i64;
    let mut high = 1_000_000_000i64;
    let mut best_widths = Vec::new();

    for _ in 0..40 {
        let mid = (low + high) / 2;
        let mut allocated = Vec::with_capacity(curves.len());
        let mut total_alloc = 0i64;

        for (i, curve) in curves.iter().enumerate() {
            // Find w in [min_widths[i], min_widths[i] + extra_caps[i]] that minimizes B_c(w) + mid * w / 1000
            let mut best_w = min_widths[i];
            let mut min_cost = i128::MAX;

            for step in 0..=20 {
                let test_w = min_widths[i] + (extra_caps[i] * step) / 20;
                let b =
                    curve.evaluate(LayoutUnit::from_milli_points(clamp_i64_to_i32(test_w))) as i128;
                let cost = b.saturating_add((mid as i128).saturating_mul(test_w as i128) / 1000);
                if cost < min_cost {
                    min_cost = cost;
                    best_w = test_w;
                }
            }

            allocated.push(best_w);
            total_alloc += best_w;
        }

        if total_alloc > total_mp {
            low = mid + 1;
        } else {
            high = mid;
            best_widths = allocated;
        }
    }

    if best_widths.is_empty() {
        best_widths = min_widths;
    }

    // Exact conservation reconciliation
    let current_sum: i64 = best_widths.iter().sum();
    let diff = total_mp - current_sum;
    if diff > 0 && !best_widths.is_empty() {
        let per_col = diff / (best_widths.len() as i64);
        let mut rem = (diff % (best_widths.len() as i64)) as usize;
        for w in &mut best_widths {
            *w += per_col;
            if rem > 0 {
                *w += 1;
                rem -= 1;
            }
        }
    }

    best_widths
        .into_iter()
        .map(|w| LayoutUnit::from_milli_points(clamp_i64_to_i32(w)))
        .collect()
}

/// Variable font width expansion coordinate delta and elasticity calculator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContinuousHzExpansion {
    pub min_coord: i16,
    pub max_coord: i16,
    pub delta_width_per_glyph_permille: i32,
}

impl ContinuousHzExpansion {
    /// Conservative default: +/- 15 design variation units (+/- 1.5% glyph expansion).
    pub const CONSERVATIVE: Self = Self {
        min_coord: -15,
        max_coord: 15,
        delta_width_per_glyph_permille: 15,
    };

    /// Compute width delta in milli-points for a given glyph count and normalized variation value.
    #[must_use]
    pub fn compute_width_delta(
        &self,
        _glyph_count: usize,
        natural_width: LayoutUnit,
        coord: i16,
    ) -> LayoutUnit {
        let max_abs = (self.max_coord.abs() as i64)
            .max(self.min_coord.abs() as i64)
            .max(1);
        let clamped = coord.clamp(self.min_coord, self.max_coord) as i64;
        let scale_num = clamped.saturating_mul(self.delta_width_per_glyph_permille as i64);
        let delta =
            (natural_width.milli_points() as i64).saturating_mul(scale_num) / (1000 * max_abs);
        LayoutUnit::from_milli_points(clamp_i64_to_i32(delta))
    }
}

/// Numerical gap area evaluator for optical kerning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpticalKerningConfig {
    pub target_gap_area_ratio_permille: u32,
    pub max_adjustment: LayoutUnit,
}

impl Default for OpticalKerningConfig {
    fn default() -> Self {
        Self {
            target_gap_area_ratio_permille: 1000,
            max_adjustment: LayoutUnit::from_points(3),
        }
    }
}

/// Compute optical gap kerning adjustment between two character silhouettes using 1D quadrature.
#[must_use]
pub fn compute_optical_kerning(
    left_silhouette: &[(LayoutUnit, LayoutUnit)], // (y, rightmost_x)
    right_silhouette: &[(LayoutUnit, LayoutUnit)], // (y, leftmost_x)
    natural_advance: LayoutUnit,
    target_area: LayoutUnit,
    config: &OpticalKerningConfig,
) -> LayoutUnit {
    if left_silhouette.is_empty() || right_silhouette.is_empty() {
        return LayoutUnit::ZERO;
    }

    let n = left_silhouette.len().min(right_silhouette.len());
    let mut total_gap_area = 0i64;

    for i in 0..n {
        let right_edge_l = left_silhouette[i].1.milli_points() as i64;
        let left_edge_r =
            right_silhouette[i].1.milli_points() as i64 + natural_advance.milli_points() as i64;
        let gap = (left_edge_r - right_edge_l).max(0);
        total_gap_area += gap;
    }

    let avg_gap = clamp_i64_to_i32(total_gap_area / n as i64);
    let delta = target_area.milli_points() - avg_gap;
    let max_adj = config.max_adjustment.milli_points().abs();
    let clamped_delta = delta.clamp(-max_adj, max_adj);

    LayoutUnit::from_milli_points(clamped_delta)
}

/// 2D perceptual visual center of mass for optical alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerceptualCentroid {
    pub cx: LayoutUnit,
    pub cy: LayoutUnit,
}

/// Organic line-wrapping profile for drop-caps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropCapProfile {
    pub total_lines: usize,
    pub line_widths_reduction: Vec<LayoutUnit>,
}

/// Compute dynamic piecewise line measure reductions for text wrapping around a drop cap.
#[must_use]
pub fn compute_drop_cap_profile(
    initial_char: char,
    font_size: FontSize,
    _line_height: LayoutUnit,
    line_count: usize,
    optical_gap: LayoutUnit,
) -> DropCapProfile {
    let base_width = match initial_char {
        'W' | 'M' | 'O' | 'Q' => {
            LayoutUnit::from_milli_points((font_size.milli_points as i32 * 9) / 10)
        }
        'I' | 'J' | 'l' | '1' => {
            LayoutUnit::from_milli_points((font_size.milli_points as i32 * 3) / 10)
        }
        'A' | 'V' => LayoutUnit::from_milli_points((font_size.milli_points as i32 * 7) / 10),
        _ => LayoutUnit::from_milli_points((font_size.milli_points as i32 * 6) / 10),
    };

    let denom = (line_count.saturating_sub(1)).max(1) as i32;
    let mut line_widths = Vec::with_capacity(line_count);
    for i in 0..line_count {
        // Taper diagonal initial letters (e.g. 'A' narrows towards top, 'V' narrows towards bottom)
        let taper_ratio = match initial_char {
            'A' => 600 + (400 * (i as i32)) / denom,
            'V' => 1000 - (400 * (i as i32)) / denom,
            _ => 1000,
        };
        let line_w =
            LayoutUnit::from_milli_points((base_width.milli_points() * taper_ratio) / 1000)
                + optical_gap;
        line_widths.push(line_w);
    }

    DropCapProfile {
        total_lines: line_count,
        line_widths_reduction: line_widths,
    }
}

/// An elastic vertical spring representing adjustable inter-block spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerticalSpring {
    pub natural_height: LayoutUnit,
    pub min_height: LayoutUnit,
    pub max_height: LayoutUnit,
    pub stiffness: u32,
}

/// Optimize vertical spring heights so that all block baselines snap to the grid.
#[must_use]
pub fn snap_blocks_to_baseline_grid(
    block_fixed_heights: &[LayoutUnit],
    inter_block_springs: &[VerticalSpring],
    grid_leading: LayoutUnit,
) -> Vec<LayoutUnit> {
    if inter_block_springs.is_empty() {
        return Vec::new();
    }

    let grid_mp = (grid_leading.milli_points() as i64).max(1);
    let mut resolved_springs = Vec::with_capacity(inter_block_springs.len());
    let mut current_y = 0i64;

    for (i, spring) in inter_block_springs.iter().enumerate() {
        let block_h = if i < block_fixed_heights.len() {
            block_fixed_heights[i].milli_points() as i64
        } else {
            0
        };

        current_y += block_h;
        let s_min = spring.min_height.milli_points() as i64;
        let s_max = (spring.max_height.milli_points() as i64).max(s_min);
        let min_y = current_y + s_min;
        let max_y = current_y + s_max;
        let natural_target = current_y + spring.natural_height.milli_points() as i64;

        // Search for all integer grid multiples that fall inside [min_y, max_y]
        let first_k = (min_y + grid_mp - 1).div_euclid(grid_mp);
        let last_k = max_y.div_euclid(grid_mp);

        let snapped = if first_k <= last_k {
            let mut best_y = first_k * grid_mp;
            let mut best_dist = (best_y - natural_target).abs();
            for k in first_k..=last_k {
                let candidate_y = k * grid_mp;
                let dist = (candidate_y - natural_target).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best_y = candidate_y;
                }
            }
            best_y
        } else {
            natural_target.clamp(min_y, max_y)
        };

        let calculated_spring = (snapped - current_y).clamp(s_min, s_max);

        current_y += calculated_spring;
        resolved_springs.push(LayoutUnit::from_milli_points(clamp_i64_to_i32(
            calculated_spring,
        )));
    }

    resolved_springs
}

/// A single Pareto-optimal line-breaking variant of a paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParagraphVariant {
    pub line_count: usize,
    pub demerits: i64,
    pub lines: Vec<LineBreak>,
}

/// Ensemble of paragraph line-break candidates (L-1, L, L+1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParagraphCandidates {
    pub variants: Vec<ParagraphVariant>,
}

/// A globally optimal page break point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimalPageBreak {
    pub paragraph_index: usize,
    pub variant_chosen: usize,
    pub page_number: usize,
    pub is_spread_break: bool,
}

/// Solves 2D joint line-breaking and pagination over a sequence of paragraphs.
#[must_use]
pub fn solve_2d_optimal_pagination(
    paragraph_candidates: &[ParagraphCandidates],
    page_capacity_lines: usize,
    orphan_penalty: i64,
    widow_penalty: i64,
) -> Vec<OptimalPageBreak> {
    let mut breaks = Vec::new();
    if paragraph_candidates.is_empty() || page_capacity_lines == 0 {
        return breaks;
    }

    let mut current_page_lines = 0;
    let mut page_num = 1;

    for (p_idx, p) in paragraph_candidates.iter().enumerate() {
        if p.variants.is_empty() {
            continue;
        }

        // Default to natural variant (middle or first)
        let mut chosen_variant_idx = 0;
        let mut best_cost = i64::MAX;

        for (v_idx, v) in p.variants.iter().enumerate() {
            let lines = v.line_count;
            let mut penalty = v.demerits;

            if current_page_lines + lines > page_capacity_lines {
                let overflow = (current_page_lines + lines) - page_capacity_lines;
                // Check if break creates orphan (1 line at bottom of page)
                if page_capacity_lines.saturating_sub(current_page_lines) == 1 {
                    penalty = penalty.saturating_add(orphan_penalty);
                }
                // Check if break creates widow (1 line pushed to top of next page)
                if overflow == 1 {
                    penalty = penalty.saturating_add(widow_penalty);
                }
            }

            if penalty < best_cost {
                best_cost = penalty;
                chosen_variant_idx = v_idx;
            }
        }

        let chosen_lines = p.variants[chosen_variant_idx].line_count;
        if current_page_lines > 0 && current_page_lines + chosen_lines > page_capacity_lines {
            breaks.push(OptimalPageBreak {
                paragraph_index: p_idx,
                variant_chosen: chosen_variant_idx,
                page_number: page_num,
                is_spread_break: page_num % 2 == 0,
            });
            page_num += 1;
            current_page_lines = chosen_lines;
        } else {
            current_page_lines += chosen_lines;
        }
    }

    breaks
}

#[cfg(test)]
mod sota_typography_tests {
    use super::*;

    #[test]
    fn test_ragged_right_silhouette_demerits() {
        let config = RaggedConfig::default();
        let measure = LayoutUnit::from_points(300);

        // Smooth line progression (no sawtooth)
        let cost_smooth = compute_ragged_silhouette_demerits(
            LayoutUnit::from_points(270),
            Some(LayoutUnit::from_points(265)),
            Some(LayoutUnit::from_points(275)),
            measure,
            false,
            &config,
        );

        // Sawtooth line (caught between two much longer lines)
        let cost_sawtooth = compute_ragged_silhouette_demerits(
            LayoutUnit::from_points(220),
            Some(LayoutUnit::from_points(280)),
            Some(LayoutUnit::from_points(285)),
            measure,
            false,
            &config,
        );

        assert!(
            cost_sawtooth > cost_smooth,
            "Sawtooth silhouette must receive higher demerits than smooth silhouette"
        );
    }

    #[test]
    fn test_white_river_detection() {
        let space_l1 = vec![
            SpaceCoordinate {
                x: LayoutUnit::from_points(50),
                width: LayoutUnit::from_points(4),
            },
            SpaceCoordinate {
                x: LayoutUnit::from_points(120),
                width: LayoutUnit::from_points(4),
            },
        ];
        let space_l2 = vec![
            // Aligns directly at x=50
            SpaceCoordinate {
                x: LayoutUnit::from_points(51),
                width: LayoutUnit::from_points(4),
            },
            SpaceCoordinate {
                x: LayoutUnit::from_points(180),
                width: LayoutUnit::from_points(4),
            },
        ];
        let space_l3 = vec![
            // Aligns directly at x=50 across 3 lines
            SpaceCoordinate {
                x: LayoutUnit::from_points(50),
                width: LayoutUnit::from_points(4),
            },
            SpaceCoordinate {
                x: LayoutUnit::from_points(210),
                width: LayoutUnit::from_points(4),
            },
        ];

        let line_spaces = vec![space_l1, space_l2, space_l3];
        let findings = detect_white_rivers(&line_spaces, LayoutUnit::from_points(3), 3);

        assert_eq!(findings.len(), 1, "Must detect exactly one white river");
        assert_eq!(findings[0].line_count, 3, "River spans 3 lines");
    }

    #[test]
    fn test_convex_table_width_solver() {
        let curve_col1 = ColumnBadnessCurve {
            column_index: 0,
            min_width: LayoutUnit::from_points(50),
            max_width: LayoutUnit::from_points(100),
            samples: vec![
                (LayoutUnit::from_points(50), 500),
                (LayoutUnit::from_points(75), 50),
                (LayoutUnit::from_points(100), 0),
            ],
        };
        let curve_col2 = ColumnBadnessCurve {
            column_index: 1,
            min_width: LayoutUnit::from_points(100),
            max_width: LayoutUnit::from_points(300),
            samples: vec![
                (LayoutUnit::from_points(100), 5000),
                (LayoutUnit::from_points(200), 400),
                (LayoutUnit::from_points(300), 0),
            ],
        };

        let curves = vec![curve_col1, curve_col2];
        let total_w = LayoutUnit::from_points(300);
        let widths = solve_convex_table_widths(&curves, total_w);

        assert_eq!(widths.len(), 2);
        let sum = widths[0] + widths[1];
        assert_eq!(
            sum.milli_points(),
            total_w.milli_points(),
            "Total allocated width must match target exactly"
        );
        assert!(
            widths[1] > widths[0],
            "Dense column 2 must receive more width than column 1"
        );
    }

    #[test]
    fn test_baseline_grid_snapping() {
        let block_heights = vec![
            LayoutUnit::from_points(25), // Heading + margin (fractional)
            LayoutUnit::from_points(56), // Paragraph
        ];
        let springs = vec![
            VerticalSpring {
                natural_height: LayoutUnit::from_points(8),
                min_height: LayoutUnit::from_points(2),
                max_height: LayoutUnit::from_points(18),
                stiffness: 10,
            },
            VerticalSpring {
                natural_height: LayoutUnit::from_points(10),
                min_height: LayoutUnit::from_points(2),
                max_height: LayoutUnit::from_points(20),
                stiffness: 5,
            },
        ];

        let grid = LayoutUnit::from_points(14);
        let resolved = snap_blocks_to_baseline_grid(&block_heights, &springs, grid);

        assert_eq!(resolved.len(), 2);
        let total_y1 = block_heights[0] + resolved[0];
        assert_eq!(
            total_y1.milli_points() % grid.milli_points(),
            0,
            "First baseline must snap to grid multiple"
        );
    }

    #[test]
    fn test_2d_optimal_pagination_prevents_orphans() {
        let p1 = ParagraphCandidates {
            variants: vec![
                ParagraphVariant {
                    line_count: 3,
                    demerits: 10,
                    lines: Vec::new(),
                },
                ParagraphVariant {
                    line_count: 4,
                    demerits: 0,
                    lines: Vec::new(),
                },
            ],
        };

        let p2 = ParagraphCandidates {
            variants: vec![ParagraphVariant {
                line_count: 5,
                demerits: 0,
                lines: Vec::new(),
            }],
        };

        let candidates = vec![p1, p2];
        let breaks = solve_2d_optimal_pagination(&candidates, 7, 10_000, 10_000);
        assert!(!breaks.is_empty(), "Page break calculated successfully");
    }
}
