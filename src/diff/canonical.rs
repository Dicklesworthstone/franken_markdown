//! Structural keys for semantic document comparison.
//!
//! Only ordinary prose and soft breaks are whitespace-normalized. Literal
//! payloads retain their bytes, and length-prefixed fields prevent authored
//! text from impersonating structural markers or adjacent fields.

use std::fmt::Write;

use crate::ast::{Block, Inline};

pub(super) fn canonical_block(block: &Block, with_dests: bool) -> String {
    let mut out = String::new();
    push_block(block, with_dests, &mut out);
    out
}

fn field(out: &mut String, value: &str) {
    let _ = write!(out, "{}:", value.len());
    out.push_str(value);
}

fn optional_field(out: &mut String, value: Option<&str>) {
    match value {
        Some(value) => {
            out.push('+');
            field(out, value);
        }
        None => out.push('-'),
    }
}

fn number(out: &mut String, value: u64) {
    let _ = write!(out, "{value};");
}

fn push_block(block: &Block, with_dests: bool, out: &mut String) {
    // Every nested block carries its own kind, not only the top-level key.
    out.push(char::from(b'A' + super::block_tag(block)));
    out.push('(');
    match block {
        Block::Heading { level, inlines } => {
            number(out, u64::from(*level));
            push_inlines(inlines, with_dests, true, out);
        }
        Block::Paragraph(inlines) => push_inlines(inlines, with_dests, true, out),
        Block::CodeBlock { lang, code } => {
            optional_field(out, lang.as_deref());
            field(out, code);
        }
        Block::BlockQuote(blocks) => push_children(blocks, with_dests, out),
        Block::List(list) => {
            out.push(if list.ordered { '1' } else { '0' });
            number(out, list.start);
            out.push(if list.tight { '1' } else { '0' });
            for item in &list.items {
                out.push('(');
                out.push(match item.task {
                    Some(true) => '1',
                    Some(false) => '0',
                    None => '-',
                });
                push_children(&item.blocks, with_dests, out);
                out.push(')');
            }
        }
        Block::Table(table) => {
            out.push('(');
            for align in &table.align {
                number(out, *align as u64);
            }
            out.push(')');
            push_cells(&table.head, with_dests, out);
            for row in &table.rows {
                push_cells(row, with_dests, out);
            }
        }
        Block::ThematicBreak | Block::PageBreak => {}
        Block::HtmlBlock(html) => field(out, html),
        Block::FootnoteDefinition { id, blocks } => {
            field(out, id);
            push_children(blocks, with_dests, out);
        }
        Block::MathBlock(math) => field(out, math),
        Block::DefinitionList(items) => {
            for item in items {
                out.push('(');
                push_cells(&item.terms, with_dests, out);
                push_cells(&item.definitions, with_dests, out);
                out.push(')');
            }
        }
    }
    out.push(')');
}

fn push_children(blocks: &[Block], with_dests: bool, out: &mut String) {
    for block in blocks {
        push_block(block, with_dests, out);
    }
}

fn push_cells(cells: &[Vec<Inline>], with_dests: bool, out: &mut String) {
    out.push('(');
    for cell in cells {
        out.push('(');
        push_inlines(cell, with_dests, true, out);
        out.push(')');
    }
    out.push(')');
}

fn push_prose(text: &str, normalized: &mut String, whitespace: &mut bool) {
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !*whitespace {
                normalized.push(' ');
            }
            *whitespace = true;
        } else {
            normalized.push(ch);
            *whitespace = false;
        }
    }
}

fn push_inlines(inlines: &[Inline], with_dests: bool, trim_edges: bool, out: &mut String) {
    let mut i = 0;
    while i < inlines.len() {
        if matches!(inlines[i], Inline::Text(_) | Inline::SoftBreak) {
            // Parser segmentation and prose wrapping do not change meaning.
            // Coalesce neighboring text/soft-break nodes before framing them.
            let mut text = String::new();
            let mut whitespace = trim_edges && i == 0;
            while let Some(inline) = inlines.get(i) {
                match inline {
                    Inline::Text(value) => push_prose(value, &mut text, &mut whitespace),
                    Inline::SoftBreak => push_prose(" ", &mut text, &mut whitespace),
                    _ => break,
                }
                i += 1;
            }
            if trim_edges && i == inlines.len() && text.ends_with(' ') {
                text.pop();
            }
            if !text.is_empty() {
                out.push('t');
                field(out, &text);
            }
            continue;
        }

        match &inlines[i] {
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => {
                out.push(match &inlines[i] {
                    Inline::Emphasis(_) => 'e',
                    Inline::Strong(_) => 's',
                    _ => 'd',
                });
                out.push('(');
                push_inlines(children, with_dests, false, out);
                out.push(')');
            }
            Inline::Code(code) => {
                out.push('c');
                field(out, code);
            }
            Inline::Link {
                dest,
                title,
                content,
            } => {
                out.push_str("l(");
                push_inlines(content, with_dests, false, out);
                out.push(')');
                if with_dests {
                    field(out, dest);
                    optional_field(out, title.as_deref());
                }
            }
            Inline::Image { dest, title, alt } => {
                out.push('i');
                field(out, alt);
                if with_dests {
                    field(out, dest);
                    optional_field(out, title.as_deref());
                }
            }
            Inline::HardBreak => out.push('b'),
            Inline::Html(html) => {
                out.push('h');
                field(out, html);
            }
            Inline::FootnoteRef { id } => {
                out.push('f');
                field(out, id);
            }
            Inline::Math(math) => {
                out.push('m');
                field(out, math);
            }
            Inline::DisplayMath(math) => {
                out.push('M');
                field(out, math);
            }
            Inline::Text(_) | Inline::SoftBreak => {}
        }
        i += 1;
    }
}
