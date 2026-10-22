//! Shared heading and fragment normalization for rendering and navigation.
//!
//! Heading slugs retain Unicode letters in lowercase and ASCII digits. Spaces,
//! hyphens, and underscores separate words; other punctuation is omitted.
//! Normalization does not assign collision suffixes or normalize Unicode
//! composition. Fragment lookup is case-sensitive and percent-decodes once.

use crate::Inline;

/// Normalize a heading's visible inline content into a base anchor.
///
/// Inline markup contributes its visible text, images contribute their alt
/// text, and footnote markers do not change a heading's identity. An empty
/// result is left empty so callers can apply their `section` fallback before
/// assigning document-local collision suffixes.
#[must_use]
pub fn heading_slug(inlines: &[Inline]) -> String {
    fn walk(inlines: &[Inline], out: &mut String, pending_dash: &mut bool) {
        for inline in inlines {
            match inline {
                Inline::FootnoteRef { .. } => {}
                Inline::Text(text)
                | Inline::Code(text)
                | Inline::Math(text)
                | Inline::DisplayMath(text) => push_slug_text(out, pending_dash, text),
                Inline::Html(html) => {
                    push_slug_text(out, pending_dash, &crate::safe_html::visible_text(html));
                }
                Inline::Emphasis(inner)
                | Inline::Strong(inner)
                | Inline::Strikethrough(inner)
                | Inline::Link { content: inner, .. } => walk(inner, out, pending_dash),
                Inline::Image { alt, .. } => push_slug_text(out, pending_dash, alt),
                Inline::SoftBreak | Inline::HardBreak => push_slug_char(out, pending_dash, ' '),
            }
        }
    }
    let mut out = String::new();
    walk(inlines, &mut out, &mut false);
    out
}

/// Normalize visible heading text using the same rules as [`heading_slug`].
#[must_use]
pub fn text_slug(text: &str) -> String {
    let mut out = String::new();
    push_slug_text(&mut out, &mut false, text);
    out
}

fn push_slug_text(out: &mut String, pending_dash: &mut bool, text: &str) {
    // Preserve the inexpensive ASCII path used for ordinary documentation.
    for ch in text.chars() {
        push_slug_char(out, pending_dash, ch);
    }
}

pub(crate) fn push_slug_char(out: &mut String, pending_dash: &mut bool, ch: char) {
    if ch.is_ascii_alphanumeric() {
        if *pending_dash && !out.is_empty() {
            out.push('-');
        }
        out.push(ch.to_ascii_lowercase());
        *pending_dash = false;
    } else if ch.is_alphabetic() {
        if *pending_dash && !out.is_empty() {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
        *pending_dash = false;
    } else if matches!(ch, ' ' | '-' | '_') {
        *pending_dash = true;
    }
}

/// Percent-decode a URL fragment exactly once, rejecting malformed escapes,
/// invalid UTF-8, and controls. The leading `#` is not part of the input.
#[must_use]
pub fn decode_fragment(fragment: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let source = fragment.as_bytes();
    let mut bytes = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%' {
            bytes.push(hex(*source.get(index + 1)?)? * 16 + hex(*source.get(index + 2)?)?);
            index += 3;
        } else {
            bytes.push(source[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(bytes).ok()?;
    (!decoded.chars().any(char::is_control)).then_some(decoded)
}
