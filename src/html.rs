//! All-in-one HTML emitter: turns the AST into a single self-contained `.html`
//! document with the default theme stylesheet inlined. The default styling is
//! tuned to look like a high-quality Markdown preview (Cursor/GitHub-grade):
//! readable measure and leading, gorgeous tables with subtle striping, elegant
//! blockquotes, and code blocks ready for syntax highlighting.

use std::collections::{BTreeMap, BTreeSet};

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
        .clone()
        .unwrap_or_else(|| default_css(doc, opts));

    let mut body = String::new();
    let mut state = RenderState::default();
    render_blocks(&doc.blocks, &mut body, opts, &mut state);

    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{css}</style>\n</head>\n\
         <body>\n<main class=\"fmd\">\n{body}</main>\n</body>\n</html>\n",
        title = escape_text(&title),
    )
}

fn first_heading_text(doc: &Document) -> Option<String> {
    doc.blocks.iter().find_map(|b| match b {
        Block::Heading { inlines, .. } => Some(inlines_to_plain(inlines)),
        _ => None,
    })
}

#[derive(Default)]
struct RenderState {
    used_heading_ids: BTreeSet<String>,
    next_heading_suffix: BTreeMap<String, usize>,
}

impl RenderState {
    fn heading_id(&mut self, text: &str) -> String {
        let mut base = slug(text);
        if base.is_empty() {
            base.push_str("section");
        }

        let mut suffix = self.next_heading_suffix.get(&base).copied().unwrap_or(1);
        loop {
            let candidate = if suffix == 1 {
                base.clone()
            } else {
                format!("{base}-{suffix}")
            };
            suffix += 1;
            if self.used_heading_ids.insert(candidate.clone()) {
                self.next_heading_suffix.insert(base, suffix);
                return candidate;
            }
        }
    }
}

fn render_blocks(blocks: &[Block], out: &mut String, opts: &HtmlOptions, state: &mut RenderState) {
    for block in blocks {
        render_block(block, out, opts, state);
    }
}

fn render_block(block: &Block, out: &mut String, opts: &HtmlOptions, state: &mut RenderState) {
    match block {
        Block::Heading { level, inlines } => {
            let id = state.heading_id(&inlines_to_plain(inlines));
            out.push_str(&format!("<h{level} id=\"{}\">", escape_attr(&id)));
            render_inlines(inlines, out, opts);
            out.push_str(&format!("</h{level}>\n"));
        }
        Block::Paragraph(inlines) => {
            out.push_str("<p>");
            render_inlines(inlines, out, opts);
            out.push_str("</p>\n");
        }
        Block::CodeBlock { lang, code } => {
            let cls = lang
                .as_deref()
                .map(|l| format!(" class=\"language-{}\"", escape_attr(l)))
                .unwrap_or_default();
            out.push_str(&format!("<pre><code{cls}>"));
            // Clean-room syntax highlighting when we have a lexer for the
            // language; otherwise the code is rendered as escaped plain text.
            match lang.as_deref() {
                Some(l) if crate::highlight::is_supported(l) => {
                    highlight_code(l, code, out);
                }
                _ => out.push_str(&escape_text(code)),
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
                out.push_str(&format!("<p>{}</p>\n", escape_text(html)));
            }
        }
    }
}

fn render_list(list: &List, out: &mut String, opts: &HtmlOptions, state: &mut RenderState) {
    let tag = if list.ordered { "ol" } else { "ul" };
    if list.ordered && list.start != 1 {
        out.push_str(&format!("<{tag} start=\"{}\">\n", list.start));
    } else {
        out.push_str(&format!("<{tag}>\n"));
    }
    for item in &list.items {
        match item.task {
            Some(checked) => {
                let mark = if checked { " checked" } else { "" };
                out.push_str(&format!(
                    "<li class=\"task\"><input type=\"checkbox\" disabled{mark}> "
                ));
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
    out.push_str(&format!("</{tag}>\n"));
}

fn render_table(table: &crate::ast::Table, out: &mut String, opts: &HtmlOptions) {
    out.push_str(
        "<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n",
    );
    out.push_str("<table>\n<thead>\n<tr>");
    for (idx, cell) in table.head.iter().enumerate() {
        let align = align_attr(table.align.get(idx).copied().unwrap_or(Align::None));
        out.push_str(&format!("<th{align}>"));
        render_inlines(cell, out, opts);
        out.push_str("</th>");
    }
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in &table.rows {
        out.push_str("<tr>");
        for (idx, cell) in row.iter().enumerate() {
            let align = align_attr(table.align.get(idx).copied().unwrap_or(Align::None));
            out.push_str(&format!("<td{align}>"));
            render_inlines(cell, out, opts);
            out.push_str("</td>");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</div>\n");
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
            Inline::Text(t) => out.push_str(&escape_text(t)),
            Inline::Emphasis(c) => wrap(out, "em", c, opts),
            Inline::Strong(c) => wrap(out, "strong", c, opts),
            Inline::Strikethrough(c) => wrap(out, "del", c, opts),
            Inline::Code(t) => out.push_str(&format!("<code>{}</code>", escape_text(t))),
            Inline::Link {
                dest,
                title,
                content,
            } => {
                if let Some(href) = safe_url(dest, UrlContext::Link) {
                    let t = title
                        .as_deref()
                        .map(|s| format!(" title=\"{}\"", escape_attr(s)))
                        .unwrap_or_default();
                    out.push_str(&format!("<a href=\"{}\"{t}>", escape_attr(href)));
                    render_inlines(content, out, opts);
                    out.push_str("</a>");
                } else {
                    render_inlines(content, out, opts);
                }
            }
            Inline::Image { dest, title, alt } => {
                if let Some(src) = safe_url(dest, UrlContext::Image) {
                    let t = title
                        .as_deref()
                        .map(|s| format!(" title=\"{}\"", escape_attr(s)))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "<img src=\"{}\" alt=\"{}\"{t}>",
                        escape_attr(src),
                        escape_attr(alt)
                    ));
                } else {
                    out.push_str(&escape_text(alt));
                }
            }
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br>\n"),
            Inline::Html(h) => {
                if opts.allow_raw_html {
                    out.push_str(h);
                } else {
                    out.push_str(&escape_text(h));
                }
            }
        }
    }
}

fn wrap(out: &mut String, tag: &str, content: &[Inline], opts: &HtmlOptions) {
    out.push_str(&format!("<{tag}>"));
    render_inlines(content, out, opts);
    out.push_str(&format!("</{tag}>"));
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

fn slug(text: &str) -> String {
    let mut s = String::new();
    let mut pending_dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !s.is_empty() {
                s.push('-');
            }
            s.push(c.to_ascii_lowercase());
            pending_dash = false;
        } else if c == ' ' || c == '-' || c == '_' {
            pending_dash = true;
        }
    }
    s
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
                out.push_str(&escape_text(text));
                out.push_str("</span>");
            }
            None => out.push_str(&escape_text(text)),
        }
    }
}

fn escape_text(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            _ => o.push(c),
        }
    }
    o
}

fn escape_attr(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
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

    let dark = match theme.dark_mode {
        DarkModePolicy::Auto => dark_mode_css(&theme.dark_colors),
        DarkModePolicy::Disabled => String::new(),
    };
    let token_dark = match theme.dark_mode {
        DarkModePolicy::Auto => TOKEN_DARK_CSS,
        DarkModePolicy::Disabled => "",
    };

    format!(
        "{}{}\n\
:root {{ --fmd-base: {}px; --fmd-measure: {}px; --fmd-line-height: {}; \
--fmd-radius: {}px; --fmd-table-pad-y: {}em; --fmd-table-pad-x: {}em; \
--fmd-font-body: {body_font}; --fmd-font-mono: {mono_font}; }}\n\
{BASE_CSS}\n{TOKEN_CSS}\n{dark}{token_dark}",
        embedded.css,
        color_vars(colors),
        spacing.base_px,
        spacing.max_width_px,
        css_num(spacing.line_height),
        spacing.radius_px,
        css_num(spacing.table_cell_padding_y_em),
        css_num(spacing.table_cell_padding_x_em),
    )
}

fn color_vars(colors: &ThemeColors) -> String {
    format!(
        ":root {{\n  --fmd-fg: {};\n  --fmd-fg-muted: {};\n  --fmd-bg: {};\n  \
         --fmd-bg-subtle: {};\n  --fmd-border: {};\n  --fmd-border-muted: {};\n  \
         --fmd-code-bg: {};\n  --fmd-stripe: {};\n  --fmd-quote-fg: {};\n  \
         --fmd-quote-bar: {};\n  --fmd-accent: {};\n}}",
        css_token(&colors.fg),
        css_token(&colors.fg_muted),
        css_token(&colors.bg),
        css_token(&colors.bg_subtle),
        css_token(&colors.border),
        css_token(&colors.border_muted),
        css_token(&colors.code_bg),
        css_token(&colors.stripe),
        css_token(&colors.quote_fg),
        css_token(&colors.quote_bar),
        css_token(&colors.accent),
    )
}

fn dark_mode_css(colors: &ThemeColors) -> String {
    let vars = color_vars(colors);
    format!("\n@media (prefers-color-scheme: dark) {{\n  {vars}\n}}\n")
}

fn css_token(s: &str) -> String {
    let out: String = s
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '#' | '-' | '_' | ',' | '.' | '%' | '(' | ')' | ' ' | '/' | '"'
                )
        })
        .collect();
    if out.trim().is_empty() {
        "initial".to_string()
    } else {
        out
    }
}

fn css_num(value: f32) -> String {
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
    body_regular: BTreeSet<char>,
    body_bold: BTreeSet<char>,
    body_italic: BTreeSet<char>,
    body_bold_italic: BTreeSet<char>,
    mono: BTreeSet<char>,
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
    fn body_slot(&mut self, style: InlineStyle) -> &mut BTreeSet<char> {
        match (style.bold, style.italic) {
            (false, false) => &mut self.body_regular,
            (true, false) => &mut self.body_bold,
            (false, true) => &mut self.body_italic,
            (true, true) => &mut self.body_bold_italic,
        }
    }

    fn add_body_text(&mut self, text: &str, style: InlineStyle) {
        self.body_slot(style).extend(text.chars());
    }

    fn add_mono_text(&mut self, text: &str) {
        self.mono.extend(text.chars());
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

fn add_seed_if_used(chars: &mut BTreeSet<char>) {
    if chars.is_empty() {
        return;
    }
    chars.extend(HTML_FONT_SEED.chars());
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
    chars: &BTreeSet<char>,
) {
    if chars.is_empty() {
        return;
    }

    let keep: Vec<char> = chars.iter().copied().collect();
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
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied().unwrap_or(0);
        let b2 = bytes.get(i + 2).copied().unwrap_or(0);

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
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
    use super::{base64_encode, slug};

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
    fn slug_collapses_separator_runs_in_one_pass() {
        let mut heading = String::from("---Alpha");
        heading.push_str(&" _-".repeat(4096));
        heading.push_str("Beta---");

        assert_eq!(slug(&heading), "alpha-beta");
    }
}
