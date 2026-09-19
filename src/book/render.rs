//! Reusable, host-independent book rendering from one parsed book.

use std::collections::BTreeSet;

use super::{Book, BookInput, build_book, inject_book_nav, merge, out_name, paths};
use crate::wasm::WasmRenderOptions;
use crate::{
    Block, Document, HtmlOptions, Inline, PdfImageAsset, PdfOptions, RenderError, Result,
    ZipWriter, render_html_document,
};

#[path = "pdf_links.rs"]
mod pdf_links;
#[path = "site_search.rs"]
pub(crate) mod site_search;

const MAX_CHAPTERS: usize = 4096;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_IMAGES: usize = 4096;
const MAX_SITE_BYTES: usize = 256 * 1024 * 1024;

/// Parse once, then render PDF, EPUB, or a complete HTML-site ZIP repeatedly.
/// Assets are supplied as bytes, keyed by book-relative source paths. No
/// filesystem, network, environment, or native runtime is accessed.
///
/// Render options can change without reparsing. They do not retroactively
/// change the parsed Markdown dialect. Each render validates asset budgets;
/// direct changes through `options_mut` cannot bypass those checks.
#[derive(Debug, Clone)]
pub struct BookRenderer {
    book: Book,
    options: WasmRenderOptions,
    source_length: usize,
}

impl BookRenderer {
    /// Build a reusable renderer. Input order is the reading order.
    ///
    /// # Errors
    /// Rejects invalid book paths, collisions, more than 4096 chapters, or
    /// more than 64 MiB of source text and logical path bytes before parsing.
    pub fn new(inputs: &[BookInput]) -> Result<Self> {
        if inputs.is_empty() || inputs.len() > MAX_CHAPTERS {
            return Err(invalid("expected between 1 and 4096 chapters"));
        }
        let mut source_length = 0usize;
        let mut total = 0usize;
        for input in inputs {
            source_length = source_length.checked_add(input.source.len())
                .ok_or_else(|| invalid("source size overflow"))?;
            for len in [input.source.len(), input.path.len()] {
                total = total.checked_add(len).ok_or_else(|| invalid("source size overflow"))?;
                if total > MAX_SOURCE_BYTES {
                    return Err(invalid("source text and paths exceed the 64 MiB limit"));
                }
            }
        }
        Ok(Self {
            book: build_book(inputs)?,
            options: WasmRenderOptions::default(),
            source_length,
        })
    }

    /// The parsed, immutable book model.
    #[must_use]
    pub fn book(&self) -> &Book {
        &self.book
    }

    /// Original Markdown byte count, excluding logical filenames.
    #[must_use]
    pub fn source_length(&self) -> usize {
        self.source_length
    }

    /// Current shared render options.
    #[must_use]
    pub fn options(&self) -> &WasmRenderOptions {
        &self.options
    }

    /// Configure subsequent renders without reparsing source documents.
    pub fn options_mut(&mut self) -> &mut WasmRenderOptions {
        &mut self.options
    }

    /// Add or replace one host image. Replacement is atomic on validation
    /// failure and preserves the position of an existing destination.
    ///
    /// # Errors
    /// Rejects empty keys/payloads, images over 32 MiB, more than 4096 assets,
    /// or an aggregate image payload over 128 MiB.
    pub fn set_image(&mut self, destination: &str, bytes: Vec<u8>) -> Result<()> {
        let key = destination.trim();
        if key.is_empty() || bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            return Err(invalid("image needs a nonempty destination and 1..=32 MiB of bytes"));
        }
        let assets = &mut self.options.pdf_image_assets;
        let total = validate_assets(assets)?;
        let existing = assets.iter().position(|asset| asset.destination.trim() == key);
        let old_len = existing.map_or(0, |index| assets[index].bytes.len());
        let projected = total - old_len + bytes.len();
        if projected > MAX_TOTAL_IMAGE_BYTES || (existing.is_none() && assets.len() >= MAX_IMAGES) {
            return Err(invalid("images exceed the 128 MiB or 4096-asset limit"));
        }
        let asset = PdfImageAsset { destination: key.to_string(), bytes };
        if let Some(index) = existing {
            assets[index] = asset;
        } else {
            assets.push(asset);
        }
        Ok(())
    }

    /// Render one continuous PDF, resolving chapter-relative image assets.
    ///
    /// # Errors
    /// Returns path, asset-budget, or renderer validation failures.
    pub fn render_pdf(&self) -> Result<Vec<u8>> {
        validate_assets(&self.options.pdf_image_assets)?;
        render_book_pdf(&self.book, &self.options.pdf_options())
    }

    /// Render one EPUB with one spine item per chapter and packaged images.
    ///
    /// # Errors
    /// Returns asset-budget or EPUB validation failures.
    pub fn render_epub(&self) -> Result<Vec<u8>> {
        validate_assets(&self.options.pdf_image_assets)?;
        crate::epub::render_book_epub(&self.book, &self.options.html_options())
    }

    /// Render a complete HTML site as a deterministic ZIP archive.
    ///
    /// # Errors
    /// Returns path, asset-budget, output-budget, or HTML rendering failures.
    pub fn render_site(&self) -> Result<Vec<u8>> {
        validate_assets(&self.options.pdf_image_assets)?;
        render_book_site(&self.book, &self.options.html_options())
    }
}

/// Assemble a PDF document while resolving images against each chapter's
/// source directory. A matching book-relative asset takes precedence over an
/// unqualified fallback key. Missing, external, malformed, and unsafe URLs
/// remain unchanged and still pass through the PDF renderer's URL policy.
///
/// Both the book and asset bytes are borrowed. The AST is cloned only once;
/// no image payloads are cloned or given synthetic aliases. Use the returned
/// document with the same asset list in `PdfOptions.image_assets`.
///
/// # Errors
/// Rejects invalid/duplicate chapter paths and asset-budget violations.
pub fn book_pdf_document_with_assets(book: &Book, assets: &[PdfImageAsset]) -> Result<Document> {
    let sources = checked_paths(book)?;
    validate_assets(assets)?;
    let keys = asset_keys(assets);
    Ok(merge::assemble_with(book, |index, blocks| {
        resolve_images(blocks, &sources[index], &keys);
    }))
}

/// Render a PDF book with chapter-local citations, images, and navigation.
/// Known chapter links and fragment-only links bind to actual PDF outline
/// destinations, including duplicate/wrapped headings and moved note bodies.
/// A bare link to a chapter without a leading heading adds its chapter title
/// as the landing heading. Missing local anchors remain inert; external URLs
/// and all caller-provided typography/metadata options are preserved.
///
/// # Errors
/// Returns book validation, PDF renderer, or destination-binding errors.
pub fn render_book_pdf(book: &Book, options: &PdfOptions) -> Result<Vec<u8>> {
    let document = book_pdf_document_with_assets(book, &options.image_assets)?;
    pdf_links::render(book, document, options)
}

/// Render once and read the actual emitted page count for the CLI receipt.
/// This includes generated contents/landing pages and all footnote pages.
#[cfg(feature = "cli")]
pub(super) fn render_book_pdf_counted(book: &Book, options: &PdfOptions) -> Result<(Vec<u8>, u64)> {
    let bytes = render_book_pdf(book, options)?;
    let pages = pdf_links::page_count(&bytes)
        .ok_or_else(|| invalid("could not read the emitted PDF page count"))?;
    Ok((bytes, pages))
}

/// Render a self-contained HTML page per chapter, shared navigation, a landing
/// page, an offline search page, and a chapter-addressed search index into one deterministic ZIP.
///
/// Each page receives its chapter title and optional frontmatter language.
/// The book-level title labels the landing page. The search index nests the
/// existing per-document index under an explicit source/output path, avoiding
/// merged-document heading IDs that do not exist in any chapter page.
///
/// # Errors
/// Rejects invalid/duplicate paths or output names, invalid assets, and more
/// than 256 MiB of uncompressed site content. `BookChapter.out_name` must agree
/// with the generated name used by `build_book`.
pub fn render_book_site(book: &Book, options: &HtmlOptions) -> Result<Vec<u8>> {
    let sources = checked_paths(book)?;
    validate_assets(&options.image_assets)?;
    let mut output_names = BTreeSet::new();
    for (chapter, source) in book.chapters.iter().zip(&sources) {
        if chapter.out_name != out_name(source)
            || chapter.out_name.len() > 255
            || chapter.out_name.eq_ignore_ascii_case("index.html")
            || !output_names.insert(chapter.out_name.to_ascii_lowercase())
        {
            return Err(invalid("invalid, nonportable, or colliding HTML output filename"));
        }
    }
    let known = book.chapters.iter().map(|chapter| chapter.out_name.clone()).collect();
    let keys = asset_keys(&options.image_assets);
    // One AST copy for the entire site, not an extra copy per transformation.
    let mut chapters = book.chapters.clone();
    for (chapter, source) in chapters.iter_mut().zip(&sources) {
        chapter.path.clone_from(source);
    }
    paths::canonicalize(&mut chapters);
    let mut page_options = options.clone();
    let mut archive = ZipWriter::new();
    let mut total = 0usize;
    let search = site_search::index_json(book)?;
    add_site_bytes(&mut total, search.len())?;
    for (index, chapter) in chapters.iter_mut().enumerate() {
        paths::rewrite_for_site(&mut chapter.doc, &known);
        resolve_images(&mut chapter.doc.blocks, &sources[index], &keys);
        page_options.title = Some(chapter.title.clone());
        page_options.lang = chapter.frontmatter.as_ref()
            .and_then(|frontmatter| frontmatter.lang.clone())
            .or_else(|| options.lang.clone());
        let html = render_html_document(&chapter.doc, &page_options)?;
        let html = inject_book_nav(&html, book, &chapter.out_name);
        let html = site_search::inject_link(&html);
        add_site_bytes(&mut total, html.len())?;
        archive.add_deflated(&chapter.out_name, html.as_bytes());
    }
    archive.add_deflated("search-index.json", search.as_bytes());
    let first = &book.chapters[0];
    let search_page = site_search::page(
        &search, options.title.as_deref().unwrap_or(&first.title), options.lang.as_deref(),
    )?;
    add_site_bytes(&mut total, search_page.len())?;
    archive.add_deflated(site_search::PAGE_NAME, search_page.as_bytes());
    let first_name = super::escape_attr_pub(&first.out_name);
    let title = super::escape_text_pub(options.title.as_deref().unwrap_or(&first.title));
    let landing = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0; url={first_name}\"><title>{title}</title></head><body><p>Open <a href=\"{first_name}\">{title}</a>.</p></body></html>\n"
    );
    add_site_bytes(&mut total, landing.len())?;
    archive.add_deflated("index.html", landing.as_bytes());
    Ok(archive.finish())
}

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("book: {message}"))
}

fn checked_paths(book: &Book) -> Result<Vec<String>> {
    if book.chapters.is_empty() || book.chapters.len() > MAX_CHAPTERS {
        return Err(invalid("expected between 1 and 4096 chapters"));
    }
    let mut seen = BTreeSet::new();
    let mut sources = Vec::with_capacity(book.chapters.len());
    for chapter in &book.chapters {
        let source = paths::source_path(&chapter.path)
            .ok_or_else(|| invalid("invalid or escaping chapter source path"))?;
        if !seen.insert(source.clone()) {
            return Err(invalid("duplicate normalized chapter source path"));
        }
        sources.push(source);
    }
    Ok(sources)
}

fn validate_assets(assets: &[PdfImageAsset]) -> Result<usize> {
    if assets.len() > MAX_IMAGES {
        return Err(invalid("more than 4096 image assets"));
    }
    let mut total = 0usize;
    for asset in assets {
        if asset.destination.trim().is_empty() || asset.bytes.is_empty() || asset.bytes.len() > MAX_IMAGE_BYTES {
            return Err(invalid("image needs a nonempty destination and 1..=32 MiB of bytes"));
        }
        total = total.checked_add(asset.bytes.len()).ok_or_else(|| invalid("image size overflow"))?;
        if total > MAX_TOTAL_IMAGE_BYTES {
            return Err(invalid("image payloads exceed the 128 MiB limit"));
        }
    }
    Ok(total)
}

fn asset_keys(assets: &[PdfImageAsset]) -> BTreeSet<&str> {
    assets.iter().map(|asset| asset.destination.trim()).collect()
}

fn resolve_images(blocks: &mut [Block], source: &str, keys: &BTreeSet<&str>) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                resolve_inline_images(inlines, source, keys);
            }
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                resolve_images(inner, source, keys);
            }
            Block::List(list) => {
                for item in &mut list.items {
                    resolve_images(&mut item.blocks, source, keys);
                }
            }
            Block::Table(table) => {
                for cell in &mut table.head {
                    resolve_inline_images(cell, source, keys);
                }
                for row in &mut table.rows {
                    for cell in row {
                        resolve_inline_images(cell, source, keys);
                    }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        resolve_inline_images(inlines, source, keys);
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve_inline_images(inlines: &mut [Inline], source: &str, keys: &BTreeSet<&str>) {
    for inline in inlines {
        match inline {
            Inline::Image { dest, .. } => {
                if let Some((target, suffix)) = paths::destination(source, dest) {
                    let key = format!("{target}{suffix}");
                    if keys.contains(key.as_str()) {
                        *dest = key;
                    }
                }
            }
            Inline::Emphasis(inner)
            | Inline::Strong(inner)
            | Inline::Strikethrough(inner)
            | Inline::Link { content: inner, .. } => resolve_inline_images(inner, source, keys),
            _ => {}
        }
    }
}

fn add_site_bytes(total: &mut usize, bytes: usize) -> Result<()> {
    *total = total.checked_add(bytes).ok_or_else(|| invalid("HTML site size overflow"))?;
    if *total > MAX_SITE_BYTES {
        return Err(invalid("HTML site exceeds the 256 MiB uncompressed limit"));
    }
    Ok(())
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < ' ' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn renderer() -> BookRenderer {
        BookRenderer::new(&[
            BookInput {
                path: "guide/start.md".into(),
                source: "# Start\n\n![Chart](figure.svg)\n".into(),
            },
            BookInput {
                path: "appendix/end.md".into(),
                source: "# End\n\n![Chart](figure.svg)\n".into(),
            },
        ]).unwrap()
    }

    fn svg(width: u32) -> Vec<u8> {
        format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"10\"><rect width=\"{width}\" height=\"10\"/></svg>").into_bytes()
    }

    #[test]
    fn identical_image_basenames_resolve_in_their_own_chapters() {
        let mut renderer = renderer();
        renderer.set_image("guide/figure.svg", svg(10)).unwrap();
        renderer.set_image("appendix/figure.svg", svg(20)).unwrap();
        let before = renderer.book.chapters[0].doc.clone();
        let prepared = book_pdf_document_with_assets(
            &renderer.book, &renderer.options.pdf_image_assets,
        ).unwrap();
        let images: Vec<_> = prepared.blocks.iter().filter_map(|block| {
            if let Block::Paragraph(inlines) = block {
                inlines.iter().find_map(|inline| {
                    if let Inline::Image { dest, .. } = inline { Some(dest.as_str()) } else { None }
                })
            } else { None }
        }).collect();
        assert_eq!(images, vec!["guide/figure.svg", "appendix/figure.svg"]);
        assert_eq!(renderer.book.chapters[0].doc, before);
    }

    #[test]
    fn missing_external_and_unsafe_image_urls_are_not_reinterpreted() {
        let mut doc = crate::parse_markdown("![A](figure.svg) ![B](https://host/image.svg) ![C](javascript:bad)");
        let before = doc.clone();
        let keys = BTreeSet::from(["figure.svg", "https://host/image.svg", "javascript:bad"]);
        resolve_images(&mut doc.blocks, "guide/start.md", &keys);
        assert_eq!(doc, before, "unqualified fallback and URL policy must be preserved");
    }

    #[test]
    fn image_replacement_is_atomic_and_does_not_duplicate_payloads() {
        let mut renderer = renderer();
        renderer.set_image(" figure.svg ", svg(10)).unwrap();
        renderer.set_image("figure.svg", svg(20)).unwrap();
        assert_eq!(renderer.options.pdf_image_assets.len(), 1);
        assert_eq!(renderer.options.pdf_image_assets[0].bytes, svg(20));
        let before = renderer.options.pdf_image_assets.clone();
        assert!(renderer.set_image("figure.svg", vec![]).is_err());
        assert_eq!(renderer.options.pdf_image_assets, before);
    }

    #[test]
    fn render_validation_cannot_be_bypassed_via_mutable_options() {
        let mut renderer = renderer();
        renderer.options_mut().pdf_image_assets.push(PdfImageAsset {
            destination: "bad.svg".into(), bytes: vec![],
        });
        assert!(renderer.render_pdf().is_err());
        assert!(renderer.render_epub().is_err());
        assert!(renderer.render_site().is_err());
    }

    #[test]
    fn one_parsed_book_renders_all_formats_deterministically() {
        let mut renderer = renderer();
        renderer.options_mut().custom_css = Some(".fmd { line-height: 1.6; }".into());
        renderer.set_image("guide/figure.svg", svg(10)).unwrap();
        renderer.set_image("appendix/figure.svg", svg(20)).unwrap();
        let pdf = renderer.render_pdf().unwrap();
        let epub = renderer.render_epub().unwrap();
        let site = renderer.render_site().unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(epub.starts_with(b"PK\x03\x04"));
        assert!(site.starts_with(b"PK\x03\x04"));
        assert_eq!(pdf, renderer.render_pdf().unwrap());
        assert_eq!(epub, renderer.render_epub().unwrap());
        assert_eq!(site, renderer.render_site().unwrap());
    }

    #[test]
    fn json_metadata_escaping_preserves_controls_and_unicode() {
        assert_eq!(json_string("a\n\t\0\"\\中"), "\"a\\n\\t\\u0000\\\"\\\\中\"");
    }
}
