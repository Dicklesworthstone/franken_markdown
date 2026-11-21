//! Removing a reference prefix must preserve the paragraph it already opened.
//!
//! The expected structures were checked against the independent CommonMark
//! 0.31.2 reference implementation. A blank line still ends that paragraph;
//! a reference definition alone does not permit indented code or a non-1 list
//! to interrupt it. No reference parser is a production or test dependency.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans_with_profile, reference_label_spans_with_profile};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, Profile, parse_markdown_profiled_with_profile,
    parse_markdown_spanned_profiled_with_profile, parse_markdown_spanned_with_profile,
    parse_markdown_with_profile,
};

const PROFILES: [Option<Profile>; 3] = [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)];

fn document(source: &str, profile: Option<Profile>) -> Document {
    let parsed = parse_markdown_with_profile(source, profile);
    assert_eq!(
        parse_markdown_profiled_with_profile(source, profile).document,
        parsed
    );
    assert_eq!(
        parse_markdown_spanned_with_profile(source, profile).to_document(),
        parsed
    );
    assert_eq!(
        parse_markdown_spanned_profiled_with_profile(source, profile)
            .document
            .to_document(),
        parsed
    );
    parsed
}

fn fragment(source: &str, profile: Option<Profile>) -> String {
    franken_markdown::html::render_fragment(
        &document(source, profile).blocks,
        &HtmlOptions {
            profile,
            ..HtmlOptions::default()
        },
    )
}

#[test]
fn reference_prefixes_do_not_turn_hanging_prose_into_literal_code() {
    for profile in PROFILES {
        for definition in [
            "[r]: /target\n",
            "[r]: /target\n[s]: /second\n",
            "[r]: /target\n  \"title\"\n",
            "[r]:\n  /target\n  \"title\"\n",
        ] {
            for indent in ["    ", "        ", "\t", " \t"] {
                let source = format!("{definition}{indent}**visible** [r]\n");
                let parsed = document(&source, profile);
                assert!(
                    matches!(&parsed.blocks[..], [Block::Paragraph(_)]),
                    "{source:?}: {parsed:?}"
                );
                let html = fragment(&source, profile);
                assert!(
                    html.starts_with("<p><strong>visible</strong> <a href=\"/target\""),
                    "{source:?}: {html}"
                );
                assert!(html.ends_with(">r</a></p>\n"), "{html}");
            }
        }
    }
}

#[test]
fn next_line_titles_accept_indentation_but_invalid_titles_remain_visible_prose() {
    for profile in PROFILES {
        for indent in ["    ", "        ", "\t", " \t"] {
            for title in [
                "\"Café &amp; guide\"",
                "'Café &amp; guide'",
                "(Café &amp; guide)",
            ] {
                let source = format!("[r]: /target\n{indent}{title}\n\n[r]\n");
                assert_eq!(
                    document(&source, profile).blocks,
                    [Block::Paragraph(vec![Inline::Link {
                        dest: "/target".into(),
                        title: Some("Café & guide".into()),
                        content: vec![Inline::Text("r".into())],
                    }])],
                    "{source:?}"
                );
                let source =
                    format!("[r]: /target\n{indent}{title} extra\n[fake]: /phantom\n\n[fake]\n");
                let parsed = document(&source, profile);
                assert!(
                    matches!(
                        &parsed.blocks[..],
                        [Block::Paragraph(_), Block::Paragraph(_)]
                    ),
                    "{source:?}: {parsed:?}"
                );
                assert_eq!(
                    parsed.blocks[1],
                    Block::Paragraph(vec![Inline::Text("[fake]".into())])
                );
                assert_eq!(destination_spans_with_profile(&source, profile).len(), 1);
            }
        }
    }
}

#[test]
fn hanging_definition_prefixes_keep_original_targets_and_end_at_blank_or_visible_prose() {
    for profile in PROFILES {
        for indent in ["    ", "        ", "\t", " \t"] {
            for newline in ["\n", "\r\n", "\r"] {
                let source = format!(
                    "\u{feff}[a]: /first{newline}{indent}[b]: /réel{newline}{indent}[c]:{newline}{indent}/third{newline}{indent}'title'{newline}{newline}[b] [c]{newline}"
                );
                assert_eq!(
                    document(&source, profile).blocks,
                    [Block::Paragraph(vec![
                        Inline::Link {
                            dest: "/réel".into(),
                            title: None,
                            content: vec![Inline::Text("b".into())],
                        },
                        Inline::Text(" ".into()),
                        Inline::Link {
                            dest: "/third".into(),
                            title: Some("title".into()),
                            content: vec![Inline::Text("c".into())],
                        },
                    ])],
                    "{source:?}"
                );
                let destinations = destination_spans_with_profile(&source, profile);
                assert_eq!(
                    destinations
                        .iter()
                        .map(|span| span.dest.as_str())
                        .collect::<Vec<_>>(),
                    ["/first", "/réel", "/third"],
                    "{source:?}"
                );
                for destination in destinations {
                    assert_eq!(&source[destination.range], destination.dest);
                }
                let labels = reference_label_spans_with_profile(&source, profile);
                assert_eq!(labels.len(), 5, "{source:?}: {labels:?}");
                for label in labels {
                    assert_eq!(&source[label.label.unwrap()], label.key);
                }
            }
            for boundary in ["\n", "visible\n"] {
                let source = format!("[a]: /first\n{boundary}{indent}[b]: /literal\n\n[b]\n");
                let parsed = document(&source, profile);
                assert_eq!(
                    parsed.blocks.last(),
                    Some(&Block::Paragraph(vec![Inline::Text("[b]".into())])),
                    "{source:?}: {parsed:?}"
                );
                let destinations = destination_spans_with_profile(&source, profile);
                assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
                assert_eq!(destinations[0].dest, "/first");
            }
        }
    }
}

#[test]
fn noninterrupting_markers_after_a_reference_stay_in_the_paragraph() {
    for profile in PROFILES {
        for body in [
            "2. item",
            "0) item",
            "-",
            "*",
            "===",
            "    # heading",
            "    > quote",
            "    ~~~",
        ] {
            let source = format!("[r]: /target\n{body}\n[fake]: /phantom\n\n[fake]\n");
            let parsed = document(&source, profile);
            assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
            assert_eq!(
                parsed.blocks[0],
                Block::Paragraph(vec![
                    Inline::Text(body.trim_start().into()),
                    Inline::SoftBreak,
                    Inline::Text("[fake]: /phantom".into()),
                ]),
                "{source:?}"
            );
            assert_eq!(
                parsed.blocks[1],
                Block::Paragraph(vec![Inline::Text("[fake]".into())])
            );
            let destinations = destination_spans_with_profile(&source, profile);
            assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
            assert_eq!(destinations[0].dest, "/target");
        }
    }
}

#[test]
fn explicit_nested_continuations_keep_links_without_granting_phantom_definitions() {
    for profile in PROFILES {
        for (opening, continuation) in [("> ", "> "), ("- ", "  "), ("> - ", ">   ")] {
            let source = format!(
                "{opening}[r]: /real\n{continuation}    [hidden](/literal)\n{continuation}[fake]: /phantom\n\n[fake]\n"
            );
            let parsed = document(&source, profile);
            assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
            assert_eq!(
                parsed.blocks[1],
                Block::Paragraph(vec![Inline::Text("[fake]".into())])
            );
            let html = fragment(&source, profile);
            assert!(
                html.contains("<a href=\"/literal\">hidden</a>\n[fake]: /phantom"),
                "{source:?}: {html}"
            );
            assert!(!html.contains("<pre>"), "{html}");
            let destinations = destination_spans_with_profile(&source, profile);
            assert_eq!(
                destinations
                    .iter()
                    .map(|span| span.dest.as_str())
                    .collect::<Vec<_>>(),
                ["/real", "/literal"]
            );
            for destination in destinations {
                assert_eq!(&source[destination.range], destination.dest);
            }
            assert_eq!(
                reference_label_spans_with_profile(&source, profile).len(),
                1
            );
        }
    }
}

#[test]
fn blank_lines_reopen_real_code_and_list_boundaries() {
    for profile in PROFILES {
        for (body, expected) in [
            ("    **literal**", "<pre><code>**literal**\n</code></pre>\n"),
            ("2. item", "<ol start=\"2\">\n<li>item</li>\n</ol>\n"),
            ("-", "<ul>\n<li></li>\n</ul>\n"),
        ] {
            for gap in ["\n", " \t\n", "\n\n"] {
                let source = format!("[r]: /target\n{gap}{body}\n");
                assert_eq!(fragment(&source, profile), expected, "{source:?}");
            }
        }
    }
}

#[test]
fn genuine_paragraph_interruptions_still_start_their_own_blocks() {
    for profile in PROFILES {
        for (body, expected) in [
            ("# Heading", "<h1 id=\"heading\">Heading</h1>\n"),
            ("1. item", "<ol>\n<li>item</li>\n</ol>\n"),
            ("- item", "<ul>\n<li>item</li>\n</ul>\n"),
            ("> quote", "<blockquote>\n<p>quote</p>\n</blockquote>\n"),
            ("```\ncode\n```", "<pre><code>code\n</code></pre>\n"),
        ] {
            let source = format!("[r]: /target\n{body}\n");
            assert_eq!(fragment(&source, profile), expected, "{source:?}");
        }
    }
}

#[test]
fn reference_prefixes_preserve_table_and_definition_list_conversion() {
    for profile in PROFILES {
        let table = "| Key |\n| --- |\n| [cell](/literal) |";
        let source = format!("[r]: /target\n{table}\n[s]: /second\n\n[r] [s]\n");
        let parsed = document(&source, profile);
        assert!(matches!(
            &parsed.blocks[..],
            [Block::Table(_), Block::Paragraph(_)]
        ));
        let spanned = parse_markdown_spanned_with_profile(&source, profile);
        assert_eq!(spanned.blocks[0].span.slice(&source), Some(table));
        assert_eq!(spanned.blocks[1].span.slice(&source), Some("[r] [s]"));
        assert_eq!(
            destination_spans_with_profile(&source, profile)
                .iter()
                .map(|span| span.dest.as_str())
                .collect::<Vec<_>>(),
            ["/target", "/literal", "/second"]
        );

        let source = "[r]: /target\nTerm\n: [definition](/literal)\n\n[r]\n";
        let parsed = document(source, profile);
        assert_eq!(parsed.blocks.len(), 2);
        if profile == Some(Profile::CommonMarkGfm) {
            assert!(matches!(parsed.blocks[0], Block::Paragraph(_)));
        } else {
            assert!(matches!(parsed.blocks[0], Block::DefinitionList(_)));
        }
        assert_eq!(
            destination_spans_with_profile(source, profile)
                .iter()
                .map(|span| span.dest.as_str())
                .collect::<Vec<_>>(),
            ["/target", "/literal"]
        );
    }
}

#[test]
fn malformed_leading_definitions_warn_without_granting_later_reference_authority() {
    let source = "[r]: /target\n[bad]:\n[fake]: /phantom\n\n[fake]\n";
    for profile in PROFILES {
        let parsed = document(source, profile);
        assert_eq!(parsed.blocks.len(), 2);
        assert_eq!(
            parsed.blocks[1],
            Block::Paragraph(vec![Inline::Text("[fake]".into())])
        );
        let spanned = parse_markdown_spanned_with_profile(source, profile);
        assert_eq!(spanned.diagnostics.len(), 1, "{:?}", spanned.diagnostics);
        assert_eq!(spanned.diagnostics[0].span.slice(source), Some("[bad]:"));
        assert!(
            spanned.diagnostics[0]
                .message
                .contains("malformed link reference")
        );
        let destinations = destination_spans_with_profile(source, profile);
        assert_eq!(destinations.len(), 1);
        assert_eq!(destinations[0].dest, "/target");
        assert_eq!(reference_label_spans_with_profile(source, profile).len(), 1);
    }
}

#[test]
fn padded_leading_definitions_keep_original_reference_ranges() {
    for profile in PROFILES {
        for newline in ["\n", "\r\n", "\r"] {
            for indent in ["    ", "\t", " \t"] {
                let source = format!(
                    "\u{feff}> [r]: /first{newline}{indent}[s]: /réel{newline}{newline}[s]{newline}"
                );
                assert_eq!(
                    document(&source, profile).blocks,
                    [
                        Block::BlockQuote(Vec::new()),
                        Block::Paragraph(vec![Inline::Link {
                            dest: "/réel".into(),
                            title: None,
                            content: vec![Inline::Text("s".into())],
                        }]),
                    ],
                    "{source:?}"
                );
                let spanned = parse_markdown_spanned_with_profile(&source, profile);
                assert_eq!(spanned.blocks[1].span.slice(&source), Some("[s]"));
                assert!(spanned.diagnostics.is_empty());
                let destinations = destination_spans_with_profile(&source, profile);
                assert_eq!(
                    destinations
                        .iter()
                        .map(|span| span.dest.as_str())
                        .collect::<Vec<_>>(),
                    ["/first", "/réel"]
                );
                for destination in destinations {
                    assert_eq!(&source[destination.range], destination.dest);
                }
                let labels = reference_label_spans_with_profile(&source, profile);
                assert_eq!(labels.len(), 3);
                for label in labels {
                    assert_eq!(&source[label.label.unwrap()], label.key);
                }

                // A failed candidate still starts visible lazy prose. Neither
                // it nor a later reference-looking line may gain authority.
                let source = format!(
                    "> [r]: /first{newline}{indent}[bad]:{newline}{indent}[fake]: /phantom{newline}{newline}[fake]{newline}"
                );
                let parsed = document(&source, profile);
                assert_eq!(
                    parsed.blocks.last(),
                    Some(&Block::Paragraph(vec![Inline::Text("[fake]".into())]))
                );
                let destinations = destination_spans_with_profile(&source, profile);
                assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
                assert_eq!(destinations[0].dest, "/first");
                assert!(
                    parse_markdown_spanned_with_profile(&source, profile)
                        .diagnostics
                        .is_empty()
                );
            }
        }
    }
}

#[test]
fn reference_prefixes_preserve_footnotes_without_promoting_hanging_prose() {
    fn note_count(blocks: &[Block]) -> usize {
        blocks
            .iter()
            .map(|block| match block {
                Block::FootnoteDefinition { blocks, .. } => 1 + note_count(blocks),
                Block::BlockQuote(blocks) => note_count(blocks),
                Block::List(list) => list.items.iter().map(|item| note_count(&item.blocks)).sum(),
                _ => 0,
            })
            .sum()
    }

    for profile in PROFILES {
        for (opening, continuation) in [("", ""), ("> ", "> "), ("- ", "  "), ("> - ", ">   ")] {
            for indent in ["", "   "] {
                for blank in [false, true] {
                    let gap = if blank {
                        format!("{continuation}\n")
                    } else {
                        String::new()
                    };
                    let source = format!(
                        "{opening}[r]: /target\n{gap}{continuation}{indent}[^n]: [note](/note)\n\nUse [^n] and [r].\n"
                    );
                    let parsed = document(&source, profile);
                    assert_eq!(note_count(&parsed.blocks), 1, "{source:?}: {parsed:?}");
                    let destinations = destination_spans_with_profile(&source, profile);
                    assert_eq!(
                        destinations
                            .iter()
                            .map(|span| span.dest.as_str())
                            .collect::<Vec<_>>(),
                        ["/target", "/note"],
                        "{source:?}"
                    );
                    for destination in destinations {
                        assert_eq!(&source[destination.range], destination.dest);
                    }
                    assert!(fragment(&source, profile).contains("footnotes"));
                }
            }
        }

        // Column-zero note markers after a definition-only container prefix
        // retain their existing extension boundary without a repeated marker.
        // Both the baseline and cmark-gfm accept the nested reference here.
        for source in [
            "> [r]: /target\n[^n]: [fake]: /phantom\n\n[fake] [^n]\n",
            "- [r]: /target\n[^n]: [fake]: /phantom\n\n[fake] [^n]\n",
        ] {
            let parsed = document(source, profile);
            assert_eq!(note_count(&parsed.blocks), 1, "{source:?}: {parsed:?}");
            let destinations = destination_spans_with_profile(source, profile);
            assert_eq!(
                destinations
                    .iter()
                    .map(|span| span.dest.as_str())
                    .collect::<Vec<_>>(),
                ["/target", "/phantom"]
            );
            for destination in destinations {
                assert_eq!(&source[destination.range], destination.dest);
            }
        }

        // The repo deliberately disallows notes interrupting visible prose.
        // Hanging indentation and lazy container content keep that ownership,
        // even when the note body could otherwise define a nested reference.
        for source in [
            "[r]: /target\nvisible\n[^n]: [fake]: /phantom\n\n[fake] [^n]\n",
            "[r]: /target\n    [^n]: [fake]: /phantom\n\n[fake] [^n]\n",
            "[r]: /target\n\t[^n]: [fake]: /phantom\n\n[fake] [^n]\n",
            "[r]: /target\n \t[^n]: [fake]: /phantom\n\n[fake] [^n]\n",
            "[r]: /target\n\n    [^n]: [fake]: /phantom\n\n[fake] [^n]\n",
            "> [r]: /target\n    [^n]: [fake]: /phantom\n\n[fake] [^n]\n",
        ] {
            let parsed = document(source, profile);
            assert_eq!(note_count(&parsed.blocks), 0, "{source:?}: {parsed:?}");
            let destinations = destination_spans_with_profile(source, profile);
            assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
            assert_eq!(destinations[0].dest, "/target");
            assert_eq!(reference_label_spans_with_profile(source, profile).len(), 1);
        }

        // An unmarked blank ends the outer quote. The following quote starts
        // fresh, so its five-column line is code, not a note in the old list.
        let source = "> - [r]: /target\n\n>      [^n]: [note](/note)\n\nUse [^n] and [r].\n";
        let parsed = document(source, profile);
        assert_eq!(note_count(&parsed.blocks), 0);
        let destinations = destination_spans_with_profile(source, profile);
        assert_eq!(destinations.len(), 1);
        assert_eq!(destinations[0].dest, "/target");
    }
}

#[test]
fn continued_reference_paragraphs_keep_original_unicode_line_endings_and_spans() {
    for profile in PROFILES {
        for newline in ["\n", "\r\n", "\r"] {
            let body = "\t**café** [visible](/réel)\n[fake]: /phantom".replace('\n', newline);
            let source =
                format!("\u{feff}[r]: /target{newline}{body}{newline}{newline}[fake]{newline}");
            let parsed = document(&source, profile);
            assert!(matches!(&parsed.blocks[0], Block::Paragraph(_)));
            let spanned = parse_markdown_spanned_with_profile(&source, profile);
            assert_eq!(spanned.blocks[0].span.slice(&source), Some(body.as_str()));
            assert_eq!(spanned.blocks[1].span.slice(&source), Some("[fake]"));
            assert!(spanned.diagnostics.is_empty(), "{:?}", spanned.diagnostics);
            let destinations = destination_spans_with_profile(&source, profile);
            assert_eq!(
                destinations
                    .iter()
                    .map(|span| span.dest.as_str())
                    .collect::<Vec<_>>(),
                ["/target", "/réel"]
            );
            for destination in destinations {
                assert_eq!(&source[destination.range], destination.dest);
            }
        }
    }
}
