//! PDF renderer.
//!
//! v0 (this file): a self-contained, dependency-free PDF writer that lays out the
//! document and produces a valid, **tiny** PDF using the base-14 fonts (Helvetica
//! family + Courier) — no embedded font data, so the file is small and opens in
//! every viewer. Greedy line-wrapping with approximate metrics, automatic
//! pagination, headings/paragraphs/code/lists/blockquotes/rules.
//!
//! Roadmap (next increments, each clean-room + zero-dep):
//!   * embed **IBM Plex** (Sans/Serif/Mono) + **Computer Modern / Latin Modern**
//!     as `include_bytes!` subsets so the default output is beautiful and the
//!     binary stays self-contained (the metrics then come from the real `hmtx`);
//!   * **Knuth-Plass** optimal line breaking + kerning + ligatures + hyphenation
//!     (`layout.rs`, `text.rs`);
//!   * FlateDecode stream compression + font subsetting for a minimal file size.
//!
//! The renderer is pure computation (no `std::fs`, no deps) so it compiles for
//! `wasm32-unknown-unknown` and `--no-default-features --lib`.

use crate::PdfOptions;
use crate::ast::{Block, Document, Inline, List};
use crate::error::Result;

const PAGE_W: f32 = 612.0; // US Letter, points
const PAGE_H: f32 = 792.0;
const MARGIN: f32 = 72.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;

// Font ids used in the page Resources: F1 body, F2 bold, F3 italic, F4 mono.
const F_BODY: u8 = 1;
const F_BOLD: u8 = 2;
const F_ITALIC: u8 = 3;
const F_MONO: u8 = 4;

/// One laid-out, pre-wrapped line of text positioned by the paginator.
struct Line {
    x: f32,
    size: f32,
    font: u8,
    text: String,
    /// Extra vertical gap to leave after this line (block spacing).
    gap_after: f32,
    /// True to draw a thin horizontal rule at this position instead of text.
    rule: bool,
}

/// Render a document to PDF bytes.
///
/// # Errors
/// Infallible today, but returns [`Result`] so richer validation can be added
/// without changing the signature.
pub fn render(doc: &Document, opts: &PdfOptions) -> Result<Vec<u8>> {
    let lines = layout(&doc.blocks, opts);
    Ok(serialize(&lines, opts))
}

// ---- layout -----------------------------------------------------------------

fn layout(blocks: &[Block], opts: &PdfOptions) -> Vec<Line> {
    let mut out = Vec::new();
    layout_blocks(blocks, 0.0, &mut out, opts);
    out
}

fn layout_blocks(blocks: &[Block], indent: f32, out: &mut Vec<Line>, opts: &PdfOptions) {
    for block in blocks {
        layout_block(block, indent, out, opts);
    }
}

fn layout_block(block: &Block, indent: f32, out: &mut Vec<Line>, opts: &PdfOptions) {
    match block {
        Block::Heading { level, inlines } => {
            let size = match level {
                1 => 24.0,
                2 => 19.0,
                3 => 16.0,
                4 => 13.5,
                5 => 12.0,
                _ => 11.0,
            };
            push_wrapped(&inline_text(inlines), indent, size, F_BOLD, 6.0, out);
        }
        Block::Paragraph(inlines) => {
            push_wrapped(&inline_text(inlines), indent, 11.0, F_BODY, 7.0, out);
        }
        Block::CodeBlock { code, .. } => {
            for raw in code.lines() {
                // Code is not reflowed; long lines are hard-clipped at the margin.
                let clipped = clip_to_width(raw, CONTENT_W - indent - 8.0, 9.5, F_MONO);
                out.push(Line {
                    x: MARGIN + indent + 8.0,
                    size: 9.5,
                    font: F_MONO,
                    text: clipped,
                    gap_after: 1.5,
                    rule: false,
                });
            }
            gap(out, 6.0);
        }
        Block::BlockQuote(inner) => {
            layout_blocks(inner, indent + 18.0, out, opts);
            gap(out, 3.0);
        }
        Block::List(list) => layout_list(list, indent, out, opts),
        Block::Table(table) => {
            // v0: render each row as a tab-joined text line (real grid is a bead).
            let header = table
                .head
                .iter()
                .map(|c| inline_text(c))
                .collect::<Vec<_>>();
            push_wrapped(&header.join("   |   "), indent, 11.0, F_BOLD, 2.0, out);
            for row in &table.rows {
                let cells = row.iter().map(|c| inline_text(c)).collect::<Vec<_>>();
                push_wrapped(&cells.join("   |   "), indent, 11.0, F_BODY, 2.0, out);
            }
            gap(out, 6.0);
        }
        Block::ThematicBreak => {
            out.push(Line {
                x: MARGIN + indent,
                size: 6.0,
                font: F_BODY,
                text: String::new(),
                gap_after: 8.0,
                rule: true,
            });
        }
        Block::HtmlBlock(html) => {
            if !opts.allow_raw_html {
                push_wrapped(html, indent, 11.0, F_BODY, 7.0, out);
            }
        }
    }
}

fn layout_list(list: &List, indent: f32, out: &mut Vec<Line>, _opts: &PdfOptions) {
    for (i, item) in list.items.iter().enumerate() {
        let marker = match item.task {
            Some(true) => "[x]".to_string(),
            Some(false) => "[ ]".to_string(),
            None if list.ordered => format!("{}.", list.start + i as u64),
            None => "•".to_string(),
        };
        let body = item
            .blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph(inl) => inline_text(inl),
                other => block_plain(other),
            })
            .collect::<Vec<_>>()
            .join(" ");
        let text = format!("{marker}  {body}");
        push_wrapped(&text, indent + 16.0, 11.0, F_BODY, 2.0, out);
    }
    gap(out, 6.0);
}

fn push_wrapped(text: &str, indent: f32, size: f32, font: u8, gap_after: f32, out: &mut Vec<Line>) {
    let max = (CONTENT_W - indent).max(40.0);
    let wrapped = wrap(text, max, size, font);
    let n = wrapped.len();
    if n == 0 {
        gap(out, gap_after);
        return;
    }
    for (idx, l) in wrapped.into_iter().enumerate() {
        out.push(Line {
            x: MARGIN + indent,
            size,
            font,
            text: l,
            gap_after: if idx + 1 == n { gap_after } else { 0.0 },
            rule: false,
        });
    }
}

fn gap(out: &mut [Line], amount: f32) {
    if let Some(last) = out.last_mut() {
        last.gap_after += amount;
    }
}

// ---- pagination + serialization --------------------------------------------

fn serialize(lines: &[Line], opts: &PdfOptions) -> Vec<u8> {
    // Build per-page content streams.
    let mut pages: Vec<String> = Vec::new();
    let mut content = String::new();
    let mut y = PAGE_H - MARGIN;
    let top = PAGE_H - MARGIN;
    for line in lines {
        let leading = line.size * 1.32;
        if y - leading < MARGIN {
            pages.push(std::mem::take(&mut content));
            y = top;
        }
        y -= leading;
        if line.rule {
            let x2 = PAGE_W - MARGIN;
            content.push_str(&format!(
                "0.82 0.82 0.84 RG 0.7 w {x:.2} {yy:.2} m {x2:.2} {yy:.2} l S\n",
                x = line.x,
                yy = y + line.size * 0.5,
            ));
        } else if !line.text.is_empty() {
            content.push_str(&format!(
                "BT /F{f} {s:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm ({t}) Tj ET\n",
                f = line.font,
                s = line.size,
                x = line.x,
                y = y,
                t = pdf_escape(&line.text),
            ));
        }
        y -= line.gap_after;
    }
    pages.push(content);
    if pages.iter().all(String::is_empty) {
        pages = vec![String::new()];
    }

    build_pdf(&pages, opts)
}

fn build_pdf(pages: &[String], opts: &PdfOptions) -> Vec<u8> {
    // Object layout:
    //   1 Catalog, 2 Pages, 3..3+P-1 Page objects, then P content streams,
    //   then 4 font objects.
    let p = pages.len();
    let page_obj = |i: usize| 3 + i; // page object numbers
    let content_obj = |i: usize| 3 + p + i; // content stream object numbers
    let title = opts.title.clone().unwrap_or_default();
    let font_base = 3 + 2 * p; // first font object number
    let info_obj = font_base + 4;
    let total_objs = if title.is_empty() {
        font_base + 3
    } else {
        info_obj
    };

    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = vec![0; total_objs + 1]; // 1-indexed

    let emit = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, n: usize, body: &str| {
        offsets[n] = buf.len();
        buf.extend_from_slice(format!("{n} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    // 1: Catalog.
    emit(
        &mut buf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R >>",
    );
    // 2: Pages.
    let kids = (0..p)
        .map(|i| format!("{} 0 R", page_obj(i)))
        .collect::<Vec<_>>()
        .join(" ");
    emit(
        &mut buf,
        &mut offsets,
        2,
        &format!("<< /Type /Pages /Count {p} /Kids [ {kids} ] >>"),
    );
    // Page objects.
    let fonts = format!(
        "/F{F_BODY} {fb} 0 R /F{F_BOLD} {fo} 0 R /F{F_ITALIC} {fi} 0 R /F{F_MONO} {fm} 0 R",
        fb = font_base,
        fo = font_base + 1,
        fi = font_base + 2,
        fm = font_base + 3,
    );
    for i in 0..p {
        emit(
            &mut buf,
            &mut offsets,
            page_obj(i),
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.0} {PAGE_H:.0}] \
                 /Resources << /Font << {fonts} >> >> /Contents {c} 0 R >>",
                c = content_obj(i),
            ),
        );
    }
    // Content streams.
    for (i, page) in pages.iter().enumerate() {
        offsets[content_obj(i)] = buf.len();
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {len} >>\nstream\n{page}endstream\nendobj\n",
                n = content_obj(i),
                len = page.len(),
            )
            .as_bytes(),
        );
    }
    // Base-14 fonts (WinAnsi so Latin-1 accents render).
    let font_def = |name: &str| {
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{name} /Encoding /WinAnsiEncoding >>")
    };
    emit(&mut buf, &mut offsets, font_base, &font_def("Helvetica"));
    emit(
        &mut buf,
        &mut offsets,
        font_base + 1,
        &font_def("Helvetica-Bold"),
    );
    emit(
        &mut buf,
        &mut offsets,
        font_base + 2,
        &font_def("Helvetica-Oblique"),
    );
    emit(&mut buf, &mut offsets, font_base + 3, &font_def("Courier"));
    if !title.is_empty() {
        emit(
            &mut buf,
            &mut offsets,
            info_obj,
            &format!("<< /Title ({}) >>", pdf_escape(&title)),
        );
    }

    // xref.
    let xref_pos = buf.len();
    let size = total_objs + 1;
    buf.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets.iter().take(total_objs + 1).skip(1) {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    let info = if title.is_empty() {
        String::new()
    } else {
        format!(" /Info {info_obj} 0 R")
    };
    buf.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R{info} >>\nstartxref\n{xref_pos}\n%%EOF\n")
            .as_bytes(),
    );
    buf
}

// ---- text helpers -----------------------------------------------------------

fn pdf_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '(' => o.push_str("\\("),
            ')' => o.push_str("\\)"),
            '\\' => o.push_str("\\\\"),
            '•' => o.push_str("\\225"),
            '\r' => o.push_str("\\r"),
            '\n' => o.push(' '),
            c if (c as u32) < 256 => o.push(c),
            // Non-Latin-1 codepoints have no WinAnsi byte; approximate.
            _ => o.push('?'),
        }
    }
    o
}

/// Greedy word-wrap to a max width (in points) using approximate metrics.
fn wrap(text: &str, max_width: f32, size: f32, font: u8) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0_f32;
    for word in text.split_whitespace() {
        let ww = text_width(word, size, font);
        let sw = text_width(" ", size, font);
        if !cur.is_empty() && cur_w + sw + ww > max_width {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0.0;
        }
        if cur.is_empty() {
            cur.push_str(word);
            cur_w = ww;
        } else {
            cur.push(' ');
            cur.push_str(word);
            cur_w += sw + ww;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn clip_to_width(text: &str, max_width: f32, size: f32, font: u8) -> String {
    if text_width(text, size, font) <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut w = 0.0;
    for c in text.chars() {
        let cw = char_width(c, font) * size / 1000.0;
        if w + cw > max_width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out
}

fn text_width(s: &str, size: f32, font: u8) -> f32 {
    s.chars().map(|c| char_width(c, font)).sum::<f32>() * size / 1000.0
}

/// Approximate base-14 advance width in 1/1000 em. Courier is monospaced; the
/// Helvetica approximation is good enough for greedy wrapping (the viewer paints
/// with the font's real metrics regardless).
fn char_width(c: char, font: u8) -> f32 {
    if font == F_MONO {
        return 600.0;
    }
    match c {
        ' ' => 278.0,
        'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 240.0,
        'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '/' | '\\' | '"' | '-' => 320.0,
        'm' | 'w' | 'M' | 'W' => 880.0,
        'A'..='Z' => 690.0,
        '0'..='9' => 556.0,
        c if c.is_ascii_lowercase() => 540.0,
        _ => 560.0,
    }
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => s.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                s.push_str(&inline_text(c));
            }
            Inline::Link { content, .. } => s.push_str(&inline_text(content)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::Html(_) => {}
        }
    }
    s
}

fn block_plain(block: &Block) -> String {
    match block {
        Block::Paragraph(inl) | Block::Heading { inlines: inl, .. } => inline_text(inl),
        Block::CodeBlock { code, .. } => code.clone(),
        _ => String::new(),
    }
}
