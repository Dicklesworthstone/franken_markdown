//! Semantic Markdown AST diffing and visual change rendering.
//!
//! Provides structural LCS diffing between two Markdown ASTs at the block and
//! inline levels, calculating similarity scores and rendering visual green/red
//! side-by-side or unified change reports in HTML and PDF.

use crate::ast::{Block, Document, Inline};
use crate::theme::Theme;

/// High-level change metrics between two Markdown documents.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiffStats {
    /// Number of unchanged blocks.
    pub unchanged_blocks: usize,
    /// Number of inserted blocks.
    pub inserted_blocks: usize,
    /// Number of deleted blocks.
    pub deleted_blocks: usize,
    /// Number of modified blocks.
    pub modified_blocks: usize,
    /// Word-level insertion count.
    pub words_inserted: usize,
    /// Word-level deletion count.
    pub words_deleted: usize,
    /// Structural similarity coefficient (0.0 = completely different, 1.0 = identical).
    pub similarity_ratio: f32,
}

/// A semantic diff element at the block level.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffBlock {
    /// Block is identical in both documents.
    Unchanged(Block),
    /// Block was added in the new document.
    Inserted(Block),
    /// Block was removed from the old document.
    Deleted(Block),
    /// Block was modified between old and new versions.
    Modified {
        old: Box<Block>,
        new: Box<Block>,
        inline_diff: Vec<DiffInline>,
    },
}

/// A fine-grained diff element at the inline/phrase level.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffInline {
    /// Inline text/element is unchanged.
    Unchanged(Inline),
    /// Inline text/element was inserted.
    Inserted(Inline),
    /// Inline text/element was deleted.
    Deleted(Inline),
}

/// Aggregated semantic diff between two Markdown documents.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentDiff {
    pub old_name: String,
    pub new_name: String,
    pub stats: DiffStats,
    pub blocks: Vec<DiffBlock>,
}

#[derive(Debug, Clone, PartialEq)]
enum RawDiffItem<T> {
    Unchanged(T),
    Inserted(T),
    Deleted(T),
}

/// Compute semantic structural diff between two parsed Markdown documents.
#[must_use]
pub fn compute_diff(
    doc_a: &Document,
    doc_b: &Document,
    name_a: &str,
    name_b: &str,
) -> DocumentDiff {
    let raw_diff = lcs_diff(&doc_a.blocks, &doc_b.blocks);
    let mut blocks = Vec::new();
    let mut stats = DiffStats::default();

    let mut i = 0;
    while i < raw_diff.len() {
        match &raw_diff[i] {
            RawDiffItem::Unchanged(b) => {
                stats.unchanged_blocks += 1;
                blocks.push(DiffBlock::Unchanged(b.clone()));
                i += 1;
            }
            RawDiffItem::Deleted(_) | RawDiffItem::Inserted(_) => {
                // Collect contiguous cluster of Deleted and Inserted items
                let mut dels = Vec::new();
                let mut inss = Vec::new();
                while i < raw_diff.len() {
                    match &raw_diff[i] {
                        RawDiffItem::Deleted(b) => {
                            dels.push(b.clone());
                            i += 1;
                        }
                        RawDiffItem::Inserted(b) => {
                            inss.push(b.clone());
                            i += 1;
                        }
                        RawDiffItem::Unchanged(_) => break,
                    }
                }

                let mut used_inss = vec![false; inss.len()];
                for del in dels {
                    let mut matched = false;
                    for (ins_idx, ins) in inss.iter().enumerate() {
                        if !used_inss[ins_idx] && can_pair_blocks(&del, ins) {
                            used_inss[ins_idx] = true;
                            let inline_diff = diff_block_inlines(&del, ins, &mut stats);
                            stats.modified_blocks += 1;
                            blocks.push(DiffBlock::Modified {
                                old: Box::new(del.clone()),
                                new: Box::new(ins.clone()),
                                inline_diff,
                            });
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        stats.deleted_blocks += 1;
                        count_block_words(&del, &mut stats.words_deleted);
                        blocks.push(DiffBlock::Deleted(del));
                    }
                }

                for (ins_idx, ins) in inss.into_iter().enumerate() {
                    if !used_inss[ins_idx] {
                        stats.inserted_blocks += 1;
                        count_block_words(&ins, &mut stats.words_inserted);
                        blocks.push(DiffBlock::Inserted(ins));
                    }
                }
            }
        }
    }

    let total_operations = stats.unchanged_blocks
        + stats.inserted_blocks
        + stats.deleted_blocks
        + stats.modified_blocks;
    stats.similarity_ratio = if total_operations == 0 {
        1.0
    } else {
        (stats.unchanged_blocks as f32) / (total_operations as f32)
    };

    DocumentDiff {
        old_name: name_a.to_string(),
        new_name: name_b.to_string(),
        stats,
        blocks,
    }
}

fn can_pair_blocks(a: &Block, b: &Block) -> bool {
    matches!(
        (a, b),
        (Block::Paragraph(_), Block::Paragraph(_))
            | (Block::Heading { .. }, Block::Heading { .. })
            | (Block::CodeBlock { .. }, Block::CodeBlock { .. })
            | (Block::Table(_), Block::Table(_))
    )
}

fn diff_block_inlines(old: &Block, new: &Block, stats: &mut DiffStats) -> Vec<DiffInline> {
    match (old, new) {
        (Block::Paragraph(in_a), Block::Paragraph(in_b))
        | (Block::Heading { inlines: in_a, .. }, Block::Heading { inlines: in_b, .. }) => {
            let raw = lcs_diff(in_a, in_b);
            raw.into_iter()
                .map(|item| match item {
                    RawDiffItem::Unchanged(inl) => DiffInline::Unchanged(inl),
                    RawDiffItem::Inserted(inl) => {
                        count_inline_words(&inl, &mut stats.words_inserted);
                        DiffInline::Inserted(inl)
                    }
                    RawDiffItem::Deleted(inl) => {
                        count_inline_words(&inl, &mut stats.words_deleted);
                        DiffInline::Deleted(inl)
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn lcs_diff<T: PartialEq + Clone>(old: &[T], new: &[T]) -> Vec<RawDiffItem<T>> {
    let m = old.len();
    let n = new.len();
    if m == 0 && n == 0 {
        return Vec::new();
    }
    if m == 0 {
        return new.iter().cloned().map(RawDiffItem::Inserted).collect();
    }
    if n == 0 {
        return old.iter().cloned().map(RawDiffItem::Deleted).collect();
    }

    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 0..m {
        for j in 0..n {
            if old[i] == new[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    let mut diff = Vec::with_capacity(m + n);
    let mut i = m;
    let mut j = n;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            diff.push(RawDiffItem::Unchanged(old[i - 1].clone()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            diff.push(RawDiffItem::Inserted(new[j - 1].clone()));
            j -= 1;
        } else if i > 0 && (j == 0 || dp[i][j - 1] < dp[i - 1][j]) {
            diff.push(RawDiffItem::Deleted(old[i - 1].clone()));
            i -= 1;
        }
    }

    diff.reverse();
    diff
}

fn count_block_words(block: &Block, count: &mut usize) {
    match block {
        Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
            for inl in inlines {
                count_inline_words(inl, count);
            }
        }
        Block::BlockQuote(inner) => {
            for b in inner {
                count_block_words(b, count);
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for b in &item.blocks {
                    count_block_words(b, count);
                }
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                for inl in cell {
                    count_inline_words(inl, count);
                }
            }
            for row in &table.rows {
                for cell in row {
                    for inl in cell {
                        count_inline_words(inl, count);
                    }
                }
            }
        }
        Block::CodeBlock { code, .. } => {
            *count += code.split_whitespace().count();
        }
        _ => {}
    }
}

fn count_inline_words(inline: &Inline, count: &mut usize) {
    match inline {
        Inline::Text(t) | Inline::Code(t) | Inline::Image { alt: t, .. } => {
            *count += t.split_whitespace().count();
        }
        Inline::Emphasis(inner)
        | Inline::Strong(inner)
        | Inline::Strikethrough(inner)
        | Inline::Link { content: inner, .. } => {
            for inl in inner {
                count_inline_words(inl, count);
            }
        }
        _ => {}
    }
}

impl DocumentDiff {
    /// Render visual diff document to standalone HTML with custom styles.
    #[must_use]
    pub fn to_html(&self, theme: &Theme) -> String {
        let mut out = String::with_capacity(16384);
        out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
        out.push_str("<meta charset=\"utf-8\">\n");
        out.push_str(
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n",
        );
        out.push_str(&format!(
            "<title>Diff: {} vs {}</title>\n",
            html_escape(&self.old_name),
            html_escape(&self.new_name)
        ));
        out.push_str("<style>\n");
        out.push_str(
            r#"
:root {
  --diff-bg: #ffffff;
  --diff-text: #1f2328;
  --ins-bg: #dafbe1;
  --ins-text: #1a7f37;
  --ins-border: #4ac26b;
  --del-bg: #ffebe9;
  --del-text: #cf222e;
  --del-border: #ff8182;
  --header-bg: #f6f8fa;
  --header-border: #d0d7de;
}
@media (prefers-color-scheme: dark) {
  :root {
    --diff-bg: #0d1117;
    --diff-text: #e6edf3;
    --ins-bg: #033a16;
    --ins-text: #3fb950;
    --ins-border: #238636;
    --del-bg: #490202;
    --del-text: #ff7b72;
    --del-border: #da3633;
    --header-bg: #161b22;
    --header-border: #30363d;
  }
}
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
  background: var(--diff-bg);
  color: var(--diff-text);
  margin: 0;
  padding: 24px;
  line-height: 1.6;
}
.diff-container {
  max-width: 960px;
  margin: 0 auto;
}
.diff-header {
  background: var(--header-bg);
  border: 1px solid var(--header-border);
  border-radius: 6px;
  padding: 16px 20px;
  margin-bottom: 24px;
}
.diff-header h1 {
  font-size: 18px;
  margin: 0 0 12px 0;
}
.diff-stats-bar {
  display: flex;
  gap: 16px;
  font-size: 14px;
  font-weight: 500;
}
.diff-badge-ins {
  color: var(--ins-text);
  background: var(--ins-bg);
  padding: 2px 8px;
  border-radius: 12px;
}
.diff-badge-del {
  color: var(--del-text);
  background: var(--del-bg);
  padding: 2px 8px;
  border-radius: 12px;
}
.diff-block-ins {
  background: var(--ins-bg);
  border-left: 4px solid var(--ins-border);
  padding: 8px 16px;
  margin: 12px 0;
  border-radius: 0 4px 4px 0;
}
.diff-block-del {
  background: var(--del-bg);
  border-left: 4px solid var(--del-border);
  padding: 8px 16px;
  margin: 12px 0;
  text-decoration: line-through;
  opacity: 0.85;
  border-radius: 0 4px 4px 0;
}
.diff-block-mod {
  margin: 12px 0;
}
ins.diff-inline {
  background: var(--ins-bg);
  color: var(--ins-text);
  text-decoration: none;
  font-weight: 600;
  padding: 1px 3px;
  border-radius: 3px;
}
del.diff-inline {
  background: var(--del-bg);
  color: var(--del-text);
  text-decoration: line-through;
  opacity: 0.8;
  padding: 1px 3px;
  border-radius: 3px;
}
"#,
        );
        out.push_str("</style>\n</head>\n<body>\n");
        out.push_str("<div class=\"diff-container\">\n");

        // Header summary
        out.push_str("<div class=\"diff-header\">\n");
        out.push_str(&format!(
            "<h1>Comparing <code>{}</code> &rarr; <code>{}</code></h1>\n",
            html_escape(&self.old_name),
            html_escape(&self.new_name)
        ));
        out.push_str("<div class=\"diff-stats-bar\">\n");
        out.push_str(&format!(
            "<span class=\"diff-badge-ins\">+{} blocks (+{} words)</span>\n",
            self.stats.inserted_blocks + self.stats.modified_blocks,
            self.stats.words_inserted
        ));
        out.push_str(&format!(
            "<span class=\"diff-badge-del\">&minus;{} blocks (&minus;{} words)</span>\n",
            self.stats.deleted_blocks + self.stats.modified_blocks,
            self.stats.words_deleted
        ));
        out.push_str(&format!(
            "<span>Similarity: {:.1}%</span>\n",
            self.stats.similarity_ratio * 100.0
        ));
        out.push_str("</div>\n</div>\n\n");

        let html_opts = crate::HtmlOptions {
            theme: theme.clone(),
            ..Default::default()
        };

        // Body blocks
        for block in &self.blocks {
            match block {
                DiffBlock::Unchanged(b) => {
                    let blocks = [b.clone()];
                    out.push_str(&crate::html::render_fragment(&blocks, &html_opts));
                }
                DiffBlock::Inserted(b) => {
                    out.push_str("<div class=\"diff-block-ins\">\n");
                    let blocks = [b.clone()];
                    out.push_str(&crate::html::render_fragment(&blocks, &html_opts));
                    out.push_str("</div>\n");
                }
                DiffBlock::Deleted(b) => {
                    out.push_str("<div class=\"diff-block-del\">\n");
                    let blocks = [b.clone()];
                    out.push_str(&crate::html::render_fragment(&blocks, &html_opts));
                    out.push_str("</div>\n");
                }
                DiffBlock::Modified {
                    inline_diff, new, ..
                } => {
                    out.push_str("<div class=\"diff-block-mod\">\n");
                    if inline_diff.is_empty() {
                        let blocks = [(**new).clone()];
                        out.push_str(&crate::html::render_fragment(&blocks, &html_opts));
                    } else {
                        out.push_str("<p>");
                        for inl in inline_diff {
                            match inl {
                                DiffInline::Unchanged(i) => {
                                    render_diff_inline(i, &mut out);
                                }
                                DiffInline::Inserted(i) => {
                                    out.push_str("<ins class=\"diff-inline\">");
                                    render_diff_inline(i, &mut out);
                                    out.push_str("</ins>");
                                }
                                DiffInline::Deleted(i) => {
                                    out.push_str("<del class=\"diff-inline\">");
                                    render_diff_inline(i, &mut out);
                                    out.push_str("</del>");
                                }
                            }
                        }
                        out.push_str("</p>\n");
                    }
                    out.push_str("</div>\n");
                }
            }
        }

        out.push_str("</div>\n</body>\n</html>\n");
        out
    }

    /// Render machine-readable JSON representation of diff.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(4096);
        out.push('{');
        out.push_str("\"schema\":\"fmd-diff-v1\",");
        out.push_str(&format!(
            "\"old_name\":\"{}\",",
            json_escape(&self.old_name)
        ));
        out.push_str(&format!(
            "\"new_name\":\"{}\",",
            json_escape(&self.new_name)
        ));
        out.push_str("\"stats\":{");
        out.push_str(&format!(
            "\"unchanged_blocks\":{},",
            self.stats.unchanged_blocks
        ));
        out.push_str(&format!(
            "\"inserted_blocks\":{},",
            self.stats.inserted_blocks
        ));
        out.push_str(&format!(
            "\"deleted_blocks\":{},",
            self.stats.deleted_blocks
        ));
        out.push_str(&format!(
            "\"modified_blocks\":{},",
            self.stats.modified_blocks
        ));
        out.push_str(&format!(
            "\"words_inserted\":{},",
            self.stats.words_inserted
        ));
        out.push_str(&format!("\"words_deleted\":{},", self.stats.words_deleted));
        out.push_str(&format!(
            "\"similarity_ratio\":{:.3}",
            self.stats.similarity_ratio
        ));
        out.push_str("},");
        out.push_str(&format!("\"total_diff_blocks\":{}", self.blocks.len()));
        out.push('}');
        out
    }
}

fn render_diff_inline(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(t) => out.push_str(&html_escape(t)),
        Inline::Code(c) => {
            out.push_str("<code>");
            out.push_str(&html_escape(c));
            out.push_str("</code>");
        }
        Inline::Emphasis(inner) => {
            out.push_str("<em>");
            for i in inner {
                render_diff_inline(i, out);
            }
            out.push_str("</em>");
        }
        Inline::Strong(inner) => {
            out.push_str("<strong>");
            for i in inner {
                render_diff_inline(i, out);
            }
            out.push_str("</strong>");
        }
        Inline::Strikethrough(inner) => {
            out.push_str("<del>");
            for i in inner {
                render_diff_inline(i, out);
            }
            out.push_str("</del>");
        }
        Inline::Link { dest, content, .. } => {
            out.push_str(&format!("<a href=\"{}\">", html_escape(dest)));
            for i in content {
                render_diff_inline(i, out);
            }
            out.push_str("</a>");
        }
        Inline::Image { alt, .. } => {
            out.push_str(&format!("[Image: {}]", html_escape(alt)));
        }
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        Inline::Html(h) => out.push_str(h),
        Inline::FootnoteRef { id } => out.push_str(&format!("[^{}]", html_escape(id))),
        Inline::Math(m) => out.push_str(&format!("${}$", html_escape(m))),
        Inline::DisplayMath(m) => out.push_str(&format!("$${}$$", html_escape(m))),
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_markdown;

    #[test]
    fn compute_diff_detects_block_and_inline_modifications() {
        let md_a = "# Title\n\nOriginal paragraph text.\n\n```rust\nlet x = 1;\n```\n";
        let md_b =
            "# Title\n\nUpdated paragraph text with additions.\n\n```rust\nlet x = 2;\n```\n";

        let doc_a = parse_markdown(md_a);
        let doc_b = parse_markdown(md_b);

        let diff = compute_diff(&doc_a, &doc_b, "v1.md", "v2.md");

        assert_eq!(diff.stats.unchanged_blocks, 1); // Heading is unchanged
        assert_eq!(diff.stats.modified_blocks, 2); // Paragraph and CodeBlock are modified
        assert!(diff.stats.words_inserted > 0);

        let html = diff.to_html(&Theme::default());
        assert!(html.contains("Comparing <code>v1.md</code> &rarr; <code>v2.md</code>"));
        assert!(html.contains("<ins class=\"diff-inline\">"));

        let json = diff.to_json();
        assert!(json.contains("\"schema\":\"fmd-diff-v1\""));
        assert!(json.contains("\"modified_blocks\":2"));
    }
}
// ===========================================================================
// Semantic AST diff report: `diff_documents` / `DiffReport` / `report_json` /
// `report_text`.
//
// A complementary, machine-oriented surface next to the visual `DocumentDiff`
// above: it classifies each top-level block change into a stable `ChangeKind`
// taxonomy and renders it as compact per-change JSON or plain text. Alignment
// is heading-aware: blocks are keyed by (kind, canonical text) and aligned
// with the shared `lcs_diff` core over those keys, so a renamed heading is
// reported as `HeadingChanged`, never as remove+add.
//
// Two JSON surfaces live in this module, with distinct schemas:
// - `DocumentDiff::to_json` — schema `fmd-diff-v1`: aggregate stats payload
//   (block/word counters, similarity ratio) for the visual diff.
// - `report_json` — schema `fmd-diff-changes-v1`: per-change taxonomy payload
//   (kind, summary, old/new block indices) from `diff_documents`.
//
// Semantics and deliberate choices:
// - Reflow-insensitive: whitespace runs in prose are collapsed before
//   comparison, so re-wrapping a paragraph is not a change (unlike the
//   `PartialEq`-on-`Block` alignment in `compute_diff`). Code, math, and raw
//   HTML payloads are compared verbatim — their whitespace is meaningful.
// - Ordering is deterministic: changes are emitted in document order; move
//   detection scans old blocks in order and takes the first exact-key match
//   on the new side.
// - Moves are detected by an exact-key match among otherwise unmatched
//   blocks (bounded to 4M candidate pairs; past that the blocks fall back to
//   add/remove reporting). The LCS middle is bounded to 16M DP cells, with a
//   memory-free greedy in-order scan beyond that, so adversarial inputs
//   cannot force an out-of-memory abort.
// - The AST carries no frontmatter block, so `FrontmatterChanged` is
//   reserved and never emitted.
// - Block kinds without a dedicated `ChangeKind` (block quote, thematic
//   break, raw HTML, footnote definition, math block, definition list) are
//   reported under the closest category; `summary` always names the true
//   block kind.

/// The category of a semantic change between two document versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// A heading exists only in the new document.
    HeadingAdded,
    /// A heading exists only in the old document.
    HeadingRemoved,
    /// A heading's text and/or level changed; `level` is the new level.
    HeadingChanged { level: u8 },
    /// Paragraph (or prose container) text changed, or a prose block was
    /// added/removed; inspect `old_index`/`new_index` for the direction.
    ParagraphEdited,
    /// A code-like block changed; `lang` is the new fence info word, if any.
    CodeBlockChanged { lang: Option<String> },
    /// A table's cells, alignment, or shape changed.
    TableChanged,
    /// Link/image text is unchanged but a destination changed.
    LinkTargetChanged { old: String, new: String },
    /// A block is unchanged but its position changed.
    BlockMoved,
    /// A list's items or structure changed.
    ListChanged,
    /// Reserved: the AST has no frontmatter block, so this is never emitted.
    FrontmatterChanged,
}

/// One semantic change, addressed by top-level block indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The change category.
    pub kind: ChangeKind,
    /// Short, deterministic human-readable description.
    pub summary: String,
    /// Index into `old.blocks` (`None` for pure additions).
    pub old_index: Option<usize>,
    /// Index into `new.blocks` (`None` for pure removals).
    pub new_index: Option<usize>,
}

/// The result of [`diff_documents`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffReport {
    /// All changes, in deterministic document order.
    pub changes: Vec<Change>,
    /// True when no semantic differences were found.
    pub identical: bool,
}

/// Separator/marker bytes used inside canonical keys. Key equality is all
/// that matters, so these never leak into rendered output.
const KEY_SEP: char = '\u{4}';

/// A top-level block reduced to its diff-relevant fingerprint.
struct KeyedBlock {
    /// Coarse block-kind tag (see `block_tag`).
    tag: u8,
    /// Canonical key: kind prefix + structural inline markers + link dests.
    key: String,
    /// Same as `key` but with link/image destinations and titles elided;
    /// used to spot `LinkTargetChanged`.
    key_no_dest: String,
    /// Link/image destinations in document order.
    dests: Vec<String>,
    /// Whitespace-normalized plain text for human summaries.
    plain: String,
    /// Heading level (0 for non-headings).
    level: u8,
    /// Code fence info word (None for non-code blocks).
    lang: Option<String>,
}

impl KeyedBlock {
    fn from_block(block: &Block) -> Self {
        let (level, lang) = match block {
            Block::Heading { level, .. } => (*level, None),
            Block::CodeBlock { lang, .. } => (0, lang.clone()),
            _ => (0, None),
        };
        KeyedBlock {
            tag: block_tag(block),
            key: canonical_block(block, true),
            key_no_dest: canonical_block(block, false),
            dests: block_link_dests(block),
            plain: plain_block(block),
            level,
            lang,
        }
    }

    fn same_block(&self, other: &KeyedBlock) -> bool {
        self.tag == other.tag && self.key == other.key
    }
}

/// Coarse, stable block-kind tags.
fn block_tag(block: &Block) -> u8 {
    match block {
        Block::Heading { .. } => 0,
        Block::Paragraph(_) => 1,
        Block::CodeBlock { .. } => 2,
        Block::BlockQuote(_) => 3,
        Block::List(_) => 4,
        Block::Table(_) => 5,
        Block::ThematicBreak => 6,
        Block::HtmlBlock(_) => 7,
        Block::FootnoteDefinition { .. } => 8,
        Block::MathBlock(_) => 9,
        Block::DefinitionList(_) => 10,
    }
}

fn block_name(tag: u8) -> &'static str {
    match tag {
        0 => "heading",
        1 => "paragraph",
        2 => "code block",
        3 => "block quote",
        4 => "list",
        5 => "table",
        6 => "thematic break",
        7 => "HTML block",
        8 => "footnote definition",
        9 => "math block",
        10 => "definition list",
        _ => "block",
    }
}

/// Collapse every whitespace run to a single space and trim the ends.
/// This is what makes paragraph reflow a no-op for the differ.
fn normalize_prose(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Canonical structural text for a block. Prose is whitespace-normalized;
/// code/math/HTML payloads stay verbatim. Inline structure (emphasis, code
/// spans, links, images, math, footnote refs) is preserved as markers, so a
/// formatting change is a real diff.
fn canonical_block(block: &Block, with_dests: bool) -> String {
    match block {
        Block::Heading { level, inlines } => {
            let mut s = String::from("h");
            s.push_str(&level.to_string());
            s.push(':');
            canonical_inlines(inlines, with_dests, &mut s);
            normalize_prose(&s)
        }
        Block::Paragraph(inlines) => {
            let mut s = String::new();
            canonical_inlines(inlines, with_dests, &mut s);
            normalize_prose(&s)
        }
        Block::CodeBlock { lang, code } => {
            let mut s = String::from("c:");
            if let Some(lang) = lang {
                s.push_str(lang);
            }
            s.push(':');
            s.push_str(code.trim_end());
            s
        }
        Block::BlockQuote(inner) => canonical_children(inner, with_dests),
        Block::List(list) => {
            let mut s = String::from(if list.ordered { "ol" } else { "ul" });
            s.push_str(&list.start.to_string());
            s.push(if list.tight { 't' } else { 'l' });
            for item in &list.items {
                s.push(KEY_SEP);
                s.push_str(match item.task {
                    Some(true) => "[x]",
                    Some(false) => "[ ]",
                    None => "",
                });
                for child in &item.blocks {
                    s.push(KEY_SEP);
                    s.push_str(&canonical_block(child, with_dests));
                }
            }
            s
        }
        Block::Table(table) => {
            let mut s = String::new();
            for align in &table.align {
                s.push_str(&(*align as u8).to_string());
            }
            for cell in &table.head {
                s.push(KEY_SEP);
                push_canonical_cell(cell, with_dests, &mut s);
            }
            for row in &table.rows {
                s.push(KEY_SEP);
                for cell in row {
                    s.push(KEY_SEP);
                    push_canonical_cell(cell, with_dests, &mut s);
                }
            }
            s
        }
        Block::ThematicBreak => String::from("hr"),
        Block::HtmlBlock(html) => {
            let mut s = String::from("H:");
            s.push_str(html);
            s
        }
        Block::FootnoteDefinition { id, blocks } => {
            let mut s = String::from("f:");
            s.push_str(id);
            s.push(KEY_SEP);
            s.push_str(&canonical_children(blocks, with_dests));
            s
        }
        Block::MathBlock(math) => {
            let mut s = String::from("m:");
            s.push_str(math.trim());
            s
        }
        Block::DefinitionList(items) => {
            let mut s = String::from("dl");
            for item in items {
                for term in &item.terms {
                    s.push(KEY_SEP);
                    push_canonical_cell(term, with_dests, &mut s);
                }
                for def in &item.definitions {
                    s.push(KEY_SEP);
                    s.push(KEY_SEP);
                    push_canonical_cell(def, with_dests, &mut s);
                }
            }
            s
        }
    }
}

fn canonical_children(blocks: &[Block], with_dests: bool) -> String {
    blocks
        .iter()
        .map(|b| canonical_block(b, with_dests))
        .collect::<Vec<String>>()
        .join("\u{4}")
}

/// A table cell / definition-list entry is prose: normalize its whitespace.
fn push_canonical_cell(inlines: &[Inline], with_dests: bool, out: &mut String) {
    let mut cell = String::new();
    canonical_inlines(inlines, with_dests, &mut cell);
    out.push_str(&normalize_prose(&cell));
}

fn canonical_inlines(inlines: &[Inline], with_dests: bool, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Emphasis(children) => {
                out.push_str("\u{1}em\u{2}");
                canonical_inlines(children, with_dests, out);
                out.push('\u{3}');
            }
            Inline::Strong(children) => {
                out.push_str("\u{1}strong\u{2}");
                canonical_inlines(children, with_dests, out);
                out.push('\u{3}');
            }
            Inline::Strikethrough(children) => {
                out.push_str("\u{1}del\u{2}");
                canonical_inlines(children, with_dests, out);
                out.push('\u{3}');
            }
            Inline::Code(code) => {
                out.push_str("\u{1}code\u{2}");
                out.push_str(code);
                out.push('\u{3}');
            }
            Inline::Link {
                dest,
                title,
                content,
            } => {
                out.push_str("\u{1}a\u{2}");
                canonical_inlines(content, with_dests, out);
                if with_dests {
                    out.push('\u{1}');
                    out.push_str(dest);
                    if let Some(title) = title {
                        out.push('\u{1}');
                        out.push_str(title);
                    }
                }
                out.push('\u{3}');
            }
            Inline::Image { dest, title, alt } => {
                out.push_str("\u{1}img\u{2}");
                out.push_str(alt);
                if with_dests {
                    out.push('\u{1}');
                    out.push_str(dest);
                    if let Some(title) = title {
                        out.push('\u{1}');
                        out.push_str(title);
                    }
                }
                out.push('\u{3}');
            }
            Inline::SoftBreak => out.push(' '),
            Inline::HardBreak => out.push_str("\u{1}br\u{2}"),
            Inline::Html(html) => {
                out.push_str("\u{1}html\u{2}");
                out.push_str(html);
                out.push('\u{3}');
            }
            Inline::FootnoteRef { id } => {
                out.push_str("\u{1}fn\u{2}");
                out.push_str(id);
                out.push('\u{3}');
            }
            Inline::Math(math) => {
                out.push_str("\u{1}math\u{2}");
                out.push_str(math.trim());
                out.push('\u{3}');
            }
            Inline::DisplayMath(math) => {
                out.push_str("\u{1}dmath\u{2}");
                out.push_str(math.trim());
                out.push('\u{3}');
            }
        }
    }
}

/// Link/image destinations of a block, in document order.
fn block_link_dests(block: &Block) -> Vec<String> {
    let mut out = Vec::new();
    collect_block_dests(block, &mut out);
    out
}

fn collect_block_dests(block: &Block, out: &mut Vec<String>) {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            collect_inline_dests(inlines, out);
        }
        Block::BlockQuote(inner) => {
            for child in inner {
                collect_block_dests(child, out);
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for child in &item.blocks {
                    collect_block_dests(child, out);
                }
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                collect_inline_dests(cell, out);
            }
            for row in &table.rows {
                for cell in row {
                    collect_inline_dests(cell, out);
                }
            }
        }
        Block::FootnoteDefinition { blocks, .. } => {
            for child in blocks {
                collect_block_dests(child, out);
            }
        }
        Block::DefinitionList(items) => {
            for item in items {
                for term in &item.terms {
                    collect_inline_dests(term, out);
                }
                for def in &item.definitions {
                    collect_inline_dests(def, out);
                }
            }
        }
        Block::CodeBlock { .. }
        | Block::ThematicBreak
        | Block::HtmlBlock(_)
        | Block::MathBlock(_) => {}
    }
}

fn collect_inline_dests(inlines: &[Inline], out: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => {
                out.push(dest.clone());
                collect_inline_dests(content, out);
            }
            Inline::Image { dest, .. } => out.push(dest.clone()),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => collect_inline_dests(children, out),
            _ => {}
        }
    }
}

/// Whitespace-normalized plain text of a block, for human summaries.
fn plain_block(block: &Block) -> String {
    let mut s = String::new();
    plain_block_into(block, &mut s);
    normalize_prose(&s)
}

fn plain_block_into(block: &Block, out: &mut String) {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            plain_inlines(inlines, out);
        }
        Block::CodeBlock { code, .. } => out.push_str(code),
        Block::BlockQuote(inner) => {
            for child in inner {
                plain_block_into(child, out);
                out.push(' ');
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for child in &item.blocks {
                    plain_block_into(child, out);
                    out.push(' ');
                }
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                plain_inlines(cell, out);
                out.push(' ');
            }
            for row in &table.rows {
                for cell in row {
                    plain_inlines(cell, out);
                    out.push(' ');
                }
            }
        }
        Block::ThematicBreak => {}
        Block::HtmlBlock(html) => out.push_str(html),
        Block::FootnoteDefinition { id, blocks } => {
            out.push_str(id);
            out.push(' ');
            for child in blocks {
                plain_block_into(child, out);
                out.push(' ');
            }
        }
        Block::MathBlock(math) => out.push_str(math),
        Block::DefinitionList(items) => {
            for item in items {
                for term in &item.terms {
                    plain_inlines(term, out);
                    out.push(' ');
                }
                for def in &item.definitions {
                    plain_inlines(def, out);
                    out.push(' ');
                }
            }
        }
    }
}

fn plain_inlines(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => plain_inlines(children, out),
            Inline::Code(code) => out.push_str(code),
            Inline::Link { content, .. } => plain_inlines(content, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Html(html) => out.push_str(html),
            Inline::FootnoteRef { id } => out.push_str(id),
            Inline::Math(math) | Inline::DisplayMath(math) => out.push_str(math),
        }
    }
}

/// First `MAX_CHARS` chars of `text`, plus `...` when truncated.
fn snippet(text: &str) -> String {
    const MAX_CHARS: usize = 40;
    let mut iter = text.chars();
    let mut s: String = (&mut iter).take(MAX_CHARS).collect();
    if iter.next().is_some() {
        s.push_str("...");
    }
    s
}

/// Alignment between two keyed block sequences: anchor pairs plus matched
/// flags. Common prefixes/suffixes are trimmed first; the middle is aligned
/// with the shared `lcs_diff` core over canonical keys, bounded to 16M DP
/// cells with a memory-free greedy in-order scan beyond that.
struct Alignment {
    anchors: Vec<(usize, usize)>,
    matched_old: Vec<bool>,
    matched_new: Vec<bool>,
}

fn align_blocks(old: &[KeyedBlock], new: &[KeyedBlock]) -> Alignment {
    let mut matched_old = vec![false; old.len()];
    let mut matched_new = vec![false; new.len()];
    let mut anchors: Vec<(usize, usize)> = Vec::new();

    let mut start = 0usize;
    while start < old.len() && start < new.len() && old[start].same_block(&new[start]) {
        matched_old[start] = true;
        matched_new[start] = true;
        anchors.push((start, start));
        start += 1;
    }
    let mut end_old = old.len();
    let mut end_new = new.len();
    let mut suffix_anchors: Vec<(usize, usize)> = Vec::new();
    while end_old > start && end_new > start && old[end_old - 1].same_block(&new[end_new - 1]) {
        end_old -= 1;
        end_new -= 1;
        matched_old[end_old] = true;
        matched_new[end_new] = true;
        suffix_anchors.push((end_old, end_new));
    }

    let om = end_old - start;
    let nm = end_new - start;
    const MAX_CELLS: usize = 16_000_000;
    if om > 0 && nm > 0 && om.saturating_mul(nm) <= MAX_CELLS {
        let old_mid: Vec<String> = old[start..end_old].iter().map(|k| k.key.clone()).collect();
        let new_mid: Vec<String> = new[start..end_new].iter().map(|k| k.key.clone()).collect();
        let raw = lcs_diff(&old_mid, &new_mid);
        let (mut oi, mut ni) = (start, start);
        for item in raw {
            match item {
                RawDiffItem::Unchanged(_) => {
                    matched_old[oi] = true;
                    matched_new[ni] = true;
                    anchors.push((oi, ni));
                    oi += 1;
                    ni += 1;
                }
                RawDiffItem::Deleted(_) => oi += 1,
                RawDiffItem::Inserted(_) => ni += 1,
            }
        }
    } else if om > 0 && nm > 0 {
        let mut cursor = start;
        for i in start..end_old {
            let mut j = cursor;
            while j < end_new {
                if old[i].same_block(&new[j]) {
                    matched_old[i] = true;
                    matched_new[j] = true;
                    anchors.push((i, j));
                    cursor = j + 1;
                    break;
                }
                j += 1;
            }
        }
    }

    anchors.extend(suffix_anchors);
    anchors.sort_unstable();
    Alignment {
        anchors,
        matched_old,
        matched_new,
    }
}

fn first_dest_diff(old: &[String], new: &[String]) -> (String, String) {
    let max = old.len().max(new.len());
    for i in 0..max {
        let o = old.get(i);
        let n = new.get(i);
        if o != n {
            return (
                o.cloned().unwrap_or_default(),
                n.cloned().unwrap_or_default(),
            );
        }
    }
    (String::new(), String::new())
}

/// Build the change for two paired blocks of the same kind whose keys differ.
fn changed_change(old: &KeyedBlock, new: &KeyedBlock, oi: usize, ni: usize) -> Change {
    let (kind, summary) = match old.tag {
        0 => {
            let summary = if old.plain == new.plain {
                format!(
                    "heading level changed {} -> {}: \"{}\"",
                    old.level,
                    new.level,
                    snippet(&new.plain)
                )
            } else if old.level != new.level {
                format!(
                    "heading changed (level {} -> {}): \"{}\" -> \"{}\"",
                    old.level,
                    new.level,
                    snippet(&old.plain),
                    snippet(&new.plain)
                )
            } else {
                format!(
                    "heading changed: \"{}\" -> \"{}\"",
                    snippet(&old.plain),
                    snippet(&new.plain)
                )
            };
            (ChangeKind::HeadingChanged { level: new.level }, summary)
        }
        1 => {
            if old.key_no_dest == new.key_no_dest && old.dests != new.dests {
                let (old_dest, new_dest) = first_dest_diff(&old.dests, &new.dests);
                (
                    ChangeKind::LinkTargetChanged {
                        old: old_dest.clone(),
                        new: new_dest.clone(),
                    },
                    format!("link target changed: {old_dest} -> {new_dest}"),
                )
            } else {
                (
                    ChangeKind::ParagraphEdited,
                    format!(
                        "paragraph edited: \"{}\" -> \"{}\"",
                        snippet(&old.plain),
                        snippet(&new.plain)
                    ),
                )
            }
        }
        2 => (
            ChangeKind::CodeBlockChanged {
                lang: new.lang.clone(),
            },
            format!(
                "code block changed ({})",
                new.lang.as_deref().unwrap_or("plain")
            ),
        ),
        3 => (
            ChangeKind::ParagraphEdited,
            format!(
                "block quote changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        4 => (
            ChangeKind::ListChanged,
            format!(
                "list changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        5 => (
            ChangeKind::TableChanged,
            format!(
                "table changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        7 => (
            ChangeKind::CodeBlockChanged { lang: None },
            String::from("HTML block changed"),
        ),
        8 => (
            ChangeKind::ParagraphEdited,
            format!(
                "footnote definition changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        9 => (
            ChangeKind::CodeBlockChanged { lang: None },
            String::from("math block changed"),
        ),
        10 => (
            ChangeKind::ListChanged,
            format!(
                "definition list changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        _ => (
            ChangeKind::ParagraphEdited,
            format!("{} changed", block_name(old.tag)),
        ),
    };
    Change {
        kind,
        summary,
        old_index: Some(oi),
        new_index: Some(ni),
    }
}

/// Build the change for an unpaired block (pure addition or removal).
fn add_remove_change(block: &KeyedBlock, index: usize, added: bool) -> Change {
    let kind = match (block.tag, added) {
        (0, true) => ChangeKind::HeadingAdded,
        (0, false) => ChangeKind::HeadingRemoved,
        (2, _) => ChangeKind::CodeBlockChanged {
            lang: block.lang.clone(),
        },
        (4, _) => ChangeKind::ListChanged,
        (5, _) => ChangeKind::TableChanged,
        (7, _) | (9, _) => ChangeKind::CodeBlockChanged { lang: None },
        (10, _) => ChangeKind::ListChanged,
        _ => ChangeKind::ParagraphEdited,
    };
    let mut summary = format!(
        "{} {}",
        block_name(block.tag),
        if added { "added" } else { "removed" }
    );
    if block.tag == 2
        && let Some(lang) = &block.lang
    {
        summary.push_str(&format!(" ({lang})"));
    }
    if !block.plain.is_empty() {
        summary.push_str(&format!(": \"{}\"", snippet(&block.plain)));
    }
    Change {
        kind,
        summary,
        old_index: if added { None } else { Some(index) },
        new_index: if added { Some(index) } else { None },
    }
}

/// Pair unmatched blocks inside one gap between two anchors: same-kind
/// in-order pairing becomes a typed change, leftovers become add/remove.
fn pair_gap(
    okeys: &[KeyedBlock],
    nkeys: &[KeyedBlock],
    matched_old: &[bool],
    matched_new: &[bool],
    olds: std::ops::Range<usize>,
    news: std::ops::Range<usize>,
    changes: &mut Vec<Change>,
) {
    let gap_olds: Vec<usize> = olds.filter(|&i| !matched_old[i]).collect();
    let gap_news: Vec<usize> = news.filter(|&i| !matched_new[i]).collect();
    let mut used_new = vec![false; gap_news.len()];
    for &oi in &gap_olds {
        let mut paired = None;
        for (k, &ni) in gap_news.iter().enumerate() {
            if !used_new[k] && nkeys[ni].tag == okeys[oi].tag {
                paired = Some(k);
                break;
            }
        }
        match paired {
            Some(k) => {
                used_new[k] = true;
                changes.push(changed_change(
                    &okeys[oi],
                    &nkeys[gap_news[k]],
                    oi,
                    gap_news[k],
                ));
            }
            None => changes.push(add_remove_change(&okeys[oi], oi, false)),
        }
    }
    for (k, &ni) in gap_news.iter().enumerate() {
        if !used_new[k] {
            changes.push(add_remove_change(&nkeys[ni], ni, true));
        }
    }
}

/// Compute the semantic diff between two parsed Markdown documents.
///
/// Block-level, heading-aware, reflow-insensitive, and fully deterministic:
/// identical inputs always produce a byte-identical report.
#[must_use]
pub fn diff_documents(old: &Document, new: &Document) -> DiffReport {
    if old == new {
        return DiffReport {
            changes: Vec::new(),
            identical: true,
        };
    }
    let okeys: Vec<KeyedBlock> = old.blocks.iter().map(KeyedBlock::from_block).collect();
    let nkeys: Vec<KeyedBlock> = new.blocks.iter().map(KeyedBlock::from_block).collect();
    let alignment = align_blocks(&okeys, &nkeys);
    let mut matched_old = alignment.matched_old;
    let mut matched_new = alignment.matched_new;

    let mut changes: Vec<Change> = Vec::new();

    // Global move pass: exact-key matches among otherwise unmatched blocks.
    const MAX_MOVE_CELLS: usize = 4_000_000;
    let rem_count = matched_old.iter().filter(|&&m| !m).count();
    let add_count = matched_new.iter().filter(|&&m| !m).count();
    if rem_count.saturating_mul(add_count) <= MAX_MOVE_CELLS {
        for (oi, okey) in okeys.iter().enumerate() {
            if matched_old[oi] {
                continue;
            }
            for (ni, nkey) in nkeys.iter().enumerate() {
                if !matched_new[ni] && okey.same_block(nkey) {
                    matched_old[oi] = true;
                    matched_new[ni] = true;
                    changes.push(Change {
                        kind: ChangeKind::BlockMoved,
                        summary: format!(
                            "{} moved from block {} to block {}: \"{}\"",
                            block_name(okey.tag),
                            oi,
                            ni,
                            snippet(&okey.plain)
                        ),
                        old_index: Some(oi),
                        new_index: Some(ni),
                    });
                    break;
                }
            }
        }
    }

    // Pair leftovers inside each gap between consecutive anchors.
    let mut prev = (0usize, 0usize);
    for &(ai, aj) in &alignment.anchors {
        pair_gap(
            &okeys,
            &nkeys,
            &matched_old,
            &matched_new,
            prev.0..ai,
            prev.1..aj,
            &mut changes,
        );
        prev = (ai + 1, aj + 1);
    }
    pair_gap(
        &okeys,
        &nkeys,
        &matched_old,
        &matched_new,
        prev.0..okeys.len(),
        prev.1..nkeys.len(),
        &mut changes,
    );

    // Deterministic document order; stable sort preserves emission order on
    // position ties.
    changes.sort_by_key(|c| c.old_index.or(c.new_index).unwrap_or(0));
    DiffReport {
        identical: changes.is_empty(),
        changes,
    }
}

fn change_kind_str(kind: &ChangeKind) -> &'static str {
    match kind {
        ChangeKind::HeadingAdded => "heading_added",
        ChangeKind::HeadingRemoved => "heading_removed",
        ChangeKind::HeadingChanged { .. } => "heading_changed",
        ChangeKind::ParagraphEdited => "paragraph_edited",
        ChangeKind::CodeBlockChanged { .. } => "code_block_changed",
        ChangeKind::TableChanged => "table_changed",
        ChangeKind::LinkTargetChanged { .. } => "link_target_changed",
        ChangeKind::BlockMoved => "block_moved",
        ChangeKind::ListChanged => "list_changed",
        ChangeKind::FrontmatterChanged => "frontmatter_changed",
    }
}

fn push_opt_index(out: &mut String, index: Option<usize>) {
    match index {
        Some(i) => out.push_str(&i.to_string()),
        None => out.push_str("null"),
    }
}

/// Render a [`DiffReport`] as compact JSON: schema `fmd-diff-changes-v1`,
/// snake_case keys, fixed key order, no timestamps. (The aggregate-stats
/// JSON surface on [`DocumentDiff::to_json`] keeps schema `fmd-diff-v1`.)
#[must_use]
pub fn report_json(report: &DiffReport) -> String {
    let mut out = String::with_capacity(64 + report.changes.len() * 96);
    out.push_str("{\"schema\":\"fmd-diff-changes-v1\",\"identical\":");
    out.push_str(if report.identical { "true" } else { "false" });
    out.push_str(",\"changes\":[");
    for (i, change) in report.changes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"kind\":\"");
        out.push_str(change_kind_str(&change.kind));
        out.push('"');
        match &change.kind {
            ChangeKind::HeadingChanged { level } => {
                out.push_str(",\"level\":");
                out.push_str(&level.to_string());
            }
            ChangeKind::CodeBlockChanged { lang } => {
                out.push_str(",\"lang\":");
                match lang {
                    Some(lang) => {
                        out.push('"');
                        out.push_str(&json_escape(lang));
                        out.push('"');
                    }
                    None => out.push_str("null"),
                }
            }
            ChangeKind::LinkTargetChanged { old, new } => {
                out.push_str(",\"old\":\"");
                out.push_str(&json_escape(old));
                out.push_str("\",\"new\":\"");
                out.push_str(&json_escape(new));
                out.push('"');
            }
            _ => {}
        }
        out.push_str(",\"summary\":\"");
        out.push_str(&json_escape(&change.summary));
        out.push_str("\",\"old_index\":");
        push_opt_index(&mut out, change.old_index);
        out.push_str(",\"new_index\":");
        push_opt_index(&mut out, change.new_index);
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// Render a [`DiffReport`] as a plain-text, human-readable change list.
#[must_use]
pub fn report_text(report: &DiffReport) -> String {
    if report.identical {
        return String::from("fmd diff: documents identical\n");
    }
    let n = report.changes.len();
    let mut out = format!("fmd diff: {n} change{}\n", if n == 1 { "" } else { "s" });
    for change in &report.changes {
        let marker = match (change.old_index, change.new_index) {
            (Some(_), Some(_)) => '~',
            (None, Some(_)) => '+',
            (Some(_), None) => '-',
            (None, None) => '?',
        };
        out.push(marker);
        out.push_str(" [");
        if let Some(i) = change.old_index {
            out.push_str(&i.to_string());
        }
        out.push_str("->");
        if let Some(i) = change.new_index {
            out.push_str(&i.to_string());
        }
        out.push_str("] ");
        out.push_str(&change.summary);
        out.push('\n');
    }
    out
}
