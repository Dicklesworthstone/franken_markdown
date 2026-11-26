//! Shared heading and fragment normalization for rendering and navigation.
//!
//! Heading slugs retain Unicode letters in lowercase and ASCII digits. Spaces,
//! hyphens, and underscores separate words; other punctuation is omitted.
//! Normalization does not assign collision suffixes or normalize Unicode
//! composition. Fragment lookup is case-sensitive and percent-decodes once.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crate::{Block, Document, Inline};

/// Source-derived compatibility anchors for GitHub-authored repeated separators.
///
/// Canonical emitted IDs always win. Aliases do not change heading IDs, decode
/// a fragment a second time, or grant authority to an arbitrary URL. Exceeding
/// the bounded inventory disables every fallback; exact navigation is unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeadingAliases {
    exact: BTreeSet<String>,
    aliases: BTreeMap<String, String>,
    suffixes: BTreeMap<String, usize>,
    bytes: usize,
    headings: usize,
    disabled: bool,
}

impl HeadingAliases {
    const MAX_HEADINGS: usize = 4096;
    const MAX_BYTES: usize = 1024 * 1024;
    const MAX_ID_BYTES: usize = 8192;

    fn charge(&mut self, bytes: usize) -> bool {
        if self.disabled {
            return false;
        }
        if bytes > Self::MAX_BYTES.saturating_sub(self.bytes) {
            self.disabled = true;
            self.exact.clear();
            self.aliases.clear();
            self.suffixes.clear();
            return false;
        }
        self.bytes += bytes;
        true
    }

    pub(crate) fn heading(&mut self, inlines: &[Inline], canonical: &str) {
        if self.disabled {
            return;
        }
        self.headings += 1;
        if self.headings > Self::MAX_HEADINGS {
            let _ = self.charge(Self::MAX_BYTES + 1);
            return;
        }
        let Some(base) = separated_heading_slug(inlines) else {
            let _ = self.charge(Self::MAX_BYTES + 1);
            return;
        };
        // Charge retained keys/targets before inserting. Collision suffixes are
        // bounded by the heading count; their maximum decimal width is small.
        if !self.charge(canonical.len() * 2 + base.len() * 3 + 32) {
            return;
        }
        self.exact.insert(canonical.to_owned());
        let mut suffix = self.suffixes.get(&base).copied().unwrap_or(0);
        let alias = loop {
            let candidate = if suffix == 0 {
                base.clone()
            } else {
                format!("{base}-{suffix}")
            };
            suffix += 1;
            if !self.suffixes.contains_key(&candidate) {
                self.suffixes.insert(candidate.clone(), 1);
                break candidate;
            }
        };
        self.suffixes.insert(base, suffix);
        // Only repeated-separator compatibility is offered. Other slug dialects
        // and arbitrary misspellings retain their existing behavior.
        if alias.contains("--") && alias != canonical {
            self.aliases.insert(alias, canonical.to_owned());
        }
    }

    pub(crate) fn reserve(&mut self, id: &str) {
        if self.charge(id.len()) {
            self.exact.insert(id.to_owned());
        }
    }

    /// Resolve an already decoded compatibility spelling, only when no emitted
    /// ID owns it. Callers keep their existing exact-match/ambiguity checks.
    /// Returns no fallback when the complete alias inventory exceeds its limit.
    #[must_use]
    pub fn resolve_alias(&self, id: &str) -> Option<&str> {
        if self.exact.contains(id) {
            None
        } else {
            self.aliases.get(id).map(String::as_str)
        }
    }

    /// Iterate unshadowed compatibility spellings and their canonical IDs in
    /// lexical order. Exact emitted IDs never appear as compatibility entries.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.aliases.iter().filter_map(|(alias, target)| {
            (!self.exact.contains(alias)).then_some((alias.as_str(), target.as_str()))
        })
    }

    /// Rewrite an actual local alias, retaining its query. Already-canonical
    /// source bytes, malformed escapes and unknown targets are left untouched.
    pub(crate) fn destination(&self, destination: &str) -> Option<String> {
        if !destination.starts_with(['#', '?']) {
            return None;
        }
        let (prefix, fragment) = destination.split_once('#')?;
        let decoded = decode_fragment(fragment)?;
        let canonical = self.resolve_alias(&decoded)?;
        Some(format!("{prefix}#{canonical}"))
    }
}

/// Keep separate separators around removed punctuation. The visible inline walk
/// matches canonical slugs, including code, image alt text, safe markup and the
/// omission of footnote marks. This is deliberately not a general slugger API.
fn separated_heading_slug(inlines: &[Inline]) -> Option<String> {
    fn text(source: &str, out: &mut String, separators: &mut usize) -> Option<()> {
        for ch in source.chars() {
            if ch.is_ascii_alphanumeric() || ch.is_alphabetic() || ch == '_' {
                let extra = if out.is_empty() { 0 } else { *separators };
                if extra > HeadingAliases::MAX_ID_BYTES.saturating_sub(out.len()) {
                    return None;
                }
                out.extend(std::iter::repeat_n('-', extra));
                out.extend(ch.to_lowercase());
                if out.len() > HeadingAliases::MAX_ID_BYTES {
                    return None;
                }
                *separators = 0;
            } else if matches!(ch, ' ' | '-') {
                *separators += 1;
            }
        }
        Some(())
    }
    fn walk(inlines: &[Inline], out: &mut String, separators: &mut usize) -> Option<()> {
        for inline in inlines {
            match inline {
                Inline::FootnoteRef { .. } => {}
                Inline::Text(value)
                | Inline::Code(value)
                | Inline::Math(value)
                | Inline::DisplayMath(value) => text(value, out, separators)?,
                Inline::Html(value) => {
                    text(&crate::safe_html::visible_text(value), out, separators)?;
                }
                Inline::Emphasis(inner)
                | Inline::Strong(inner)
                | Inline::Strikethrough(inner)
                | Inline::Link { content: inner, .. } => walk(inner, out, separators)?,
                Inline::Image { alt, .. } => text(alt, out, separators)?,
                Inline::SoftBreak | Inline::HardBreak => text(" ", out, separators)?,
            }
        }
        Some(())
    }
    let mut out = String::new();
    walk(inlines, &mut out, &mut 0)?;
    if out.is_empty() {
        out.push_str("section");
    }
    Some(out)
}

/// Build compatibility targets in the same body/first-reference note order as
/// HTML publication. The source AST and canonical identifiers are unchanged.
#[must_use]
pub fn heading_aliases(doc: &Document) -> HeadingAliases {
    crate::html::heading_identifiers(doc).aliases
}

/// Preserve the source identity of headings addressed through compatibility
/// fragments. Include their canonical spellings so both forms bind identically
/// after exporters insert headings or reorder note bodies. Other headings keep
/// their ordinary renderer path; this map inherits the bounded alias inventory.
pub(crate) fn heading_alias_targets(doc: &Document) -> BTreeMap<String, usize> {
    let identifiers = crate::html::heading_identifiers(doc);
    if identifiers.aliases.entries().next().is_none() {
        return BTreeMap::new();
    }
    let canonical: BTreeMap<_, _> = identifiers
        .headings
        .iter()
        .enumerate()
        .filter_map(|(ordinal, id)| id.as_deref().map(|id| (id, ordinal)))
        .collect();
    let mut targets = BTreeMap::new();
    for (alias, id) in identifiers.aliases.entries() {
        if let Some(&ordinal) = canonical.get(id) {
            targets.insert(alias.to_owned(), ordinal);
            targets.insert(id.to_owned(), ordinal);
        }
    }
    targets
}

/// Compose note-preparation provenance with optional safe-HTML lowering
/// provenance. Generated headings remain unowned at either transformation.
pub(crate) fn compose_heading_origins(
    mut prepared: Vec<Option<usize>>,
    lowered: Option<&[Option<usize>]>,
) -> Vec<Option<usize>> {
    if let Some(lowered) = lowered {
        for origin in &mut prepared {
            *origin = origin.and_then(|ordinal| lowered.get(ordinal).copied().flatten());
        }
    }
    prepared
}

/// Resolve aliases before transformations introduce export-only headings.
/// Documents without a matching aliased link retain their borrowed AST.
#[must_use]
pub fn canonicalize_links(doc: &Document) -> Cow<'_, Document> {
    fn inlines(nodes: &[Inline], aliases: &HeadingAliases) -> bool {
        nodes.iter().any(|node| match node {
            Inline::Link { dest, content, .. } => {
                aliases.destination(dest).is_some() || inlines(content, aliases)
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                inlines(inner, aliases)
            }
            Inline::Html(raw) => html(raw, aliases),
            _ => false,
        })
    }
    fn html(raw: &str, aliases: &HeadingAliases) -> bool {
        crate::safe_html::url_spans(raw).any(|span| {
            span.kind == crate::safe_html::UrlKind::Link
                && aliases.destination(&span.destination).is_some()
        })
    }
    fn blocks(nodes: &[Block], aliases: &HeadingAliases) -> bool {
        nodes.iter().any(|node| match node {
            Block::Paragraph(content)
            | Block::Heading {
                inlines: content, ..
            } => inlines(content, aliases),
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                blocks(inner, aliases)
            }
            Block::List(list) => list.items.iter().any(|item| blocks(&item.blocks, aliases)),
            Block::Table(table) => table
                .head
                .iter()
                .chain(table.rows.iter().flatten())
                .any(|cell| inlines(cell, aliases)),
            Block::DefinitionList(items) => items.iter().any(|item| {
                item.terms
                    .iter()
                    .chain(&item.definitions)
                    .any(|content| inlines(content, aliases))
            }),
            Block::HtmlBlock(raw) => html(raw, aliases),
            _ => false,
        })
    }
    let aliases = heading_aliases(doc);
    if aliases.aliases.is_empty() || !blocks(&doc.blocks, &aliases) {
        return Cow::Borrowed(doc);
    }
    let mut rewritten = doc.clone();
    crate::safe_html::visit_destinations(&mut rewritten.blocks, &mut |destination, kind| {
        if kind == crate::safe_html::UrlKind::Link
            && let Some(canonical) = aliases.destination(destination)
        {
            *destination = canonical;
        }
    });
    Cow::Owned(rewritten)
}

/// Normalize a heading's visible inline content into a base anchor.
///
/// Inline markup contributes its visible text, images contribute their alt
/// text, and footnote markers do not change a heading's identity. An empty
/// result is left empty so callers can apply their `section` fallback before
/// assigning document-local collision suffixes.
#[must_use]
pub fn heading_slug(inlines: &[Inline]) -> String {
    fn walk(inlines: &[Inline], out: &mut String, pending_dash: &mut bool) {
        for inline in inlines {
            match inline {
                Inline::FootnoteRef { .. } => {}
                Inline::Text(text)
                | Inline::Code(text)
                | Inline::Math(text)
                | Inline::DisplayMath(text) => push_slug_text(out, pending_dash, text),
                Inline::Html(html) => {
                    push_slug_text(out, pending_dash, &crate::safe_html::visible_text(html));
                }
                Inline::Emphasis(inner)
                | Inline::Strong(inner)
                | Inline::Strikethrough(inner)
                | Inline::Link { content: inner, .. } => walk(inner, out, pending_dash),
                Inline::Image { alt, .. } => push_slug_text(out, pending_dash, alt),
                Inline::SoftBreak | Inline::HardBreak => push_slug_char(out, pending_dash, ' '),
            }
        }
    }
    let mut out = String::new();
    walk(inlines, &mut out, &mut false);
    out
}

/// Normalize visible heading text using the same rules as [`heading_slug`].
#[must_use]
pub fn text_slug(text: &str) -> String {
    let mut out = String::new();
    push_slug_text(&mut out, &mut false, text);
    out
}

fn push_slug_text(out: &mut String, pending_dash: &mut bool, text: &str) {
    // Preserve the inexpensive ASCII path used for ordinary documentation.
    for ch in text.chars() {
        push_slug_char(out, pending_dash, ch);
    }
}

pub(crate) fn push_slug_char(out: &mut String, pending_dash: &mut bool, ch: char) {
    if ch.is_ascii_alphanumeric() {
        if *pending_dash && !out.is_empty() {
            out.push('-');
        }
        out.push(ch.to_ascii_lowercase());
        *pending_dash = false;
    } else if ch.is_alphabetic() {
        if *pending_dash && !out.is_empty() {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
        *pending_dash = false;
    } else if matches!(ch, ' ' | '-' | '_') {
        *pending_dash = true;
    }
}

/// Percent-decode a URL fragment exactly once, rejecting malformed escapes,
/// invalid UTF-8, and controls. The leading `#` is not part of the input.
#[must_use]
pub fn decode_fragment(fragment: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let source = fragment.as_bytes();
    let mut bytes = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%' {
            bytes.push(hex(*source.get(index + 1)?)? * 16 + hex(*source.get(index + 2)?)?);
            index += 3;
        } else {
            bytes.push(source[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(bytes).ok()?;
    (!decoded.chars().any(char::is_control)).then_some(decoded)
}

#[cfg(test)]
mod alias_tests {
    use super::*;

    #[test]
    fn alias_limits_discard_partial_inventory_without_changing_exact_ids() {
        let heading = [Inline::Text("A — B".to_string())];
        let mut aliases = HeadingAliases::default();
        aliases.heading(&heading, "a-b");
        assert_eq!(aliases.resolve_alias("a--b"), Some("a-b"));
        assert!(!aliases.charge(HeadingAliases::MAX_BYTES));
        assert!(aliases.resolve_alias("a--b").is_none());
        aliases.heading(&heading, "a-b-2");
        assert!(aliases.resolve_alias("a--b-1").is_none());
    }

    #[test]
    fn heading_limit_never_exposes_a_partial_alias_namespace() {
        let heading = [Inline::Text("A — B".to_string())];
        let mut aliases = HeadingAliases::default();
        for index in 0..=HeadingAliases::MAX_HEADINGS {
            aliases.heading(&heading, &format!("a-b-{index}"));
        }
        assert!(aliases.resolve_alias("a--b").is_none());
        assert!(aliases.entries().next().is_none());
    }

    #[test]
    fn rewriting_borrows_documents_without_an_actual_local_alias() {
        let document = crate::parse_markdown(
            "# A — B\n\n[exact](#a-b) [unknown](#a---b) \
             [remote](https://example.test/#a--b) ![image](#a--b)",
        );
        assert!(matches!(canonicalize_links(&document), Cow::Borrowed(_)));
        let document = crate::parse_markdown("# A — B\n\n[local](?view=print#a%2D%2Db)");
        let before = document.clone();
        let rewritten = canonicalize_links(&document);
        assert!(matches!(rewritten, Cow::Owned(_)));
        assert_eq!(
            rewritten.as_ref(),
            &crate::parse_markdown("# A — B\n\n[local](?view=print#a-b)")
        );
        assert_eq!(document, before);
    }
}
