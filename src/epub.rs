//! EPUB 3 (Open Container Format) rendering for a single parsed document.
//!
//! Produces a byte-deterministic `.epub` — a ZIP archive with the mandated
//! OCF layout:
//!
//! * `mimetype` — always the FIRST entry and always STORED, containing exactly
//!   `application/epub+zip` (the OCF magic invariant).
//! * `META-INF/container.xml` — points at the package document.
//! * `OEBPS/content.opf` — the EPUB 3 package: metadata (`dc:title`,
//!   `dc:language`, a content-derived deterministic `dc:identifier`),
//!   manifest, and spine.
//! * `OEBPS/nav.xhtml` — the EPUB 3 navigation document listing every heading
//!   in document order, linked to the anchors the chapter HTML carries.
//! * `OEBPS/chapter-1.xhtml` — the rendered document body.
//! * `OEBPS/style.css` — a minimal deterministic stylesheet.
//!
//! The chapter body comes from the crate's own HTML renderer: the document is
//! rendered once with the stylesheet slot blanked (EPUB carries its own
//! `style.css`), the `<main class="fmd">` body is lifted out with exact string
//! surgery on the renderer's fixed shell markers, and the handful of HTML5
//! void-element spellings the emitter produces (`<br>`, `<hr>`, `<img …>`,
//! `<input …>`) are rewritten to their XML self-closing forms. Raw-HTML
//! pass-through is disabled for the EPUB render so user markup cannot break
//! XHTML well-formedness. MathML emitted by the crate's math engine is already
//! namespace-qualified and free of self-closing tags, so it passes through
//! unchanged.
//!
//! Determinism: no clocks, no environment reads, no hash-iteration leaks. The
//! one timestamp-shaped field the EPUB 3 schema mandates
//! (`dcterms:modified`) is pinned to the constant `1970-01-01T00:00:00Z`.

use std::collections::BTreeMap;

use franken_markdown::ast::{Block, Document, Inline};
use franken_markdown::{
    HtmlOptions, RenderError, Result, find_html_text_escape, find_xml_attr_escape,
};

use crate::zip::ZipWriter;

/// OCF mimetype payload — byte-exact, no trailing newline.
const MIMETYPE: &[u8] = b"application/epub+zip";

/// The EPUB 3 schema mandates exactly one `dcterms:modified`. Pinned to the
/// epoch constant so renders stay byte-deterministic.
const DCTERMS_MODIFIED: &str = "1970-01-01T00:00:00Z";

/// Fixed OCF container pointing at the single package document.
const CONTAINER_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\
  <rootfiles>\n\
    <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\n\
  </rootfiles>\n\
</container>\n";

/// Minimal deterministic stylesheet for the chapter and nav documents.
const STYLE_CSS: &str = "body{font-family:serif;line-height:1.6;margin:5%;}\n\
h1,h2,h3,h4,h5,h6{line-height:1.25;}\n\
pre,code{font-family:monospace;}\n\
pre{overflow-x:auto;border:1px solid #ccc;padding:0.5em;}\n\
table{border-collapse:collapse;}\n\
th,td{border:1px solid #ccc;padding:0.25em 0.5em;}\n\
blockquote{margin-left:0;padding-left:1em;border-left:0.25em solid #ccc;}\n\
img{max-width:100%;}\n";

/// Render `doc` as a complete EPUB 3 archive.
///
/// The archive is byte-deterministic for a given `(doc, opts)` pair: ZIP
/// entries are emitted in a fixed order with zeroed timestamps, and all
/// metadata derives from the content itself.
///
/// # Errors
/// Returns [`RenderError::InvalidInput`] if the HTML renderer's fixed
/// `<main class="fmd">` wrapper cannot be located — a renderer-contract
/// violation, never caused by user input.
pub fn render_epub(doc: &Document, opts: &HtmlOptions) -> Result<Vec<u8>> {
    let title = opts
        .title
        .clone()
        .or_else(|| first_heading_text(doc))
        .unwrap_or_else(|| "Document".to_string());
    let lang = opts.lang.clone().unwrap_or_else(|| "en".to_string());

    // Render once through the crate HTML renderer with the stylesheet slot
    // blanked (EPUB carries its own style.css, and this skips the font
    // subsetting work the default stylesheet performs) and raw-HTML
    // pass-through off (arbitrary HTML could break XHTML well-formedness).
    let mut html_opts = opts.clone();
    html_opts.custom_css = Some(String::new());
    html_opts.allow_raw_html = false;
    let page = franken_markdown::html::render(doc, &html_opts);
    let body = extract_main_body(&page).ok_or_else(|| {
        RenderError::InvalidInput("epub: HTML renderer <main> wrapper not found".to_string())
    })?;
    let chapter_body = html_fragment_to_xhtml(&body);

    let identifier = content_identifier(&title, &lang, &chapter_body);

    let chapter = chapter_xhtml(&title, &lang, &chapter_body);
    let nav = nav_xhtml(&title, &lang, doc);
    let opf = content_opf(&title, &lang, &identifier);

    let mut zip = ZipWriter::new();
    // OCF invariant: the mimetype entry is first and stored, byte-exact.
    zip.add_stored("mimetype", MIMETYPE);
    zip.add_deflated("META-INF/container.xml", CONTAINER_XML.as_bytes());
    zip.add_deflated("OEBPS/content.opf", opf.as_bytes());
    zip.add_deflated("OEBPS/nav.xhtml", nav.as_bytes());
    zip.add_deflated("OEBPS/chapter-1.xhtml", chapter.as_bytes());
    zip.add_deflated("OEBPS/style.css", STYLE_CSS.as_bytes());
    Ok(zip.finish())
}

// ---------------------------------------------------------------------------
// Chapter body extraction and XHTML conversion.

/// Extract the `<main class="fmd">` body from a full renderer document. The
/// renderer's shell is a fixed byte sequence, so this is exact string surgery
/// on two constant markers, not parsing.
fn extract_main_body(page: &str) -> Option<&str> {
    const OPEN: &str = "<main class=\"fmd\">\n";
    const CLOSE: &str = "</main>";
    let start = page.find(OPEN)? + OPEN.len();
    let end = page.rfind(CLOSE)?;
    (end >= start).then(|| &page[start..end])
}

fn html_fragment_to_xhtml(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 16);
    let mut rest = html;
    // Bulk-copy pass: the renderer's text nodes arrive already escaped, so
    // every byte before the next '<' is pass-through — copy the whole run in
    // one push_str and only stop to inspect tag openers. '<' is ASCII, so the
    // find always lands on a UTF-8 char boundary and multi-byte characters in
    // a clean run never need per-character decoding.
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = &rest[lt..];
        if let Some(stripped) = rest.strip_prefix("<br>") {
            out.push_str("<br/>");
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("<br/>") {
            out.push_str("<br/>");
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("<br />") {
            out.push_str("<br/>");
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("<hr>") {
            out.push_str("<hr/>");
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("<hr/>") {
            out.push_str("<hr/>");
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("<hr />") {
            out.push_str("<hr/>");
            rest = stripped;
            continue;
        }
        if rest.starts_with("<img ") || rest.starts_with("<input ") {
            match rest.find('>') {
                Some(end) => {
                    let tag_body = rest[..end].trim_end_matches([' ', '/']);
                    push_void_tag_xhtml(tag_body, &mut out);
                    out.push_str("/>");
                    rest = &rest[end + 1..];
                }
                None => {
                    out.push_str(rest);
                    rest = "";
                }
            }
            continue;
        }
        // A '<' that opens none of the rewritable spellings is pass-through.
        out.push('<');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

/// XML-legalize one void-tag body (text between `<name` and the closing `>`):
/// bare boolean attributes — the emitter's fixed task-list checkbox spellings
/// `disabled` / `checked` — gain explicit values. Quote-aware so attribute
/// *values* containing those words are left alone.
fn push_void_tag_xhtml(tag: &str, out: &mut String) {
    const BOOL_ATTRS: [&str; 2] = ["checked", "disabled"];
    let bytes = tag.as_bytes();
    let mut in_quotes = false;
    let mut i = 0;
    while i < bytes.len() {
        if !in_quotes
            && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r')
        {
            let after = &tag[i + 1..];
            let matched = BOOL_ATTRS.iter().find(|name| {
                after.starts_with(**name)
                    && after[name.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| c.is_ascii_whitespace() || c == '/')
            });
            if let Some(name) = matched {
                out.push(' ');
                out.push_str(name);
                out.push_str("=\"");
                out.push_str(name);
                out.push('"');
                i += 1 + name.len();
                continue;
            }
        }
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
        }
        let ch_len = tag[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&tag[i..i + ch_len]);
        i += ch_len;
    }
}

// ---------------------------------------------------------------------------
// XML escaping (total: every text node and attribute value passes through).

/// XML text-node escaping: `&`, `<`, `>` — the same byte set as the scanner's
/// HTML text scanner, so bulk-copy clean runs between escapable bytes. All
/// specials are ASCII, so byte indexing is UTF-8-safe.
fn escape_xml_text(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut start = 0;
    while let Some(rel) = find_html_text_escape(&bytes[start..]) {
        let pos = start + rel;
        out.push_str(&s[start..pos]);
        match bytes[pos] {
            b'&' => out.push_str("&amp;"),
            b'<' => out.push_str("&lt;"),
            _ => out.push_str("&gt;"),
        }
        start = pos + 1;
    }
    out.push_str(&s[start..]);
}

/// XML double-quoted attribute escaping: `&`, `<`, `>`, `"`, `'` — bulk-copy
/// clean runs between escapable bytes (the scanner's XML attribute set).
fn escape_xml_attr(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut start = 0;
    while let Some(rel) = find_xml_attr_escape(&bytes[start..]) {
        let pos = start + rel;
        out.push_str(&s[start..pos]);
        match bytes[pos] {
            b'&' => out.push_str("&amp;"),
            b'<' => out.push_str("&lt;"),
            b'>' => out.push_str("&gt;"),
            b'"' => out.push_str("&quot;"),
            _ => out.push_str("&apos;"),
        }
        start = pos + 1;
    }
    out.push_str(&s[start..]);
}

// ---------------------------------------------------------------------------
// Document shells.

fn chapter_xhtml(title: &str, lang: &str, body: &str) -> String {
    let mut s = String::with_capacity(body.len() + 320);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" lang=\"");
    escape_xml_attr(lang, &mut s);
    s.push_str("\" xml:lang=\"");
    escape_xml_attr(lang, &mut s);
    s.push_str("\">\n<head>\n<meta charset=\"utf-8\"/>\n<title>");
    escape_xml_text(title, &mut s);
    s.push_str("</title>\n<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>\n</head>\n<body>\n");
    s.push_str(body);
    s.push_str("</body>\n</html>\n");
    s
}

fn nav_xhtml(title: &str, lang: &str, doc: &Document) -> String {
    let headings = collect_headings(&doc.blocks);
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"");
    escape_xml_attr(lang, &mut s);
    s.push_str("\" xml:lang=\"");
    escape_xml_attr(lang, &mut s);
    s.push_str("\">\n<head>\n<meta charset=\"utf-8\"/>\n<title>");
    escape_xml_text(title, &mut s);
    s.push_str("</title>\n</head>\n<body>\n<nav epub:type=\"toc\" id=\"toc\">\n<h1>");
    escape_xml_text(title, &mut s);
    s.push_str("</h1>\n<ol>\n");
    if headings.is_empty() {
        // A nav document must exist even for heading-less documents.
        s.push_str("<li><a href=\"chapter-1.xhtml\">");
        escape_xml_text(title, &mut s);
        s.push_str("</a></li>\n");
    } else {
        for heading in &headings {
            s.push_str("<li class=\"lv");
            s.push(char::from(b'0' + heading.level.clamp(1, 6)));
            s.push_str("\"><a href=\"chapter-1.xhtml#");
            escape_xml_attr(&heading.id, &mut s);
            s.push_str("\">");
            escape_xml_text(&heading.text, &mut s);
            s.push_str("</a></li>\n");
        }
    }
    s.push_str("</ol>\n</nav>\n</body>\n</html>\n");
    s
}

fn content_opf(title: &str, lang: &str, identifier: &str) -> String {
    let mut s = String::with_capacity(1024);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n");
    s.push_str("<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n");
    s.push_str("<dc:identifier id=\"bookid\">");
    escape_xml_text(identifier, &mut s);
    s.push_str("</dc:identifier>\n<dc:title>");
    escape_xml_text(title, &mut s);
    s.push_str("</dc:title>\n<dc:language>");
    escape_xml_text(lang, &mut s);
    s.push_str("</dc:language>\n<meta property=\"dcterms:modified\">");
    s.push_str(DCTERMS_MODIFIED);
    s.push_str("</meta>\n</metadata>\n<manifest>\n");
    s.push_str("<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n");
    s.push_str(
        "<item id=\"chapter-1\" href=\"chapter-1.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
    );
    s.push_str("<item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n");
    s.push_str("</manifest>\n<spine>\n<itemref idref=\"chapter-1\"/>\n</spine>\n</package>\n");
    s
}

// ---------------------------------------------------------------------------
// Deterministic content identifier.

/// FNV-1a 64-bit over `bytes`, chained from `seed`.
fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = seed;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// Derive a deterministic `urn:uuid` identifier from the rendered content:
/// two FNV-1a 64 passes with distinct seeds give 128 bits, formatted in the
/// canonical 8-4-4-4-12 UUID shape.
fn content_identifier(title: &str, lang: &str, chapter: &str) -> String {
    const SEED_A: u64 = 0xcbf2_9ce4_8422_2325; // FNV offset basis
    const SEED_B: u64 = 0x9e37_79b9_7f4a_7c15; // golden-ratio constant
    let mut a = SEED_A;
    let mut b = SEED_B;
    for part in [title.as_bytes(), lang.as_bytes(), chapter.as_bytes()] {
        a = fnv1a64(a, part);
        b = fnv1a64(b, part);
    }
    format!(
        "urn:uuid:{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xFFFF,
        a & 0xFFFF,
        b >> 48,
        b & 0xFFFF_FFFF_FFFF
    )
}

// ---------------------------------------------------------------------------
// Heading walk for the nav document — mirrors the HTML renderer's TOC walk
// and anchor-id assignment exactly, so nav links match the chapter's `id`
// attributes by construction.

struct NavHeading {
    level: u8,
    text: String,
    id: String,
}

fn collect_headings(blocks: &[Block]) -> Vec<NavHeading> {
    let mut suffixes: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = Vec::new();
    walk_headings(blocks, &mut suffixes, &mut out);
    out
}

/// Container-aware walk matching `html.rs` `collect_toc_entries`: block
/// quotes and list items recurse; footnote definitions are skipped (they are
/// rendered outside the body flow and never receive body heading ids).
fn walk_headings(
    blocks: &[Block],
    suffixes: &mut BTreeMap<String, usize>,
    out: &mut Vec<NavHeading>,
) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                out.push(NavHeading {
                    level: *level,
                    text: inlines_to_plain(inlines),
                    id: assign_heading_id(inlines, suffixes),
                });
            }
            Block::BlockQuote(inner) => walk_headings(inner, suffixes, out),
            Block::List(list) => {
                for item in &list.items {
                    walk_headings(&item.blocks, suffixes, out);
                }
            }
            _ => {}
        }
    }
}

/// Mirror of the renderer's `push_heading_id_from_inlines`: first occurrence
/// gets the bare slug; later collisions get `-2`, `-3`, … suffixes.
fn assign_heading_id(inlines: &[Inline], suffixes: &mut BTreeMap<String, usize>) -> String {
    let mut base = slug_inlines(inlines);
    if base.is_empty() {
        base.push_str("section");
    }
    let mut suffix = suffixes.get(base.as_str()).copied().unwrap_or(1);
    loop {
        if suffix == 1 {
            suffix += 1;
            if !suffixes.contains_key(base.as_str()) {
                suffixes.insert(base.clone(), suffix);
                return base;
            }
            continue;
        }
        let candidate = format!("{base}-{suffix}");
        suffix += 1;
        if !suffixes.contains_key(candidate.as_str()) {
            suffixes.insert(candidate.clone(), 1);
            suffixes.insert(base, suffix);
            return candidate;
        }
    }
}

/// Mirror of the renderer's `slug_inlines`: ASCII alphanumerics lowercased;
/// spaces, dashes, and underscores collapse to single dashes; everything else
/// is dropped.
fn slug_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    let mut pending_dash = false;
    push_slug_inlines(inlines, &mut s, &mut pending_dash);
    s
}

fn push_slug_inlines(inlines: &[Inline], out: &mut String, pending_dash: &mut bool) {
    for inl in inlines {
        match inl {
            Inline::FootnoteRef { .. } => {}
            Inline::Text(t)
            | Inline::Code(t)
            | Inline::Html(t)
            | Inline::Math(t)
            | Inline::DisplayMath(t) => {
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

/// Mirror of the renderer's `inlines_to_plain`: visible text with breaks as
/// spaces, footnote refs as `[^id]`, image alt text inlined.
fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    push_inlines_to_plain(inlines, &mut s);
    s
}

fn push_inlines_to_plain(inlines: &[Inline], out: &mut String) {
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) | Inline::Math(t) | Inline::DisplayMath(t) => {
                out.push_str(t);
            }
            Inline::FootnoteRef { id } => {
                out.push_str("[^");
                out.push_str(id);
                out.push(']');
            }
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

/// Mirror of the renderer's title fallback: the first top-level heading's
/// plain text.
fn first_heading_text(doc: &Document) -> Option<String> {
    doc.blocks.iter().find_map(|b| match b {
        Block::Heading { inlines, .. } => Some(inlines_to_plain(inlines)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xhtml_void_element_conversion() {
        assert_eq!(
            html_fragment_to_xhtml("<p>line 1<br>line 2<br/>line 3<br />line 4</p>"),
            "<p>line 1<br/>line 2<br/>line 3<br/>line 4</p>"
        );
        assert_eq!(html_fragment_to_xhtml("<hr><hr/><hr />"), "<hr/><hr/><hr/>");
        assert_eq!(
            html_fragment_to_xhtml(
                "<img src=\"test.png\" alt=\"pic\"><img src=\"test.png\" alt=\"pic\" />"
            ),
            "<img src=\"test.png\" alt=\"pic\"/><img src=\"test.png\" alt=\"pic\"/>"
        );
        assert_eq!(
            html_fragment_to_xhtml("<input type=\"checkbox\" disabled checked>"),
            "<input type=\"checkbox\" disabled=\"disabled\" checked=\"checked\"/>"
        );
        assert_eq!(
            html_fragment_to_xhtml(
                "<input type=\"checkbox\" disabled=\"disabled\" checked=\"checked\"/>"
            ),
            "<input type=\"checkbox\" disabled=\"disabled\" checked=\"checked\"/>"
        );
    }

    #[test]
    fn content_identifier_format_and_stability() {
        let id1 = content_identifier("Doc Title", "en", "<p>Hello</p>");
        let id2 = content_identifier("Doc Title", "en", "<p>Hello</p>");
        let id3 = content_identifier("Doc Title", "fr", "<p>Bonjour</p>");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(id1.starts_with("urn:uuid:"));
        assert_eq!(id1.len(), 45); // urn:uuid: (9) + 8-4-4-4-12 (36) = 45
    }

    #[test]
    fn extract_main_body_boundaries() {
        let full = "<html><head></head><body>\n<main class=\"fmd\">\n<p>Content</p>\n</main>\n</body></html>";
        assert_eq!(extract_main_body(full), Some("<p>Content</p>\n"));
        assert_eq!(extract_main_body("invalid"), None);
    }

    // -----------------------------------------------------------------------
    // Differential oracles: the verbatim pre-bulk per-char reference bodies.

    fn html_fragment_to_xhtml_reference(html: &str) -> String {
        let mut out = String::with_capacity(html.len() + 16);
        let mut rest = html;
        while !rest.is_empty() {
            if let Some(stripped) = rest.strip_prefix("<br>") {
                out.push_str("<br/>");
                rest = stripped;
                continue;
            }
            if let Some(stripped) = rest.strip_prefix("<br/>") {
                out.push_str("<br/>");
                rest = stripped;
                continue;
            }
            if let Some(stripped) = rest.strip_prefix("<br />") {
                out.push_str("<br/>");
                rest = stripped;
                continue;
            }
            if let Some(stripped) = rest.strip_prefix("<hr>") {
                out.push_str("<hr/>");
                rest = stripped;
                continue;
            }
            if let Some(stripped) = rest.strip_prefix("<hr/>") {
                out.push_str("<hr/>");
                rest = stripped;
                continue;
            }
            if let Some(stripped) = rest.strip_prefix("<hr />") {
                out.push_str("<hr/>");
                rest = stripped;
                continue;
            }
            if rest.starts_with("<img ") || rest.starts_with("<input ") {
                match rest.find('>') {
                    Some(end) => {
                        let tag_body = rest[..end].trim_end_matches([' ', '/']);
                        push_void_tag_xhtml(tag_body, &mut out);
                        out.push_str("/>");
                        rest = &rest[end + 1..];
                    }
                    None => {
                        out.push_str(rest);
                        rest = "";
                    }
                }
                continue;
            }
            let ch_len = rest.chars().next().map_or(1, char::len_utf8);
            out.push_str(&rest[..ch_len]);
            rest = &rest[ch_len..];
        }
        out
    }

    fn escape_xml_text_reference(s: &str, out: &mut String) {
        for ch in s.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                _ => out.push(ch),
            }
        }
    }

    fn escape_xml_attr_reference(s: &str, out: &mut String) {
        for ch in s.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                _ => out.push(ch),
            }
        }
    }

    /// Hostile corpus for the escapers: every escapable character, dense
    /// mixes, multi-byte UTF-8 next to specials, empty input, long clean
    /// runs, and a boundary sweep placing a special at every offset around
    /// the scanner's 8/16/32-byte chunk edges.
    fn escape_hostile_corpus() -> Vec<String> {
        let mut cases: Vec<String> = vec![
            String::new(),
            "&".to_string(),
            "<".to_string(),
            ">".to_string(),
            "\"".to_string(),
            "'".to_string(),
            "&<>\"'".to_string(),
            "&<>&<>&<>".to_string(),
            "&amp; already named".to_string(),
            "\u{00e9}\u{4e2d}\u{6587}\u{1f680} caf\u{00e9}".to_string(),
            "&\u{1f680}<\"\u{e9}'\u{4e2d}>".to_string(),
            "\u{1f680}\u{1f680}\u{1f680}".to_string(),
            "a".repeat(4096),
            "clean ascii words only ".repeat(64),
            "\u{4e2d}\u{6587}".repeat(1024),
            "&".repeat(2048),
            "&f&f&f&f".repeat(256),
        ];
        for len in 0..=40usize {
            for pos in 0..len {
                for special in ["&", "<", ">", "\"", "'"] {
                    let mut s = String::from("z".repeat(len).as_str());
                    s.replace_range(pos..pos + 1, special);
                    cases.push(s);
                }
            }
        }
        for align in 0..64usize {
            let mut s = String::from("z".repeat(128).as_str());
            s.replace_range(align..align + 1, "\"");
            cases.push(s);
        }
        cases
    }

    #[test]
    fn xml_escapers_match_per_char_reference() {
        for case in escape_hostile_corpus() {
            let mut bulk = String::new();
            let mut reference = String::new();
            escape_xml_text(&case, &mut bulk);
            escape_xml_text_reference(&case, &mut reference);
            assert_eq!(bulk, reference, "text escape diverged for {case:?}");

            let mut bulk = String::new();
            let mut reference = String::new();
            escape_xml_attr(&case, &mut bulk);
            escape_xml_attr_reference(&case, &mut reference);
            assert_eq!(bulk, reference, "attr escape diverged for {case:?}");
        }
    }

    /// Hostile corpus for the fragment converter: all void-tag spellings,
    /// stray and truncated `<` openers, specials inside attribute values,
    /// multi-byte UTF-8, long clean runs, and the empty string.
    #[test]
    fn xhtml_conversion_matches_per_char_reference() {
        let mut cases: Vec<String> = vec![
            String::new(),
            "<p>plain paragraph & entity \u{2014} em dash</p>".to_string(),
            "text < stray bracket <b <br <bra <brx <img <input".to_string(),
            "<p>line 1<br>line 2<br/>line 3<br />line 4</p>".to_string(),
            "<hr><hr/><hr />".to_string(),
            "<img src=\"test.png\" alt=\"pic\"><img src=\"test.png\" alt=\"pic\" />".to_string(),
            "<img src=\"x.png\" alt=\"a<b & c>d 'quote' \\\"q\\\"\">".to_string(),
            "<input type=\"checkbox\" disabled checked>".to_string(),
            "<input type=\"checkbox\" disabled=\"disabled\" checked=\"checked\"/>".to_string(),
            "<img src=\"unterminated".to_string(),
            "<input value=\"no closing gt".to_string(),
            "<".to_string(),
            "<<<<".to_string(),
            "> alone".to_string(),
            "& alone".to_string(),
            "\u{4e2d}\u{6587}<br>\u{1f680}<hr/>\u{00e9}".to_string(),
            "z".repeat(4096),
            "\u{1f680}".repeat(1024),
            "<p>tail br<br>".to_string(),
            "<br><br/><br /><hr><hr/><hr />".to_string(),
        ];
        for len in 0..=40usize {
            for pos in 0..=len {
                let mut s = "y".repeat(len);
                s.insert_str(pos, "<br>");
                cases.push(s);
                let mut s = "y".repeat(len);
                s.insert(pos, '<');
                cases.push(s);
            }
        }
        for case in &cases {
            assert_eq!(
                html_fragment_to_xhtml(case),
                html_fragment_to_xhtml_reference(case),
                "fragment conversion diverged for {case:?}"
            );
        }
    }
}
