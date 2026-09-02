//! Document analysis: structural counts, readability metrics, and link health
//! over a parsed Markdown AST.
//!
//! All metrics are pure functions of the AST — no clocks, no randomness, no
//! hash-order leaks — so a fixed input document always yields a byte-identical
//! [`analysis_json`] report.
//!
//! ## Prose scope
//!
//! "Prose" is the text of [`Block::Paragraph`] and [`Block::Heading`] blocks,
//! walked recursively through block quotes, list items, and footnote
//! definitions. Code blocks, code spans, math, raw HTML, tables, and footnote
//! reference markers never contribute to prose metrics. Inside prose inlines,
//! emphasis/strong/strikethrough recurse, link content and image alt text
//! count, and excluded inlines contribute a single space separator so words
//! are never concatenated across a dropped span.
//!
//! ## Readability
//!
//! * `reading_time_secs`: `word_count / 200` words-per-minute, expressed in
//!   seconds with round-half-up integer math (`(words * 60 + 100) / 200`).
//! * Sentences: a sentence ends at `.`, `!`, or `?` followed by whitespace or
//!   end of text (per-block text is joined with spaces, so a terminator at the
//!   end of a paragraph counts). A document with words but no terminator
//!   counts as one sentence.
//! * Syllables use a deterministic vowel-group heuristic (see
//!   [`count_syllables`]); it is an estimator, documented rather than precise.
//! * `flesch_reading_ease`: `206.835 - 1.015*(words/sentences) - 84.6*(syllables/words)`
//!   over prose only; `0.0` when there are no prose words. Stored rounded to
//!   one decimal so both the field and its JSON form are byte-stable.
//!
//! ## Heading anchors
//!
//! Broken internal-anchor detection compares `#fragment` link targets against
//! the heading `id`s the HTML renderer emits. The assignment algorithm below
//! mirrors `src/html.rs` (`slug_inlines`, `push_slug_char`,
//! `push_heading_id_from_inlines`, and the block/footnote walk order) exactly:
//! same lowercase/dash slugging, same empty-slug `section` fallback, same
//! `-N` collision suffixes, same render order (body walk with block quotes and
//! list items recursed, footnote definitions rendered afterwards in
//! first-reference order, unreferenced definitions omitted). Anchor comparison
//! is exact and case-sensitive on the raw fragment (no percent-decoding).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use franken_markdown::ast::{Block, Document, Inline};

/// Aggregated structural, readability, and link-health metrics for one document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Analysis {
    /// Words in prose (paragraph and heading text; see module docs).
    pub word_count: usize,
    /// Estimated silent reading time in seconds at 200 words per minute,
    /// rounded half-up.
    pub reading_time_secs: u32,
    /// Total heading count (all levels).
    pub heading_count: usize,
    /// Headings per level: index 0 = level 1, …, index 5 = level 6.
    pub heading_depth_histogram: [u32; 6],
    /// Total fenced/indented code blocks.
    pub code_blocks: usize,
    /// Code-block counts by info-string language, sorted by language name.
    pub code_languages: BTreeMap<String, usize>,
    /// Total GFM pipe tables.
    pub table_count: usize,
    /// Total inline images.
    pub image_count: usize,
    /// Images whose alt text is empty or whitespace-only.
    pub images_missing_alt: usize,
    /// Link totals and internal-anchor health.
    pub links: LinkAnalysis,
    /// Flesch Reading Ease over prose only, rounded to one decimal.
    /// Higher = easier to read; `0.0` when the document has no prose words.
    pub flesch_reading_ease: f32,
}

/// Link inventory and broken-internal-anchor detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkAnalysis {
    /// All inline links.
    pub total: usize,
    /// Links whose destination starts with `#` (same-document anchors).
    pub internal: usize,
    /// Links with any other destination.
    pub external: usize,
    /// Internal links whose fragment matches no emitted heading anchor.
    pub broken_anchor_count: usize,
}

/// Analyze a parsed document into structural, readability, and link metrics.
#[must_use]
pub fn analyze_document(doc: &Document) -> Analysis {
    let anchors = collect_heading_anchors(&doc.blocks);

    let mut analysis = Analysis::default();
    let mut prose = String::new();
    collect_block_metrics(&doc.blocks, &mut analysis, &anchors, &mut prose);

    let words: Vec<&str> = prose
        .split_whitespace()
        .filter(|token| token.chars().any(char::is_alphanumeric))
        .collect();
    analysis.word_count = words.len();
    analysis.reading_time_secs = reading_time_secs(words.len());

    let syllables: usize = words.iter().map(|word| count_syllables(word)).sum();
    let mut sentences = count_sentences(&prose);
    if sentences == 0 && !words.is_empty() {
        // A run of prose with no terminator is still one sentence.
        sentences = 1;
    }
    analysis.flesch_reading_ease = if words.is_empty() {
        0.0
    } else {
        let words_f = words.len() as f64;
        #[allow(clippy::cast_precision_loss)]
        let raw =
            206.835 - 1.015 * (words_f / sentences as f64) - 84.6 * (syllables as f64 / words_f);
        round_one_decimal(raw) as f32
    };
    analysis
}

/// Render an [`Analysis`] as compact JSON, schema `fmd-analyze-v1`.
///
/// Key order is fixed, map keys are sorted (BTreeMap), and the one float is
/// emitted with exactly one decimal — the output is byte-identical for a
/// fixed input on every platform.
#[must_use]
pub fn analysis_json(analysis: &Analysis) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("{\"schema\":\"fmd-analyze-v1\"");
    push_json_usize(&mut out, "word_count", analysis.word_count);
    push_json_usize(
        &mut out,
        "reading_time_secs",
        analysis.reading_time_secs as usize,
    );
    push_json_usize(&mut out, "heading_count", analysis.heading_count);
    out.push_str(",\"heading_depth_histogram\":[");
    for (i, count) in analysis.heading_depth_histogram.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{count}");
    }
    out.push(']');
    push_json_usize(&mut out, "code_blocks", analysis.code_blocks);
    out.push_str(",\"code_languages\":{");
    for (i, (lang, count)) in analysis.code_languages.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, lang);
        let _ = write!(out, ":{count}");
    }
    out.push('}');
    push_json_usize(&mut out, "table_count", analysis.table_count);
    push_json_usize(&mut out, "image_count", analysis.image_count);
    push_json_usize(&mut out, "images_missing_alt", analysis.images_missing_alt);
    out.push_str(",\"links\":{");
    let _ = write!(out, "\"total\":{}", analysis.links.total);
    let _ = write!(out, ",\"internal\":{}", analysis.links.internal);
    let _ = write!(out, ",\"external\":{}", analysis.links.external);
    let _ = write!(
        out,
        ",\"broken_anchor_count\":{}",
        analysis.links.broken_anchor_count
    );
    out.push('}');
    out.push_str(",\"flesch_reading_ease\":");
    push_f32_one_decimal(&mut out, analysis.flesch_reading_ease);
    out.push('}');
    out
}

/// `word_count / 200` words per minute, in seconds, rounded half-up.
#[inline(always)]
fn reading_time_secs(word_count: usize) -> u32 {
    let secs = (word_count as u64).saturating_mul(60).saturating_add(100) / 200;
    u32::try_from(secs).unwrap_or(u32::MAX)
}

/// Round half away from zero to one decimal, in f64.
#[inline(always)]
fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Emit an f32 with exactly one decimal; normalizes negative zero to `0.0`.
#[inline(always)]
fn push_f32_one_decimal(out: &mut String, value: f32) {
    let rounded = round_one_decimal(f64::from(value));
    if rounded == 0.0 {
        out.push_str("0.0");
    } else {
        let _ = write!(out, "{rounded:.1}");
    }
}

#[inline(always)]
fn push_json_usize(out: &mut String, key: &str, value: usize) {
    let _ = write!(out, ",\"{key}\":{value}");
}

/// Minimal JSON string escaping (quotes, backslash, controls).
fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------------------
// Block/inline metric collection
// ---------------------------------------------------------------------------

/// Recursive block walk: structural counts, prose text, and inline metrics.
/// Footnote definitions contribute prose and inline metrics like any other
/// block container (their text is real document prose).
fn collect_block_metrics(
    blocks: &[Block],
    analysis: &mut Analysis,
    anchors: &BTreeSet<String>,
    prose: &mut String,
) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                analysis.heading_count += 1;
                let idx = usize::from(level.saturating_sub(1)).min(5);
                analysis.heading_depth_histogram[idx] += 1;
                push_prose_text(inlines, prose);
                prose.push(' ');
                collect_inline_metrics(inlines, analysis, anchors);
            }
            Block::Paragraph(inlines) => {
                push_prose_text(inlines, prose);
                prose.push(' ');
                collect_inline_metrics(inlines, analysis, anchors);
            }
            Block::CodeBlock { lang, .. } => {
                analysis.code_blocks += 1;
                if let Some(lang) = lang {
                    *analysis.code_languages.entry(lang.clone()).or_insert(0) += 1;
                }
            }
            Block::Table(table) => {
                analysis.table_count += 1;
                for cell in &table.head {
                    collect_inline_metrics(cell, analysis, anchors);
                }
                for row in &table.rows {
                    for cell in row {
                        collect_inline_metrics(cell, analysis, anchors);
                    }
                }
            }
            Block::BlockQuote(inner) => collect_block_metrics(inner, analysis, anchors, prose),
            Block::List(list) => {
                for item in &list.items {
                    collect_block_metrics(&item.blocks, analysis, anchors, prose);
                }
            }
            Block::FootnoteDefinition { blocks, .. } => {
                collect_block_metrics(blocks, analysis, anchors, prose);
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for term in &item.terms {
                        collect_inline_metrics(term, analysis, anchors);
                    }
                    for def in &item.definitions {
                        collect_inline_metrics(def, analysis, anchors);
                    }
                }
            }
            Block::ThematicBreak | Block::HtmlBlock(_) | Block::MathBlock(_) | Block::PageBreak => {
            }
        }
    }
}

/// Recursive inline walk for link and image metrics.
fn collect_inline_metrics(inlines: &[Inline], analysis: &mut Analysis, anchors: &BTreeSet<String>) {
    for inl in inlines {
        match inl {
            Inline::Link { dest, content, .. } => {
                analysis.links.total += 1;
                if let Some(fragment) = dest.strip_prefix('#') {
                    analysis.links.internal += 1;
                    if !anchors.contains(fragment) {
                        analysis.links.broken_anchor_count += 1;
                    }
                } else {
                    analysis.links.external += 1;
                }
                collect_inline_metrics(content, analysis, anchors);
            }
            Inline::Image { alt, .. } => {
                analysis.image_count += 1;
                if alt.trim().is_empty() {
                    analysis.images_missing_alt += 1;
                }
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                collect_inline_metrics(inner, analysis, anchors);
            }
            _ => {}
        }
    }
}

/// Extract prose text from inlines: literal text and image alt count;
/// emphasis/strong/strikethrough and link content recurse; soft/hard breaks
/// become spaces. Code spans, math, raw HTML, and footnote reference markers
/// are excluded, contributing a single space so adjacent words never merge.
fn push_prose_text(inlines: &[Inline], out: &mut String) {
    for inl in inlines {
        match inl {
            Inline::Text(t) => out.push_str(t),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                push_prose_text(inner, out)
            }
            Inline::Link { content, .. } => push_prose_text(content, out),
            Inline::SoftBreak
            | Inline::HardBreak
            | Inline::Code(_)
            | Inline::Math(_)
            | Inline::DisplayMath(_)
            | Inline::Html(_)
            | Inline::FootnoteRef { .. } => out.push(' '),
        }
    }
}

/// Count sentence terminators: `.`, `!`, or `?` followed by whitespace or
/// end of text.
fn count_sentences(text: &str) -> usize {
    let mut count = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|next| next.is_whitespace()) {
            count += 1;
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Syllable heuristic
// ---------------------------------------------------------------------------

/// Deterministic syllable estimator (documented heuristic, not a dictionary):
///
/// 1. Lowercase and keep ASCII letters only; a word with no letters (digits,
///    punctuation) contributes zero syllables.
/// 2. Words of three letters or fewer count as one syllable.
/// 3. Count vowel *groups* — maximal runs of vowels. Vowels are `a e i o u`,
///    plus `y` when it directly follows a consonant (`happy`, `rhythm`).
/// 4. Silent trailing `e`: subtract one group when the word ends in `e`, has
///    more than one group, and does not end in `ee`/`ye` or consonant + `le`
///    (`table` keeps its final group, `make` loses it).
/// 5. Every word with letters has at least one syllable.
#[inline(always)]
fn count_syllables(word: &str) -> usize {
    let mut buf = [0u8; 64];
    let mut n = 0usize;
    for &b in word.as_bytes() {
        if b.is_ascii_alphabetic() && n < buf.len() {
            buf[n] = b.to_ascii_lowercase();
            n += 1;
        }
    }
    if n == 0 {
        return 0;
    }
    if n <= 3 {
        return 1;
    }
    let is_vowel_byte = |b: u8| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u');
    let is_vowel_at = |i: usize| -> bool {
        is_vowel_byte(buf[i])
            || (buf[i] == b'y' && i > 0 && !is_vowel_byte(buf[i - 1]) && buf[i - 1] != b'y')
    };
    let mut groups = 0usize;
    let mut prev_vowel = false;
    for i in 0..n {
        let vowel = is_vowel_at(i);
        if vowel && !prev_vowel {
            groups += 1;
        }
        prev_vowel = vowel;
    }
    if buf[n - 1] == b'e'
        && groups > 1
        && !matches!(buf[n - 2], b'e' | b'y')
        && !(buf[n - 2] == b'l' && n >= 3 && !is_vowel_byte(buf[n - 3]))
    {
        groups -= 1;
    }
    groups.max(1)
}

// ---------------------------------------------------------------------------
// Heading-anchor mirror of src/html.rs
// ---------------------------------------------------------------------------

/// Compute the exact set of heading `id`s the HTML renderer emits for these
/// blocks. Mirrors `html.rs`: body walk first (block quotes and list items
/// recurse, footnote definitions are skipped in the body), then the trailing
/// notes section in first-reference order, where references discovered while
/// rendering a note body are appended and rendered too. Unreferenced
/// footnote definitions never render, so their headings get no ids.
fn collect_heading_anchors(blocks: &[Block]) -> BTreeSet<String> {
    let defs = collect_footnote_defs(blocks);
    let def_ids: BTreeSet<&str> = defs.iter().map(|(id, _)| *id).collect();

    let mut assigner = AnchorAssigner::default();
    let mut anchors = BTreeSet::new();
    let mut recorded: BTreeSet<&str> = BTreeSet::new();
    let mut order: Vec<&str> = Vec::new();

    walk_blocks_for_anchors(
        blocks,
        &def_ids,
        &mut assigner,
        &mut anchors,
        &mut recorded,
        &mut order,
    );

    let mut idx = 0usize;
    while idx < order.len() {
        let id = order[idx];
        idx += 1;
        if let Some((_, def_blocks)) = defs.iter().find(|(def_id, _)| *def_id == id) {
            walk_blocks_for_anchors(
                def_blocks,
                &def_ids,
                &mut assigner,
                &mut anchors,
                &mut recorded,
                &mut order,
            );
        }
    }
    anchors
}

/// Footnote definitions in document order, container-aware — the same walk
/// `html.rs::collect_footnote_defs` performs (nested definitions included).
fn collect_footnote_defs(blocks: &[Block]) -> Vec<(&str, &[Block])> {
    let mut defs = Vec::new();
    fn walk<'a>(blocks: &'a [Block], defs: &mut Vec<(&'a str, &'a [Block])>) {
        for block in blocks {
            match block {
                Block::FootnoteDefinition { id, blocks: inner } => {
                    defs.push((id.as_str(), inner.as_slice()));
                    walk(inner, defs);
                }
                Block::BlockQuote(inner) => walk(inner, defs),
                Block::List(list) => {
                    for item in &list.items {
                        walk(&item.blocks, defs);
                    }
                }
                _ => {}
            }
        }
    }
    walk(blocks, &mut defs);
    defs
}

/// Body/notes walk for heading-id assignment and footnote-reference
/// numbering, mirroring the `html.rs` render order: block quotes and list
/// items recurse; footnote definitions are skipped here (they render in the
/// notes section, handled by the caller's index loop).
#[allow(clippy::too_many_arguments)]
fn walk_blocks_for_anchors<'a>(
    blocks: &'a [Block],
    def_ids: &BTreeSet<&'a str>,
    assigner: &mut AnchorAssigner,
    anchors: &mut BTreeSet<String>,
    recorded: &mut BTreeSet<&'a str>,
    order: &mut Vec<&'a str>,
) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } => {
                anchors.insert(assigner.assign(inlines));
                walk_inlines_for_refs(inlines, def_ids, recorded, order);
            }
            Block::Paragraph(inlines) => walk_inlines_for_refs(inlines, def_ids, recorded, order),
            Block::BlockQuote(inner) => {
                walk_blocks_for_anchors(inner, def_ids, assigner, anchors, recorded, order);
            }
            Block::List(list) => {
                for item in &list.items {
                    walk_blocks_for_anchors(
                        &item.blocks,
                        def_ids,
                        assigner,
                        anchors,
                        recorded,
                        order,
                    );
                }
            }
            Block::Table(table) => {
                for cell in &table.head {
                    walk_inlines_for_refs(cell, def_ids, recorded, order);
                }
                for row in &table.rows {
                    for cell in row {
                        walk_inlines_for_refs(cell, def_ids, recorded, order);
                    }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for term in &item.terms {
                        walk_inlines_for_refs(term, def_ids, recorded, order);
                    }
                    for def in &item.definitions {
                        walk_inlines_for_refs(def, def_ids, recorded, order);
                    }
                }
            }
            Block::FootnoteDefinition { .. } => {}
            Block::CodeBlock { .. }
            | Block::ThematicBreak
            | Block::HtmlBlock(_)
            | Block::MathBlock(_)
            | Block::PageBreak => {}
        }
    }
}

/// Record footnote references in first-appearance order — but only those
/// whose id has a definition, mirroring `html.rs::render_footnote_ref`.
fn walk_inlines_for_refs<'a>(
    inlines: &'a [Inline],
    def_ids: &BTreeSet<&'a str>,
    recorded: &mut BTreeSet<&'a str>,
    order: &mut Vec<&'a str>,
) {
    for inl in inlines {
        match inl {
            Inline::FootnoteRef { id }
                if def_ids.contains(id.as_str()) && !recorded.contains(id.as_str()) =>
            {
                recorded.insert(id.as_str());
                order.push(id.as_str());
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                walk_inlines_for_refs(inner, def_ids, recorded, order);
            }
            Inline::Link { content, .. } => {
                walk_inlines_for_refs(content, def_ids, recorded, order)
            }
            _ => {}
        }
    }
}

/// Heading-id assignment state, mirroring `html.rs::RenderState`'s
/// `heading_id_suffixes` map (base/candidate -> next suffix).
#[derive(Default)]
struct AnchorAssigner {
    suffixes: BTreeMap<String, usize>,
}

impl AnchorAssigner {
    /// Assign the id for one heading — a line-for-line mirror of
    /// `html.rs::RenderState::push_heading_id_from_inlines`.
    fn assign(&mut self, inlines: &[Inline]) -> String {
        let mut base = slug_inlines(inlines);
        if base.is_empty() {
            base.push_str("section");
        }

        let mut suffix = self.suffixes.get(base.as_str()).copied().unwrap_or(1);
        loop {
            if suffix == 1 {
                suffix += 1;
                if !self.suffixes.contains_key(base.as_str()) {
                    self.suffixes.insert(base.clone(), suffix);
                    return base;
                }
                continue;
            }

            let candidate = format!("{base}-{suffix}");
            suffix += 1;
            if !self.suffixes.contains_key(candidate.as_str()) {
                self.suffixes.insert(candidate.clone(), 1);
                self.suffixes.insert(base, suffix);
                return candidate;
            }
        }
    }
}

/// Slug a heading's inlines — mirror of `html.rs::slug_inlines` /
/// `push_slug_inlines`: ASCII alphanumerics lowercased; runs of spaces,
/// dashes, and underscores collapse to single dashes; every other character
/// (including non-ASCII letters) is dropped without separating.
fn slug_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    let mut pending_dash = false;
    push_slug_inlines(inlines, &mut s, &mut pending_dash);
    s
}

fn push_slug_inlines(inlines: &[Inline], out: &mut String, pending_dash: &mut bool) {
    for inl in inlines {
        match inl {
            Inline::FootnoteRef { .. } => {}
            Inline::Text(t)
            | Inline::Code(t)
            | Inline::Html(t)
            | Inline::Math(t)
            | Inline::DisplayMath(t) => {
                for c in t.chars() {
                    push_slug_char(out, pending_dash, c);
                }
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                push_slug_inlines(inner, out, pending_dash);
            }
            Inline::Link { content, .. } => push_slug_inlines(content, out, pending_dash),
            Inline::Image { alt, .. } => {
                for c in alt.chars() {
                    push_slug_char(out, pending_dash, c);
                }
            }
            Inline::SoftBreak | Inline::HardBreak => push_slug_char(out, pending_dash, ' '),
        }
    }
}

fn push_slug_char(out: &mut String, pending_dash: &mut bool, c: char) {
    if c.is_ascii_alphanumeric() {
        if *pending_dash && !out.is_empty() {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
        *pending_dash = false;
    } else if c == ' ' || c == '-' || c == '_' {
        *pending_dash = true;
    }
}
