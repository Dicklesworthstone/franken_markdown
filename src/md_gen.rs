//! Grammar-aware Markdown document generator (bead 2c72.1).
//!
//! Std-only, LCG-seeded, size-capped. Intended for property tests and
//! harnesses — not part of the render hot path. The same seed always yields
//! the same document. Failures should print the seed.

use core::fmt::Write as _;

/// Numerical Recipes LCG, matching the existing parser fuzz harness family.
#[derive(Clone, Debug)]
pub struct Lcg {
    state: u64,
}

impl Lcg {
    /// Mix `seed` so small integers still avalanche.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state ^ (self.state >> 27)
    }

    /// Uniform in `0..max`. `max == 0` yields 0.
    pub fn below(&mut self, max: usize) -> usize {
        if max == 0 {
            0
        } else {
            (self.next_u64() % max as u64) as usize
        }
    }

    /// Inclusive range.
    pub fn in_range(&mut self, lo: usize, hi: usize) -> usize {
        if hi <= lo {
            lo
        } else {
            lo + self.below(hi - lo + 1)
        }
    }

    /// True with probability `n/d` (d == 0 => false).
    pub fn chance(&mut self, n: usize, d: usize) -> bool {
        d != 0 && self.below(d) < n
    }

    /// Pick one element. Empty slice is a programming error in the grammar
    /// tables; return the first of a static fallback rather than panic.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }
}

/// Caps and verbosity for one generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenOptions {
    /// Hard UTF-8 byte ceiling. Generation stops at or before this.
    pub max_bytes: usize,
    /// Nested list / quote depth.
    pub max_depth: usize,
    /// Top-level block count ceiling.
    pub max_blocks: usize,
    /// When true, emit one stderr phase line (count + bytes).
    pub verbose: bool,
}

impl Default for GenOptions {
    fn default() -> Self {
        Self {
            max_bytes: 16_384,
            max_depth: 4,
            max_blocks: 24,
            verbose: false,
        }
    }
}

/// Named adversarial corpora (not LCG-shaped).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adversary {
    /// Embedded NUL in otherwise ordinary prose.
    Nul,
    /// Astral-plane and combining scalars.
    Astral,
    /// CRLF line endings mixed with LF.
    Crlf,
    /// Unclosed fences, emphasis, links, HTML.
    Unclosed,
    /// Pathological emphasis-delimiter run, size-capped.
    EmphasisRun,
}

/// All named adversaries, for table-driven tests.
pub const ADVERSARIES: &[Adversary] = &[
    Adversary::Nul,
    Adversary::Astral,
    Adversary::Crlf,
    Adversary::Unclosed,
    Adversary::EmphasisRun,
];

impl Adversary {
    /// Stable name for logs and artifacts.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Nul => "nul",
            Self::Astral => "astral",
            Self::Crlf => "crlf",
            Self::Unclosed => "unclosed",
            Self::EmphasisRun => "emphasis-run",
        }
    }
}

/// Generate a document from `seed` under `opts`. Always UTF-8; never panics.
#[must_use]
pub fn generate(seed: u64, opts: &GenOptions) -> String {
    let mut rng = Lcg::new(seed);
    let mut sink = Sink::new(opts.max_bytes);
    let n_blocks = rng.in_range(1, opts.max_blocks.max(1));
    if opts.verbose {
        eprintln!(
            "md_gen phase=start seed={seed} max_bytes={} max_blocks={n_blocks}",
            opts.max_bytes
        );
    }
    for i in 0..n_blocks {
        if !sink.has_room(8) {
            break;
        }
        if i > 0 {
            sink.push("\n\n");
        }
        emit_block(&mut rng, &mut sink, 0, opts);
    }
    if opts.verbose {
        eprintln!(
            "md_gen phase=done seed={seed} bytes={} blocks_requested={n_blocks}",
            sink.buf.len()
        );
    }
    sink.buf
}

/// Generate one named adversarial document, still honoring `max_bytes`.
#[must_use]
pub fn adversarial(kind: Adversary, max_bytes: usize) -> String {
    let cap = max_bytes.max(1);
    match kind {
        Adversary::Nul => truncate_utf8("pre\0mid\0*post* `[link](x)`\n", cap),
        Adversary::Astral => truncate_utf8(
            "A 🦀 汉 \u{1F980} e\u{0301} \u{1F3F4}\u{200D}\u{2620}\u{FE0F}\n",
            cap,
        ),
        Adversary::Crlf => truncate_utf8("# H\r\n\r\npara one\r\n\r\n- a\r\n- b\n", cap),
        Adversary::Unclosed => {
            truncate_utf8("```rust\nfn x() {\n**[unclosed]( and <div>\n> quote\n", cap)
        }
        Adversary::EmphasisRun => {
            let budget = cap.saturating_sub(2).max(1);
            let n = (budget / 2).min(50_000);
            let mut s = String::with_capacity(n * 2 + 1);
            for _ in 0..n {
                s.push('*');
            }
            s.push('x');
            for _ in 0..n {
                s.push('*');
            }
            truncate_utf8(&s, cap)
        }
    }
}

struct Sink {
    buf: String,
    cap: usize,
}

impl Sink {
    fn new(cap: usize) -> Self {
        Self {
            buf: String::new(),
            cap: cap.max(1),
        }
    }

    fn has_room(&self, n: usize) -> bool {
        self.buf.len().saturating_add(n) <= self.cap
    }

    fn remaining(&self) -> usize {
        self.cap.saturating_sub(self.buf.len())
    }

    fn push(&mut self, s: &str) -> bool {
        if s.is_empty() {
            return self.buf.len() < self.cap;
        }
        if self.buf.len() >= self.cap {
            return false;
        }
        let room = self.remaining();
        if s.len() <= room {
            self.buf.push_str(s);
        } else {
            let mut end = room;
            while end > 0 && !s.is_char_boundary(end) {
                end -= 1;
            }
            self.buf.push_str(&s[..end]);
        }
        self.buf.len() < self.cap
    }
}

fn truncate_utf8(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_owned();
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

#[derive(Clone, Copy)]
enum BlockKind {
    Atx,
    Setext,
    Paragraph,
    List,
    Table,
    Fence,
    Quote,
    Html,
    Rule,
}

const BLOCK_TABLE: &[(usize, BlockKind)] = &[
    (12, BlockKind::Atx),
    (5, BlockKind::Setext),
    (28, BlockKind::Paragraph),
    (14, BlockKind::List),
    (8, BlockKind::Table),
    (10, BlockKind::Fence),
    (8, BlockKind::Quote),
    (5, BlockKind::Html),
    (4, BlockKind::Rule),
];

fn pick_block(rng: &mut Lcg) -> BlockKind {
    let total: usize = BLOCK_TABLE.iter().map(|(w, _)| *w).sum();
    let mut roll = rng.below(total.max(1));
    for &(w, kind) in BLOCK_TABLE {
        if roll < w {
            return kind;
        }
        roll -= w;
    }
    BlockKind::Paragraph
}

fn emit_block(rng: &mut Lcg, sink: &mut Sink, depth: usize, opts: &GenOptions) {
    match pick_block(rng) {
        BlockKind::Atx => emit_atx(rng, sink),
        BlockKind::Setext => emit_setext(rng, sink),
        BlockKind::Paragraph => emit_paragraph(rng, sink),
        BlockKind::List => emit_list(rng, sink, depth, opts),
        BlockKind::Table => emit_table(rng, sink),
        BlockKind::Fence => emit_fence(rng, sink),
        BlockKind::Quote => emit_quote(rng, sink, depth, opts),
        BlockKind::Html => emit_html(rng, sink),
        BlockKind::Rule => {
            let _ = sink.push("---");
        }
    }
}

fn emit_atx(rng: &mut Lcg, sink: &mut Sink) {
    let level = rng.in_range(1, 6);
    let mut hashes = String::new();
    for _ in 0..level {
        hashes.push('#');
    }
    hashes.push(' ');
    sink.push(&hashes);
    emit_inlines_r(rng, sink, 1, 6);
}

fn emit_setext(rng: &mut Lcg, sink: &mut Sink) {
    emit_inlines_r(rng, sink, 1, 5);
    sink.push("\n");
    let ch = if rng.chance(1, 2) { '=' } else { '-' };
    let n = rng.in_range(3, 8);
    let mut underline = String::new();
    for _ in 0..n {
        underline.push(ch);
    }
    sink.push(&underline);
}

fn emit_paragraph(rng: &mut Lcg, sink: &mut Sink) {
    let lines = rng.in_range(1, 3);
    for i in 0..lines {
        if i > 0 {
            sink.push("\n");
        }
        emit_inlines_r(rng, sink, 2, 10);
    }
}

fn emit_list(rng: &mut Lcg, sink: &mut Sink, depth: usize, opts: &GenOptions) {
    let ordered = rng.chance(1, 3);
    let items = rng.in_range(1, 4);
    for i in 0..items {
        if i > 0 {
            sink.push("\n");
        }
        if ordered {
            let mut n = String::new();
            let _ = write!(&mut n, "{}. ", i + 1);
            sink.push(&n);
        } else {
            sink.push("- ");
        }
        emit_inlines_r(rng, sink, 1, 6);
        if depth + 1 < opts.max_depth && rng.chance(1, 4) && sink.has_room(16) {
            sink.push("\n  ");
            emit_list(rng, sink, depth + 1, opts);
        }
    }
}

fn emit_table(rng: &mut Lcg, sink: &mut Sink) {
    let cols = rng.in_range(2, 4);
    let rows = rng.in_range(1, 3);
    row(rng, sink, cols);
    sink.push("\n");
    sink.push("|");
    for _ in 0..cols {
        sink.push(" --- |");
    }
    for _ in 0..rows {
        sink.push("\n");
        row(rng, sink, cols);
    }
}

fn row(rng: &mut Lcg, sink: &mut Sink, cols: usize) {
    sink.push("|");
    for _ in 0..cols {
        sink.push(" ");
        emit_inlines_r(rng, sink, 1, 3);
        sink.push(" |");
    }
}

fn emit_fence(rng: &mut Lcg, sink: &mut Sink) {
    let langs = ["", "rust", "python", "js", "json"];
    let lang = rng.pick(&langs).copied().unwrap_or("");
    sink.push("```");
    sink.push(lang);
    sink.push("\n");
    let n = rng.in_range(1, 4);
    for i in 0..n {
        if i > 0 {
            sink.push("\n");
        }
        emit_plain_r(rng, sink, 4, 24);
    }
    sink.push("\n```");
}

fn emit_quote(rng: &mut Lcg, sink: &mut Sink, depth: usize, opts: &GenOptions) {
    sink.push("> ");
    emit_inlines_r(rng, sink, 1, 8);
    if depth + 1 < opts.max_depth && rng.chance(1, 3) && sink.has_room(8) {
        sink.push("\n> ");
        emit_block(rng, sink, depth + 1, opts);
    }
}

fn emit_html(rng: &mut Lcg, sink: &mut Sink) {
    let forms = [
        "<div>x</div>",
        "<!-- comment -->",
        "<hr>",
        "<span class=\"k\">v</span>",
    ];
    if let Some(s) = rng.pick(&forms) {
        sink.push(s);
    }
}

fn emit_inlines_r(rng: &mut Lcg, sink: &mut Sink, lo: usize, hi: usize) {
    let n = rng.in_range(lo, hi);
    emit_inlines(rng, sink, n);
}

fn emit_plain_r(rng: &mut Lcg, sink: &mut Sink, lo: usize, hi: usize) {
    let n = rng.in_range(lo, hi);
    emit_plain_run(rng, sink, n);
}

fn emit_inlines(rng: &mut Lcg, sink: &mut Sink, n: usize) {
    for i in 0..n {
        if i > 0 {
            sink.push(" ");
        }
        emit_inline(rng, sink);
    }
}

fn emit_inline(rng: &mut Lcg, sink: &mut Sink) {
    match rng.below(8) {
        0 => {
            sink.push("*");
            emit_plain_r(rng, sink, 1, 6);
            sink.push("*");
        }
        1 => {
            sink.push("**");
            emit_plain_r(rng, sink, 1, 6);
            sink.push("**");
        }
        2 => {
            sink.push("`");
            emit_plain_r(rng, sink, 1, 8);
            sink.push("`");
        }
        3 => {
            sink.push("[");
            emit_plain_r(rng, sink, 1, 5);
            sink.push("](https://ex.test/");
            emit_plain_r(rng, sink, 1, 4);
            sink.push(")");
        }
        4 => {
            sink.push("<https://ex.test/");
            emit_plain_r(rng, sink, 1, 4);
            sink.push(">");
        }
        5 => {
            sink.push("&amp;");
        }
        6 => {
            sink.push("![alt](img.png)");
        }
        _ => emit_plain_r(rng, sink, 2, 12),
    }
}

const WORDS: &[&str] = &[
    "lorem", "ipsum", "dolor", "sit", "amet", "code", "list", "table", "alpha", "beta", "gamma",
    "delta", "fn", "let", "mut", "the", "and", "for",
];

fn emit_plain_run(rng: &mut Lcg, sink: &mut Sink, n: usize) {
    for i in 0..n {
        if i > 0 {
            sink.push(" ");
        }
        if let Some(w) = rng.pick(WORDS) {
            sink.push(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Adversary, GenOptions, adversarial, generate};

    #[test]
    fn same_seed_is_byte_identical() {
        let opts = GenOptions::default();
        let a = generate(42, &opts);
        let b = generate(42, &opts);
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn honors_byte_cap() {
        let opts = GenOptions {
            max_bytes: 200,
            max_depth: 2,
            max_blocks: 50,
            verbose: false,
        };
        for seed in 0..40u64 {
            let s = generate(seed, &opts);
            assert!(s.len() <= 200, "seed={seed} len={} exceeds cap", s.len());
        }
        let run = adversarial(Adversary::EmphasisRun, 200);
        assert!(run.len() <= 200, "emphasis-run len={}", run.len());
    }
}
