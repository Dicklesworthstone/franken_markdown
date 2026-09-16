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
/// share one resource. Other URLs retain their existing renderer semantics.
pub(super) fn prepare(html: &str) -> Result<Chapter, &'static str> {
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
                                href: format!("assets/image-{}.{}", index + 1, extension),
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
        } else {
            chapter.body.push_str(tag);
        }
        rest = &rest[end + 1..];
    }
    chapter.body.push_str(rest);
    Ok(chapter)
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
    fn packages_and_deduplicates_images_in_first_use_order() {
        let html = "<img src=\"data:image/png;base64,AQID\" alt=\"one\"/><img src=\"data:image/jpeg;base64,BAU=\"/><img src=\"data:image/png;base64,AQID\"/>";
        let chapter = prepare(html).expect("valid resources");
        assert_eq!(chapter.resources.len(), 2);
        assert_eq!(chapter.resources[0].bytes, [1, 2, 3]);
        assert_eq!(chapter.resources[1].bytes, [4, 5]);
        assert_eq!(chapter.resources[1].media_type, "image/jpeg");
        assert_eq!(chapter.body.matches("assets/image-1.png").count(), 2);
        assert!(chapter.body.contains("assets/image-2.jpg"));
        assert!(!chapter.body.contains("data:"));
    }

    #[test]
    fn scans_real_tags_not_quoted_or_escaped_markup() {
        let html = "<p title='<math><svg> src=\"data:image/png;base64,AQID\"'>text</p>&lt;svg&gt;<img alt='a > b' src='data:image/png;base64,AQID'/>";
        let chapter = prepare(html).expect("valid markup");
        assert!(!chapter.mathml);
        assert!(!chapter.svg);
        assert_eq!(chapter.resources.len(), 1);
        assert!(chapter.body.contains("alt='a > b' src='assets/image-1.png'"));
    }

    #[test]
    fn detects_mathml_and_svg_including_referenced_svg() {
        let chapter = prepare(
            "<math><mi>x</mi></math><img src=\"data:image/svg+xml;base64,PHN2Zy8+\"/>",
        )
        .expect("valid markup");
        assert!(chapter.mathml);
        assert!(chapter.svg);
        assert_eq!(chapter.resources[0].bytes, b"<svg/>");
        assert!(prepare("<svg></svg>").expect("valid SVG").svg);
        assert!(!prepare("<mathematics/><svgish/>").expect("plain tags").svg);
    }

    #[test]
    fn leaves_external_unsupported_and_malformed_images_unchanged() {
        for html in [
            "<img src=\"https://example.com/a.png\"/>",
            "<img src=\"local.png\"/>",
            "<img src=\"data:text/plain;base64,AQID\"/>",
            "<img src=\"data:image/png;base64,!!!!\"/>",
            "<img src=\"unterminated",
        ] {
            let chapter = prepare(html).expect("pass-through");
            assert_eq!(chapter.body, html);
            assert!(chapter.resources.is_empty());
        }
    }

    #[test]
    fn strict_base64_padding_and_round_trips() {
        for (encoded, plain) in [("Zg==", "f"), ("Zm8=", "fo"), ("Zm9v", "foo")] {
            assert_eq!(decode_base64(encoded).as_deref(), Some(plain.as_bytes()));
        }
        for invalid in ["", "Zg=", "Zh==", "Zm9=", "Zg==AAAA", "=AAA", "A===", "!!!!"] {
            assert!(decode_base64(invalid).is_none(), "accepted {invalid}");
        }
    }
}
