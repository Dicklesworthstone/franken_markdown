//! Dependency-aware change analysis for editor/reflow hosts (FCB-037.A).
//!
//! A byte range alone cannot prove that distant Markdown is unchanged: an edit
//! can open a fence, introduce a reference definition, or renumber footnotes.
//! `invalidate` therefore fails closed. `compare` parses both snapshots with the
//! shared parser and permits reuse only after exact source AND AST comparison.
//! Hashes are diagnostic fingerprints, never correctness or cache-hit proofs.

#![forbid(unsafe_code)]

use crate::ast::{Block, Inline};
use crate::span::{SourceSpan, SpannedDocument};

pub mod browser;
pub use browser::{BrowserFlowError, BrowserFlowSession};

pub mod cache;
pub use cache::{FlowShapeCache, FlowShapeCacheLimits, FlowShapeCacheStats};

mod matching;

pub mod session;
pub use session::{FlowAssetRemap, FlowAssetReuse, FlowSession, FlowSessionError, FlowUpdate};

/// A conservative lexical dependency candidate, not a second Markdown parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DependencyKind {
    Reference { label: String },
    Footnote { label: String },
    /// A parsed heading's base slug. Its span may enclose a nested container.
    Heading { level: u8, slug: String },
    Include { path: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentDependency {
    pub kind: DependencyKind,
    /// UTF-8 byte boundaries in the captured source.
    pub start: usize,
    pub end: usize,
    /// Fingerprint of the source envelope, NOT of an unobserved external asset.
    pub content_digest: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationResult {
    pub unchanged: Vec<(usize, usize)>,
    pub dirty: Vec<(usize, usize)>,
    pub distant_dirty: bool,
}

/// One top-level block verified reusable between two captured source versions.
/// Coordinates belong to their respective versions; never reuse old offsets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusableBlock {
    pub old_index: usize,
    pub new_index: usize,
    pub old_span: SourceSpan,
    pub new_span: SourceSpan,
}

/// Matching block content/intrinsic inputs, not reusable absolute coordinates.
/// Options, fonts, resource bytes, adjacent spacing and pagination still belong
/// to the host cache key. This does not permit splicing serialized PDF/HTML.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentChangeSet {
    pub reusable: Vec<ReusableBlock>,
    pub dirty_blocks: Vec<usize>,
    pub removed_blocks: Vec<usize>,
    /// Heading/footnote context changed, so all block reuse was refused.
    pub global_context_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RenderContext {
    Frontmatter(String),
    Heading(u8, Vec<Inline>),
    FootnoteDefinition(String, Vec<Block>),
    FootnoteReference(String),
    Include(String),
}

/// An owned source/AST snapshot. No filesystem, network or host fonts are read.
#[derive(Debug)]
pub struct DependencyGraph {
    deps: Vec<DocumentDependency>,
    source: String,
    document: SpannedDocument,
    context: Vec<RenderContext>,
}

impl DependencyGraph {
    /// Capture a source snapshot using the same parser as the renderers.
    /// The candidate inventory can over-report syntax inside code; it is for
    /// discovery only and NEVER used to prove that a subtree is unchanged.
    #[must_use]
    pub fn scan(source: &str) -> Self {
        Self::from_document(source, crate::parse_markdown_spanned(source))
    }

    pub(crate) fn from_document(source: &str, document: SpannedDocument) -> Self {
        let mut deps = candidates(source);
        let mut context = Vec::new();
        let normalized = source.strip_prefix('\u{feff}').unwrap_or(source);
        let (metadata, body) = crate::parse::split_frontmatter(normalized);
        if metadata.is_some() {
            context.push(RenderContext::Frontmatter(normalized[..normalized.len() - body.len()].to_owned()));
        }
        enum Node<'a> { Block(&'a Block, SourceSpan), Inline(&'a Inline) }
        let mut stack = Vec::new();
        for block in document.blocks().iter().rev() {
            stack.push(Node::Block(&block.node, block.span));
        }
        while let Some(node) = stack.pop() {
            match node {
                Node::Block(block, span) => match block {
                    Block::Heading { level, inlines } => {
                        context.push(RenderContext::Heading(*level, inlines.clone()));
                        let slug = crate::html::slug_inlines(inlines);
                        deps.push(DocumentDependency {
                            kind: DependencyKind::Heading {
                                level: *level,
                                slug: if slug.is_empty() { "section".to_owned() } else { slug },
                            },
                            start: span.start, end: span.end,
                            content_digest: fingerprint(span.slice(source).unwrap_or_default()),
                        });
                        stack.extend(inlines.iter().rev().map(Node::Inline));
                    }
                    Block::Paragraph(inlines) => stack.extend(inlines.iter().rev().map(Node::Inline)),
                    Block::FootnoteDefinition { id, blocks } => {
                        context.push(RenderContext::FootnoteDefinition(id.clone(), blocks.clone()));
                        stack.extend(blocks.iter().rev().map(|block| Node::Block(block, span)));
                    }
                    Block::BlockQuote(blocks) => {
                        stack.extend(blocks.iter().rev().map(|block| Node::Block(block, span)));
                    }
                    Block::List(list) => {
                        for item in list.items.iter().rev() {
                            stack.extend(item.blocks.iter().rev().map(|block| Node::Block(block, span)));
                        }
                    }
                    Block::Table(table) => {
                        for row in table.rows.iter().rev().chain(std::iter::once(&table.head)) {
                            for cell in row.iter().rev() { stack.extend(cell.iter().rev().map(Node::Inline)); }
                        }
                    }
                    Block::DefinitionList(items) => {
                        for item in items.iter().rev() {
                            for content in item.definitions.iter().rev().chain(item.terms.iter().rev()) {
                                stack.extend(content.iter().rev().map(Node::Inline));
                            }
                        }
                    }
                    _ => {}
                },
                Node::Inline(inline) => match inline {
                    Inline::FootnoteRef { id } => context.push(RenderContext::FootnoteReference(id.clone())),
                    Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strikethrough(children)
                    | Inline::Link { content: children, .. } => stack.extend(children.iter().rev().map(Node::Inline)),
                    _ => {}
                },
            }
        }
        // Includes are host-owned. Changing their directive text disables reuse;
        // hosts must still invalidate when the external bytes change in place.
        for dep in &deps {
            if let DependencyKind::Include { path } = &dep.kind {
                context.push(RenderContext::Include(path.clone()));
            }
        }
        deps.sort_by_key(|dep| (dep.start, dep.end));
        Self { deps, source: source.to_owned(), document, context }
    }

    #[must_use]
    pub fn dependencies(&self) -> &[DocumentDependency] { &self.deps }

    #[must_use]
    pub fn source(&self) -> &str { &self.source }

    /// A range-only invalidation cannot know the replacement text. Even a
    /// zero-width insertion can change every later block or a forward reference.
    /// Invalid ranges also fail closed. Use `compare` for verified reuse.
    #[must_use]
    pub fn invalidate(&self, _changed_start: usize, _changed_end: usize) -> InvalidationResult {
        let dirty = if self.source.is_empty() { Vec::new() } else { vec![(0, self.source.len())] };
        InvalidationResult { unchanged: Vec::new(), distant_dirty: !dirty.is_empty(), dirty }
    }

    /// Compare exact parsed snapshots, including resolved reference destinations.
    /// Reuse common edges and a deterministic, monotone chain of verified middle
    /// anchors. Ambiguous unmatched occurrences remain dirty. Heading and footnote
    /// context changes invalidate all blocks. External resources behind unchanged
    /// URLs are NOT verified here; every reused block still needs layout context.
    #[must_use]
    pub fn compare(&self, next: &Self) -> DocumentChangeSet {
        let old = self.document.blocks();
        let new = next.document.blocks();
        let global_context_changed = self.context != next.context;
        let pairs = if global_context_changed {
            Vec::new()
        } else {
            matching::reusable_indices(self, next)
        };
        let mut reusable = Vec::with_capacity(pairs.len());
        let mut dirty_blocks = Vec::new();
        let mut removed_blocks = Vec::new();
        let mut old_cursor = 0;
        let mut new_cursor = 0;
        for (a, b) in pairs {
            removed_blocks.extend(old_cursor..a);
            dirty_blocks.extend(new_cursor..b);
            reusable.push(ReusableBlock {
                old_index: a, new_index: b, old_span: old[a].span, new_span: new[b].span,
            });
            old_cursor = a + 1;
            new_cursor = b + 1;
        }
        removed_blocks.extend(old_cursor..old.len());
        dirty_blocks.extend(new_cursor..new.len());
        DocumentChangeSet {
            reusable, dirty_blocks, removed_blocks, global_context_changed,
        }
    }
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

// Discovery only. Every search consumes the bytes it inspected, or terminates
// the current line on failure. No suffix rescan at every unmatched '['; no
// byte-by-byte slicing through a Unicode scalar. Syntax is decided by the AST.
fn candidates(source: &str) -> Vec<DocumentDependency> {
    let mut out = Vec::new();
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        let mut pos = 0;
        while pos < line.len() {
            let rest = &line[pos..];
            let (kind, used) = if let Some(body) = rest.strip_prefix("{{include:") {
                let Some(close) = body.find("}}") else { break; };
                (DependencyKind::Include { path: body[..close].trim().to_owned() }, 10 + close + 2)
            } else if let Some(body) = rest.strip_prefix('[') {
                let Some(close) = body.find(']') else { break; };
                let label = &body[..close];
                let end = close + 2;
                let after = &rest[end..];
                if after.starts_with('(') {
                    pos += end + after.find(')').map_or(after.len(), |i| i + 1);
                    continue;
                }
                let (kind, used) = if let Some(note) = label.strip_prefix('^') {
                    (DependencyKind::Footnote { label: note.to_owned() }, end)
                } else if let Some(second) = after.strip_prefix('[') {
                    let Some(close) = second.find(']') else { break; };
                    let target = &second[..close];
                    (DependencyKind::Reference { label: if target.is_empty() { label } else { target }.to_owned() }, end + close + 2)
                } else {
                    (DependencyKind::Reference { label: label.to_owned() }, end)
                };
                // The destination/title/body is part of a definition's envelope.
                (kind, if after.starts_with(':') { rest.len() } else { used })
            } else {
                let Some(ch) = rest.chars().next() else { break; };
                pos += ch.len_utf8();
                continue;
            };
            let end = pos + used;
            out.push(DocumentDependency {
                kind, start: line_start + pos, end: line_start + end,
                content_digest: fingerprint(&line[pos..end]),
            });
            pos = end;
        }
        line_start += line.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_and_unterminated_syntax_never_slice_inside_a_scalar() {
        for source in ["é東京😀 [label]", "[x](é", "[^é]: 東京\r\n", "{{include: café.md}}", "[" ] {
            let graph = DependencyGraph::scan(source);
            for dep in graph.dependencies() {
                assert!(source.get(dep.start..dep.end).is_some());
                assert_ne!(dep.content_digest, 0);
            }
        }
        let graph = DependencyGraph::scan("é {{include: café.md}}");
        assert!(graph.dependencies().iter().any(|dep| matches!(&dep.kind,
            DependencyKind::Include { path } if path == "café.md")));
        let hostile = format!("{}é", "[".repeat(32_000));
        assert!(DependencyGraph::scan(&hostile).dependencies().is_empty());
    }

    #[test]
    fn range_only_edits_and_insertions_never_claim_verified_unchanged_content() {
        let graph = DependencyGraph::scan("first\n\n[use][r]\n\n[r]: old\n");
        for (start, end) in [(0, 0), (5, 5), (0, 5), (usize::MAX, 0)] {
            let result = graph.invalidate(start, end);
            assert!(result.unchanged.is_empty());
            assert_eq!(result.dirty, vec![(0, graph.source().len())]);
            assert!(result.distant_dirty);
        }
    }

    #[test]
    fn reference_destination_changes_invalidate_distant_resolved_ast() {
        let a = DependencyGraph::scan("[use][r]\n\nunchanged\n\n[r]: old\n");
        let b = DependencyGraph::scan("[use][r]\n\nunchanged\n\n[r]: new\n");
        let change = a.compare(&b);
        assert!(change.dirty_blocks.contains(&0));
        assert!(change.reusable.iter().all(|item| item.old_index != 0));
        assert!(change.reusable.iter().any(|item| item.old_index == 1));
        assert!(!change.global_context_changed);
    }

    #[test]
    fn new_reference_definition_can_change_a_previously_literal_use() {
        let a = DependencyGraph::scan("[ref]\n\nlast\n");
        let b = DependencyGraph::scan("[ref]\n\nlast\n\n[ref]: target\n");
        assert!(a.compare(&b).dirty_blocks.contains(&0));
    }

    #[test]
    fn unchanged_suffix_uses_new_offsets_after_unicode_edit() {
        let a = DependencyGraph::scan("before\n\nold\n\nafter\n");
        let b = DependencyGraph::scan("before\n\n東京 changed\n\nafter\n");
        let change = a.compare(&b);
        assert_eq!(change.dirty_blocks, vec![1]);
        assert_eq!(change.removed_blocks, vec![1]);
        assert_eq!(change.reusable.len(), 2);
        let tail = &change.reusable[1];
        assert_ne!(tail.old_span.start, tail.new_span.start);
        assert_eq!(tail.old_span.slice(a.source()), tail.new_span.slice(b.source()));
    }

    #[test]
    fn heading_and_footnote_context_changes_refuse_global_reuse() {
        for (old, new) in [
            ("# Same\n\nbody\n", "# Other\n\nbody\n"),
            ("---\nlang=en\n---\nbody\n", "---\nlang=de\n---\nbody\n"),
            ("text[^a]\n\n[^a]: old\n", "text[^a]\n\n[^a]: new\n"),
            ("{{include: a.md}}\n\nbody", "{{include: b.md}}\n\nbody"),
        ] {
            let change = DependencyGraph::scan(old).compare(&DependencyGraph::scan(new));
            assert!(change.global_context_changed);
            assert!(change.reusable.is_empty());
        }
    }

    #[test]
    fn fence_edits_and_duplicate_blocks_do_not_reuse_the_wrong_structure() {
        let a = DependencyGraph::scan("one\n\none\n\nlast\n");
        let b = DependencyGraph::scan("```\none\n\none\n\nlast\n");
        let change = a.compare(&b);
        assert!(change.reusable.is_empty());
        assert_eq!(change.dirty_blocks, vec![0]);
        let same = a.compare(&DependencyGraph::scan(a.source()));
        assert_eq!(same.reusable.len(), 3);
        assert!(same.dirty_blocks.is_empty());
        assert!(same.removed_blocks.is_empty());
    }
}
