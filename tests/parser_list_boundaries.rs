//! Container continuation is shared by every renderer and source-aware editor.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_spanned, render_html,
};

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.to_owned())])
}

fn first_item(source: &str) -> Vec<Block> {
    let document = parse_markdown(source);
    let Block::List(list) = &document.blocks[0] else {
        panic!("expected list: {source:?}: {document:?}");
    };
    assert_eq!(list.items.len(), 1, "{source:?}");
    list.items[0].blocks.clone()
}

#[test]
fn dedented_blocks_follow_the_list_instead_of_becoming_item_content() {
    for outside in [
        "# Outside",
        "> Outside",
        "```\nOutside\n```",
        "~~~\nOutside\n~~~",
        "<div>Outside</div>",
        "***",
    ] {
        let source = format!("- item\n{outside}\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{source:?}: {document:?}");
        assert_eq!(first_item(&source), vec![paragraph("item")]);
        assert_eq!(document.blocks[1..], parse_markdown(outside).blocks);
    }
}

#[test]
fn completed_nonparagraph_blocks_cannot_lazily_capture_outside_prose() {
    for body in [
        "# Heading",
        "Title\n  ===",
        "```\n  code\n  ```",
        "~~~\n  code\n  ~~~",
        "    indented code",
        "<!-- comment\n  body\n  -->",
        "<div>\n  body",
        "$$\n  x + y\n  $$",
        "| A | B |\n  | --- | --- |\n  | a | b |",
        "[^note]: Note body",
    ] {
        let source = format!("- {body}\noutside\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{source:?}: {document:?}");
        assert_eq!(document.blocks[1], paragraph("outside"), "{source:?}");
    }
}

#[test]
fn task_markers_do_not_hide_the_actual_first_block_from_continuation_rules() {
    for task in ["[ ]", "[x]", "[X]"] {
        for body in ["# Heading", "```\n  code\n  ```", "> # Heading"] {
            let source = format!("- {task} {body}\noutside\n");
            let document = parse_markdown(&source);
            assert_eq!(document.blocks.len(), 2, "{source:?}: {document:?}");
            assert_eq!(document.blocks[1], paragraph("outside"));
            let Block::List(list) = &document.blocks[0] else {
                panic!("list")
            };
            assert_eq!(list.items[0].task, Some(task != "[ ]"));
        }
    }
    assert_eq!(parse_markdown("- [ ] task\ncontinued\n").blocks.len(), 1);
}

#[test]
fn unclosed_list_fences_end_at_their_container_boundary() {
    for marker in ["- ", "1. ", "23) "] {
        let indent = " ".repeat(marker.len());
        for fence in ["```", "~~~"] {
            let source = format!("{marker}{fence}\n{indent}literal\noutside\n");
            let document = parse_markdown(&source);
            assert_eq!(document.blocks.len(), 2, "{source:?}: {document:?}");
            assert_eq!(document.blocks[1], paragraph("outside"));
            assert_eq!(
                first_item(&source),
                vec![Block::CodeBlock {
                    lang: None,
                    code: "literal\n".into()
                }],
                "{source:?}"
            );
        }
    }
}

#[test]
fn a_dedented_closing_fence_begins_a_new_top_level_fence() {
    let source = "- ```\n  literal\n```\noutside\n";
    let document = parse_markdown(source);
    assert_eq!(document.blocks.len(), 2, "{document:?}");
    assert_eq!(
        document.blocks[1],
        Block::CodeBlock {
            lang: None,
            code: "outside\n".into()
        }
    );
    assert_eq!(
        first_item(source),
        vec![Block::CodeBlock {
            lang: None,
            code: "literal\n".into()
        }]
    );
}

#[test]
fn nested_fences_and_quotes_do_not_turn_literal_content_into_lazy_prose() {
    for body in [
        "- ```\n    literal",
        "1. ~~~\n     literal",
        "> ```\n  > literal",
        "> <pre>\n  > literal",
        "- # Heading",
        "> # Heading",
        "- title\n    ===",
    ] {
        let source = format!("- {body}\noutside\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{source:?}: {document:?}");
        assert_eq!(document.blocks[1], paragraph("outside"));
    }
}

#[test]
fn valid_lazy_paragraphs_remain_inside_all_open_containers() {
    for source in [
        "- first\ncontinued\n",
        "- $$x$$ text\ncontinued\n",
        "- first\n      hanging\ncontinued\n",
        "- - first\ncontinued\n",
        "- 1. first\ncontinued\n",
        "- > first\ncontinued\n",
        "- - > first\ncontinued\n",
        "- ```\n  code\n  ```\n  first\ncontinued\n",
        "- first\n=\ncontinued\n",
        "- first\n  - child\n    hanging\ncontinued\n",
    ] {
        let document = parse_markdown(source);
        assert_eq!(document.blocks.len(), 1, "{source:?}: {document:?}");
        let html = render_html(source, &HtmlOptions::default()).unwrap();
        let continued = html
            .find("continued</li>")
            .or_else(|| html.find("continued</p>"));
        assert!(continued.is_some(), "{source:?}: {document:?}");
        assert!(
            continued.unwrap() < html.rfind("</ul>").unwrap(),
            "{source:?}"
        );
    }
}

#[test]
fn properly_indented_block_starters_still_belong_to_the_item() {
    for body in [
        "# Heading\n  text",
        "first\n  > quote",
        "first\n  ```\n  literal\n  ```",
        "first\n  <div>\n  body\n  </div>",
        "first\n  - child\n    # Heading\n    text",
    ] {
        let source = format!("- {body}\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 1, "{source:?}: {document:?}");
    }
}

#[test]
fn fenced_blank_lines_do_not_loosen_the_containing_list_or_lose_spaces() {
    let source = "- ```\n  literal\n    \n  next\n  ```\n- item\n";
    let document = parse_markdown(source);
    let Block::List(list) = &document.blocks[0] else {
        panic!("list")
    };
    assert!(list.tight);
    assert_eq!(list.items.len(), 2);
    assert_eq!(
        list.items[0].blocks,
        vec![Block::CodeBlock {
            lang: None,
            code: "literal\n  \nnext\n".into(),
        }]
    );
}

#[test]
fn source_spans_stop_at_the_same_boundary_as_the_semantic_list() {
    let source = "- ```\r\n  café\r\n# Outside\r\n\r\n[link](/target)\r\n";
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.blocks.len(), 3, "{spanned:?}");
    assert_eq!(
        spanned.blocks[0].span.slice(source),
        Some("- ```\r\n  café")
    );
    assert_eq!(spanned.blocks[1].span.slice(source), Some("# Outside"));
    assert_eq!(
        spanned.blocks[2].span.slice(source),
        Some("[link](/target)")
    );
    assert_eq!(spanned.to_document(), parse_markdown(source));
}

#[test]
fn destination_collection_uses_corrected_list_and_code_ownership() {
    let source = "- ```\n  [literal](/not-a-link)\n[real](/réal#heading)\n";
    let destinations = franken_markdown::parse::destination_spans(source);
    assert_eq!(destinations.len(), 1, "{destinations:?}");
    assert_eq!(destinations[0].dest, "/réal#heading");
    assert_eq!(&source[destinations[0].range.clone()], "/réal#heading");
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("[literal](/not-a-link)"));
    assert!(html.contains("</ul>\n<p><a href=\"/r%C3%A9al#heading\">real</a></p>"));
}

#[test]
fn list_fences_do_not_steal_real_reference_definitions_or_define_code_examples() {
    for code in ["```", "~~~"] {
        for close in ["", &format!("  {code}\n")] {
            let source =
                format!("- {code}\n  [fake]: /literal\n{close}[real]: /target\n\n[fake] [real]\n");
            let html = render_html(&source, &HtmlOptions::default()).unwrap();
            assert!(html.contains("[fake]: /literal"), "{source:?}");
            assert!(
                html.contains("[fake] <a href=\"/target\">real</a>"),
                "{source:?}"
            );
            assert!(!html.contains("href=\"/literal\""), "{source:?}");
            let destinations = franken_markdown::parse::destination_spans(&source);
            assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
            assert_eq!(destinations[0].dest, "/target");
        }
    }
}

#[test]
fn reference_titles_preserve_lazy_paragraph_continuation() {
    for definition in [
        "[ref]: /target\n  \"a title\"",
        "[ref]: /target\n  \"a multiline\n  title\"",
        "[ref]:\n  /target\n  'a title'",
    ] {
        let source = format!("- {definition}\noutside [ref]\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 1, "{source:?}: {document:?}");
        let blocks = first_item(&source);
        let Block::Paragraph(inlines) = &blocks[0] else {
            panic!("remaining paragraph")
        };
        assert!(
            matches!(&inlines[1], Inline::Link { dest, title: Some(_), .. } if dest == "/target")
        );
    }
}

#[test]
fn container_reference_precedence_follows_source_order() {
    let definitions = [
        ("> [ref]: /quote", "/quote"),
        ("- [ref]: /list", "/list"),
        ("[^n]: [ref]: /note", "/note"),
        ("[ref]: /top", "/top"),
    ];
    for (first, first_dest) in definitions {
        for (second, second_dest) in definitions {
            if first_dest == second_dest {
                continue;
            }
            let source = format!("{first}\n\n{second}\n\n[ref]\n");
            let document = parse_markdown(&source);
            let Block::Paragraph(inlines) = document.blocks.last().unwrap() else {
                panic!("reference use")
            };
            assert!(
                matches!(&inlines[..], [Inline::Link { dest, .. }] if dest == first_dest),
                "{source:?}: {document:?}"
            );
        }
    }
}

#[test]
fn nested_definitions_keep_first_definition_order_and_exact_destination_spans() {
    let source = "- first\n\n  [ref]: /nested\n\n[ref]: /later\n\n[ref]\n";
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("href=\"/nested\">ref</a>"));
    let destinations = franken_markdown::parse::destination_spans(source);
    assert_eq!(destinations.len(), 2, "{destinations:?}");
    for (span, expected) in destinations.iter().zip(["/nested", "/later"]) {
        assert_eq!(span.dest, expected);
        assert_eq!(&source[span.range.clone()], expected);
    }
}
