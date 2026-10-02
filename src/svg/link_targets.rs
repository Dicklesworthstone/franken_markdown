//! Bounded navigation metadata. It never fetches a destination or parses markup.

use super::super::Inline;

pub(super) const MAX_TARGET_BYTES: usize = 4096;
const MAX_LABEL_CHARS: usize = 256;
const MAX_SLUG_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Target {
    External(String),
    Fragment(String),
}

/// Deliberately narrower than HTML: a portable poster has no document base URL.
/// Keep relative paths, executable/custom schemes and obfuscated URLs inert.
/// Fragments are decoded once and later resolved ONLY against heading slugs.
pub(super) fn target(raw: &str) -> Option<Target> {
    if raw.len() > MAX_TARGET_BYTES {
        return None;
    }
    let raw = raw.trim_matches(' ');
    if raw
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace() || invalid_xml(ch))
    {
        return None;
    }
    if let Some(fragment) = raw.strip_prefix('#') {
        let decoded = percent_decode(fragment)?;
        if decoded.chars().any(|ch| ch.is_control() || invalid_xml(ch)) {
            return None;
        }
        return Some(Target::Fragment(decoded));
    }
    let (scheme, rest) = raw.split_once(':')?;
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        let authority = rest.strip_prefix("//")?.split(['/', '?', '#']).next()?;
        if authority.is_empty() || raw.contains('\\') {
            return None;
        }
    } else if scheme.eq_ignore_ascii_case("mailto") {
        if rest.is_empty() || rest.starts_with("//") || rest.contains('\\') {
            return None;
        }
    } else {
        return None;
    }
    // Refuse malformed escapes and encoded controls as well as literal ones.
    // Do not replace the URL with this decoded string: %2F and %23 have meaning.
    let decoded = percent_decode(raw)?;
    if decoded.chars().any(|ch| ch.is_control() || invalid_xml(ch)) {
        return None;
    }
    Some(Target::External(raw.to_owned()))
}

fn percent_decode(input: &str) -> Option<String> {
    let source = input.as_bytes();
    let mut out = Vec::with_capacity(source.len());
    let mut i = 0;
    while i < source.len() {
        if source[i] == b'%' {
            let high = hex(*source.get(i + 1)?)?;
            let low = hex(*source.get(i + 2)?)?;
            out.push(high * 16 + low);
            i += 3;
        } else {
            out.push(source[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid_xml(ch: char) -> bool {
    // Reject noncharacters too, even where an XML version would admit them.
    let code = ch as u32;
    code & 0xffff >= 0xfffe || (0xfdd0..=0xfdef).contains(&code)
}

pub(super) fn text_label(text: &str) -> String {
    text.chars().take(MAX_LABEL_CHARS).map(label_char).collect()
}

fn label_char(ch: char) -> char {
    if ch.is_control() {
        ' '
    } else if invalid_xml(ch) {
        '\u{fffd}'
    } else {
        ch
    }
}

pub(super) fn inline_label(inlines: &[Inline]) -> String {
    let mut out = String::new();
    let mut remaining = MAX_LABEL_CHARS;
    walk_text(inlines, true, |part| {
        for ch in part.chars() {
            if remaining == 0 {
                return false;
            }
            out.push(label_char(ch));
            remaining -= 1;
        }
        true
    });
    out
}

/// Match html::slug_inlines: ASCII alphanumerics, lowercase, deferred hyphens
/// for space/hyphen/underscore, no footnote reference text, and `section` for
/// an empty slug. Refuse over-budget metadata rather than truncate into a
/// different heading's identity. The source heading is still painted normally.
pub(super) fn heading_slug(inlines: &[Inline]) -> Option<String> {
    let mut out = String::new();
    let mut dash = false;
    let mut remaining = 64 * 1024usize;
    let complete = walk_text(inlines, false, |part| {
        if part.len() > remaining {
            return false;
        }
        remaining -= part.len();
        for ch in part.chars() {
            if ch.is_ascii_alphanumeric() {
                let extra = usize::from(dash && !out.is_empty()) + 1;
                if out.len() + extra > MAX_SLUG_BYTES {
                    return false;
                }
                if dash && !out.is_empty() {
                    out.push('-');
                }
                out.push(ch.to_ascii_lowercase());
                dash = false;
            } else if matches!(ch, ' ' | '-' | '_') {
                dash = true;
            }
        }
        true
    });
    if !complete {
        return None;
    }
    if out.is_empty() {
        out.push_str("section");
    }
    Some(out)
}

/// Metadata-only walk: bounded stack and node count, including empty nodes.
/// Rendering still uses the original AST and is not truncated by these limits.
fn walk_text(inlines: &[Inline], notes: bool, mut visit: impl FnMut(&str) -> bool) -> bool {
    let mut stack = vec![inlines.iter()];
    let mut count = 0;
    while let Some(iter) = stack.last_mut() {
        let Some(inline) = iter.next() else {
            stack.pop();
            continue;
        };
        count += 1;
        if count > 8192 {
            return false;
        }
        let keep_going = match inline {
            Inline::Text(text)
            | Inline::Code(text)
            | Inline::Html(text)
            | Inline::Math(text)
            | Inline::DisplayMath(text) => visit(text),
            Inline::Image { alt, .. } => visit(alt),
            Inline::SoftBreak | Inline::HardBreak => visit(" "),
            Inline::FootnoteRef { id } => !notes || (visit("[^") && visit(id) && visit("]")),
            Inline::Emphasis(inner)
            | Inline::Strong(inner)
            | Inline::Strikethrough(inner)
            | Inline::Link { content: inner, .. } => {
                if stack.len() >= 128 {
                    return false;
                }
                stack.push(inner.iter());
                true
            }
        };
        if !keep_going {
            return false;
        }
    }
    true
}
