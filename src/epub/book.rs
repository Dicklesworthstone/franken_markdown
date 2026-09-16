//! Multi-chapter EPUB export using the existing parsed book model.

use std::collections::BTreeMap;

use franken_markdown::{Book, HtmlOptions, PdfImageAsset, RenderError, Result};

use super::{
    Block, CONTAINER_XML, DCTERMS_MODIFIED, Inline, MIMETYPE, NavHeading, STYLE_CSS,
    ZipWriter, chapter_xhtml, collect_headings, escape_xml_attr, escape_xml_text,
    extract_main_body, fnv1a64, html_fragment_to_xhtml, push_nav_headings, resources,
};

const MAX_CHAPTERS: usize = 4096;
const MAX_RESOURCES: usize = 4096;
const MAX_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_BOOK_BYTES: usize = 256 * 1024 * 1024;

struct Chapter {
    title: String,
    file: String,
    xhtml: String,
    headings: Vec<NavHeading>,
    content: resources::Chapter,
}

struct PreparedBook {
    chapters: Vec<Chapter>,
    opf: String,
    nav: String,
}

/// Render a parsed [`Book`] as one EPUB with a separate XHTML spine item per
/// chapter. Chapter order is the input order; output names are generated,
/// rather than derived from source paths, so flattened-name collisions cannot
/// overwrite chapters or assets.
///
/// Relative links to other chapters (including `../`, percent-encoded UTF-8,
/// queries and fragments) are rewritten to their EPUB destinations. External
/// links and links to files outside the book remain unchanged. Images may be
/// supplied in `opts.image_assets` under book-relative paths; a matching
/// book-relative asset takes precedence over the original destination key.
/// No filesystem or network access occurs in the render core.
///
/// `opts.title` and `opts.lang` describe the book. Chapter titles and language
/// overrides come from the book model. `opts.custom_css` replaces the default
/// EPUB stylesheet. Raw HTML is escaped regardless of the pass-through option.
///
/// # Errors
/// Rejects empty books, duplicate or invalid source paths, more than 4096
/// chapters or image resources, more than 128 MiB of image payloads, and more
/// than 256 MiB of rendered chapter markup, stylesheet and book metadata.
/// The single-document image limits also apply to each chapter.
pub fn render_book_epub(book: &Book, opts: &HtmlOptions) -> Result<Vec<u8>> {
    let prepared = prepare_book(book, opts)?;
    let mut zip = ZipWriter::new();
    zip.add_stored("mimetype", MIMETYPE);
    zip.add_deflated("META-INF/container.xml", CONTAINER_XML.as_bytes());
    zip.add_deflated("OEBPS/content.opf", prepared.opf.as_bytes());
    zip.add_deflated("OEBPS/nav.xhtml", prepared.nav.as_bytes());
    zip.add_deflated(
        "OEBPS/style.css",
        opts.custom_css.as_deref().unwrap_or(STYLE_CSS).as_bytes(),
    );
    for chapter in &prepared.chapters {
        zip.add_deflated(&format!("OEBPS/{}", chapter.file), chapter.xhtml.as_bytes());
        for resource in &chapter.content.resources {
            let path = format!("OEBPS/{}", resource.href);
            if resource.media_type == "image/svg+xml" {
                zip.add_deflated(&path, &resource.bytes);
            } else {
                zip.add_stored(&path, &resource.bytes);
            }
        }
    }
    Ok(zip.finish())
}

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("epub book: {message}"))
}

fn prepare_book(book: &Book, opts: &HtmlOptions) -> Result<PreparedBook> {
    if book.chapters.is_empty() || book.chapters.len() > MAX_CHAPTERS {
        return Err(invalid("expected between 1 and 4096 chapters"));
    }
    let mut known = BTreeMap::new();
    let mut paths = Vec::with_capacity(book.chapters.len());
    for (index, chapter) in book.chapters.iter().enumerate() {
        if chapter.path.starts_with('/') || has_scheme(&chapter.path) {
            return Err(invalid("chapter paths must be book-relative"));
        }
        let path = normalize_path(&chapter.path)
            .ok_or_else(|| invalid("invalid or escaping chapter source path"))?;
        if known.insert(path.clone(), index).is_some() {
            return Err(invalid("duplicate normalized chapter source path"));
        }
        paths.push(path);
    }

    let title = opts.title.as_deref().unwrap_or("Book");
    let lang = opts.lang.as_deref().unwrap_or("en");
    let css = opts.custom_css.as_deref().unwrap_or(STYLE_CSS);
    let mut byte_count = 0;
    for text in [title, lang, css] {
        add_bytes(&mut byte_count, text.len())?;
    }
    let mut identity = Identity::new();
    for text in ["franken_markdown/epub-book/v1", title, lang, css] {
        identity.part(text);
    }
    let mut image_bytes = 0usize;
    let mut resource_count = 0usize;
    let mut chapters = Vec::with_capacity(book.chapters.len());
    for (index, source) in book.chapters.iter().enumerate() {
        let mut doc = source.doc.clone();
        rewrite_blocks(&mut doc.blocks, &paths[index], &known, &opts.image_assets);
        let chapter_lang = source
            .frontmatter
            .as_ref()
            .and_then(|fm| fm.lang.as_deref())
            .unwrap_or(lang);
        let mut html_opts = opts.clone();
        html_opts.title = Some(source.title.clone());
        html_opts.lang = Some(chapter_lang.to_string());
        html_opts.custom_css = Some(String::new());
        html_opts.allow_raw_html = false;
        let page = franken_markdown::html::render(&doc, &html_opts);
        add_bytes(&mut byte_count, page.len())?;
        let body = extract_main_body(&page)
            .ok_or_else(|| invalid("HTML renderer <main> wrapper not found"))?;
        let body = html_fragment_to_xhtml(body);
        for text in [&paths[index], &source.title, chapter_lang, &body] {
            identity.part(text);
        }
        let content = resources::prepare_with_prefix(&body, &format!("chapter-{}-", index + 1))
            .map_err(invalid)?;
        resource_count += content.resources.len();
        image_bytes += content.resources.iter().map(|r| r.bytes.len()).sum::<usize>();
        if resource_count > MAX_RESOURCES || image_bytes > MAX_IMAGE_BYTES {
            return Err(invalid("book exceeds 4096 images or 128 MiB of image payloads"));
        }
        let wrapped = format!("<main class=\"fmd\">\n{}</main>\n", content.body);
        chapters.push(Chapter {
            title: source.title.clone(),
            file: chapter_file(index),
            xhtml: chapter_xhtml(&source.title, chapter_lang, &wrapped),
            headings: collect_headings(&doc.blocks),
            content,
        });
    }
    let opf = package(title, lang, &identity.finish(), &chapters);
    let nav = navigation(title, lang, &chapters);
    Ok(PreparedBook { chapters, opf, nav })
}

fn add_bytes(total: &mut usize, count: usize) -> Result<()> {
    *total = total.checked_add(count).ok_or_else(|| invalid("book size overflow"))?;
    if *total > MAX_BOOK_BYTES {
        return Err(invalid("rendered book exceeds 256 MiB"));
    }
    Ok(())
}

fn chapter_file(index: usize) -> String {
    format!("chapter-{}.xhtml", index + 1)
}

fn has_scheme(path: &str) -> bool {
    path.split('/').next().is_some_and(|part| part.contains(':'))
}

/// Normalize logical book paths, never native filesystem paths.
fn normalize_path(path: &str) -> Option<String> {
    if path.contains(['\\', '\0']) || path.ends_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn decode_path(path: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push((hex(*bytes.get(i + 1)?)? << 4) | hex(*bytes.get(i + 2)?)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn destination(source: &str, dest: &str) -> Option<(String, String)> {
    let dest = dest.trim();
    if dest.starts_with(['#', '?']) || dest.starts_with("//") || has_scheme(dest) {
        return None;
    }
    let end = dest.find(['#', '?']).unwrap_or(dest.len());
    let path = decode_path(&dest[..end])?;
    if path.is_empty() || has_scheme(&path) {
        return None;
    }
    let joined = if path.starts_with('/') {
        path
    } else if let Some((parent, _)) = source.rsplit_once('/') {
        format!("{parent}/{path}")
    } else {
        path
    };
    Some((normalize_path(&joined)?, dest[end..].to_string()))
}

fn rewrite_blocks(
    blocks: &mut [Block],
    source: &str,
    known: &BTreeMap<String, usize>,
    assets: &[PdfImageAsset],
) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                rewrite_inlines(inlines, source, known, assets);
            }
            Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => {
                rewrite_blocks(blocks, source, known, assets);
            }
            Block::List(list) => {
                for item in &mut list.items {
                    rewrite_blocks(&mut item.blocks, source, known, assets);
                }
            }
            Block::Table(table) => {
                for cell in &mut table.head {
                    rewrite_inlines(cell, source, known, assets);
                }
                for row in &mut table.rows {
                    for cell in row {
                        rewrite_inlines(cell, source, known, assets);
                    }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        rewrite_inlines(inlines, source, known, assets);
                    }
                }
            }
            _ => {}
        }
    }
}

fn rewrite_inlines(
    inlines: &mut [Inline],
    source: &str,
    known: &BTreeMap<String, usize>,
    assets: &[PdfImageAsset],
) {
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => {
                if let Some((target, suffix)) = destination(source, dest) {
                    if let Some(&index) = known.get(&target) {
                        *dest = format!("{}{suffix}", chapter_file(index));
                    }
                }
                rewrite_inlines(content, source, known, assets);
            }
            Inline::Image { dest, .. } => {
                if let Some((target, suffix)) = destination(source, dest) {
                    let key = format!("{target}{suffix}");
                    if assets.iter().any(|asset| asset.destination.trim() == key) {
                        *dest = key;
                    }
                }
            }
            Inline::Emphasis(content) | Inline::Strong(content) | Inline::Strikethrough(content) => {
                rewrite_inlines(content, source, known, assets);
            }
            _ => {}
        }
    }
}

/// Length-prefix every component with a fixed-width integer, independent of
/// native/WASM pointer width. Hash original embedded bytes, not asset names.
struct Identity(u64, u64);

impl Identity {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325, 0x9e37_79b9_7f4a_7c15)
    }

    fn part(&mut self, text: &str) {
        let len = (text.len() as u64).to_le_bytes();
        self.0 = fnv1a64(fnv1a64(self.0, &len), text.as_bytes());
        self.1 = fnv1a64(fnv1a64(self.1, &len), text.as_bytes());
    }

    fn finish(self) -> String {
        format!(
            "urn:uuid:{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            self.0 >> 32,
            (self.0 >> 16) & 0xFFFF,
            self.0 & 0xFFFF,
            self.1 >> 48,
            self.1 & 0xFFFF_FFFF_FFFF
        )
    }
}

fn package(title: &str, lang: &str, identifier: &str, chapters: &[Chapter]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n<dc:identifier id=\"bookid\">");
    escape_xml_text(identifier, &mut out);
    out.push_str("</dc:identifier>\n<dc:title>");
    escape_xml_text(title, &mut out);
    out.push_str("</dc:title>\n<dc:language>");
    escape_xml_text(lang, &mut out);
    out.push_str("</dc:language>\n<meta property=\"dcterms:modified\">");
    out.push_str(DCTERMS_MODIFIED);
    out.push_str("</meta>\n</metadata>\n<manifest>\n<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n<item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n");
    for (index, chapter) in chapters.iter().enumerate() {
        out.push_str(&format!("<item id=\"chapter-{}\" href=\"{}\" media-type=\"application/xhtml+xml\"", index + 1, chapter.file));
        let properties = match (chapter.content.mathml, chapter.content.svg) {
            (true, true) => "mathml svg",
            (true, false) => "mathml",
            (false, true) => "svg",
            (false, false) => "",
        };
        if !properties.is_empty() {
            out.push_str(&format!(" properties=\"{properties}\""));
        }
        out.push_str("/>\n");
        for (image, resource) in chapter.content.resources.iter().enumerate() {
            out.push_str(&format!("<item id=\"chapter-{}-image-{}\" href=\"", index + 1, image + 1));
            escape_xml_attr(&resource.href, &mut out);
            out.push_str(&format!("\" media-type=\"{}\"/>\n", resource.media_type));
        }
    }
    out.push_str("</manifest>\n<spine>\n");
    for index in 0..chapters.len() {
        out.push_str(&format!("<itemref idref=\"chapter-{}\"/>\n", index + 1));
    }
    out.push_str("</spine>\n</package>\n");
    out
}

fn navigation(title: &str, lang: &str, chapters: &[Chapter]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"");
    escape_xml_attr(lang, &mut out);
    out.push_str("\" xml:lang=\"");
    escape_xml_attr(lang, &mut out);
    out.push_str("\"><head><title>");
    escape_xml_text(title, &mut out);
    out.push_str("</title></head><body><nav epub:type=\"toc\" id=\"toc\"><h1>");
    escape_xml_text(title, &mut out);
    out.push_str("</h1>\n<ol>\n");
    for chapter in chapters {
        out.push_str("<li><a href=\"");
        escape_xml_attr(&chapter.file, &mut out);
        out.push_str("\">");
        escape_xml_text(&chapter.title, &mut out);
        out.push_str("</a>");
        if !chapter.headings.is_empty() {
            out.push_str("\n<ol>\n");
            push_nav_headings(&chapter.headings, &chapter.file, &mut out);
            out.push_str("</ol>\n");
        }
        out.push_str("</li>\n");
    }
    out.push_str("</ol>\n</nav></body></html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use franken_markdown::{BookInput, build_book};

    fn sample() -> Result<Book> {
        build_book(&[
            BookInput {
                path: "guide/first.md".into(),
                source: "# First\n\n[Next](../second.md#second) and [Web](https://example.com/second.md).\n\n## Detail\n".into(),
            },
            BookInput {
                path: "second.md".into(),
                source: "---\nlang: fr\n---\n# Second\n\n[Back](guide/first.md#first)\n".into(),
            },
        ])
    }

    #[test]
    fn chapters_keep_order_links_languages_and_navigation() -> Result<()> {
        let book = sample()?;
        let prepared = prepare_book(&book, &HtmlOptions::default())?;
        assert!(prepared.chapters[0].xhtml.contains("href=\"chapter-2.xhtml#second\""));
        assert!(prepared.chapters[1].xhtml.contains("href=\"chapter-1.xhtml#first\""));
        assert!(prepared.chapters[1].xhtml.contains("xml:lang=\"fr\""));
        assert!(prepared.chapters[0].xhtml.contains("https://example.com/second.md"));
        assert!(prepared.opf.contains("<itemref idref=\"chapter-1\"/>\n<itemref idref=\"chapter-2\"/>"));
        assert!(prepared.nav.contains("chapter-1.xhtml#detail"));
        assert!(prepared.nav.contains("chapter-2.xhtml#second"));
        assert_eq!(render_book_epub(&book, &HtmlOptions::default())?, render_book_epub(&book, &HtmlOptions::default())?);
        Ok(())
    }

    #[test]
    fn resolves_encoded_paths_without_rewriting_external_or_escaping_links() {
        assert_eq!(destination("guide/first.md", "../caf%C3%A9%20notes.md?q=1#part"), Some(("café notes.md".into(), "?q=1#part".into())));
        assert_eq!(destination("guide/first.md", "/intro.md#top"), Some(("intro.md".into(), "#top".into())));
        for dest in ["#local", "?query", "https://example.com/x.md", "//host/x.md", "mailto:a@b", "../../outside.md", "%ff.md", "%zz.md", "bad%", ""] {
            assert!(destination("guide/first.md", dest).is_none(), "rewrote {dest}");
        }
    }

    #[test]
    fn rejects_empty_duplicate_and_escaping_chapters() -> Result<()> {
        assert!(render_book_epub(&Book { chapters: vec![] }, &HtmlOptions::default()).is_err());
        let mut book = sample()?;
        book.chapters[1].path = "guide/./first.md".into();
        assert!(render_book_epub(&book, &HtmlOptions::default()).is_err());
        book.chapters[1].path = "../outside.md".into();
        assert!(render_book_epub(&book, &HtmlOptions::default()).is_err());
        Ok(())
    }

    #[test]
    fn book_relative_assets_are_isolated_and_declared() -> Result<()> {
        let mut book = sample()?;
        for chapter in &mut book.chapters {
            chapter.doc = franken_markdown::parse_markdown("# Image\n\n![shape](figure.svg)");
        }
        let opts = HtmlOptions {
            image_assets: vec![
                PdfImageAsset {
                    destination: "guide/figure.svg".into(),
                    bytes: b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>".to_vec(),
                },
                PdfImageAsset {
                    destination: "figure.svg".into(),
                    bytes: b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><circle cx=\"10\" cy=\"10\" r=\"5\"/></svg>".to_vec(),
                },
            ],
            ..HtmlOptions::default()
        };
        let prepared = prepare_book(&book, &opts)?;
        assert_eq!(prepared.chapters[0].content.resources.len(), 1);
        assert_eq!(prepared.chapters[1].content.resources.len(), 1);
        assert!(prepared.chapters[0].xhtml.contains("assets/chapter-1-image-1.svg"));
        assert!(prepared.chapters[1].xhtml.contains("assets/chapter-2-image-1.svg"));
        assert_ne!(prepared.chapters[0].content.resources[0].bytes, prepared.chapters[1].content.resources[0].bytes);
        assert_eq!(prepared.opf.matches("properties=\"svg\"").count(), 2);
        Ok(())
    }

    #[test]
    fn styles_and_chapter_order_affect_identity() -> Result<()> {
        let book = sample()?;
        let plain = prepare_book(&book, &HtmlOptions::default())?;
        let opts = HtmlOptions { custom_css: Some(".fmd { color: navy; }".into()), ..HtmlOptions::default() };
        let styled = prepare_book(&book, &opts)?;
        assert_ne!(plain.opf, styled.opf);
        assert!(styled.chapters[0].xhtml.contains("<main class=\"fmd\">"));
        let mut reversed = book;
        reversed.chapters.reverse();
        assert_ne!(plain.opf, prepare_book(&reversed, &HtmlOptions::default())?.opf);
        Ok(())
    }
}
