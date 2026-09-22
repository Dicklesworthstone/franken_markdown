//! In-memory book transclusion over an explicit, immutable host source set.
//! Include syntax and selectors remain owned by the shared expansion engine.

use std::cell::Cell;
use std::collections::BTreeMap;

use super::{BookInput, BookRenderer, MAX_CHAPTERS, MAX_SOURCE_BYTES, paths};
use crate::transclude::{ResolveResult, expand_includes_with_limit, has_includes};
use crate::{RenderError, Result};

const MAX_BOOK_RESOLUTIONS: usize = 4096;

impl BookRenderer {
    // Crate-internal transactions for BookWorkspace. Keep expansion policy in
    // this module instead of building a second include resolver in the editor.
    pub(crate) fn prepare_workspace_sources(
        chapters: &[BookInput],
        resources: &[BookInput],
    ) -> Result<(Vec<BookInput>, usize)> {
        prepare(chapters, resources, MAX_SOURCE_BYTES, MAX_BOOK_RESOLUTIONS)
    }

    pub(crate) fn from_workspace_sources(
        expanded: &[BookInput],
        source_length: usize,
    ) -> Result<Self> {
        let mut renderer = Self::new(expanded)?;
        renderer.source_length = source_length;
        Ok(renderer)
    }

    pub(crate) fn commit_workspace_sources(
        &mut self,
        replacements: Vec<Option<crate::book::BookChapter>>,
        source_length: usize,
    ) {
        debug_assert_eq!(replacements.len(), self.book.chapters.len());
        for (chapter, replacement) in self.book.chapters.iter_mut().zip(replacements) {
            if let Some(replacement) = replacement {
                *chapter = replacement;
            }
        }
        // The selected path set is unchanged. Canonicalization is idempotent
        // for retained chapters, and binds newly parsed links against ALL pages.
        paths::canonicalize(&mut self.book.chapters);
        self.source_length = source_length;
    }
    /// Expand an explicitly supplied source collection, then parse once.
    ///
    /// `chapters` fixes the reading order. `include_sources` supplies additional
    /// UTF-8 resources which may be included but never become chapters, spine
    /// items, or standalone site pages. Chapters may also include one another.
    /// No file, URL, or system path is opened; missing sources are errors.
    ///
    /// Includes resolve relative to the file containing the directive. Paths
    /// are literal (not percent-decoded URLs), case-sensitive, and confined to
    /// the logical book root. As in native textual transclusion, Markdown links
    /// and image URLs in inserted text resolve in the containing chapter, not
    /// the snippet's directory. Use book-root URLs where that distinction matters.
    ///
    /// The existing `new` constructor deliberately remains a parse-only API
    /// for already expanded source. `source_length()` counts the original
    /// chapter and include-source bytes once each, not the expanded copies.
    ///
    /// # Errors
    /// Rejects empty books, more than 4096 total sources, duplicate normalized
    /// keys, invalid paths/output names, missing includes, invalid selectors,
    /// cycles, or expansion depth over 16. The whole book shares limits of
    /// 4096 resolver calls and 64 MiB each for input text/paths, expanded
    /// chapter text/paths, and original text plus resolver copies. All source
    /// validation and expansion finish before parsing; no partial book escapes.
    pub fn from_sources(chapters: &[BookInput], include_sources: &[BookInput]) -> Result<Self> {
        let (expanded, source_length) = prepare(
            chapters, include_sources, MAX_SOURCE_BYTES, MAX_BOOK_RESOLUTIONS,
        )?;
        let mut renderer = Self::new(&expanded)?;
        renderer.source_length = source_length;
        Ok(renderer)
    }
}

fn invalid(message: impl std::fmt::Display) -> RenderError {
    RenderError::InvalidInput(format!("book_sources: {message}"))
}

fn add(total: &mut usize, bytes: usize, limit: usize, label: &str) -> Result<()> {
    if bytes > limit.saturating_sub(*total) {
        return Err(invalid(format!("{label} exceeds the {limit}-byte limit")));
    }
    *total += bytes;
    Ok(())
}

fn prepare(
    chapters: &[BookInput],
    include_sources: &[BookInput],
    max_bytes: usize,
    max_resolutions: usize,
) -> Result<(Vec<BookInput>, usize)> {
    if chapters.is_empty() || chapters.len() > MAX_CHAPTERS
        || include_sources.len() > MAX_CHAPTERS.saturating_sub(chapters.len())
    {
        return Err(invalid("expected at least one chapter and at most 4096 total sources"));
    }
    // Admit every source before allocating canonical keys or expanding anything.
    let mut input_bytes = 0usize;
    let mut source_length = 0usize;
    for input in chapters.iter().chain(include_sources) {
        add(&mut input_bytes, input.path.len(), max_bytes, "source text and paths")?;
        add(&mut input_bytes, input.source.len(), max_bytes, "source text and paths")?;
        source_length += input.source.len(); // Bounded by input_bytes above.
    }
    // Only actual chapters need distinct output filenames. Include-only files
    // can have the same stem or flattened name without colliding at publication.
    let ordered = paths::input_paths(chapters)?;
    let mut files = BTreeMap::new();
    for input in chapters.iter().chain(include_sources) {
        let path = logical_path("", &input.path).map_err(invalid)?;
        if files.insert(path.clone(), input.source.as_str()).is_some() {
            return Err(invalid(format!("duplicate normalized source path {path:?}")));
        }
    }
    let resolutions = Cell::new(0usize);
    let copied_bytes = Cell::new(source_length);
    let mut output_bytes = 0usize;
    let mut expanded = Vec::with_capacity(chapters.len());
    for (input, path) in chapters.iter().zip(ordered) {
        add(&mut output_bytes, path.len(), max_bytes, "expanded chapter text and paths")?;
        let remaining = max_bytes - output_bytes;
        let source = if has_includes(&input.source) {
            let resolve = |requested: &str, origin: &str| -> ResolveResult {
                if resolutions.get() >= max_resolutions {
                    return Err(format!("book exceeds {max_resolutions} include resolutions"));
                }
                resolutions.set(resolutions.get() + 1);
                let origin = if origin == "<input>" { path.as_str() } else { origin };
                let parent = origin.rsplit_once('/').map_or("", |(parent, _)| parent);
                let key = logical_path(parent, requested)?;
                let Some(&content) = files.get(&key) else { return Ok(None); };
                // A tiny selected range must not hide repeated whole-file copies
                // required by the resolver API. Charge before creating the String.
                if content.len() > max_bytes.saturating_sub(copied_bytes.get()) {
                    return Err(format!("book source/include copies exceed the {max_bytes}-byte limit"));
                }
                copied_bytes.set(copied_bytes.get() + content.len());
                Ok(Some((content.to_string(), key)))
            };
            expand_includes_with_limit(&input.source, &resolve, remaining)
                .map_err(|error| invalid(format!("{path}: {error}")))?
        } else {
            if input.source.len() > remaining {
                return Err(invalid("expanded chapter text and paths exceed the book limit"));
            }
            input.source.clone()
        };
        add(&mut output_bytes, source.len(), max_bytes, "expanded chapter text and paths")?;
        expanded.push(BookInput { path, source });
    }
    Ok((expanded, source_length))
}

// Same literal-path policy as native book ingress. Percent signs and '#'
// belong to filenames here; selectors have already been removed by transclude.
fn logical_path(parent: &str, requested: &str) -> std::result::Result<String, String> {
    if requested.is_empty() || requested.starts_with(['/', '\\'])
        || requested.ends_with(['/', '\\']) || requested.contains(':')
        || requested.chars().any(char::is_control)
    {
        return Err(format!("include_escape: expected a nonempty relative book path: {requested:?}"));
    }
    let joined = if parent.is_empty() { requested.to_string() } else { format!("{parent}/{requested}") };
    paths::source_path(&joined)
        .ok_or_else(|| format!("include_escape: path leaves the selected book: {requested:?}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn file(path: &str, source: &str) -> BookInput {
        BookInput { path: path.to_string(), source: source.to_string() }
    }

    #[test]
    fn nested_includes_use_their_own_origins_and_resources_are_not_chapters() {
        let chapters = [file("guide/start.md", "# Manual\n\n{{#include ../parts/body.md}}\n"), file("end.md", "# End\n")];
        let resources = [file("parts/body.md", "## Shared\n\n{{#include detail.md}}\n"), file("parts/detail.md", "Nested content.\n")];
        let (expanded, bytes) = prepare(&chapters, &resources, 4096, 64).unwrap();
        assert_eq!(expanded[0].source, "# Manual\n\n## Shared\n\nNested content.\n");
        assert_eq!(expanded[1].path, "end.md");
        let renderer = BookRenderer::from_sources(&chapters, &resources).unwrap();
        assert_eq!(renderer.book().chapters.len(), 2);
        assert_eq!(renderer.source_length(), bytes);
        assert_eq!(bytes, chapters.iter().chain(&resources).map(|file| file.source.len()).sum::<usize>());
        assert_eq!(renderer.book().chapters[0].doc, crate::parse_markdown(&expanded[0].source));
        assert!(chapters[0].source.contains("{{#include"), "input source is never rewritten");
    }

    #[test]
    fn selectors_and_fenced_examples_use_the_shared_transclusion_engine() {
        let chapters = [file("book.md", "{{#include parts.md:chosen}}\n\n{{#include rows.txt:2:3}}\n\n```md\n{{#include missing.md}}\n```\n")];
        let resources = [file("parts.md", "<!-- ANCHOR: chosen -->\nSelected.\n<!-- ANCHOR_END: chosen -->\nIgnored.\n"),
            file("rows.txt", "one\r\ntwo\r\nthree\r\nfour\r\n")];
        let (expanded, _) = prepare(&chapters, &resources, 4096, 64).unwrap();
        assert!(expanded[0].source.contains("Selected.\n"));
        assert!(!expanded[0].source.contains("Ignored."));
        assert!(expanded[0].source.contains("two\r\nthree\r\n"));
        assert!(expanded[0].source.contains("```md\n{{#include missing.md}}\n```"));
    }

    #[test]
    fn source_paths_are_literal_case_sensitive_and_normalized_before_lookup() {
        assert_eq!(logical_path("guide", "../parts/a%20b#c.md").unwrap(), "parts/a%20b#c.md");
        assert_eq!(logical_path("guide", "..\\parts\\x.md").unwrap(), "parts/x.md");
        let chapters = [file("book.md", "{{#include \"a%20b#c.md\"}}\n")];
        let resources = [file("./a%20b#c.md", "Literal key\n")];
        assert_eq!(prepare(&chapters, &resources, 4096, 64).unwrap().0[0].source, "Literal key\n");
        assert!(prepare(&[file("book.md", "{{#include A.md}}")], &[file("a.md", "wrong")], 4096, 64).is_err());
    }

    #[test]
    fn missing_cycles_and_root_escapes_are_errors_not_partial_books() {
        assert!(BookRenderer::from_sources(&[file("book.md", "{{#include missing.md}}")], &[]).is_err());
        assert!(BookRenderer::from_sources(&[file("book.md", "{{#include a.md}}")],
            &[file("a.md", "{{#include book.md}}")]).is_err());
        for path in ["../outside.md", "/absolute.md", "\\absolute.md", "https://host/a.md", "C:\\x.md", "", "a\0.md"] {
            assert!(logical_path("", path).is_err(), "accepted {path:?}");
        }
        assert!(BookRenderer::from_sources(&[file("book.md", "{{#include ../outside.md}}")],
            &[file("outside.md", "not authorized by an escaping path")]).is_err());
    }

    #[test]
    fn duplicate_source_keys_and_chapter_output_collisions_fail_before_expansion() {
        assert!(BookRenderer::from_sources(&[file("a.md", "# A")], &[file("./a.md", "# Other")]).is_err());
        assert!(BookRenderer::from_sources(&[file("a/b.md", "# A"), file("a__b.md", "# B")], &[]).is_err());
        // These resources are never emitted as standalone pages.
        assert!(BookRenderer::from_sources(&[file("book.md", "# Book")],
            &[file("a/b.md", "One"), file("a__b.md", "Two")]).is_ok());
        assert!(BookRenderer::from_sources(&[], &[file("a.md", "# A")]).is_err());
    }

    #[test]
    fn include_call_budget_is_shared_across_chapters_even_for_empty_resources() {
        let chapters = [file("a.md", "{{#include empty.md}}"), file("b.md", "{{#include empty.md}}")];
        let resources = [file("empty.md", "")];
        let error = prepare(&chapters, &resources, 4096, 1).unwrap_err().to_string();
        assert!(error.contains("include resolutions"), "{error}");
    }

    #[test]
    fn repeated_selected_ranges_charge_full_resolver_copies_before_allocation() {
        let chapters = [file("book.md", "{{#include large.txt:1}}\n{{#include large.txt:1}}\n")];
        let resources = [file("large.txt", &format!("short\n{}", "x".repeat(100)))];
        let error = prepare(&chapters, &resources, 300, 64).unwrap_err().to_string();
        assert!(error.contains("source/include copies"), "{error}");
    }

    #[test]
    fn input_byte_budget_counts_paths_and_preserves_untouched_unicode() {
        assert!(prepare(&[file("a.md", "x")], &[], 4, 64).is_err());
        let chapters = [file("a.md", "{{#include empty.md}}\n".repeat(10).as_str())];
        assert!(prepare(&chapters, &[file("empty.md", "")], 64, 64).is_err());
        let (out, _) = prepare(&[file("a.md", "\u{feff}# A\r\n")], &[], 4096, 64).unwrap();
        assert_eq!(out[0].source, "\u{feff}# A\r\n");
    }

    #[test]
    fn expanded_book_budget_is_checked_before_appending_a_shared_fragment() {
        let path = format!("{}.md", "x".repeat(300));
        let chapters = [file(&path, "{{#include p.md}}\n{{#include p.md}}\n")];
        let resources = [file("p.md", &format!("{}\n", "x".repeat(50)))];
        let error = prepare(&chapters, &resources, 400, 64).unwrap_err().to_string();
        assert!(error.contains("include_budget"), "{error}");
    }

    #[test]
    fn expansion_matches_preexpanded_rendering_and_keeps_parse_only_constructor() {
        let raw = [file("book.md", "# Book\n\n{{#include part.md}}\n")];
        let renderer = BookRenderer::from_sources(&raw, &[file("part.md", "A shared paragraph.\n")]).unwrap();
        let plain = BookRenderer::new(&[file("book.md", "# Book\n\nA shared paragraph.\n")]).unwrap();
        assert_eq!(renderer.render_site().unwrap(), plain.render_site().unwrap());
        assert_eq!(renderer.render_epub().unwrap(), plain.render_epub().unwrap());
        assert_eq!(renderer.render_pdf().unwrap(), plain.render_pdf().unwrap());
        let literal = BookRenderer::new(&raw).unwrap();
        assert_ne!(literal.book().chapters[0].doc, renderer.book().chapters[0].doc);
    }
}
