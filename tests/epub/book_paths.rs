//! EPUB publication must retain the book resolver's URL ownership decisions.

use super::{assert_well_formed_xml, entry_text, parse_central_directory};
use franken_markdown::{BookInput, HtmlOptions, PdfImageAsset, build_book, epub};

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

#[test]
fn encoded_network_links_cannot_be_recaptured_by_matching_book_chapters() {
    let book = build_book(&[
        input(
            "guide/start.md",
            "# Start\n\n\
             [Encoded network](%2f%2fexample.test/chapter.md?edition=2#part)\n\
             [Mixed network](/%2Fexample.test/chapter.md#part)\n\
             [Literal network](//example.test/chapter.md#part)\n\
             [Book root](%2Fexample.test/chapter.md#part)\n\
             [Relative](../example.test/chapter.md#part)\n\
             [Literal percent](../percent%2520.md?edition=2#part)\n\n\
             <a href='%2F%2Fexample.test/chapter.md#part'>HTML network</a>\n",
        ),
        input("example.test/chapter.md", "# Part\n"),
        input("percent%20.md", "# Part\n"),
    ])
    .unwrap();
    let options = HtmlOptions::default();
    let archive = epub::render_book_epub(&book, &options).unwrap();
    let chapter = entry_text(&archive, "OEBPS/chapter-1.xhtml");
    assert_well_formed_xml(&chapter);
    for (label, destination) in [
        (
            "Encoded network",
            "%2f%2fexample.test/chapter.md?edition=2#part",
        ),
        ("Mixed network", "/%2Fexample.test/chapter.md#part"),
        ("Literal network", "//example.test/chapter.md#part"),
        ("HTML network", "%2F%2Fexample.test/chapter.md#part"),
        ("Book root", "chapter-2.xhtml#part"),
        ("Relative", "chapter-2.xhtml#part"),
        ("Literal percent", "chapter-3.xhtml?edition=2#part"),
    ] {
        assert!(
            chapter.contains(&format!("href=\"{destination}\">{label}</a>")),
            "publication changed the target owned by {label}: {chapter}",
        );
    }
    assert_eq!(archive, epub::render_book_epub(&book, &options).unwrap());
}

#[test]
fn rejected_resource_paths_cannot_substitute_unrelated_host_images() {
    let book = build_book(&[input(
        "guide/start.md",
        "# Images\n\n\
         ![Network](%2f%2fexample.test/picture.svg)\n\n\
         ![Control](picture%0A.svg)\n\n\
         ![Rooted](/example.test/picture.svg)\n\n\
         ![Explicit](%2f%2fexplicit.test/picture.svg)\n",
    )])
    .unwrap();
    let rooted = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"green\"/></svg>";
    let explicit = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"blue\"/></svg>";
    let unrelated = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg>";
    let options = HtmlOptions {
        image_assets: vec![
            PdfImageAsset::new("example.test/picture.svg", rooted.to_vec()),
            PdfImageAsset::new("guide/picture\n.svg", unrelated.to_vec()),
            // Exact opaque host keys remain authorized, including an external
            // URL. Rejecting a path alias must not reject its explicit bytes.
            PdfImageAsset::new("%2f%2fexplicit.test/picture.svg", explicit.to_vec()),
        ],
        ..HtmlOptions::default()
    };
    let archive = epub::render_book_epub(&book, &options).unwrap();
    let chapter = entry_text(&archive, "OEBPS/chapter-1.xhtml");
    assert_well_formed_xml(&chapter);
    for (alt, destination) in [
        ("Network", "%2f%2fexample.test/picture.svg"),
        ("Control", "picture%0A.svg"),
    ] {
        assert!(chapter.contains(&format!("[Image unavailable: {alt}]</span>")));
        assert!(chapter.contains(&format!("data-image-source=\"{destination}\"")));
    }
    assert!(chapter.contains("src=\"assets/chapter-1-image-1.svg\" alt=\"Rooted\""));
    assert!(chapter.contains("src=\"assets/chapter-1-image-2.svg\" alt=\"Explicit\""));
    let images: Vec<_> = parse_central_directory(&archive)
        .into_iter()
        .filter(|entry| entry.name.starts_with("OEBPS/assets/"))
        .collect();
    assert_eq!(
        images.len(),
        2,
        "only explicitly resolved images are packaged"
    );
    assert_eq!(
        entry_text(&archive, "OEBPS/assets/chapter-1-image-1.svg").as_bytes(),
        rooted,
    );
    assert_eq!(
        entry_text(&archive, "OEBPS/assets/chapter-1-image-2.svg").as_bytes(),
        explicit,
    );
}

#[test]
fn host_created_book_paths_cannot_bypass_control_character_admission() {
    let original = build_book(&[input("chapter.md", "# Chapter\n")]).unwrap();
    for control in ['\t', '\n', '\r', '\u{7f}', '\u{85}'] {
        let mut book = original.clone();
        book.chapters[0].path = format!("chapter{control}.md");
        let Err(error) = epub::render_book_epub(&book, &HtmlOptions::default()) else {
            panic!("chapter path with {control:?} must be refused");
        };
        assert!(
            error.to_string().contains("chapter source path"),
            "refuse invalid source identity before generating an archive: {error}",
        );
    }
}
