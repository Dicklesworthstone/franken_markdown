//! Preserve file-relative resource destinations after native transclusion.
//!
//! Include expansion records exact source origins. The parser identifies real
//! Markdown destination tokens in the final expanded context, so rebasing does
//! not modify code, raw HTML, titles, or the spelling of reference labels. This
//! module operates only on strings: the native callers retain all filesystem
//! authorization, symlink checks, and read budgets.

use crate::book::resolve_book_destination;
use crate::transclude::ExpandedDocument;

/// Rewrite destinations from included files relative to the published source.
/// `logical_origin` maps canonical host keys to paths inside the same input
/// root. Root-source destinations retain their original Markdown spelling.
pub(crate) fn rebase_destinations(
    expanded: ExpandedDocument,
    output_source: &str,
    logical_origin: impl Fn(&str) -> Option<String>,
    max_output_bytes: usize,
) -> Result<String, String> {
    let mut edits = Vec::new();
    let mut output_len = expanded.text.len();
    for destination in crate::parse::destination_spans(&expanded.text) {
        let Some(location) = expanded.source_at(destination.range.start) else {
            continue;
        };
        let Some(origin) = logical_origin(location.origin) else {
            return Err(format!(
                "include_origin: cannot resolve resource origin {:?}",
                location.origin
            ));
        };
        // Both sources already use the same base directory. Avoid changing
        // harmless original spelling, escaping, or percent-encoding choices.
        if parent(&origin) == parent(output_source) {
            continue;
        }
        let Some(rebased) = rebase_url(&origin, output_source, &destination.dest) else {
            continue;
        };
        if rebased == destination.dest {
            continue;
        }
        let replacement = markdown_destination(&rebased);
        output_len = output_len
            .checked_sub(destination.range.len())
            .and_then(|len| len.checked_add(replacement.len()))
            .filter(|&len| len <= max_output_bytes)
            .ok_or_else(|| {
                format!("include_output_budget: rebased document exceeds {max_output_bytes} bytes")
            })?;
        edits.push((destination.range, replacement));
    }
    if edits.is_empty() {
        return Ok(expanded.text);
    }
    let mut output = String::with_capacity(output_len);
    let mut offset = 0;
    for (range, replacement) in edits {
        let Some(unchanged) = expanded.text.get(offset..range.start) else {
            return Err("include_origin: overlapping Markdown destination spans".into());
        };
        output.push_str(unchanged);
        output.push_str(&replacement);
        offset = range.end;
    }
    output.push_str(&expanded.text[offset..]);
    Ok(output)
}

fn parent(source: &str) -> &str {
    source.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn rebase_url(origin: &str, output_source: &str, destination: &str) -> Option<String> {
    let destination = destination.trim();
    // These URLs have no file-relative origin. The book resolver also rejects
    // schemes, malformed encodings, and paths outside the declared source root.
    if destination.starts_with(['/', '#', '?']) || destination.is_empty() {
        return None;
    }
    let end = destination.find(['#', '?']).unwrap_or(destination.len());
    let path = &destination[..end];
    let decoded = decode_path(path)?;
    if decoded.starts_with('/') {
        return None;
    }
    let directory =
        decoded.ends_with('/') || matches!(decoded.rsplit('/').next(), Some("." | ".."));
    let (target, suffix) = if directory {
        // The shared resolver validates file paths. A synthetic leaf lets it
        // normalize directory aliases without changing their trailing slash,
        // which book navigation uses to choose index/README chapters.
        let with_leaf = format!("{path}/.__fmd_origin_leaf__{}", &destination[end..]);
        let (target, suffix) = resolve_book_destination(origin, &with_leaf)?;
        let target = target.strip_suffix(".__fmd_origin_leaf__")?.to_owned();
        (target, suffix)
    } else {
        resolve_book_destination(origin, destination)?
    };
    let from: Vec<_> = parent(output_source)
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to: Vec<_> = target.split('/').filter(|part| !part.is_empty()).collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = vec![".."; from.len() - common];
    relative.extend_from_slice(&to[common..]);
    let mut path = relative.join("/");
    if directory {
        if path.is_empty() {
            path.push('.');
        }
        path.push('/');
    }
    Some(format!("{}{suffix}", encode_path(&path)))
}

/// Decode URI path bytes exactly once. Callers separately validate path
/// authority, traversal, controls, regular-file status and containment.
pub(crate) fn decode_path(path: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let mut bytes = path.bytes();
    let mut decoded = Vec::with_capacity(path.len());
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            decoded.push((hex(bytes.next()?)? << 4) | hex(bytes.next()?)?);
        } else {
            decoded.push(byte);
        }
    }
    String::from_utf8(decoded).ok()
}

fn encode_path(path: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

/// An angle destination allows spaces and balanced/unbalanced parentheses.
/// Encode Markdown-sensitive characters without changing the decoded URL.
fn markdown_destination(destination: &str) -> String {
    let mut output = String::with_capacity(destination.len() + 2);
    output.push('<');
    for character in destination.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("&#10;"),
            '\r' => output.push_str("&#13;"),
            character => output.push(character),
        }
    }
    output.push('>');
    output
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn rebasing_preserves_url_suffixes_and_decodes_paths_exactly_once() {
        for (destination, expected) in [
            ("chart.svg?v=2#panel", "parts/chart.svg?v=2#panel"),
            ("../next.md#heading", "next.md#heading"),
            ("space%20name.svg", "parts/space%20name.svg"),
            ("percent%2520.svg", "parts/percent%2520.svg"),
            ("中%23x.svg#view", "parts/%E4%B8%AD%23x.svg#view"),
            ("../guide/?mode=print#top", "guide/?mode=print#top"),
            (".%2F", "parts/"),
            ("..", "./"),
        ] {
            assert_eq!(
                rebase_url("parts/snippet.md", "root.md", destination).as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            rebase_url("shared/snippet.md", "chapters/root.md", "next.md").as_deref(),
            Some("../shared/next.md")
        );
    }

    #[test]
    fn nonlocal_and_escaping_destinations_do_not_gain_native_authority() {
        for destination in [
            "#local",
            "?query",
            "/root.md",
            "//host/path",
            "https://host/path",
            "mailto:person@example.com",
            "custom+v1:value",
            "%2froot.md",
            "%2f%2fhost/path",
            "../../secret.svg",
            "%2e%2e/%2e%2e/secret.svg",
            "%68ttps%3a/remote.svg",
            "bad%ZZ.svg",
            "nul%00.svg",
            "..\\secret.svg",
        ] {
            assert!(
                rebase_url("parts/snippet.md", "root.md", destination).is_none(),
                "{destination}"
            );
        }
    }

    #[test]
    fn markdown_encoding_preserves_the_parsers_destination_value() {
        let destination = "parts/a%20b.svg?a=&copy;#<view>\\x\n";
        let source = format!("![x]({} \"caption\")", markdown_destination(destination));
        let parsed = crate::parse::parse_inlines(&source);
        assert!(
            matches!(&parsed[..], [crate::Inline::Image { dest, title, .. }]
            if dest == destination && title.as_deref() == Some("caption"))
        );
    }

    #[test]
    fn expanded_origin_rewrites_resources_but_preserves_original_markdown_elsewhere() {
        let source = "# Root\n\n[direct](root.md)\n\n{{#include parts.md}}\n";
        let included = "## Snippet\n\n![image][chart]\n\n[chart]: <chart.svg#view> 'Title'\n\n`![literal](chart.svg)`\n\n```md\n[code](next.md)\n```\n";
        let expanded = crate::transclude::expand_includes_mapped(source, "root.md", &|_, _| {
            Ok(Some((included.into(), "parts/snippet.md".into())))
        })
        .unwrap();
        let rebased =
            rebase_destinations(expanded, "root.md", |origin| Some(origin.into()), 4096).unwrap();
        assert!(rebased.contains("[direct](root.md)"));
        assert!(rebased.contains("[chart]: <parts/chart.svg#view> 'Title'"));
        assert!(rebased.contains("`![literal](chart.svg)`"));
        assert!(rebased.contains("```md\n[code](next.md)\n```"));
    }

    #[test]
    fn rebasing_is_charged_to_the_same_expanded_output_budget() {
        let expanded =
            crate::transclude::expand_includes_mapped("{{#include part}}", "root.md", &|_, _| {
                Ok(Some((
                    "![x](a.svg)".into(),
                    "long_directory/part.md".into(),
                )))
            })
            .unwrap();
        let budget = expanded.text.len();
        assert!(
            rebase_destinations(expanded, "root.md", |origin| Some(origin.into()), budget)
                .unwrap_err()
                .contains("include_output_budget")
        );
    }
}
