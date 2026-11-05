//! Inline-code documentation examples must not invoke the include resolver.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse_markdown;
use franken_markdown::transclude::{
    ResolveResult, expand_includes, expand_includes_mapped, expand_includes_with_limit,
};
use std::cell::RefCell;
use std::collections::BTreeMap;

const REAL: &str = "{{#include actual.md}}";

fn expands_actual_only(source: &str) {
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, origin: &str| -> ResolveResult {
        calls
            .borrow_mut()
            .push((path.to_owned(), origin.to_owned()));
        match path {
            "actual.md" => Ok(Some(("ACTUAL\n".into(), "/actual.md".into()))),
            _ => Err(format!("code example invoked resolver: {path}")),
        }
    };
    assert_eq!(
        expand_includes(source, &resolve).unwrap(),
        source.replace(REAL, "ACTUAL"),
        "{source:?}"
    );
    assert_eq!(&*calls.borrow(), &[("actual.md".into(), "<input>".into())]);
}

#[test]
fn multiline_inline_code_keeps_directives_literal() {
    let source = "An example: `\n{{#include missing.md}}\n`\n\n{{#include actual.md}}\n";
    let document = parse_markdown(source);
    assert_eq!(
        document.blocks[0],
        Block::Paragraph(vec![
            Inline::Text("An example: ".into()),
            Inline::Code("{{#include missing.md}}".into()),
        ])
    );
    expands_actual_only(source);
}

#[test]
fn matching_code_delimiters_and_container_boundaries_come_from_the_parser() {
    for length in [1, 2, 4, 9] {
        let ticks = "`".repeat(length);
        let source =
            format!("Example {ticks}\n{{{{#include missing.md}}}}\nclosing {ticks}\n\n{REAL}\n");
        expands_actual_only(&source);
    }
    for example in [
        "Example ``\n{{#include malformed\n` not a closer\n{{#include missing.md}}\n``\n",
        "- Example `\n  {{#include missing.md}}\n  `\n",
        "-\tExample `\n  {{#include missing.md}}\n  `\n",
        "- Outer\n  - Example `\n    {{#include missing.md}}\n    `\n",
        "> Example `\n{{#include missing.md}}\n`\n",
        "- > Example `\n  {{#include missing.md}}\n  `\n",
        // This line is already literal because its directive prefix belongs
        // to the first span. Its next backtick may open the following span.
        "Example `\n{{#include part`x`.md}}\n{{#include missing.md}}\nclosing `\n",
        "Example `\n{{#include \"part` and `more.md\"}}\n{{#include missing.md}}\n`\n",
        // Indented lines are always literal to expansion, even when the block
        // parser treats them as a lazy continuation of the open paragraph.
        "Prose\n    {{#include \"part`x.md\"}}\n{{#include missing.md}}\n`\n",
        "Prose\n\t{{#include \"part`x.md\"}}\n{{#include missing.md}}\n`\n",
    ] {
        expands_actual_only(&format!("{example}\n{REAL}\n"));
    }
}

#[test]
fn code_in_accepted_and_rejected_link_labels_retains_its_literal_ownership() {
    for example in [
        "[Example `\n{{#include missing.md}}\n`](/target)\n",
        "![`\n{{#include missing.md}}\n`](image.svg)\n",
        "![`\n{{#include missing.md}}\n`][image]\n\n[image]: image.svg\n",
        "[outer `\n{{#include missing.md}}\n` [inner](/nested)](/outer)\n",
    ] {
        expands_actual_only(&format!("{example}\n{REAL}\n"));
    }
}

#[test]
fn unmatched_escaped_and_other_block_backticks_do_not_hide_real_includes() {
    for source in [
        format!("Unmatched `\n{REAL}\n"),
        format!("Wrong length ``\n{REAL}\n`\n"),
        format!("Different paragraph `\n\n{REAL}\n\nAnother `\n"),
        format!("# Heading `\n{REAL}\n`\n"),
        format!("- First `\n- Second\n\n{REAL}\n`\n"),
        format!("Escaped \\`\n{REAL}\n`\n"),
        format!("[label](/target \"`\")\n{REAL}\n`\n"),
        format!("![label](image.svg \"`\")\n{REAL}\n`\n"),
        // Keep the existing include policy inside HTML: an attribute's
        // backtick is never Markdown code and cannot acquire a code envelope.
        format!("<span title=\"`\">\n{REAL}\n`\n"),
    ] {
        expands_actual_only(&source);
    }
}

#[test]
fn backticks_in_active_include_paths_cannot_own_following_source() {
    for path in ["part`x`.md", "part`x.md", "part``x``.md", "part`x``.md"] {
        for quote in ["", "\""] {
            let indent = if quote.is_empty() { "" } else { "  " };
            let directive = format!("{indent}{{{{#include {quote}{path}{quote}}}}}");
            let calls = RefCell::new(Vec::new());
            let resolve = |target: &str, origin: &str| -> ResolveResult {
                calls
                    .borrow_mut()
                    .push((target.to_owned(), origin.to_owned()));
                if target == path {
                    Ok(Some(("PART\n".into(), "/part.md".into())))
                } else if target == "actual.md" {
                    Ok(Some(("ACTUAL\n".into(), "/actual.md".into())))
                } else {
                    Err(format!("code example invoked resolver: {target}"))
                }
            };
            for (tail, expected_tail) in [
                (
                    format!("Example: `\n{{{{#include missing.md}}}}\n`\n\n{REAL}\n"),
                    "Example: `\n{{#include missing.md}}\n`\n\nACTUAL\n",
                ),
                (format!("{REAL}\nclosing `\n"), "ACTUAL\nclosing `\n"),
            ] {
                calls.borrow_mut().clear();
                let source = format!("{directive}\n{tail}");
                assert_eq!(
                    expand_includes(&source, &resolve).unwrap(),
                    format!("PART\n{expected_tail}"),
                    "{source:?}"
                );
                assert_eq!(
                    &*calls.borrow(),
                    &[
                        (path.into(), "<input>".into()),
                        ("actual.md".into(), "<input>".into()),
                    ]
                );
            }
        }
    }
}

#[test]
fn source_selections_and_included_files_cannot_supply_each_others_delimiters() {
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, origin: &str| -> ResolveResult {
        calls
            .borrow_mut()
            .push((path.to_owned(), origin.to_owned()));
        match path {
            "part.md" => Ok(Some((
                format!("Example `\n{REAL}\n`\n"),
                "/parts/part.md".into(),
            ))),
            "opener.md" => Ok(Some(("Example `\n".into(), "/parts/opener.md".into()))),
            "actual.md" => Ok(Some(("ACTUAL\n".into(), "/actual.md".into()))),
            _ => Err(format!("unexpected include: {path}")),
        }
    };
    assert_eq!(
        expand_includes("{{#include part.md:2}}\n", &resolve).unwrap(),
        "ACTUAL\n"
    );
    assert_eq!(
        &*calls.borrow(),
        &[
            ("part.md".into(), "<input>".into()),
            ("actual.md".into(), "/parts/part.md".into()),
        ]
    );
    calls.borrow_mut().clear();
    assert_eq!(
        expand_includes(
            &format!("{{{{#include opener.md}}}}\n{REAL}\n`\n"),
            &resolve
        )
        .unwrap(),
        "Example `\nACTUAL\n`\n"
    );
    assert_eq!(
        &*calls.borrow(),
        &[
            ("opener.md".into(), "<input>".into()),
            ("actual.md".into(), "<input>".into()),
        ]
    );
}

#[test]
fn selected_unicode_crlf_examples_keep_exact_origin_maps() {
    let part = "Ignored α\r\n<!-- ANCHOR: chosen -->\r\nExample é ``\r\n{{#include missing.md}}\r\nend ``\r\n\r\n{{#include child.md}}\r\n<!-- ANCHOR_END: chosen -->\r\n";
    for selector in ["chosen", "3:7"] {
        let root = format!("Préface\r\n{{{{#include parts.md:{selector}}}}}\r\n");
        let sources = BTreeMap::from([
            ("/book/root.md", root.as_str()),
            ("/book/parts.md", part),
            ("/book/child.md", "β child"),
        ]);
        let calls = RefCell::new(Vec::new());
        let resolve = |path: &str, origin: &str| -> ResolveResult {
            calls
                .borrow_mut()
                .push((path.to_owned(), origin.to_owned()));
            let key = match (path, origin) {
                ("parts.md", "/book/root.md") => "/book/parts.md",
                ("child.md", "/book/parts.md") => "/book/child.md",
                _ => return Err(format!("unexpected include: {path} from {origin}")),
            };
            Ok(Some((sources[key].into(), key.into())))
        };
        let expanded = expand_includes_mapped(&root, "/book/root.md", &resolve).unwrap();
        assert_eq!(
            expanded.text,
            "Préface\r\nExample é ``\r\n{{#include missing.md}}\r\nend ``\r\n\r\nβ child\n"
        );
        assert_eq!(calls.borrow().len(), 2);
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
            if span.generated {
                assert_eq!(span.original.start, span.original.end);
                assert_eq!(&expanded.text[span.expanded.clone()], "\n");
            } else {
                assert_eq!(
                    &expanded.text[span.expanded.clone()],
                    &sources[expanded.sources[span.source_id].as_str()][span.original.clone()]
                );
            }
        }
        assert_eq!(covered, expanded.text.len());
    }
}

#[test]
fn stripped_anchor_markers_do_not_break_a_named_snippets_inline_code() {
    let part = "<!-- ANCHOR: chosen -->\nExample `\n{{#include missing.md}}\n<!-- ANCHOR: nested -->\n{{#include also-missing.md}}\n<!-- ANCHOR_END: nested -->\n`\n\n{{#include actual.md}}\n<!-- ANCHOR_END: chosen -->\n";
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, origin: &str| -> ResolveResult {
        calls
            .borrow_mut()
            .push((path.to_owned(), origin.to_owned()));
        match path {
            "part.md" => Ok(Some((part.into(), "/part.md".into()))),
            "actual.md" => Ok(Some(("ACTUAL\n".into(), "/actual.md".into()))),
            _ => Err(format!("code example invoked resolver: {path}")),
        }
    };
    let expanded =
        expand_includes_mapped("{{#include part.md:chosen}}\n", "/root.md", &resolve).unwrap();
    assert_eq!(
        expanded.text,
        "Example `\n{{#include missing.md}}\n{{#include also-missing.md}}\n`\n\nACTUAL\n"
    );
    assert_eq!(
        &*calls.borrow(),
        &[
            ("part.md".into(), "/root.md".into()),
            ("actual.md".into(), "/part.md".into()),
        ]
    );
    let literal = expanded.text.find("{{#include also-missing.md}}").unwrap();
    assert_eq!(
        expanded.source_at(literal).unwrap().byte_offset,
        part.find("{{#include also-missing.md}}").unwrap()
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
fn an_abandoned_inline_parse_does_not_leave_speculative_code_ownership() {
    let hostile = format!("{}leaf{}", "[".repeat(32), "](/nested)".repeat(32));
    let protected = "Protected `\n{{#include missing.md}}\n`";
    let degraded = format!("Degraded `\n{REAL}\n` {hostile}");
    let source = format!("{protected}\n\n{degraded}\n\n{protected}\n");
    let document = parse_markdown(&source);
    assert_eq!(
        document.blocks[1],
        Block::Paragraph(vec![Inline::Text(degraded)])
    );
    expands_actual_only(&source);
}

#[test]
fn literal_directives_do_not_spend_resolution_budget_but_count_towards_output_size() {
    let source = format!(
        "Example `\n{}`\n\n{REAL}\n",
        "{{#include recursive.md}}\n".repeat(4_100)
    );
    let expected = source.replace(REAL, "ACTUAL");
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, _: &str| -> ResolveResult {
        calls.borrow_mut().push(path.to_owned());
        match path {
            "actual.md" => Ok(Some(("ACTUAL\n".into(), "/actual.md".into()))),
            _ => Err(format!("code example invoked resolver: {path}")),
        }
    };
    assert_eq!(
        expand_includes_with_limit(&source, &resolve, expected.len()).unwrap(),
        expected
    );
    assert_eq!(&*calls.borrow(), &["actual.md"]);
    assert!(
        expand_includes_with_limit(&source, &resolve, expected.len() - 1)
            .unwrap_err()
            .to_string()
            .contains("include_size")
    );
}
