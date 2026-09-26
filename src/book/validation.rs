//! Checks local Markdown navigation against the expanded, parsed book.
//!
//! This is not a network checker or a PDF/EPUB conformance validator. Heading
//! and footnote anchors follow safe HTML emission; raw HTML IDs, images, and
//! non-chapter downloads are outside this check and never acquire authority.

use std::collections::{BTreeMap, BTreeSet};

use super::{Book, BookRenderer, paths};
use crate::search_index::build_search_index;
use crate::{Block, Document, Inline, RenderError, Result};

#[path = "document_links.rs"]
mod document_links;
pub use document_links::{
    AnchorKind, DocumentAnchor, DocumentLinkAnalysis, DocumentReference, ReferenceKind,
    analyze_document_links,
};

const MAX_NODES: usize = 250_000;
const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;
const MAX_FINDINGS: usize = 4096;
const MAX_REPORT_BYTES: usize = 4 * 1024 * 1024;

/// One broken local reference, scoped to its expanded chapter. No source span
/// is invented: expansion and parsing may have moved it from an included file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkFinding {
    pub code: &'static str,
    pub destination: String,
    pub message: &'static str,
}

/// Results for one published chapter, in the book's reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterLinks {
    pub path: String,
    /// Local chapter/anchor and parsed footnote-reference checks, including failures.
    pub checked: usize,
    /// Scheme-bearing and protocol-relative URLs, counted but never fetched.
    pub external: usize,
    /// Other local resources (e.g. downloads), counted but not verified.
    pub unchecked: usize,
    pub findings: Vec<LinkFinding>,
}

/// A deterministic, complete report. Exceeding an admission limit is an error,
/// never a truncated report that might be mistaken for a clean result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkReport {
    pub chapters: Vec<ChapterLinks>,
}

impl LinkReport {
    #[must_use]
    pub fn finding_count(&self) -> usize {
        self.chapters.iter().map(|chapter| chapter.findings.len()).sum()
    }

    /// Serialize without retaining source bodies or image/font bytes.
    ///
    /// # Errors
    /// Rejects JSON output above 4 MiB. This also bounds manually built reports.
    pub fn to_json(&self) -> Result<String> {
        let mut out = Json(String::new());
        out.raw("{\"schema\":\"fmd-book-link-report-v1\",\"scope\":\"expanded-html-navigation\",\"chapters\":[")?;
        let mut totals = [0usize; 4];
        for (index, chapter) in self.chapters.iter().enumerate() {
            if index > 0 { out.raw(",")?; }
            out.raw("{\"path\":")?; out.string(&chapter.path)?;
            out.number("checked", chapter.checked)?;
            totals[0] = totals[0].checked_add(chapter.checked).ok_or_else(|| invalid("count overflow"))?;
            out.number("external", chapter.external)?;
            out.number("unchecked", chapter.unchecked)?;
            totals[1] = totals[1].checked_add(chapter.external).ok_or_else(|| invalid("count overflow"))?;
            totals[2] = totals[2].checked_add(chapter.unchecked).ok_or_else(|| invalid("count overflow"))?;
            totals[3] = totals[3].checked_add(chapter.findings.len()).ok_or_else(|| invalid("count overflow"))?;
            out.raw(",\"findings\":[")?;
            for (index, finding) in chapter.findings.iter().enumerate() {
                if index > 0 { out.raw(",")?; }
                out.raw("{\"code\":")?; out.string(finding.code)?;
                out.raw(",\"destination\":")?; out.string(&finding.destination)?;
                out.raw(",\"message\":")?; out.string(finding.message)?; out.raw("}")?;
            }
            out.raw("]}")?;
        }
        out.raw("],\"summary\":{\"chapters\":")?;
        out.raw(&self.chapters.len().to_string())?;
        for (name, value) in ["checked", "external", "unchecked", "findings"].into_iter().zip(totals) {
            out.number(name, value)?;
        }
        out.raw("}}")?;
        Ok(out.0)
    }
}

impl BookRenderer {
    /// Check chapter links and anchors on the retained parsed book, without
    /// rendering pages, loading assets, or parsing the chapters a second time.
    /// Use `from_sources` first when the supplied source contains includes.
    ///
    /// # Errors
    /// See [`check_book_links`].
    pub fn validate_links(&self) -> Result<LinkReport> {
        check_book_links(self.book())
    }
}

/// Validate links against the same chapter paths and heading-ID algorithm used
/// by HTML publication. Handles forward references, nested containers, queries,
/// percent-encoded fragments, duplicate headings, and referenced notes in their
/// actual emission order. Unreferenced note bodies and code examples are ignored.
///
/// The report verifies Markdown chapter navigation, not external URLs, local
/// downloads, raw HTML IDs, asset availability, or final PDF/EPUB destinations.
/// It does not mutate the book or grant access to any file or URL.
///
/// # Errors
/// Rejects empty/invalid books, over 4096 chapters, over 250000 AST nodes,
/// nesting over 128, over 64 MiB of AST text, over 4096 findings, or excessive
/// report text. No partial report is returned on an admission failure.
pub fn check_book_links(book: &Book) -> Result<LinkReport> {
    if book.chapters.is_empty() || book.chapters.len() > 4096 {
        return Err(invalid("expected 1 through 4096 chapters"));
    }
    let mut known = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    let mut portable = BTreeSet::new();
    let mut budget = Budget::default();
    for (index, chapter) in book.chapters.iter().enumerate() {
        budget.text(&chapter.path)?;
        let path = paths::source_path(&chapter.path).ok_or_else(|| invalid("invalid chapter path"))?;
        if !markdown(&path) { return Err(invalid("navigation validation requires Markdown chapter paths")); }
        if known.insert(path.clone(), index).is_some() { return Err(invalid("duplicate chapter path")); }
        if chapter.out_name != super::out_name(&path) || chapter.out_name.len() > 255
            || !portable.insert(chapter.out_name.to_ascii_lowercase()) {
            return Err(invalid("invalid or colliding chapter output name"));
        }
        outputs.insert(chapter.out_name.as_str(), index);
        admit_blocks(&chapter.doc.blocks, 0, &mut budget)?;
    }
    // Build every chapter's anchors before checking links: forward references
    // must not depend on file order. Only heading blocks are cloned, never a
    // paragraph-sized index with a copy of a long heading ID on every row.
    let navigation: Vec<_> = book.chapters.iter().map(|chapter| navigation(&chapter.doc)).collect();
    let mut chapters = Vec::with_capacity(book.chapters.len());
    let mut finding_count = 0usize;
    let mut finding_bytes = 0usize;
    for (index, chapter) in book.chapters.iter().enumerate() {
        let source = paths::source_path(&chapter.path).ok_or_else(|| invalid("invalid chapter path"))?;
        let nav = &navigation[index];
        let mut result = ChapterLinks { path: source.clone(), checked: 0, external: 0, unchecked: 0, findings: Vec::new() };
        for reference in &nav.references {
            let (destination, problem) = match reference {
                Reference::Note(id, _) => {
                    result.checked += 1;
                    (*id, (!nav.definitions.contains_key(id)).then_some(("missing_footnote", "This parsed footnote reference has no definition in its chapter.")))
                }
                Reference::Link(destination, _) => {
                    let dest = destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
                    if dest.starts_with("//") || scheme(dest) {
                        result.external += 1;
                        continue;
                    }
                    let local = if dest.is_empty() || dest.starts_with(['#', '?']) {
                        Some((index, dest.to_string()))
                    } else if let Some((path, suffix)) = paths::destination(&source, dest) {
                        if let Some(&chapter) = known.get(&path) { Some((chapter, suffix)) }
                        else {
                            // Already-output-addressed links remain literal in
                            // HTML. Recognize exact flat output names as well.
                            let end = dest.find(['#', '?']).unwrap_or(dest.len());
                            let output = decode(&dest[..end]);
                            let output = output.as_deref().map(|path| path.strip_prefix("./").unwrap_or(path));
                            if let Some(&chapter) = output.and_then(|path| outputs.get(path)) {
                                Some((chapter, suffix))
                            } else if markdown(&path) {
                                result.checked += 1;
                                push_finding(&mut result, destination, "missing_chapter", "This Markdown chapter is not in the published book.", &mut finding_count, &mut finding_bytes)?;
                                continue;
                            } else { result.unchecked += 1; continue; }
                        }
                    } else {
                        result.checked += 1;
                        push_finding(&mut result, destination, "invalid_local_destination", "This local destination is malformed or escapes the book root.", &mut finding_count, &mut finding_bytes)?;
                        continue;
                    };
                    let Some((target, suffix)) = local else { continue; };
                    result.checked += 1;
                    let problem = resolve_fragment(&navigation[target], &suffix).err();
                    (*destination, problem)
                }
            };
            if let Some((code, message)) = problem {
                push_finding(&mut result, destination, code, message, &mut finding_count, &mut finding_bytes)?;
            }
        }
        chapters.push(result);
    }
    Ok(LinkReport { chapters })
}

fn invalid(message: &str) -> RenderError { RenderError::InvalidInput(format!("book_validation: {message}")) }
fn markdown(path: &str) -> bool { let lower = path.to_ascii_lowercase(); lower.ends_with(".md") || lower.ends_with(".markdown") }
fn scheme(value: &str) -> bool {
    let Some((prefix, _)) = value.split_once(':') else { return false; };
    !prefix.is_empty() && prefix.as_bytes()[0].is_ascii_alphabetic()
        && prefix.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}
fn decode(value: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> { match byte { b'0'..=b'9' => Some(byte - b'0'), b'a'..=b'f' => Some(byte - b'a' + 10), b'A'..=b'F' => Some(byte - b'A' + 10), _ => None } }
    let bytes = value.as_bytes(); let mut output = Vec::with_capacity(bytes.len()); let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' { output.push(hex(*bytes.get(index + 1)?)? * 16 + hex(*bytes.get(index + 2)?)?); index += 3; }
        else { output.push(bytes[index]); index += 1; }
    }
    let decoded = String::from_utf8(output).ok()?;
    (!decoded.chars().any(char::is_control)).then_some(decoded)
}
fn push_finding(chapter: &mut ChapterLinks, dest: &str, code: &'static str, message: &'static str, count: &mut usize, bytes: &mut usize) -> Result<()> {
    if *count >= MAX_FINDINGS || dest.len() > 8192 || dest.len() > (256 * 1024usize).saturating_sub(*bytes) {
        return Err(invalid("finding count or destination text exceeds the report limit"));
    }
    *count += 1; *bytes += dest.len();
    chapter.findings.push(LinkFinding { code, destination: dest.to_string(), message });
    Ok(())
}

#[derive(Default)]
struct Budget { nodes: usize, bytes: usize }
impl Budget {
    fn node(&mut self, depth: usize) -> Result<()> {
        if depth > 128 || self.nodes >= MAX_NODES { return Err(invalid("AST depth or node budget exceeded")); }
        self.nodes += 1; Ok(())
    }
    fn text(&mut self, text: &str) -> Result<()> {
        if text.len() > MAX_TEXT_BYTES.saturating_sub(self.bytes) { return Err(invalid("AST text exceeds 64 MiB")); }
        self.bytes += text.len(); Ok(())
    }
}
fn admit_blocks(blocks: &[Block], depth: usize, budget: &mut Budget) -> Result<()> {
    for block in blocks {
        budget.node(depth)?;
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => admit_inlines(inlines, depth + 1, budget)?,
            Block::BlockQuote(inner) => admit_blocks(inner, depth + 1, budget)?,
            Block::FootnoteDefinition { id, blocks } => { budget.text(id)?; admit_blocks(blocks, depth + 1, budget)?; }
            Block::List(list) => for item in &list.items { budget.node(depth)?; admit_blocks(&item.blocks, depth + 1, budget)?; },
            Block::Table(table) => for row in std::iter::once(&table.head).chain(&table.rows) {
                budget.node(depth)?;
                for cell in row { budget.node(depth)?; admit_inlines(cell, depth + 1, budget)?; }
            },
            Block::DefinitionList(items) => for item in items {
                budget.node(depth)?;
                for inlines in item.terms.iter().chain(&item.definitions) { budget.node(depth)?; admit_inlines(inlines, depth + 1, budget)?; }
            },
            Block::CodeBlock { lang, code } => { if let Some(lang) = lang { budget.text(lang)?; } budget.text(code)?; }
            Block::MathBlock(text) | Block::HtmlBlock(text) => budget.text(text)?,
            _ => {}
        }
    }
    Ok(())
}
fn admit_inlines(inlines: &[Inline], depth: usize, budget: &mut Budget) -> Result<()> {
    for inline in inlines {
        budget.node(depth)?;
        match inline {
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => admit_inlines(inner, depth + 1, budget)?,
            Inline::Link { dest, title, content } => { budget.text(dest)?; if let Some(title) = title { budget.text(title)?; } admit_inlines(content, depth + 1, budget)?; }
            Inline::Image { dest, title, alt } => { budget.text(dest)?; budget.text(alt)?; if let Some(title) = title { budget.text(title)?; } }
            Inline::Text(text) | Inline::Code(text) | Inline::Html(text) | Inline::Math(text) | Inline::DisplayMath(text) => budget.text(text)?,
            Inline::FootnoteRef { id } => budget.text(id)?,
            _ => {}
        }
    }
    Ok(())
}

// Keep source ownership at top-level AST-block granularity. Nested and note
// visitors retain the owning index; no substring search invents an inline span.
enum Reference<'a> { Link(&'a str, usize), Note(&'a str, usize) }
struct Navigation<'a> {
    anchors: BTreeMap<String, DocumentAnchor>,
    definitions: BTreeMap<&'a str, (&'a [Block], usize)>,
    references: Vec<Reference<'a>>,
    order: Vec<&'a str>,
    order_blocks: Vec<usize>,
    seen: BTreeSet<&'a str>,
    headings: Vec<Block>,
    heading_blocks: Vec<usize>,
}
impl Navigation<'_> {
    fn anchor(&mut self, id: String, block_index: usize, title: String, kind: AnchorKind) {
        self.anchors.entry(id.clone()).and_modify(|entry| entry.occurrences += 1)
            .or_insert(DocumentAnchor { id, block_index, title, kind, occurrences: 1 });
    }
}
fn navigation(doc: &Document) -> Navigation<'_> {
    let mut nav = Navigation {
        anchors: BTreeMap::new(), definitions: BTreeMap::new(), references: Vec::new(),
        order: Vec::new(), order_blocks: Vec::new(), seen: BTreeSet::new(), headings: Vec::new(), heading_blocks: Vec::new(),
    };
    for (index, block) in doc.blocks.iter().enumerate() {
        definitions(std::slice::from_ref(block), index, &mut nav.definitions);
    }
    for (index, block) in doc.blocks.iter().enumerate() {
        visit_blocks(std::slice::from_ref(block), index, &mut nav);
    }
    let mut cursor = 0;
    while let Some(&id) = nav.order.get(cursor) {
        let reference_block = nav.order_blocks[cursor];
        cursor += 1;
        if let Some(&(blocks, definition_block)) = nav.definitions.get(id) {
            nav.anchor(format!("fn-{id}"), definition_block, id.to_string(), AnchorKind::Footnote);
            nav.anchor(format!("fnref-{cursor}"), reference_block, id.to_string(), AnchorKind::FootnoteBackreference);
            visit_blocks(blocks, definition_block, &mut nav);
        }
    }
    let headings = Document { blocks: std::mem::take(&mut nav.headings) };
    // This temporary document contains ONLY headings, so each index entry has
    // exactly one corresponding owner. IDs use the publication algorithm.
    let owners = std::mem::take(&mut nav.heading_blocks);
    for (entry, block_index) in build_search_index(&headings).entries.into_iter().zip(owners) {
        nav.anchor(entry.anchor, block_index, entry.text, AnchorKind::Heading);
    }
    nav
}
fn definitions<'a>(blocks: &'a [Block], owner: usize, defs: &mut BTreeMap<&'a str, (&'a [Block], usize)>) {
    for block in blocks { match block {
        Block::FootnoteDefinition { id, blocks } => { defs.entry(id).or_insert((blocks, owner)); definitions(blocks, owner, defs); }
        Block::BlockQuote(blocks) => definitions(blocks, owner, defs),
        Block::List(list) => for item in &list.items { definitions(&item.blocks, owner, defs); },
        _ => {}
    } }
}
fn visit_blocks<'a>(blocks: &'a [Block], owner: usize, nav: &mut Navigation<'a>) {
    for block in blocks { match block {
        Block::Heading { inlines, .. } => {
            nav.headings.push(block.clone()); nav.heading_blocks.push(owner);
            visit_inlines(inlines, owner, nav);
        }
        Block::Paragraph(inlines) => visit_inlines(inlines, owner, nav),
        Block::BlockQuote(blocks) => visit_blocks(blocks, owner, nav),
        Block::List(list) => for item in &list.items { visit_blocks(&item.blocks, owner, nav); },
        Block::Table(table) => for cell in table.head.iter().chain(table.rows.iter().flatten()) { visit_inlines(cell, owner, nav); },
        Block::DefinitionList(items) => for item in items { for inlines in item.terms.iter().chain(&item.definitions) { visit_inlines(inlines, owner, nav); } },
        _ => {}
    } }
}
fn visit_inlines<'a>(inlines: &'a [Inline], owner: usize, nav: &mut Navigation<'a>) {
    for inline in inlines { match inline {
        Inline::Link { dest, content, .. } => { nav.references.push(Reference::Link(dest, owner)); visit_inlines(content, owner, nav); }
        Inline::FootnoteRef { id } => {
            nav.references.push(Reference::Note(id, owner));
            if let Some((&id, _)) = nav.definitions.get_key_value(id.as_str()) {
                if nav.seen.insert(id) { nav.order.push(id); nav.order_blocks.push(owner); }
            }
        }
        Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => visit_inlines(inner, owner, nav),
        _ => {}
    } }
}

// Shared by book checks and editor/document analysis. Decode exactly once;
// empty fragments denote the document itself rather than a missing heading.
fn resolve_fragment<'a>(nav: &'a Navigation<'_>, destination: &str)
    -> std::result::Result<Option<&'a DocumentAnchor>, (&'static str, &'static str)>
{
    let Some((_, fragment)) = destination.split_once('#') else { return Ok(None); };
    if fragment.is_empty() { return Ok(None); }
    let id = decode(fragment).ok_or(("invalid_fragment", "This fragment is not valid percent-encoded UTF-8."))?;
    match nav.anchors.get(&id) {
        None => Err(("missing_anchor", "The target chapter has no emitted heading or referenced-footnote anchor with this ID.")),
        Some(anchor) if anchor.occurrences == 1 => Ok(Some(anchor)),
        Some(_) => Err(("ambiguous_anchor", "More than one emitted element uses this anchor; rename the colliding heading or note.")),
    }
}

struct Json(String);
impl Json {
    fn raw(&mut self, text: &str) -> Result<()> {
        if text.len() > MAX_REPORT_BYTES.saturating_sub(self.0.len()) { return Err(invalid("report exceeds 4 MiB")); }
        self.0.push_str(text); Ok(())
    }
    fn number(&mut self, key: &str, value: usize) -> Result<()> { self.raw(",")?; self.string(key)?; self.raw(":")?; self.raw(&value.to_string()) }
    fn string(&mut self, text: &str) -> Result<()> {
        self.raw("\"")?;
        for ch in text.chars() {
            match ch {
                '"' => self.raw("\\\"")?, '\\' => self.raw("\\\\")?,
                ch if ch < ' ' => self.raw(&format!("\\u{:04x}", ch as u32))?,
                ch => { let mut buffer = [0; 4]; self.raw(ch.encode_utf8(&mut buffer))?; }
            }
        }
        self.raw("\"")
    }
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
