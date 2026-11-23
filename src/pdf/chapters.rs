//! Book chapter identity independent of painted headings or source anchor IDs.
//!
//! A chapter starts at its first actual body line. Metadata never inserts text,
//! consumes a Markdown heading ordinal, or takes an author's fragment name.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{Line, OutlineEntry, PageGeom};
use crate::{RenderError, Result};

pub(super) const UNASSIGNED: u32 = u32::MAX;

#[derive(Clone, Debug, Default)]
pub(crate) struct Titles {
    names: Vec<Arc<str>>,
}

impl Titles {
    pub(crate) fn new<'a>(names: impl Iterator<Item = &'a str>) -> Result<Self> {
        let mut titles = Self::default();
        for name in names {
            if titles.names.len() >= 4096 {
                return Err(RenderError::InvalidInput(
                    "book_pdf_chapters: expected at most 4096 chapters".into(),
                ));
            }
            titles.names.push(Arc::from(
                name.chars()
                    .map(|ch| if ch.is_control() { ' ' } else { ch })
                    .collect::<String>(),
            ));
        }
        Ok(titles)
    }

    pub(crate) const fn empty() -> Self {
        Self { names: Vec::new() }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub(super) fn get(&self, scope: u32) -> Option<&Arc<str>> {
        scope
            .checked_sub(1)
            .and_then(|index| self.names.get(index as usize))
    }

    pub(super) fn iter(&self) -> impl DoubleEndedIterator<Item = (u32, &Arc<str>)> {
        self.names
            .iter()
            .enumerate()
            .map(|(index, name)| (index as u32 + 1, name))
    }
}

/// Generated contents, page notes and endnote bodies cannot claim a body start.
pub(super) fn body_scope(line: &Line) -> Option<u32> {
    let scope = line.flow.chapter_scope;
    (scope != 0
        && scope != UNASSIGNED
        && line.flow.note == 0
        && line.flow.note_owner == 0
        && !line.flow.document_language)
        .then_some(scope)
}

/// This is an internal lookup key, never a serialized name or source fragment.
/// Heading slugs and admitted URI fragments cannot contain NUL. Chapter links
/// additionally retain their own `LinkTarget` variant throughout the writer.
pub(super) fn target_key(scope: u32) -> String {
    format!("\0fmd-book-chapter:{scope}")
}

pub(super) fn outline(scope: u32, title: &str, page_index: usize, y: f32) -> OutlineEntry {
    OutlineEntry {
        source_identity: None,
        id: target_key(scope),
        title: title.to_string(),
        level: 1,
        chapter_scope: scope,
        chapter_root: true,
        page_index,
        y,
    }
}

/// Empty chapters share the next painted chapter's start, or the final page.
/// They never manufacture a visible heading, a blank page, or a broken action.
pub(super) fn complete_outlines(
    outlines: &mut Vec<OutlineEntry>,
    titles: &Titles,
    page_count: usize,
    page: PageGeom,
) {
    let starts: BTreeMap<_, _> = outlines
        .iter()
        .filter(|entry| entry.chapter_root)
        .map(|entry| (entry.chapter_scope, (entry.page_index, entry.y)))
        .collect();
    let mut next = (page_count.saturating_sub(1), page.top_y());
    for (scope, title) in titles.iter().rev() {
        if let Some(&start) = starts.get(&scope) {
            next = start;
        } else {
            outlines.push(outline(scope, title, next.0, next.1));
        }
    }
}

pub(super) fn complete_page_numbers(
    pages: &mut BTreeMap<String, usize>,
    titles: &Titles,
    page_count: usize,
) {
    let mut next = page_count.max(1);
    for (scope, _) in titles.iter().rev() {
        next = *pages.entry(target_key(scope)).or_insert(next);
    }
}

/// Chapter roots own their headings, including note headings painted later.
/// Source heading levels still control their hierarchy within that chapter.
/// Only a leading heading with the same title and actual destination is omitted
/// from the bookmark list; its source destination remains available to links.
pub(super) fn bookmarks(outlines: &[OutlineEntry]) -> Vec<(&OutlineEntry, u8)> {
    if !outlines.iter().any(|entry| entry.chapter_root) {
        return outlines
            .iter()
            .filter(|entry| entry.level != 0)
            .map(|entry| (entry, entry.level))
            .collect();
    }
    let mut roots = BTreeMap::new();
    let mut children = BTreeMap::<u32, Vec<&OutlineEntry>>::new();
    let mut unscoped = Vec::new();
    for entry in outlines.iter().filter(|entry| entry.level != 0) {
        if entry.chapter_root {
            roots.insert(entry.chapter_scope, entry);
        } else if entry.chapter_scope != 0 && entry.chapter_scope != UNASSIGNED {
            children.entry(entry.chapter_scope).or_default().push(entry);
        } else {
            unscoped.push((entry, entry.level));
        }
    }
    let mut result = Vec::with_capacity(outlines.len());
    for (scope, root) in roots {
        result.push((root, 1));
        if let Some(headings) = children.remove(&scope) {
            for (index, entry) in headings.into_iter().enumerate() {
                if index == 0
                    && entry.title == root.title
                    && entry.page_index == root.page_index
                    && (entry.y - root.y).abs() <= 0.01
                {
                    continue;
                }
                result.push((entry, entry.level.saturating_add(1).min(7)));
            }
        }
    }
    result.extend(unscoped);
    result
}
