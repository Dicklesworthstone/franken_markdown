//! Transclusion (bead qpqv): `{{#include relative/path.md}}` splices another
//! Markdown file's content into the source at that line, recursively.
//!
//! The pure render core never touches the filesystem. A host resolver owns
//! path sandboxing and bounded file reads. Expansion uses one output buffer,
//! checks its budget before every append, and bounds depth and resolver calls.

use crate::{RenderError, Result};

const INCLUDE_PREFIX: &str = "{{#include";
const MAX_DEPTH: usize = 16;
const MAX_RESOLUTIONS: usize = 4096;

/// Default expanded-document byte budget (64 MiB).
pub const DEFAULT_MAX_EXPANDED_BYTES: usize = 64 * 1024 * 1024;

/// True when the source may contain an include directive.
#[inline(always)]
#[must_use]
pub fn has_includes(src: &str) -> bool {
    src.contains(INCLUDE_PREFIX)
}

/// Content and canonical origin key, missing file, or a host policy refusal.
/// The resolver must bound its own reads; returned strings already occupy
/// memory before the expansion engine can check their emitted size.
pub type ResolveResult = std::result::Result<Option<(String, String)>, String>;

/// Expand include directives with a 64 MiB output budget.
///
/// `resolver(path, origin)` returns content and a canonical key. Nested paths
/// resolve against that key; the root origin is `<input>`. Repeated inclusion
/// outside the active chain is allowed. Canonical keys identify cycles.
///
/// # Errors
/// Reports missing/malformed includes, cycles, depth over 16, more than 4096
/// resolver calls, output over 64 MiB, and host policy refusals. No partial
/// expanded document is returned. Use [`expand_includes_with_limit`] to supply
/// a different output budget.
pub fn expand_includes(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
) -> Result<String> {
    expand_includes_with_limit(src, resolver, DEFAULT_MAX_EXPANDED_BYTES)
}

/// Expand into a single shared output buffer, with an explicit byte budget.
///
/// The limit applies to the final UTF-8 bytes, including inserted newlines.
/// It is checked before copying, not after materializing an oversized result.
/// Depth and resolution-count limits apply even when included content is empty.
/// No filesystem, network, clock, or process access occurs here.
///
/// # Errors
/// Same errors as [`expand_includes`], using `max_output_bytes` for output size.
pub fn expand_includes_with_limit(
    src: &str,
    resolver: &dyn Fn(&str, &str) -> ResolveResult,
    max_output_bytes: usize,
) -> Result<String> {
    let mut expansion = Expansion {
        resolver,
        output: String::with_capacity(src.len().min(max_output_bytes)),
        stack: Vec::new(),
        resolutions: 0,
        max_output_bytes,
    };
    expansion.expand(src, 0, "<input>")?;
    Ok(expansion.output)
}

struct Expansion<'a> {
    resolver: &'a dyn Fn(&str, &str) -> ResolveResult,
    output: String,
    stack: Vec<String>,
    resolutions: usize,
    max_output_bytes: usize,
}

impl Expansion<'_> {
    fn append(&mut self, text: &str) -> Result<()> {
        if text.len() > self.max_output_bytes.saturating_sub(self.output.len()) {
            return Err(RenderError::InvalidInput(format!(
                "include_size: expanded document exceeds {} bytes",
                self.max_output_bytes
            )));
        }
        self.output.push_str(text);
        Ok(())
    }

    fn expand(&mut self, src: &str, depth: usize, origin: &str) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(RenderError::InvalidInput(format!(
                "include_depth: include nesting exceeds {MAX_DEPTH} levels (origin {origin})"
            )));
        }
        if !has_includes(src) {
            return self.append(src);
        }
        for line in src.split_inclusive('\n') {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix(INCLUDE_PREFIX) else {
                self.append(line)?;
                continue;
            };
            let path = rest.strip_suffix("}}").map(str::trim).filter(|p| !p.is_empty())
                .ok_or_else(|| RenderError::InvalidInput(format!(
                    "include_missing: malformed or empty include directive in {origin}: {trimmed}"
                )))?;
            if self.resolutions == MAX_RESOLUTIONS {
                return Err(RenderError::InvalidInput(format!(
                    "include_budget: more than {MAX_RESOLUTIONS} include resolutions"
                )));
            }
            self.resolutions += 1;
            let (content, resolved) = match (self.resolver)(path, origin) {
                Ok(Some(pair)) => pair,
                Ok(None) => {
                    let mut chain = self.stack.clone();
                    chain.push(path.to_string());
                    return Err(RenderError::InvalidInput(format!(
                        "include_missing: cannot read {path} (chain: {})", chain.join(" -> ")
                    )));
                }
                Err(reason) => return Err(RenderError::InvalidInput(reason)),
            };
            if self.stack.contains(&resolved) {
                let mut chain = self.stack.clone();
                chain.push(resolved);
                return Err(RenderError::InvalidInput(format!(
                    "include_cycle: {} forms an include cycle", chain.join(" -> ")
                )));
            }
            let start = self.output.len();
            self.stack.push(resolved.clone());
            let result = self.expand(&content, depth + 1, &resolved);
            self.stack.pop();
            result?;
            if !self.output[start..].ends_with('\n') {
                self.append("\n")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeMap;

    fn resolver(files: &[(&str, &str)]) -> impl Fn(&str, &str) -> ResolveResult {
        let map: BTreeMap<String, String> = files.iter()
            .map(|(key, value)| (key.to_string(), value.to_string())).collect();
        move |path, _origin| Ok(map.get(path).map(|text| (text.clone(), path.to_string())))
    }

    #[test]
    fn splices_content_in_place() {
        let resolve = resolver(&[("part.md", "shared **bold** text\n")]);
        assert_eq!(
            expand_includes("# A\n\n{{#include part.md}}\n\nend\n", &resolve).unwrap(),
            "# A\n\nshared **bold** text\n\nend\n"
        );
    }

    #[test]
    fn no_directive_is_identity() {
        let source = "# A\n\nno includes here\n";
        assert_eq!(expand_includes(source, &resolver(&[])).unwrap(), source);
    }

    #[test]
    fn cycle_reports_chain() {
        let resolve = resolver(&[
            ("a.md", "{{#include b.md}}\n"),
            ("b.md", "{{#include a.md}}\n"),
        ]);
        let error = expand_includes("{{#include a.md}}\n", &resolve).unwrap_err().to_string();
        assert!(error.contains("include_cycle"));
        assert!(error.contains("a.md -> b.md -> a.md"));
    }

    #[test]
    fn missing_names_chain() {
        let error = expand_includes("{{#include gone.md}}\n", &resolver(&[])).unwrap_err();
        assert!(error.to_string().contains("include_missing"));
        assert!(error.to_string().contains("gone.md"));
    }

    #[test]
    fn policy_refusal_surfaces_verbatim() {
        let resolve = |_: &str, _: &str| -> ResolveResult {
            Err("include_escape: path leaves the document root".to_string())
        };
        let error = expand_includes("{{#include ../secret.md}}\n", &resolve).unwrap_err();
        assert!(error.to_string().contains("include_escape"));
    }

    #[test]
    fn nested_include_expands_transitively() {
        let resolve = resolver(&[
            ("a.md", "A\n{{#include b.md}}\n"),
            ("b.md", "B\n{{#include c.md}}\n"),
            ("c.md", "C\n"),
        ]);
        assert_eq!(expand_includes("{{#include a.md}}\n", &resolve).unwrap(), "A\nB\nC\n");
    }

    #[test]
    fn depth_cap_errors() {
        let map: BTreeMap<_, _> = (0..20).map(|index| (
            format!("d{index}.md"), format!("{{{{#include d{}.md}}}}\n", index + 1)
        )).collect();
        let resolve = |path: &str, _: &str| {
            Ok(map.get(path).map(|content| (content.clone(), path.to_string())))
        };
        let error = expand_includes("{{#include d0.md}}\n", &resolve).unwrap_err();
        assert!(error.to_string().contains("include_depth"));
    }

    #[test]
    fn malformed_directive_errors() {
        for source in ["{{#include oops.md }\n", "{{#include }}\n"] {
            assert!(expand_includes(source, &resolver(&[])).unwrap_err()
                .to_string().contains("include_missing"));
        }
    }

    #[test]
    fn budget_counts_utf8_and_inserted_newlines_exactly() {
        let resolve = resolver(&[("part", "中")]);
        assert_eq!(expand_includes_with_limit("{{#include part}}", &resolve, 4).unwrap(), "中\n");
        assert!(expand_includes_with_limit("{{#include part}}", &resolve, 3).unwrap_err()
            .to_string().contains("include_size"));
        assert_eq!(expand_includes_with_limit("", &resolve, 0).unwrap(), "");
        assert!(expand_includes_with_limit("a", &resolve, 0).is_err());
    }

    #[test]
    fn repeated_includes_share_one_output_budget() {
        let resolve = resolver(&[("part", "12345\n")]);
        let source = "{{#include part}}\n{{#include part}}\n";
        assert_eq!(expand_includes_with_limit(source, &resolve, 12).unwrap(), "12345\n12345\n");
        assert!(expand_includes_with_limit(source, &resolve, 11).is_err());
    }

    #[test]
    fn exponential_expansion_stops_before_visiting_the_full_tree() {
        let calls = Cell::new(0usize);
        let resolve = |path: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            let index: usize = path.parse().unwrap();
            let content = if index == 12 { "leaf\n".to_string() } else {
                format!("{{{{#include {}}}}}\n{{{{#include {}}}}}\n", index + 1, index + 1)
            };
            Ok(Some((content, path.to_string())))
        };
        let error = expand_includes_with_limit("{{#include 0}}", &resolve, 64).unwrap_err();
        assert!(error.to_string().contains("include_size"));
        assert!(calls.get() < 100, "visited {} nodes", calls.get());
    }

    #[test]
    fn resolver_call_budget_applies_to_empty_includes() {
        let calls = Cell::new(0usize);
        let resolve = |path: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            Ok(Some((String::new(), path.to_string())))
        };
        let source = "{{#include empty}}\n".repeat(MAX_RESOLUTIONS + 1);
        let error = expand_includes_with_limit(&source, &resolve, source.len()).unwrap_err();
        assert!(error.to_string().contains("include_budget"));
        assert_eq!(calls.get(), MAX_RESOLUTIONS);
    }

    #[test]
    fn canonical_keys_allow_same_spelling_in_different_directories() {
        let resolve = |path: &str, origin: &str| -> ResolveResult {
            match (path, origin) {
                ("part", "<input>") => Ok(Some(("{{#include part}}".into(), "first/part".into()))),
                ("part", "first/part") => Ok(Some(("nested".into(), "second/part".into()))),
                _ => Ok(None),
            }
        };
        assert_eq!(expand_includes("{{#include part}}", &resolve).unwrap(), "nested\n");
    }
}
