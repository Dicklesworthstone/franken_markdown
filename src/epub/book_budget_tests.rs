//! Exercise the real export pipeline at small, exact admission boundaries.
use super::*;
use franken_markdown::{BookInput, FontAssetSlot, FontAssets, FontFamily, build_book};
use franken_markdown::fonts::{self, FontStyle};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn book() -> Result<Book> {
    build_book(&[
        BookInput { path: "one.md".into(), source: "# One\n\nFirst chapter.".into() },
        BookInput { path: "two.md".into(), source: "# Two\n\nSecond chapter.".into() },
    ])
}

fn emitted_size(book: &PreparedBook) -> usize {
    book.opf.len() + book.nav.len() + book.css.len() + CONTAINER_XML.len()
        + MIMETYPE.len() + book.fonts.byte_len()
        + book.chapters.iter().map(|chapter| {
            chapter.xhtml.len() + chapter.content.resources.iter()
                .map(|resource| resource.bytes.len()).sum::<usize>()
        }).sum::<usize>()
}

#[test]
fn exact_output_budget_applies_to_font_free_publications() -> TestResult {
    let book = book()?;
    let opts = HtmlOptions::default();
    let prepared = prepare_book(&book, &opts)?;
    assert_eq!(prepared.fonts.byte_len(), 0);
    let exact = emitted_size(&prepared);
    assert_eq!(check_output(&prepared, exact)?, exact);
    assert!(check_output(&prepared, exact - 1).is_err());
    let repeated = prepare_book_with_budget(&book, &opts, exact)?;
    assert_eq!(repeated.opf, prepared.opf);
    assert_eq!(repeated.nav, prepared.nav);
    assert!(prepare_book_with_budget(&book, &opts, exact - 1).is_err());
    Ok(())
}

#[test]
fn fonts_and_their_inserted_stylesheet_links_count_in_the_same_budget() -> TestResult {
    let book = book()?;
    let opts = HtmlOptions {
        font_assets: FontAssets::default().with_slot(
            FontAssetSlot::BodyRegular,
            fonts::body_bytes(FontFamily::Sans, FontStyle::Regular).to_vec(),
        )?,
        ..HtmlOptions::default()
    };
    let prepared = prepare_book(&book, &opts)?;
    assert!(prepared.fonts.byte_len() > 0);
    let exact = emitted_size(&prepared);
    assert!(prepare_book_with_budget(&book, &opts, exact).is_ok());
    assert!(prepare_book_with_budget(&book, &opts, exact - 1).is_err());
    assert!(check_output(&prepared, exact - prepared.fonts.byte_len()).is_err());
    Ok(())
}

#[test]
fn metadata_and_navigation_cannot_bypass_font_free_admission() -> TestResult {
    let mut book = book()?;
    book.chapters[0].title = "<&> ".repeat(128);
    let opts = HtmlOptions { title: Some("<&> ".repeat(128)), ..HtmlOptions::default() };
    let prepared = prepare_book(&book, &opts)?;
    let exact = emitted_size(&prepared);
    let without_metadata = exact - prepared.opf.len() - prepared.nav.len();
    assert!(check_output(&prepared, without_metadata).is_err());
    assert!(prepare_book_with_budget(&book, &opts, without_metadata).is_err());
    assert!(prepare_book_with_budget(&book, &opts, exact).is_ok());
    Ok(())
}

#[test]
fn repeated_base64_images_are_not_charged_as_retained_publication_bytes() -> TestResult {
    let mut source = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\">");
    for _ in 0..256 {
        source.push_str("<rect width=\"1\" height=\"1\"/>");
    }
    source.push_str("</svg>");
    let book = build_book(&(0..8).map(|index| BookInput {
        path: format!("part-{index}.md"),
        source: "# Illustration\n\n![shared](figure.svg)".into(),
    }).collect::<Vec<_>>())?;
    let opts = HtmlOptions {
        image_assets: vec![PdfImageAsset { destination: "figure.svg".into(), bytes: source.into_bytes() }],
        ..HtmlOptions::default()
    };
    let prepared = prepare_book(&book, &opts)?;
    let exact = emitted_size(&prepared);
    assert_eq!(prepared.chapters.iter().map(|chapter| chapter.content.resources.len()).sum::<usize>(), 1);
    let payload = prepared.chapters[0].content.resources[0].bytes.len();
    assert!(payload > 0);
    assert!(check_output(&prepared, exact - payload).is_err());
    assert!(prepare_book_with_budget(&book, &opts, exact).is_ok());
    assert!(prepare_book_with_budget(&book, &opts, exact - 1).is_err());
    // This is the representation that the old cumulative gate charged.
    let mut html_opts = opts.clone();
    html_opts.custom_css = Some(String::new());
    let transient: usize = book.chapters.iter().map(|chapter| {
        html_opts.title = Some(chapter.title.clone());
        franken_markdown::html::render(&chapter.doc, &html_opts).len()
    }).sum();
    assert!(transient > exact);
    Ok(())
}

#[test]
fn addition_is_exact_overflow_checked_and_nonmutating_on_refusal() -> TestResult {
    let mut total = 5;
    add_bytes(&mut total, 7, 12)?;
    assert_eq!(total, 12);
    assert!(add_bytes(&mut total, 1, 12).is_err());
    assert_eq!(total, 12);
    assert!(add_bytes(&mut total, usize::MAX, usize::MAX).is_err());
    assert_eq!(total, 12);
    let mut empty = 0;
    add_bytes(&mut empty, 0, 0)?;
    assert!(add_bytes(&mut empty, 1, 0).is_err());
    Ok(())
}
