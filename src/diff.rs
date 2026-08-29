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
