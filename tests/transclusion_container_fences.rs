//! Include examples inside Markdown code must not resolve files or hide real includes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::transclude::{
    ResolveResult, expand_includes, expand_includes_mapped, expand_includes_with_limit,
};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

const REAL: &str = "{{#include actual.md}}";

fn expands_only_real(source: &str) {
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, origin: &str| -> ResolveResult {
        calls
            .borrow_mut()
            .push((path.to_string(), origin.to_string()));
        if path == "actual.md" {
            Ok(Some(("ACTUAL CONTENT".into(), "actual.md".into())))
        } else {
            Err(format!("literal include unexpectedly resolved: {path}"))
        }
    };
    let expected: String = source
        .split_inclusive('\n')
        .map(|line| {
            if line.trim() == REAL {
                "ACTUAL CONTENT\n"
            } else {
                line
            }
        })
        .collect();
    assert_eq!(
        expand_includes(source, &resolve).unwrap(),
        expected,
        "{source:?}"
    );
    assert_eq!(&*calls.borrow(), &[("actual.md".into(), "<input>".into())]);
}

#[test]
fn list_and_quote_fences_preserve_examples_and_following_real_includes() {
    for prefix in [
        "- ```md\n  {{#include missing.md}}\n  ```\n",
        "+ ~~~md\n  {{#include missing.md}}\n  ~~~~\n",
        "3) ```md\n   {{#include missing.md}}\n   ```\n",
        "10. ```md\n    {{#include missing.md}}\n    ```\n",
        "- [x] ```md\n  {{#include missing.md}}\n  ```\n",
        "- outer\n  - ```md\n    {{#include missing.md}}\n    ```\n",
        "- outer\n\n  ```md\n  {{#include missing.md}}\n  ```\n",
        "- outer\nlazy continuation\n\n  ```md\n  {{#include missing.md}}\n  ```\n",
        "> - ```md\n>   {{#include missing.md}}\n>   ```\n",
        "- > ```md\n  > {{#include missing.md}}\n  > ```\n",
        "-\t```md\n\t{{#include missing.md}}\n\t```\n",
    ] {
        expands_only_real(&format!("{prefix}\n{REAL}\n"));
    }
}

#[test]
fn unclosed_fences_end_with_their_container() {
    for prefix in [
        "- ```md\n  {{#include missing.md}}\n",
        "1. ~~~md\n   {{#include missing.md}}\n",
        "> ```md\n> {{#include missing.md}}\n",
        "> - ```md\n>   {{#include missing.md}}\n>\n",
        "- outer\n\n  ```md\n  {{#include missing.md}}\n",
    ] {
        expands_only_real(&format!("{prefix}\n{REAL}\n"));
    }
}

#[test]
fn only_matching_container_fences_close_literal_code() {
    let literal = "- ````markdown\n  ```\n  {{#include shorter.md}}\n  ~~~~\n  {{#include wrong-marker.md}}\n  ```` not a closer\n  {{#include trailing-text.md}}\n      ````\n  {{#include overindented.md}}\n  `````\t\n";
    expands_only_real(&format!("{literal}\n{REAL}\n"));
}

#[test]
fn existing_indented_code_and_directive_scope_are_unchanged() {
    let calls = Cell::new(0);
    let resolve = |_: &str, _: &str| -> ResolveResult {
        calls.set(calls.get() + 1);
        Err("literal source must not resolve".into())
    };
    for source in [
        "before {{#include missing.md}} after\n",
        "> {{#include missing.md}}\n",
        "- {{#include missing.md}}\n",
        "    {{#include missing.md}}\n",
        "\t{{#include missing.md}}\n",
        "```md\n{{#include missing.md}}\n```\n",
        "    ```md\n    {{#include missing.md}}\n    ```\n",
    ] {
        assert_eq!(expand_includes(source, &resolve).unwrap(), source);
    }
    assert_eq!(calls.get(), 0);
    expands_only_real(&format!("  {REAL}\n"));
    // An ordered marker other than 1 cannot interrupt this open paragraph.
    expands_only_real(&format!("Paragraph\n2. ```md\n  {REAL}\n"));
}

#[test]
fn selected_fragments_keep_exact_unicode_crlf_and_source_origins() {
    let part = "α ignored\r\n<!-- ANCHOR: chosen -->\r\n- ```markdown\r\n  {{#include missing.md}}\r\n  ```\r\n\r\n{{#include child.md}}\r\n<!-- ANCHOR_END: chosen -->\r\n";
    let expected = "Préface\r\n\r\n- ```markdown\r\n  {{#include missing.md}}\r\n  ```\r\n\r\nβ child\n\r\nACTUAL\n";
    for selector in ["chosen", "3:7"] {
        let root = format!(
            "Préface\r\n\r\n{{{{#include parts.md:{selector}}}}}\r\n\r\n{{{{#include actual.md}}}}\r\n"
        );
        let sources = BTreeMap::from([
            ("/book/root.md", root.as_str()),
            ("/book/parts.md", part),
            ("/book/child.md", "β child"),
            ("/book/actual.md", "ACTUAL"),
        ]);
        let calls = RefCell::new(Vec::new());
        let resolve = |path: &str, origin: &str| -> ResolveResult {
            calls
                .borrow_mut()
                .push((path.to_string(), origin.to_string()));
            let key = match (path, origin) {
                ("parts.md", "/book/root.md") => "/book/parts.md",
                ("child.md", "/book/parts.md") => "/book/child.md",
                ("actual.md", "/book/root.md") => "/book/actual.md",
                _ => {
                    return Err(format!(
                        "unexpected origin or literal include: {path} in {origin}"
                    ));
                }
            };
            Ok(Some((sources[key].to_string(), key.into())))
        };
        let expanded = expand_includes_mapped(&root, "/book/root.md", &resolve).unwrap();
        assert_eq!(expanded.text, expected);
        assert_eq!(calls.borrow().len(), 3);
        let literal = expanded.text.find("{{#include missing.md}}").unwrap();
        let location = expanded.source_at(literal).unwrap();
        assert_eq!(location.origin, "/book/parts.md");
        assert_eq!(
            location.byte_offset,
            part.find("{{#include missing.md}}").unwrap()
        );
        assert!(!location.generated);
        let mut covered = 0;
        for span in &expanded.spans {
            assert_eq!(span.expanded.start, covered);
            covered = span.expanded.end;
            let origin = expanded.sources[span.source_id].as_str();
            if span.generated {
                assert_eq!(span.original.start, span.original.end);
                assert_eq!(&expanded.text[span.expanded.clone()], "\n");
            } else {
                assert_eq!(
                    &expanded.text[span.expanded.clone()],
                    &sources[origin][span.original.clone()]
                );
            }
        }
        assert_eq!(covered, expanded.text.len());
        assert!(expanded.source_at(expanded.text.len()).is_none());
    }
}

#[test]
fn literal_directives_do_not_consume_resolver_or_output_budgets() {
    let source = format!("- ```md\n  {{{{#include cycle.md}}}}\n  ```\n\n{REAL}\n");
    let expected = source.replace(&format!("{REAL}\n"), "ACTUAL\n");
    let calls = Cell::new(0);
    let resolve = |path: &str, _: &str| -> ResolveResult {
        calls.set(calls.get() + 1);
        Ok(Some((
            if path == "actual.md" {
                "ACTUAL"
            } else {
                "{{#include cycle.md}}\n"
            }
            .into(),
            path.into(),
        )))
    };
    assert_eq!(
        expand_includes_with_limit(&source, &resolve, expected.len()).unwrap(),
        expected
    );
    assert_eq!(calls.get(), 1);
    assert!(
        expand_includes_with_limit(&source, &resolve, expected.len() - 1)
            .unwrap_err()
            .to_string()
            .contains("include_size")
    );
    assert!(
        expand_includes(&format!("{source}{{{{#include cycle.md}}}}\n"), &resolve)
            .unwrap_err()
            .to_string()
            .contains("include_cycle")
    );
}

#[test]
fn many_list_examples_leave_the_following_include_active() {
    let examples = "- ```md\n  {{#include missing.md}}\n  ```\n".repeat(1_500);
    expands_only_real(&format!("{examples}\n{REAL}\n"));
}

#[test]
fn stripped_nested_anchor_markers_do_not_end_a_named_snippet_fence() {
    let part = "<!-- ANCHOR: chosen -->\n- ```md\n  {{#include missing.md}}\n<!-- ANCHOR: nested -->\n  {{#include also-literal.md}}\n<!-- ANCHOR_END: nested -->\n  ```\n\n{{#include actual.md}}\n<!-- ANCHOR_END: chosen -->\n";
    let root = "{{#include part.md:chosen}}\n";
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, origin: &str| -> ResolveResult {
        calls
            .borrow_mut()
            .push((path.to_string(), origin.to_string()));
        match path {
            "part.md" => Ok(Some((part.into(), "/part.md".into()))),
            "actual.md" => Ok(Some(("ACTUAL\n".into(), "/actual.md".into()))),
            _ => Err(format!("literal directive resolved: {path}")),
        }
    };
    let expanded = expand_includes_mapped(root, "/root.md", &resolve).unwrap();
    assert_eq!(
        expanded.text,
        "- ```md\n  {{#include missing.md}}\n  {{#include also-literal.md}}\n  ```\n\nACTUAL\n"
    );
    assert_eq!(
        &*calls.borrow(),
        &[
            ("part.md".into(), "/root.md".into()),
            ("actual.md".into(), "/part.md".into())
        ]
    );
    let offset = expanded.text.find("{{#include also-literal.md}}").unwrap();
    let original = expanded.source_at(offset).unwrap();
    assert_eq!(original.origin, "/part.md");
    assert_eq!(
        original.byte_offset,
        part.find("{{#include also-literal.md}}").unwrap()
    );
    for span in &expanded.spans {
        if !span.generated && expanded.sources[span.source_id] == "/part.md" {
            assert_eq!(
                &expanded.text[span.expanded.clone()],
                &part[span.original.clone()]
            );
        }
    }
}

#[test]
fn container_code_without_include_examples_cannot_hide_a_real_include() {
    for prefix in [
        "- ```md\n  example only\n  ```\n",
        "1. ~~~text\n   example only\n   ~~~\n",
        "- outer\n\n  ```md\n  unclosed example\n",
    ] {
        expands_only_real(&format!("{prefix}\n{REAL}\n"));
    }
}
