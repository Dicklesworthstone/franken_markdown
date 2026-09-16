//! Publication integrity across the existing book CLI and pure render APIs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use franken_markdown::book::rewrite_links_for_site_from;
use franken_markdown::{
    Block, Book, BookInput, Document, HtmlOptions, Inline, PdfOptions,
    book_pdf_document, build_book, out_name, parse_markdown,
    render_html_document, render_pdf_document, rewrite_links_for_site,
};

fn nested_book() -> Book {
    build_book(&[
        BookInput {
            path: "index.md".into(),
            source: "# Home\n\n[Guide](guide/start.md)\n".into(),
        },
        BookInput {
            path: "guide/start.md".into(),
            source: "# Start\n\n[Home](../index.md#home) [Next](next.md?v=1#next) [Local](#start) [Missing](absent.md) [Remote](https://example.com/next.md)\n".into(),
        },
        BookInput {
            path: "guide/next.md".into(),
            source: "# Next\n\n[Back](./start.md)\n".into(),
        },
    ]).expect("nested book")
}

fn destinations(doc: &Document) -> Vec<String> {
    fn inlines(items: &[Inline], out: &mut Vec<String>) {
        for item in items {
            match item {
                Inline::Link { dest, content, .. } => {
                    out.push(dest.clone());
                    inlines(content, out);
                }
                Inline::Emphasis(items) | Inline::Strong(items) | Inline::Strikethrough(items) => inlines(items, out),
                _ => {}
            }
        }
    }
    fn blocks(items: &[Block], out: &mut Vec<String>) {
        for item in items {
            match item {
                Block::Paragraph(items) | Block::Heading { inlines: items, .. } => inlines(items, out),
                Block::BlockQuote(items) | Block::FootnoteDefinition { blocks: items, .. } => blocks(items, out),
                Block::List(list) => {
                    for item in &list.items { blocks(&item.blocks, out); }
                }
                Block::Table(table) => {
                    for cell in &table.head { inlines(cell, out); }
                    for row in &table.rows {
                        for cell in row { inlines(cell, out); }
                    }
                }
                Block::DefinitionList(items) => {
                    for item in items {
                        for values in item.terms.iter().chain(&item.definitions) { inlines(values, out); }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    blocks(&doc.blocks, &mut out);
    out
}

#[test]
fn existing_source_less_site_api_resolves_nested_books_correctly() {
    let book = nested_book();
    let known: BTreeSet<_> = book.chapters.iter().map(|chapter| chapter.out_name.clone()).collect();
    let original = book.chapters[1].doc.clone();
    let mut doc = original.clone();
    assert_eq!(rewrite_links_for_site(&mut doc, &known), 2);
    let links = destinations(&doc);
    assert!(links.contains(&format!("{}#home", out_name("index.md"))));
    assert!(links.contains(&"guide__next.html?v=1#next".to_string()));
    assert!(links.contains(&"#start".to_string()));
    assert!(links.contains(&"absent.md".to_string()));
    assert!(links.contains(&"https://example.com/next.md".to_string()));
    assert_eq!(rewrite_links_for_site(&mut doc, &known), 0);
    assert_eq!(book.chapters[1].doc, original);
    let html = render_html_document(&doc, &HtmlOptions {
        custom_css: Some(String::new()), ..HtmlOptions::default()
    }).expect("HTML");
    assert!(html.contains("href=\"guide__next.html?v=1#next\""));
}

#[test]
fn independently_parsed_chapters_can_supply_explicit_source_context() {
    let book = nested_book();
    let mut doc = parse_markdown("[Sibling](next.md) [Parent](../index.md) [Root](/index.md#home)");
    assert_eq!(rewrite_links_for_site_from(&mut doc, "guide/start.md", &book).unwrap(), 3);
    assert_eq!(destinations(&doc), vec![
        "guide__next.html".to_string(),
        out_name("index.md"),
        format!("{}#home", out_name("index.md")),
    ]);
    let before = doc.clone();
    assert!(rewrite_links_for_site_from(&mut doc, "../escape.md", &book).is_err());
    assert_eq!(doc, before, "invalid context must not partially rewrite");
}

#[test]
fn encoded_unicode_and_reserved_filename_characters_resolve_to_real_outputs() {
    let book = build_book(&[
        BookInput { path: "guide/start.md".into(), source: "[Unicode](../%E4%B8%AD.md?q=1#part) [Punctuation](../a%23b%3Fc.md)\n".into() },
        BookInput { path: "中.md".into(), source: "# Unicode\n".into() },
        BookInput { path: "a#b?c.md".into(), source: "# Punctuation\n".into() },
    ]).unwrap();
    let known = book.chapters.iter().map(|chapter| chapter.out_name.clone()).collect();
    let mut doc = book.chapters[0].doc.clone();
    assert_eq!(rewrite_links_for_site(&mut doc, &known), 2);
    assert_eq!(destinations(&doc), vec![
        format!("{}?q=1#part", book.chapters[1].out_name),
        book.chapters[2].out_name.clone(),
    ]);
    assert_eq!(book.chapters[1].out_name, "~e4~b8~ad.html");
    assert_eq!(book.chapters[2].out_name, "a~23b~3fc.html");
}

#[test]
fn normalized_paths_and_collisions_are_checked_before_publication() {
    let book = build_book(&[
        BookInput { path: "./guide/../start.md".into(), source: "# Start".into() },
        BookInput { path: "guide\\next.md".into(), source: "# Next".into() },
    ]).unwrap();
    assert_eq!(book.chapters[0].path, "start.md");
    assert_eq!(book.chapters[1].path, "guide/next.md");
    for pair in [["a/b.md", "a__b.md"], ["Chapter.md", "chapter.md"], ["a.md", "a.markdown"], ["a.md", "./a.md"]] {
        let inputs = pair.map(|path| BookInput { path: path.into(), source: "# Chapter".into() });
        assert!(build_book(&inputs).is_err(), "silently accepted collision {pair:?}");
    }
}

#[test]
fn same_footnote_label_in_two_sources_survives_pdf_book_assembly() {
    let book = build_book(&[
        BookInput { path: "one.md".into(), source: "# First\n\nFirst source[^1].\n\n[^1]: First citation.\n".into() },
        BookInput { path: "two.md".into(), source: "# Second\n\nSecond source[^1].\n\n[^1]: Second citation.\n".into() },
    ]).unwrap();
    let merged = book_pdf_document(&book);
    let html = render_html_document(&merged, &HtmlOptions {
        custom_css: Some(String::new()), ..HtmlOptions::default()
    }).unwrap();
    assert!(html.contains("First citation."));
    assert!(html.contains("Second citation."));
    let opts = PdfOptions::default();
    let pdf = render_pdf_document(&merged, &opts).expect("PDF book");
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(pdf, render_pdf_document(&merged, &opts).unwrap());
}

#[test]
fn canonical_book_links_remain_compatible_with_epub_export() {
    let book = nested_book();
    let options = HtmlOptions::default();
    let first = franken_markdown::epub::render_book_epub(&book, &options).unwrap();
    let second = franken_markdown::epub::render_book_epub(&book, &options).unwrap();
    assert!(first.starts_with(b"PK\x03\x04"));
    assert_eq!(first, second);
}

#[cfg(feature = "cli")]
mod cli {
    use super::*;
    use std::{fs, path::PathBuf, process::Command};

    fn temporary_directory() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("fmd-book-integrity-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cli_keeps_index_chapter_content_and_rewrites_sibling_and_parent_links() {
        let temp = temporary_directory();
        let input = temp.join("book");
        let output = temp.join("site");
        fs::create_dir_all(input.join("guide")).unwrap();
        fs::write(input.join("index.md"), "# Home\n\nHOME_CONTENT_MUST_SURVIVE\n").unwrap();
        fs::write(input.join("guide/start.md"), "# Start\n\n[Home](../index.md#home) [Next](next.md?v=1#next)\n").unwrap();
        fs::write(input.join("guide/next.md"), "# Next\n").unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "book"])
            .arg(&input)
            .args(["--to", "html", "--out-dir"])
            .arg(&output)
            .arg("--json")
            .output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let home = fs::read_to_string(output.join(out_name("index.md"))).unwrap();
        assert!(home.contains("HOME_CONTENT_MUST_SURVIVE"));
        let landing = fs::read_to_string(output.join("index.html")).unwrap();
        assert!(landing.contains("http-equiv=\"refresh\""));
        assert!(!landing.contains("url=index.html"));
        let start = fs::read_to_string(output.join("guide__start.html")).unwrap();
        assert!(start.contains(&format!("href=\"{}#home\"", out_name("index.md"))));
        assert!(start.contains("href=\"guide__next.html?v=1#next\""));
        assert!(String::from_utf8_lossy(&result.stdout).contains("\"unresolved_links\":0"));
    }

    #[test]
    fn cli_rejects_colliding_chapter_names_without_writing_partial_site() {
        let temp = temporary_directory();
        let input = temp.join("book");
        let output = temp.join("site");
        fs::create_dir_all(input.join("a")).unwrap();
        fs::write(input.join("a/b.md"), "# Nested\n").unwrap();
        fs::write(input.join("a__b.md"), "# Flat\n").unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "book"])
            .arg(&input)
            .args(["--to", "html", "--out-dir"])
            .arg(&output)
            .arg("--json")
            .output().unwrap();
        assert_eq!(result.status.code(), Some(66));
        assert!(!output.exists(), "collision must fail before creating output files");
    }
}
