//! Source-preserving file moves for a complete, explicitly captured book.
//! The parser and transclusion engine own edited tokens. Re-expanding and
//! comparing every chapter proves that an edit cannot change literal examples,
//! reference ownership, selected snippets, or any non-destination Markdown.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::{BookInput, BookRenderer, paths};
use crate::parse::{DestinationSpan, destination_spans_with_profile};
use crate::safe_html::{UrlKind, visit_destinations};
use crate::{Document, Profile, RenderError, Result, parse_markdown_with_profile};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_JSON: usize = 128 * 1024 * 1024;
const MAX_TOKENS: usize = 32_768;
const MAX_EDITS: usize = 16_384;
const MAX_EDIT_BYTES: usize = 8 * 1024 * 1024;

/// A complete proposed source capture. This operation never moves a host file,
/// mutates a workspace, or changes image/font authorizations. Hosts publish the
/// proposal only while they still own the original source revision.
#[derive(Debug, Clone)]
pub struct BookSourceRename {
    pub old_path: String,
    pub new_path: String,
    pub files: Vec<BookInput>,
    pub include_sources: Vec<BookInput>,
    /// Sources whose path or source bytes differ, counting each source once.
    pub changed_source_count: usize,
    /// Unique replaced source tokens, excluding the file path itself.
    pub edit_count: usize,
}

impl BookSourceRename {
    /// Complete source-only transaction JSON, bounded before each append.
    ///
    /// # Errors
    /// Refuses a serialized result over 128 MiB, including manually made reports.
    pub fn to_json(&self) -> Result<String> {
        let mut out = Json(String::new());
        out.raw("{\"schema\":\"fmd-book-source-rename-v1\",\"oldPath\":")?;
        out.string(&self.old_path)?;
        out.raw(",\"newPath\":")?;
        out.string(&self.new_path)?;
        for (name, sources) in [
            ("files", &self.files),
            ("includeSources", &self.include_sources),
        ] {
            out.raw(&format!(",\"{name}\":["))?;
            for (index, source) in sources.iter().enumerate() {
                if index != 0 {
                    out.raw(",")?;
                }
                out.raw("{\"path\":")?;
                out.string(&source.path)?;
                out.raw(",\"source\":")?;
                out.string(&source.source)?;
                out.raw("}")?;
            }
            out.raw("]")?;
        }
        out.raw(&format!(
            ",\"changedSourceCount\":{},\"editCount\":{}}}",
            self.changed_source_count, self.edit_count
        ))?;
        Ok(out.0)
    }
}

fn invalid(reason: &str) -> RenderError {
    RenderError::InvalidInput(format!("book_source_rename: {reason}"))
}

struct Json(String);
impl Json {
    fn raw(&mut self, text: &str) -> Result<()> {
        if text.len() > MAX_JSON.saturating_sub(self.0.len()) {
            return Err(invalid("source report exceeds 128 MiB"));
        }
        self.0.push_str(text);
        Ok(())
    }
    fn string(&mut self, text: &str) -> Result<()> {
        self.raw("\"")?;
        let mut last = 0;
        for (index, ch) in text.char_indices() {
            if ch == '"' || ch == '\\' || ch.is_ascii_control() {
                self.raw(&text[last..index])?;
                match ch {
                    '"' => self.raw("\\\"")?,
                    '\\' => self.raw("\\\\")?,
                    _ => self.raw(&format!("\\u{:04x}", u32::from(ch)))?,
                }
                last = index + ch.len_utf8();
            }
        }
        self.raw(&text[last..])?;
        self.raw("\"")
    }
}

#[derive(Default)]
struct Budget {
    tokens: usize,
    edits: usize,
    edit_bytes: usize,
}

struct Edit {
    range: Range<usize>,
    text: String,
}

fn apply(source: &str, edits: &[Edit]) -> Result<String> {
    let mut length = source.len();
    let mut previous = 0;
    for edit in edits {
        if edit.range.start < previous || source.get(edit.range.clone()).is_none() {
            return Err(invalid("source token ranges overlap or are unavailable"));
        }
        length = length
            .checked_sub(edit.range.len())
            .and_then(|n| n.checked_add(edit.text.len()))
            .filter(|&n| n <= MAX_BYTES)
            .ok_or_else(|| invalid("rewritten source exceeds 64 MiB"))?;
        previous = edit.range.end;
    }
    let mut out = String::with_capacity(length);
    previous = 0;
    for edit in edits {
        out.push_str(&source[previous..edit.range.start]);
        out.push_str(&edit.text);
        previous = edit.range.end;
    }
    out.push_str(&source[previous..]);
    Ok(out)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn relative(origin: &str, target: &str) -> String {
    let from: Vec<_> = parent(origin)
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to: Vec<_> = target.split('/').filter(|part| !part.is_empty()).collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec![".."; from.len() - common];
    parts.extend_from_slice(&to[common..]);
    let mut result = if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    };
    if (target.is_empty() || target.ends_with('/')) && !result.ends_with('/') {
        result.push('/');
    }
    result
}

fn encode(path: &str) -> String {
    let mut out = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
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

fn markdown_token(value: &str) -> String {
    let mut out = String::from("<");
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(ch),
        }
    }
    out.push('>');
    out
}

struct Plan<'a> {
    old: &'a str,
    new: &'a str,
    before: BTreeSet<String>,
    after: BTreeSet<String>,
    image_keys: BTreeSet<String>,
}
impl Plan<'_> {
    fn moved<'a>(&'a self, path: &'a str) -> &'a str {
        if path == self.old { self.new } else { path }
    }

    fn target(
        &self,
        origin: &str,
        destination: &str,
        kind: UrlKind,
        after: bool,
    ) -> Option<(String, String)> {
        let known = if after { &self.after } else { &self.before };
        if kind == UrlKind::Link
            && let Some(target) =
                paths::chapter_destination(origin, destination, |path| known.contains(path))
        {
            return Some(target);
        }
        if let Some(target) = paths::destination(origin, destination) {
            return Some(target);
        }
        let end = destination.find(['#', '?']).unwrap_or(destination.len());
        let raw = &destination[..end];
        let decoded = crate::source_origins::decode_path(raw)?;
        if !decoded.ends_with('/') && !matches!(decoded.rsplit('/').next(), Some("." | "..")) {
            return None;
        }
        let probe = format!("{raw}/.__fmd_rename_directory__{}", &destination[end..]);
        let (target, suffix) = paths::destination(origin, &probe)?;
        Some((
            target.strip_suffix(".__fmd_rename_directory__")?.to_owned(),
            suffix,
        ))
    }

    fn rewrite(&self, origin: &str, destination: &str, kind: UrlKind) -> Result<Option<String>> {
        if kind == UrlKind::Image {
            return self.rewrite_image(origin, destination);
        }
        let Some((before, suffix)) = self.target(origin, destination, kind, false) else {
            return Ok(None);
        };
        if kind == UrlKind::Link && self.old != self.new && before == self.new {
            return Err(invalid(
                "rename would capture an existing destination at the new source path",
            ));
        }
        // Source files and host-authorized image bytes occupy separate
        // namespaces. Only link targets move; an image keeps its original key.
        let after = if kind == UrlKind::Link {
            self.moved(&before)
        } else {
            before.as_str()
        };
        let new_origin = self.moved(origin);
        if self.target(new_origin, destination, kind, true).as_ref()
            == Some(&(after.to_owned(), suffix.clone()))
        {
            return Ok(None);
        }
        let replacement = if destination.trim().starts_with('/') {
            format!("/{}{suffix}", encode(after))
        } else {
            format!("{}{suffix}", encode(&relative(new_origin, after)))
        };
        if self.target(new_origin, &replacement, kind, true).as_ref()
            != Some(&(after.to_owned(), suffix))
        {
            return Err(invalid(
                "renamed destination cannot retain its original target",
            ));
        }
        Ok(Some(replacement))
    }

    fn image_binding(&self, origin: &str, destination: &str) -> Option<String> {
        let selected = super::render::qualified_image_key(origin, destination, |key| {
            self.image_keys.contains(key)
        })
        .unwrap_or_else(|| destination.trim().to_owned());
        self.image_keys.contains(&selected).then_some(selected)
    }

    fn rewrite_image(&self, origin: &str, destination: &str) -> Result<Option<String>> {
        let new_origin = self.moved(origin);
        if let Some(key) = self.image_binding(origin, destination) {
            if self.image_binding(new_origin, destination).as_deref() == Some(&key) {
                return Ok(None);
            }
            // A direct-key grant can become shadowed by a newly qualified key.
            // A rooted URI also survives transclusion rebasing unchanged. The
            // literal key remains a candidate for schemes and non-path keys.
            for candidate in [format!("/{}", encode(&key)), key.clone()] {
                if self.image_binding(new_origin, &candidate).as_deref() == Some(&key) {
                    return Ok(Some(candidate));
                }
            }
            return Err(invalid(
                "renamed image cannot retain its authorized asset key",
            ));
        }
        let Some((target, suffix)) = self.target(origin, destination, UrlKind::Image, false) else {
            return Ok(None);
        };
        let candidates = [
            destination.to_owned(),
            format!("{}{suffix}", encode(&relative(new_origin, &target))),
            format!("/{}{suffix}", encode(&target)),
        ];
        for candidate in candidates {
            if self
                .target(new_origin, &candidate, UrlKind::Image, true)
                .as_ref()
                == Some(&(target.clone(), suffix.clone()))
                && self.image_binding(new_origin, &candidate).is_none()
            {
                return Ok((candidate != destination).then_some(candidate));
            }
        }
        Err(invalid(
            "renamed image would capture a previously unresolved asset",
        ))
    }

    fn canonical(&self, doc: &mut Document, origin: &str, after: bool) -> Result<()> {
        let mut captured = false;
        visit_destinations(&mut doc.blocks, &mut |destination, kind| {
            if kind == UrlKind::Image {
                // Every image uses a tagged projection, including external and
                // unresolved values. No literal URL can impersonate an asset
                // binding merely by spelling an internal proof marker.
                *destination = if let Some(key) = self.image_binding(origin, destination) {
                    format!("fmd-image-proof:asset:{}", encode(&key))
                } else if let Some((target, suffix)) = self.target(origin, destination, kind, after)
                {
                    format!(
                        "fmd-image-proof:file:{}|{}",
                        encode(&target),
                        encode(&suffix)
                    )
                } else {
                    format!("fmd-image-proof:literal:{}", encode(destination))
                };
                return;
            }
            let (target, suffix) = if destination.starts_with(['#', '?']) && kind == UrlKind::Link {
                (origin.to_owned(), destination.clone())
            } else if let Some(target) = self.target(origin, destination, kind, after) {
                target
            } else {
                return;
            };
            if kind == UrlKind::Link && !after && self.old != self.new && target == self.new {
                captured = true;
            }
            let target = if after || kind == UrlKind::Image {
                target.as_str()
            } else {
                self.moved(&target)
            };
            *destination = format!("/{}{suffix}", encode(target));
        });
        if captured {
            return Err(invalid(
                "rename would capture an existing destination at the new source path",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct Roles {
    link: bool,
    image: bool,
}

fn roles(source: &str, tokens: &[DestinationSpan], profile: Option<Profile>) -> Result<Vec<Roles>> {
    let original = parse_markdown_with_profile(source, profile);
    let mut existing = BTreeSet::new();
    let mut copy = original.clone();
    visit_destinations(&mut copy.blocks, &mut |dest, _| {
        existing.insert(dest.clone());
    });
    let prefix = (0..32)
        .map(|n| format!("#fmd-book-rename-{n}-"))
        .find(|prefix| {
            !source.contains(prefix)
                && !existing.iter().any(|dest| dest.contains(prefix))
                && !tokens.iter().any(|token| token.dest.contains(prefix))
        })
        .ok_or_else(|| invalid("cannot reserve source ownership markers"))?;
    let markers: BTreeMap<_, _> = (0..tokens.len())
        .map(|i| (format!("{prefix}{i}"), i))
        .collect();
    let edits: Vec<_> = tokens
        .iter()
        .enumerate()
        .map(|(i, token)| Edit {
            range: token.range.clone(),
            text: format!("{prefix}{i}"),
        })
        .collect();
    let amended = apply(source, &edits)?;
    let mut restored = parse_markdown_with_profile(&amended, profile);
    let mut roles = vec![Roles::default(); tokens.len()];
    visit_destinations(&mut restored.blocks, &mut |destination, kind| {
        if let Some(&index) = markers.get(destination) {
            match kind {
                UrlKind::Link => roles[index].link = true,
                UrlKind::Image => roles[index].image = true,
            }
            destination.clone_from(&tokens[index].dest);
        }
    });
    if restored != original {
        return Err(invalid("destination source ownership cannot be proven"));
    }
    Ok(roles)
}

fn include_path(origin: &str, target: &str) -> Result<String> {
    if target.is_empty()
        || target.starts_with(['/', '\\'])
        || target.ends_with(['/', '\\'])
        || target.contains(':')
        || target.chars().any(char::is_control)
    {
        return Err(invalid("include path is not a relative book source"));
    }
    paths::source_path(format!("{}/{}", parent(origin), target).trim_start_matches('/'))
        .ok_or_else(|| invalid("include path escapes the captured book"))
}

fn include_token(path: &str) -> Result<String> {
    crate::transclude::include_path_token(path)
        .map_err(|_| invalid("renamed include path cannot be quoted without changing its bytes"))
}

fn rewrite_source(
    input: &BookInput,
    plan: &Plan<'_>,
    profile: Option<Profile>,
    budget: &mut Budget,
) -> Result<BookInput> {
    let includes = crate::transclude::include_target_spans(&input.source, profile)?;
    let discovered = destination_spans_with_profile(&input.source, profile);
    budget.tokens += discovered.len() + includes.len();
    if budget.tokens > MAX_TOKENS {
        return Err(invalid("rename exceeds 32768 source tokens"));
    }
    let mut include_index = 0;
    let tokens: Vec<_> = discovered
        .into_iter()
        .filter(|token| {
            while includes
                .get(include_index)
                .is_some_and(|include| include.line.end <= token.range.start)
            {
                include_index += 1;
            }
            !includes
                .get(include_index)
                .is_some_and(|include| include.line.start < token.range.end)
        })
        .collect();
    let roles = roles(&input.source, &tokens, profile)?;
    let mut edits = Vec::new();
    for (token, role) in tokens.into_iter().zip(roles) {
        let replacement = match (role.link, role.image) {
            (true, false) => plan.rewrite(&input.path, &token.dest, UrlKind::Link)?,
            (false, true) => plan.rewrite(&input.path, &token.dest, UrlKind::Image)?,
            _ => {
                let link = plan.rewrite(&input.path, &token.dest, UrlKind::Link)?;
                let image = plan.rewrite(&input.path, &token.dest, UrlKind::Image)?;
                if link != image {
                    return Err(invalid(
                        "shared or unused reference has incompatible link and image targets",
                    ));
                }
                link
            }
        };
        if let Some(replacement) = replacement {
            edits.push(Edit {
                range: token.range,
                text: markdown_token(&replacement),
            });
        }
    }
    for include in includes {
        let target = include_path(&input.path, &include.path)?;
        if plan.old != plan.new && target == plan.new {
            return Err(invalid(
                "rename would capture an existing include at the new source path",
            ));
        }
        let target = plan.moved(&target);
        let origin = plan.moved(&input.path);
        if include_path(origin, &include.path).as_deref().ok() != Some(target) {
            edits.push(Edit {
                range: include.range,
                text: include_token(&relative(origin, target))?,
            });
        }
    }
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    budget.edits += edits.len();
    budget.edit_bytes += edits.iter().map(|edit| edit.text.len()).sum::<usize>();
    if budget.edits > MAX_EDITS || budget.edit_bytes > MAX_EDIT_BYTES {
        return Err(invalid("rename exceeds its source-edit budget"));
    }
    let result = BookInput {
        path: plan.moved(&input.path).to_owned(),
        source: apply(&input.source, &edits)?,
    };
    let mut expected =
        parse_markdown_with_profile(&without_directives(&input.source, profile)?, profile);
    let mut actual =
        parse_markdown_with_profile(&without_directives(&result.source, profile)?, profile);
    plan.canonical(&mut expected, &input.path, false)?;
    plan.canonical(&mut actual, &result.path, true)?;
    if expected != actual {
        return Err(invalid(
            "rename cannot preserve complete source Markdown semantics",
        ));
    }
    Ok(result)
}

fn without_directives(source: &str, profile: Option<Profile>) -> Result<String> {
    let edits = crate::transclude::include_target_spans(source, profile)?
        .into_iter()
        .map(|token| {
            let text = source[token.line.clone()]
                .bytes()
                .map(|byte| {
                    if matches!(byte, b'\r' | b'\n') {
                        char::from(byte)
                    } else {
                        ' '
                    }
                })
                .collect();
            Edit {
                range: token.line,
                text,
            }
        })
        .collect::<Vec<_>>();
    apply(source, &edits)
}

fn canonical_path(path: &str) -> bool {
    path.len() <= 4096 && !path.contains(':') && paths::source_path(path).as_deref() == Some(path)
}

/// Rename one captured chapter or include source while maintaining its links,
/// image paths, and transclusion edges. Source order and roles are retained.
/// Incoming Markdown links and reference definitions follow the moved file;
/// outgoing local destinations retain their previous logical targets. Queries,
/// fragments, titles, code examples and unrelated source bytes are preserved.
///
/// Includes use the real expansion grammar and selector handling. Complete
/// before/after expanded AST comparison rejects conflicting snippet contexts,
/// unproven HTML rewrites, alias capture and any non-destination content change.
/// This immutable operation reads no files and grants no assets.
/// This convenience form declares that no image assets are supplied. Hosts
/// with authorized images use [`rename_book_source_with_images`] instead.
///
/// # Errors
/// Rejects noncanonical/duplicate/colliding paths, missing endpoints, source or
/// token budgets, invalid include graphs, and changes whose complete semantics
/// cannot be proven. No partial proposal is returned.
pub fn rename_book_source(
    chapters: &[BookInput],
    resources: &[BookInput],
    old_path: &str,
    new_path: &str,
    profile: Option<Profile>,
) -> Result<BookSourceRename> {
    rename_book_source_with_images(chapters, resources, old_path, new_path, profile, &[])
}

/// The image-aware source rename uses the complete immutable inventory of host
/// image destination keys, without reading or cloning image bytes. Qualified
/// keys and the renderer's literal-key fallback retain their exact selected
/// key; unresolved images cannot acquire a different supplied asset.
/// Omit keys only when no image assets are supplied.
///
/// # Errors
/// All errors from [`rename_book_source`], plus invalid image-key inventories
/// or an image binding that cannot be represented after the source moves.
pub fn rename_book_source_with_images(
    chapters: &[BookInput],
    resources: &[BookInput],
    old_path: &str,
    new_path: &str,
    profile: Option<Profile>,
    image_destinations: &[String],
) -> Result<BookSourceRename> {
    if image_destinations.len() > 4096 {
        return Err(invalid("image inventory exceeds 4096 destination keys"));
    }
    let mut image_keys = BTreeSet::new();
    for key in image_destinations {
        if key.len() > 4096 || key.trim().is_empty() || !image_keys.insert(key.trim().to_owned()) {
            return Err(invalid(
                "image keys must be nonempty, unique after trimming, and at most 4096 bytes",
            ));
        }
    }
    if chapters.is_empty()
        || chapters.len() + resources.len() > 4096
        || !canonical_path(old_path)
        || !canonical_path(new_path)
    {
        return Err(invalid(
            "expected canonical source paths and 1..=4096 captured sources",
        ));
    }
    let mut known = BTreeSet::new();
    let mut folded = BTreeSet::new();
    let mut bytes = 0usize;
    for input in chapters.iter().chain(resources) {
        if !canonical_path(&input.path)
            || !known.insert(input.path.clone())
            || !folded.insert(input.path.to_lowercase())
        {
            return Err(invalid("captured source paths are noncanonical or collide"));
        }
        bytes = bytes
            .checked_add(input.path.len())
            .and_then(|n| n.checked_add(input.source.len()))
            .filter(|&n| n <= MAX_BYTES)
            .ok_or_else(|| invalid("captured source exceeds 64 MiB"))?;
    }
    if !known.contains(old_path) {
        return Err(invalid("source to rename is not captured"));
    }
    let moved: BTreeSet<_> = known
        .iter()
        .map(|path| {
            if path == old_path {
                new_path.to_owned()
            } else {
                path.clone()
            }
        })
        .collect();
    let folded: BTreeSet<_> = moved.iter().map(|path| path.to_lowercase()).collect();
    if moved.len() != known.len()
        || folded.len() != known.len()
        || folded.iter().any(|path| {
            path.match_indices('/')
                .any(|(at, _)| folded.contains(&path[..at]))
        })
    {
        return Err(invalid(
            "renamed path collides with a captured source or directory",
        ));
    }
    let (before, _, _) = BookRenderer::prepare_workspace_sources(chapters, resources, profile)?;
    let plan = Plan {
        old: old_path,
        new: new_path,
        before: chapters.iter().map(|file| file.path.clone()).collect(),
        after: chapters
            .iter()
            .map(|file| {
                if file.path == old_path {
                    new_path.to_owned()
                } else {
                    file.path.clone()
                }
            })
            .collect(),
        image_keys,
    };
    let mut budget = Budget::default();
    let files: Vec<_> = chapters
        .iter()
        .map(|file| rewrite_source(file, &plan, profile, &mut budget))
        .collect::<Result<_>>()?;
    let include_sources: Vec<_> = resources
        .iter()
        .map(|file| rewrite_source(file, &plan, profile, &mut budget))
        .collect::<Result<_>>()?;
    let (after, _, _) = BookRenderer::prepare_workspace_sources(&files, &include_sources, profile)?;
    for (before, after) in before.iter().zip(&after) {
        let mut expected = parse_markdown_with_profile(&before.source, profile);
        let mut actual = parse_markdown_with_profile(&after.source, profile);
        plan.canonical(&mut expected, &before.path, false)?;
        plan.canonical(&mut actual, &after.path, true)?;
        if expected != actual {
            return Err(invalid(
                "rename cannot preserve complete expanded Markdown semantics",
            ));
        }
    }
    let changed_source_count = chapters
        .iter()
        .chain(resources)
        .zip(files.iter().chain(&include_sources))
        .filter(|(before, after)| before.path != after.path || before.source != after.source)
        .count();
    Ok(BookSourceRename {
        old_path: old_path.into(),
        new_path: new_path.into(),
        files,
        include_sources,
        changed_source_count,
        edit_count: budget.edits,
    })
}

#[cfg(test)]
#[path = "source_rename_tests.rs"]
mod tests;
