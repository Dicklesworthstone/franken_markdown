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

/// Cache only explicit Markdown roots, never recursively crawl linked files.
/// An unreadable/unstable source retains its last successful dependency set.
#[derive(Default)]
pub(super) struct Graph {
    roots: BTreeSet<PathBuf>,
    documents: std::collections::BTreeMap<PathBuf, Snapshot>,
    failures: BTreeSet<PathBuf>,
}

struct Snapshot {
    fingerprint: super::Fingerprint,
    paths: Vec<PathBuf>,
    includes: std::collections::BTreeMap<PathBuf, Option<super::Fingerprint>>,
    complete: bool,
}

const MAX_DISCOVERY_BYTES: u64 = 64 * 1024 * 1024;

impl Graph {
    pub(super) fn new(roots: &[PathBuf]) -> Self {
        Self { roots: roots.iter().cloned().collect(), ..Self::default() }
    }

    pub(super) fn add_root(&mut self, path: PathBuf) {
        self.roots.insert(path);
    }

    pub(super) fn failures(&self) -> &BTreeSet<PathBuf> {
        &self.failures
    }

    pub(super) fn refresh(
        &mut self,
        observed: &std::collections::BTreeMap<PathBuf, Option<super::Fingerprint>>,
        settling: &BTreeSet<PathBuf>,
    ) -> BTreeSet<PathBuf> {
        for root in &self.roots {
            let old = self.documents.get(root);
            if !is_markdown(root) || settling.contains(root)
                || old.is_some_and(|old| old.includes.keys().any(|path| settling.contains(path)))
            {
                // Do not drop old edges during an unsettled edit. If the edit
                // is undone, changed assets must still have their old baseline.
                continue;
            }
            let Some(expected) = observed.get(root).copied().flatten() else {
                self.failures.insert(root.clone());
                continue;
            };
            if old.is_some_and(|old| old.fingerprint == expected
                && old.includes.iter().all(|(path, expected)| observed.get(path) == Some(expected)))
            {
                if old.is_some_and(|old| old.complete) { self.failures.remove(root); }
                continue;
            }
            let Some(source) = read_source(root, expected, MAX_DISCOVERY_BYTES) else {
                // In particular, do not cache `expected` after a racing save:
                // the next poll must retry even if that fingerprint repeats.
                self.failures.insert(root.clone());
                continue;
            };
            let base = root.parent().unwrap_or_else(|| Path::new("."));
            let expansion = super::expand_file_includes(&source, root, MAX_DISCOVERY_BYTES);
            let complete = expansion.result.is_ok();
            let mut paths = paths(expansion.result.as_deref().unwrap_or(&source), base);
            if !complete {
                // Retain working asset edges during a missing include or bad
                // save, but also watch newly discovered include destinations.
                if let Some(old) = old { paths.extend(old.paths.iter().cloned()); }
                self.failures.insert(root.clone());
            } else {
                self.failures.remove(root);
            }
            paths.extend(expansion.dependencies.keys().cloned());
            self.documents.insert(root.clone(), Snapshot {
                fingerprint: expected, paths, includes: expansion.dependencies, complete,
            });
        }
        let mut needed = self.roots.clone();
        for snapshot in self.documents.values() {
            needed.extend(snapshot.paths.iter().cloned());
        }
        needed
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md")
            || extension.eq_ignore_ascii_case("markdown"))
}

/// Read a bounded, valid UTF-8 snapshot that matches the content observed by
/// the polling pass. A second filesystem read must not silently bind a new
/// graph to an old fingerprint during a write-temp-then-rename save.
fn read_source(path: &Path, expected: super::Fingerprint, limit: u64) -> Option<String> {
    use std::io::Read;
    if expected.len > limit || !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > limit || bytes.len() as u64 != expected.len {
        return None;
    }
    let hash = bytes.iter().fold(super::FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(super::FNV_PRIME)
    });
    if hash != expected.hash {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Source(PathBuf);
    impl Source {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "fmd-watch-snapshot-{}-{}.md", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file = std::fs::File::create_new(&path).unwrap();
            std::io::Write::write_all(&mut file, bytes).unwrap();
            Self(path)
        }
        fn fingerprint(&self) -> super::super::Fingerprint {
            super::super::fingerprint(&self.0).unwrap()
        }
    }
    impl Drop for Source {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
    }

    #[test]
    fn snapshot_rejects_same_length_races_and_invalid_utf8() {
        let source = Source::new(b"first");
        let expected = source.fingerprint();
        assert_eq!(read_source(&source.0, expected, 5).as_deref(), Some("first"));
        std::fs::write(&source.0, b"other").unwrap();
        assert!(read_source(&source.0, expected, 5).is_none());
        std::fs::write(&source.0, [0xFF]).unwrap();
        assert!(read_source(&source.0, source.fingerprint(), 5).is_none());
    }

    #[test]
    fn oversized_snapshots_are_rejected_at_the_configured_bound() {
        let source = Source::new(b"12345");
        assert!(read_source(&source.0, source.fingerprint(), 4).is_none());
        assert_eq!(read_source(&source.0, source.fingerprint(), 5).as_deref(), Some("12345"));
    }

    #[test]
    fn racing_read_does_not_cache_a_fingerprint_and_is_retried() {
        let source = Source::new(b"![p](p.png)");
        let expected = source.fingerprint();
        let observed = std::collections::BTreeMap::from([(source.0.clone(), Some(expected))]);
        let mut graph = Graph::new(std::slice::from_ref(&source.0));
        std::fs::write(&source.0, b"different source").unwrap();
        graph.refresh(&observed, &BTreeSet::new());
        assert!(graph.documents.is_empty());
        assert!(graph.failures().contains(&source.0));
        std::fs::write(&source.0, b"![p](p.png)").unwrap();
        let needed = graph.refresh(&observed, &BTreeSet::new());
        assert!(needed.contains(&source.0.parent().unwrap().join("p.png")));
        assert!(graph.failures().is_empty());
    }

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
