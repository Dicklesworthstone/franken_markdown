//! Format-neutral book paths and chapter-aware URL resolution.
//!
//! A source path is a logical, book-relative name, never a filesystem path to
//! open. URLs are decoded exactly once; queries and fragments are preserved.

use std::collections::{BTreeMap, BTreeSet};

use super::{Book, BookChapter, BookInput};
use crate::{Block, Document, Inline, RenderError, Result};

/// Validate every input before parsing any chapter. Output-name collisions
/// are errors, not implicit last-writer-wins publication of the wrong chapter.
pub(super) fn input_paths(inputs: &[BookInput]) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut outputs = BTreeMap::new();
    let mut paths = Vec::with_capacity(inputs.len());
    for input in inputs {
        let path = source_path(&input.path).ok_or_else(|| {
            RenderError::InvalidInput(format!(
                "book: invalid book-relative chapter path {:?}", input.path
            ))
        })?;
        if !seen.insert(path.clone()) {
            return Err(RenderError::InvalidInput(format!(
                "book: duplicate normalized chapter path {path:?}"
            )));
        }
        let output = output_name(&path);
        // Portability: do not silently overwrite on case-insensitive hosts.
        if let Some(previous) = outputs.insert(output.to_ascii_lowercase(), path.clone()) {
            return Err(RenderError::InvalidInput(format!(
                "book: {previous:?} and {path:?} both map to output {output:?}; rename one chapter"
            )));
        }
        paths.push(path);
    }
    Ok(paths)
}

pub(super) fn source_path(path: &str) -> Option<String> {
    if path.starts_with(['/', '\\']) || has_scheme(path) {
        return None;
    }
    normalize(&path.replace('\\', "/"))
}

fn normalize(path: &str) -> Option<String> {
    if path.is_empty() || path.ends_with('/') || path.chars().any(char::is_control) {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => { parts.pop()?; }
            _ => parts.push(part),
        }
    }
    if matches!(parts.last(), None | Some(&"") | Some(&".") | Some(&"..")) {
        return None;
    }
    Some(parts.join("/"))
}

fn has_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else { return false; };
    let scheme = &value[..colon];
    !scheme.is_empty()
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// Resolve against the source chapter, not the flattened output directory.
/// A leading slash means book root; a leading double slash remains a network
/// URL. Escapes above the book root and malformed percent encodings fail shut.
pub(super) fn destination(source: &str, dest: &str) -> Option<(String, String)> {
    let dest = dest.trim();
    if dest.starts_with(['#', '?']) || dest.starts_with("//") || has_scheme(dest) {
        return None;
    }
    let end = dest.find(['#', '?']).unwrap_or(dest.len());
    let path = decode(&dest[..end])?;
    if path.is_empty() || path.contains('\\') || path.starts_with("//") || has_scheme(&path) {
        return None;
    }
    let joined = if let Some(rooted) = path.strip_prefix('/') {
        rooted.to_string()
    } else if let Some((parent, _)) = source.rsplit_once('/') {
        format!("{parent}/{path}")
    } else {
        path
    };
    Some((normalize(&joined)?, dest[end..].to_string()))
}

/// Resolve a chapter without assuming that every local URL names Markdown.
/// Exact source paths win. Otherwise an extensionless URL may name a .md or
/// .markdown file, or a directory containing index/README in either spelling.
/// An explicit directory URL considers only its index/README candidates.
/// More than one candidate is ambiguous and is deliberately left unchanged.
///
/// The membership callback always receives a decoded, normalized source path.
/// Each candidate passes through the same URL and root-containment policy as
/// an exact link; suffixes are opaque and percent escapes are not decoded twice.
pub(super) fn chapter_destination(
    source: &str,
    dest: &str,
    mut contains: impl FnMut(&str) -> bool,
) -> Option<(String, String)> {
    let dest = dest.trim();
    let end = dest.find(['#', '?']).unwrap_or(dest.len());
    let raw_path = &dest[..end];
    let decoded = decode(raw_path)?;
    let directory = decoded.ends_with('/')
        || matches!(decoded.as_str(), "." | "..")
        || decoded.ends_with("/.")
        || decoded.ends_with("/..");
    let (base, suffix) = if directory {
        // Resolving an actual candidate also handles root and dot-directory
        // links, which cannot be normalized as standalone source filenames.
        let separator = if decoded.ends_with('/') { "" } else { "/" };
        let candidate = format!("{raw_path}{separator}index.md{}", &dest[end..]);
        let (path, suffix) = destination(source, &candidate)?;
        (path.strip_suffix("index.md")?.to_string(), suffix)
    } else {
        let (path, suffix) = destination(source, dest)?;
        if contains(&path) {
            return Some((path, suffix));
        }
        // Do not capture an unknown image, archive, or explicitly named file
        // just because a similarly named Markdown chapter exists.
        if path.rsplit('/').next()?.contains('.') {
            return None;
        }
        (path, suffix)
    };
    let mut candidates = Vec::with_capacity(6);
    if !directory {
        candidates.push(format!("{base}.md"));
        candidates.push(format!("{base}.markdown"));
    }
    let separator = if directory { "" } else { "/" };
    for name in ["index.md", "index.markdown", "README.md", "README.markdown"] {
        candidates.push(format!("{base}{separator}{name}"));
    }
    let mut found = None;
    for candidate in candidates {
        if contains(&candidate) {
            if found.is_some() {
                return None;
            }
            found = Some(candidate);
        }
    }
    found.map(|path| (path, suffix))
}

fn decode(path: &str) -> Option<String> {
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

fn encode_path(path: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    out
}

/// Preserve ordinary historic names. Escape URL-reserved and non-ASCII bytes
/// with a literal, filesystem-safe `~hh` spelling (not URI percent encoding).
/// Reserve index.html for the host's generated landing page, so index.md can
/// never be overwritten by its own redirect. The escape marker itself is
/// escaped, preventing a literal source name from impersonating these names.
pub(super) fn output_name(path: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let clean = path
        .strip_prefix("./")
        .or_else(|| path.strip_prefix(".\\"))
        .or_else(|| path.strip_prefix('/'))
        .or_else(|| path.strip_prefix('\\'))
        .unwrap_or(path);
    let lower = clean.to_ascii_lowercase();
    let stem = if lower.ends_with(".markdown") {
        &clean[..clean.len() - 9]
    } else if lower.ends_with(".md") {
        &clean[..clean.len() - 3]
    } else {
        clean
    };
    let mut out = String::with_capacity(stem.len() + 5);
    for byte in stem.bytes() {
        match byte {
            b'/' | b'\\' => out.push_str("__"),
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') => {
                out.push(char::from(byte));
            }
            byte => {
                out.push('~');
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 15)]));
            }
        }
    }
    if out.is_empty() || out == "." || out == ".." {
        out.insert_str(0, "~chapter");
    }
    let lower = out.to_ascii_lowercase();
    let device = lower.split('.').next().unwrap_or(&lower);
    let reserved = lower == "index"
        || matches!(device, "con" | "prn" | "aux" | "nul")
        || ((device.starts_with("com") || device.starts_with("lpt"))
            && device.len() == 4
            && matches!(device.as_bytes()[3], b'1'..=b'9'));
    if reserved {
        out.insert_str(0, "~chapter-");
    }
    out.push_str(".html");
    out
}

/// Record resolved chapter links as root-relative Markdown URLs. This keeps
/// the AST format-neutral while giving existing source-less HTML consumers
/// enough context to resolve nested chapters. EPUB already understands these
/// rooted source URLs. Unknown files and external links are not rewritten.
pub(super) fn canonicalize(chapters: &mut [BookChapter]) {
    let known: BTreeSet<_> = chapters.iter().map(|chapter| chapter.path.clone()).collect();
    for chapter in chapters {
        let source = &chapter.path;
        rewrite_blocks(&mut chapter.doc.blocks, &mut |dest| {
            let (target, suffix) = chapter_destination(source, dest, |path| known.contains(path))?;
            Some(format!("/{}{suffix}", encode_path(&target)))
        });
    }
}

pub(super) fn rewrite_for_site(doc: &mut Document, known: &BTreeSet<String>) -> usize {
    rewrite_blocks(&mut doc.blocks, &mut |dest| {
        let (target, suffix) = destination("", dest)?;
        let lower = target.to_ascii_lowercase();
        if !lower.ends_with(".md") && !lower.ends_with(".markdown") {
            return None;
        }
        let output = output_name(&target);
        known.contains(&output).then(|| format!("{output}{suffix}"))
    })
}

/// Explicit context for callers rendering a document parsed outside build_book.
pub(super) fn rewrite_from(doc: &mut Document, source: &str, book: &Book) -> Result<usize> {
    let source = source_path(source).ok_or_else(|| {
        RenderError::InvalidInput("book: invalid source chapter path".to_string())
    })?;
    let mut known = BTreeMap::new();
    for chapter in &book.chapters {
        let path = source_path(&chapter.path).ok_or_else(|| {
            RenderError::InvalidInput("book: invalid target chapter path".to_string())
        })?;
        if known.insert(path, chapter.out_name.as_str()).is_some() {
            return Err(RenderError::InvalidInput("book: duplicate target chapter path".to_string()));
        }
    }
    Ok(rewrite_blocks(&mut doc.blocks, &mut |dest| {
        let (target, suffix) = chapter_destination(&source, dest, |path| known.contains_key(path))?;
        known.get(&target).map(|output| format!("{output}{suffix}"))
    }))
}

fn rewrite_blocks(blocks: &mut [Block], rewrite: &mut impl FnMut(&str) -> Option<String>) -> usize {
    let mut count = 0;
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                count += rewrite_inlines(inlines, rewrite);
            }
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                count += rewrite_blocks(inner, rewrite);
            }
            Block::List(list) => {
                for item in &mut list.items { count += rewrite_blocks(&mut item.blocks, rewrite); }
            }
            Block::Table(table) => {
                for cell in &mut table.head { count += rewrite_inlines(cell, rewrite); }
                for row in &mut table.rows {
                    for cell in row { count += rewrite_inlines(cell, rewrite); }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        count += rewrite_inlines(inlines, rewrite);
                    }
                }
            }
            _ => {}
        }
    }
    count
}

fn rewrite_inlines(inlines: &mut [Inline], rewrite: &mut impl FnMut(&str) -> Option<String>) -> usize {
    let mut count = 0;
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => {
                if let Some(replacement) = rewrite(dest) {
                    if replacement != *dest {
                        *dest = replacement;
                        count += 1;
                    }
                }
                count += rewrite_inlines(content, rewrite);
            }
            Inline::Emphasis(content) | Inline::Strong(content) | Inline::Strikethrough(content) => {
                count += rewrite_inlines(content, rewrite);
            }
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn paths_are_normalized_without_escaping_the_book() {
        assert_eq!(source_path("./guide/../intro.md").as_deref(), Some("intro.md"));
        assert_eq!(source_path("guide\\install.md").as_deref(), Some("guide/install.md"));
        for path in ["", ".", "..", "../escape.md", "/absolute.md", "C:\\file.md", "\\\\host\\file.md", "dir/", "a\0.md"] {
            assert!(source_path(path).is_none(), "accepted {path:?}");
        }
    }

    #[test]
    fn urls_keep_suffixes_and_use_source_directory_not_output_directory() {
        for (url, target, suffix) in [
            ("next.md#part", "guide/next.md", "#part"),
            ("../intro.md?mode=print#part", "intro.md", "?mode=print#part"),
            ("/reference.md#q?literal", "reference.md", "#q?literal"),
            ("%2e%2e/space%20name.md", "space name.md", ""),
            ("../%E4%B8%AD.md", "中.md", ""),
            ("../percent%2520.md", "percent%20.md", ""),
        ] {
            assert_eq!(destination("guide/start.md", url), Some((target.into(), suffix.into())));
        }
        for url in ["#local", "?mode=print", "https://host/intro.md", "HTTPS://host/a.md", "//host/a.md", "mailto:a.md", "custom+v1:a.md", "../../escape.md", "%2f%2fhost/a.md", "%68ttps%3a/x.md", "bad%ZZ.md", "%FF.md", "a%00.md", "..\\a.md"] {
            assert!(destination("guide/start.md", url).is_none(), "accepted {url:?}");
        }
    }

    #[test]
    fn output_names_remain_compatible_and_cannot_replace_the_landing_page() {
        assert_eq!(output_name("intro.md"), "intro.html");
        assert_eq!(output_name("guide.markdown"), "guide.html");
        assert_eq!(output_name("guide/install.md"), "guide__install.html");
        assert_eq!(output_name("win\\path\\part.md"), "win__path__part.html");
        assert_eq!(output_name("index.md"), "~chapter-index.html");
        assert_ne!(output_name("index.md"), output_name("~chapter-index.md"));
        for path in ["a?b#c.md", "中.md", "a%20b.md", "a\"b.md", "a'b.md"] {
            let name = output_name(path);
            assert!(name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'~' | b'_' | b'-' | b'.')));
        }
    }

    #[test]
    fn duplicate_paths_and_case_insensitive_output_collisions_fail_before_parsing() {
        for pair in [["intro.md", "./intro.md"], ["a/b.md", "a__b.md"], ["chapter.md", "chapter.markdown"], ["Chapter.md", "chapter.md"]] {
            let inputs = pair.map(|path| BookInput { path: path.into(), source: "# text".into() });
            assert!(input_paths(&inputs).is_err(), "accepted {pair:?}");
        }
    }

    #[test]
    fn path_encoding_is_reversible_without_interpreting_fragments_as_filenames() {
        for path in ["guide/hello world.md", "中.md", "a%20b.md", "a#b?c.md"] {
            assert_eq!(decode(&encode_path(path)).as_deref(), Some(path));
        }
    }

    #[test]
    fn chapter_aliases_resolve_relative_rooted_and_encoded_urls() {
        let known: BTreeSet<_> = [
            "install.md", "guide/next.markdown", "docs/index.md",
            "manual/README.markdown", "中.md", "percent%20.md",
        ].into_iter().map(String::from).collect();
        for (url, path, suffix) in [
            ("../install#setup", "install.md", "#setup"),
            ("next?print#part", "guide/next.markdown", "?print#part"),
            ("/docs/", "docs/index.md", ""),
            ("../docs", "docs/index.md", ""),
            ("/manual%2F#intro", "manual/README.markdown", "#intro"),
            ("../%E4%B8%AD#标题", "中.md", "#标题"),
            ("../percent%2520", "percent%20.md", ""),
        ] {
            assert_eq!(
                chapter_destination("guide/start.md", url, |path| known.contains(path)),
                Some((path.into(), suffix.into())),
                "failed to resolve {url:?}",
            );
        }
    }

    #[test]
    fn directory_links_include_root_and_dot_directories() {
        let known: BTreeSet<_> = ["index.md", "guide/index.markdown"]
            .into_iter().map(String::from).collect();
        for (url, path) in [
            ("/", "index.md"), ("..", "index.md"), ("../", "index.md"),
            (".", "guide/index.markdown"), ("./", "guide/index.markdown"),
            ("/guide/.", "guide/index.markdown"),
        ] {
            assert_eq!(
                chapter_destination("guide/start.md", url, |path| known.contains(path)),
                Some((path.into(), String::new())),
            );
        }
    }

    #[test]
    fn exact_chapters_win_and_ambiguous_aliases_are_not_guessed() {
        let known: BTreeSet<_> = [
            "guide", "guide/index.md", "manual.md", "manual/README.md",
            "docs/index.md", "docs/README.md", "asset.pdf.md",
        ].into_iter().map(String::from).collect();
        assert_eq!(
            chapter_destination("start.md", "guide", |path| known.contains(path)),
            Some(("guide".into(), String::new())),
        );
        for url in ["manual", "docs/", "asset.pdf", "absent"] {
            assert!(chapter_destination("start.md", url, |path| known.contains(path)).is_none());
        }
        assert_eq!(
            chapter_destination("start.md", "manual/", |path| known.contains(path)),
            Some(("manual/README.md".into(), String::new())),
        );
    }

    #[test]
    fn aliases_do_not_weaken_url_or_root_containment_policy() {
        for url in [
            "#local", "?print", "//host/", "https://host/", "mailto:user",
            "../../", "%2e%2e/%2e%2e/", "%2f%2fhost/", "%68ttps%3a/",
            "bad%ZZ/", "%FF/", "a%00/", "..\\guide/", "../%252e%252e/",
        ] {
            assert!(
                chapter_destination("guide/start.md", url, |path| {
                    matches!(path, "index.md" | "guide/index.md")
                }).is_none(),
                "accepted {url:?}",
            );
        }
    }

    #[test]
    fn build_book_canonicalizes_aliases_once_for_every_export() {
        let inputs = [
            ("guide/start.md", "[Install](../install#setup) [Docs](/docs/?print#intro)"),
            ("install.md", "# Setup"),
            ("docs/index.md", "# Intro"),
        ].map(|(path, source)| BookInput { path: path.into(), source: source.into() });
        let mut book = crate::book::build_book(&inputs).expect("build aliased book");
        let first = book.chapters[0].doc.clone();
        let mut destinations = Vec::new();
        rewrite_blocks(&mut book.chapters[0].doc.blocks, &mut |dest| {
            destinations.push(dest.to_string());
            None
        });
        assert_eq!(destinations, ["/install.md#setup", "/docs/index.md?print#intro"]);
        canonicalize(&mut book.chapters);
        assert_eq!(book.chapters[0].doc, first, "canonicalization must be idempotent");
    }

    #[test]
    fn source_context_rewriting_uses_the_same_alias_policy() {
        let inputs = [
            ("guide/start.md", "# Start"), ("docs/README.md", "# Docs"),
        ].map(|(path, source)| BookInput { path: path.into(), source: source.into() });
        let book = crate::book::build_book(&inputs).expect("build book");
        let mut doc = Document {
            blocks: vec![Block::Paragraph(vec![Inline::Strong(vec![Inline::Link {
                dest: "../docs/#intro".into(), title: None,
                content: vec![Inline::Text("Docs".into())],
            }])])],
        };
        assert_eq!(rewrite_from(&mut doc, "guide/start.md", &book).unwrap(), 1);
        let mut destinations = Vec::new();
        rewrite_blocks(&mut doc.blocks, &mut |dest| {
            destinations.push(dest.to_string());
            None
        });
        assert_eq!(destinations, ["docs__README.html#intro"]);
    }
}
