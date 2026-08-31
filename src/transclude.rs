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
#[inline(always)]
#[must_use]
pub fn has_includes(src: &str) -> bool {
    src.contains(INCLUDE_PREFIX)
}

/// Maximum include nesting depth. Deeper nesting errors rather than risking
/// pathological expansion.
const MAX_DEPTH: usize = 16;

/// Host resolver result: content, missing, or a policy refusal with a stable
/// detail (sandbox escapes, size caps) that surfaces verbatim.
pub type ResolveResult = std::result::Result<Option<(String, String)>, String>;

/// Expand `{{#include path}}` directives in `src` recursively.
///
/// `resolver(path, origin)` returns `Ok(Some((content, resolved)))` when
/// readable — `resolved` is the host's canonical key for the path (nested
/// includes resolve against IT, keeping relative nesting correct) —
/// `Ok(None)` when the path does not exist (reported as `include_missing`
/// with the chain), or `Err(reason)` for a policy refusal (surfaced
/// verbatim). `origin` is the including document's resolved key (`<input>`
/// at the root).
///
/// # Errors
/// - `include_missing`: the path did not resolve (chain named).
/// - `include_cycle`: a path appears twice in the active include stack.
/// - `include_depth`: nesting exceeded `MAX_DEPTH`.
/// - the resolver's `Err` detail, unchanged.
pub fn expand_includes(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
) -> Result<String> {
    let mut stack = Vec::new();
    expand_inner(src, resolver, &mut stack, 0, "<input>")
}

fn expand_inner(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
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
            let (content, resolved) = match resolver(path, origin) {
                Ok(Some(pair)) => pair,
                Ok(None) => {
                    let mut chain = stack.clone();
                    chain.push(path.to_string());
                    return Err(RenderError::InvalidInput(format!(
                        "include_missing: cannot read {} (chain: {})",
                        path,
                        chain.join(" -> ")
                    )));
                }
                Err(reason) => return Err(RenderError::InvalidInput(reason)),
            };
            if stack.iter().any(|p| p == path || p == &resolved) {
                let mut chain = stack.clone();
                chain.push(path.to_string());
                return Err(RenderError::InvalidInput(format!(
                    "include_cycle: {} forms an include cycle",
                    chain.join(" -> ")
                )));
            }
            stack.push(resolved.clone());
            let expanded = expand_inner(&content, resolver, stack, depth + 1, &resolved)?;
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

    fn resolver(files: &[(&str, &str)]) -> impl Fn(&str, &str) -> ResolveResult {
        let map: BTreeMap<String, String> = files
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |p, _origin| Ok(map.get(p).map(|c| (c.clone(), p.to_string())))
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
    fn policy_refusal_surfaces_verbatim() {
        let r = |_: &str, _: &str| -> ResolveResult {
            Err("include_escape: path leaves the document root".to_string())
        };
        let err = expand_includes("{{#include ../secret.md}}\n", &r).unwrap_err();
        assert!(err.to_string().contains("include_escape"), "{err}");
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
        let files: Vec<(String, String)> = (0..20)
            .map(|i| {
                (
                    format!("d{i}.md"),
                    format!("{{{{#include d{}.md}}}}\n", i + 1),
                )
            })
            .collect();
        let map: BTreeMap<String, String> = files.into_iter().collect();
        let r = move |p: &str, _o: &str| Ok(map.get(p).map(|c| (c.clone(), p.to_string())));
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
