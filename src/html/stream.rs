//! Bounded HTML emission for ordinary blocks, including one enormous block.
//!
//! Specialized renderers keep their existing string contract, but only their
//! individual result is buffered. Document state is shared across every write.

use std::io::{self, Write};

use super::{HtmlOptions, RenderState, Span, financial_tables};
use crate::ast::{Align, Block, Document, Inline, List, Table};
use crate::table_format::FootingRule;

const BUFFER_BYTES: usize = 64 * 1024;
const MAX_RETAINED_SCRATCH_BYTES: usize = 256 * 1024;

struct Output<'a, W: Write + ?Sized> {
    writer: &'a mut W,
    buffer: Vec<u8>,
    scratch: String,
    written: usize,
}

impl<'a, W: Write + ?Sized> Output<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            buffer: Vec::with_capacity(BUFFER_BYTES),
            scratch: String::new(),
            written: 0,
        }
    }

    fn text(&mut self, text: &str) -> io::Result<()> {
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            // Borrow complete clean runs directly instead of copying them
            // through the buffer. Each write is still bounded, even for a
            // megabyte-long token or a single unescaped prose text node.
            if self.buffer.is_empty() && bytes.len() >= BUFFER_BYTES {
                self.writer.write_all(&bytes[..BUFFER_BYTES])?;
                self.written += BUFFER_BYTES;
                bytes = &bytes[BUFFER_BYTES..];
            } else {
                let count = bytes.len().min(BUFFER_BYTES - self.buffer.len());
                self.buffer.extend_from_slice(&bytes[..count]);
                bytes = &bytes[count..];
                if self.buffer.len() == BUFFER_BYTES {
                    self.flush_buffer()?;
                }
            }
        }
        Ok(())
    }

    fn flush_buffer(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            self.writer.write_all(&self.buffer)?;
            self.written += self.buffer.len();
            self.buffer.clear();
        }
        Ok(())
    }

    fn buffered<T>(&mut self, render: impl FnOnce(&mut String) -> T) -> io::Result<T> {
        let mut scratch = std::mem::take(&mut self.scratch);
        let value = render(&mut scratch);
        let result = self.text(&scratch);
        scratch.clear();
        if scratch.capacity() <= MAX_RETAINED_SCRATCH_BYTES {
            self.scratch = scratch;
        }
        result.map(|()| value)
    }

    fn number(&mut self, value: u64) -> io::Result<()> {
        self.buffered(|out| super::push_u64(out, value))
    }

    fn escaped(&mut self, text: &str, attribute: bool) -> io::Result<()> {
        let bytes = text.as_bytes();
        let mut start = 0;
        loop {
            let next = if attribute {
                crate::scanner::find_html_escape(&bytes[start..])
            } else {
                crate::scanner::find_html_text_escape(&bytes[start..])
            };
            let Some(offset) = next else {
                return self.text(&text[start..]);
            };
            let position = start + offset;
            self.text(&text[start..position])?;
            self.text(match bytes[position] {
                b'&' => "&amp;",
                b'<' => "&lt;",
                b'>' => "&gt;",
                b'\"' => "&quot;",
                _ => "",
            })?;
            start = position + 1;
        }
    }

    fn prose(&mut self, text: &str) -> io::Result<()> {
        if !text.contains(':') {
            return self.escaped(text, false);
        }
        let bytes = text.as_bytes();
        let mut written = 0;
        let mut index = 0;
        while let Some(offset) = text[index..].find(':') {
            let open = index + offset;
            let name_start = open + 1;
            let name_len = bytes[name_start..]
                .iter()
                .take(41)
                .take_while(|&&byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'+' | b'-')
                })
                .count();
            let close = name_start + name_len;
            if name_len > 0
                && name_len <= 40
                && bytes.get(close) == Some(&b':')
                && let Some(emoji) = crate::emoji_shortcodes::lookup(&text[name_start..close])
            {
                self.escaped(&text[written..open], false)?;
                self.text(emoji)?;
                written = close + 1;
                index = close + 1;
            } else {
                index = name_start;
            }
        }
        self.escaped(&text[written..], false)
    }

    fn token(&mut self, code: &str, span: Span) -> io::Result<()> {
        let text = code.get(span.start..span.end).unwrap_or("");
        if let Some(class) = span.kind.css_class() {
            self.text("<span class=\"")?;
            self.text(class)?;
            self.text("\">")?;
            self.escaped(text, false)?;
            self.text("</span>")
        } else {
            self.escaped(text, false)
        }
    }
}

pub(super) fn render<W: Write + ?Sized>(
    doc: &Document,
    opts: &HtmlOptions,
    writer: &mut W,
) -> io::Result<usize> {
    let mut out = Output::new(writer);
    let mut state = out.buffered(|html| super::start_document(doc, opts, html, 0))?;
    out.flush_buffer()?;
    for child in &doc.blocks {
        block(child, &mut out, opts, &mut state)?;
        // Preserve prompt delivery of small top-level blocks. File callers
        // already use BufWriter; large blocks flush themselves as they grow.
        out.flush_buffer()?;
    }
    out.buffered(|html| state.raw_html.close_to(0, html))?;
    footnotes(&mut out, opts, &mut state)?;
    out.text("</main>\n</body>\n</html>\n")?;
    out.flush_buffer()?;
    // The caller owns flushing and publication. No Drop implementation retries
    // a failed write or emits a trailing fragment after an error.
    Ok(out.written)
}

fn blocks<W: Write + ?Sized>(
    leading: Option<&[Inline]>,
    children: &[Block],
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    let saved_base = state.raw_html_base;
    state.raw_html_base = state.raw_html.depth();
    if let Some(leading) = leading.filter(|inlines| !inlines.is_empty()) {
        paragraph(leading, out, opts, state)?;
    }
    for child in children {
        block(child, out, opts, state)?;
    }
    out.buffered(|html| state.raw_html.close_to(state.raw_html_base, html))?;
    state.raw_html_base = saved_base;
    Ok(())
}

fn block<W: Write + ?Sized>(
    child: &Block,
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    match child {
        Block::Paragraph(content) => paragraph(content, out, opts, state),
        Block::Heading {
            level,
            inlines: content,
        } => {
            out.buffered(|html| {
                state.before_heading(opts, html);
                html.push_str("<h");
                super::push_u64(html, u64::from(*level));
                html.push_str(" id=\"");
                state.push_heading_id_from_inlines(content, html);
                html.push_str("\">");
            })?;
            inlines(content, out, opts, state)?;
            out.text("</h")?;
            out.number(u64::from(*level))?;
            out.text(">\n")
        }
        Block::CodeBlock { lang, code } => code_block(lang.as_deref(), code, out, state),
        Block::BlockQuote(children) => {
            if let Some(parts) = crate::ast::alert_parts(children) {
                out.text("<aside class=\"callout callout-")?;
                out.text(parts.tag)?;
                out.text("\">\n<p class=\"callout-title\">")?;
                out.text(parts.label)?;
                out.text("</p>\n")?;
                blocks(Some(parts.leading), parts.rest, out, opts, state)?;
                out.text("</aside>\n")
            } else {
                out.text("<blockquote>\n")?;
                blocks(None, children, out, opts, state)?;
                out.text("</blockquote>\n")
            }
        }
        Block::List(value) => list(value, out, opts, state),
        Block::DefinitionList(items) => {
            out.text("<dl>\n")?;
            for item in items {
                for term in &item.terms {
                    out.text("<dt>")?;
                    inlines(term, out, opts, state)?;
                    out.text("</dt>\n")?;
                }
                for definition in &item.definitions {
                    out.text("<dd>")?;
                    inlines(definition, out, opts, state)?;
                    out.text("</dd>\n")?;
                }
            }
            out.text("</dl>\n")
        }
        Block::Table(value) => table(value, out, opts, state),
        Block::HtmlBlock(html) if opts.allow_raw_html => {
            out.text(html)?;
            out.text("\n")
        }
        // These retain their specialized renderer, not an enclosing paragraph,
        // list, table or footnote. Scratch is released after a large result.
        _ => out.buffered(|html| super::render_block(child, html, opts, state)),
    }
}

fn paragraph<W: Write + ?Sized>(
    content: &[Inline],
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    if let Some(depth) = super::toc_marker_depth(content) {
        return out.buffered(|html| {
            super::push_toc_nav(&state.toc_entries, depth.or(opts.toc_depth), html);
        });
    }
    out.text("<p>")?;
    inlines(content, out, opts, state)?;
    out.text("</p>\n")
}

fn inlines<W: Write + ?Sized>(
    content: &[Inline],
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    let saved_base = state.raw_html_base;
    state.raw_html_base = state.raw_html.depth();
    for inline in content {
        match inline {
            Inline::Text(text) => out.prose(text)?,
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                let (open, close) = match inline {
                    Inline::Emphasis(_) => ("<em>", "</em>"),
                    Inline::Strong(_) => ("<strong>", "</strong>"),
                    _ => ("<del>", "</del>"),
                };
                out.text(open)?;
                inlines(inner, out, opts, state)?;
                out.text(close)?;
            }
            Inline::Code(code) => {
                out.text("<code>")?;
                out.escaped(code, false)?;
                out.text("</code>")?;
            }
            Inline::Link {
                dest,
                title,
                content,
            } => {
                let linked = out.buffered(|html| {
                    super::render_link_open(dest, title.as_deref(), html, state)
                })?;
                inlines(content, out, opts, state)?;
                if linked {
                    out.text("</a>")?;
                }
            }
            Inline::SoftBreak => out.text("\n")?,
            Inline::HardBreak => out.text("<br>\n")?,
            Inline::Html(html) if opts.allow_raw_html => out.text(html)?,
            _ => out.buffered(|html| {
                // Do not close the sanitizer after an individual inline: its
                // safe tags may surround subsequent text in this same run.
                super::render_inlines_body(std::slice::from_ref(inline), html, opts, state);
            })?,
        }
    }
    out.buffered(|html| state.raw_html.close_to(state.raw_html_base, html))?;
    state.raw_html_base = saved_base;
    Ok(())
}

fn code_block<W: Write + ?Sized>(
    lang: Option<&str>,
    code: &str,
    out: &mut Output<'_, W>,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    let language = lang.unwrap_or("");
    if crate::diagrams::is_diagram_code(code, language)
        && let Some(svg) = crate::diagrams::render_diagram_svg(code, language)
    {
        out.text("<div class=\"fmd-diagram-wrapper\">\n")?;
        out.text(&svg)?;
        return out.text("</div>\n");
    }
    out.text("<pre><code")?;
    if let Some(lang) = lang {
        out.text(" class=\"language-")?;
        out.escaped(lang, true)?;
        out.text("\"")?;
    }
    out.text(">")?;
    if lang.is_none() {
        out.escaped(code, false)?;
    } else if super::cacheable_highlight_source(language, code) {
        out.buffered(|html| state.highlight_code(language, code, html))?;
    } else if let Some(result) =
        crate::highlight::highlight_streamed(language, code, |span| out.token(code, span))
    {
        result?;
    } else if super::highlight_supported_into(language, code, &mut state.highlight_spans) {
        let result = state
            .highlight_spans
            .iter()
            .try_for_each(|span| out.token(code, *span));
        state.release_large_highlight_spans();
        result?;
    } else {
        out.escaped(code, false)?;
    }
    out.text("</code></pre>\n")
}

fn list<W: Write + ?Sized>(
    list: &List,
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    let tag = if list.ordered { "ol" } else { "ul" };
    out.text("<")?;
    out.text(tag)?;
    if list.ordered && list.start != 1 {
        out.text(" start=\"")?;
        out.number(list.start)?;
        out.text("\"")?;
    }
    out.text(">\n")?;
    for item in &list.items {
        if let Some(checked) = item.task {
            out.text("<li class=\"task\"><input type=\"checkbox\" disabled")?;
            if checked {
                out.text(" checked")?;
            }
            out.text("> ")?;
        } else if item.blocks.is_empty()
            || (list.tight && matches!(item.blocks.first(), Some(Block::Paragraph(_))))
        {
            out.text("<li>")?;
        } else {
            out.text("<li>\n")?;
        }
        for (index, child) in item.blocks.iter().enumerate() {
            if let Block::Paragraph(content) = child
                && list.tight
                && super::toc_marker_depth(content).is_none()
            {
                inlines(content, out, opts, state)?;
                if index + 1 < item.blocks.len() {
                    out.text("\n")?;
                }
            } else {
                block(child, out, opts, state)?;
            }
        }
        out.text("</li>\n")?;
    }
    out.text("</")?;
    out.text(tag)?;
    out.text(">\n")
}

fn table<W: Write + ?Sized>(
    table: &Table,
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    out.text("<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n")?;
    if let Some(rows) = crate::table_format::financial_rows(table) {
        let widths = financial_tables::suffix_widths(table, &rows, opts);
        out.text("<table class=\"fmd-financial-table\">\n<thead>\n<tr>")?;
        financial_cells(
            &table.head,
            &table.align,
            &[],
            &widths,
            true,
            out,
            opts,
            state,
        )?;
        out.text("</tr>\n</thead>\n<tbody>\n")?;
        for row in rows {
            out.text("<tr>")?;
            financial_cells(
                row.cells,
                &table.align,
                &row.rules,
                &widths,
                false,
                out,
                opts,
                state,
            )?;
            out.text("</tr>\n")?;
        }
    } else {
        out.text("<table>\n<thead>\n<tr>")?;
        table_cells(&table.head, &table.align, true, out, opts, state)?;
        out.text("</tr>\n</thead>\n<tbody>\n")?;
        for row in &table.rows {
            out.text("<tr>")?;
            table_cells(row, &table.align, false, out, opts, state)?;
            out.text("</tr>\n")?;
        }
    }
    out.text("</tbody>\n</table>\n</div>\n")
}

fn table_cells<W: Write + ?Sized>(
    cells: &[Vec<Inline>],
    align: &[Align],
    header: bool,
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    for (column, cell) in cells.iter().enumerate() {
        out.text(if header { "<th" } else { "<td" })?;
        out.text(super::align_attr(
            align.get(column).copied().unwrap_or(Align::None),
        ))?;
        out.text(">")?;
        inlines(cell, out, opts, state)?;
        out.text(if header { "</th>" } else { "</td>" })?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn financial_cells<W: Write + ?Sized>(
    cells: &[Vec<Inline>],
    align: &[Align],
    rules: &[FootingRule],
    widths: &[Option<f32>],
    header: bool,
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    for (column, cell) in cells.iter().enumerate() {
        out.buffered(|html| {
            financial_tables::render_cells(
                std::slice::from_ref(cell),
                align.get(column..).unwrap_or(&[]),
                rules.get(column..).unwrap_or(&[]),
                widths.get(column..).unwrap_or(&[]),
                header,
                html,
                opts,
                state,
            );
        })?;
    }
    Ok(())
}

fn footnotes<W: Write + ?Sized>(
    out: &mut Output<'_, W>,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) -> io::Result<()> {
    if state.footnote_order.is_empty() {
        return Ok(());
    }
    state.automatic_toc = false;
    out.text("<section class=\"footnotes\">\n<ol>\n")?;
    let mut index = 0;
    while index < state.footnote_order.len() {
        let id = state.footnote_order[index].clone();
        if let Some(children) = state.footnote_defs.get(id.as_str()).copied() {
            out.text("<li id=\"")?;
            out.escaped(state.id_prefix, true)?;
            out.text("fn-")?;
            out.escaped(&id, true)?;
            out.text("\">")?;
            blocks(None, children, out, opts, state)?;
            out.text(" <a class=\"footnote-backref\" href=\"#")?;
            out.escaped(state.id_prefix, true)?;
            out.text("fnref-")?;
            out.number((index + 1) as u64)?;
            out.text("\">↩</a>\n</li>\n")?;
        }
        index += 1;
        out.flush_buffer()?;
    }
    out.text("</ol>\n</section>\n")
}
