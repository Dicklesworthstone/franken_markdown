//! Bounded native book discovery, include expansion and explicit image assets.
//! No network fetching. Symlink entries are never followed during discovery;
//! explicit chapter/include/image reads reject symlink components. This is
//! portable path validation, not a race-free sandbox against concurrent hostile
//! filesystem mutation: callers must control the tree while it is being read.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::manifest::{self, Manifest};
use crate::book::{Book, BookInput, build_book, resolve_book_destination};
use crate::{Block, Inline, PdfImageAsset};

const MAX_CHAPTERS: usize = 4096;
const MAX_ENTRIES: usize = 100_000;
const MAX_DEPTH: usize = 64;
const MAX_TOTAL_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

pub(super) struct LoadedBook {
    pub book: Book,
    pub manifest: Manifest,
    pub source_bytes: Vec<usize>,
    pub images: Vec<PdfImageAsset>,
    pub warnings: Vec<String>,
    pub protected_paths: BTreeSet<PathBuf>,
}

pub(super) fn load(input: &Path, max_input_bytes: u64, max_image_bytes: u64) -> Result<LoadedBook, String> {
    let root = input.canonicalize().map_err(|error| format!("opening {}: {error}", input.display()))?;
    if !root.is_dir() {
        return Err(format!("book input is not a directory: {}", input.display()));
    }
    let mut warnings = Vec::new();
    let discovered = discover(&root, &mut warnings)?;
    let mut protected_paths = BTreeSet::new();
    let manifest = match fs::symlink_metadata(root.join("book.toml")) {
        Ok(_) => {
            let (path, bytes) = read_regular(&root, "book.toml", MAX_MANIFEST_BYTES)?;
            protected_paths.insert(path);
            let source = String::from_utf8(bytes).map_err(|_| "book.toml must be UTF-8".to_string())?;
            manifest::parse(&source)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Manifest::default(),
        Err(error) => return Err(format!("reading book.toml: {error}")),
    };
    let order = chapter_order(&discovered, &manifest.order, &manifest.include_only)?;
    if order.is_empty() {
        let message = if discovered.is_empty() { "no Markdown files found" }
            else { "no published Markdown chapters remain after include_only selection" };
        return Err(format!("{message} in {}", input.display()));
    }
    let mut inputs = Vec::with_capacity(order.len());
    let mut source_bytes = Vec::with_capacity(order.len());
    let read_bytes = Cell::new(0u64);
    let mut expanded_bytes = 0u64;
    // Includes may be requested repeatedly; charge every read rather than
    // allowing a short expansion to hide unbounded resolver I/O.
    let include_paths = std::cell::RefCell::new(BTreeSet::new());
    let per_file = max_input_bytes.min(MAX_TOTAL_SOURCE_BYTES);
    // Declared sources retain overwrite protection even when no chapter uses
    // them. Validate their identity without reading unused Markdown bodies.
    for relative in &manifest.include_only {
        protected_paths.insert(regular_path(&root, relative, per_file)?);
    }
    for relative in order {
        let (path, bytes) = read_regular(&root, &relative, per_file)?;
        charge_read(&read_bytes, bytes.len())?;
        protected_paths.insert(path);
        source_bytes.push(bytes.len());
        let raw = String::from_utf8(bytes).map_err(|_| format!("{relative}: Markdown must be UTF-8"))?;
        let source = if crate::transclude::has_includes(&raw) {
            let resolve = |requested: &str, origin: &str| -> crate::transclude::ResolveResult {
                let origin = if origin == "<input>" { &relative } else { origin };
                let parent = origin.rsplit_once('/').map_or("", |(parent, _)| parent);
                let requested = relative_path(parent, requested)?;
                let (path, bytes) = read_regular(&root, &requested, per_file)?;
                charge_read(&read_bytes, bytes.len())?;
                include_paths.borrow_mut().insert(path);
                let content = String::from_utf8(bytes)
                    .map_err(|_| format!("{requested}: included Markdown must be UTF-8"))?;
                Ok(Some((content, requested)))
            };
            let remaining = (MAX_TOTAL_SOURCE_BYTES - expanded_bytes).min(per_file) as usize;
            crate::transclude::expand_includes_with_limit(&raw, &resolve, remaining)
                .map_err(|error| format!("{relative}: {error}"))?
        } else {
            raw
        };
        expanded_bytes = expanded_bytes.checked_add(source.len() as u64)
            .ok_or_else(|| "expanded book size overflow".to_string())?;
        if expanded_bytes > MAX_TOTAL_SOURCE_BYTES {
            return Err("expanded book exceeds 64 MiB".to_string());
        }
        inputs.push(BookInput { path: relative, source });
    }
    protected_paths.extend(include_paths.into_inner());
    let mut book = build_book(&inputs).map_err(|error| error.to_string())?;
    drop(inputs);
    let mut requests = ImageRequests::default();
    for chapter in &mut book.chapters {
        discover_images(&mut chapter.doc.blocks, &chapter.path, &mut requests, &mut warnings)?;
    }
    let mut images = Vec::new();
    let mut total_images = 0usize;
    let mut request_count = 0usize;
    for (relative, keys) in requests.files {
        request_count = request_count.saturating_add(keys.len());
        if request_count > 4096 {
            return Err("book exceeds 4096 distinct image references".to_string());
        }
        match read_regular(&root, &relative, max_image_bytes.min(32 * 1024 * 1024)) {
            Ok((path, bytes)) => {
                protected_paths.insert(path);
                if !supported_image(&bytes) {
                    warnings.push(format!("image_unsupported: {relative}"));
                    continue;
                }
                let added = bytes.len().checked_mul(keys.len())
                    .ok_or_else(|| "image size overflow".to_string())?;
                total_images = total_images.checked_add(added)
                    .ok_or_else(|| "image size overflow".to_string())?;
                if total_images > MAX_TOTAL_IMAGE_BYTES {
                    return Err("book image payloads exceed 128 MiB".to_string());
                }
                let count = keys.len();
                let mut bytes = Some(bytes);
                for (index, destination) in keys.into_iter().enumerate() {
                    let payload = if index + 1 == count {
                        bytes.take().unwrap_or_default()
                    } else {
                        bytes.as_deref().unwrap_or_default().to_vec()
                    };
                    images.push(PdfImageAsset { destination, bytes: payload });
                }
            }
            Err(error) => warnings.push(format!("image_unavailable: {relative}: {error}")),
        }
    }
    warnings.sort();
    warnings.dedup();
    Ok(LoadedBook { book, manifest, source_bytes, images, warnings, protected_paths })
}

fn charge_read(total: &Cell<u64>, bytes: usize) -> Result<(), String> {
    let next = total.get().checked_add(bytes as u64)
        .ok_or_else(|| "book read size overflow".to_string())?;
    if next > MAX_TOTAL_SOURCE_BYTES {
        return Err("book source/include reads exceed 64 MiB".to_string());
    }
    total.set(next);
    Ok(())
}

fn markdown(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

fn discover(root: &Path, warnings: &mut Vec<String>) -> Result<BTreeSet<String>, String> {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut chapters = BTreeSet::new();
    let mut visited = 0usize;
    while let Some((directory, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err("book directory nesting exceeds 64 levels".to_string());
        }
        for entry in fs::read_dir(&directory).map_err(|error| format!("walking {}: {error}", directory.display()))? {
            let entry = entry.map_err(|error| format!("walking {}: {error}", directory.display()))?;
            visited += 1;
            if visited > MAX_ENTRIES {
                return Err("book discovery exceeds 100000 directory entries".to_string());
            }
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| "book paths must be UTF-8".to_string())?;
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let relative = path.strip_prefix(root).map_err(|_| "book path escapes root".to_string())?
                .to_str().ok_or_else(|| "book paths must be UTF-8".to_string())?
                .replace('\\', "/");
            let kind = entry.file_type().map_err(|error| format!("reading {relative}: {error}"))?;
            if kind.is_symlink() {
                warnings.push(format!("symlink_skipped: {relative}"));
            } else if kind.is_dir() {
                stack.push((path, depth + 1));
            } else if kind.is_file() && markdown(&relative) {
                relative_path("", &relative)?;
                chapters.insert(relative);
                if chapters.len() > MAX_CHAPTERS {
                    return Err("book exceeds 4096 Markdown chapters".to_string());
                }
            }
        }
    }
    Ok(chapters)
}

fn chapter_order(
    discovered: &BTreeSet<String>, requested: &[String], include_only: &[String],
) -> Result<Vec<String>, String> {
    let mut resources = BTreeSet::new();
    for name in include_only {
        let name = relative_path("", name)?;
        if !resources.insert(name.clone()) {
            return Err(format!("book.toml: duplicate include_only source {name:?}"));
        }
        if !discovered.contains(&name) {
            return Err(format!("book.toml: include_only source {name:?} is not a discovered regular Markdown file"));
        }
    }
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::with_capacity(discovered.len());
    for name in requested {
        let name = relative_path("", name)?;
        if !seen.insert(name.clone()) {
            return Err(format!("book.toml: duplicate chapter {name:?}"));
        }
        if !discovered.contains(&name) {
            return Err(format!("book.toml: chapter {name:?} is not a discovered regular Markdown file"));
        }
        if resources.contains(&name) {
            return Err(format!("book.toml: {name:?} cannot be both an ordered chapter and an include_only source"));
        }
        ordered.push(name);
    }
    ordered.extend(discovered.iter().filter(|path| !seen.contains(*path) && !resources.contains(*path)).cloned());
    Ok(ordered)
}

/// Literal filesystem paths, not URLs. Manifest/include paths can contain
/// spaces, commas and '#'; only image destinations go through URI decoding.
fn relative_path(parent: &str, requested: &str) -> Result<String, String> {
    if requested.is_empty() || requested.starts_with(['/', '\\'])
        || requested.ends_with(['/', '\\']) || requested.contains(':')
        || requested.chars().any(char::is_control)
    {
        return Err(format!("book path must be relative and nonempty: {requested:?}"));
    }
    let normalized = requested.replace('\\', "/");
    let mut parts: Vec<&str> = parent.split('/').filter(|part| !part.is_empty()).collect();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("book path escapes root: {requested:?}"));
                }
            }
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(format!("book path does not name a file: {requested:?}"));
    }
    Ok(parts.join("/"))
}

fn regular_path(root: &Path, relative: &str, limit: u64) -> Result<PathBuf, String> {
    let relative = relative_path("", relative)?;
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path.push(component);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("reading {relative}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("symlink component is not allowed: {relative}"));
        }
    }
    let metadata = fs::metadata(&path).map_err(|error| format!("reading {relative}: {error}"))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("{relative}: expected a regular file of at most {limit} bytes"));
    }
    let canonical = path.canonicalize().map_err(|error| format!("resolving {relative}: {error}"))?;
    if !canonical.starts_with(root) {
        return Err(format!("book path escapes root: {relative}"));
    }
    Ok(canonical)
}

fn read_regular(root: &Path, relative: &str, limit: u64) -> Result<(PathBuf, Vec<u8>), String> {
    let canonical = regular_path(root, relative, limit)?;
    let file = fs::File::open(&canonical).map_err(|error| format!("opening {relative}: {error}"))?;
    let metadata = file.metadata().map_err(|error| format!("reading {relative}: {error}"))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("{relative}: expected a regular file of at most {limit} bytes"));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)
        .map_err(|error| format!("reading {relative}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{relative}: file exceeds {limit} bytes"));
    }
    Ok((canonical, bytes))
}

fn supported_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.starts_with(b"\xff\xd8\xff")
        || std::str::from_utf8(bytes).is_ok_and(|text| {
            let start = text.trim_start_matches('\u{feff}').trim_start();
            start.starts_with("<svg") || (start.starts_with("<?xml") && start.contains("<svg"))
        })
}

fn uri_path(path: &str) -> String {
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

#[derive(Default)]
struct ImageRequests {
    files: BTreeMap<String, BTreeSet<String>>,
    count: usize,
}

impl ImageRequests {
    fn insert(&mut self, path: String, key: String) -> Result<(), String> {
        let keys = self.files.entry(path).or_default();
        if !keys.contains(&key) {
            if self.count == 4096 {
                return Err("book exceeds 4096 distinct image references".to_string());
            }
            self.count += 1;
            keys.insert(key);
        }
        Ok(())
    }
}

fn discover_images(
    blocks: &mut [Block], source: &str,
    requests: &mut ImageRequests, warnings: &mut Vec<String>,
) -> Result<(), String> {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => image_inlines(inlines, source, requests, warnings)?,
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => discover_images(inner, source, requests, warnings)?,
            Block::List(list) => {
                for item in &mut list.items { discover_images(&mut item.blocks, source, requests, warnings)?; }
            }
            Block::Table(table) => {
                for cell in &mut table.head { image_inlines(cell, source, requests, warnings)?; }
                for row in &mut table.rows {
                    for cell in row { image_inlines(cell, source, requests, warnings)?; }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for values in item.terms.iter_mut().chain(&mut item.definitions) {
                        image_inlines(values, source, requests, warnings)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn image_inlines(
    inlines: &mut [Inline], source: &str,
    requests: &mut ImageRequests, warnings: &mut Vec<String>,
) -> Result<(), String> {
    for inline in inlines {
        match inline {
            Inline::Image { dest, .. } => {
                if let Some((relative, suffix)) = resolve_book_destination(source, dest) {
                    let key = format!("/{}{suffix}", uri_path(&relative));
                    requests.insert(relative, key.clone())?;
                    *dest = key;
                } else if !dest.trim().to_ascii_lowercase().starts_with("data:") {
                    if warnings.len() >= 4096 {
                        return Err("book image diagnostics exceed 4096 entries".to_string());
                    }
                    warnings.push(format!("image_not_loaded: {source}: {dest}"));
                }
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner)
            | Inline::Link { content: inner, .. } => image_inlines(inner, source, requests, warnings)?,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn literal_paths_normalize_and_refuse_absolute_or_escaping_reads() {
        assert_eq!(relative_path("guide", "../a,b#c.md").unwrap(), "a,b#c.md");
        assert_eq!(relative_path("", "guide\\next.md").unwrap(), "guide/next.md");
        for path in ["../secret.md", "/root.md", "C:\\file.md", "a/b:stream", "", ".", "a\0.md"] {
            assert!(relative_path("", path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn manifest_order_is_explicit_with_lexical_remainder() {
        let files = BTreeSet::from(["a.md".into(), "b.md".into(), "z.md".into()]);
        assert_eq!(chapter_order(&files, &["z.md".into()], &[]).unwrap(), ["z.md", "a.md", "b.md"]);
        assert!(chapter_order(&files, &["missing.md".into()], &[]).is_err());
        assert!(chapter_order(&files, &["a.md".into(), "./a.md".into()], &[]).is_err());
    }

    #[test]
    fn unicode_and_punctuation_are_encoded_only_for_render_destinations() {
        assert_eq!(uri_path("a/b#c?.svg"), "a/b%23c%3F.svg");
        assert_eq!(uri_path("中.svg"), "%E4%B8%AD.svg");
        assert_eq!(uri_path("percent%20.svg"), "percent%2520.svg");
    }

    #[test]
    fn total_read_budget_cannot_be_overrun_by_repeated_includes() {
        let total = Cell::new(MAX_TOTAL_SOURCE_BYTES - 1);
        charge_read(&total, 1).unwrap();
        assert!(charge_read(&total, 1).is_err());
        assert_eq!(total.get(), MAX_TOTAL_SOURCE_BYTES);
    }

    #[test]
    fn include_only_paths_are_removed_without_reordering_other_chapters() {
        let files = BTreeSet::from([
            "a.md".into(), "b.md".into(), "parts/共享.md".into(), "z.md".into(),
        ]);
        let order = chapter_order(&files, &["z.md".into()], &["./parts/共享.md".into()]).unwrap();
        assert_eq!(order, ["z.md", "a.md", "b.md"]);
        assert_eq!(files.len(), 4, "selection must not alter discovery");
    }

    #[test]
    fn include_only_rejects_unknown_duplicate_escaping_and_conflicting_roles() {
        let files = BTreeSet::from(["a.md".into(), "parts/shared.md".into()]);
        for resources in [
            vec!["missing.md".into()], vec!["../outside.md".into()],
            vec!["parts/shared.md".into(), "./parts/shared.md".into()],
            vec!["/parts/shared.md".into()],
        ] {
            assert!(chapter_order(&files, &[], &resources).is_err(), "{resources:?}");
        }
        assert!(chapter_order(&files, &["parts/shared.md".into()], &["parts\\shared.md".into()]).is_err());
        assert!(chapter_order(&files, &[], &files.iter().cloned().collect::<Vec<_>>()).unwrap().is_empty());
    }
}
