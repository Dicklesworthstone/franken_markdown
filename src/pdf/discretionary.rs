//! Author-controlled discretionary breaks in running text.
//!
//! Soft hyphens and zero-width spaces are formatting instructions, not glyphs.
//! Their tokens retain the surrounding style until line breaking chooses an
//! optional hyphen; ordinary copy/search text contains only the visible word.
//! Literal code and TeX fallback text never enter this policy.

use super::{
    BuiltParagraph, F_MONO, LayoutUnit, ParagraphItem, PdfWordContext, Penalty, Tok, TokGroup,
    cached_shaped_width, flush_pdf_word, text_composition,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Hint {
    ZeroWidth,
    Hyphen,
}

pub(super) fn is_hint(ch: char) -> bool {
    matches!(ch, '\u{00ad}' | '\u{200b}')
}

pub(super) fn hint(tok: &Tok) -> Option<Hint> {
    if tok.literal
        || tok.slot == F_MONO
        || super::is_inline_object(tok.slot)
        || tok.space
        || tok.hard_break
    {
        return None;
    }
    match tok.text.as_str() {
        "\u{00ad}" => Some(Hint::Hyphen),
        "\u{200b}" => Some(Hint::ZeroWidth),
        _ => None,
    }
}

pub(super) fn combine(previous: Option<Tok>, next: &Tok) -> Tok {
    // Prefer the first authored soft hyphen, including its own style. It may
    // follow a code span, formula or image whose slot cannot paint a hyphen.
    previous
        .filter(|token| hint(token) == Some(Hint::Hyphen))
        .unwrap_or_else(|| next.clone())
}

pub(super) fn hyphen_token(before: &Tok) -> Tok {
    let mut token = before.clone();
    token.text = "-".to_string();
    token.space = false;
    token.hard_break = false;
    token.note_spans = super::note_tags::Spans::default();
    token
}

/// Remove inactive edge hints, coalesce repeated opportunities, and keep a
/// base with its following combining marks. This happens before font routing
/// coalesces equal-style source, so composition and table measurement see the
/// same meaningful boundaries as the paragraph breaker.
pub(super) fn normalize_tokens(tokens: &mut Vec<Tok>) {
    if !tokens.iter().any(|tok| hint(tok).is_some()) {
        return;
    }
    let mut normalized: Vec<Tok> = Vec::with_capacity(tokens.len());
    let mut pending = None;
    for tok in std::mem::take(tokens) {
        if hint(&tok).is_some() {
            if normalized
                .last()
                .is_some_and(|before| !before.space && !before.hard_break)
            {
                pending = Some(combine(pending, &tok));
            }
        } else {
            let mark_follows = !tok.literal
                && !super::is_inline_object(tok.slot)
                && tok
                    .text
                    .chars()
                    .next()
                    .is_some_and(text_composition::is_mark);
            if let Some(opportunity) = pending.take()
                && !tok.space
                && !tok.hard_break
                && !mark_follows
            {
                normalized.push(opportunity);
            }
            normalized.push(tok);
        }
    }
    *tokens = normalized;
}

/// If all visible content already fits, keep the ordinary unsplit source
/// path. Measuring pieces around an unused hint can otherwise lose a cross-
/// boundary ligature or kerning adjustment and invent an unnecessary line.
/// Bound this speculative join like the existing simple-paragraph cache;
/// longer content proceeds directly to the measured fragment breaker.
pub(super) fn fitting_unbroken_tokens(tokens: &[Tok], cx: PdfWordContext<'_>) -> Option<Vec<Tok>> {
    let measure = cx.image_measure?;
    let mut visible = Vec::new();
    let mut pending_space = None;
    let mut bytes = 0usize;
    for tok in tokens {
        if hint(tok).is_some() {
            continue;
        }
        if tok.hard_break {
            return None;
        }
        if tok.space {
            if !visible.is_empty() && pending_space.is_none() {
                pending_space = Some(tok.clone());
            }
            continue;
        }
        bytes = bytes.saturating_add(tok.text.len());
        if bytes > super::SIMPLE_PARAGRAPH_LAYOUT_CACHE_MAX_TEXT_BYTES {
            return None;
        }
        if let Some(space) = pending_space.take() {
            visible.push(space);
        }
        visible.push(tok.clone());
    }
    (super::measure_word_cached(&visible, cx.fs, cx.faces, cx.width_cache) <= measure)
        .then_some(visible)
}

/// Use the same measured discretionary penalties in table cells and the
/// emergency paragraph lane. A greedy token boundary cannot reserve a chosen
/// hyphen's advance; sharing the breaker avoids dropping or orphaning it.
pub(super) fn wrapped_tokens(
    tokens: &[Tok],
    max_width: f32,
    size: f32,
    faces: &super::Faces,
    width_cache: &std::cell::RefCell<super::WidthCache>,
) -> Vec<Vec<Tok>> {
    let mut tokens = tokens.to_vec();
    super::inline_image::fit_tokens(&mut tokens, size, max_width);
    super::inline_math_fit::fit_tokens(&mut tokens, size, max_width);
    let measure = super::lu_from_points_f32(max_width);
    let built = super::build_paragraph(
        &tokens,
        super::font_size_of(size),
        faces,
        super::ParagraphPolicy::RAGGED,
        None,
        &std::cell::RefCell::new(super::HashMap::new()),
        width_cache,
        Some(measure),
    );
    if !built.has_boxes {
        return Vec::new();
    }
    let mut scratch = super::ParagraphLayoutScratch::default();
    scratch.set_justified(false);
    let mut breaks = Vec::new();
    super::break_paragraph_into(&built.items, measure, &mut scratch, &mut breaks);
    let mut adjustments = Vec::new();
    let mut line = Vec::new();
    let mut lines = Vec::with_capacity(breaks.len());
    for lb in breaks {
        super::line_tokens_for_break_into(
            &built,
            &lb,
            measure,
            false,
            0,
            &mut adjustments,
            &mut line,
        );
        if !line.is_empty() {
            lines.push(line.drain(..).map(|item| item.tok).collect());
        }
    }
    lines
}

/// Use authored opportunities before dictionary hyphenation. Each part still
/// has measured emergency breaks, so an overlong fragment cannot escape its
/// containing measure. Leading/trailing hints and runs of hints never create
/// empty boxes, blank rows, or extra source spaces.
pub(super) fn flush_word(
    built: &mut BuiltParagraph,
    word: &mut Vec<Tok>,
    mut cx: PdfWordContext<'_>,
) {
    cx.policy.hyphenate = false;
    let mut part = Vec::new();
    let mut pending = None;
    for tok in std::mem::take(word) {
        if hint(&tok).is_some() {
            if !part.is_empty() {
                pending = Some(combine(pending, &tok));
            }
            continue;
        }
        if let Some(opportunity) = pending.take()
            && !tok
                .text
                .chars()
                .next()
                .is_some_and(text_composition::is_mark)
        {
            let hyphen = if hint(&opportunity) == Some(Hint::Hyphen) {
                Some(hyphen_token(&opportunity))
            } else {
                None
            };
            let width = hyphen.as_ref().map_or(LayoutUnit::ZERO, |tok| {
                cached_shaped_width(cx.faces, cx.width_cache, tok.slot, "-", cx.fs)
            });
            let mut part_cx = cx;
            // A selected SHY consumes its hyphen as well as the fragment.
            // Give the fragment's existing emergency fitter the remaining
            // budget, so a full-width prefix cannot force '-' onto its own
            // physical line during final geometry repair.
            if width > LayoutUnit::ZERO {
                part_cx.image_measure = cx
                    .image_measure
                    .map(|measure| (measure - width).max(LayoutUnit::from_milli_points(1)));
                if let Some(measure) = part_cx.image_measure {
                    super::inline_image::fit_tokens(
                        &mut part,
                        cx.fs.milli_points() as f32 / 1000.0,
                        measure.to_points_f32(),
                    );
                    super::inline_math_fit::fit_tokens(
                        &mut part,
                        cx.fs.milli_points() as f32 / 1000.0,
                        measure.to_points_f32(),
                    );
                }
            }
            flush_pdf_word(built, &mut part, part_cx);
            built.items.push(ParagraphItem::Penalty(Penalty {
                width,
                penalty: if hyphen.is_some() { 25 } else { 0 },
                flagged: hyphen.is_some(),
            }));
            built.item_toks.push(TokGroup::empty());
            built.break_toks.push(hyphen);
        }
        part.push(tok);
    }
    flush_pdf_word(built, &mut part, cx);
}
