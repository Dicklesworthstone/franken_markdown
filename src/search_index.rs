//! Search index extraction for a parsed Markdown document.
//!
//! Walks the AST in document order and emits one [`IndexEntry`] per heading
//! and per paragraph. Heading entries carry the anchor `id` the HTML emitter
//! assigns; paragraph entries carry the anchor of the nearest preceding
//! heading (empty string when no heading precedes them), so every entry can
//! be deep-linked into the rendered HTML.
//!
//! [`build_full_search_index`] additionally includes code, tables, definitions,
//! mathematics, and referenced footnotes in HTML emission order. The original
//! [`build_search_index`] retains its heading/body-paragraph-only contract.
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

use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;

use franken_markdown::ast::{Block, Document, Inline};

/// JSON schema tag emitted by [`search_index_json`].
pub const SEARCH_INDEX_SCHEMA: &str = "fmd-search-index-v1";

/// The kind of block an [`IndexEntry`] was extracted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// An ATX heading (`#` .. `######`).
    Heading,
    /// Non-heading content. The full index also uses this v1 wire kind for
    /// code, table rows, definitions, mathematics, and footnote content.
    Paragraph,
}

impl EntryKind {
    #[inline(always)]
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
    build_index(doc, false)
}

/// Build a full-content index for reader-facing document and book search.
///
/// Includes body headings/paragraphs, code and diagram source, table rows,
/// definition terms/bodies, TeX math, and every referenced footnote. Raw HTML
/// blocks and layout-only breaks are omitted. Non-heading entries retain the
/// v1 `paragraph` kind, so existing JSON consumers need no new enum variants.
///
/// Notes follow the HTML renderer's first-reference queue (including references
/// inside tables, definitions, and other notes). Unreferenced notes are omitted,
/// duplicate definitions use the first, and cycles emit each note only once.
/// Note headings consume IDs after all body headings, preserving collision
/// parity. Note text before its first heading targets the actual `fn-ID` anchor.
///
/// This function does not retain source text or clone the document AST.
#[must_use]
pub fn build_full_search_index(doc: &Document) -> SearchIndex {
    build_index(doc, true)
}

fn build_index(doc: &Document, full: bool) -> SearchIndex {
    let mut state = AnchorState::default();
    let mut entries = Vec::new();
    let mut current_anchor = String::new();
    let mut notes = Notes {
        root: &doc.blocks, definitions: None, order: Vec::new(), seen: HashSet::new(),
    };
    walk_blocks(&doc.blocks, &mut state, &mut current_anchor, &mut entries, full, &mut notes);
    let mut cursor = 0;
    while let Some(id) = notes.order.get(cursor).copied() {
        cursor += 1;
        let blocks = notes.definitions.as_ref().and_then(|defs| defs.get(id)).copied();
        if let Some(blocks) = blocks {
            current_anchor = format!("fn-{id}");
            walk_blocks(blocks, &mut state, &mut current_anchor, &mut entries, full, &mut notes);
        }
    }
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
    full: bool,
    notes: &mut Notes<'_>,
) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                if full { notes.references(inlines); }
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
                if full { notes.references(inlines); }
                entries.push(IndexEntry {
                    kind: EntryKind::Paragraph,
                    level: None,
                    anchor: current_anchor.clone(),
                    text: plain_normalized(inlines),
                });
            }
            Block::BlockQuote(inner) => walk_blocks(inner, state, current_anchor, entries, full, notes),
            Block::List(list) => {
                for item in &list.items {
                    walk_blocks(&item.blocks, state, current_anchor, entries, full, notes);
                }
            }
            Block::CodeBlock { code, .. } | Block::MathBlock(code) if full => {
                push_content(code, current_anchor, entries);
            }
            Block::Table(table) if full => {
                for row in std::iter::once(&table.head).chain(table.rows.iter()) {
                    let mut text = String::new();
                    for cell in row {
                        notes.references(cell);
                        if !text.is_empty() { text.push(' '); }
                        push_inlines_to_plain(cell, &mut text);
                    }
                    push_content(&text, current_anchor, entries);
                }
            }
            Block::DefinitionList(items) if full => {
                for item in items {
                    let mut text = String::new();
                    for inlines in item.terms.iter().chain(&item.definitions) {
                        notes.references(inlines);
                        if !text.is_empty() { text.push(' '); }
                        push_inlines_to_plain(inlines, &mut text);
                    }
                    push_content(&text, current_anchor, entries);
                }
            }
            // Definition bodies are emitted only when dequeued after the body.
            Block::FootnoteDefinition { .. } => {}
            _ => {}
        }
    }
}

fn push_content(text: &str, anchor: &str, entries: &mut Vec<IndexEntry>) {
    let text = normalized_text(text);
    if !text.is_empty() {
        entries.push(IndexEntry {
            kind: EntryKind::Paragraph, level: None, anchor: anchor.to_string(), text,
        });
    }
}

// Lazy collection preserves the ordinary no-footnote fast path. Maps and sets
// are lookup-only; only first-reference insertion order affects emitted data.
struct Notes<'a> {
    root: &'a [Block],
    definitions: Option<HashMap<&'a str, &'a [Block]>>,
    order: Vec<&'a str>,
    seen: HashSet<&'a str>,
}

impl Notes<'_> {
    fn references(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            match inline {
                Inline::FootnoteRef { id } => {
                    let root = self.root;
                    let definitions = self.definitions.get_or_insert_with(|| {
                        let mut definitions = HashMap::new();
                        collect_definitions(root, &mut definitions);
                        definitions
                    });
                    if let Some((&id, _)) = definitions.get_key_value(id.as_str()) {
                        if self.seen.insert(id) { self.order.push(id); }
                    }
                }
                Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner)
                | Inline::Link { content: inner, .. } => self.references(inner),
                _ => {}
            }
        }
    }
}

fn collect_definitions<'a>(blocks: &'a [Block], definitions: &mut HashMap<&'a str, &'a [Block]>) {
    for block in blocks {
        match block {
            Block::FootnoteDefinition { id, blocks } => {
                definitions.entry(id.as_str()).or_insert(blocks.as_slice());
                // HTML collects nested definitions even beneath a duplicate.
                collect_definitions(blocks, definitions);
            }
            Block::BlockQuote(inner) => collect_definitions(inner, definitions),
            Block::List(list) => {
                for item in &list.items { collect_definitions(&item.blocks, definitions); }
            }
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
#[inline(always)]
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
    normalized_text(&plain)
}

fn normalized_text(plain: &str) -> String {
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
#[inline(always)]
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
#[inline(always)]
fn push_json_escaped(out: &mut String, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = s.as_bytes();
    let mut clean_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let esc = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            b if b < 0x20 => {
                if clean_start < i {
                    out.push_str(&s[clean_start..i]);
                }
                out.push_str("\\u00");
                out.push(char::from(HEX[((b >> 4) & 0xF) as usize]));
                out.push(char::from(HEX[(b & 0xF) as usize]));
                clean_start = i + 1;
                continue;
            }
            _ => continue,
        };
        if clean_start < i {
            out.push_str(&s[clean_start..i]);
        }
        out.push_str(esc);
        clean_start = i + 1;
    }
    if clean_start < s.len() {
        out.push_str(&s[clean_start..]);
    }
}

#[cfg(test)]
mod full_index_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use franken_markdown::{Align, DefinitionItem, HtmlOptions, Table};

    fn paragraph(text: &str) -> Block { Block::Paragraph(vec![Inline::Text(text.into())]) }
    fn heading(text: &str) -> Block { Block::Heading { level: 1, inlines: vec![Inline::Text(text.into())] } }
    fn reference(id: &str) -> Inline { Inline::FootnoteRef { id: id.into() } }
    fn note(id: &str, blocks: Vec<Block>) -> Block {
        Block::FootnoteDefinition { id: id.into(), blocks }
    }

    #[test]
    fn full_index_adds_code_tables_definitions_math_without_changing_legacy_index() {
        let doc = Document { blocks: vec![
            heading("Guide"),
            Block::CodeBlock { lang: Some("rust".into()), code: "fn  run() {\n  launch();\n}".into() },
            Block::Table(Table {
                head: vec![vec![Inline::Text("Name".into())], vec![Inline::Text("Value".into())]],
                rows: vec![vec![vec![Inline::Code("timeout".into())], vec![Inline::Text("30 seconds".into())]]],
                align: vec![Align::Left, Align::Right],
            }),
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![Inline::Text("Cache".into())]],
                definitions: vec![vec![Inline::Text("Stores reusable results".into())]],
            }]),
            Block::MathBlock("x^2  +  y^2".into()),
            Block::ThematicBreak, Block::PageBreak,
            Block::HtmlBlock("<script>not indexed</script>".into()),
        ]};
        let legacy = build_search_index(&doc);
        assert_eq!(legacy.entries.len(), 1);
        let full = build_full_search_index(&doc);
        let texts: Vec<_> = full.entries.iter().map(|entry| entry.text.as_str()).collect();
        assert_eq!(texts, ["Guide", "fn run() { launch(); }", "Name Value",
            "timeout 30 seconds", "Cache Stores reusable results", "x^2 + y^2"]);
        assert!(full.entries.iter().all(|entry| entry.anchor == "guide"));
        assert!(full.entries[1..].iter().all(|entry| entry.kind == EntryKind::Paragraph));
        assert_eq!(full, build_full_search_index(&doc));
        assert_eq!(legacy, build_search_index(&doc));
        assert!(search_index_json(&full).starts_with("{\"schema\":\"fmd-search-index-v1\""));
    }

    #[test]
    fn note_order_matches_html_and_preserves_body_and_note_heading_collisions() {
        let doc = Document { blocks: vec![
            heading("Dup"),
            note("a", vec![paragraph("First definition"), heading("Dup")]),
            Block::Paragraph(vec![reference("b"), reference("a")]),
            heading("Dup"),
            note("b", vec![Block::Paragraph(vec![Inline::Text("Second definition".into()), reference("c")]), heading("Dup")]),
            note("c", vec![paragraph("Transitive note")]),
            note("unused", vec![heading("Invisible"), paragraph("Must not be indexed")]),
        ]};
        let full = build_full_search_index(&doc);
        let anchors: Vec<_> = full.entries.iter().filter(|entry| entry.kind == EntryKind::Heading)
            .map(|entry| entry.anchor.as_str()).collect();
        assert_eq!(anchors, ["dup", "dup-2", "dup-3", "dup-4"]);
        let note_text: Vec<_> = full.entries.iter().filter(|entry| entry.anchor.starts_with("fn-"))
            .map(|entry| (entry.anchor.as_str(), entry.text.as_str())).collect();
        assert_eq!(note_text, [("fn-b", "Second definition[^c]"),
            ("fn-a", "First definition"), ("fn-c", "Transitive note")]);
        let html = franken_markdown::html::render_fragment(&doc.blocks, &HtmlOptions::default());
        for entry in &full.entries {
            assert!(html.contains(&format!("id=\"{}\"", entry.anchor)), "missing {}", entry.anchor);
        }
        assert!(!full.entries.iter().any(|entry| entry.text.contains("Invisible")));
        let first_note = html.find("id=\"fn-b\"").unwrap();
        assert!(html.find("id=\"dup-2\"").unwrap() < first_note);
        assert!(first_note < html.find("id=\"fn-a\"").unwrap());
    }

    #[test]
    fn references_inside_tables_definitions_and_inline_containers_are_followed() {
        let doc = Document { blocks: vec![
            Block::Table(Table {
                head: vec![vec![Inline::Strong(vec![reference("table")])]],
                rows: vec![], align: vec![Align::Left],
            }),
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![Inline::Link { dest: "https://example.test".into(), title: None,
                    content: vec![reference("term")] }]],
                definitions: vec![vec![Inline::Emphasis(vec![reference("body")])]],
            }]),
            note("body", vec![paragraph("Definition body note")]),
            note("term", vec![paragraph("Definition term note")]),
            note("table", vec![paragraph("Table note")]),
        ]};
        let full = build_full_search_index(&doc);
        let anchors: Vec<_> = full.entries.iter().filter(|entry| entry.anchor.starts_with("fn-"))
            .map(|entry| entry.anchor.as_str()).collect();
        assert_eq!(anchors, ["fn-table", "fn-term", "fn-body"]);
    }

    #[test]
    fn cycles_duplicate_definitions_and_undefined_notes_do_not_duplicate_results() {
        let doc = Document { blocks: vec![
            Block::Paragraph(vec![reference("a"), reference("missing"), reference("a")]),
            note("a", vec![Block::Paragraph(vec![Inline::Text("A".into()), reference("b")])]),
            note("b", vec![Block::Paragraph(vec![Inline::Text("B".into()), reference("a")])]),
            note("a", vec![paragraph("Duplicate definition must not replace A")]),
        ]};
        let full = build_full_search_index(&doc);
        assert_eq!(full.entries.len(), 3);
        assert_eq!(full.entries[1].text, "A[^b]");
        assert_eq!(full.entries[2].text, "B[^a]");
        assert_eq!(full.entries[1].anchor, "fn-a");
        assert_eq!(full.entries[2].anchor, "fn-b");
    }

    #[test]
    fn code_containing_note_syntax_does_not_reference_a_definition() {
        let doc = Document { blocks: vec![
            Block::CodeBlock { lang: None, code: "[^hidden]".into() },
            note("hidden", vec![paragraph("Not rendered")]),
        ]};
        let index = build_full_search_index(&doc);
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].text, "[^hidden]");
        assert_eq!(index.entries[0].anchor, "");
    }
}
