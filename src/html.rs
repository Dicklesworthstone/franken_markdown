//! All-in-one HTML emitter: turns the AST into a single self-contained `.html`
//! document with the default theme stylesheet inlined. The default styling is
//! tuned to look like a high-quality Markdown preview (Cursor/GitHub-grade):
//! readable measure and leading, gorgeous tables with subtle striping, elegant
//! blockquotes, and code blocks ready for syntax highlighting.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use crate::ast::{Align, Block, Document, Inline, List};
use crate::fonts::{self, FontStyle};
use crate::text::Font;
use crate::theme::{DarkModePolicy, Theme, ThemeColors};
use crate::{FontAssets, HtmlOptions};

/// Render a document to a complete HTML5 document string.
#[must_use]
pub fn render(doc: &Document, opts: &HtmlOptions) -> String {
    let title = opts
        .title
        .clone()
        .or_else(|| first_heading_text(doc))
        .unwrap_or_else(|| "Document".to_string());
    let css = opts
        .custom_css
        .as_deref()
        .map_or_else(|| default_css(doc, opts), sanitize_custom_css);

    let mut body = String::with_capacity(initial_body_capacity(doc.blocks.len()));
    let mut state = RenderState::default();
    render_blocks(&doc.blocks, &mut body, opts, &mut state);

    let escaped_title = escape_text(&title);
    let mut html = String::with_capacity(186 + escaped_title.len() + css.len() + body.len());
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>");
    html.push_str(&escaped_title);
    html.push_str("</title>\n<style>\n");
    html.push_str(&css);
    html.push_str("</style>\n</head>\n<body>\n<main class=\"fmd\">\n");
    html.push_str(&body);
    html.push_str("</main>\n</body>\n</html>\n");
    html
}

/// Make caller-supplied CSS safe to inline in a raw-text `<style>` element.
///
/// The HTML tokenizer ends a `<style>` element at the first case-insensitive
/// `</style` regardless of CSS syntax, so a stylesheet containing
/// `</style><script>…` would break out into live script. `</` inside a `<style>`
/// can never be meaningful CSS except inside a string, where CSS treats `\/` as a
/// plain `/`; inserting that backslash keeps the stylesheet's meaning while the
/// byte sequence is no longer an HTML end tag.
fn sanitize_custom_css(css: &str) -> String {
    let lower = css.to_ascii_lowercase();
    if !lower.contains("</style") {
        return css.to_string();
    }
    let lower_bytes = lower.as_bytes();
    let mut out = String::with_capacity(css.len() + 8);
    let mut i = 0;
    while i < css.len() {
        if lower_bytes.get(i..i + 7) == Some(b"</style") {
            // Insert a CSS-harmless backslash after `<`, keeping the original
            // casing of `/style` intact.
            out.push('<');
            out.push('\\');
            out.push_str(&css[i + 1..i + 7]);
            i += 7;
        } else {
            let ch = css[i..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn first_heading_text(doc: &Document) -> Option<String> {
    doc.blocks.iter().find_map(|b| match b {
        Block::Heading { inlines, .. } => Some(inlines_to_plain(inlines)),
        _ => None,
    })
}

#[derive(Default)]
struct RenderState {
    /// Keys are every emitted heading id. Values are the next suffix to try
    /// when that same id text later appears as a heading's base slug.
    heading_id_suffixes: BTreeMap<String, usize>,
}

impl RenderState {
    fn push_heading_id_from_inlines(&mut self, inlines: &[Inline], out: &mut String) {
        let mut base = slug_inlines(inlines);
        if base.is_empty() {
            base.push_str("section");
        }

        let mut suffix = self
            .heading_id_suffixes
            .get(base.as_str())
            .copied()
            .unwrap_or(1);
        loop {
            if suffix == 1 {
                suffix += 1;
                if !self.heading_id_suffixes.contains_key(base.as_str()) {
                    out.push_str(&base);
                    self.heading_id_suffixes.insert(base, suffix);
                    return;
                }
                continue;
            }

            let candidate = format!("{base}-{suffix}");
            suffix += 1;
            if let Entry::Vacant(entry) = self.heading_id_suffixes.entry(candidate) {
                out.push_str(entry.key());
                entry.insert(1);
                self.heading_id_suffixes.insert(base, suffix);
                return;
            }
        }
    }
}

fn render_blocks(blocks: &[Block], out: &mut String, opts: &HtmlOptions, state: &mut RenderState) {
    for block in blocks {
        render_block(block, out, opts, state);
    }
}

fn initial_body_capacity(blocks: usize) -> usize {
    blocks.saturating_mul(4096).min(4 * 1024 * 1024)
}

fn render_block(block: &Block, out: &mut String, opts: &HtmlOptions, state: &mut RenderState) {
    match block {
        Block::Heading { level, inlines } => {
            out.push_str("<h");
            push_u64(out, u64::from(*level));
            out.push_str(" id=\"");
            state.push_heading_id_from_inlines(inlines, out);
            out.push_str("\">");
            render_inlines(inlines, out, opts);
            out.push_str("</h");
            push_u64(out, u64::from(*level));
            out.push_str(">\n");
        }
        Block::Paragraph(inlines) => {
            out.push_str("<p>");
            render_inlines(inlines, out, opts);
            out.push_str("</p>\n");
        }
        Block::CodeBlock { lang, code } => {
            out.push_str("<pre><code");
            if let Some(l) = lang.as_deref() {
                out.push_str(" class=\"language-");
                out.push_str(&escape_attr(l));
                out.push('"');
            }
            out.push('>');
            // Clean-room syntax highlighting when we have a lexer for the
            // language; otherwise the code is rendered as escaped plain text.
            match lang.as_deref() {
                Some(l) if crate::highlight::is_supported(l) => {
                    highlight_code(l, code, out);
                }
                _ => push_escaped_text(code, out),
            }
            out.push_str("</code></pre>\n");
        }
        Block::BlockQuote(inner) => {
            out.push_str("<blockquote>\n");
            render_blocks(inner, out, opts, state);
            out.push_str("</blockquote>\n");
        }
        Block::List(list) => render_list(list, out, opts, state),
        Block::Table(table) => render_table(table, out, opts),
        Block::ThematicBreak => out.push_str("<hr>\n"),
        Block::HtmlBlock(html) => {
            if opts.allow_raw_html {
                out.push_str(html);
                out.push('\n');
            } else {
                out.push_str("<p>");
                push_escaped_text(html, out);
                out.push_str("</p>\n");
            }
        }
    }
}

fn render_list(list: &List, out: &mut String, opts: &HtmlOptions, state: &mut RenderState) {
    let tag = if list.ordered { "ol" } else { "ul" };
    if list.ordered && list.start != 1 {
        out.push('<');
        out.push_str(tag);
        out.push_str(" start=\"");
        push_u64(out, list.start);
        out.push_str("\">\n");
    } else {
        out.push('<');
        out.push_str(tag);
        out.push_str(">\n");
    }
    for item in &list.items {
        match item.task {
            Some(checked) => {
                out.push_str("<li class=\"task\"><input type=\"checkbox\" disabled");
                if checked {
                    out.push_str(" checked");
                }
                out.push_str("> ");
            }
            None => out.push_str("<li>"),
        }
        // Tight lists strip the <p> wrapper from every direct-child paragraph of
        // an item (CommonMark tight-list rendering) — including items that also
        // hold a nested list or other block. Loose lists and non-paragraph blocks
        // render normally. A stripped paragraph followed by another block is
        // separated by a newline so the following block opens on its own line.
        for (idx, block) in item.blocks.iter().enumerate() {
            match block {
                Block::Paragraph(inlines) if list.tight => {
                    render_inlines(inlines, out, opts);
                    if idx + 1 < item.blocks.len() {
                        out.push('\n');
                    }
                }
                _ => render_block(block, out, opts, state),
            }
        }
        out.push_str("</li>\n");
    }
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

fn render_table(table: &crate::ast::Table, out: &mut String, opts: &HtmlOptions) {
    let align_attrs: Vec<&'static str> = table.align.iter().copied().map(align_attr).collect();

    out.push_str(
        "<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n",
    );
    out.push_str("<table>\n<thead>\n<tr>");
    render_table_cells(&table.head, &align_attrs, "<th", "</th>", out, opts);
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in &table.rows {
        out.push_str("<tr>");
        render_table_cells(row, &align_attrs, "<td", "</td>", out, opts);
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</div>\n");
}

fn render_table_cells(
    cells: &[Vec<Inline>],
    align_attrs: &[&'static str],
    open: &str,
    close: &str,
    out: &mut String,
    opts: &HtmlOptions,
) {
    let aligned_len = cells.len().min(align_attrs.len());
    let (aligned_cells, unaligned_cells) = cells.split_at(aligned_len);
    for (cell, align) in aligned_cells.iter().zip(&align_attrs[..aligned_len]) {
        out.push_str(open);
        out.push_str(align);
        out.push('>');
        render_inlines(cell, out, opts);
        out.push_str(close);
    }
    for cell in unaligned_cells {
        out.push_str(open);
        out.push('>');
        render_inlines(cell, out, opts);
        out.push_str(close);
    }
}

fn align_attr(a: Align) -> &'static str {
    match a {
        Align::Left => " style=\"text-align:left\"",
        Align::Center => " style=\"text-align:center\"",
        Align::Right => " style=\"text-align:right\"",
        Align::None => "",
    }
}

fn render_inlines(inlines: &[Inline], out: &mut String, opts: &HtmlOptions) {
    for inl in inlines {
        match inl {
            Inline::Text(t) => push_escaped_text(t, out),
            Inline::Emphasis(c) => wrap(out, "em", c, opts),
            Inline::Strong(c) => wrap(out, "strong", c, opts),
            Inline::Strikethrough(c) => wrap(out, "del", c, opts),
            Inline::Code(t) => {
                out.push_str("<code>");
                push_escaped_text(t, out);
                out.push_str("</code>");
            }
            Inline::Link {
                dest,
                title,
                content,
            } => {
                if let Some(href) = safe_url(dest, UrlContext::Link) {
                    out.push_str("<a href=\"");
                    out.push_str(&escape_attr(href));
                    out.push('"');
                    if let Some(title) = title.as_deref() {
                        out.push_str(" title=\"");
                        out.push_str(&escape_attr(title));
                        out.push('"');
                    }
                    out.push('>');
                    render_inlines(content, out, opts);
                    out.push_str("</a>");
                } else {
                    render_inlines(content, out, opts);
                }
            }
            Inline::Image { dest, title, alt } => {
                if let Some(src) = safe_url(dest, UrlContext::Image) {
                    out.push_str("<img src=\"");
                    out.push_str(&escape_attr(src));
                    out.push_str("\" alt=\"");
                    out.push_str(&escape_attr(alt));
                    out.push('"');
                    if let Some(title) = title.as_deref() {
                        out.push_str(" title=\"");
                        out.push_str(&escape_attr(title));
                        out.push('"');
                    }
                    out.push('>');
                } else {
                    push_escaped_text(alt, out);
                }
            }
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br>\n"),
            Inline::Html(h) => {
                if opts.allow_raw_html {
                    out.push_str(h);
                } else {
                    push_escaped_text(h, out);
                }
            }
        }
    }
}

fn wrap(out: &mut String, tag: &str, content: &[Inline], opts: &HtmlOptions) {
    out.push('<');
    out.push_str(tag);
    out.push('>');
    render_inlines(content, out, opts);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

fn push_u64(out: &mut String, value: u64) {
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

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => s.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                s.push_str(&inlines_to_plain(c));
            }
            Inline::Link { content, .. } => s.push_str(&inlines_to_plain(content)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::Html(html) => s.push_str(html),
        }
    }
    s
}

#[cfg(test)]
fn slug(text: &str) -> String {
    let mut s = String::new();
    let mut pending_dash = false;
    for c in text.chars() {
        push_slug_char(&mut s, &mut pending_dash, c);
    }
    s
}

fn slug_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    let mut pending_dash = false;
    push_slug_inlines(inlines, &mut s, &mut pending_dash);
    s
}

fn push_slug_inlines(inlines: &[Inline], out: &mut String, pending_dash: &mut bool) {
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) | Inline::Html(t) => {
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

/// Emit highlighted code: one `<span class="tok-...">` per classified token,
/// always HTML-escaped; plain tokens are escaped text with no wrapping span.
fn highlight_code(lang: &str, code: &str, out: &mut String) {
    for span in crate::highlight::highlight(lang, code) {
        let text = code.get(span.start..span.end).unwrap_or("");
        match span.kind.css_class() {
            Some(cls) => {
                out.push_str("<span class=\"");
                out.push_str(cls);
                out.push_str("\">");
                push_escaped_text(text, out);
                out.push_str("</span>");
            }
            None => push_escaped_text(text, out),
        }
    }
}

fn push_escaped_text(s: &str, out: &mut String) {
    // Text nodes only need `&`, `<`, and `>` escaped. Writing into the caller's
    // buffer avoids a temporary allocation for strings that contain escapes.
    let bytes = s.as_bytes();
    let mut start = 0;
    while let Some(rel) = find_text_escape(&bytes[start..]) {
        let pos = start + rel;
        out.push_str(&s[start..pos]);
        match bytes[pos] {
            b'&' => out.push_str("&amp;"),
            b'<' => out.push_str("&lt;"),
            b'>' => out.push_str("&gt;"),
            _ => {}
        }
        start = pos + 1;
    }
    out.push_str(&s[start..]);
}

fn escape_text(s: &str) -> Cow<'_, str> {
    // Text nodes only need `&`, `<`, and `>` escaped. Quotes stay literal in
    // text content, so quote-only strings can borrow the input unchanged.
    let bytes = s.as_bytes();
    if find_text_escape(bytes).is_none() {
        return Cow::Borrowed(s);
    }
    let mut o = String::with_capacity(s.len());
    push_escaped_text(s, &mut o);
    Cow::Owned(o)
}

fn escape_attr(s: &str) -> Cow<'_, str> {
    // The attribute escape set (`& < > "`) is exactly the scanner's
    // `find_html_escape` set, so bulk-copy clean runs and escape each special.
    // All specials are ASCII, so byte indexing is UTF-8-safe.
    let bytes = s.as_bytes();
    if crate::scanner::find_html_escape(bytes).is_none() {
        return Cow::Borrowed(s);
    }
    let mut o = String::with_capacity(s.len());
    let mut start = 0;
    while let Some(rel) = crate::scanner::find_html_escape(&bytes[start..]) {
        let pos = start + rel;
        o.push_str(&s[start..pos]);
        match bytes[pos] {
            b'&' => o.push_str("&amp;"),
            b'<' => o.push_str("&lt;"),
            b'>' => o.push_str("&gt;"),
            b'"' => o.push_str("&quot;"),
            _ => {}
        }
        start = pos + 1;
    }
    o.push_str(&s[start..]);
    Cow::Owned(o)
}

fn find_text_escape(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|&byte| matches!(byte, b'&' | b'<' | b'>'))
}

#[derive(Clone, Copy)]
enum UrlContext {
    Link,
    Image,
}

enum UrlScheme {
    None,
    Scheme(String),
    Suspicious,
}

fn safe_url(url: &str, context: UrlContext) -> Option<&str> {
    let trimmed = url.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    if trimmed.is_empty() && matches!(context, UrlContext::Image) {
        return None;
    }
    match url_scheme(trimmed) {
        UrlScheme::None => Some(trimmed),
        UrlScheme::Scheme(scheme) if allowed_url_scheme(&scheme, context) => Some(trimmed),
        UrlScheme::Scheme(_) | UrlScheme::Suspicious => None,
    }
}

fn url_scheme(url: &str) -> UrlScheme {
    let mut scheme = String::new();
    let mut skipped_gap = false;
    for ch in url.chars() {
        if matches!(ch, '/' | '?' | '#') {
            return UrlScheme::None;
        }
        if ch == ':' {
            if skipped_gap || !valid_url_scheme(&scheme) {
                return UrlScheme::Suspicious;
            }
            return UrlScheme::Scheme(scheme.to_ascii_lowercase());
        }
        if ch.is_ascii_whitespace() || ch.is_control() {
            skipped_gap = true;
            continue;
        }
        scheme.push(ch);
    }
    UrlScheme::None
}

fn valid_url_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn allowed_url_scheme(scheme: &str, context: UrlContext) -> bool {
    match context {
        UrlContext::Link => matches!(scheme, "http" | "https" | "mailto" | "tel"),
        UrlContext::Image => matches!(scheme, "http" | "https"),
    }
}

/// The default, dependency-free, gorgeous stylesheet.
fn default_css(doc: &Document, opts: &HtmlOptions) -> String {
    let theme = &opts.theme;
    let embedded = embedded_font_css(doc, theme, &opts.font_assets);
    let body_font = if embedded.has_body {
        format!("\"FMD Body\", {}", theme.body_font_stack())
    } else {
        theme.body_font_stack().to_string()
    };
    let mono_font = if embedded.has_mono {
        format!("\"FMD Mono\", {}", theme.mono_font_stack())
    } else {
        theme.mono_font_stack().to_string()
    };
    let colors = &theme.colors;
    let spacing = &theme.spacing;

    let emit_dark = matches!(theme.dark_mode, DarkModePolicy::Auto);
    let token_dark = match theme.dark_mode {
        DarkModePolicy::Auto => TOKEN_DARK_CSS,
        DarkModePolicy::Disabled => "",
    };

    let color_vars_capacity = color_vars_capacity(colors);
    let dark_capacity = if emit_dark {
        dark_mode_css_capacity(&theme.dark_colors)
    } else {
        0
    };
    let line_height = css_num(spacing.line_height);
    let pad_y = css_num(spacing.table_cell_padding_y_em);
    let pad_x = css_num(spacing.table_cell_padding_x_em);
    let mut css = String::with_capacity(
        embedded.css.len()
            + color_vars_capacity
            + body_font.len()
            + mono_font.len()
            + BASE_CSS.len()
            + TOKEN_CSS.len()
            + dark_capacity
            + token_dark.len()
            + 256,
    );
    css.push_str(&embedded.css);
    push_color_vars(&mut css, colors);
    css.push_str("\n:root { --fmd-base: ");
    push_u64(&mut css, u64::from(spacing.base_px));
    css.push_str("px; --fmd-measure: ");
    push_u64(&mut css, u64::from(spacing.max_width_px));
    css.push_str("px; --fmd-line-height: ");
    css.push_str(&line_height);
    css.push_str("; --fmd-radius: ");
    push_u64(&mut css, u64::from(spacing.radius_px));
    css.push_str("px; --fmd-table-pad-y: ");
    css.push_str(&pad_y);
    css.push_str("em; --fmd-table-pad-x: ");
    css.push_str(&pad_x);
    css.push_str("em; --fmd-font-body: ");
    css.push_str(&body_font);
    css.push_str("; --fmd-font-mono: ");
    css.push_str(&mono_font);
    css.push_str("; }\n");
    css.push_str(BASE_CSS);
    css.push('\n');
    css.push_str(TOKEN_CSS);
    css.push('\n');
    if emit_dark {
        push_dark_mode_css(&mut css, &theme.dark_colors);
    }
    css.push_str(token_dark);
    css
}

fn color_vars_capacity(colors: &ThemeColors) -> usize {
    256 + css_token_capacity(&colors.fg)
        + css_token_capacity(&colors.fg_muted)
        + css_token_capacity(&colors.bg)
        + css_token_capacity(&colors.bg_subtle)
        + css_token_capacity(&colors.border)
        + css_token_capacity(&colors.border_muted)
        + css_token_capacity(&colors.code_bg)
        + css_token_capacity(&colors.stripe)
        + css_token_capacity(&colors.quote_fg)
        + css_token_capacity(&colors.quote_bar)
        + css_token_capacity(&colors.accent)
}

fn dark_mode_css_capacity(colors: &ThemeColors) -> usize {
    64 + color_vars_capacity(colors)
}

fn css_token_capacity(s: &str) -> usize {
    s.len().max(CSS_TOKEN_FALLBACK.len())
}

fn push_color_vars(css: &mut String, colors: &ThemeColors) {
    css.push_str(":root {\n");
    push_color_var(css, "--fmd-fg", &colors.fg);
    push_color_var(css, "--fmd-fg-muted", &colors.fg_muted);
    push_color_var(css, "--fmd-bg", &colors.bg);
    push_color_var(css, "--fmd-bg-subtle", &colors.bg_subtle);
    push_color_var(css, "--fmd-border", &colors.border);
    push_color_var(css, "--fmd-border-muted", &colors.border_muted);
    push_color_var(css, "--fmd-code-bg", &colors.code_bg);
    push_color_var(css, "--fmd-stripe", &colors.stripe);
    push_color_var(css, "--fmd-quote-fg", &colors.quote_fg);
    push_color_var(css, "--fmd-quote-bar", &colors.quote_bar);
    push_color_var(css, "--fmd-accent", &colors.accent);
    css.push('}');
}

fn push_color_var(css: &mut String, name: &str, value: &str) {
    css.push_str("  ");
    css.push_str(name);
    css.push_str(": ");
    push_css_token(css, value);
    css.push_str(";\n");
}

fn push_dark_mode_css(css: &mut String, colors: &ThemeColors) {
    css.push_str("\n@media (prefers-color-scheme: dark) {\n  ");
    push_color_vars(css, colors);
    css.push_str("\n}\n");
}

const CSS_TOKEN_FALLBACK: &str = "initial";

#[cfg(test)]
fn css_token(s: &str) -> String {
    let mut out = String::with_capacity(css_token_capacity(s));
    push_css_token(&mut out, s);
    out
}

fn push_css_token(out: &mut String, s: &str) {
    let start_len = out.len();
    for c in s.chars() {
        if c.is_ascii_alphanumeric()
            || matches!(
                c,
                '#' | '-' | '_' | ',' | '.' | '%' | '(' | ')' | ' ' | '/' | '"'
            )
        {
            out.push(c);
        }
    }
    if out[start_len..].trim().is_empty() {
        out.truncate(start_len);
        out.push_str(CSS_TOKEN_FALLBACK);
    }
}

fn css_num(value: f32) -> String {
    // A non-finite value (NaN/inf) would serialize to `NaN`/`inf`, which is an
    // invalid CSS token. A library caller can put such a value in a directly
    // constructed theme, so fold it to `0` (matching the PDF writer's
    // non-finite handling) rather than emitting a broken declaration.
    if !value.is_finite() {
        return "0".to_string();
    }
    let mut s = format!("{value:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() { "0".to_string() } else { s }
}

#[derive(Default)]
struct FontUsage {
    body_regular: FontCharSet,
    body_bold: FontCharSet,
    body_italic: FontCharSet,
    body_bold_italic: FontCharSet,
    mono: FontCharSet,
}

struct FontCharSet {
    ascii: [bool; 128],
    has_ascii: bool,
    non_ascii: BTreeSet<char>,
}

impl Default for FontCharSet {
    fn default() -> Self {
        Self {
            ascii: [false; 128],
            has_ascii: false,
            non_ascii: BTreeSet::new(),
        }
    }
}

impl FontCharSet {
    fn is_empty(&self) -> bool {
        !self.has_ascii && self.non_ascii.is_empty()
    }

    fn insert(&mut self, ch: char) {
        if ch.is_ascii() {
            self.ascii[ch as usize] = true;
            self.has_ascii = true;
        } else {
            self.non_ascii.insert(ch);
        }
    }

    fn extend_text(&mut self, text: &str) {
        if text.is_ascii() {
            if !text.is_empty() {
                self.has_ascii = true;
            }
            for &byte in text.as_bytes() {
                self.ascii[usize::from(byte)] = true;
            }
        } else {
            for ch in text.chars() {
                self.insert(ch);
            }
        }
    }

    fn to_chars(&self) -> Vec<char> {
        let mut chars =
            Vec::with_capacity(self.non_ascii.len() + if self.has_ascii { 128 } else { 0 });
        if self.has_ascii {
            for (idx, used) in self.ascii.iter().enumerate() {
                if *used {
                    chars.push(char::from(idx as u8));
                }
            }
        }
        chars.extend(self.non_ascii.iter().copied());
        chars
    }
}

#[derive(Clone, Copy, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
}

impl InlineStyle {
    const fn bold(self) -> Self {
        Self {
            bold: true,
            italic: self.italic,
        }
    }

    const fn italic(self) -> Self {
        Self {
            bold: self.bold,
            italic: true,
        }
    }
}

struct EmbeddedFontCss {
    css: String,
    has_body: bool,
    has_mono: bool,
}

fn embedded_font_css(doc: &Document, theme: &Theme, font_assets: &FontAssets) -> EmbeddedFontCss {
    let mut usage = collect_font_usage(doc);
    usage.add_stability_seed();

    let mut css = String::new();
    let mut has_body = false;
    let mut has_mono = false;

    push_font_face(
        &mut css,
        "FMD Body",
        "normal",
        "400",
        body_font_bytes(font_assets, theme.font, FontStyle::Regular),
        &usage.body_regular,
    );
    has_body |= !usage.body_regular.is_empty();

    push_font_face(
        &mut css,
        "FMD Body",
        "normal",
        "700",
        body_font_bytes(font_assets, theme.font, FontStyle::Bold),
        &usage.body_bold,
    );
    has_body |= !usage.body_bold.is_empty();

    push_font_face(
        &mut css,
        "FMD Body",
        "italic",
        "400",
        body_font_bytes(font_assets, theme.font, FontStyle::Italic),
        &usage.body_italic,
    );
    has_body |= !usage.body_italic.is_empty();

    push_font_face(
        &mut css,
        "FMD Body",
        "italic",
        "700",
        body_font_bytes(font_assets, theme.font, FontStyle::BoldItalic),
        &usage.body_bold_italic,
    );
    has_body |= !usage.body_bold_italic.is_empty();

    push_font_face(
        &mut css,
        "FMD Mono",
        "normal",
        "400",
        mono_font_bytes(font_assets, FontStyle::Regular),
        &usage.mono,
    );
    has_mono |= !usage.mono.is_empty();

    EmbeddedFontCss {
        css,
        has_body,
        has_mono,
    }
}

fn body_font_bytes(font_assets: &FontAssets, family: crate::FontFamily, style: FontStyle) -> &[u8] {
    match style {
        FontStyle::Regular => font_assets
            .body_regular
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
        FontStyle::Bold => font_assets
            .body_bold
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
        FontStyle::Italic => font_assets
            .body_italic
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
        FontStyle::BoldItalic => font_assets
            .body_bold_italic
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
    }
}

fn mono_font_bytes(font_assets: &FontAssets, style: FontStyle) -> &[u8] {
    font_assets
        .mono_regular
        .as_deref()
        .unwrap_or_else(|| fonts::mono_bytes(style))
}

impl FontUsage {
    fn body_slot(&mut self, style: InlineStyle) -> &mut FontCharSet {
        match (style.bold, style.italic) {
            (false, false) => &mut self.body_regular,
            (true, false) => &mut self.body_bold,
            (false, true) => &mut self.body_italic,
            (true, true) => &mut self.body_bold_italic,
        }
    }

    fn add_body_text(&mut self, text: &str, style: InlineStyle) {
        self.body_slot(style).extend_text(text);
    }

    fn add_mono_text(&mut self, text: &str) {
        self.mono.extend_text(text);
    }

    fn add_soft_break(&mut self, style: InlineStyle) {
        self.body_slot(style).insert(' ');
    }

    fn add_stability_seed(&mut self) {
        add_seed_if_used(&mut self.body_regular);
        add_seed_if_used(&mut self.body_bold);
        add_seed_if_used(&mut self.body_italic);
        add_seed_if_used(&mut self.body_bold_italic);
        add_seed_if_used(&mut self.mono);
    }
}

fn add_seed_if_used(chars: &mut FontCharSet) {
    if chars.is_empty() {
        return;
    }
    chars.extend_text(HTML_FONT_SEED);
}

fn collect_font_usage(doc: &Document) -> FontUsage {
    let mut usage = FontUsage::default();
    collect_blocks_font_usage(&doc.blocks, &mut usage);
    usage
}

fn collect_blocks_font_usage(blocks: &[Block], usage: &mut FontUsage) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } => {
                collect_inlines_font_usage(inlines, usage, InlineStyle::default().bold());
            }
            Block::Paragraph(inlines) => {
                collect_inlines_font_usage(inlines, usage, InlineStyle::default());
            }
            Block::CodeBlock { code, .. } => usage.add_mono_text(code),
            Block::BlockQuote(inner) => collect_blocks_font_usage(inner, usage),
            Block::List(list) => {
                for item in &list.items {
                    collect_blocks_font_usage(&item.blocks, usage);
                }
            }
            Block::Table(table) => {
                for cell in &table.head {
                    collect_inlines_font_usage(cell, usage, InlineStyle::default().bold());
                }
                for row in &table.rows {
                    for cell in row {
                        collect_inlines_font_usage(cell, usage, InlineStyle::default());
                    }
                }
            }
            Block::ThematicBreak => {}
            Block::HtmlBlock(html) => usage.add_body_text(html, InlineStyle::default()),
        }
    }
}

fn collect_inlines_font_usage(inlines: &[Inline], usage: &mut FontUsage, style: InlineStyle) {
    for inl in inlines {
        match inl {
            Inline::Text(text) => usage.add_body_text(text, style),
            Inline::Emphasis(children) => {
                collect_inlines_font_usage(children, usage, style.italic())
            }
            Inline::Strong(children) => collect_inlines_font_usage(children, usage, style.bold()),
            Inline::Strikethrough(children)
            | Inline::Link {
                content: children, ..
            } => {
                collect_inlines_font_usage(children, usage, style);
            }
            Inline::Code(text) => usage.add_mono_text(text),
            Inline::Image { alt, .. } => usage.add_body_text(alt, style),
            Inline::SoftBreak | Inline::HardBreak => usage.add_soft_break(style),
            Inline::Html(html) => usage.add_body_text(html, style),
        }
    }
}

fn push_font_face(
    css: &mut String,
    family: &str,
    style: &str,
    weight: &str,
    font_bytes: &[u8],
    chars: &FontCharSet,
) {
    if chars.is_empty() {
        return;
    }

    let keep = chars.to_chars();
    let subset = Font::parse(font_bytes.to_vec())
        .ok()
        .and_then(|font| font.subset(&keep))
        .unwrap_or_else(|| font_bytes.to_vec());
    let encoded = base64_encode(&subset);
    css.push_str("@font-face {\n");
    css.push_str("  font-family: \"");
    css.push_str(family);
    css.push_str("\";\n  font-style: ");
    css.push_str(style);
    css.push_str(";\n  font-weight: ");
    css.push_str(weight);
    css.push_str(";\n  font-display: swap;\n  src: url(\"data:font/ttf;base64,");
    css.push_str(&encoded);
    css.push_str("\") format(\"truetype\");\n}\n");
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0usize;
    let full_len = bytes.len() / 3 * 3;
    while i < full_len {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        let b2 = bytes[i + 2];

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);

        i += 3;
    }
    match bytes.len() - full_len {
        0 => {}
        1 => {
            let b0 = bytes[full_len];
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[((b0 & 0b0000_0011) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
        _ => {
            let b0 = bytes[full_len];
            let b1 = bytes[full_len + 1];
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
            out.push(TABLE[((b1 & 0b0000_1111) << 2) as usize] as char);
            out.push('=');
        }
    }
    out
}

const HTML_FONT_SEED: &str = " \t\n0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz.,;:!?()[]{}<>/\\'\"+-_=*#%&@|`~^•–—“”‘’";

const TOKEN_CSS: &str = r#"
.tok-kw { color: #cf222e; }
.tok-ty { color: #953800; }
.tok-fn { color: #6639ba; }
.tok-st { color: #0a3069; }
.tok-nu { color: #0550ae; }
.tok-cm { color: #6e7781; font-style: italic; }
.tok-op { color: #0550ae; }
.tok-pn { color: inherit; }
"#;

const TOKEN_DARK_CSS: &str = r#"
@media (prefers-color-scheme: dark) {
  .tok-kw { color: #ff7b72; }
  .tok-ty { color: #ffa657; }
  .tok-fn { color: #d2a8ff; }
  .tok-st { color: #a5d6ff; }
  .tok-nu { color: #79c0ff; }
  .tok-cm { color: #8b949e; }
  .tok-op { color: #79c0ff; }
}
"#;

const BASE_CSS: &str = r#"
*, *::before, *::after { box-sizing: border-box; }
html { -webkit-text-size-adjust: 100%; }
body {
  margin: 0;
  background: var(--fmd-bg);
  color: var(--fmd-fg);
  font-family: var(--fmd-font-body);
  font-size: var(--fmd-base);
  line-height: var(--fmd-line-height);
  font-feature-settings: "kern" 1, "liga" 1, "calt" 1;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  text-wrap: pretty;
  hyphens: auto;
  overflow-wrap: break-word;
}
main.fmd {
  max-width: var(--fmd-measure);
  margin: 0 auto;
  padding: 3rem 1.25rem 6rem;
}
main.fmd > :first-child { margin-top: 0; }

h1, h2, h3, h4, h5, h6 {
  margin: 2.2em 0 0.7em;
  line-height: 1.25;
  font-weight: 650;
  letter-spacing: 0;
}
h1 { font-size: 2.05em; padding-bottom: 0.3em; border-bottom: 1px solid var(--fmd-border-muted); }
h2 { font-size: 1.55em; padding-bottom: 0.3em; border-bottom: 1px solid var(--fmd-border-muted); }
h3 { font-size: 1.27em; }
h4 { font-size: 1.05em; }
h5 { font-size: 0.95em; color: var(--fmd-fg-muted); }
h6 { font-size: 0.88em; color: var(--fmd-fg-muted); }

p { margin: 0 0 1.1em; }
a { color: var(--fmd-accent); text-decoration: none; }
a:hover { text-decoration: underline; text-underline-offset: 2px; }
a:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--fmd-accent), transparent 35%);
  outline-offset: 2px;
  border-radius: 3px;
}

ul, ol { margin: 0 0 1.1em; padding-left: 1.7em; }
li { margin: 0.25em 0; }
li > ul, li > ol { margin: 0.25em 0; }
li.task { list-style: none; margin-left: -1.5em; }
li.task input { margin-right: 0.5em; transform: translateY(1px); }

blockquote {
  margin: 0 0 1.2em;
  padding: 0.25em 1.1em;
  color: var(--fmd-quote-fg);
  border-left: 0.28em solid var(--fmd-quote-bar);
  background: linear-gradient(90deg, var(--fmd-bg-subtle), transparent 60%);
  border-radius: 0 var(--fmd-radius) var(--fmd-radius) 0;
}
blockquote > :last-child { margin-bottom: 0; }

code {
  font-family: var(--fmd-font-mono);
  font-size: 0.88em;
  background: var(--fmd-code-bg);
  padding: 0.18em 0.4em;
  border-radius: calc(var(--fmd-radius) * 0.75);
}
pre {
  margin: 0 0 1.3em;
  padding: 1em 1.15em;
  background: var(--fmd-code-bg);
  border: 1px solid var(--fmd-border-muted);
  border-radius: calc(var(--fmd-radius) + 2px);
  overflow: auto;
  break-inside: avoid;
  line-height: 1.55;
}
pre code { background: none; padding: 0; font-size: 0.86em; }

hr { height: 1px; border: 0; background: var(--fmd-border); margin: 2.4em 0; }

img { max-width: 100%; border-radius: var(--fmd-radius); }

.table-wrap {
  margin: 0 0 1.4em;
  overflow-x: auto;
  border: 1px solid var(--fmd-border);
  border-radius: calc(var(--fmd-radius) + 2px);
  -webkit-overflow-scrolling: touch;
}
.table-wrap:focus-within {
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--fmd-accent), transparent 82%);
}
.table-wrap:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--fmd-accent), transparent 35%);
  outline-offset: 3px;
}
table {
  border-collapse: collapse;
  width: 100%;
  min-width: 100%;
  margin: 0;
  font-size: 0.95em;
}
thead th {
  background: var(--fmd-bg-subtle);
  font-weight: 650;
  text-align: left;
}
th, td {
  padding: var(--fmd-table-pad-y) var(--fmd-table-pad-x);
  border-bottom: 1px solid var(--fmd-border-muted);
  vertical-align: top;
}
tbody tr:nth-child(even) { background: var(--fmd-stripe); }
tbody tr:last-child td { border-bottom: 0; }

del { color: var(--fmd-fg-muted); }
strong { font-weight: 680; }

@media print {
  body {
    background: #fff;
    color: #000;
    font-size: 11pt;
    line-height: 1.5;
  }
  main.fmd {
    max-width: none;
    padding: 0;
  }
  a {
    color: #000;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  blockquote, pre, table, img {
    break-inside: avoid;
  }
  h1, h2, h3, h4, h5, h6 {
    break-after: avoid;
    color: #000;
  }
  .table-wrap {
    overflow: visible;
    border-color: #999;
  }
}
"#;

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::BTreeSet;

    use crate::HtmlOptions;
    use crate::ast::{Block, Document, Inline};

    use super::{
        FontCharSet, base64_encode, css_num, css_token, escape_attr, escape_text,
        initial_body_capacity, inlines_to_plain, push_u64, sanitize_custom_css, slug, slug_inlines,
    };

    #[test]
    fn css_num_folds_non_finite_to_zero() {
        // NaN/inf would otherwise serialize to invalid CSS tokens.
        assert_eq!(css_num(f32::NAN), "0");
        assert_eq!(css_num(f32::INFINITY), "0");
        assert_eq!(css_num(f32::NEG_INFINITY), "0");
        // Finite values are unaffected.
        assert_eq!(css_num(1.5), "1.5");
        assert_eq!(css_num(0.0), "0");
    }

    #[test]
    fn css_token_filters_to_safe_css_value_bytes() {
        assert_eq!(css_token("#123 abc/def(50%)"), "#123 abc/def(50%)");
        assert_eq!(
            css_token("url(javascript:alert(1))"),
            "url(javascriptalert(1))"
        );
        assert_eq!(css_token("\n\t;"), "initial");
    }

    #[test]
    fn font_char_set_keeps_btree_character_order() {
        let mut fast = FontCharSet::default();
        let mut btree = BTreeSet::new();
        for ch in ['é', 'A', '\n', '•', 'z', '\0', ' ', '—', 'A'] {
            fast.insert(ch);
            btree.insert(ch);
        }
        fast.extend_text("ba\t");
        btree.extend("ba\t".chars());

        let expected: Vec<char> = btree.into_iter().collect();
        assert_eq!(fast.to_chars(), expected);
    }

    #[test]
    fn sanitize_custom_css_neutralizes_style_end_tag() {
        // A stylesheet that tries to close the <style> element and inject markup
        // must not survive as a live HTML end tag.
        let css = "body{color:red}</style><script>alert(1)</script>";
        let out = sanitize_custom_css(css);
        // No `</style` end tag survives to close the raw-text element early.
        assert!(!out.to_ascii_lowercase().contains("</style"));
        // The break-out `</style` became `<\/style` — CSS-harmless (\/ == /).
        assert!(out.contains("<\\/style>"));
        // Non-style markup is left as inert raw-text CSS content, unchanged.
        assert!(out.contains("<script>alert(1)</script>"));
        assert!(out.starts_with("body{color:red}"));
    }

    #[test]
    fn sanitize_custom_css_preserves_case_and_passes_clean_css() {
        assert_eq!(sanitize_custom_css("p{margin:0}"), "p{margin:0}");
        // Mixed-case end tag is still neutralized, original casing preserved.
        let out = sanitize_custom_css("</STYLE>");
        assert_eq!(out, "<\\/STYLE>");
    }

    #[test]
    fn base64_encoder_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn push_u64_writes_decimal_without_padding() {
        let mut out = String::new();
        push_u64(&mut out, 0);
        out.push(',');
        push_u64(&mut out, 42);
        out.push(',');
        push_u64(&mut out, u64::MAX);

        assert_eq!(out, "0,42,18446744073709551615");
    }

    #[test]
    fn escape_text_and_attr_borrow_clean_inputs() {
        assert!(matches!(
            escape_text("plain quoted \" text"),
            Cow::Borrowed(_)
        ));
        assert!(matches!(escape_attr("plain-path_123"), Cow::Borrowed(_)));

        assert_eq!(escape_text("a < b & c").as_ref(), "a &lt; b &amp; c");
        assert!(matches!(escape_text("a < b"), Cow::Owned(_)));
        assert_eq!(
            escape_attr("say \"hi\" & go").as_ref(),
            "say &quot;hi&quot; &amp; go"
        );
        assert!(matches!(escape_attr("say \"hi\""), Cow::Owned(_)));
    }

    #[test]
    fn initial_body_capacity_scales_with_a_cap() {
        assert_eq!(initial_body_capacity(0), 0);
        assert_eq!(initial_body_capacity(8), 32_768);
        assert_eq!(initial_body_capacity(usize::MAX), 4 * 1024 * 1024);
    }

    #[test]
    fn heading_writer_preserves_public_ast_decimal_level() {
        let doc = Document {
            blocks: vec![Block::Heading {
                level: 12,
                inlines: vec![Inline::Text(String::from("Odd Level"))],
            }],
        };
        let html = super::render(&doc, &HtmlOptions::default());

        assert!(html.contains("<h12 id=\"odd-level\">Odd Level</h12>"));
    }

    #[test]
    fn slug_collapses_separator_runs_in_one_pass() {
        let mut heading = String::from("---Alpha");
        heading.push_str(&" _-".repeat(4096));
        heading.push_str("Beta---");

        assert_eq!(slug(&heading), "alpha-beta");
    }

    #[test]
    fn slug_inlines_matches_plain_text_slug_semantics() {
        let inlines = vec![
            Inline::Text(String::from("Alpha")),
            Inline::SoftBreak,
            Inline::Strong(vec![Inline::Text(String::from("Beta_Gamma"))]),
            Inline::HardBreak,
            Inline::Link {
                dest: String::from("https://example.com"),
                title: None,
                content: vec![Inline::Code(String::from("Delta-Value"))],
            },
            Inline::Image {
                dest: String::from("image.png"),
                title: Some(String::from("ignored")),
                alt: String::from("Echo"),
            },
            Inline::Html(String::from("<Raw>")),
        ];

        assert_eq!(slug_inlines(&inlines), slug(&inlines_to_plain(&inlines)));
    }
}
