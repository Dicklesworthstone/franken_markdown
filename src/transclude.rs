//! Transclusion (bead qpqv): `{{#include relative/path.md}}` splices another
//! Markdown file's content into the source at that line, recursively.
//!
//! The pure render core never touches the filesystem. A host resolver owns
//! path sandboxing and bounded file reads. Expansion uses one output buffer,
//! checks its budget before every append, and bounds depth and resolver calls.
//! Top-level fenced and indented code remains literal: documentation examples
//! cannot trigger include resolution merely by containing a directive.

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
        let mut fence: Option<(u8, usize)> = None;
        for line in src.split_inclusive('\n') {
            let candidate = fence_candidate(line);
            if let Some((marker, length)) = fence {
                if candidate.is_some_and(|(next, count, tail)| {
                    next == marker && count >= length
                        && tail.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
                }) {
                    fence = None;
                }
                self.append(line)?;
                continue;
            }
            if let Some((marker, length, tail)) = candidate {
                if marker == b'~' || !tail.contains('`') {
                    fence = Some((marker, length));
                    self.append(line)?;
                    continue;
                }
            }
            if indented_code(line) {
                self.append(line)?;
                continue;
            }
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


/// Only up to three leading ASCII spaces are allowed before a fence.
fn fence_candidate(line: &str) -> Option<(u8, usize, &str)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let indent = line.bytes().take_while(|&byte| byte == b' ').count();
    if indent > 3 { return None; }
    let text = &line[indent..];
    let marker = *text.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') { return None; }
    let count = text.bytes().take_while(|&byte| byte == marker).count();
    (count >= 3).then(|| (marker, count, &text[count..]))
}

fn indented_code(line: &str) -> bool {
    let mut column = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => column += 1,
            b'\t' => column += 4 - column % 4,
            _ => break,
        }
        if column >= 4 { return true; }
    }
    false
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


    #[test]
    fn literal_code_examples_never_invoke_the_resolver() {
        let calls = Cell::new(0usize);
        let resolve = |_: &str, _: &str| -> ResolveResult {
            calls.set(calls.get() + 1);
            Err("must not resolve literal code".into())
        };
        for source in [
            "```markdown\n{{#include missing.md}}\n```\n",
            "~~~ example\n{{#include missing.md}}\n~~~\n",
            "    {{#include missing.md}}\n",
            "\t{{#include missing.md}}\n",
            "  \t{{#include missing.md}}\n",
            "````markdown\n```\n{{#include missing.md}}\n````\n",
            "```\n~~~\n{{#include missing.md}}\n```\n",
            "```\n{{#include missing.md}}\n",
        ] {
            assert_eq!(expand_includes(source, &resolve).unwrap(), source);
        }
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn real_directives_resume_after_a_matching_fence() {
        let resolve = resolver(&[("real.md", "included")]);
        let source = "   ```md\r\n{{#include missing.md}}\r\n   `````\t\r\n{{#include real.md}}\n";
        let expected = "   ```md\r\n{{#include missing.md}}\r\n   `````\t\r\nincluded\n";
        assert_eq!(expand_includes(source, &resolve).unwrap(), expected);
    }

    #[test]
    fn malformed_closing_fences_do_not_reenable_includes() {
        let resolve = resolver(&[]);
        let source = "```\n``` not a close\n{{#include missing.md}}\n    ```\n{{#include missing.md}}\n```\n";
        assert_eq!(expand_includes(source, &resolve).unwrap(), source);
    }
}
