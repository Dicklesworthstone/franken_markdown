//! Search index extraction for a parsed Markdown document.
//!
//! Walks the AST in document order and emits one [`IndexEntry`] per heading
//! and per paragraph. Heading entries carry the anchor `id` the HTML emitter
//! assigns; paragraph entries carry the anchor of the nearest preceding
//! heading (empty string when no heading precedes them), so every entry can
//! be deep-linked into the rendered HTML.
//!
//! # Anchor parity with `src/html.rs`
//!
//! The heading-id algorithm below is a deliberate, line-cited mirror of the
//! HTML emitter so anchors match the rendered HTML byte-for-byte:
//!
//! - collision-suffix state `RenderState::heading_id_suffixes` — src/html.rs:119-121
//! - `RenderState::push_heading_id_from_inlines` — src/html.rs:159-189
//! - `push_usize` / `decimal_len_usize` — src/html.rs:954-976
//! - `slug_inlines` / `push_slug_inlines` / `push_slug_char` — src/html.rs:1013-1057
//! - container-aware walk of `collect_toc_entries` — src/html.rs:1063-1095
//! - `push_inlines_to_plain` — src/html.rs:984-998
//!
//! Behavioural notes inherited from that mirror: slugs keep ASCII
//! alphanumerics only (lowercased), ` `/`-`/`_` collapse to single dashes,
//! empty slugs become `section`, and duplicate headings receive `-2`, `-3`,
//! ... suffixes (the first occurrence is unsuffixed). If html.rs changes,
//! change this file in lockstep; the test suite cross-checks anchors against
//! real rendered HTML, including duplicate-heading collisions.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use franken_markdown::ast::{Block, Document, Inline};

/// JSON schema tag emitted by [`search_index_json`].
pub const SEARCH_INDEX_SCHEMA: &str = "fmd-search-index-v1";

/// The kind of block an [`IndexEntry`] was extracted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// An ATX heading (`#` .. `######`).
    Heading,
    /// A paragraph of inline content.
    Paragraph,
}

impl EntryKind {
    fn as_str(self) -> &'static str {
        match self {
            EntryKind::Heading => "heading",
            EntryKind::Paragraph => "paragraph",
        }
    }
}

/// One searchable block: a heading or a paragraph, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// Whether this entry came from a heading or a paragraph.
    pub kind: EntryKind,
    /// Heading level (1–6); `None` for paragraphs.
    pub level: Option<u8>,
    /// For headings: the anchor id the HTML emitter assigns to this heading.
    /// For paragraphs: the id of the nearest preceding heading, or the empty
    /// string when no heading precedes the paragraph.
    pub anchor: String,
    /// Plain text of the block, whitespace-normalized (trimmed, all
    /// whitespace runs collapsed to a single ASCII space).
    pub text: String,
}

/// A document-order search index over a parsed [`Document`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchIndex {
    /// Entries in document order (container-aware: block quotes and list
    /// items are walked depth-first, mirroring the HTML renderer).
    pub entries: Vec<IndexEntry>,
}

/// Build the search index for a parsed document.
///
/// Deterministic: same AST in, byte-identical index out. Heading anchors are
/// assigned with the exact algorithm (and collision-suffix state machine) the
/// HTML emitter uses, so `anchor` values match `id="..."` attributes in
/// `franken_markdown::html::render` output.
#[must_use]
pub fn build_search_index(doc: &Document) -> SearchIndex {
    let mut state = AnchorState::default();
    let mut entries = Vec::new();
    let mut current_anchor = String::new();
    walk_blocks(&doc.blocks, &mut state, &mut current_anchor, &mut entries);
    SearchIndex { entries }
}

/// Serialize the index as compact JSON (schema `fmd-search-index-v1`).
///
/// Keys are snake_case; `level` is present on heading entries only. String
/// escaping is total (quotes, backslashes, and every C0 control character),
/// so the output is always valid JSON for arbitrary document text.
#[must_use]
pub fn search_index_json(index: &SearchIndex) -> String {
    let mut out = String::with_capacity(64 + index.entries.len().saturating_mul(128));
    out.push_str("{\"schema\":\"");
    out.push_str(SEARCH_INDEX_SCHEMA);
    out.push_str("\",\"entries\":[");
    for (i, entry) in index.entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"kind\":\"");
        out.push_str(entry.kind.as_str());
        out.push('"');
        if let Some(level) = entry.level {
            out.push_str(",\"level\":");
            push_usize(&mut out, usize::from(level));
        }
        out.push_str(",\"anchor\":\"");
        push_json_escaped(&mut out, &entry.anchor);
        out.push_str("\",\"text\":\"");
        push_json_escaped(&mut out, &entry.text);
        out.push_str("\"}");
    }
    out.push_str("]}");
    out
}

/// Collision-suffix state. Mirror of `RenderState::heading_id_suffixes`
/// (src/html.rs:119-121): keys are every emitted heading id, values are the
/// next suffix to try when that same id text later appears as a base slug.
/// Lookup-only, so `HashMap` ordering never leaks into the output.
#[derive(Default)]
struct AnchorState {
    suffixes: HashMap<String, usize>,
}

/// Document-order, container-aware block walk. Mirrors `collect_toc_entries`
/// (src/html.rs:1068-1095) — which itself mirrors the block renderer's
/// recursion — and extends it with paragraph entries.
fn walk_blocks(
    blocks: &[Block],
    state: &mut AnchorState,
    current_anchor: &mut String,
    entries: &mut Vec<IndexEntry>,
) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                let anchor = heading_id(state, inlines);
                entries.push(IndexEntry {
                    kind: EntryKind::Heading,
                    level: Some(*level),
                    anchor: anchor.clone(),
                    text: plain_normalized(inlines),
                });
                *current_anchor = anchor;
            }
            Block::Paragraph(inlines) => {
                entries.push(IndexEntry {
                    kind: EntryKind::Paragraph,
                    level: None,
                    anchor: current_anchor.clone(),
                    text: plain_normalized(inlines),
                });
            }
            Block::BlockQuote(inner) => walk_blocks(inner, state, current_anchor, entries),
            Block::List(list) => {
                for item in &list.items {
                    walk_blocks(&item.blocks, state, current_anchor, entries);
                }
            }
            // Footnote definitions are skipped in the body walk and rendered
            // later by the HTML emitter (src/html.rs:1088-1092); walking them
            // here would consume heading ids the body headings never receive.
            Block::FootnoteDefinition { .. } => {}
            _ => {}
        }
    }
}

/// Mirror of `RenderState::push_heading_id_from_inlines` (src/html.rs:159-189),
/// returning the id instead of appending to an output string.
fn heading_id(state: &mut AnchorState, inlines: &[Inline]) -> String {
    let mut base = slug_inlines(inlines);
    if base.is_empty() {
        base.push_str("section");
    }

    let mut out = String::new();
    let mut suffix = state.suffixes.get(base.as_str()).copied().unwrap_or(1);
    loop {
        if suffix == 1 {
            suffix += 1;
            if !state.suffixes.contains_key(base.as_str()) {
                out.push_str(&base);
                state.suffixes.insert(base, suffix);
                return out;
            }
            continue;
        }

        let mut candidate = String::with_capacity(base.len() + 1 + decimal_len_usize(suffix));
        candidate.push_str(&base);
        candidate.push('-');
        push_usize(&mut candidate, suffix);
        suffix += 1;
        if let Entry::Vacant(entry) = state.suffixes.entry(candidate) {
            out.push_str(entry.key());
            entry.insert(1);
            state.suffixes.insert(base, suffix);
            return out;
        }
    }
}

/// Mirror of `slug_inlines` (src/html.rs:1013-1018).
fn slug_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    let mut pending_dash = false;
    push_slug_inlines(inlines, &mut s, &mut pending_dash);
    s
}

/// Mirror of `push_slug_inlines` (src/html.rs:1020-1045).
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
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                push_slug_inlines(c, out, pending_dash);
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

/// Mirror of `push_slug_char` (src/html.rs:1047-1057): ASCII alphanumerics
/// are kept (lowercased); space, `-`, and `_` collapse to a single dash;
/// every other character is dropped without touching the pending-dash state.
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

/// Mirror of `push_inlines_to_plain` (src/html.rs:984-998).
fn push_inlines_to_plain(inlines: &[Inline], out: &mut String) {
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) | Inline::Math(t) | Inline::DisplayMath(t) => {
                out.push_str(t)
            }
            Inline::FootnoteRef { id } => {
                out.push_str("[^");
                out.push_str(id);
                out.push(']');
            }
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                push_inlines_to_plain(c, out);
            }
            Inline::Link { content, .. } => push_inlines_to_plain(content, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Html(html) => out.push_str(html),
        }
    }
}

/// Plain text with whitespace normalized: trimmed, and every whitespace run
/// (including soft/hard break spaces and literal newlines/tabs inside text)
/// collapsed to a single ASCII space. Deterministic via `split_whitespace`.
fn plain_normalized(inlines: &[Inline]) -> String {
    let mut plain = String::new();
    push_inlines_to_plain(inlines, &mut plain);
    let mut out = String::with_capacity(plain.len());
    let mut words = plain.split_whitespace();
    if let Some(first) = words.next() {
        out.push_str(first);
        for word in words {
            out.push(' ');
            out.push_str(word);
        }
    }
    out
}

/// Mirror of `push_usize` (src/html.rs:954-966). The buffer only ever holds
/// ASCII digits, so the UTF-8 conversion cannot fail; `unwrap_or` keeps the
/// crate's no-`unwrap` lint satisfied.
#[inline(always)]
fn push_usize(out: &mut String, value: usize) {
    if value < 10 {
        out.push((b'0' + value as u8) as char);
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = value;
    let mut idx = buf.len();
    loop {
        idx -= 1;
        buf[idx] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.push_str(std::str::from_utf8(&buf[idx..]).unwrap_or("0"));
}

/// Mirror of `decimal_len_usize` (src/html.rs:969-976).
fn decimal_len_usize(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}

/// Total JSON string escaping: `"`, `\`, the short escapes for
/// newline/carriage-return/tab, and `\u00XX` for every other C0 control
/// character. All other characters (including non-ASCII) pass through as
/// UTF-8, which is valid JSON.
fn push_json_escaped(out: &mut String, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let n = u32::from(c);
                out.push_str("\\u00");
                out.push(char::from(HEX[((n >> 4) & 0xF) as usize]));
                out.push(char::from(HEX[(n & 0xF) as usize]));
            }
            c => out.push(c),
        }
    }
}
