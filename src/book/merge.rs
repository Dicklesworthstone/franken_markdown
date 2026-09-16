//! Merge independently parsed chapters without merging their footnote scopes.

use franken_markdown::book::Book;
use franken_markdown::{Block, Document, Inline};

pub(super) fn assemble(book: &Book) -> Document {
    let mut blocks = Vec::new();
    let isolate = book.chapters.len() > 1;
    for (index, chapter) in book.chapters.iter().enumerate() {
        if index > 0 {
            blocks.push(Block::PageBreak);
        }
        let start = blocks.len();
        blocks.extend(chapter.doc.blocks.iter().cloned());
        if isolate {
            namespace_blocks(&mut blocks[start..], index + 1);
        }
    }
    Document { blocks }
}

// The decimal chapter index plus its delimiter gives each chapter a disjoint
// namespace, even when a user-authored ID already looks like a generated ID.
// Definitions and references must both be transformed, including those inside
// tables, definition lists, and nested footnote bodies. Plain text and link
// destinations are deliberately not searched/replaced.
fn namespace_blocks(blocks: &mut [Block], chapter: usize) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                namespace_inlines(inlines, chapter);
            }
            Block::BlockQuote(inner) => namespace_blocks(inner, chapter),
            Block::FootnoteDefinition { id, blocks } => {
                *id = scoped_id(chapter, id);
                namespace_blocks(blocks, chapter);
            }
            Block::List(list) => {
                for item in &mut list.items {
                    namespace_blocks(&mut item.blocks, chapter);
                }
            }
            Block::Table(table) => {
                for cell in &mut table.head {
                    namespace_inlines(cell, chapter);
                }
                for row in &mut table.rows {
                    for cell in row {
                        namespace_inlines(cell, chapter);
                    }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for inlines in item.terms.iter_mut().chain(&mut item.definitions) {
                        namespace_inlines(inlines, chapter);
                    }
                }
            }
            Block::CodeBlock { .. }
            | Block::ThematicBreak
            | Block::HtmlBlock(_)
            | Block::MathBlock(_)
            | Block::PageBreak => {}
        }
    }
}

fn namespace_inlines(inlines: &mut [Inline], chapter: usize) {
    for inline in inlines {
        match inline {
            Inline::FootnoteRef { id } => *id = scoped_id(chapter, id),
            Inline::Emphasis(content)
            | Inline::Strong(content)
            | Inline::Strikethrough(content)
            | Inline::Link { content, .. } => namespace_inlines(content, chapter),
            _ => {}
        }
    }
}

fn scoped_id(chapter: usize, id: &str) -> String {
    format!("fmd-book-{chapter}-{id}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use franken_markdown::book::BookChapter;
    use franken_markdown::{Align, DefinitionItem, HtmlOptions, List, ListItem, Table};

    fn note_ref(id: &str) -> Inline {
        Inline::FootnoteRef { id: id.into() }
    }

    fn chapter(path: &str, note: &str) -> BookChapter {
        BookChapter {
            path: path.into(),
            out_name: format!("{path}.html"),
            title: path.into(),
            frontmatter: None,
            doc: Document {
                blocks: vec![
                    Block::Paragraph(vec![Inline::Text("Source".into()), note_ref("1")]),
                    Block::FootnoteDefinition {
                        id: "1".into(),
                        blocks: vec![Block::Paragraph(vec![Inline::Text(note.into())])],
                    },
                ],
            },
        }
    }

    #[test]
    fn repeated_ids_keep_both_chapters_notes_and_do_not_mutate_inputs() {
        let book = Book {
            chapters: vec![chapter("first", "First citation"), chapter("second", "Second citation")],
        };
        let original = book.chapters[0].doc.clone();
        let merged = assemble(&book);
        assert_eq!(book.chapters[0].doc, original);
        assert_eq!(merged.blocks.len(), 5);
        assert_eq!(merged.blocks[2], Block::PageBreak);
        for (index, start) in [(1, 0), (2, 3)] {
            let id = scoped_id(index, "1");
            assert_eq!(
                merged.blocks[start],
                Block::Paragraph(vec![Inline::Text("Source".into()), note_ref(&id)])
            );
            match &merged.blocks[start + 1] {
                Block::FootnoteDefinition { id: actual, .. } => assert_eq!(actual, &id),
                other => panic!("expected definition, got {other:?}"),
            }
        }
        let html = franken_markdown::render_html_document(
            &merged,
            &HtmlOptions { custom_css: Some(String::new()), ..HtmlOptions::default() },
        ).expect("render merged footnotes");
        assert!(html.contains("First citation"));
        assert!(html.contains("Second citation"));
        assert_eq!(merged, assemble(&book), "assembly is deterministic");
    }

    #[test]
    fn empty_and_single_chapter_books_preserve_the_existing_document() {
        assert_eq!(assemble(&Book { chapters: vec![] }), Document::default());
        let book = Book { chapters: vec![chapter("only", "Only citation")] };
        assert_eq!(assemble(&book), book.chapters[0].doc);
    }

    #[test]
    fn namespaces_every_inline_container_but_not_visible_text_or_urls() {
        let mut blocks = vec![
            Block::Heading { level: 1, inlines: vec![note_ref("x")] },
            Block::BlockQuote(vec![Block::List(List {
                ordered: false,
                start: 1,
                tight: true,
                items: vec![ListItem {
                    task: None,
                    blocks: vec![Block::Paragraph(vec![Inline::Strong(vec![
                        Inline::Emphasis(vec![Inline::Strikethrough(vec![note_ref("x")])]),
                    ])])],
                }],
            })]),
            Block::Table(Table {
                align: vec![Align::Left],
                head: vec![vec![note_ref("x")]],
                rows: vec![vec![vec![note_ref("x")]]],
            }),
            Block::DefinitionList(vec![DefinitionItem {
                terms: vec![vec![note_ref("x")]],
                definitions: vec![vec![note_ref("x")]],
            }]),
            Block::FootnoteDefinition {
                id: "x".into(),
                blocks: vec![Block::Paragraph(vec![Inline::Link {
                    dest: "#x".into(),
                    title: Some("x".into()),
                    content: vec![Inline::Text("x".into()), note_ref("x")],
                }])],
            },
            Block::CodeBlock { lang: None, code: "[^x]".into() },
        ];
        let original = blocks.clone();
        namespace_blocks(&mut blocks, 7);
        let expected_id = scoped_id(7, "x");
        assert_eq!(blocks[0], Block::Heading { level: 1, inlines: vec![note_ref(&expected_id)] });
        if let Block::Table(table) = &blocks[2] {
            assert_eq!(table.head[0], vec![note_ref(&expected_id)]);
            assert_eq!(table.rows[0][0], vec![note_ref(&expected_id)]);
        } else {
            panic!("table changed kind");
        }
        if let Block::DefinitionList(items) = &blocks[3] {
            assert_eq!(items[0].terms[0], vec![note_ref(&expected_id)]);
            assert_eq!(items[0].definitions[0], vec![note_ref(&expected_id)]);
        } else {
            panic!("definition list changed kind");
        }
        assert_eq!(blocks[4], Block::FootnoteDefinition {
            id: expected_id.clone(),
            blocks: vec![Block::Paragraph(vec![Inline::Link {
                dest: "#x".into(),
                title: Some("x".into()),
                content: vec![Inline::Text("x".into()), note_ref(&expected_id)],
            }])],
        });
        assert_eq!(blocks[5], original[5], "code is not rewritten");
        let mut expected_nested = original[1].clone();
        if let Block::BlockQuote(inner) = &mut expected_nested {
            if let Block::List(list) = &mut inner[0] {
                list.items[0].blocks = vec![Block::Paragraph(vec![Inline::Strong(vec![
                    Inline::Emphasis(vec![Inline::Strikethrough(vec![note_ref(&expected_id)])]),
                ])])];
            }
        }
        assert_eq!(blocks[1], expected_nested);
    }

    #[test]
    fn user_ids_that_resemble_generated_ids_cannot_cross_chapter_scopes() {
        assert_ne!(scoped_id(1, "fmd-book-2-x"), scoped_id(2, "x"));
        assert_ne!(scoped_id(1, "2-x"), scoped_id(12, "x"));
        assert_ne!(scoped_id(1, ""), scoped_id(2, ""));
    }
}
