//! All-in-one HTML emitter: turns the AST into a single self-contained `.html`
//! document with the default theme stylesheet inlined. The default styling is
//! tuned to look like a high-quality Markdown preview (Cursor/GitHub-grade):
//! readable measure and leading, gorgeous tables with subtle striping, elegant
//! blockquotes, and code blocks ready for syntax highlighting.

use crate::HtmlOptions;
use crate::ast::{Align, Block, Document, Inline, List};
use crate::theme::{DarkModePolicy, Theme, ThemeColors};

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
        .unwrap_or_else(|| default_css(&opts.theme));

    let mut body = String::new();
    render_blocks(&doc.blocks, &mut body, opts);

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

fn render_blocks(blocks: &[Block], out: &mut String, opts: &HtmlOptions) {
    for block in blocks {
        render_block(block, out, opts);
    }
}

fn render_block(block: &Block, out: &mut String, opts: &HtmlOptions) {
    match block {
        Block::Heading { level, inlines } => {
            let id = slug(&inlines_to_plain(inlines));
            out.push_str(&format!("<h{level} id=\"{id}\">"));
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
            render_blocks(inner, out, opts);
            out.push_str("</blockquote>\n");
        }
        Block::List(list) => render_list(list, out, opts),
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

fn render_list(list: &List, out: &mut String, opts: &HtmlOptions) {
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
        // Tight lists: render a single paragraph item without the <p> wrapper.
        if list.tight && item.blocks.len() == 1 {
            if let Some(Block::Paragraph(inlines)) = item.blocks.first() {
                render_inlines(inlines, out, opts);
                out.push_str("</li>\n");
                continue;
            }
        }
        render_blocks(&item.blocks, out, opts);
        out.push_str("</li>\n");
    }
    out.push_str(&format!("</{tag}>\n"));
}

fn render_table(table: &crate::ast::Table, out: &mut String, opts: &HtmlOptions) {
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
                let t = title
                    .as_deref()
                    .map(|s| format!(" title=\"{}\"", escape_attr(s)))
                    .unwrap_or_default();
                out.push_str(&format!("<a href=\"{}\"{t}>", escape_attr(dest)));
                render_inlines(content, out, opts);
                out.push_str("</a>");
            }
            Inline::Image { dest, title, alt } => {
                let t = title
                    .as_deref()
                    .map(|s| format!(" title=\"{}\"", escape_attr(s)))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\"{t}>",
                    escape_attr(dest),
                    escape_attr(alt)
                ));
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
            Inline::Html(_) => {}
        }
    }
    s
}

fn slug(text: &str) -> String {
    let mut s = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            s.push('-');
        }
    }
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').to_string()
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

/// The default, dependency-free, gorgeous stylesheet.
fn default_css(theme: &Theme) -> String {
    let body_font = theme.body_font_stack();
    let mono_font = theme.mono_font_stack();
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
        "{}\n\
:root {{ --fmd-base: {}px; --fmd-measure: {}px; --fmd-line-height: {}; \
--fmd-radius: {}px; --fmd-table-pad-y: {}em; --fmd-table-pad-x: {}em; \
--fmd-font-body: {body_font}; --fmd-font-mono: {mono_font}; }}\n\
{BASE_CSS}\n{TOKEN_CSS}\n{dark}{token_dark}",
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
  letter-spacing: -0.01em;
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
  line-height: 1.55;
}
pre code { background: none; padding: 0; font-size: 0.86em; }

hr { height: 1px; border: 0; background: var(--fmd-border); margin: 2.4em 0; }

img { max-width: 100%; border-radius: var(--fmd-radius); }

table {
  border-collapse: collapse;
  width: 100%;
  margin: 0 0 1.4em;
  font-size: 0.95em;
  overflow: hidden;
  border-radius: calc(var(--fmd-radius) + 2px);
  border: 1px solid var(--fmd-border);
}
thead th {
  background: var(--fmd-bg-subtle);
  font-weight: 650;
  text-align: left;
}
th, td { padding: var(--fmd-table-pad-y) var(--fmd-table-pad-x); border-bottom: 1px solid var(--fmd-border-muted); }
tbody tr:nth-child(even) { background: var(--fmd-stripe); }
tbody tr:last-child td { border-bottom: 0; }

del { color: var(--fmd-fg-muted); }
strong { font-weight: 680; }
"#;
