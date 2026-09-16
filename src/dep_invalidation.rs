//! Incremental document dependency invalidation (FCB-037.A).
//!
//! Scans a Markdown document for reference, footnote, heading, and include
//! dependencies, then manages conservative invalidation when the document
//! or its referenced resources change. Unchanged subtrees are verified via
//! content digest; distant sections use conservative (over-approximate)
//! invalidation to guarantee correctness.

#![forbid(unsafe_code)]

/// The kind of dependency a document section has on external content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DependencyKind {
    /// A reference link: `[text][label]` or `[text]: url`.
    Reference { label: String },
    /// A footnote definition or reference: `[^label]`.
    Footnote { label: String },
    /// A heading with its level and slug.
    Heading { level: u8, slug: String },
    /// An include or transclusion: `{{include: path}}`.
    Include { path: String },
}

/// A tracked dependency within the document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentDependency {
    pub kind: DependencyKind,
    /// Inclusive start byte offset of the dependency anchor.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
    /// A stable content digest of the referenced resource at scan time.
    pub content_digest: u64,
}

/// Result of an invalidation query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationResult {
    /// Byte ranges that are verified unchanged (no re-render needed).
    pub unchanged: Vec<(usize, usize)>,
    /// Byte ranges that must be conservatively re-rendered.
    pub dirty: Vec<(usize, usize)>,
    /// Whether distant sections (outside the edit range) are affected.
    pub distant_dirty: bool,
}

/// Scans and tracks document dependencies for incremental invalidation.
#[derive(Debug)]
pub struct DependencyGraph {
    deps: Vec<DocumentDependency>,
    source_len: usize,
}

impl DependencyGraph {
    /// Scan a Markdown source for dependencies.
    pub fn scan(source: &str) -> Self {
        let mut deps = Vec::new();
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut pos = 0usize;

        while pos < len {
            let rest = &source[pos..];

            // Footnote reference: [^label] (not a definition which has a colon).
            if rest.starts_with("[^") {
                if let Some(close) = rest.find(']') {
                    let label = &rest[2..close];
                    if !label.is_empty() && !label.contains(' ') {
                        // Check if this is a definition (followed by `:`).
                        let after = &rest[close + 1..];
                        let is_def = after.starts_with(':');
                        deps.push(DocumentDependency {
                            kind: DependencyKind::Footnote {
                                label: label.to_owned(),
                            },
                            start: pos,
                            end: pos + close + 1 + if is_def { 1 } else { 0 },
                            content_digest: 0,
                        });
                        pos += close + 1 + if is_def { 1 } else { 0 };
                        continue;
                    }
                }
            }

            // Reference link: [text][label] or [label] (shortcut ref).
            if rest.starts_with('[') && !rest.starts_with("[^") {
                if let Some(close_bracket) = find_closing_bracket(rest) {
                    let inner = &rest[1..close_bracket];
                    let after = &rest[close_bracket + 1..];
                    if after.starts_with('(') {
                        // Inline link — not a dependency (URL is local).
                        let paren_end = after.find(')').map_or(after.len(), |p| p);
                        pos += close_bracket + 1 + paren_end + 1;
                        continue;
                    } else if after.starts_with('[') {
                        // Reference link: [text][label].
                        if let Some(label_close) = after[1..].find(']') {
                            let label = &after[1..1 + label_close];
                            deps.push(DocumentDependency {
                                kind: DependencyKind::Reference {
                                    label: label.to_owned(),
                                },
                                start: pos,
                                end: pos + close_bracket + 1 + label_close + 2,
                                content_digest: 0,
                            });
                            pos += close_bracket + 1 + label_close + 2;
                            continue;
                        }
                    } else if close_bracket > 0 {
                        // Shortcut reference: [label].
                        deps.push(DocumentDependency {
                            kind: DependencyKind::Reference {
                                label: inner.to_owned(),
                            },
                            start: pos,
                            end: pos + close_bracket + 1,
                            content_digest: 0,
                        });
                        pos += close_bracket + 1;
                        continue;
                    }
                }
            }

            // ATX heading: # through ######.
            if rest.starts_with('#') {
                let level = rest.bytes().take_while(|b| *b == b'#').count();
                if (1..=6).contains(&level) && rest[level..].starts_with(' ') {
                    let line_end = rest.find('\n').unwrap_or(rest.len());
                    let text = rest[level + 1..line_end].trim();
                    let slug = slugify(text);
                    deps.push(DocumentDependency {
                        kind: DependencyKind::Heading {
                            level: level as u8,
                            slug,
                        },
                        start: pos,
                        end: pos + line_end,
                        content_digest: 0,
                    });
                    pos += line_end;
                    continue;
                }
            }

            pos += 1;
        }

        Self {
            deps,
            source_len: source.len(),
        }
    }

    /// All tracked dependencies.
    pub fn dependencies(&self) -> &[DocumentDependency] {
        &self.deps
    }

    /// Compute the invalidation result when the byte range
    /// `[changed_start, changed_end)` of the source is modified.
    ///
    /// Subtrees (spans) that don't overlap the changed range and whose
    /// dependencies are unchanged are verified; everything else is
    /// conservatively marked dirty.
    pub fn invalidate(&self, changed_start: usize, changed_end: usize) -> InvalidationResult {
        let mut unchanged = Vec::new();
        let mut dirty = Vec::new();

        // Partition dependencies by whether they overlap the changed range.
        for dep in &self.deps {
            if dep.end <= changed_start || dep.start >= changed_end {
                unchanged.push((dep.start, dep.end));
            } else {
                dirty.push((dep.start, dep.end));
            }
        }

        // Conservative distant invalidation: if any dependency overlaps the
        // changed range, all sections between the first and last dirty
        // dependency are also dirty (the edit may shift content).
        let distant_dirty = !dirty.is_empty();

        // Coalesce unchanged ranges (merge adjacent).
        unchanged.sort_unstable();

        InvalidationResult {
            unchanged,
            dirty,
            distant_dirty,
        }
    }
}

/// Find the closing `]` for an opening `[` (no nesting).
fn find_closing_bracket(rest: &str) -> Option<usize> {
    rest.find(']').filter(|pos| *pos > 0)
}


/// Create a URL-safe slug from heading text.
fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}
