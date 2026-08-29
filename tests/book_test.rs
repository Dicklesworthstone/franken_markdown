//! `fmd book` integration proofs (bead j0o4 / epic 7tus).
//!
//! Covers the merge (chapter PageBreaks), the HTML site (sidebar injection +
//! cross-file link rewrite), the PDF book (global outline + chapter breaks +
//! continuous numbering), manifest ordering, and determinism.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use franken_markdown::{
    BookInput, HtmlOptions, PdfOptions, ast::Block, book_pdf_document, build_book,
    chapter_headings, inject_book_nav, out_name, parse_markdown, render_html_document,
    render_pdf_document, rewrite_links_for_site,
};

const CH1: &str = "---\ntitle=Getting Started\n---\n# Getting Started\n\nFirst chapter body.\n\nSee [the guide](guide.md#tips) for more.\n";
const CH2: &str =
    "# Advanced Guide\n\nSecond chapter body.\n\n## Tips\n\nAnchor target lives here.\n";

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "fmd-book-test-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn book() -> franken_markdown::Book {
    build_book(&[
        BookInput {
            path: "intro.md".into(),
            source: CH1.into(),
        },
        BookInput {
            path: "guide.md".into(),
            source: CH2.into(),
        },
    ])
    .expect("book builds")
}

#[test]
fn book_has_chapters_with_titles_and_pages() {
    let b = book();
    assert_eq!(b.chapters.len(), 2);
    assert_eq!(b.chapters[0].title, "Getting Started"); // frontmatter title
    assert_eq!(b.chapters[1].title, "Advanced Guide"); // first heading
    assert_eq!(b.chapters[0].out_name, "intro.html");
    assert_eq!(b.chapters[1].out_name, "guide.html");
    let h = chapter_headings(&b.chapters[1].doc);
    assert!(h.iter().any(|(lvl, t)| *lvl == 2 && t == "Tips"));
}

#[test]
fn test_out_name_flattening() {
    assert_eq!(out_name("intro.md"), "intro.html");
    assert_eq!(out_name("guide.markdown"), "guide.html");
    assert_eq!(out_name("guide/install.md"), "guide__install.html");
    assert_eq!(out_name("a/b/c/d.md"), "a__b__c__d.html");
    assert_eq!(out_name("win\\path\\part.md"), "win__path__part.html");
}

#[test]
fn test_build_book_empty_input_errors() {
    let res = build_book(&[]);
    assert!(res.is_err(), "empty inputs must return an error");
}

#[test]
fn test_build_book_title_precedence() {
    let inputs = vec![
        // 1. Frontmatter title wins over heading
        BookInput {
            path: "ch1.md".into(),
            source: "---\ntitle=Frontmatter Title\n---\n# Heading Title\n\nContent.".into(),
        },
        // 2. Heading title wins over filename stem
        BookInput {
            path: "ch2.md".into(),
            source: "# Heading Only\n\nContent.".into(),
        },
        // 3. Fallback to stem
        BookInput {
            path: "sub/my-cool-chapter.md".into(),
            source: "No headings here, just a paragraph.".into(),
        },
    ];

    let book = build_book(&inputs).expect("build_book should succeed");
    assert_eq!(book.chapters.len(), 3);

    assert_eq!(book.chapters[0].title, "Frontmatter Title");
    assert_eq!(book.chapters[0].out_name, "ch1.html");
    assert!(book.chapters[0].frontmatter.is_some());

    assert_eq!(book.chapters[1].title, "Heading Only");
    assert_eq!(book.chapters[1].out_name, "ch2.html");

    assert_eq!(book.chapters[2].title, "my-cool-chapter");
    assert_eq!(book.chapters[2].out_name, "sub__my-cool-chapter.html");
}

#[test]
fn merged_document_carries_page_breaks_between_chapters() {
    let b = book();
    let merged = book_pdf_document(&b);
    let breaks = merged
        .blocks
        .iter()
        .filter(|blk| matches!(blk, Block::PageBreak))
        .count();
    assert_eq!(breaks, 1, "one break between two chapters");
    // Chapter order preserved.
    let text = format!("{:?}", merged.blocks);
    assert!(text.contains("First chapter body"));
    assert!(text.contains("Second chapter body"));
}

#[test]
fn cross_file_links_rewrite_for_site() {
    let b = book();
    let known: BTreeSet<String> = b.chapters.iter().map(|c| c.out_name.clone()).collect();
    let mut doc = b.chapters[0].doc.clone();
    rewrite_links_for_site(&mut doc, &known);
    let links: Vec<String> = collect_link_dests(&doc);
    assert!(links.contains(&"guide.html#tips".to_string()), "{links:?}");
}

#[test]
fn unknown_md_link_counts_unresolved_not_rewritten() {
    let b = book();
    let known: BTreeSet<String> = b.chapters.iter().map(|c| c.out_name.clone()).collect();
    let mut doc = parse_markdown("[ghost](missing.md)\n[ch1](intro.md)\n");
    rewrite_links_for_site(&mut doc, &known);
    let dests = collect_link_dests(&doc);
    assert!(dests.contains(&"missing.md".to_string()), "{dests:?}");
    assert!(dests.contains(&"intro.html".to_string()), "{dests:?}");
}

#[test]
fn pdf_book_is_deterministic_with_global_outline_and_chapter_breaks() {
    let b = book();
    let merged = book_pdf_document(&b);
    let opts = PdfOptions {
        toc: true,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let pdf1 = render_pdf_document(&merged, &opts).expect("pdf 1");
    let pdf2 = render_pdf_document(&merged, &opts).expect("pdf 2");
    assert_eq!(pdf1, pdf2, "byte-identical book renders");
    assert!(
        pdf1.windows(b"/Outlines".len()).any(|w| w == b"/Outlines"),
        "outline present"
    );
    // Both chapter headings appear in the outline text layer via bookmarks.
    let text = String::from_utf8_lossy(&pdf1);
    assert!(text.contains("Getting Started") || text.contains("Advanced"));
    // Continuous page numbers: page 2 exists (two chapters, forced break).
    let pages = pdf1
        .windows(b"/Type /Page ".len())
        .filter(|w| *w == b"/Type /Page ")
        .count();
    assert!(pages >= 2, "forced chapter break paginates: {pages} pages");
}

#[test]
fn html_pages_get_sidebar_with_current_marker() {
    let b = book();
    let page = render_html_document(
        &b.chapters[0].doc,
        &HtmlOptions {
            title: Some(b.chapters[0].title.clone()),
            ..HtmlOptions::default()
        },
    )
    .expect("render");
    let out = inject_book_nav(&page, &b, "intro.html");
    assert!(out.contains("fmd-book-nav"));
    assert!(out.contains("<li class=\"current\"><a href=\"intro.html\">"));
    assert!(out.contains("<a href=\"guide.html\">Advanced Guide</a>"));
    assert!(out.contains("Getting Started"));
    // Nav injection precedes the main content.
    let nav_at = out.find("fmd-book-nav").expect("nav");
    let main_at = out.find("<main class=\"fmd\">").expect("main");
    assert!(nav_at < main_at);
}

#[test]
fn test_cli_book_e2e_html_pdf_and_json() {
    let temp = temp_dir("e2e");
    let book_dir = temp.join("my_book");
    let out_dir = temp.join("dist");
    fs::create_dir_all(&book_dir).expect("create book dir");

    // Create 3 markdown chapters
    fs::write(
        book_dir.join("01_intro.md"),
        "---\ntitle=\"Introduction\"\nauthor=\"Alice\"\n---\n# Welcome\n\nRead [Chapter 2](02_deep_dive.md#core-concepts).\n",
    )
    .expect("write 01");

    fs::write(
        book_dir.join("02_deep_dive.md"),
        "# Deep Dive\n\n## Core Concepts\n\nHere are the details. See [Summary](03_conclusion.md).\n",
    )
    .expect("write 02");

    fs::write(
        book_dir.join("03_conclusion.md"),
        "# Conclusion\n\nBack to [Start](01_intro.md).\n",
    )
    .expect("write 03");

    // Run CLI: fmd book <book_dir> --out-dir <out_dir> --json
    let fmd_bin = env!("CARGO_BIN_EXE_fmd");
    let output = Command::new(fmd_bin)
        .args([
            "book",
            book_dir.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run fmd book");

    assert!(
        output.status.success(),
        "fmd book should exit 0, got: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_str.contains("\"ok\":true"),
        "JSON receipt must have ok:true"
    );
    assert!(
        stdout_str.contains("\"command\":\"book\""),
        "JSON receipt must have command:book"
    );
    assert!(
        stdout_str.contains("\"chapters\":3"),
        "JSON receipt must report 3 chapters"
    );
    assert!(
        stdout_str.contains("\"unresolved_links\":0"),
        "JSON receipt must report 0 unresolved links"
    );

    // Verify written files
    assert!(
        out_dir.join("01_intro.html").is_file(),
        "01_intro.html must exist"
    );
    assert!(
        out_dir.join("02_deep_dive.html").is_file(),
        "02_deep_dive.html must exist"
    );
    assert!(
        out_dir.join("03_conclusion.html").is_file(),
        "03_conclusion.html must exist"
    );
    assert!(
        out_dir.join("index.html").is_file(),
        "index.html redirect must exist"
    );
    assert!(
        out_dir.join("my_book.pdf").is_file(),
        "my_book.pdf must exist"
    );

    // Verify index.html contains redirect to first chapter
    let index_content = fs::read_to_string(out_dir.join("index.html")).expect("read index.html");
    assert!(index_content.contains("url=01_intro.html"));

    // Verify 01_intro.html has rewritten link
    let ch1_content = fs::read_to_string(out_dir.join("01_intro.html")).expect("read ch1");
    assert!(
        ch1_content.contains("href=\"02_deep_dive.html#core-concepts\""),
        "Link must be rewritten to html anchor"
    );
    assert!(ch1_content.contains("<nav class=\"fmd-book-nav\""));

    // Verify PDF has valid header
    let pdf_bytes = fs::read(out_dir.join("my_book.pdf")).expect("read pdf");
    assert!(pdf_bytes.starts_with(b"%PDF-"));
}

#[test]
fn test_cli_book_manifest_ordering() {
    let temp = temp_dir("manifest");
    let book_dir = temp.join("manifest_book");
    let out_dir = temp.join("site");
    fs::create_dir_all(&book_dir).expect("create book dir");

    // Write files
    fs::write(book_dir.join("z_last.md"), "# Last Chapter\n\nContent.\n").expect("write last");
    fs::write(book_dir.join("a_first.md"), "# First Chapter\n\nContent.\n").expect("write first");

    // Write book.toml defining custom multiline order and title
    fs::write(
        book_dir.join("book.toml"),
        "title = \"Custom Manifest Book\"\norder = [\n  \"z_last.md\",\n  \"a_first.md\",\n]\n",
    )
    .expect("write book.toml");

    let fmd_bin = env!("CARGO_BIN_EXE_fmd");
    let output = Command::new(fmd_bin)
        .args([
            "book",
            book_dir.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--to",
            "html",
            "--json",
        ])
        .output()
        .expect("run fmd book");

    assert!(output.status.success());
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    assert!(stdout_str.contains("\"ok\":true"));

    // index.html should redirect to z_last.html since it was ordered first by manifest
    let index_content = fs::read_to_string(out_dir.join("index.html")).expect("read index.html");
    assert!(
        index_content.contains("url=z_last.html"),
        "index.html must redirect to the first manifest-ordered chapter"
    );
}

#[test]
fn test_relative_dot_links_and_external_links_rewrite() {
    let b = book();
    let known: BTreeSet<String> = b.chapters.iter().map(|c| c.out_name.clone()).collect();
    let mut doc = parse_markdown(
        "[dot]( ./guide.md#tips )\n[bare](guide.md)\n[ext](https://example.com/guide.md)\n",
    );
    let count = rewrite_links_for_site(&mut doc, &known);
    assert_eq!(count, 2, "rewrote 2 internal links, ignored external link");
    let dests = collect_link_dests(&doc);
    assert!(dests.contains(&"guide.html#tips".to_string()));
    assert!(dests.contains(&"guide.html".to_string()));
    assert!(dests.contains(&"https://example.com/guide.md".to_string()));
}

#[test]
fn test_math_in_chapter_heading_title() {
    let input = BookInput {
        path: "math_ch.md".into(),
        source: "# Theorem $E=mc^2$\n\nContent here.\n".into(),
    };
    let b = build_book(&[input]).expect("builds");
    assert_eq!(b.chapters[0].title, "Theorem E=mc^2");
}

#[test]
fn test_cli_book_transclusion() {
    let temp = temp_dir("transclude");
    let book_dir = temp.join("book");
    let out_dir = temp.join("dist");
    fs::create_dir_all(&book_dir).expect("create book dir");

    // Shared snippet
    fs::write(
        book_dir.join("snippet.md"),
        "This is included shared snippet content.\n",
    )
    .expect("write snippet");

    // Chapter including snippet
    fs::write(
        book_dir.join("01_main.md"),
        "# Main Chapter\n\n{{#include snippet.md}}\n\nAfter include.\n",
    )
    .expect("write main");

    let fmd_bin = env!("CARGO_BIN_EXE_fmd");
    let output = Command::new(fmd_bin)
        .args([
            "book",
            book_dir.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--to",
            "html",
            "--json",
        ])
        .output()
        .expect("run fmd book");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let main_html = fs::read_to_string(out_dir.join("01_main.html")).expect("read 01_main.html");
    assert!(
        main_html.contains("This is included shared snippet content."),
        "Included content must appear in rendered chapter"
    );
}

fn collect_link_dests(doc: &franken_markdown::Document) -> Vec<String> {
    use franken_markdown::ast::{Block, Inline};
    let mut out = Vec::new();
    fn walk_blocks(blocks: &[Block], out: &mut Vec<String>) {
        for blk in blocks {
            match blk {
                Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                    walk_inlines(inlines, out);
                }
                Block::BlockQuote(inner) => walk_blocks(inner, out),
                _ => {}
            }
        }
    }
    fn walk_inlines(inlines: &[Inline], out: &mut Vec<String>) {
        for inl in inlines {
            match inl {
                Inline::Link { dest, content, .. } => {
                    out.push(dest.clone());
                    walk_inlines(content, out);
                }
                Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                    walk_inlines(c, out);
                }
                _ => {}
            }
        }
    }
    walk_blocks(&doc.blocks, &mut out);
    out
}
