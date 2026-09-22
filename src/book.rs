//! `fmd book` core (bead j0o4 / epic 7tus): assemble a directory of Markdown
//! files into one HTML site (page per file + shared sidebar) and/or a single
//! PDF book (global outline, continuous page numbers, chapter page breaks).
//!
//! [`BookRenderer`] additionally provides reusable PDF, EPUB, and HTML-site
//! ZIP exports with chapter-aware host assets. The optional browser adapter
//! exposes the same cached parsed book through the `FmdBook` class.
//!
//! Pure core: inputs arrive as (path, source) pairs from the host, which owns
//! filesystem policy and include expansion. Everything here is deterministic
//! for fixed input. No renderer function fetches URLs or reads files.

use crate::ast::{Block, Document, Inline};
use crate::parse::{self, Frontmatter};
use crate::{RenderError, Result};

#[path = "book/merge.rs"]
mod merge;
#[path = "book/paths.rs"]
mod paths;
#[path = "book/render.rs"]
mod render;
#[path = "book/workspace.rs"]
mod workspace;
pub use workspace::{BookSourceUpdate, BookWorkspace};

#[path = "book/validation.rs"]
pub mod validation;

pub use render::{
    BookRenderer, book_pdf_document_with_assets, render_book_pdf, render_book_site,
};

#[cfg(feature = "wasm-bindgen")]
#[path = "book/browser.rs"]
pub mod browser;

/// Native filesystem shell and executable dispatcher. Never compiled into
/// the no-default-features core or browser-only builds.
#[cfg(feature = "cli")]
#[path = "book/native.rs"]
pub mod native;

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
    /// Normalized book-relative source path.
    pub path: String,
    /// Portable output page name. Ordinary paths retain the historical
    /// flattening (`guide/install.md` becomes `guide__install.html`). URL
    /// punctuation is escaped and `index.html` is reserved for the landing page.
    pub out_name: String,
    /// Chapter title: frontmatter title, else first heading text, else the
    /// path stem.
    pub title: String,
    /// Parsed frontmatter (title/author/lang/toc) for the chapter.
    pub frontmatter: Option<Frontmatter>,
    /// Parsed document, with frontmatter removed. Links to known chapters are
    /// canonical book-root Markdown URLs, independent of the eventual format.
    /// Image destinations, unknown files, and external links remain unchanged.
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
/// Source paths are normalized before parsing. Known chapter links are
/// resolved relative to each source file, then represented as root-relative
/// Markdown URLs. Existing HTML and EPUB consumers can therefore resolve
/// nested links without guessing which directory a document came from.
///
/// # Errors
/// Returns `RenderError::InvalidInput` for an empty book, invalid or escaping
/// paths, duplicate normalized paths, or colliding output filenames. Output
/// collisions are checked case-insensitively so publication is portable.
pub fn build_book(inputs: &[BookInput]) -> Result<Book> {
    if inputs.is_empty() {
        return Err(RenderError::InvalidInput(
            "book: no Markdown inputs found".to_string(),
        ));
    }
    let paths = paths::input_paths(inputs)?;
    let mut chapters = Vec::with_capacity(inputs.len());
    for (input, path) in inputs.iter().zip(paths) {
        let (frontmatter, _) = parse::split_frontmatter(&input.source);
        let doc = parse::parse_document(&input.source);
        let title = frontmatter
            .as_ref()
            .and_then(|fm| fm.title.clone())
            .or_else(|| first_heading_text(&doc))
            .unwrap_or_else(|| path_stem(&path));
        chapters.push(BookChapter {
            out_name: out_name(&path),
            path,
            title,
            frontmatter,
            doc,
        });
    }
    paths::canonicalize(&mut chapters);
    Ok(Book { chapters })
}

/// Flatten a book-relative path into a portable page name: `guide/install.md`
/// becomes `guide__install.html`. Unsafe URL/filename bytes use literal `~hh`
/// escapes. Reserved names such as `index.md` receive a `~chapter-` prefix,
/// leaving `index.html` available for the host's generated landing page.
///
/// Flattening can still collide (`a/b.md` versus `a__b.md`, or `.md` versus
/// `.markdown`). [`build_book`] rejects those collisions before publication.
pub fn out_name(path: &str) -> String {
    paths::output_name(path)
}

#[inline(always)]
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

#[inline(always)]
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

#[inline(always)]
fn plain_inlines(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_plain_inlines(inlines, &mut out);
    out
}

fn push_plain_inlines(inlines: &[Inline], out: &mut String) {
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                push_plain_inlines(c, out);
            }
            Inline::Link { content, .. } => push_plain_inlines(content, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Math(m) | Inline::DisplayMath(m) => out.push_str(m),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Html(_) | Inline::FootnoteRef { .. } => {}
        }
    }
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

/// Rewrite root-relative Markdown chapter links for the HTML site, preserving
/// queries and fragments. Documents returned by [`build_book`] already carry
/// the required root context. Unknown files and external links are untouched.
/// Returns the number of changed links; repeated calls are idempotent.
///
/// For an independently parsed document in a subdirectory, use
/// [`rewrite_links_for_site_from`] to supply its source path explicitly.
pub fn rewrite_links_for_site(
    doc: &mut Document,
    known_pages: &std::collections::BTreeSet<String>,
) -> usize {
    paths::rewrite_for_site(doc, known_pages)
}

/// Rewrite a separately parsed chapter using its book-relative source path.
/// Resolves sibling, parent, and root-relative links against the actual book
/// map; applies the same rules to tables, lists, definitions, and footnotes.
///
/// # Errors
/// Returns `RenderError::InvalidInput` for invalid or duplicate chapter paths.
/// Validation completes before the document is modified.
pub fn rewrite_links_for_site_from(
    doc: &mut Document,
    source_path: &str,
    book: &Book,
) -> Result<usize> {
    paths::rewrite_from(doc, source_path, book)
}

/// Merge the book into one document for the PDF: chapters concatenate in
/// order with a [`Block::PageBreak`] between them (the layout flag forces a
/// page boundary; the landed outline/contents machinery yields the global
/// TOC and continuous page numbers).
///
/// Multi-chapter books isolate footnote definitions and references in
/// chapter-local namespaces. Reusing `[^1]` in another source file therefore
/// cannot replace a citation from the first chapter. A one-chapter book keeps
/// its original IDs, and the source documents are never mutated.
#[must_use]
pub fn book_pdf_document(book: &Book) -> Document {
    merge::assemble(book)
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
    // The nav CSS goes into the document's existing <head> <style> block (just
    // before the closing </style>) so the page stays spec-valid HTML; the nav
    // element itself goes immediately before <main> in the body.
    let nav_css = "\n.fmd-book-nav{border:1px solid var(--fmd-border, #d1d9e0);border-radius:8px;padding:0.75rem 1rem;margin-bottom:1.25rem;font-size:0.9em}\n.fmd-book-nav ul{margin:0;padding-left:1.1rem}\n.fmd-book-nav .current{font-weight:700}\n.fmd-book-nav .current>a{text-decoration:none}\n";
    let mut out = String::with_capacity(rendered.len() + nav.len() + nav_css.len());
    let main_pos = rendered.find("<main class=\"fmd\">");
    let style_end = rendered.rfind("</style>");
    match (main_pos, style_end) {
        (Some(main), Some(style)) if style < main => {
            // Insert CSS into the head's style block, nav before <main>.
            out.push_str(&rendered[..style]);
            out.push_str(nav_css);
            out.push_str(&rendered[style..main]);
            out.push_str(&nav);
            out.push_str(&rendered[main..]);
        }
        (Some(main), _) => {
            // No style block found (custom CSS mode?): inline the style with
            // the nav in the body — invalid but visible; log via comment.
            out.push_str(&rendered[..main]);
            out.push_str(&nav);
            out.push_str("<style>");
            out.push_str(nav_css);
            out.push_str("</style>");
            out.push_str(&rendered[main..]);
        }
        _ => {
            // Emitter drift: fail loudly rather than silently dropping the nav.
            out.push_str(rendered);
            out.push_str("<!-- fmd-book-nav: injection point not found -->\n");
        }
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

/// Resolve a chapter-relative resource URL to a book-relative logical path
/// plus its unchanged query/fragment suffix. Percent decoding happens exactly
/// once. External URLs, fragment-only links, malformed encodings, invalid
/// source paths, and attempts to escape the book root return `None`.
///
/// This function does not read files or grant filesystem access. Native hosts
/// must separately validate symlinks, regular-file status and byte budgets.
#[must_use]
pub fn resolve_book_destination(source_path: &str, destination: &str) -> Option<(String, String)> {
    let source = paths::source_path(source_path)?;
    paths::destination(&source, destination)
}
