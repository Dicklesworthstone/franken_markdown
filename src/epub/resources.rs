//! Deterministic extraction of embedded image data into EPUB resources.
//!
//! This scans the renderer's own markup, not arbitrary user HTML. Text is
//! already escaped and raw-HTML pass-through is disabled by the EPUB caller.

use std::collections::BTreeMap;
use std::ops::Range;

const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_IMAGES: usize = 4096;

pub(super) struct Resource {
    pub href: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

pub(super) struct Chapter {
    pub body: String,
    pub resources: Vec<Resource>,
    pub mathml: bool,
    pub svg: bool,
}

/// Package supported base64 image URLs in first-use order. Identical URLs
/// share one resource. An unavailable image becomes an explicit, accessible
/// text placeholder: EPUB requires images inside the container, and this
/// pure core cannot fetch an unresolved local or remote destination.
pub(super) fn prepare(html: &str) -> Result<Chapter, &'static str> {
    prepare_with_prefix(html, "")
}

/// Use a renderer-generated namespace for images belonging to one chapter.
/// Prefixes are deliberately restricted to filename characters, not paths.
pub(super) fn prepare_with_prefix(html: &str, prefix: &str) -> Result<Chapter, &'static str> {
    if prefix.len() > 64
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("epub: invalid generated image prefix");
    }
    let mut chapter = Chapter {
        body: String::with_capacity(html.len()),
        resources: Vec::new(),
        mathml: false,
        svg: false,
    };
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total_bytes = 0usize;
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        chapter.body.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = tag_end(rest) else {
            chapter.body.push_str(rest);
            rest = "";
            break;
        };
        let tag = &rest[..=end];
        chapter.mathml |= opens(tag, "math");
        chapter.svg |= opens(tag, "svg");
        let mut replacement = None;
        if opens(tag, "img") {
            if let Some(range) = attribute(tag, "src") {
                let src = &tag[range.clone()];
                if let Some((media_type, extension, payload)) = data_image(src) {
                    chapter.svg |= media_type == "image/svg+xml";
                    let index = if let Some(&index) = seen.get(src) {
                        Some(index)
                    } else {
                        if payload.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
                            return Err("epub: embedded image exceeds the 32 MiB limit");
                        }
                        if let Some(bytes) = decode_base64(payload) {
                            if bytes.len() > MAX_IMAGE_BYTES {
                                return Err("epub: embedded image exceeds the 32 MiB limit");
                            }
                            total_bytes += bytes.len();
                            if total_bytes > MAX_TOTAL_IMAGE_BYTES {
                                return Err("epub: embedded images exceed the 128 MiB limit");
                            }
                            if chapter.resources.len() >= MAX_IMAGES {
                                return Err("epub: embedded images exceed the 4096-resource limit");
                            }
                            let index = chapter.resources.len();
                            chapter.resources.push(Resource {
                                href: format!("assets/{prefix}image-{}.{}", index + 1, extension),
                                media_type,
                                bytes,
                            });
                            seen.insert(src, index);
                            Some(index)
                        } else {
                            // Do not reinterpret malformed or unsupported user data.
                            None
                        }
                    };
                    if let Some(index) = index {
                        replacement = Some((range, index));
                    }
                }
            }
        }
        if let Some((range, index)) = replacement {
            chapter.body.push_str(&tag[..range.start]);
            chapter.body.push_str(&chapter.resources[index].href);
            chapter.body.push_str(&tag[range.end..]);
        } else if opens(tag, "img") {
            push_image_fallback(tag, &mut chapter.body);
        } else {
            chapter.body.push_str(tag);
        }
        rest = &rest[end + 1..];
    }
    chapter.body.push_str(rest);
    Ok(chapter)
}

/// Preserve the image's alternative text and source without leaving an
/// unmanifested resource reference in an otherwise readable publication.
/// Attribute values here have already been escaped by the HTML renderer.
/// Keep their entity references intact; escape raw quotes/angles as well so
/// the scanner's accepted single-quoted values remain safe when rewrapped.
fn push_image_fallback(tag: &str, out: &mut String) {
    fn value(value: &str, out: &mut String) {
        for part in value.split_inclusive(['<', '>', '"']) {
            if let Some((last, prefix)) = part.as_bytes().split_last() {
                let replacement = match last {
                    b'<' => "&lt;",
                    b'>' => "&gt;",
                    b'"' => "&quot;",
                    _ => {
                        out.push_str(part);
                        continue;
                    }
                };
                out.push_str(&part[..prefix.len()]);
                out.push_str(replacement);
            }
        }
    }

    let alt = attribute(tag, "alt").map(|range| &tag[range]);
    out.push_str(
        "<span class=\"fmd-image-unavailable\" role=\"img\" aria-label=\"Image unavailable",
    );
    if let Some(alt) = alt.filter(|text| !text.is_empty()) {
        out.push_str(": ");
        value(alt, out);
    }
    out.push('"');
    for (original, name) in [("src", "data-image-source"), ("title", "title")] {
        if let Some(range) = attribute(tag, original) {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            value(&tag[range], out);
            out.push('"');
        }
    }
    out.push_str(">[Image unavailable");
    if let Some(alt) = alt.filter(|text| !text.is_empty()) {
        out.push_str(": ");
        value(alt, out);
    }
    out.push_str("]</span>");
}

/// The first unquoted `>`; quoted attribute values may contain `>` or `<`.
pub(super) fn tag_end(tag: &str) -> Option<usize> {
    let mut quote = None;
    for (index, byte) in tag.bytes().enumerate() {
        match (quote, byte) {
            (None, b'\'' | b'"') => quote = Some(byte),
            (Some(expected), actual) if expected == actual => quote = None,
            (None, b'>') => return Some(index),
            _ => {}
        }
    }
    None
}

fn opens(tag: &str, name: &str) -> bool {
    tag.strip_prefix('<')
        .and_then(|rest| rest.strip_prefix(name))
        .and_then(|rest| rest.as_bytes().first())
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
}

/// Locate an exact quoted attribute without matching text inside other values.
fn attribute(tag: &str, wanted: &str) -> Option<Range<usize>> {
    let bytes = tag.as_bytes();
    let mut i = 1;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
        i += 1;
    }
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && !matches!(bytes[i], b'=' | b'/' | b'>')
        {
            i += 1;
        }
        if i == start {
            return None;
        }
        let name = &tag[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = *bytes.get(i)?;
        if !matches!(quote, b'\'' | b'"') {
            return None;
        }
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        if i == bytes.len() {
            return None;
        }
        if name == wanted {
            return Some(start..i);
        }
        i += 1;
    }
    None
}

fn data_image(src: &str) -> Option<(&'static str, &'static str, &str)> {
    let (header, payload) = src.strip_prefix("data:")?.split_once(',')?;
    let mime = header.strip_suffix(";base64")?;
    let (media_type, extension) = match mime {
        "image/png" => ("image/png", "png"),
        "image/jpeg" => ("image/jpeg", "jpg"),
        "image/gif" => ("image/gif", "gif"),
        "image/svg+xml" => ("image/svg+xml", "svg"),
        _ => return None,
    };
    Some((media_type, extension, payload))
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    if input.is_empty() || input.len() & 3 != 0 {
        return None;
    }
    fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    for (index, chunk) in input.as_bytes().chunks_exact(4).enumerate() {
        let a = digit(chunk[0])?;
        let b = digit(chunk[1])?;
        let last = (index + 1) * 4 == input.len();
        out.push((a << 2) | (b >> 4));
        if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' || b & 15 != 0 {
                return None;
            }
        } else {
            let c = digit(chunk[2])?;
            out.push((b << 4) | (c >> 2));
            if chunk[3] == b'=' {
                if !last || c & 3 != 0 {
                    return None;
                }
            } else {
                out.push((c << 6) | digit(chunk[3])?);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_and_deduplicates_images_in_first_use_order() -> Result<(), &'static str> {
        let html = "<img src=\"data:image/png;base64,AQID\" alt=\"one\"/><img src=\"data:image/jpeg;base64,BAU=\"/><img src=\"data:image/png;base64,AQID\"/>";
        let chapter = prepare(html)?;
        assert_eq!(chapter.resources.len(), 2);
        assert_eq!(chapter.resources[0].bytes, [1, 2, 3]);
        assert_eq!(chapter.resources[1].bytes, [4, 5]);
        assert_eq!(chapter.resources[1].media_type, "image/jpeg");
        assert_eq!(chapter.body.matches("assets/image-1.png").count(), 2);
        assert!(chapter.body.contains("assets/image-2.jpg"));
        assert!(!chapter.body.contains("data:"));
        Ok(())
    }

    #[test]
    fn scans_real_tags_not_quoted_or_escaped_markup() -> Result<(), &'static str> {
        let html = "<p title='<math><svg> src=\"data:image/png;base64,AQID\"'>text</p>&lt;svg&gt;<img alt='a > b' src='data:image/png;base64,AQID'/>";
        let chapter = prepare(html)?;
        assert!(!chapter.mathml);
        assert!(!chapter.svg);
        assert_eq!(chapter.resources.len(), 1);
        assert!(
            chapter
                .body
                .contains("alt='a > b' src='assets/image-1.png'")
        );
        Ok(())
    }

    #[test]
    fn detects_mathml_and_svg_including_referenced_svg() -> Result<(), &'static str> {
        let chapter =
            prepare("<math><mi>x</mi></math><img src=\"data:image/svg+xml;base64,PHN2Zy8+\"/>")?;
        assert!(chapter.mathml);
        assert!(chapter.svg);
        assert_eq!(chapter.resources[0].bytes, b"<svg/>");
        assert!(prepare("<svg></svg>")?.svg);
        assert!(!prepare("<mathematics/><svgish/>")?.svg);
        Ok(())
    }

    #[test]
    fn unavailable_images_keep_visible_alternatives_without_resource_requests()
    -> Result<(), &'static str> {
        for html in [
            "<img src=\"https://example.com/a.png\"/>",
            "<img src=\"local.png\"/>",
            "<img src=\"data:text/plain;base64,AQID\"/>",
            "<img src=\"data:image/png;base64,!!!!\"/>",
        ] {
            let chapter = prepare(html)?;
            assert!(chapter.body.contains("[Image unavailable]</span>"));
            assert!(chapter.body.contains("role=\"img\""));
            assert!(!chapter.body.contains("<img"));
            let source = &html[attribute(html, "src").ok_or("missing source")?];
            assert!(chapter.body.contains(source));
            assert!(chapter.resources.is_empty());
        }
        // Truncated markup cannot be emitted by the HTML renderer. Do not
        // mistake partial attributes for an image that can be rewritten.
        assert_eq!(
            prepare("<img src=\"unterminated")?.body,
            "<img src=\"unterminated"
        );
        Ok(())
    }

    #[test]
    fn image_fallback_retains_escaped_alt_title_and_enclosing_link() -> Result<(), &'static str> {
        let html = "<a href=\"https://example.com/details\"><img src=\"missing.png?a=1&amp;b=2\" alt=\"A &amp; B &lt;tag&gt;\" title=\"Original &quot;title&quot;\"/></a>";
        let chapter = prepare(html)?;
        assert!(
            chapter
                .body
                .starts_with("<a href=\"https://example.com/details\">")
        );
        assert!(chapter.body.ends_with("]</span></a>"));
        assert!(
            chapter
                .body
                .contains("data-image-source=\"missing.png?a=1&amp;b=2\"")
        );
        assert!(
            chapter
                .body
                .contains("title=\"Original &quot;title&quot;\"")
        );
        assert!(
            chapter
                .body
                .contains("aria-label=\"Image unavailable: A &amp; B &lt;tag&gt;\"")
        );
        assert!(
            chapter
                .body
                .contains("[Image unavailable: A &amp; B &lt;tag&gt;]")
        );
        let quotes = prepare("<img src='missing\".png' alt='a < b > c'/> ")?;
        assert!(
            quotes
                .body
                .contains("data-image-source=\"missing&quot;.png\"")
        );
        assert!(quotes.body.contains("[Image unavailable: a &lt; b &gt; c]"));
        Ok(())
    }

    #[test]
    fn strict_base64_padding_and_round_trips() {
        for (encoded, plain) in [("Zg==", "f"), ("Zm8=", "fo"), ("Zm9v", "foo")] {
            assert_eq!(decode_base64(encoded).as_deref(), Some(plain.as_bytes()));
        }
        for invalid in [
            "", "Zg=", "Zh==", "Zm9=", "Zg==AAAA", "=AAA", "A===", "!!!!",
        ] {
            assert!(decode_base64(invalid).is_none(), "accepted {invalid}");
        }
    }

    #[test]
    fn exact_src_attribute_with_boolean_and_unicode_neighbors() -> Result<(), &'static str> {
        let html =
            "<img hidden data-src='ignore' alt='中文 🚀'\n src = \"data:image/gif;base64,AQID\"/>";
        let chapter = prepare(html)?;
        assert_eq!(chapter.resources.len(), 1);
        assert_eq!(chapter.resources[0].media_type, "image/gif");
        assert!(chapter.body.contains("data-src='ignore' alt='中文 🚀'"));
        assert!(chapter.body.contains("src = \"assets/image-1.gif\""));
        assert!(
            prepare("<img data-src='data:image/png;base64,AQID'/>")?
                .resources
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn resource_limit_counts_unique_images_not_references() -> Result<(), &'static str> {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut html = String::new();
        for index in 0..MAX_IMAGES {
            let a = char::from(ALPHABET[index >> 6]);
            let b = char::from(ALPHABET[index & 63]);
            html.push_str(&format!("<img src='data:image/png;base64,{a}{b}AA'/>"));
        }
        assert_eq!(prepare(&html)?.resources.len(), MAX_IMAGES);
        html.push_str("<img src='data:image/png;base64,AAAA'/>");
        assert_eq!(prepare(&html)?.resources.len(), MAX_IMAGES);
        html.push_str("<img src='data:image/png;base64,AAAB'/>");
        assert!(matches!(prepare(&html), Err(message) if message.contains("4096-resource")));
        Ok(())
    }

    #[test]
    fn chapter_prefixes_are_distinct_and_cannot_escape_assets() -> Result<(), &'static str> {
        let html = "<img src='data:image/png;base64,AQID'/>";
        let first = prepare_with_prefix(html, "chapter-1-")?;
        let second = prepare_with_prefix(html, "chapter-2-")?;
        assert_eq!(first.resources[0].href, "assets/chapter-1-image-1.png");
        assert_eq!(second.resources[0].href, "assets/chapter-2-image-1.png");
        assert!(prepare_with_prefix(html, "../").is_err());
        assert!(prepare_with_prefix(html, "bad\"").is_err());
        Ok(())
    }
}
