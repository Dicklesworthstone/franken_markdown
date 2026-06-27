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

use crate::text::Font;

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
    pub text: String,
    pub width: LayoutUnit,
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
    let mut items = Vec::new();
    let mut words = text.split_whitespace().peekable();
    let space = measure_text_with_pairs(metrics, " ", size);
    while let Some(word) = words.next() {
        items.push(ParagraphItem::Box(TextBox {
            text: word.to_string(),
            width: measure_text_with_pairs(metrics, word, size),
        }));
        if words.peek().is_some() {
            items.push(ParagraphItem::Glue(default_interword_glue(space)));
        }
    }
    items.push(ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: FORCED_BREAK_PENALTY,
        flagged: false,
    }));
    items
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
    let mut words = text.split_whitespace().peekable();
    let space = measure_text_with_pairs(metrics, " ", size);
    let hyphen_width = measure_text_with_pairs(metrics, "-", size);
    while let Some(word) = words.next() {
        push_hyphenated_word_items(&mut items, metrics, hyphenator, word, size, hyphen_width);
        if words.peek().is_some() {
            items.push(ParagraphItem::Glue(default_interword_glue(space)));
        }
    }
    items.push(ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: FORCED_BREAK_PENALTY,
        flagged: false,
    }));
    items
}

fn push_hyphenated_word_items<M: PairMetrics>(
    out: &mut Vec<ParagraphItem>,
    metrics: &M,
    hyphenator: &Hyphenator,
    word: &str,
    size: FontSize,
    hyphen_width: LayoutUnit,
) {
    let points = hyphenator.hyphenation_points(word, HyphenationOptions::default());
    if points.is_empty() {
        out.push(ParagraphItem::Box(TextBox {
            text: word.to_string(),
            width: measure_text_with_pairs(metrics, word, size),
        }));
        return;
    }

    let mut start = 0usize;
    for point in points {
        let end = byte_index_for_char_boundary(word, point);
        if end > start {
            let part = &word[start..end];
            out.push(ParagraphItem::Box(TextBox {
                text: part.to_string(),
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
            width: measure_text_with_pairs(metrics, part, size),
        }));
    }
}

fn byte_index_for_char_boundary(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    s.char_indices()
        .nth(char_idx)
        .map_or_else(|| s.len(), |(idx, _)| idx)
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
    patterns: &'static [HyphenPattern],
    exceptions: &'static [HyphenException],
}

impl Hyphenator {
    /// English starter hyphenator. It includes a compact pattern subset plus
    /// high-value exceptions for documentation-heavy words; full TeX pattern
    /// import remains a follow-up data expansion, not an algorithm change.
    #[must_use]
    pub const fn english() -> Self {
        Self {
            patterns: ENGLISH_STARTER_PATTERNS,
            exceptions: ENGLISH_EXCEPTIONS,
        }
    }

    /// Return legal hyphenation points as character offsets in `word`.
    #[must_use]
    pub fn hyphenation_points(&self, word: &str, opts: HyphenationOptions) -> Vec<usize> {
        if !word.chars().all(|c| c.is_ascii_alphabetic()) {
            return Vec::new();
        }
        let lower = word.to_ascii_lowercase();
        let len = lower.chars().count();
        if len <= opts.min_left.saturating_add(opts.min_right) {
            return Vec::new();
        }
        if let Some(exception) = self
            .exceptions
            .iter()
            .find(|exception| exception.word == lower)
        {
            return exception
                .points
                .iter()
                .copied()
                .filter(|&p| legal_hyphen_point(p, len, opts))
                .collect();
        }

        let dotted = format!(".{lower}.");
        let dotted_chars = dotted.chars().collect::<Vec<_>>();
        let mut scores = vec![0u8; dotted_chars.len() + 1];
        for pattern in self.patterns {
            apply_hyphen_pattern(pattern, &dotted_chars, &mut scores);
        }
        scores
            .iter()
            .enumerate()
            .filter_map(|(idx, &score)| {
                let point = idx.checked_sub(1)?;
                if score % 2 == 1 && legal_hyphen_point(point, len, opts) {
                    Some(point)
                } else {
                    None
                }
            })
            .collect()
    }
}

fn legal_hyphen_point(point: usize, len: usize, opts: HyphenationOptions) -> bool {
    point >= opts.min_left && len.saturating_sub(point) >= opts.min_right
}

fn apply_hyphen_pattern(pattern: &HyphenPattern, word: &[char], scores: &mut [u8]) {
    let letters = pattern.letters.chars().collect::<Vec<_>>();
    if letters.is_empty() || pattern.values.len() != letters.len() + 1 {
        return;
    }
    for start in 0..=word.len().saturating_sub(letters.len()) {
        if word[start..start + letters.len()] == letters {
            for (offset, &value) in pattern.values.iter().enumerate() {
                let idx = start + offset;
                if let Some(score) = scores.get_mut(idx) {
                    *score = (*score).max(value);
                }
            }
        }
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
    natural_width.saturating_sub(protrusion_for_text(text, size, options).total())
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

#[derive(Debug, Clone, Copy)]
struct SegmentMetrics {
    width: LayoutUnit,
    stretch: LayoutUnit,
    shrink: LayoutUnit,
}

#[derive(Debug, Clone, Copy)]
struct BreakState {
    prev: Option<usize>,
    line: LineBreak,
    flagged: bool,
    fitness: FitnessClass,
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
    let candidates = break_candidates(items);
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut states: Vec<Option<BreakState>> = vec![None; candidates.len()];
    for (j, candidate) in candidates.iter().enumerate() {
        let mut best: Option<BreakState> = None;

        for prev_idx in 0..=j {
            let (prev_state, start) = if prev_idx == j {
                (None, 0)
            } else {
                let Some(state) = states[prev_idx] else {
                    continue;
                };
                (Some((prev_idx, state)), candidates[prev_idx].next)
            };
            if start > candidate.item_index {
                continue;
            }
            let metrics = segment_metrics(items, start, *candidate);
            let badness = candidate_badness(*candidate, metrics, line_width);
            let fitness = candidate_fitness(*candidate, metrics, line_width);
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
                    natural_width: metrics.width.saturating_add(candidate.penalty_width),
                    badness,
                    fitness,
                    demerits,
                },
                flagged: candidate.flagged,
                fitness,
            };
            if best.is_none_or(|old| state.line.demerits < old.line.demerits) {
                best = Some(state);
            }
        }
        states[j] = best;
    }

    let Some(mut idx) = states.len().checked_sub(1) else {
        return Vec::new();
    };
    if states[idx].is_none() {
        return greedy_break_paragraph(items, line_width);
    }
    let mut out = Vec::new();
    while let Some(state) = states[idx] {
        out.push(state.line);
        match state.prev {
            Some(prev) => idx = prev,
            None => break,
        }
    }
    out.reverse();
    out
}

fn break_candidates(items: &[ParagraphItem]) -> Vec<BreakCandidate> {
    let mut out = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        match item {
            ParagraphItem::Glue(_) => out.push(BreakCandidate {
                item_index: idx,
                next: idx + 1,
                penalty: 0,
                penalty_width: LayoutUnit::ZERO,
                flagged: false,
            }),
            ParagraphItem::Penalty(p) if p.penalty < INF_PENALTY => out.push(BreakCandidate {
                item_index: idx,
                next: idx + 1,
                penalty: p.penalty,
                penalty_width: p.width,
                flagged: p.flagged,
            }),
            ParagraphItem::Penalty(_) | ParagraphItem::Box(_) => {}
        }
    }
    out
}

fn segment_metrics(
    items: &[ParagraphItem],
    start: usize,
    candidate: BreakCandidate,
) -> SegmentMetrics {
    let mut metrics = SegmentMetrics {
        width: LayoutUnit::ZERO,
        stretch: LayoutUnit::ZERO,
        shrink: LayoutUnit::ZERO,
    };
    for item in &items[start..candidate.item_index] {
        match item {
            ParagraphItem::Box(b) => metrics.width += b.width,
            ParagraphItem::Glue(g) => {
                metrics.width += g.width;
                metrics.stretch += g.stretch;
                metrics.shrink += g.shrink;
            }
            ParagraphItem::Penalty(_) => {}
        }
    }
    metrics.width += candidate.penalty_width;
    metrics
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

fn greedy_break_paragraph(items: &[ParagraphItem], line_width: LayoutUnit) -> Vec<LineBreak> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut last_candidate: Option<BreakCandidate> = None;
    for candidate in break_candidates(items) {
        let metrics = segment_metrics(items, start, candidate);
        if metrics.width > line_width {
            if let Some(prev) = last_candidate {
                let prev_metrics = segment_metrics(items, start, prev);
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
            }
        }
        last_candidate = Some(candidate);
    }
    if let Some(candidate) = last_candidate {
        let metrics = segment_metrics(items, start, candidate);
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
    out
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
