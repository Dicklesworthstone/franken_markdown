//! Faithful before/after review using the ordinary document renderer.

use super::{DiffBlock, DiffInline, DocumentDiff, html_escape};
use crate::HtmlOptions;
use crate::ast::{Block, Document};
use crate::html::FragmentRenderer;

const STYLE: &str = r#"
body { margin: 0; padding: 24px; }
.diff-container { max-width: 1600px; margin: 0 auto; }
.diff-header { padding: 16px 20px; border: 1px solid #8c959f66; border-radius: 6px; margin-bottom: 20px; }
.diff-header h1 { font-size: 20px; margin: 0 0 10px; }
.diff-stats-bar { display: flex; flex-wrap: wrap; gap: 16px; }
.diff-badge-ins { border-bottom: 2px solid #2da44e; }
.diff-badge-del { border-bottom: 2px solid #cf222e; }
.diff-row, .diff-column-headings { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 24px; }
.diff-column-headings { margin: 16px 0; font-weight: 700; }
.fmd.diff-pane { width: auto; max-width: none; min-width: 0; margin: 0; padding: 8px 12px; overflow-wrap: anywhere; }
.diff-pane > :first-child { margin-top: 0; }
.diff-pane > :last-child { margin-bottom: 0; }
.diff-block-ins { background: rgba(46, 160, 67, .12); border-left: 3px solid #2da44e; }
.diff-block-del { background: rgba(248, 81, 73, .12); border-left: 3px solid #cf222e; }
.diff-block-mod { border-left: 3px solid #9a6700; }
ins.diff-inline { background: rgba(46, 160, 67, .22); text-decoration: none; }
del.diff-inline { background: rgba(248, 81, 73, .22); text-decoration: line-through; }
.diff-empty { border-left: 3px solid transparent; }
.diff-note-source { margin: 12px 0; padding: 8px 12px; border: 1px solid #8c959f66; }
.diff-note-label { font-weight: 700; }
@media (max-width: 760px) {
  body { padding: 12px; }
  .diff-row { grid-template-columns: minmax(0, 1fr); gap: 0; margin-bottom: 16px; }
  .diff-pane:not(.diff-empty)::before { content: attr(data-version); display: block; font-size: .8em; opacity: .75; margin-bottom: 6px; }
  .diff-column-headings, .diff-empty { display: none; }
}
@media print {
  body { padding: 0; }
  .diff-row { break-inside: avoid; }
  .diff-pane { print-color-adjust: exact; }
}
"#;

pub(super) fn render(diff: &DocumentDiff, opts: &HtmlOptions) -> String {
    let mut old_blocks = Vec::new();
    let mut new_blocks = Vec::new();
    for block in &diff.blocks {
        match block {
            DiffBlock::Unchanged(block) => {
                old_blocks.push(block.clone());
                new_blocks.push(block.clone());
            }
            DiffBlock::Inserted(block) => new_blocks.push(block.clone()),
            DiffBlock::Deleted(block) => old_blocks.push(block.clone()),
            DiffBlock::Modified { old, new, .. } => {
                old_blocks.push((**old).clone());
                new_blocks.push((**new).clone());
            }
        }
    }
    // Subset fonts for both complete revisions, including removed glyphs.
    let css = crate::html::stylesheet(
        &Document {
            blocks: old_blocks.iter().chain(&new_blocks).cloned().collect(),
        },
        opts,
    );
    let old_name = html_escape(&diff.old_name);
    let new_name = html_escape(&diff.new_name);
    let mut out = format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Diff: {old_name} vs {new_name}</title>\n<style>\n{css}\n{STYLE}</style>\n\
         </head>\n<body>\n<main class=\"diff-container\">\n\
         <header class=\"diff-header\"><h1>Comparing <code>{old_name}</code> &rarr; <code>{new_name}</code></h1>\n\
         <div class=\"diff-stats-bar\"><span class=\"diff-badge-ins\">+{} blocks (+{} words)</span>\
         <span class=\"diff-badge-del\">&minus;{} blocks (&minus;{} words)</span>\
         <span>Similarity: {:.1}%</span></div></header>\n\
         <div class=\"diff-column-headings\"><div>Before: {old_name}</div><div>After: {new_name}</div></div>\n",
        html_escape(opts.lang.as_deref().unwrap_or("en")),
        diff.stats.inserted_blocks + diff.stats.modified_blocks,
        diff.stats.words_inserted,
        diff.stats.deleted_blocks + diff.stats.modified_blocks,
        diff.stats.words_deleted,
        diff.stats.similarity_ratio * 100.0,
    );
    let mut before = FragmentRenderer::new(&old_blocks, opts, "diff-old-");
    let mut after = FragmentRenderer::new(&new_blocks, opts, "diff-new-");
    for block in &diff.blocks {
        // Definitions belong to their revision's notes, not empty body rows.
        if only_note_definitions(block) {
            continue;
        }
        out.push_str("<div class=\"diff-row\">\n");
        match block {
            DiffBlock::Unchanged(block) => {
                pane_start(&mut out, "old", "");
                before.block(block, &mut out);
                pane_end(&mut out);
                pane_start(&mut out, "new", "");
                after.block(block, &mut out);
                pane_end(&mut out);
            }
            DiffBlock::Deleted(block) => {
                pane_start(&mut out, "old", "diff-block-del");
                before.block(block, &mut out);
                pane_end(&mut out);
                empty_pane(&mut out, "new");
            }
            DiffBlock::Inserted(block) => {
                empty_pane(&mut out, "old");
                pane_start(&mut out, "new", "diff-block-ins");
                after.block(block, &mut out);
                pane_end(&mut out);
            }
            DiffBlock::Modified {
                old,
                new,
                inline_diff,
            } => {
                let inline = !inline_diff.is_empty()
                    && matches!(&**old, Block::Paragraph(_) | Block::Heading { .. })
                    && matches!(&**new, Block::Paragraph(_) | Block::Heading { .. });
                pane_start(
                    &mut out,
                    "old",
                    if inline {
                        "diff-block-mod"
                    } else {
                        "diff-block-del"
                    },
                );
                if inline {
                    render_inline_changes(&mut before, old, inline_diff, false, &mut out);
                } else {
                    before.block(old, &mut out);
                }
                pane_end(&mut out);
                pane_start(
                    &mut out,
                    "new",
                    if inline {
                        "diff-block-mod"
                    } else {
                        "diff-block-ins"
                    },
                );
                if inline {
                    render_inline_changes(&mut after, new, inline_diff, true, &mut out);
                } else {
                    after.block(new, &mut out);
                }
                pane_end(&mut out);
            }
        }
        out.push_str("</div>\n");
    }
    let old_defs = note_definitions(&old_blocks);
    let new_defs = note_definitions(&new_blocks);
    let mut old_notes = String::new();
    let mut new_notes = String::new();
    show_changed_unreferenced_notes(&old_defs, &new_defs, &mut before, &mut old_notes);
    show_changed_unreferenced_notes(&new_defs, &old_defs, &mut after, &mut new_notes);
    before.finish(&mut old_notes);
    after.finish(&mut new_notes);
    if !old_notes.is_empty() || !new_notes.is_empty() {
        out.push_str("<div class=\"diff-row diff-notes\">\n");
        pane_start(&mut out, "old", "");
        out.push_str(&old_notes);
        pane_end(&mut out);
        pane_start(&mut out, "new", "");
        out.push_str(&new_notes);
        pane_end(&mut out);
        out.push_str("</div>\n");
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

fn pane_start(out: &mut String, side: &str, class: &str) {
    let name = if side == "old" { "Before" } else { "After" };
    out.push_str(&format!(
        "<section class=\"fmd diff-pane diff-{side} {class}\" data-version=\"{name}\" aria-label=\"{name}\">\n"
    ));
}

fn pane_end(out: &mut String) {
    out.push_str("</section>\n");
}

fn empty_pane(out: &mut String, side: &str) {
    pane_start(out, side, "diff-empty");
    pane_end(out);
}

fn render_inline_changes(
    renderer: &mut FragmentRenderer<'_>,
    block: &Block,
    changes: &[DiffInline],
    after: bool,
    out: &mut String,
) {
    renderer.start_inline_block(block, out);
    for change in changes {
        match change {
            DiffInline::Unchanged(inline) => renderer.inline(inline, out),
            DiffInline::Inserted(inline) if after => {
                out.push_str("<ins class=\"diff-inline\">");
                renderer.inline(inline, out);
                out.push_str("</ins>");
            }
            DiffInline::Deleted(inline) if !after => {
                out.push_str("<del class=\"diff-inline\">");
                renderer.inline(inline, out);
                out.push_str("</del>");
            }
            _ => {}
        }
    }
    renderer.end_inline_block(block, out);
}

fn only_note_definitions(block: &DiffBlock) -> bool {
    match block {
        DiffBlock::Unchanged(b) | DiffBlock::Inserted(b) | DiffBlock::Deleted(b) => {
            matches!(b, Block::FootnoteDefinition { .. })
        }
        DiffBlock::Modified { old, new, .. } => {
            matches!(&**old, Block::FootnoteDefinition { .. })
                && matches!(&**new, Block::FootnoteDefinition { .. })
        }
    }
}

fn note_definitions(blocks: &[Block]) -> Vec<(&str, &[Block])> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            Block::FootnoteDefinition { id, blocks } => {
                out.push((id.as_str(), blocks.as_slice()));
                out.extend(note_definitions(blocks));
            }
            Block::BlockQuote(blocks) => out.extend(note_definitions(blocks)),
            Block::List(list) => {
                for item in &list.items {
                    out.extend(note_definitions(&item.blocks));
                }
            }
            _ => {}
        }
    }
    out
}

fn show_changed_unreferenced_notes(
    definitions: &[(&str, &[Block])],
    other: &[(&str, &[Block])],
    renderer: &mut FragmentRenderer<'_>,
    out: &mut String,
) {
    for &(id, blocks) in definitions {
        if renderer.has_note_reference(id)
            || other
                .iter()
                .any(|&(other_id, body)| id == other_id && blocks == body)
        {
            continue;
        }
        out.push_str(
            "<aside class=\"diff-note-source\"><div class=\"diff-note-label\">Footnote [^",
        );
        out.push_str(&html_escape(id));
        out.push_str("]</div>\n");
        for block in blocks {
            renderer.block(block, out);
        }
        out.push_str("</aside>\n");
    }
}
