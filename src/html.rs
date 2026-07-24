//! All-in-one HTML emitter: turns the AST into a single self-contained `.html`
//! document with the default theme stylesheet inlined. The default styling is
//! tuned to look like a high-quality Markdown preview (Cursor/GitHub-grade):
//! readable measure and leading, gorgeous tables with subtle striping, elegant
//! blockquotes, and code blocks ready for syntax highlighting.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, hash_map::Entry};

use crate::ast::{Align, Block, Document, Inline, List};
use crate::fonts::{self, FontStyle};
use crate::highlight::{Span, Tok, highlight_supported_into};
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

    let escaped_title = escape_text(&title);
    let mut html = String::with_capacity(
        186 + escaped_title.len() + css.len() + initial_body_capacity(doc.blocks.len()),
    );
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>");
    html.push_str(&escaped_title);
    html.push_str("</title>\n<style>\n");
    html.push_str(&css);
    html.push_str("</style>\n</head>\n<body>\n<main class=\"fmd\">\n");
    let mut state = RenderState::default();
    render_blocks(&doc.blocks, &mut html, opts, &mut state);
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
struct RenderState<'a> {
    /// Keys are every emitted heading id. Values are the next suffix to try
    /// when that same id text later appears as a heading's base slug.
    heading_id_suffixes: HashMap<String, usize>,
    /// Reused by code block highlighting to avoid one Vec allocation per fence.
    highlight_spans: Vec<Span>,
    /// Bounded per-render cache for repeated non-consecutive code blocks.
    highlight_cache: Vec<HighlightCacheEntry<'a>>,
    highlight_cache_next: usize,
    /// Bounded per-render cache for repeated link/image destinations.
    url_cache: Vec<UrlCacheEntry<'a>>,
    url_cache_next: usize,
}

struct HighlightCacheEntry<'a> {
    lang: &'a str,
    code: &'a str,
    rendered_html: String,
}

struct UrlCacheEntry<'a> {
    context: UrlContext,
    raw_url: &'a str,
    safe_url: Option<&'a str>,
}

const HIGHLIGHT_CACHE_MAX_ENTRIES: usize = 16;
const URL_CACHE_MAX_ENTRIES: usize = 32;

impl<'a> RenderState<'a> {
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

            let mut candidate = String::with_capacity(base.len() + 1 + decimal_len_usize(suffix));
            candidate.push_str(&base);
            candidate.push('-');
            push_usize(&mut candidate, suffix);
            suffix += 1;
            if let Entry::Vacant(entry) = self.heading_id_suffixes.entry(candidate) {
                out.push_str(entry.key());
                entry.insert(1);
                self.heading_id_suffixes.insert(base, suffix);
                return;
            }
        }
    }

    fn highlight_code(&mut self, lang: &'a str, code: &'a str, out: &mut String) {
        if let Some(entry) = self
            .highlight_cache
            .iter()
            .find(|entry| entry.lang == lang && entry.code == code)
        {
            out.push_str(&entry.rendered_html);
            return;
        }

        if !highlight_supported_into(lang, code, &mut self.highlight_spans) {
            push_escaped_text(code, out);
            return;
        }
        let mut rendered_html = String::with_capacity(code.len());
        emit_highlighted_spans(code, &mut rendered_html, &self.highlight_spans);
        out.push_str(&rendered_html);
        self.remember_highlight(lang, code, rendered_html);
    }

    fn remember_highlight(&mut self, lang: &'a str, code: &'a str, rendered_html: String) {
        let entry = HighlightCacheEntry {
            lang,
            code,
            rendered_html,
        };
        if self.highlight_cache.len() < HIGHLIGHT_CACHE_MAX_ENTRIES {
            self.highlight_cache.push(entry);
            return;
        }

        self.highlight_cache[self.highlight_cache_next] = entry;
        self.highlight_cache_next = (self.highlight_cache_next + 1) % HIGHLIGHT_CACHE_MAX_ENTRIES;
    }

    fn safe_url_cached(&mut self, url: &'a str, context: UrlContext) -> Option<&'a str> {
        if let Some(entry) = self
            .url_cache
            .iter()
            .find(|entry| entry.context == context && entry.raw_url == url)
        {
            return entry.safe_url;
        }

        let safe = safe_url(url, context);
        let entry = UrlCacheEntry {
            context,
            raw_url: url,
            safe_url: safe,
        };
        if self.url_cache.len() < URL_CACHE_MAX_ENTRIES {
            self.url_cache.push(entry);
        } else {
            self.url_cache[self.url_cache_next] = entry;
            self.url_cache_next = (self.url_cache_next + 1) % URL_CACHE_MAX_ENTRIES;
        }
        safe
    }
}

fn render_blocks<'a>(
    blocks: &'a [Block],
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'a>,
) {
    for block in blocks {
        render_block(block, out, opts, state);
    }
}

fn initial_body_capacity(blocks: usize) -> usize {
    blocks.saturating_mul(4096).min(4 * 1024 * 1024)
}

fn render_block<'a>(
    block: &'a Block,
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'a>,
) {
    match block {
        Block::Heading { level, inlines } => {
            out.push_str("<h");
            push_u64(out, u64::from(*level));
            out.push_str(" id=\"");
            state.push_heading_id_from_inlines(inlines, out);
            out.push_str("\">");
            render_inlines(inlines, out, opts, state);
            out.push_str("</h");
            push_u64(out, u64::from(*level));
            out.push_str(">\n");
        }
        Block::Paragraph(inlines) => {
            out.push_str("<p>");
            render_inlines(inlines, out, opts, state);
            out.push_str("</p>\n");
        }
        Block::CodeBlock { lang, code } => {
            out.push_str("<pre><code");
            if let Some(l) = lang.as_deref() {
                out.push_str(" class=\"language-");
                push_escaped_attr(l, out);
                out.push('"');
            }
            out.push('>');
            // Supported languages use the shared highlighter; unknown languages
            // remain plain escaped code inside the language-tagged block.
            match lang.as_deref() {
                Some(l) => state.highlight_code(l, code, out),
                None => push_escaped_text(code, out),
            }
            out.push_str("</code></pre>\n");
        }
        Block::BlockQuote(inner) => {
            out.push_str("<blockquote>\n");
            render_blocks(inner, out, opts, state);
            out.push_str("</blockquote>\n");
        }
        Block::List(list) => render_list(list, out, opts, state),
        Block::Table(table) => render_table(table, out, opts, state),
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

fn render_list<'a>(
    list: &'a List,
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'a>,
) {
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
                    render_inlines(inlines, out, opts, state);
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

fn render_table<'a>(
    table: &'a crate::ast::Table,
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'a>,
) {
    out.push_str(
        "<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n",
    );
    out.push_str("<table>\n<thead>\n<tr>");
    render_table_cells(&table.head, &table.align, "<th", "</th>", out, opts, state);
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in &table.rows {
        out.push_str("<tr>");
        render_table_cells(row, &table.align, "<td", "</td>", out, opts, state);
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</div>\n");
}

fn render_table_cells<'a>(
    cells: &'a [Vec<Inline>],
    align: &[Align],
    open: &str,
    close: &str,
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'a>,
) {
    let aligned_len = cells.len().min(align.len());
    let (aligned_cells, unaligned_cells) = cells.split_at(aligned_len);
    for (cell, align) in aligned_cells.iter().zip(&align[..aligned_len]) {
        out.push_str(open);
        out.push_str(align_attr(*align));
        out.push('>');
        render_inlines(cell, out, opts, state);
        out.push_str(close);
    }
    for cell in unaligned_cells {
        out.push_str(open);
        out.push('>');
        render_inlines(cell, out, opts, state);
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

fn render_inlines<'a>(
    inlines: &'a [Inline],
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'a>,
) {
    for inl in inlines {
        match inl {
            Inline::Text(t) => push_escaped_text(t, out),
            Inline::Emphasis(c) => wrap(out, "em", c, opts, state),
            Inline::Strong(c) => wrap(out, "strong", c, opts, state),
            Inline::Strikethrough(c) => wrap(out, "del", c, opts, state),
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
                if let Some(href) = state.safe_url_cached(dest, UrlContext::Link) {
                    out.push_str("<a href=\"");
                    push_escaped_attr(href, out);
                    out.push('"');
                    if let Some(title) = title.as_deref() {
                        out.push_str(" title=\"");
                        push_escaped_attr(title, out);
                        out.push('"');
                    }
                    out.push('>');
                    render_inlines(content, out, opts, state);
                    out.push_str("</a>");
                } else {
                    render_inlines(content, out, opts, state);
                }
            }
            Inline::Image { dest, title, alt } => {
                if let Some(src) = state.safe_url_cached(dest, UrlContext::Image) {
                    out.push_str("<img src=\"");
                    if opts.image_assets.is_empty()
                        || !push_html_image_asset_data_uri(src, opts, out)
                    {
                        push_escaped_attr(src, out);
                    }
                    out.push_str("\" alt=\"");
                    push_escaped_attr(alt, out);
                    out.push('"');
                    if let Some(title) = title.as_deref() {
                        out.push_str(" title=\"");
                        push_escaped_attr(title, out);
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

fn push_html_image_asset_data_uri(dest: &str, opts: &HtmlOptions, out: &mut String) -> bool {
    let Some((mime, bytes)) = html_image_asset(dest, opts) else {
        return false;
    };
    let bytes = if mime == "image/svg+xml" {
        svg_without_remote_style_imports(bytes)
    } else {
        Cow::Borrowed(bytes)
    };
    out.push_str("data:");
    out.push_str(mime);
    out.push_str(";base64,");
    push_base64_encoded(out, bytes.as_ref());
    true
}

fn html_image_asset<'a>(dest: &str, opts: &'a HtmlOptions) -> Option<(&'static str, &'a [u8])> {
    let dest = dest.trim();
    opts.image_assets.iter().find_map(|asset| {
        if asset.destination.trim() == dest {
            html_image_asset_mime(&asset.destination, &asset.bytes)
                .map(|mime| (mime, asset.bytes.as_slice()))
        } else {
            None
        }
    })
}

fn html_image_asset_mime(destination: &str, bytes: &[u8]) -> Option<&'static str> {
    let path = destination
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    let ext = path.rsplit_once('.').map(|(_, ext)| ext)?;
    if ext.eq_ignore_ascii_case("png") && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if ext.eq_ignore_ascii_case("svg") && looks_like_svg(bytes) {
        return Some("image/svg+xml");
    }
    if (ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
        && bytes.starts_with(&[0xFF, 0xD8, 0xFF])
    {
        return Some("image/jpeg");
    }
    None
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    starts_with_ascii_case_insensitive(trimmed, "<svg")
        || (starts_with_ascii_case_insensitive(trimmed, "<?xml")
            && contains_ascii_case_insensitive(trimmed, "<svg"))
}

fn svg_without_remote_style_imports(bytes: &[u8]) -> Cow<'_, [u8]> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Cow::Borrowed(bytes);
    };
    if !contains_ascii_case_insensitive(text, "@import") {
        return Cow::Borrowed(bytes);
    }

    let mut out = String::with_capacity(text.len());
    let mut search = 0;
    let mut last = 0;
    let mut changed = false;
    while let Some(style_rel) = find_ascii_case_insensitive(&text[search..], "<style") {
        let style_start = search + style_rel;
        let Some(open_end_rel) = text[style_start..].find('>') else {
            break;
        };
        let content_start = style_start + open_end_rel + 1;
        let Some(close_rel) = find_ascii_case_insensitive(&text[content_start..], "</style") else {
            break;
        };
        let content_end = content_start + close_rel;
        if let Some(cleaned) = css_without_remote_imports(&text[content_start..content_end]) {
            out.push_str(&text[last..content_start]);
            out.push_str(&cleaned);
            last = content_end;
            changed = true;
        }
        search = content_end;
    }

    if changed {
        out.push_str(&text[last..]);
        Cow::Owned(out.into_bytes())
    } else {
        Cow::Borrowed(bytes)
    }
}

fn css_without_remote_imports(css: &str) -> Option<String> {
    let mut out = String::with_capacity(css.len());
    let mut search = 0;
    let mut last = 0;
    let mut changed = false;
    while let Some(import_rel) = find_ascii_case_insensitive(&css[search..], "@import") {
        let import_start = search + import_rel;
        let Some(statement_end) = css_import_statement_end(css, import_start) else {
            break;
        };
        let statement = &css[import_start..statement_end];
        if css_import_at_rule_boundary(css, import_start)
            && css_import_statement_is_remote(statement)
        {
            out.push_str(&css[last..import_start]);
            let mut next = statement_end;
            while css
                .as_bytes()
                .get(next)
                .is_some_and(u8::is_ascii_whitespace)
            {
                next += 1;
            }
            last = next;
            search = next;
            changed = true;
        } else {
            search = statement_end;
        }
    }

    if changed {
        out.push_str(&css[last..]);
        Some(out)
    } else {
        None
    }
}

fn css_import_statement_is_remote(statement: &str) -> bool {
    contains_ascii_case_insensitive(statement, "http://")
        || contains_ascii_case_insensitive(statement, "https://")
        || css_import_url_value(statement).is_some_and(|url| url.starts_with("//"))
}

fn css_import_url_value(statement: &str) -> Option<&str> {
    let mut value = statement.trim_start();
    if !starts_with_ascii_case_insensitive(value, "@import") {
        return None;
    }
    value = trim_css_ws_and_comments_start(&value["@import".len()..]);
    if starts_with_ascii_case_insensitive(value, "url") {
        let mut args = trim_css_ws_and_comments_start(&value[3..]);
        args = args.strip_prefix('(')?;
        let end = args.rfind(')')?;
        return Some(unquote_css_url(args[..end].trim()));
    }
    let quote = value.as_bytes().first().copied()?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let mut idx = 1usize;
    while idx < value.len() {
        let byte = value.as_bytes()[idx];
        if byte == b'\\' {
            idx = (idx + 2).min(value.len());
            continue;
        }
        if byte == quote {
            return value.get(1..idx);
        }
        idx += 1;
    }
    None
}

fn trim_css_ws_and_comments_start(mut value: &str) -> &str {
    loop {
        value = value.trim_start();
        let Some(rest) = value.strip_prefix("/*") else {
            return value;
        };
        let Some(end) = rest.find("*/") else {
            return value;
        };
        value = &rest[end + 2..];
    }
}

fn unquote_css_url(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && matches!(bytes.first(), Some(b'\'' | b'"'))
        && bytes.last() == bytes.first()
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn css_import_statement_end(css: &str, import_start: usize) -> Option<usize> {
    let bytes = css.as_bytes();
    let mut idx = import_start;
    let mut quote = None;
    let mut paren_depth = 0usize;
    let mut in_comment = false;

    while idx < bytes.len() {
        let byte = bytes[idx];
        if in_comment {
            if byte == b'*' && bytes.get(idx + 1) == Some(&b'/') {
                idx += 2;
                in_comment = false;
            } else {
                idx += 1;
            }
            continue;
        }

        if let Some(quote_byte) = quote {
            if byte == b'\\' {
                idx += 1;
                if idx < bytes.len() {
                    idx += 1;
                }
                continue;
            }
            if byte == quote_byte {
                quote = None;
            }
            idx += 1;
            continue;
        }

        if byte == b'/' && bytes.get(idx + 1) == Some(&b'*') {
            idx += 2;
            in_comment = true;
            continue;
        }

        match byte {
            b'\'' | b'"' => {
                quote = Some(byte);
                idx += 1;
            }
            b'(' => {
                paren_depth += 1;
                idx += 1;
            }
            b')' => {
                paren_depth = paren_depth.saturating_sub(1);
                idx += 1;
            }
            b';' if paren_depth == 0 => return Some(idx + 1),
            _ => idx += 1,
        }
    }

    (quote.is_none() && paren_depth == 0).then_some(bytes.len())
}

fn css_import_at_rule_boundary(css: &str, import_start: usize) -> bool {
    let bytes = css.as_bytes();
    let mut idx = import_start;
    loop {
        while idx > 0 && bytes[idx - 1].is_ascii_whitespace() {
            idx -= 1;
        }
        if idx >= 2
            && bytes[idx - 2] == b'*'
            && bytes[idx - 1] == b'/'
            && let Some(comment_start) = css[..idx - 2].rfind("/*")
        {
            idx = comment_start;
            continue;
        }
        break;
    }
    idx == 0 || matches!(bytes[idx - 1], b';' | b'}')
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    find_ascii_case_insensitive(haystack, needle).is_some()
}

fn starts_with_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle.as_bytes()))
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

fn wrap<'a>(
    out: &mut String,
    tag: &str,
    content: &'a [Inline],
    opts: &HtmlOptions,
    state: &mut RenderState<'a>,
) {
    out.push('<');
    out.push_str(tag);
    out.push('>');
    render_inlines(content, out, opts, state);
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

fn push_usize(out: &mut String, value: usize) {
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

fn decimal_len_usize(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    push_inlines_to_plain(inlines, &mut s);
    s
}

fn push_inlines_to_plain(inlines: &[Inline], out: &mut String) {
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                push_inlines_to_plain(c, out);
            }
            Inline::Link { content, .. } => push_inlines_to_plain(content, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Html(html) => out.push_str(html),
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
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

/// Emit highlighted code: one `<span class="tok-...">` per classified token;
/// plain and potentially symbolic tokens are escaped text with no wrapping span.
fn emit_highlighted_spans(code: &str, out: &mut String, spans: &[Span]) {
    for span in spans.iter().copied() {
        let text = code.get(span.start..span.end).unwrap_or("");
        match span.kind.css_class() {
            Some(cls) => {
                out.push_str("<span class=\"");
                out.push_str(cls);
                out.push_str("\">");
                if highlighted_span_kind_is_html_safe(span.kind) {
                    out.push_str(text);
                } else {
                    push_escaped_text(text, out);
                }
                out.push_str("</span>");
            }
            None => push_escaped_text(text, out),
        }
    }
}

fn highlighted_span_kind_is_html_safe(kind: Tok) -> bool {
    matches!(kind, Tok::Keyword | Tok::Type | Tok::Func | Tok::Number)
}

fn push_escaped_text(s: &str, out: &mut String) {
    // Text nodes only need `&`, `<`, and `>` escaped. Writing into the caller's
    // buffer avoids a temporary allocation for strings that contain escapes.
    let bytes = s.as_bytes();
    let mut start = 0;
    while let Some(rel) = crate::scanner::find_html_text_escape(&bytes[start..]) {
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
    if crate::scanner::find_html_text_escape(bytes).is_none() {
        return Cow::Borrowed(s);
    }
    let mut o = String::with_capacity(s.len());
    push_escaped_text(s, &mut o);
    Cow::Owned(o)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
fn escape_attr(s: &str) -> Cow<'_, str> {
    // The attribute escape set (`& < > "`) is exactly the scanner's
    // `find_html_escape` set, so bulk-copy clean runs and escape each special.
    // All specials are ASCII, so byte indexing is UTF-8-safe.
    let bytes = s.as_bytes();
    if crate::scanner::find_html_escape(bytes).is_none() {
        return Cow::Borrowed(s);
    }
    let mut o = String::with_capacity(s.len());
    push_escaped_attr(s, &mut o);
    Cow::Owned(o)
}

fn push_escaped_attr(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut start = 0;
    while let Some(rel) = crate::scanner::find_html_escape(&bytes[start..]) {
        let pos = start + rel;
        out.push_str(&s[start..pos]);
        match bytes[pos] {
            b'&' => out.push_str("&amp;"),
            b'<' => out.push_str("&lt;"),
            b'>' => out.push_str("&gt;"),
            b'"' => out.push_str("&quot;"),
            _ => {}
        }
        start = pos + 1;
    }
    out.push_str(&s[start..]);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UrlContext {
    Link,
    Image,
}

enum UrlScheme<'a> {
    None,
    Scheme(&'a str),
    Suspicious,
}

fn safe_url(url: &str, context: UrlContext) -> Option<&str> {
    let trimmed = url.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    if trimmed.is_empty() && matches!(context, UrlContext::Image) {
        return None;
    }
    match url_scheme(trimmed) {
        UrlScheme::None => Some(trimmed),
        UrlScheme::Scheme(scheme) if allowed_url_scheme(scheme, context) => Some(trimmed),
        UrlScheme::Scheme(_) | UrlScheme::Suspicious => None,
    }
}

fn url_scheme(url: &str) -> UrlScheme<'_> {
    let mut skipped_gap = false;
    for (idx, byte) in url.bytes().enumerate() {
        if matches!(byte, b'/' | b'?' | b'#') {
            return UrlScheme::None;
        }
        if byte == b':' {
            let scheme = &url[..idx];
            if skipped_gap || !valid_url_scheme(scheme) {
                return UrlScheme::Suspicious;
            }
            return UrlScheme::Scheme(scheme);
        }
        if byte.is_ascii_whitespace() || byte.is_ascii_control() {
            skipped_gap = true;
        }
    }
    UrlScheme::None
}

fn valid_url_scheme(scheme: &str) -> bool {
    let Some((&first, rest)) = scheme.as_bytes().split_first() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && rest
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn allowed_url_scheme(scheme: &str, context: UrlContext) -> bool {
    match context {
        UrlContext::Link => {
            scheme.eq_ignore_ascii_case("http")
                || scheme.eq_ignore_ascii_case("https")
                || scheme.eq_ignore_ascii_case("mailto")
                || scheme.eq_ignore_ascii_case("tel")
        }
        UrlContext::Image => {
            scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
        }
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
#[cfg_attr(coverage_nightly, coverage(off))]
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

#[derive(Default)]
struct FontCharSet {
    ascii_mask: u128,
    non_ascii: BTreeSet<char>,
}

impl FontCharSet {
    fn is_empty(&self) -> bool {
        self.ascii_mask == 0 && self.non_ascii.is_empty()
    }

    fn insert(&mut self, ch: char) {
        if ch.is_ascii() {
            self.ascii_mask |= ascii_char_mask(ch as u8);
        } else {
            self.non_ascii.insert(ch);
        }
    }

    fn extend_text(&mut self, text: &str) {
        let mut mask = 0u128;
        for (idx, &byte) in text.as_bytes().iter().enumerate() {
            if byte.is_ascii() {
                mask |= ascii_char_mask(byte);
            } else {
                self.ascii_mask |= mask;
                for ch in text[idx..].chars() {
                    self.insert(ch);
                }
                return;
            }
        }
        self.ascii_mask |= mask;
    }

    #[cfg(test)]
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn extend_text_slow_reference(&mut self, text: &str) {
        for ch in text.chars() {
            self.insert(ch);
        }
    }

    fn to_chars(&self) -> Vec<char> {
        let mut chars =
            Vec::with_capacity(self.non_ascii.len() + self.ascii_mask.count_ones() as usize);
        let mut mask = self.ascii_mask;
        while mask != 0 {
            let idx = mask.trailing_zeros() as u8;
            chars.push(char::from(idx));
            mask &= mask - 1;
        }
        chars.extend(self.non_ascii.iter().copied());
        chars
    }
}

fn ascii_char_mask(byte: u8) -> u128 {
    debug_assert!(byte < 128);
    ASCII_CHAR_MASKS[byte as usize]
}

const ASCII_CHAR_MASKS: [u128; 256] = ascii_char_masks();

const fn ascii_char_masks() -> [u128; 256] {
    let mut masks = [0u128; 256];
    let mut idx = 0usize;
    while idx < 128 {
        masks[idx] = 1u128 << idx;
        idx += 1;
    }
    masks
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

#[derive(Clone, Copy)]
struct HtmlFontFace<'a> {
    bytes: &'a [u8],
    parsed: Option<&'static Font>,
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
        body_font_face(font_assets, theme.font, FontStyle::Regular),
        &usage.body_regular,
    );
    has_body |= !usage.body_regular.is_empty();

    push_font_face(
        &mut css,
        "FMD Body",
        "normal",
        "700",
        body_font_face(font_assets, theme.font, FontStyle::Bold),
        &usage.body_bold,
    );
    has_body |= !usage.body_bold.is_empty();

    push_font_face(
        &mut css,
        "FMD Body",
        "italic",
        "400",
        body_font_face(font_assets, theme.font, FontStyle::Italic),
        &usage.body_italic,
    );
    has_body |= !usage.body_italic.is_empty();

    push_font_face(
        &mut css,
        "FMD Body",
        "italic",
        "700",
        body_font_face(font_assets, theme.font, FontStyle::BoldItalic),
        &usage.body_bold_italic,
    );
    has_body |= !usage.body_bold_italic.is_empty();

    push_font_face(
        &mut css,
        "FMD Mono",
        "normal",
        "400",
        mono_font_face(font_assets, FontStyle::Regular),
        &usage.mono,
    );
    has_mono |= !usage.mono.is_empty();

    EmbeddedFontCss {
        css,
        has_body,
        has_mono,
    }
}

fn body_font_face(
    font_assets: &FontAssets,
    family: crate::FontFamily,
    style: FontStyle,
) -> HtmlFontFace<'_> {
    match style {
        FontStyle::Regular => html_font_face(
            font_assets.body_regular.as_deref(),
            || fonts::body_bytes(family, style),
            || fonts::body_font(family, style).ok(),
        ),
        FontStyle::Bold => html_font_face(
            font_assets.body_bold.as_deref(),
            || fonts::body_bytes(family, style),
            || fonts::body_font(family, style).ok(),
        ),
        FontStyle::Italic => html_font_face(
            font_assets.body_italic.as_deref(),
            || fonts::body_bytes(family, style),
            || fonts::body_font(family, style).ok(),
        ),
        FontStyle::BoldItalic => html_font_face(
            font_assets.body_bold_italic.as_deref(),
            || fonts::body_bytes(family, style),
            || fonts::body_font(family, style).ok(),
        ),
    }
}

fn mono_font_face(font_assets: &FontAssets, style: FontStyle) -> HtmlFontFace<'_> {
    html_font_face(
        font_assets.mono_regular.as_deref(),
        || fonts::mono_bytes(style),
        || fonts::mono_font(style).ok(),
    )
}

fn html_font_face<'a>(
    custom_bytes: Option<&'a [u8]>,
    default_bytes: impl FnOnce() -> &'static [u8],
    default_font: impl FnOnce() -> Option<&'static Font>,
) -> HtmlFontFace<'a> {
    match custom_bytes {
        Some(bytes) => HtmlFontFace {
            bytes,
            parsed: None,
        },
        None => HtmlFontFace {
            bytes: default_bytes(),
            parsed: default_font(),
        },
    }
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
    font_face: HtmlFontFace<'_>,
    chars: &FontCharSet,
) {
    if chars.is_empty() {
        return;
    }

    let keep = chars.to_chars();
    let subset = font_face
        .parsed
        .and_then(|font| font.subset(&keep))
        .or_else(|| {
            Font::parse(font_face.bytes.to_vec())
                .ok()
                .and_then(|font| font.subset(&keep))
        })
        .unwrap_or_else(|| font_face.bytes.to_vec());
    let encoded_len = base64_encoded_len(subset.len());
    css.reserve(font_face_css_capacity(
        family.len(),
        style.len(),
        weight.len(),
        encoded_len,
    ));
    css.push_str("@font-face {\n");
    css.push_str("  font-family: \"");
    css.push_str(family);
    css.push_str("\";\n  font-style: ");
    css.push_str(style);
    css.push_str(";\n  font-weight: ");
    css.push_str(weight);
    css.push_str(";\n  font-display: swap;\n  src: url(\"data:font/ttf;base64,");
    push_base64_encoded(css, &subset);
    css.push_str("\") format(\"truetype\");\n}\n");
}

fn font_face_css_capacity(
    family_len: usize,
    style_len: usize,
    weight_len: usize,
    encoded_len: usize,
) -> usize {
    128usize
        .saturating_add(family_len)
        .saturating_add(style_len)
        .saturating_add(weight_len)
        .saturating_add(encoded_len)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(base64_encoded_len(bytes.len()));
    push_base64_encoded(&mut out, bytes);
    out
}

fn base64_encoded_len(byte_len: usize) -> usize {
    byte_len.div_ceil(3) * 4
}

fn push_base64_encoded(out: &mut String, bytes: &[u8]) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let start_len = out.len();
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
    debug_assert_eq!(out.len() - start_len, base64_encoded_len(bytes.len()));
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
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use std::borrow::Cow;
    use std::collections::BTreeSet;

    use crate::ast::{Block, Document, Inline};
    use crate::highlight::{Span, Tok};
    use crate::{HtmlOptions, PdfImageAsset};

    use super::{
        FontCharSet, HTML_FONT_SEED, RenderState, UrlContext, ascii_char_mask, ascii_char_masks,
        base64_encode, css_num, css_token, css_without_remote_imports, emit_highlighted_spans,
        escape_attr, escape_text, find_ascii_case_insensitive, highlighted_span_kind_is_html_safe,
        html_image_asset_mime, initial_body_capacity, inlines_to_plain, push_escaped_attr,
        push_html_image_asset_data_uri, push_u64, render, sanitize_custom_css, slug, slug_inlines,
        svg_without_remote_style_imports,
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
    fn repeated_supported_code_blocks_reuse_non_consecutive_highlight_html() {
        let mut state = RenderState::default();
        let rust = "fn main() { println!(\"hi\"); }\n";
        let python = "print('hi')\n";

        let mut first_rust = String::new();
        state.highlight_code("rust", rust, &mut first_rust);
        assert!(!state.highlight_spans.is_empty());

        let mut first_python = String::new();
        state.highlight_code("python", python, &mut first_python);

        let mut second_rust = String::new();
        state.highlight_code("rust", rust, &mut second_rust);

        assert_eq!(second_rust, first_rust);
        assert_eq!(state.highlight_cache.len(), 2);
        assert!(
            state
                .highlight_cache
                .iter()
                .any(|entry| entry.lang == "rust"
                    && entry.code == rust
                    && entry.rendered_html == first_rust)
        );
        assert!(
            state
                .highlight_cache
                .iter()
                .any(|entry| entry.lang == "python"
                    && entry.code == python
                    && entry.rendered_html == first_python)
        );
    }

    #[test]
    fn unknown_code_blocks_emit_plain_text_without_cache_entry() {
        let mut state = RenderState::default();
        let rust = "fn main() {}\n";
        let unknown = "1 < 2 && 3\n";

        let mut rust_out = String::new();
        state.highlight_code("rust", rust, &mut rust_out);
        assert_eq!(state.highlight_cache.len(), 1);
        assert!(!state.highlight_spans.is_empty());

        let mut unknown_out = String::new();
        state.highlight_code("not-a-language", unknown, &mut unknown_out);

        assert_eq!(unknown_out, "1 &lt; 2 &amp;&amp; 3\n");
        assert_eq!(
            state.highlight_cache.len(),
            1,
            "unsupported languages must not evict supported highlight entries"
        );
        assert!(
            state
                .highlight_cache
                .iter()
                .any(|entry| entry.lang == "rust" && entry.code == rust)
        );
        assert!(
            state.highlight_spans.is_empty(),
            "unsupported languages should leave no stale reusable spans"
        );
    }

    #[test]
    fn url_cache_memoizes_safe_and_rejected_destinations_by_context() {
        let mut state = RenderState::default();
        let link = " https://example.com/path?a=1 ";
        let unsafe_link = "java script:alert(1)";
        let mailto = "mailto:team@example.com";

        assert_eq!(
            state.safe_url_cached(link, UrlContext::Link),
            Some("https://example.com/path?a=1")
        );
        assert_eq!(state.url_cache.len(), 1);
        assert_eq!(
            state.safe_url_cached(link, UrlContext::Link),
            Some("https://example.com/path?a=1")
        );
        assert_eq!(
            state.url_cache.len(),
            1,
            "repeated safe URLs should reuse the cached decision"
        );

        assert_eq!(state.safe_url_cached(unsafe_link, UrlContext::Link), None);
        assert_eq!(state.url_cache.len(), 2);
        assert_eq!(state.safe_url_cached(unsafe_link, UrlContext::Link), None);
        assert_eq!(
            state.url_cache.len(),
            2,
            "repeated rejected URLs should reuse the cached decision"
        );

        assert_eq!(
            state.safe_url_cached(mailto, UrlContext::Link),
            Some(mailto)
        );
        assert_eq!(state.safe_url_cached(mailto, UrlContext::Image), None);
        assert_eq!(
            state.url_cache.len(),
            4,
            "the same raw URL has distinct safety decisions by render context"
        );
    }

    #[test]
    fn highlighted_safe_token_kinds_bypass_escape_scan_but_unsafe_tokens_escape() {
        for kind in [Tok::Keyword, Tok::Type, Tok::Func, Tok::Number] {
            assert!(highlighted_span_kind_is_html_safe(kind), "{kind:?}");
        }
        for kind in [
            Tok::Plain,
            Tok::Str,
            Tok::Comment,
            Tok::Operator,
            Tok::Punct,
        ] {
            assert!(!highlighted_span_kind_is_html_safe(kind), "{kind:?}");
        }

        let code = "fn < &";
        let spans = [
            Span {
                kind: Tok::Keyword,
                start: 0,
                end: 2,
            },
            Span {
                kind: Tok::Plain,
                start: 2,
                end: 3,
            },
            Span {
                kind: Tok::Operator,
                start: 3,
                end: 4,
            },
            Span {
                kind: Tok::Plain,
                start: 4,
                end: 5,
            },
            Span {
                kind: Tok::Punct,
                start: 5,
                end: 6,
            },
        ];
        let mut out = String::new();
        emit_highlighted_spans(code, &mut out, &spans);

        assert_eq!(
            out,
            "<span class=\"tok-kw\">fn</span> <span class=\"tok-op\">&lt;</span> <span class=\"tok-pn\">&amp;</span>"
        );
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
    fn font_char_set_extend_text_matches_per_char_reference() {
        for text in [
            "",
            "Plain ASCII text 123 !?",
            "\0\t\n ASCII controls and punctuation",
            "é",
            "ASCII prefix then Café — and emoji \u{1F600}",
            "Unicode first Ω then ASCII tail",
            HTML_FONT_SEED,
        ] {
            let mut fast = FontCharSet::default();
            let mut reference = FontCharSet::default();

            fast.extend_text(text);
            reference.extend_text_slow_reference(text);

            assert_eq!(fast.ascii_mask, reference.ascii_mask, "text {text:?}");
            assert_eq!(fast.non_ascii, reference.non_ascii, "text {text:?}");
            assert_eq!(fast.to_chars(), reference.to_chars(), "text {text:?}");
        }
    }

    #[test]
    fn ascii_char_mask_table_matches_shift_definition() {
        for byte in 0u8..=127 {
            assert_eq!(ascii_char_mask(byte), 1u128 << u32::from(byte));
        }
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
        assert_eq!(base64_encode(&[0xff]), "/w==");
        assert_eq!(base64_encode(&[0x00, 0xff]), "AP8=");
        assert_eq!(
            base64_encode(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05]),
            "AAECAwQF"
        );
        assert_eq!(
            base64_encode(&[0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa]),
            "/+7dzLuq"
        );
    }

    #[test]
    fn svg_asset_data_uri_strips_remote_style_imports() {
        let dirty = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>@import url('https://fonts.googleapis.com/css2?family=Inter');
.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let clean = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new("diagram.svg", dirty.as_slice())],
            ..HtmlOptions::default()
        };
        let mut out = String::new();

        assert!(push_html_image_asset_data_uri(
            "diagram.svg",
            &opts,
            &mut out
        ));
        assert_eq!(
            out,
            format!(
                "data:image/svg+xml;base64,{}",
                base64_encode(clean.as_slice())
            )
        );
    }

    #[test]
    fn svg_asset_data_uri_accepts_case_insensitive_xml_and_svg_tags() {
        let svg = br#"<?XML version="1.0" encoding="UTF-8"?>
<SVG xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<rect width="10" height="10"/></SVG>"#;
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new("diagram.svg", svg.as_slice())],
            ..HtmlOptions::default()
        };
        let mut out = String::new();

        assert!(push_html_image_asset_data_uri(
            "diagram.svg",
            &opts,
            &mut out
        ));
        assert_eq!(
            out,
            format!(
                "data:image/svg+xml;base64,{}",
                base64_encode(svg.as_slice())
            )
        );
    }

    #[test]
    fn svg_asset_data_uri_strips_remote_imports_with_semicolon_rich_urls() {
        let dirty = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&amp;display=swap');
.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let clean = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new("diagram.svg", dirty.as_slice())],
            ..HtmlOptions::default()
        };
        let mut out = String::new();

        assert!(push_html_image_asset_data_uri(
            "diagram.svg",
            &opts,
            &mut out
        ));
        assert_eq!(
            out,
            format!(
                "data:image/svg+xml;base64,{}",
                base64_encode(clean.as_slice())
            )
        );
    }

    #[test]
    fn svg_asset_data_uri_strips_protocol_relative_remote_style_imports() {
        let dirty = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>@import url(//cdn.example.test/fmd.css);
@import "//fonts.example.test/fmd.css";
@IMPORT /* remote */ URL("//cdn.example.test/upper.css");
.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let clean = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new("diagram.svg", dirty.as_slice())],
            ..HtmlOptions::default()
        };
        let mut out = String::new();

        assert!(push_html_image_asset_data_uri(
            "diagram.svg",
            &opts,
            &mut out
        ));
        assert_eq!(
            out,
            format!(
                "data:image/svg+xml;base64,{}",
                base64_encode(clean.as_slice())
            )
        );
    }

    #[test]
    fn svg_asset_data_uri_strips_remote_import_at_style_eof_without_semicolon() {
        let dirty = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>@import url('https://fonts.example.test/fmd.css')</style>
<style>.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let clean = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style></style>
<style>.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new("diagram.svg", dirty.as_slice())],
            ..HtmlOptions::default()
        };
        let mut out = String::new();

        assert!(push_html_image_asset_data_uri(
            "diagram.svg",
            &opts,
            &mut out
        ));
        assert_eq!(
            out,
            format!(
                "data:image/svg+xml;base64,{}",
                base64_encode(clean.as_slice())
            )
        );
    }

    #[test]
    fn checked_in_showcase_svg_drops_google_fonts_import_without_residue() {
        let clean =
            svg_without_remote_style_imports(include_bytes!("../examples/showcase-mermaid.svg"));
        assert!(
            std::str::from_utf8(clean.as_ref()).is_ok(),
            "showcase SVG should stay UTF-8"
        );
        let clean_text = String::from_utf8_lossy(clean.as_ref());

        assert!(!clean_text.contains("@import"));
        assert!(!clean_text.contains("fonts.googleapis"));
        assert!(!clean_text.contains("500;600;700"));
        assert!(clean_text.contains(".fm-text"));
        assert!(matches!(clean, Cow::Owned(_)));
    }

    #[test]
    fn svg_asset_data_uri_leaves_non_remote_imports_unchanged() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>@import url('diagram.css');
@import url('/assets//diagram.css');
.node{fill:#123456}</style>
<rect class="node" width="10" height="10"/></svg>"#;

        assert!(matches!(
            svg_without_remote_style_imports(svg),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn svg_asset_data_uri_does_not_strip_import_text_inside_css_strings() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg">
<style>.node::before{content:"@import url('https://example.com/not-a-rule.css');"}</style>
<rect class="node" width="10" height="10"/></svg>"#;

        assert!(matches!(
            svg_without_remote_style_imports(svg),
            Cow::Borrowed(_)
        ));
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
        let mut attr = String::from("prefix:");
        push_escaped_attr("say \"hi\" & go", &mut attr);
        assert_eq!(attr, "prefix:say &quot;hi&quot; &amp; go");
        assert_eq!(
            escape_attr("say \"hi\" & go").as_ref(),
            &attr["prefix:".len()..]
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
    fn render_writes_body_between_document_envelope() {
        let doc = Document {
            blocks: vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![Inline::Text("Title".to_string())],
                },
                Block::Paragraph(vec![Inline::Text("A < B".to_string())]),
            ],
        };
        let opts = HtmlOptions {
            title: Some("Pinned".to_string()),
            custom_css: Some("p{margin:0}".to_string()),
            ..HtmlOptions::default()
        };

        let html = render(&doc, &opts);

        assert_eq!(
            html,
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>Pinned</title>\n<style>\np{margin:0}</style>\n</head>\n<body>\n\
<main class=\"fmd\">\n<h1 id=\"title\">Title</h1>\n<p>A &lt; B</p>\n\
</main>\n</body>\n</html>\n"
        );
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

    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

    #[test]
    fn image_asset_mime_requires_matching_extension_and_magic() {
        // PNG: extension (case-insensitive, query/fragment stripped) AND magic.
        assert_eq!(
            html_image_asset_mime("chart.png", PNG_MAGIC),
            Some("image/png")
        );
        assert_eq!(
            html_image_asset_mime("a.PNG?v=1#frag", PNG_MAGIC),
            Some("image/png")
        );
        assert_eq!(html_image_asset_mime("chart.png", b"not-a-png"), None);

        // SVG: extension AND content that actually reads as SVG.
        assert_eq!(
            html_image_asset_mime("pic.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            Some("image/svg+xml")
        );
        assert_eq!(
            html_image_asset_mime("pic.svg", b"\xff\xfe"),
            None,
            "non-UTF-8 bytes can never be SVG"
        );
        assert_eq!(
            html_image_asset_mime("pic.svg", b"<?xml version='1.0'?><rect/>"),
            None,
            "an XML prolog without an <svg> root is not SVG"
        );
        assert_eq!(
            html_image_asset_mime("pic.svg", b"plain text"),
            None,
            "text with neither an <svg> root nor an XML prolog is not SVG"
        );

        // No extension at all: never emitted as a data URI.
        assert_eq!(html_image_asset_mime("logo", PNG_MAGIC), None);
    }

    #[test]
    fn png_asset_renders_as_data_uri_and_unmatched_asset_falls_back_to_url() {
        let doc = Document {
            blocks: vec![Block::Paragraph(vec![
                Inline::Image {
                    dest: "chart.png".to_string(),
                    title: None,
                    alt: "Chart".to_string(),
                },
                Inline::Image {
                    dest: "missing.png".to_string(),
                    title: None,
                    alt: "Other".to_string(),
                },
            ])],
        };
        let opts = HtmlOptions {
            image_assets: vec![
                // A non-matching asset first, so lookup iterates past it.
                PdfImageAsset::new("decoy.svg", b"<svg/>".as_slice()),
                PdfImageAsset::new("chart.png", PNG_MAGIC),
            ],
            ..HtmlOptions::default()
        };

        let html = render(&doc, &opts);

        assert!(
            html.contains("<img src=\"data:image/png;base64,iVBORw0KGgo=\" alt=\"Chart\">"),
            "matched PNG asset must inline as a data URI: {html}"
        );
        assert!(
            html.contains("<img src=\"missing.png\" alt=\"Other\">"),
            "unmatched destination must keep the original URL: {html}"
        );
    }

    #[test]
    fn svg_stripper_returns_borrowed_for_non_utf8_or_no_remote_url_inputs() {
        // Invalid UTF-8 is passed through untouched.
        assert!(matches!(
            svg_without_remote_style_imports(b"\xff\xfe@import http://x"),
            Cow::Borrowed(_)
        ));
        // An @import with no remote URL anywhere in the SVG returns early.
        assert!(matches!(
            svg_without_remote_style_imports(b"<svg><style>@import url('x.css');</style></svg>"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn svg_stripper_detects_https_only_remote_imports() {
        // No plain `http://` anywhere (the xmlns is https too): the https probe
        // alone must trigger the strip.
        let dirty = b"<svg xmlns=\"https://www.w3.org/2000/svg\">\
<style>@import url('https://fonts.example/css');.a{fill:#123}</style></svg>";
        let clean =
            String::from_utf8_lossy(svg_without_remote_style_imports(dirty).as_ref()).into_owned();
        assert!(!clean.contains("@import"), "{clean}");
        assert!(!clean.contains("fonts.example"), "{clean}");
        assert!(clean.contains(".a{fill:#123}"), "{clean}");
    }

    #[test]
    fn svg_stripper_stops_at_unclosed_style_tags() {
        // `<style` with no `>` at all.
        assert!(matches!(
            svg_without_remote_style_imports(b"<svg d='@import http://x'><style"),
            Cow::Borrowed(_)
        ));
        // `<style>` opened but never closed with `</style`.
        assert!(matches!(
            svg_without_remote_style_imports(b"<svg d='@import http://x'><style>.a{}"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn css_import_stripping_handles_comments_quotes_parens_and_boundaries() {
        // A comment inside the @import statement is scanned through (including a
        // lone '*' that does not close it).
        assert_eq!(
            css_without_remote_imports("@import /* a*b */ url(http://x);rest").as_deref(),
            Some("rest")
        );
        // A backslash-escaped quote inside the URL string does not end it.
        assert_eq!(
            css_without_remote_imports("@import \"a\\\"b http://x\";p{}").as_deref(),
            Some("p{}")
        );
        // A ';' inside url(...) parens does not terminate the statement.
        assert_eq!(
            css_without_remote_imports("@import url(http://x;y);tail").as_deref(),
            Some("tail")
        );
        // Style-block EOF is a valid at-rule boundary, even without ';'.
        assert_eq!(
            css_without_remote_imports("@import url(http://x)").as_deref(),
            Some("")
        );
        // A structurally incomplete url(...) is still left untouched.
        assert_eq!(css_without_remote_imports("@import url(http://x"), None);
        // A trailing backslash inside a quoted URL cannot escape past EOF.
        assert_eq!(css_without_remote_imports("@import \"http://x\\"), None);
        // `*X` (not `*/`) before @import is no comment end; `*/` with no
        // matching `/*` earlier is not a comment either — neither is a rule
        // boundary, so both stay untouched.
        assert_eq!(css_without_remote_imports("**@import url(http://x);"), None);
        assert_eq!(css_without_remote_imports("*/@import url(http://x);"), None);
        // At-rule boundary holds across a preceding comment...
        assert_eq!(
            css_without_remote_imports("p{}/* note */@import url(http://x);").as_deref(),
            Some("p{}/* note */")
        );
        // ...but a non-boundary "@import" (mid-declaration) is not a rule.
        assert_eq!(css_without_remote_imports("a@import url(http://x);"), None);
        // A local import is preserved while the remote one after it is dropped.
        assert_eq!(
            css_without_remote_imports("@import url(local.css);@import url(http://r);x").as_deref(),
            Some("@import url(local.css);x")
        );
    }

    #[test]
    fn find_ascii_case_insensitive_empty_needle_matches_at_start() {
        assert_eq!(find_ascii_case_insensitive("abc", ""), Some(0));
        assert_eq!(find_ascii_case_insensitive("", ""), Some(0));
        assert_eq!(find_ascii_case_insensitive("aBc", "bC"), Some(1));
    }

    #[test]
    fn ascii_char_masks_builder_sets_exactly_one_bit_per_ascii_index() {
        let masks = ascii_char_masks();
        for (idx, mask) in masks.iter().enumerate() {
            if idx < 128 {
                assert_eq!(*mask, 1u128 << idx, "mask for ASCII {idx}");
            } else {
                assert_eq!(*mask, 0, "non-ASCII index {idx} must stay empty");
            }
        }
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
