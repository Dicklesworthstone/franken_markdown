//! Transclusion (bead qpqv): `{{#include relative/path.md}}` splices another
//! Markdown file's content into the source at that line, recursively.
//!
//! The pure render core never touches the filesystem: expansion happens at the
//! shell layer (CLI/book) with a host resolver closure, BEFORE parsing, so the
//! AST sees one fully-spliced document. Includes are whole Markdown documents,
//! so text-level splicing is parse-order-preserving and byte-deterministic.
//!
//! Safety: include cycles error with the cycle path; nesting depth is capped;
//! a missing file errors with the include chain. The resolver owns all path
//! policy (sandboxing, size caps) — this module never reads files.

use crate::{RenderError, Result};

/// Marker: a line containing exactly `{{#include path}}` (optional whitespace
/// around the payload) splices `path`'s content at that position.
const INCLUDE_PREFIX: &str = "{{#include";

/// True when the source may contain an include directive (cheap pre-filter so
/// include-free documents never pay for the expansion walk).
#[must_use]
pub fn has_includes(src: &str) -> bool {
    src.contains(INCLUDE_PREFIX)
}

/// Maximum include nesting depth. Deeper nesting errors rather than risking
/// pathological expansion.
const MAX_DEPTH: usize = 16;

/// Expand `{{#include path}}` directives in `src` recursively.
///
/// `resolver(path, origin)` returns the file's content or None when
/// unreadable. `origin` is the including document's path ("<input>" at the
/// root), so hosts resolve relative includes against the INCLUDING file's
/// directory and enforce their sandbox root.
///
/// # Errors
/// - `include_missing`: the resolver returned None for a path (chain named).
/// - `include_cycle`: a path appears twice in the active include stack.
/// - `include_depth`: nesting exceeded `MAX_DEPTH`.
/// Check whether `src` contains any `{{#include` directives.
#[must_use]
pub fn has_includes(src: &str) -> bool {
    src.contains(INCLUDE_PREFIX)
}

pub fn expand_includes(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> Result<Option<String>, String>,
) -> Result<String> {
    let mut stack = Vec::new();
    expand_inner(src, resolver, &mut stack, 0, "<input>")
}

fn expand_inner(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> Result<Option<String>, String>,
    stack: &mut Vec<String>,
    depth: usize,
    origin: &str,
) -> Result<String> {
    if depth > MAX_DEPTH {
        return Err(RenderError::InvalidInput(format!(
            "include_depth: include nesting exceeds {MAX_DEPTH} levels (origin {origin})"
        )));
    }
    // Fast path: no directive present at all.
    if !src.contains(INCLUDE_PREFIX) {
        return Ok(src.to_string());
    }
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(INCLUDE_PREFIX) {
            let Some(path_part) = rest.strip_suffix("}}") else {
                return Err(RenderError::InvalidInput(format!(
                    "include_missing: malformed include directive in {origin}: {trimmed}"
                )));
            };
            let path = path_part.trim();
            if path.is_empty() {
                return Err(RenderError::InvalidInput(format!(
                    "include_missing: empty include path in {origin}"
                )));
            }
            if stack.iter().any(|p| p == path) {
                let mut chain = stack.clone();
                chain.push(path.to_string());
                return Err(RenderError::InvalidInput(format!(
                    "include_cycle: {} forms an include cycle",
                    chain.join(" -> ")
                )));
            }
            let content = match resolver(path, origin) {
                Ok(Some(content)) => content,
                Ok(None) => {
                    let mut chain = stack.clone();
                    chain.push(path.to_string());
                    return Err(RenderError::InvalidInput(format!(
                        "include_missing: cannot read {} (chain: {})",
                        path,
                        chain.join(" -> ")
                    )));
                }
                Err(reason) => {
                    return Err(RenderError::InvalidInput(reason));
                }
            };
            stack.push(path.to_string());
            let expanded = expand_inner(&content, resolver, stack, depth + 1, path)?;
            stack.pop();
            out.push_str(&expanded);
            if !expanded.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn resolver(files: &[(&str, &str)]) -> impl Fn(&str, &str) -> Option<String> {
        let map: BTreeMap<String, String> = files
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |p: &str, _origin: &str| Ok(map.get(p).cloned())
    }

    #[test]
    fn splices_content_in_place() {
        let r = resolver(&[("part.md", "shared **bold** text\n")]);
        let out = expand_includes("# A\n\n{{#include part.md}}\n\nend\n", &r).unwrap();
        assert_eq!(out, "# A\n\nshared **bold** text\n\nend\n");
    }

    #[test]
    fn no_directive_is_identity() {
        let r = resolver(&[]);
        let src = "# A\n\nno includes here\n";
        assert_eq!(expand_includes(src, &r).unwrap(), src);
    }

    #[test]
    fn cycle_reports_chain() {
        let r = resolver(&[
            ("a.md", "{{#include b.md}}\n"),
            ("b.md", "{{#include a.md}}\n"),
        ]);
        let err = expand_includes("{{#include a.md}}\n", &r).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("include_cycle"), "{msg}");
        assert!(msg.contains("a.md -> b.md -> a.md"), "{msg}");
    }

    #[test]
    fn missing_names_chain() {
        let r = resolver(&[]);
        let err = expand_includes("{{#include gone.md}}\n", &r).unwrap_err();
        assert!(err.to_string().contains("include_missing"));
        assert!(err.to_string().contains("gone.md"));
    }

    #[test]
    fn nested_include_expands_transitively() {
        let r = resolver(&[
            ("a.md", "A\n{{#include b.md}}\n"),
            ("b.md", "B\n{{#include c.md}}\n"),
            ("c.md", "C\n"),
        ]);
        let out = expand_includes("{{#include a.md}}\n", &r).unwrap();
        assert_eq!(out, "A\nB\nC\n");
    }

    #[test]
    fn depth_cap_errors() {
        // a0 includes a1 includes ... past MAX_DEPTH via self-similar chain.
        let files: Vec<(String, String)> = (0..20)
            .map(|i| {
                (
                    format!("d{i}.md"),
                    format!("{{{{#include d{}.md}}}}\n", i + 1),
                )
            })
            .collect();
        let map: BTreeMap<String, String> = files.into_iter().collect();
        let r = move |p: &str, _o: &str| Ok(map.get(p).cloned());
        let err = expand_includes("{{#include d0.md}}\n", &r).unwrap_err();
        assert!(err.to_string().contains("include_depth"), "{err}");
    }

    #[test]
    fn malformed_directive_errors() {
        let r = resolver(&[]);
        let err = expand_includes("{{#include oops.md }\n", &r).unwrap_err();
        assert!(err.to_string().contains("include_missing"), "{err}");
    }
}
