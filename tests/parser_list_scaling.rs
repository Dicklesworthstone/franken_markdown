//! Large list items retain their structure without revisiting their whole history.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{Block, Inline, parse_markdown, parse_markdown_spanned};

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.to_owned())])
}

#[test]
fn blank_rich_list_items_preserve_every_paragraph_and_source_boundary() {
    const PARAGRAPHS: usize = 2_048;
    let mut source = String::from("- first\n");
    for index in 0..PARAGRAPHS {
        source.push_str(&format!("\n  paragraph {index} café\n"));
    }
    source.push_str("- last\n\n# Outside\n");
    let document = parse_markdown(&source);
    let Block::List(list) = &document.blocks[0] else {
        panic!("expected list");
    };
    assert!(!list.tight);
    assert_eq!(list.items.len(), 2);
    assert_eq!(list.items[0].blocks.len(), PARAGRAPHS + 1);
    assert_eq!(list.items[0].blocks[0], paragraph("first"));
    for index in 0..PARAGRAPHS {
        assert_eq!(
            list.items[0].blocks[index + 1],
            paragraph(&format!("paragraph {index} café"))
        );
    }
    assert_eq!(list.items[1].blocks, vec![paragraph("last")]);
    let spanned = parse_markdown_spanned(&source);
    assert_eq!(spanned.to_document(), document);
    assert_eq!(spanned.blocks.len(), 2);
    assert_eq!(spanned.blocks[1].span.slice(&source), Some("# Outside"));
    assert!(spanned.blocks[0].span.end <= source.find("# Outside").unwrap());
}

#[test]
fn nested_marker_history_does_not_leak_into_the_next_item() {
    let source = "- first\n  - nested\n- second\n\n      literal code\n";
    let document = parse_markdown(source);
    let Block::List(list) = &document.blocks[0] else {
        panic!("expected list");
    };
    assert!(
        !list.tight,
        "the second item's separate code block loosens the list"
    );
    assert_eq!(list.items.len(), 2);
    assert!(matches!(list.items[0].blocks[1], Block::List(_)));
    assert_eq!(
        list.items[1].blocks,
        vec![
            paragraph("second"),
            Block::CodeBlock {
                lang: None,
                code: "literal code\n".into(),
            }
        ]
    );
}

#[test]
fn literal_list_markers_and_blank_lines_stay_inside_fences() {
    let source = "- ```text\n  - literal marker\n\n      indented literal\n  ```\n- final\n";
    let document = parse_markdown(source);
    let Block::List(list) = &document.blocks[0] else {
        panic!("expected list");
    };
    assert!(list.tight);
    assert_eq!(list.items.len(), 2);
    assert_eq!(
        list.items[0].blocks,
        vec![Block::CodeBlock {
            lang: Some("text".into()),
            code: "- literal marker\n\n    indented literal\n".into(),
        }]
    );
    assert_eq!(list.items[1].blocks, vec![paragraph("final")]);
}
