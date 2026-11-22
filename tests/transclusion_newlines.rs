//! Include expansion and source edits share the Markdown parser's line endings.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::Profile;
use franken_markdown::transclude::{
    ExpandedDocument, ResolveResult, expand_includes, expand_includes_mapped,
    expand_includes_with_limit, include_target_spans,
};
use std::cell::RefCell;
use std::collections::BTreeMap;

fn assert_exact_mapping(expanded: &ExpandedDocument, sources: &BTreeMap<&str, &str>) {
    let mut covered = 0;
    for span in &expanded.spans {
        assert_eq!(span.expanded.start, covered);
        covered = span.expanded.end;
        let source = sources[expanded.sources[span.source_id].as_str()];
        if span.generated {
            assert!(span.original.is_empty());
            assert!(source.is_char_boundary(span.original.start));
            assert!(
                expanded.text[span.expanded.clone()]
                    .bytes()
                    .all(|byte| byte == b'\n')
            );
        } else {
            assert_eq!(
                &expanded.text[span.expanded.clone()],
                &source[span.original.clone()]
            );
        }
    }
    assert_eq!(covered, expanded.text.len());
    assert!(expanded.source_at(expanded.text.len()).is_none());
}

#[test]
fn whole_includes_recognize_each_line_ending_and_preserve_fragment_bytes() {
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!("Before{ending}{{{{#include part.md}}}}{ending}After{ending}");
        for (content, inserted) in [
            ("Included\n", "Included\n"),
            ("Included\r\n", "Included\r\n"),
            ("Included\r", "Included\r"),
            ("Included", "Included\n"),
        ] {
            let calls = RefCell::new(Vec::new());
            let resolve = |path: &str, origin: &str| -> ResolveResult {
                calls
                    .borrow_mut()
                    .push((path.to_owned(), origin.to_owned()));
                assert_eq!(path, "part.md");
                Ok(Some((content.into(), "/part.md".into())))
            };
            assert_eq!(
                expand_includes(&source, &resolve).unwrap(),
                format!("Before{ending}{inserted}After{ending}")
            );
            assert_eq!(&*calls.borrow(), &[("part.md".into(), "<input>".into())]);
        }
    }
}

#[test]
fn numbered_selectors_count_logical_lines_without_normalizing_them() {
    for ending in ["\n", "\r\n", "\r"] {
        let content = format!("α{ending}β{ending}γ{ending}");
        let resolve = |path: &str, _: &str| -> ResolveResult {
            assert_eq!(path, "part.md");
            Ok(Some((content.clone(), "/part.md".into())))
        };
        for (selector, expected) in [
            (":2", format!("β{ending}")),
            (":2:3", format!("β{ending}γ{ending}")),
            (":2:", format!("β{ending}γ{ending}")),
            ("::2", format!("α{ending}β{ending}")),
            (":3", format!("γ{ending}")),
        ] {
            let source = format!("{{{{#include part.md{selector}}}}}");
            assert_eq!(expand_includes(&source, &resolve).unwrap(), expected);
        }
        let error = expand_includes("{{#include part.md:4}}", &resolve).unwrap_err();
        assert!(error.to_string().contains("include_range:"));
        assert!(error.to_string().contains("3 available lines"));
    }

    for (content, selector, expected) in [
        ("zero\rα\r\nβ\nfinal", ":2:4", "α\r\nβ\nfinal\n"),
        ("first\r\rthird\r", ":2", "\r"),
    ] {
        let source = format!("{{{{#include part.md{selector}}}}}");
        let resolve = |_: &str, _: &str| Ok(Some((content.into(), "/part.md".into())));
        assert_eq!(expand_includes(&source, &resolve).unwrap(), expected);
    }
}

#[test]
fn named_and_nested_selections_keep_exact_mixed_newline_origins() {
    let root = "Préface\r{{#include parts.md:chosen}}\r\nTail\r";
    let part = "Ignored\r<!-- ANCHOR: chosen -->\r\nα\r<!-- ANCHOR: inner -->\n{{#include child.md:2}}\r<!-- ANCHOR_END: inner -->\r\nω\r<!-- ANCHOR_END: chosen -->\nIgnored";
    let child = "Skipped\r文😀\rUnused";
    let sources = BTreeMap::from([
        ("/root.md", root),
        ("/parts.md", part),
        ("/child.md", child),
    ]);
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, origin: &str| -> ResolveResult {
        calls
            .borrow_mut()
            .push((path.to_owned(), origin.to_owned()));
        let key = match (path, origin) {
            ("parts.md", "/root.md") => "/parts.md",
            ("child.md", "/parts.md") => "/child.md",
            _ => return Err(format!("unexpected include {path} from {origin}")),
        };
        Ok(Some((sources[key].into(), key.into())))
    };
    let expanded = expand_includes_mapped(root, "/root.md", &resolve).unwrap();
    assert_eq!(expanded.text, "Préface\rα\r文😀\rω\rTail\r");
    assert_eq!(
        &*calls.borrow(),
        &[
            ("parts.md".into(), "/root.md".into()),
            ("child.md".into(), "/parts.md".into()),
        ]
    );
    assert_exact_mapping(&expanded, &sources);
    assert!(expanded.spans.iter().all(|span| !span.generated));
    for (text, origin, original) in [
        ("α", "/parts.md", part.find('α').unwrap()),
        ("😀", "/child.md", child.find('😀').unwrap()),
        ("Tail", "/root.md", root.find("Tail").unwrap()),
    ] {
        let offset = expanded.text.find(text).unwrap();
        let location = expanded.source_at(offset).unwrap();
        assert_eq!(location.origin, origin);
        assert_eq!(location.byte_offset, original);
        assert!(!location.generated);
    }
    let emoji = expanded.text.find('😀').unwrap();
    assert!(expanded.source_at(emoji + 1).is_none());
}

#[test]
fn code_boundaries_preserve_literal_directives_without_hiding_following_includes() {
    for ending in ["\n", "\r\n", "\r"] {
        for template in [
            "```md\n{{#include forbidden.md}}\n```\n",
            "Example `\n{{#include forbidden.md}}\n`\n",
            "- ```md\n  {{#include forbidden.md}}\n  ```\n",
            "- ```md\n  {{#include forbidden.md}}\n",
            "    {{#include forbidden.md}}\n",
        ] {
            let literal = template.replace('\n', ending);
            let source = format!("{literal}{ending}{{{{#include actual.md}}}}{ending}");
            let calls = RefCell::new(Vec::new());
            let resolve = |path: &str, origin: &str| -> ResolveResult {
                calls
                    .borrow_mut()
                    .push((path.to_owned(), origin.to_owned()));
                if path != "actual.md" {
                    return Err(format!("code example resolved {path}"));
                }
                Ok(Some(("ACTUAL\r".into(), "/actual.md".into())))
            };
            assert_eq!(
                expand_includes(&source, &resolve).unwrap(),
                format!("{literal}{ending}ACTUAL\r"),
                "{source:?}"
            );
            assert_eq!(&*calls.borrow(), &[("actual.md".into(), "<input>".into())]);
            for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
                let targets = include_target_spans(&source, profile).unwrap();
                assert_eq!(targets.len(), 1, "{source:?}");
                assert_eq!(targets[0].path, "actual.md");
            }
        }
    }
}

#[test]
fn include_path_inventory_retains_original_token_and_line_coordinates() {
    let source = "pré\r```\r{{#include forbidden.md}}\r```\r  {{#include \"déjà:part.md\":2:3}}\r\nBetween\n{{#include tail.md}}\rEND";
    let targets = include_target_spans(source, None).unwrap();
    assert_eq!(targets.len(), 2);
    for (target, token, line, path) in [
        (
            &targets[0],
            "\"déjà:part.md\"",
            "  {{#include \"déjà:part.md\":2:3}}\r\n",
            "déjà:part.md",
        ),
        (&targets[1], "tail.md", "{{#include tail.md}}\r", "tail.md"),
    ] {
        let token_start = source.find(token).unwrap();
        let line_start = source.find(line).unwrap();
        assert_eq!(target.path, path);
        assert_eq!(target.range, token_start..token_start + token.len());
        assert_eq!(target.line, line_start..line_start + line.len());
        assert_eq!(&source[target.range.clone()], token);
        assert_eq!(&source[target.line.clone()], line);
    }
}

#[test]
fn only_unterminated_fragments_gain_a_bounded_generated_separator() {
    let root = "{{#include part.md}}\r";
    for (content, expected, generated) in [
        ("value\r", "value\r", false),
        ("value\r\n", "value\r\n", false),
        ("value\n", "value\n", false),
        ("value", "value\n", true),
        ("", "\n", true),
    ] {
        let resolve = |_: &str, _: &str| Ok(Some((content.into(), "/part.md".into())));
        let expanded = expand_includes_mapped(root, "/root.md", &resolve).unwrap();
        assert_eq!(expanded.text, expected);
        assert_eq!(expanded.spans.iter().any(|span| span.generated), generated);
        assert_exact_mapping(
            &expanded,
            &BTreeMap::from([("/root.md", root), ("/part.md", content)]),
        );
        if generated {
            let separator = expanded.source_at(expected.len() - 1).unwrap();
            assert_eq!(separator.origin, "/part.md");
            assert_eq!(separator.byte_offset, content.len());
            assert!(separator.generated);
        }
        assert_eq!(
            expand_includes_with_limit(root, &resolve, expected.len()).unwrap(),
            expected
        );
        let error = expand_includes_with_limit(root, &resolve, expected.len() - 1).unwrap_err();
        assert!(error.to_string().contains("include_size:"));
    }
}

#[test]
fn independent_cr_and_lf_breaks_cannot_coalesce_at_include_boundaries() {
    for (root, part, expected) in [
        (
            "{{#include part.md}}\n\nAfter\n",
            "Before\r",
            "Before\r\n\nAfter\n",
        ),
        (
            "Before\r{{#include part.md}}\rAfter\r",
            "\nIncluded\r",
            "Before\r\n\nIncluded\rAfter\r",
        ),
        (
            "Before\r{{#include part.md}}\rAfter\r",
            "",
            "Before\r\n\nAfter\r",
        ),
    ] {
        let resolve = |path: &str, _: &str| -> ResolveResult {
            assert_eq!(path, "part.md");
            Ok(Some((part.into(), "/part.md".into())))
        };
        let expanded = expand_includes_mapped(root, "/root.md", &resolve).unwrap();
        assert_eq!(expanded.text, expected);
        assert_exact_mapping(
            &expanded,
            &BTreeMap::from([("/root.md", root), ("/part.md", part)]),
        );
        assert_eq!(
            expand_includes_with_limit(root, &resolve, expected.len()).unwrap(),
            expected
        );
        let error = expand_includes_with_limit(root, &resolve, expected.len() - 1).unwrap_err();
        assert!(error.to_string().contains("include_size:"));
    }
}

#[test]
fn nested_fragment_separators_are_mapped_to_the_preceding_source_boundary() {
    let root = "{{#include outer.md}}\n\nTail\n";
    let outer = "{{#include left.md}}\r{{#include right.md}}\r";
    let left = "A\r";
    let right = "\nB\r";
    let sources = BTreeMap::from([
        ("/root.md", root),
        ("/outer.md", outer),
        ("/left.md", left),
        ("/right.md", right),
    ]);
    let calls = RefCell::new(Vec::new());
    let resolve = |path: &str, origin: &str| -> ResolveResult {
        calls
            .borrow_mut()
            .push((path.to_owned(), origin.to_owned()));
        let key = match (path, origin) {
            ("outer.md", "/root.md") => "/outer.md",
            ("left.md", "/outer.md") => "/left.md",
            ("right.md", "/outer.md") => "/right.md",
            _ => return Err(format!("unexpected include {path} from {origin}")),
        };
        Ok(Some((sources[key].into(), key.into())))
    };
    let expanded = expand_includes_mapped(root, "/root.md", &resolve).unwrap();
    assert_eq!(expanded.text, "A\r\n\nB\r\n\nTail\n");
    assert_eq!(calls.borrow().len(), 3);
    assert_exact_mapping(&expanded, &sources);
    let boundaries: Vec<_> = expanded
        .spans
        .iter()
        .filter(|span| span.generated)
        .map(|span| {
            (
                expanded.sources[span.source_id].as_str(),
                span.original.start,
            )
        })
        .collect();
    assert_eq!(
        boundaries,
        [("/left.md", left.len()), ("/right.md", right.len())]
    );
}
