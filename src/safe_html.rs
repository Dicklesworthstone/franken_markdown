//! GitHub-style safe lowering of raw HTML into native Markdown nodes.
//!
//! READMEs routinely carry a little HTML: centered logo blocks, `<img>` with a
//! width, `<br>`, `<b>`/`<kbd>`, `<details>`/`<summary>`, comments that hold
//! lint directives. When raw HTML is not allowed through, fmd used to print
//! every such tag as literal text. This pass instead rewrites the HTML it
//! understands into ordinary AST nodes, the way GitHub's sanitizer keeps a
//! safe subset:
//!
//! - comments, processing instructions and declarations disappear,
//! - inline formatting tags become emphasis, strong, strikethrough or code,
//! - `<a href>` becomes a link and `<img src>` an image (both still pass the
//!   renderers' own safe-URL policies, so no new URL can become active),
//! - `<br>` is a hard break and `<hr>` a thematic break,
//! - block tags (`<p>`, `<div>`, `<h1>`..`<h6>`, `<pre>`, `<blockquote>`,
//!   `<ul>`/`<ol>`/`<li>`, `<table>`, `<details>`/`<summary>`) become
//!   paragraphs, headings, code blocks, quotes, lists and tables,
//! - presentational wrappers (`<span>`, `<sup>`, `<picture>`, ...) keep only
//!   their content, and attributes other than the few above are dropped.
//!
//! Any other tag (`<script>`, `<iframe>`, `<style>`, form controls, ...) stays
//! an [`Inline::Html`] node, which every renderer escapes into visible text,
//! exactly as before. The pass only ever *produces* native nodes, so it adds no
//! markup an escaping renderer would not already emit. It is pure, bounded by
//! the input size, and idempotent.

use std::borrow::Cow;

use crate::ast::{Align, Block, DefinitionItem, Document, Inline, List, ListItem, Table};

/// Lower every raw HTML node in `doc` the pass understands. Documents without
/// raw HTML are returned borrowed.
#[must_use]
pub fn lower(doc: &Document) -> Cow<'_, Document> {
    if !blocks_have_html(&doc.blocks) {
        return Cow::Borrowed(doc);
    }
    Cow::Owned(Document {
        blocks: lower_blocks(&doc.blocks),
    })
}

fn blocks_have_html(blocks: &[Block]) -> bool {
    blocks.iter().any(|block| match block {
        Block::HtmlBlock(_) => true,
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => inlines_have_html(inlines),
        Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
            blocks_have_html(inner)
        }
        Block::List(list) => list.items.iter().any(|item| blocks_have_html(&item.blocks)),
        Block::Table(table) => table
            .head
            .iter()
            .chain(table.rows.iter().flatten())
            .any(|cell| inlines_have_html(cell)),
        Block::DefinitionList(items) => items.iter().any(|item| {
            item.terms
                .iter()
                .chain(&item.definitions)
                .any(|inlines| inlines_have_html(inlines))
        }),
        Block::CodeBlock { .. } | Block::ThematicBreak | Block::MathBlock(_) | Block::PageBreak => {
            false
        }
    })
}

fn inlines_have_html(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| match inline {
        Inline::Html(_) => true,
        Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strikethrough(children) => {
            inlines_have_html(children)
        }
        Inline::Link { content, .. } => inlines_have_html(content),
        _ => false,
    })
}

fn lower_blocks(blocks: &[Block]) -> Vec<Block> {
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            Block::HtmlBlock(html) => out.extend(lower_html_block(html)),
            Block::Paragraph(inlines) => {
                let lowered = lower_inlines(inlines);
                if has_visible_content(&lowered) {
                    out.push(Block::Paragraph(trim_inlines(lowered)));
                }
            }
            Block::Heading { level, inlines } => out.push(Block::Heading {
                level: *level,
                inlines: trim_inlines(lower_inlines(inlines)),
            }),
            Block::BlockQuote(inner) => out.push(Block::BlockQuote(lower_blocks(inner))),
            Block::FootnoteDefinition { id, blocks } => out.push(Block::FootnoteDefinition {
                id: id.clone(),
                blocks: lower_blocks(blocks),
            }),
            Block::List(list) => out.push(Block::List(List {
                ordered: list.ordered,
                start: list.start,
                tight: list.tight,
                items: list
                    .items
                    .iter()
                    .map(|item| ListItem {
                        task: item.task,
                        blocks: lower_blocks(&item.blocks),
                    })
                    .collect(),
            })),
            Block::Table(table) => out.push(Block::Table(Table {
                align: table.align.clone(),
                head: table.head.iter().map(|cell| lower_inlines(cell)).collect(),
                rows: table
                    .rows
                    .iter()
                    .map(|row| row.iter().map(|cell| lower_inlines(cell)).collect())
                    .collect(),
            })),
            Block::DefinitionList(items) => out.push(Block::DefinitionList(
                items
                    .iter()
                    .map(|item| DefinitionItem {
                        terms: item.terms.iter().map(|t| lower_inlines(t)).collect(),
                        definitions: item.definitions.iter().map(|d| lower_inlines(d)).collect(),
                    })
                    .collect(),
            )),
            other => out.push(other.clone()),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// HTML tokenizing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Tag {
    /// ASCII-lowercased element name.
    name: String,
    closing: bool,
    self_closing: bool,
    attrs: Vec<(String, String)>,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    Tag(Tag, &'a str),
    /// Comment, declaration, CDATA or processing instruction: dropped.
    Ignorable,
}

/// Split raw HTML into text runs, tags and ignorable markup. A `<` that does
/// not start well-formed markup is ordinary text.
fn pieces(html: &str) -> Vec<Piece<'_>> {
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut text_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let Some((piece, len)) = markup_at(&html[i..]) else {
            i += 1;
            continue;
        };
        if text_start < i {
            out.push(Piece::Text(&html[text_start..i]));
        }
        out.push(piece);
        i += len;
        text_start = i;
    }
    if text_start < bytes.len() {
        out.push(Piece::Text(&html[text_start..]));
    }
    out
}

fn markup_at(s: &str) -> Option<(Piece<'_>, usize)> {
    if let Some(rest) = s.strip_prefix("<!--") {
        let end = rest.find("-->")?;
        return Some((Piece::Ignorable, 4 + end + 3));
    }
    if let Some(rest) = s.strip_prefix("<![CDATA[") {
        let end = rest.find("]]>")?;
        return Some((Piece::Ignorable, 9 + end + 3));
    }
    if s.starts_with("<!") || s.starts_with("<?") {
        let end = s.find('>')?;
        return Some((Piece::Ignorable, end + 1));
    }
    let (tag, len) = parse_tag(s)?;
    Some((Piece::Tag(tag, &s[..len]), len))
}

fn parse_tag(s: &str) -> Option<(Tag, usize)> {
    let bytes = s.as_bytes();
    let mut i = 1usize;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let name_start = i;
    if !bytes.get(i)?.is_ascii_alphabetic() {
        return None;
    }
    while bytes
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
    {
        i += 1;
    }
    let name = s[name_start..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        let before_space = i;
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        match bytes.get(i)? {
            b'>' => return Some((tag(name, closing, self_closing, attrs), i + 1)),
            b'/' if bytes.get(i + 1) == Some(&b'>') && !closing => {
                self_closing = true;
                return Some((tag(name, closing, self_closing, attrs), i + 2));
            }
            _ if closing => return None,
            // Attributes must be separated from the name and each other.
            _ if i == before_space => return None,
            _ => {}
        }
        let attr_start = i;
        if !bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b == b':')
        {
            return None;
        }
        while bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
        {
            i += 1;
        }
        let attr_name = s[attr_start..i].to_ascii_lowercase();
        let mut j = i;
        while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        let mut value = String::new();
        if bytes.get(j) == Some(&b'=') {
            j += 1;
            while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
                j += 1;
            }
            match *bytes.get(j)? {
                quote @ (b'"' | b'\'') => {
                    let close = s[j + 1..].find(char::from(quote))?;
                    value = decode_entities(&s[j + 1..j + 1 + close]);
                    j += close + 2;
                }
                _ => {
                    let start = j;
                    while bytes.get(j).is_some_and(|b| {
                        !b.is_ascii_whitespace()
                            && !matches!(b, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
                    }) {
                        j += 1;
                    }
                    if start == j {
                        return None;
                    }
                    value = decode_entities(&s[start..j]);
                }
            }
            i = j;
        }
        attrs.push((attr_name, value));
    }
}

fn tag(name: String, closing: bool, self_closing: bool, attrs: Vec<(String, String)>) -> Tag {
    Tag {
        name,
        closing,
        self_closing,
        attrs,
    }
}

/// Decode HTML character references (named, decimal and hexadecimal).
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let decoded = after
            .find(';')
            .filter(|&semi| semi > 0 && semi <= 32)
            .and_then(|semi| {
                let body = &after[..semi];
                let value = if let Some(hex) =
                    body.strip_prefix("#x").or_else(|| body.strip_prefix("#X"))
                {
                    numeric_reference(hex, 16)
                } else if let Some(dec) = body.strip_prefix('#') {
                    numeric_reference(dec, 10)
                } else {
                    crate::parse::entities::lookup(body).map(str::to_string)
                };
                value.map(|value| (value, semi + 1))
            });
        match decoded {
            Some((value, consumed)) => {
                out.push_str(&value);
                rest = &after[consumed..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn numeric_reference(digits: &str, radix: u32) -> Option<String> {
    if digits.is_empty() || digits.len() > 8 {
        return None;
    }
    let value = u32::from_str_radix(digits, radix).ok()?;
    let ch = if value == 0 {
        '\u{FFFD}'
    } else {
        char::from_u32(value).unwrap_or('\u{FFFD}')
    };
    Some(ch.to_string())
}

// ---------------------------------------------------------------------------
// Inline lowering
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineRole {
    Strong,
    Emphasis,
    Strike,
    Code,
    Link,
    Quote,
    /// Keep the content, drop the tag.
    Transparent,
}

fn inline_role(name: &str) -> Option<InlineRole> {
    Some(match name {
        "b" | "strong" => InlineRole::Strong,
        "i" | "em" | "cite" | "var" | "dfn" => InlineRole::Emphasis,
        "s" | "del" | "strike" => InlineRole::Strike,
        "code" | "kbd" | "samp" | "tt" => InlineRole::Code,
        "a" => InlineRole::Link,
        "q" => InlineRole::Quote,
        "span" | "sup" | "sub" | "small" | "big" | "u" | "ins" | "mark" | "abbr" | "font"
        | "time" | "picture" | "nobr" | "bdi" | "bdo" | "data" | "label" | "center" | "p"
        | "div" | "summary" | "details" | "figure" | "figcaption" => InlineRole::Transparent,
        _ => return None,
    })
}

/// Void elements and harmless elements whose tag is simply dropped.
fn is_dropped_inline_tag(name: &str) -> bool {
    matches!(name, "wbr" | "source" | "track" | "col" | "colgroup")
}

struct Frame {
    name: String,
    role: InlineRole,
    link: Option<(String, Option<String>)>,
    children: Vec<Inline>,
}

struct InlineBuilder {
    root: Vec<Inline>,
    stack: Vec<Frame>,
}

impl InlineBuilder {
    fn new() -> Self {
        Self {
            root: Vec::new(),
            stack: Vec::new(),
        }
    }

    fn current(&mut self) -> &mut Vec<Inline> {
        match self.stack.last_mut() {
            Some(frame) => &mut frame.children,
            None => &mut self.root,
        }
    }

    fn push(&mut self, inline: Inline) {
        let current = self.current();
        if let (Inline::Text(text), Some(Inline::Text(last))) = (&inline, current.last_mut()) {
            last.push_str(text);
            return;
        }
        current.push(inline);
    }

    fn extend(&mut self, inlines: Vec<Inline>) {
        for inline in inlines {
            self.push(inline);
        }
    }

    /// Feed one raw-HTML inline node (normally a single tag).
    fn html(&mut self, raw: &str) {
        for piece in pieces(raw) {
            match piece {
                Piece::Ignorable => {}
                Piece::Text(text) => self.push(Inline::Text(decode_entities(text))),
                Piece::Tag(tag, source) => self.tag(&tag, source),
            }
        }
    }

    fn tag(&mut self, tag: &Tag, source: &str) {
        match tag.name.as_str() {
            "br" => {
                if !tag.closing {
                    self.push(Inline::HardBreak);
                }
                return;
            }
            "img" => {
                if !tag.closing {
                    if let Some(image) = image_from_tag(tag) {
                        self.push(image);
                    }
                }
                return;
            }
            "hr" => return,
            name if is_dropped_inline_tag(name) => return,
            _ => {}
        }
        let Some(role) = inline_role(&tag.name) else {
            // Unknown or unsafe: stays raw, so renderers escape it visibly.
            self.push(Inline::Html(source.to_string()));
            return;
        };
        if tag.closing {
            self.close(&tag.name);
        } else if !tag.self_closing {
            let link = if role == InlineRole::Link {
                tag.attr("href").map(|href| {
                    (
                        href.trim().to_string(),
                        tag.attr("title").map(str::to_string),
                    )
                })
            } else {
                None
            };
            self.stack.push(Frame {
                name: tag.name.clone(),
                role,
                link,
                children: Vec::new(),
            });
        }
    }

    fn close(&mut self, name: &str) {
        let Some(pos) = self.stack.iter().rposition(|frame| frame.name == name) else {
            return; // A stray close tag has nothing to end.
        };
        while self.stack.len() > pos {
            self.finish_top();
        }
    }

    fn finish_top(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let lowered = finish_frame(frame);
        self.extend(lowered);
    }

    fn finish(mut self) -> Vec<Inline> {
        while !self.stack.is_empty() {
            self.finish_top();
        }
        self.root
    }
}

fn finish_frame(frame: Frame) -> Vec<Inline> {
    let children = frame.children;
    if children.is_empty() {
        return Vec::new();
    }
    match frame.role {
        InlineRole::Strong => vec![Inline::Strong(children)],
        InlineRole::Emphasis => vec![Inline::Emphasis(children)],
        InlineRole::Strike => vec![Inline::Strikethrough(children)],
        InlineRole::Code => {
            let mut text = String::new();
            plain_text(&children, &mut text);
            vec![Inline::Code(text)]
        }
        InlineRole::Link => match frame.link {
            Some((dest, title)) if !dest.is_empty() => vec![Inline::Link {
                dest,
                title,
                content: children,
            }],
            _ => children,
        },
        InlineRole::Quote => {
            let mut out = Vec::with_capacity(children.len() + 2);
            out.push(Inline::Text("\u{201c}".to_string()));
            out.extend(children);
            out.push(Inline::Text("\u{201d}".to_string()));
            out
        }
        InlineRole::Transparent => children,
    }
}

fn image_from_tag(tag: &Tag) -> Option<Inline> {
    let src = tag.attr("src")?.trim();
    if src.is_empty() {
        return None;
    }
    Some(Inline::Image {
        dest: src.to_string(),
        title: tag.attr("title").map(str::to_string),
        alt: tag.attr("alt").unwrap_or_default().to_string(),
    })
}

fn plain_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) | Inline::Code(text) | Inline::Math(text) => out.push_str(text),
            Inline::DisplayMath(text) | Inline::Html(text) => out.push_str(text),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => plain_text(children, out),
            Inline::Link { content, .. } => plain_text(content, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::FootnoteRef { id } => {
                out.push_str("[^");
                out.push_str(id);
                out.push(']');
            }
        }
    }
}

fn lower_inlines(inlines: &[Inline]) -> Vec<Inline> {
    let mut builder = InlineBuilder::new();
    feed_inlines(&mut builder, inlines);
    builder.finish()
}

fn feed_inlines(builder: &mut InlineBuilder, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Html(raw) => builder.html(raw),
            Inline::Emphasis(children) => builder.push(Inline::Emphasis(lower_inlines(children))),
            Inline::Strong(children) => builder.push(Inline::Strong(lower_inlines(children))),
            Inline::Strikethrough(children) => {
                builder.push(Inline::Strikethrough(lower_inlines(children)));
            }
            Inline::Link {
                dest,
                title,
                content,
            } => builder.push(Inline::Link {
                dest: dest.clone(),
                title: title.clone(),
                content: lower_inlines(content),
            }),
            other => builder.push(other.clone()),
        }
    }
}

fn is_blank_inline(inline: &Inline) -> bool {
    match inline {
        Inline::Text(text) => text.trim().is_empty(),
        Inline::SoftBreak | Inline::HardBreak => true,
        _ => false,
    }
}

fn has_visible_content(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| !is_blank_inline(inline))
}

/// Drop leading/trailing whitespace and breaks a removed tag or comment left
/// at a paragraph edge.
fn trim_inlines(mut inlines: Vec<Inline>) -> Vec<Inline> {
    while inlines.first().is_some_and(is_blank_inline) {
        inlines.remove(0);
    }
    while inlines.last().is_some_and(is_blank_inline) {
        inlines.pop();
    }
    if let Some(Inline::Text(text)) = inlines.first_mut() {
        let trimmed = text.trim_start().len();
        text.drain(..text.len() - trimmed);
    }
    if let Some(Inline::Text(text)) = inlines.last_mut() {
        text.truncate(text.trim_end().len());
    }
    inlines
}

// ---------------------------------------------------------------------------
// Block lowering
// ---------------------------------------------------------------------------

enum Container {
    Root(Vec<Block>),
    Quote(Vec<Block>),
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
    },
    Item(Vec<Block>),
    Table {
        before: Vec<Block>,
        rows: Vec<Vec<Vec<Inline>>>,
        row: Option<Vec<Vec<Inline>>>,
    },
    Cell(Vec<Inline>),
}

impl Container {
    fn tag_name(&self) -> &'static str {
        match self {
            Self::Root(_) => "",
            Self::Quote(_) => "blockquote",
            Self::List { ordered: true, .. } => "ol",
            Self::List { ordered: false, .. } => "ul",
            Self::Item(_) => "li",
            Self::Table { .. } => "table",
            Self::Cell(_) => "td",
        }
    }
}

struct BlockBuilder {
    stack: Vec<Container>,
    /// Raw inline nodes of the paragraph being collected.
    para: Vec<Inline>,
    heading: Option<u8>,
    summary: bool,
    pre: Option<String>,
}

impl BlockBuilder {
    fn new() -> Self {
        Self {
            stack: vec![Container::Root(Vec::new())],
            para: Vec::new(),
            heading: None,
            summary: false,
            pre: None,
        }
    }

    fn push_block(&mut self, block: Block) {
        match self.stack.last_mut() {
            Some(Container::Root(blocks) | Container::Quote(blocks) | Container::Item(blocks)) => {
                blocks.push(block);
            }
            Some(Container::Cell(cell)) => {
                // Cells are inline-only: separate block content with a break.
                let inlines = match block {
                    Block::Paragraph(inlines) | Block::Heading { inlines, .. } => inlines,
                    other => {
                        let mut text = String::new();
                        block_text(&other, &mut text);
                        vec![Inline::Text(text)]
                    }
                };
                if !cell.is_empty() {
                    cell.push(Inline::HardBreak);
                }
                cell.extend(inlines);
            }
            Some(Container::Table { before, .. }) => before.push(block),
            Some(Container::List { items, .. }) => items.push(ListItem {
                task: None,
                blocks: vec![block],
            }),
            None => {}
        }
    }

    fn flush(&mut self) {
        let raw = std::mem::take(&mut self.para);
        let heading = self.heading.take();
        let summary = std::mem::replace(&mut self.summary, false);
        let inlines = trim_inlines(lower_inlines(&raw));
        if !has_visible_content(&inlines) {
            return;
        }
        if let Some(Container::Cell(cell)) = self.stack.last_mut() {
            if !cell.is_empty() {
                cell.push(Inline::Text(" ".to_string()));
            }
            cell.extend(inlines);
            return;
        }
        let block = match heading {
            Some(level) => Block::Heading { level, inlines },
            None if summary => Block::Paragraph(vec![Inline::Strong(inlines)]),
            None => Block::Paragraph(inlines),
        };
        self.push_block(block);
    }

    fn text(&mut self, text: &str) {
        if let Some(pre) = &mut self.pre {
            pre.push_str(&decode_entities(text));
            return;
        }
        // HTML collapses whitespace runs, including newlines, to one space.
        let mut collapsed = String::with_capacity(text.len());
        let mut in_space = false;
        for ch in decode_entities(text).chars() {
            if ch.is_ascii_whitespace() {
                if !in_space {
                    collapsed.push(' ');
                }
                in_space = true;
            } else {
                collapsed.push(ch);
                in_space = false;
            }
        }
        if collapsed.trim().is_empty() && self.para.is_empty() {
            return;
        }
        if let Some(Inline::Text(last)) = self.para.last_mut() {
            last.push_str(&collapsed);
        } else {
            self.para.push(Inline::Text(collapsed));
        }
    }

    fn tag(&mut self, tag: &Tag, source: &str) {
        let name = tag.name.as_str();
        if self.pre.is_some() {
            if name == "pre" && tag.closing {
                let code = self.pre.take().unwrap_or_default();
                let code = code.strip_prefix('\n').unwrap_or(&code).to_string();
                self.push_block(Block::CodeBlock { lang: None, code });
            }
            return;
        }
        match name {
            "p" | "div" | "center" | "section" | "article" | "header" | "footer" | "main"
            | "nav" | "aside" | "figure" | "figcaption" | "address" | "hgroup" | "details"
            | "caption" | "thead" | "tbody" | "tfoot" | "dl" | "dt" | "dd" => self.flush(),
            "summary" => {
                self.flush();
                self.summary = !tag.closing;
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                if !tag.closing {
                    self.heading = name[1..].parse().ok();
                }
            }
            "hr" => {
                self.flush();
                self.push_block(Block::ThematicBreak);
            }
            "pre" => {
                if !tag.closing {
                    self.flush();
                    self.pre = Some(String::new());
                }
            }
            "blockquote" => {
                self.flush();
                if tag.closing {
                    self.close_container("blockquote");
                } else {
                    self.stack.push(Container::Quote(Vec::new()));
                }
            }
            "ul" | "ol" => {
                self.flush();
                if tag.closing {
                    self.close_container(if name == "ol" { "ol" } else { "ul" });
                } else {
                    self.stack.push(Container::List {
                        ordered: name == "ol",
                        start: tag
                            .attr("start")
                            .and_then(|start| start.trim().parse().ok())
                            .unwrap_or(1),
                        items: Vec::new(),
                    });
                }
            }
            "li" => {
                self.flush();
                if matches!(self.stack.last(), Some(Container::Item(_))) {
                    self.pop_container();
                }
                if !tag.closing && matches!(self.stack.last(), Some(Container::List { .. })) {
                    self.stack.push(Container::Item(Vec::new()));
                }
            }
            "table" => {
                self.flush();
                if tag.closing {
                    self.close_container("table");
                } else {
                    self.stack.push(Container::Table {
                        before: Vec::new(),
                        rows: Vec::new(),
                        row: None,
                    });
                }
            }
            "tr" => {
                self.flush();
                if matches!(self.stack.last(), Some(Container::Cell(_))) {
                    self.pop_container();
                }
                if let Some(Container::Table { rows, row, .. }) = self.stack.last_mut() {
                    if let Some(done) = row.take() {
                        rows.push(done);
                    }
                    if !tag.closing {
                        *row = Some(Vec::new());
                    }
                }
            }
            "td" | "th" => {
                self.flush();
                if matches!(self.stack.last(), Some(Container::Cell(_))) {
                    self.pop_container();
                }
                if !tag.closing && matches!(self.stack.last(), Some(Container::Table { .. })) {
                    self.stack.push(Container::Cell(Vec::new()));
                }
            }
            _ => self.para.push(Inline::Html(source.to_string())),
        }
    }

    fn close_container(&mut self, name: &str) {
        let Some(pos) = self
            .stack
            .iter()
            .rposition(|container| container.tag_name() == name)
        else {
            return;
        };
        while self.stack.len() > pos {
            self.pop_container();
        }
    }

    fn pop_container(&mut self) {
        if self.stack.len() <= 1 {
            return;
        }
        let Some(container) = self.stack.pop() else {
            return;
        };
        match container {
            Container::Root(_) => {}
            Container::Quote(blocks) => {
                if !blocks.is_empty() {
                    self.push_block(Block::BlockQuote(blocks));
                }
            }
            Container::Item(blocks) => {
                if let Some(Container::List { items, .. }) = self.stack.last_mut() {
                    items.push(ListItem { task: None, blocks });
                } else {
                    for block in blocks {
                        self.push_block(block);
                    }
                }
            }
            Container::List {
                ordered,
                start,
                items,
            } => {
                if !items.is_empty() {
                    self.push_block(Block::List(List {
                        ordered,
                        start,
                        tight: true,
                        items,
                    }));
                }
            }
            Container::Cell(inlines) => {
                if let Some(Container::Table { row, .. }) = self.stack.last_mut() {
                    row.get_or_insert_with(Vec::new).push(inlines);
                }
            }
            Container::Table {
                before,
                mut rows,
                row,
            } => {
                for block in before {
                    self.push_block(block);
                }
                if let Some(row) = row {
                    rows.push(row);
                }
                rows.retain(|row| !row.is_empty());
                if let Some(table) = table_from_rows(rows) {
                    self.push_block(Block::Table(table));
                }
            }
        }
    }

    fn finish(mut self) -> Vec<Block> {
        if let Some(code) = self.pre.take() {
            self.push_block(Block::CodeBlock { lang: None, code });
        }
        self.flush();
        while self.stack.len() > 1 {
            self.pop_container();
        }
        match self.stack.pop() {
            Some(Container::Root(blocks)) => blocks,
            _ => Vec::new(),
        }
    }
}

fn table_from_rows(mut rows: Vec<Vec<Vec<Inline>>>) -> Option<Table> {
    let columns = rows.iter().map(Vec::len).max()?;
    if columns == 0 {
        return None;
    }
    for row in &mut rows {
        row.resize_with(columns, Vec::new);
    }
    let mut rows = rows.into_iter();
    let head = rows.next()?;
    Some(Table {
        align: vec![Align::None; columns],
        head,
        rows: rows.collect(),
    })
}

fn block_text(block: &Block, out: &mut String) {
    match block {
        Block::Paragraph(inlines) | Block::Heading { inlines, .. } => plain_text(inlines, out),
        Block::CodeBlock { code, .. } => out.push_str(code),
        Block::BlockQuote(blocks) => {
            for block in blocks {
                block_text(block, out);
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for block in &item.blocks {
                    block_text(block, out);
                }
            }
        }
        _ => {}
    }
}

fn lower_html_block(html: &str) -> Vec<Block> {
    let mut builder = BlockBuilder::new();
    for piece in pieces(html) {
        match piece {
            Piece::Ignorable => {}
            Piece::Text(text) => builder.text(text),
            Piece::Tag(tag, source) => builder.tag(&tag, source),
        }
    }
    builder.finish()
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    fn lowered(markdown: &str) -> Vec<Block> {
        lower(&crate::parse_markdown(markdown)).into_owned().blocks
    }

    fn text(value: &str) -> Inline {
        Inline::Text(value.to_string())
    }

    #[test]
    fn documents_without_html_are_borrowed() {
        let doc = crate::parse_markdown("# Plain\n\nNo *markup* here.\n");
        assert!(matches!(lower(&doc), Cow::Borrowed(_)));
    }

    #[test]
    fn comments_vanish_inline_and_as_blocks() {
        assert_eq!(
            lowered("a <!-- hidden --> b\n\n<!-- markdownlint-disable -->\n\nc\n"),
            vec![
                Block::Paragraph(vec![text("a  b")]),
                Block::Paragraph(vec![text("c")]),
            ]
        );
    }

    #[test]
    fn inline_formatting_tags_become_native_nodes() {
        assert_eq!(
            lowered("x <b>bold</b> <i>it</i> <kbd>Ctrl</kbd> <del>gone</del><br>y\n"),
            vec![Block::Paragraph(vec![
                text("x "),
                Inline::Strong(vec![text("bold")]),
                text(" "),
                Inline::Emphasis(vec![text("it")]),
                text(" "),
                Inline::Code("Ctrl".to_string()),
                text(" "),
                Inline::Strikethrough(vec![text("gone")]),
                Inline::HardBreak,
                text("y"),
            ])]
        );
    }

    #[test]
    fn anchors_and_images_keep_only_safe_attributes() {
        assert_eq!(
            lowered(
                "<a href=\"https://e.x/a?b=1&amp;c=2\" onclick=\"evil()\">go</a> \
                 <img src=\"logo.png\" alt=\"Logo\" width=\"100\" onerror=\"x()\">\n"
            ),
            vec![Block::Paragraph(vec![
                Inline::Link {
                    dest: "https://e.x/a?b=1&c=2".to_string(),
                    title: None,
                    content: vec![text("go")],
                },
                text(" "),
                Inline::Image {
                    dest: "logo.png".to_string(),
                    title: None,
                    alt: "Logo".to_string(),
                },
            ])]
        );
    }

    #[test]
    fn unsafe_and_unknown_tags_stay_raw_for_escaping() {
        let blocks = lowered("A <script>alert(1)</script> b\n");
        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![
                text("A "),
                Inline::Html("<script>".to_string()),
                text("alert(1)"),
                Inline::Html("</script>".to_string()),
                text(" b"),
            ])]
        );
    }

    #[test]
    fn centered_readme_header_lowers_to_heading_image_and_text() {
        let blocks = lowered(
            "<div align=\"center\">\n\n# Title\n\n<img src=\"logo.png\" alt=\"Logo\">\n\n</div>\n\n\
             <p align=\"center\">\n  <img src=\"a.png\" width=\"40\">\n  <br>Made with &hearts;\n</p>\n",
        );
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![text("Title")],
                },
                Block::Paragraph(vec![Inline::Image {
                    dest: "logo.png".to_string(),
                    title: None,
                    alt: "Logo".to_string(),
                }]),
                Block::Paragraph(vec![
                    Inline::Image {
                        dest: "a.png".to_string(),
                        title: None,
                        alt: String::new(),
                    },
                    text(" "),
                    Inline::HardBreak,
                    text("Made with \u{2665}"),
                ]),
            ]
        );
    }

    #[test]
    fn details_summary_headings_pre_and_rules() {
        let blocks = lowered(
            "<details>\n<summary>More &amp; less</summary>\n\nHidden\n\n</details>\n\n\
             <h2>Sub <em>title</em></h2>\n<hr>\n<pre>\nfn main() {\n  1 &lt; 2\n}\n</pre>\n",
        );
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph(vec![Inline::Strong(vec![text("More & less")])]),
                Block::Paragraph(vec![text("Hidden")]),
                Block::Heading {
                    level: 2,
                    inlines: vec![text("Sub "), Inline::Emphasis(vec![text("title")])],
                },
                Block::ThematicBreak,
                Block::CodeBlock {
                    lang: None,
                    code: "fn main() {\n  1 < 2\n}\n".to_string(),
                },
            ]
        );
    }

    #[test]
    fn html_lists_quotes_and_tables_become_markdown_structures() {
        let blocks = lowered(
            "<ul>\n<li>one</li>\n<li><b>two</b></li>\n</ul>\n\n\
             <blockquote>quoted</blockquote>\n\n\
             <table>\n<tr><th>A</th><th>B</th></tr>\n<tr><td>1</td><td><a href=\"#x\">2</a></td></tr>\n</table>\n",
        );
        assert_eq!(
            blocks,
            vec![
                Block::List(List {
                    ordered: false,
                    start: 1,
                    tight: true,
                    items: vec![
                        ListItem {
                            task: None,
                            blocks: vec![Block::Paragraph(vec![text("one")])],
                        },
                        ListItem {
                            task: None,
                            blocks: vec![Block::Paragraph(vec![Inline::Strong(vec![text("two")])])],
                        },
                    ],
                }),
                Block::BlockQuote(vec![Block::Paragraph(vec![text("quoted")])]),
                Block::Table(Table {
                    align: vec![Align::None, Align::None],
                    head: vec![vec![text("A")], vec![text("B")]],
                    rows: vec![vec![
                        vec![text("1")],
                        vec![Inline::Link {
                            dest: "#x".to_string(),
                            title: None,
                            content: vec![text("2")],
                        }],
                    ]],
                }),
            ]
        );
    }

    #[test]
    fn unclosed_and_stray_tags_never_lose_text() {
        assert_eq!(
            lowered("<b>open and </i>stray\n"),
            vec![Block::Paragraph(vec![Inline::Strong(vec![text(
                "open and stray"
            )])])]
        );
        assert_eq!(
            lowered("a < b and c<d\n"),
            vec![Block::Paragraph(vec![text("a < b and c<d")])]
        );
    }

    #[test]
    fn lowering_is_idempotent() {
        let doc = crate::parse_markdown(
            "<p>x <b>y</b> <script>z</script></p>\n\n<!-- c -->\n\nq <span>r</span>\n",
        );
        let once = lower(&doc).into_owned();
        let twice = lower(&once).into_owned();
        assert_eq!(once, twice);
    }

    #[test]
    fn entities_decode_named_and_numeric_references() {
        assert_eq!(
            decode_entities("&lt;&#65;&#x42;&copy;&bogus; &"),
            "<AB\u{a9}&bogus; &"
        );
    }
}
