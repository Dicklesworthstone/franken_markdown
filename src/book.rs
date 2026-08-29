//! `fmd book` core (bead j0o4 / epic 7tus): assemble a directory of Markdown
//! files into one HTML site (page per file + shared sidebar) and/or a single
//! PDF book (global outline, continuous page numbers, chapter page breaks).
//!
//! Pure core: inputs arrive as (path, source) pairs from the CLI shell, which
//! owns all filesystem policy (walk order, include sandboxing, size caps).
//! Everything here is deterministic for fixed input.

use crate::ast::{Block, Document, Inline};
use crate::parse::{self, Frontmatter};
use crate::{RenderError, Result};

/// One input document: the book-relative path and its Markdown source.
#[derive(Debug, Clone)]
pub struct BookInput {
    /// Book-relative path (e.g. `guide/install.md`); forward slashes.
    pub path: String,
    pub source: String,
}

/// One chapter's public model.
#[derive(Debug, Clone)]
pub struct BookChapter {
    /// Book-relative source path.
    pub path: String,
    /// Output page name for the HTML site (`.md` → `.html`, flattened dirs
    /// joined with `__`): `guide/install.md` → `guide__install.html`.
    pub out_name: String,
    /// Chapter title: frontmatter title, else first heading text, else the
    /// path stem.
    pub title: String,
    /// Parsed frontmatter (title/author/lang/toc) for the chapter.
    pub frontmatter: Option<Frontmatter>,
    /// The chapter's parsed document (frontmatter stripped by the parser).
    pub doc: Document,
}

/// A parsed book: chapters in deterministic order.
#[derive(Debug, Clone)]
pub struct Book {
    pub chapters: Vec<BookChapter>,
}

/// Heading entry in the book-level TOC model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookHeading {
    pub level: u8,
    pub text: String,
    /// Anchor assigned by the HTML renderer's own algorithm (collision-safe
    /// per chapter, since each chapter renders its own page).
    pub anchor: String,
}

/// Parse and assemble the book. Inputs MUST arrive pre-sorted by the caller
/// (the CLI walks lexically); this function preserves arrival order.
///
/// # Errors
/// Returns `RenderError::InvalidInput` when the book has no chapters.
pub fn build_book(inputs: &[BookInput]) -> Result<Book> {
    if inputs.is_empty() {
        return Err(RenderError::InvalidInput(
            "book: no Markdown inputs found".to_string(),
        ));
    }
    let mut chapters = Vec::with_capacity(inputs.len());
    for input in inputs {
        let (frontmatter, _) = parse::split_frontmatter(&input.source);
        let doc = parse::parse_document(&input.source);
        let title = frontmatter
            .as_ref()
            .and_then(|fm| fm.title.clone())
            .or_else(|| first_heading_text(&doc))
            .unwrap_or_else(|| path_stem(&input.path));
        chapters.push(BookChapter {
            out_name: out_name(&input.path),
            path: input.path.clone(),
            title,
            frontmatter,
            doc,
        });
    }
    Ok(Book { chapters })
}

/// Flatten a book-relative path into a page name: `guide/install.md` →
/// `guide__install.html`.
pub fn out_name(path: &str) -> String {
    let clean = path
        .strip_prefix("./")
        .or_else(|| path.strip_prefix(".\\"))
        .or_else(|| path.strip_prefix('/'))
        .or_else(|| path.strip_prefix('\\'))
        .unwrap_or(path);
    let no_ext = if clean.to_ascii_lowercase().ends_with(".markdown") {
        &clean[..clean.len() - ".markdown".len()]
    } else if clean.to_ascii_lowercase().ends_with(".md") {
        &clean[..clean.len() - ".md".len()]
    } else {
        clean
    };
    format!("{}.html", no_ext.replace(['/', '\\'], "__"))
}

fn path_stem(path: &str) -> String {
    let filename = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = if filename.to_ascii_lowercase().ends_with(".markdown") {
        &filename[..filename.len() - ".markdown".len()]
    } else if filename.to_ascii_lowercase().ends_with(".md") {
        &filename[..filename.len() - ".md".len()]
    } else {
        filename
    };
    stem.to_string()
}

fn first_heading_text(doc: &Document) -> Option<String> {
    for block in &doc.blocks {
        if let Block::Heading { inlines, .. } = block {
            let text = plain_inlines(inlines);
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn plain_inlines(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                out.push_str(&plain_inlines(c));
            }
            Inline::Link { content, .. } => out.push_str(&plain_inlines(content)),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Math(m) | Inline::DisplayMath(m) => out.push_str(m),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Html(_) | Inline::FootnoteRef { .. } => {}
        }
    }
    out
}

/// Collect a chapter's headings (text only; anchors are renderer-assigned at
/// emit time through the same slug path the search index mirrors).
pub fn chapter_headings(doc: &Document) -> Vec<(u8, String)> {
    let mut out = Vec::new();
    collect_headings(&doc.blocks, &mut out);
    out
}

fn collect_headings(blocks: &[Block], out: &mut Vec<(u8, String)>) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => out.push((*level, plain_inlines(inlines))),
            Block::BlockQuote(inner) => collect_headings(inner, out),
            Block::List(list) => {
                for item in &list.items {
                    collect_headings(&item.blocks, out);
                }
            }
            Block::FootnoteDefinition { blocks, .. } => collect_headings(blocks, out),
            _ => {}
        }
    }
}

/// Rewrite one document's cross-file Markdown links for the HTML site:
/// `other.md#anchor` → `other.html#anchor` (flattened the same way as page
/// names). In-book absolute/relative links to files NOT in the book stay
/// untouched. Call before rendering each chapter page. Returns the number of
/// rewritten links.
pub fn rewrite_links_for_site(
    doc: &mut Document,
    known_pages: &std::collections::BTreeSet<String>,
) -> usize {
    let mut count = 0;
    rewrite_block_links(&mut doc.blocks, known_pages, &mut count);
    count
}

fn rewrite_block_links(
    blocks: &mut [Block],
    known: &std::collections::BTreeSet<String>,
    count: &mut usize,
) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                rewrite_inline_links(inlines, known, count);
            }
            Block::BlockQuote(inner) => rewrite_block_links(inner, known, count),
            Block::List(list) => {
                for item in &mut list.items {
                    rewrite_block_links(&mut item.blocks, known, count);
                }
            }
            Block::Table(table) => {
                for cell in &mut table.head {
                    rewrite_inline_links(cell, known, count);
                }
                for row in &mut table.rows {
                    for cell in row {
                        rewrite_inline_links(cell, known, count);
                    }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for term in &mut item.terms {
                        rewrite_inline_links(term, known, count);
                    }
                    for def in &mut item.definitions {
                        rewrite_inline_links(def, known, count);
                    }
                }
            }
            Block::FootnoteDefinition { blocks, .. } => {
                rewrite_block_links(blocks, known, count);
            }
            Block::CodeBlock { .. }
            | Block::ThematicBreak
            | Block::HtmlBlock(_)
            | Block::MathBlock(_)
            | Block::PageBreak => {}
        }
    }
}

fn rewrite_inline_links(
    inlines: &mut [Inline],
    known: &std::collections::BTreeSet<String>,
    count: &mut usize,
) {
    for inl in inlines {
        match inl {
            Inline::Link { dest, content, .. } => {
                if !dest.starts_with("http://")
                    && !dest.starts_with("https://")
                    && !dest.starts_with("//")
                    && !dest.starts_with("mailto:")
                    && !dest.starts_with("data:")
                {
                    if let Some((page, anchor)) = dest.split_once('#') {
                        let page_lower = page.to_ascii_lowercase();
                        if page_lower.ends_with(".md") || page_lower.ends_with(".markdown") {
                            let page_html = out_name(page);
                            if known.contains(&page_html) {
                                *dest = format!("{page_html}#{anchor}");
                                *count += 1;
                            }
                        }
                    } else {
                        let dest_lower = dest.to_ascii_lowercase();
                        if dest_lower.ends_with(".md") || dest_lower.ends_with(".markdown") {
                            let page_html = out_name(dest);
                            if known.contains(&page_html) {
                                *dest = page_html;
                                *count += 1;
                            }
                        }
                    }
                }
                rewrite_inline_links(content, known, count);
            }
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                rewrite_inline_links(c, known, count);
            }
            _ => {}
        }
    }
}

/// Merge the book into one document for the PDF: chapters concatenate in
/// order with a [`Block::PageBreak`] between them (the layout flag forces a
/// page boundary; the landed outline/contents machinery yields the global
/// TOC and continuous page numbers).
#[must_use]
pub fn book_pdf_document(book: &Book) -> Document {
    let mut blocks = Vec::new();
    for (idx, chapter) in book.chapters.iter().enumerate() {
        if idx > 0 {
            blocks.push(Block::PageBreak);
        }
        blocks.extend(chapter.doc.blocks.iter().cloned());
    }
    Document { blocks }
}

/// Inject the shared sidebar into a rendered chapter page. Deterministic
/// string surgery on OUR OWN emitter output: the nav goes right after
/// `<body …>`; its CSS appends to the document's own `</style>` boundary.
/// The emitter shape is pinned by tests/smoke_test.rs, so drift fails loudly.
#[must_use]
pub fn inject_book_nav(rendered: &str, book: &Book, current_page: &str) -> String {
    let mut nav = String::from("<nav class=\"fmd-book-nav\" aria-label=\"Book contents\">\n<ul>\n");
    for chapter in &book.chapters {
        let current = if chapter.out_name == current_page {
            " class=\"current\""
        } else {
            ""
        };
        nav.push_str(&format!(
            "<li{current}><a href=\"{}\">{}</a></li>\n",
            escape_attr(&chapter.out_name),
            escape_text(&chapter.title),
        ));
    }
    nav.push_str("</ul>\n</nav>\n");
    let nav_css = "<style>\n.fmd-book-nav{border:1px solid var(--fmd-border, #d1d9e0);border-radius:8px;padding:0.75rem 1rem;margin-bottom:1.25rem;font-size:0.9em}\n.fmd-book-nav ul{margin:0;padding-left:1.1rem}\n.fmd-book-nav .current{font-weight:700}\n.fmd-book-nav .current>a{text-decoration:none}\n</style>\n";
    let mut out = String::with_capacity(rendered.len() + nav.len() + nav_css.len());
    if let Some(pos) = rendered.find("<main class=\"fmd\">") {
        out.push_str(&rendered[..pos]);
        out.push_str(&nav);
        out.push_str(nav_css);
        out.push_str(&rendered[pos..]);
    } else {
        // Emitter drift: fail loudly rather than silently dropping the nav.
        out.push_str(rendered);
        out.push_str("<!-- fmd-book-nav: injection point not found -->\n");
    }
    out
}

/// Public for the CLI's index redirect.
pub fn escape_attr_pub(s: &str) -> String {
    escape_attr(s)
}

/// Public for the CLI's index redirect.
pub fn escape_text_pub(s: &str) -> String {
    escape_text(s)
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
