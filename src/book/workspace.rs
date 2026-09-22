//! Transactional source revisions for a retained, parsed book.
//!
//! The source set and reading order are fixed at creation. Replacing chapter
//! or include-only source reuses the shared bounded expansion engine, compares
//! the exact resulting chapter text, and reparses only changed chapters.
//! Rendering options and authorized image/font bytes survive every revision.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;

use super::{BookChapter, BookInput, BookRenderer, first_heading_text, path_stem, paths};
use crate::parse;
use crate::wasm::WasmRenderOptions;
use crate::{RenderError, Result};

const MAX_SOURCES: usize = 4096;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;

/// A successfully committed source revision. Chapter indexes are zero-based,
/// strictly increasing, and refer to the original reading order. They identify
/// actual parser invocations, not guessed dependency invalidations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSourceUpdate {
    pub revision: u32,
    pub source_length: usize,
    pub chapter_count: usize,
    pub changed_sources: usize,
    pub reparsed_chapters: Vec<usize>,
}

impl BookSourceUpdate {
    /// Reports returned by `update_sources` contain at most 4096 indexes, with no source text,
    /// paths, asset payloads, or renderer-controlled diagnostic strings.
    #[must_use]
    pub fn to_json(&self) -> String {
        use std::fmt::Write as _;
        let mut json = format!(
            "{{\"schema\":\"fmd-book-source-update-v1\",\"revision\":{},\"source_length\":{},\"chapter_count\":{},\"changed_sources\":{},\"reparsed_chapters\":[",
            self.revision, self.source_length, self.chapter_count, self.changed_sources,
        );
        for (index, chapter) in self.reparsed_chapters.iter().enumerate() {
            if index != 0 {
                json.push(',');
            }
            let _ = write!(json, "{chapter}");
        }
        json.push_str("]}");
        json
    }
}

/// An editable source capture around the ordinary book renderer.
///
/// Unlike `BookRenderer`, this opt-in type retains original source strings and,
/// for transcluding books, expanded chapter strings. There is no hidden global
/// cache, hash-based equality, filesystem lookup, or asynchronous runtime.
///
/// Read/render methods are available through `Deref<Target = BookRenderer>`.
/// Mutable access is limited to render options and images: replacing the inner
/// renderer would desynchronize its AST from the retained source capture.
#[derive(Debug, Clone)]
pub struct BookWorkspace {
    renderer: BookRenderer,
    chapters: Vec<BookInput>,
    resources: Vec<BookInput>,
    /// None preserves the parse-only constructor, including literal directives.
    expanded: Option<Vec<BookInput>>,
    revision: u32,
}

impl Deref for BookWorkspace {
    type Target = BookRenderer;

    fn deref(&self) -> &Self::Target {
        &self.renderer
    }
}

impl BookWorkspace {
    /// Capture and parse already-expanded sources, in reading order.
    ///
    /// # Errors
    /// Uses the ordinary renderer's path, count, collision and source budgets.
    pub fn new(chapters: &[BookInput]) -> Result<Self> {
        let renderer = BookRenderer::new(chapters)?;
        Ok(Self {
            renderer,
            chapters: capture(chapters)?,
            resources: Vec::new(),
            expanded: None,
            revision: 0,
        })
    }

    /// Expand chapters against the immutable selected source set, then parse
    /// once. Include-only resources never become chapters. The constructor and
    /// every edit use the exact same selectors, cycle checks and whole-book
    /// expansion/copy/resolver limits as `BookRenderer::from_sources`.
    ///
    /// # Errors
    /// Rejects invalid source sets and any include-expansion or parsing ingress
    /// failure. No partially initialized workspace escapes.
    pub fn from_sources(chapters: &[BookInput], resources: &[BookInput]) -> Result<Self> {
        let (expanded, source_length) = BookRenderer::prepare_workspace_sources(chapters, resources)?;
        let renderer = BookRenderer::from_workspace_sources(&expanded, source_length)?;
        Ok(Self {
            renderer,
            chapters: capture(chapters)?,
            resources: capture(resources)?,
            expanded: Some(expanded),
            revision: 0,
        })
    }

    /// Monotonic committed source revision. Exact no-op batches do not advance
    /// it. Asset/presentation changes are independent of this source revision.
    #[must_use]
    pub const fn source_revision(&self) -> u32 {
        self.revision
    }

    /// Change presentation without invalidating the captured source or AST.
    pub fn options_mut(&mut self) -> &mut WasmRenderOptions {
        self.renderer.options_mut()
    }

    /// Delegate atomic image admission to the ordinary renderer.
    ///
    /// # Errors
    /// Same image key/count/byte validation as `BookRenderer::set_image`.
    pub fn set_image(&mut self, destination: &str, bytes: Vec<u8>) -> Result<()> {
        self.renderer.set_image(destination, bytes)
    }

    /// Replace one or more existing chapter/include sources as one transaction.
    /// Paths identify existing captures; adding, deleting, renaming or reordering
    /// sources requires constructing another workspace. Duplicate normalized
    /// paths are rejected, never last-write-wins.
    ///
    /// Every requested source and the projected complete source budget is
    /// validated before copying. Include expansion finishes before any parsing.
    /// Exact expanded-text equality skips parsing, including edits to unused
    /// resources or unselected snippets. Changed ASTs and their full-book link
    /// bindings publish together; unrelated AST allocations and all render
    /// options/assets are retained.
    ///
    /// # Errors
    /// Rejects empty/oversized batches, unknown or duplicate paths, source or
    /// expansion budget violations, missing/cyclic includes, and revision
    /// exhaustion. All failures leave the previous source capture, AST, revision,
    /// metadata and assets intact, so the last successful book remains usable.
    pub fn update_sources(&mut self, updates: &[BookInput]) -> Result<BookSourceUpdate> {
        self.update_with_limit(updates, MAX_SOURCE_BYTES)
    }

    /// Replace source only when the caller still owns the expected revision.
    /// This check happens before update admission, copying, expansion or parsing.
    /// A stale no-op batch is rejected as well: equality is not permission to
    /// publish an operation prepared against another source capture.
    ///
    /// # Errors
    /// Rejects a stale revision, or any error from [`Self::update_sources`].
    pub fn update_sources_at_revision(
        &mut self,
        updates: &[BookInput],
        expected_revision: u32,
    ) -> Result<BookSourceUpdate> {
        if expected_revision != self.revision {
            return Err(invalid("stale source revision; refresh the source capture before editing"));
        }
        self.update_sources(updates)
    }

    fn update_with_limit(&mut self, updates: &[BookInput], limit: usize) -> Result<BookSourceUpdate> {
        if updates.is_empty() || updates.len() > MAX_SOURCES {
            return Err(invalid("expected 1..=4096 source replacements"));
        }
        let mut batch_bytes = 0usize;
        for update in updates {
            add_bytes(&mut batch_bytes, update.path.len(), limit)?;
            add_bytes(&mut batch_bytes, update.source.len(), limit)?;
        }
        let known: BTreeMap<_, _> = self.chapters.iter().chain(&self.resources)
            .enumerate().map(|(index, input)| (input.path.as_str(), index)).collect();
        let mut seen = BTreeSet::new();
        let mut changes = Vec::new();
        let mut removed = 0usize;
        let mut added = 0usize;
        let mut current_bytes = 0usize;
        for input in self.chapters.iter().chain(&self.resources) {
            add_bytes(&mut current_bytes, input.path.len(), MAX_SOURCE_BYTES)?;
            add_bytes(&mut current_bytes, input.source.len(), MAX_SOURCE_BYTES)?;
        }
        for update in updates {
            let path = paths::source_path(&update.path)
                .ok_or_else(|| invalid("invalid source replacement path"))?;
            if !seen.insert(path.clone()) {
                return Err(invalid("duplicate normalized source replacement path"));
            }
            let &index = known.get(path.as_str())
                .ok_or_else(|| invalid("source replacement path is not in this book capture"))?;
            let original = if index < self.chapters.len() {
                &self.chapters[index]
            } else {
                &self.resources[index - self.chapters.len()]
            };
            if original.source != update.source {
                removed += original.source.len();
                added += update.source.len();
                changes.push((index, update.source.as_str()));
            }
        }
        // Compute the final budget, not order-sensitive intermediate totals:
        // shrinking one source may make room for another in the same batch.
        let projected = current_bytes - removed;
        if added > limit.saturating_sub(projected) || projected > limit {
            return Err(invalid("complete source text and paths exceed the source byte limit"));
        }
        if changes.is_empty() {
            return Ok(self.report(0, Vec::new()));
        }
        let revision = self.revision.checked_add(1)
            .ok_or_else(|| invalid("source revision exhausted; create a new workspace"))?;
        let source_length = self.renderer.source_length() - removed + added;
        let mut chapters = self.chapters.clone();
        let mut resources = self.resources.clone();
        for &(index, source) in &changes {
            let target = if index < chapters.len() {
                &mut chapters[index]
            } else {
                &mut resources[index - chapters.len()]
            };
            source.clone_into(&mut target.source);
        }
        let expanded = if self.expanded.is_some() {
            let (expanded, _) = BookRenderer::prepare_workspace_sources(&chapters, &resources)?;
            Some(expanded)
        } else {
            None
        };
        let next = expanded.as_deref().unwrap_or(&chapters);
        let previous = self.expanded.as_deref().unwrap_or(&self.chapters);
        let mut replacements = Vec::with_capacity(chapters.len());
        let mut reparsed = Vec::new();
        for (index, (new, old)) in next.iter().zip(previous).enumerate() {
            if new.source == old.source {
                replacements.push(None);
            } else {
                replacements.push(Some(parse_chapter(new, &self.renderer.book().chapters[index])));
                reparsed.push(index);
            }
        }
        let report = BookSourceUpdate {
            revision,
            source_length,
            chapter_count: chapters.len(),
            changed_sources: changes.len(),
            reparsed_chapters: reparsed,
        };
        // No fallible operation follows publication. Keep the original options
        // and asset vectors, rather than cloning large image/font payloads.
        self.renderer.commit_workspace_sources(replacements, source_length);
        self.chapters = chapters;
        self.resources = resources;
        self.expanded = expanded;
        self.revision = revision;
        Ok(report)
    }

    fn report(&self, changed_sources: usize, reparsed_chapters: Vec<usize>) -> BookSourceUpdate {
        BookSourceUpdate {
            revision: self.revision,
            source_length: self.renderer.source_length(),
            chapter_count: self.chapters.len(),
            changed_sources,
            reparsed_chapters,
        }
    }
}

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("book_update: {message}"))
}

fn add_bytes(total: &mut usize, bytes: usize, limit: usize) -> Result<()> {
    if bytes > limit.saturating_sub(*total) {
        return Err(invalid("source text and paths exceed the source byte limit"));
    }
    *total += bytes;
    Ok(())
}

fn capture(inputs: &[BookInput]) -> Result<Vec<BookInput>> {
    inputs.iter().map(|input| {
        let path = paths::source_path(&input.path)
            .ok_or_else(|| invalid("invalid captured source path"))?;
        Ok(BookInput { path, source: input.source.clone() })
    }).collect()
}

fn parse_chapter(input: &BookInput, previous: &BookChapter) -> BookChapter {
    let (frontmatter, _) = parse::split_frontmatter(&input.source);
    let doc = parse::parse_document(&input.source);
    let title = frontmatter.as_ref().and_then(|value| value.title.clone())
        .or_else(|| first_heading_text(&doc)).unwrap_or_else(|| path_stem(&previous.path));
    BookChapter {
        path: previous.path.clone(),
        out_name: previous.out_name.clone(),
        title,
        frontmatter,
        doc,
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
