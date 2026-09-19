//! Native watch dependencies, derived from the same AST as rendering.
//!
//! An image is a dependency even before its file exists. Ordinary links keep
//! the historical existing-file behavior: a broken document link should not
//! allocate a watcher for an arbitrary URL. No target contents are read here.

use crate::ast::{Block, Inline};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) fn paths(markdown: &str, base_dir: &Path) -> Vec<PathBuf> {
    let document = crate::parse_markdown(markdown);
    let mut pending: Vec<_> = document.blocks.iter().rev().map(Node::Block).collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    while let Some(node) = pending.pop() {
        let destination = match node {
            Node::Block(Block::Paragraph(inlines) | Block::Heading { inlines, .. }) => {
                push_inlines(&mut pending, inlines);
                None
            }
            Node::Block(Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. }) => {
                pending.extend(blocks.iter().rev().map(Node::Block));
                None
            }
            Node::Block(Block::List(list)) => {
                for item in list.items.iter().rev() {
                    pending.extend(item.blocks.iter().rev().map(Node::Block));
                }
                None
            }
            Node::Block(Block::Table(table)) => {
                for row in table.rows.iter().rev() {
                    for cell in row.iter().rev() {
                        push_inlines(&mut pending, cell);
                    }
                }
                for cell in table.head.iter().rev() {
                    push_inlines(&mut pending, cell);
                }
                None
            }
            Node::Block(Block::DefinitionList(items)) => {
                for item in items.iter().rev() {
                    for inlines in item.terms.iter().chain(&item.definitions).rev() {
                        push_inlines(&mut pending, inlines);
                    }
                }
                None
            }
            Node::Inline(Inline::Emphasis(inlines) | Inline::Strong(inlines)
                | Inline::Strikethrough(inlines)) => {
                push_inlines(&mut pending, inlines);
                None
            }
            Node::Inline(Inline::Link { dest, content, .. }) => {
                push_inlines(&mut pending, content);
                Some((dest.as_str(), false))
            }
            Node::Inline(Inline::Image { dest, .. }) => Some((dest.as_str(), true)),
            // Code, math and raw HTML are never rescanned as Markdown.
            _ => None,
        };
        if let Some((dest, image)) = destination {
            if let Some(path) = local_path(dest, base_dir) {
                // Do not open directories, devices, sockets or pipes. A missing
                // image is retained so a later save can recover the preview.
                let watch = match std::fs::metadata(&path) {
                    Ok(metadata) => metadata.is_file(),
                    Err(error) => image && error.kind() == std::io::ErrorKind::NotFound,
                };
                if watch && seen.insert(path.clone()) {
                    out.push(path);
                }
            }
        }
    }
    out
}

enum Node<'a> {
    Block(&'a Block),
    Inline(&'a Inline),
}

fn push_inlines<'a>(pending: &mut Vec<Node<'a>>, inlines: &'a [Inline]) {
    pending.extend(inlines.iter().rev().map(Node::Inline));
}

/// Resolve URL spelling, not Markdown spelling (the parser already unescaped
/// that). Split URL suffixes BEFORE decoding so `%23`/`%3F` remain filename
/// characters; decode exactly once, and reject scheme/UNC/control escapes.
fn local_path(destination: &str, base_dir: &Path) -> Option<PathBuf> {
    let destination = destination.trim();
    let end = destination.find(['?', '#']).unwrap_or(destination.len());
    let raw = &destination[..end];
    if raw.is_empty() || unsafe_path(raw) {
        return None;
    }
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded).ok()?;
    if unsafe_path(&decoded) {
        return None;
    }
    // Do not canonicalize: nonexistent assets must remain watchable and `..`
    // must retain filesystem (including symlink) semantics.
    Some(base_dir.join(decoded))
}

fn unsafe_path(path: &str) -> bool {
    path.starts_with("//") || path.contains(['\\', ':']) || path.chars().any(char::is_control)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_suffixes_encoding_and_parent_paths_are_resolved_once() {
        let base = Path::new("guide");
        for (input, expected) in [
            ("../figures/plot.png?v=2#panel", "../figures/plot.png"),
            ("caf%C3%A9%20plot.svg", "café plot.svg"),
            ("figure%23one%3Ftwo.png#fragment", "figure#one?two.png"),
            ("100%25.png", "100%.png"),
            ("x%2523y.png", "x%23y.png"),
            ("plot(one).png", "plot(one).png"),
        ] {
            assert_eq!(local_path(input, base), Some(base.join(expected)), "{input}");
        }
    }

    #[test]
    fn schemes_unc_invalid_utf8_and_controls_never_become_local_paths() {
        for input in [
            "", "#local", "?version=2", "http://example.com/x", "HTTPS://example.com/x",
            "//example.com/x", "file:///etc/passwd", "mailto:a@b", "data:image/png,x",
            "javascript:x", "C:/image.png", "a\\b", "%2F%2Fhost/x", "%5Chost/x",
            "file%3Asecret", "a%00b", "a%0Ab", "a\nb", "%FF.png", "bad%", "bad%zz",
        ] {
            assert!(local_path(input, Path::new("guide")).is_none(), "{input:?}");
        }
    }
}
