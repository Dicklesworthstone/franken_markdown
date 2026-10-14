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
pub use workspace::{BookSourceSetUpdate, BookSourceUpdate, BookWorkspace};

#[path = "book/validation.rs"]
pub mod validation;

pub use render::{BookRenderer, book_pdf_document_with_assets, render_book_pdf, render_book_site};

#[cfg(feature = "wasm-book")]
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

/// Collect body headings with the same collision-safe anchors as HTML search.
/// Includes headings inside lists and quotes; footnote bodies are not sections
/// of the chapter body. This does not mutate the source document or invent a
/// second slug algorithm. The existing tuple-only [`chapter_headings`] API is
/// retained for callers that do not need destinations.
#[must_use]
pub fn chapter_outline(doc: &Document) -> Vec<BookHeading> {
    crate::search_index::build_search_index(doc)
        .entries
        .into_iter()
        .filter(|entry| entry.kind == crate::search_index::EntryKind::Heading)
        .map(|entry| BookHeading {
            level: entry.level.unwrap_or(1),
            text: entry.text,
            anchor: entry.anchor,
        })
        .collect()
}

/// Inject chapter navigation, an expandable current-chapter outline, and
/// previous/next reading-order links into the renderer's HTML output. The
/// selected chapter is marked with `aria-current="page"`; unknown pages get
/// only the shared chapter list. No JavaScript or network access is required.
///
/// Only the current chapter's body is inspected for sections. Navigation CSS
/// stays inside the existing head, even when raw body HTML contains styles.
/// Fragments without a head retain functional, unstyled navigation. Missing
/// renderer `<main>` wrappers retain the existing explicit diagnostic comment.
#[must_use]
pub fn inject_book_nav(rendered: &str, book: &Book, current_page: &str) -> String {
    let selected = book
        .chapters
        .iter()
        .position(|chapter| chapter.out_name == current_page);
    let mut nav = String::from("<nav class=\"fmd-book-nav\" aria-label=\"Book contents\">\n<ul>\n");
    for (index, chapter) in book.chapters.iter().enumerate() {
        let (current, aria) = if Some(index) == selected {
            (" class=\"current\"", " aria-current=\"page\"")
        } else {
            ("", "")
        };
        nav.push_str(&format!(
            "<li{current}><a href=\"{}\"{aria}>{}</a></li>\n",
            escape_attr(&chapter.out_name),
            escape_text(&chapter.title),
        ));
    }
    nav.push_str("</ul>\n");
    if let Some(index) = selected {
        let headings = chapter_outline(&book.chapters[index].doc);
        push_book_outline(&headings, &mut nav);
        nav.push_str(&format!(
            "<p class=\"fmd-book-position\">Chapter {} of {}</p>\n",
            index + 1,
            book.chapters.len(),
        ));
        if book.chapters.len() > 1 {
            nav.push_str("<div class=\"fmd-book-sequence\">\n");
            if let Some(previous) = index.checked_sub(1).and_then(|i| book.chapters.get(i)) {
                push_book_neighbor(previous, "prev", "Previous chapter", &mut nav);
            }
            if let Some(next) = book.chapters.get(index + 1) {
                push_book_neighbor(next, "next", "Next chapter", &mut nav);
            }
            nav.push_str("</div>\n");
        }
    }
    nav.push_str("</nav>\n");

    let mut out = String::with_capacity(rendered.len() + nav.len() + BOOK_NAV_CSS.len() + 32);
    let Some(main) = rendered.find("<main class=\"fmd\">") else {
        out.push_str(rendered);
        out.push_str("<!-- fmd-book-nav: injection point not found -->\n");
        return out;
    };
    let prefix = &rendered[..main];
    if let Some(head) = prefix.find("</head>") {
        if let Some(style) = prefix[..head].rfind("</style>") {
            out.push_str(&prefix[..style]);
            out.push_str(BOOK_NAV_CSS);
            out.push_str(&prefix[style..]);
        } else {
            out.push_str(&prefix[..head]);
            out.push_str("<style>");
            out.push_str(BOOK_NAV_CSS);
            out.push_str("</style>\n");
            out.push_str(&prefix[head..]);
        }
    } else {
        out.push_str(prefix);
    }
    out.push_str(&nav);
    out.push_str(&rendered[main..]);
    out
}

const BOOK_NAV_CSS: &str = "\n.fmd-book-nav{border:1px solid var(--fmd-border, #d1d9e0);border-radius:8px;padding:0.75rem 1rem;margin-bottom:1.25rem;font-size:0.9em;overflow-wrap:anywhere}\n.fmd-book-nav ul,.fmd-book-outline ol{margin:0;padding-left:1.1rem}\n.fmd-book-nav .current{font-weight:700}\n.fmd-book-nav .current>a{text-decoration:none}\n.fmd-book-outline{margin-top:0.75rem}\n.fmd-book-outline summary{cursor:pointer;font-weight:600;margin-bottom:0.35rem}\n.fmd-book-position{margin:0.75rem 0 0.35rem;font-size:0.9em}\n.fmd-book-sequence{display:flex;flex-wrap:wrap;gap:0.5rem 1rem;justify-content:space-between}\n.fmd-book-sequence a{max-width:100%}\n.fmd-book-sequence a[rel=next]{margin-left:auto;text-align:right}\n";

fn push_book_neighbor(chapter: &BookChapter, rel: &str, label: &str, out: &mut String) {
    out.push_str(&format!(
        "<a rel=\"{rel}\" href=\"{}\">{label}: {}</a>\n",
        escape_attr(&chapter.out_name),
        escape_text(&chapter.title),
    ));
}

fn push_book_outline(headings: &[BookHeading], out: &mut String) {
    if headings.is_empty() {
        return;
    }
    out.push_str(
        "<details class=\"fmd-book-outline\" open>\n<summary>On this page</summary>\n<ol>\n",
    );
    // Track actual ancestors, not numeric depth. A jump from h2 to h5 opens
    // one child list, never three empty list items. Its following h4 is a
    // sibling under h2. Clamp host-built levels to bound the stack to six.
    let mut levels: Vec<u8> = Vec::with_capacity(6);
    for heading in headings {
        let level = heading.level.clamp(1, 6);
        if let Some(&previous) = levels.last() {
            if level > previous {
                out.push_str("\n<ol>\n");
                levels.push(level);
            } else {
                while levels.len() > 1 && level <= levels[levels.len() - 2] {
                    out.push_str("</li>\n</ol>\n");
                    levels.pop();
                }
                out.push_str("</li>\n");
                if let Some(current) = levels.last_mut() {
                    *current = level;
                }
            }
        } else {
            levels.push(level);
        }
        let label = if heading.text.is_empty() {
            "Untitled section"
        } else {
            &heading.text
        };
        out.push_str(&format!(
            "<li><a href=\"#{}\">{}</a>",
            escape_attr(&heading.anchor),
            escape_text(label),
        ));
    }
    for _ in levels {
        out.push_str("</li>\n</ol>\n");
    }
    out.push_str("</details>\n");
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

#[cfg(test)]
mod navigation_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::{HtmlOptions, render_html_document};

    const PAGE: &str = "<!doctype html><html><head><style>body{margin:0}</style></head><body><main class=\"fmd\">Body</main></body></html>";

    fn fixture() -> Book {
        build_book(&[
            BookInput {
                path: "zeta.md".into(),
                source: "# First\n\n## First section\n".into(),
            },
            BookInput {
                path: "alpha.md".into(),
                source: "# Middle\n\n## Middle section\n".into(),
            },
            BookInput {
                path: "guide/last.md".into(),
                source: "# Last\n\n## Last section\n".into(),
            },
        ])
        .expect("valid book")
    }

    #[test]
    fn neighbors_follow_reading_order_not_path_sorting() {
        let book = fixture();
        let original = book.chapters[1].doc.clone();
        let html = inject_book_nav(PAGE, &book, "alpha.html");
        assert!(html.contains(
            "<li class=\"current\"><a href=\"alpha.html\" aria-current=\"page\">Middle</a>"
        ));
        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
        assert!(html.contains("Chapter 2 of 3"));
        assert!(html.contains("<a rel=\"prev\" href=\"zeta.html\">Previous chapter: First</a>"));
        assert!(html.contains("<a rel=\"next\" href=\"guide__last.html\">Next chapter: Last</a>"));
        assert!(html.contains("href=\"#middle-section\""));
        assert!(!html.contains("href=\"#first-section\""));
        assert!(!html.contains("href=\"#last-section\""));
        assert_eq!(book.chapters[1].doc, original);
        assert_eq!(html, inject_book_nav(PAGE, &book, "alpha.html"));
    }

    #[test]
    fn first_and_last_chapters_never_wrap_or_link_outside_the_book() {
        let book = fixture();
        let first = inject_book_nav(PAGE, &book, "zeta.html");
        assert!(!first.contains("rel=\"prev\""));
        assert!(first.contains("rel=\"next\" href=\"alpha.html\""));
        assert!(first.contains("Chapter 1 of 3"));
        let last = inject_book_nav(PAGE, &book, "guide__last.html");
        assert!(last.contains("rel=\"prev\" href=\"alpha.html\""));
        assert!(!last.contains("rel=\"next\""));
        assert!(last.contains("Chapter 3 of 3"));
    }

    #[test]
    fn unknown_empty_and_single_chapter_books_have_no_phantom_neighbors() {
        let mut book = fixture();
        let unknown = inject_book_nav(PAGE, &book, "missing.html");
        assert!(!unknown.contains("aria-current="));
        assert!(!unknown.contains("<details"));
        assert!(!unknown.contains("rel=\"prev\""));
        assert!(!unknown.contains("rel=\"next\""));
        assert!(!unknown.contains("Chapter 1 of"));
        book.chapters.truncate(1);
        let single = inject_book_nav(PAGE, &book, "zeta.html");
        assert!(single.contains("Chapter 1 of 1"));
        assert!(!single.contains("rel=\"prev\""));
        assert!(!single.contains("rel=\"next\""));
        book.chapters.clear();
        let empty = inject_book_nav(PAGE, &book, "zeta.html");
        assert!(!empty.contains("<a "));
        assert!(!empty.contains("<details"));
    }

    #[test]
    fn outline_targets_real_duplicate_and_nested_heading_ids() {
        let source = "# Repeat\n\n# Repeat-2\n\n# Repeat\n\n> ## Nested\n\n# 中\n\n```md\n# Not a heading\n```\n";
        let book = build_book(&[BookInput {
            path: "chapter.md".into(),
            source: source.into(),
        }])
        .expect("valid chapter");
        let chapter = &book.chapters[0];
        let headings = chapter_outline(&chapter.doc);
        assert_eq!(
            headings
                .iter()
                .map(|entry| entry.anchor.as_str())
                .collect::<Vec<_>>(),
            ["repeat", "repeat-2", "repeat-3", "nested", "section"]
        );
        let rendered = render_html_document(
            &chapter.doc,
            &HtmlOptions {
                custom_css: Some(String::new()),
                ..HtmlOptions::default()
            },
        )
        .expect("HTML renderer");
        let with_nav = inject_book_nav(&rendered, &book, &chapter.out_name);
        for heading in headings {
            assert!(rendered.contains(&format!("id=\"{}\"", escape_attr(&heading.anchor))));
            assert!(with_nav.contains(&format!("href=\"#{}\"", escape_attr(&heading.anchor))));
        }
        assert!(!with_nav.contains("href=\"#not-a-heading\""));
    }

    #[test]
    fn skipped_levels_form_real_ancestor_lists_without_empty_items() {
        let headings: Vec<_> = [3, 5, 4, 2, 2, 6, 1]
            .into_iter()
            .enumerate()
            .map(|(index, level)| BookHeading {
                level,
                text: index.to_string(),
                anchor: format!("s{index}"),
            })
            .collect();
        let mut out = String::new();
        push_book_outline(&headings, &mut out);
        assert_eq!(
            out,
            concat!(
                "<details class=\"fmd-book-outline\" open>\n<summary>On this page</summary>\n<ol>\n",
                "<li><a href=\"#s0\">0</a>\n<ol>\n",
                "<li><a href=\"#s1\">1</a></li>\n",
                "<li><a href=\"#s2\">2</a></li>\n</ol>\n</li>\n",
                "<li><a href=\"#s3\">3</a></li>\n",
                "<li><a href=\"#s4\">4</a>\n<ol>\n",
                "<li><a href=\"#s5\">5</a></li>\n</ol>\n</li>\n",
                "<li><a href=\"#s6\">6</a></li>\n</ol>\n</details>\n",
            )
        );
    }

    #[test]
    fn hostile_labels_and_anchors_cannot_become_navigation_markup() {
        let mut book = fixture();
        book.chapters[0].title = "<script>alert(1)</script> & Previous".into();
        let html = inject_book_nav(PAGE, &book, "alpha.html");
        assert!(!html.contains("<script>"));
        assert!(
            html.contains("Previous chapter: &lt;script&gt;alert(1)&lt;/script&gt; &amp; Previous")
        );
        let mut out = String::new();
        push_book_outline(
            &[BookHeading {
                level: 255,
                text: "<img src=x> &".into(),
                anchor: "\" data-x=\"bad&".into(),
            }],
            &mut out,
        );
        assert!(out.contains("href=\"#&quot; data-x=&quot;bad&amp;\""));
        assert!(out.contains("&lt;img src=x&gt; &amp;"));
        assert_eq!(out.matches("<ol>").count(), 1);
    }

    #[test]
    fn headings_without_visible_text_still_have_named_links() {
        let mut out = String::new();
        push_book_outline(
            &[BookHeading {
                level: 0,
                text: String::new(),
                anchor: "section".into(),
            }],
            &mut out,
        );
        assert!(out.contains("<a href=\"#section\">Untitled section</a>"));
        out.clear();
        push_book_outline(&[], &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn navigation_styles_stay_in_the_head_when_body_html_has_styles() {
        let source = "<html><head><style>head{}</style></head><body><main class=\"fmd\"><style>body{}</style>Keep me</main></body></html>";
        let html = inject_book_nav(source, &fixture(), "alpha.html");
        assert!(html.find(BOOK_NAV_CSS).unwrap() < html.find("</head>").unwrap());
        assert_eq!(html.matches("<style>").count(), 2);
        let main = source.find("<main").unwrap();
        assert!(html.ends_with(&source[main..]));
    }

    #[test]
    fn stylesheet_free_pages_get_head_css_but_fragments_do_not_get_body_styles() {
        let book = fixture();
        let page = "<html><head><title>T</title></head><body><main class=\"fmd\">Body</main></body></html>";
        let html = inject_book_nav(page, &book, "alpha.html");
        assert!(html.find("<style>").unwrap() < html.find("</head>").unwrap());
        assert_eq!(html.matches("<style>").count(), 1);
        let fragment = "<main class=\"fmd\">Body</main>";
        let html = inject_book_nav(fragment, &book, "alpha.html");
        assert!(html.contains("rel=\"next\""));
        assert!(!html.contains("<style>"));
        assert!(html.ends_with(fragment));
    }

    #[test]
    fn missing_main_keeps_the_original_and_the_existing_diagnostic() {
        let source = "<html><head></head><body>Unrecognized emitter</body></html>";
        assert_eq!(
            inject_book_nav(source, &fixture(), "alpha.html"),
            format!("{source}<!-- fmd-book-nav: injection point not found -->\n")
        );
    }
}
