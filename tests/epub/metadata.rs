//! Publication metadata is preserved across emitted HTML and EPUB packages.

use super::{assert_well_formed_xml, entry_text};
use franken_markdown::{BookInput, HtmlOptions, build_book, parse_markdown, render_epub};

const SOURCE: &str = "# Publication\n\nThe complete body.\n";

fn identifier(bytes: &[u8]) -> String {
    entry_text(bytes, "OEBPS/content.opf")
        .split_once("<dc:identifier id=\"bookid\">")
        .unwrap()
        .1
        .split_once("</dc:identifier>")
        .unwrap()
        .0
        .to_string()
}

fn publications(options: &HtmlOptions) -> [Vec<u8>; 2] {
    let book = build_book(&[BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }])
    .unwrap();
    [
        render_epub(&parse_markdown(SOURCE), options).unwrap(),
        franken_markdown::epub::render_book_epub(&book, options).unwrap(),
    ]
}

#[test]
fn creator_is_escaped_and_changes_publication_identity_without_changing_content() {
    let defaults = publications(&HtmlOptions::default());
    let options = HtmlOptions {
        author: Some("  Ada & <Research> \"É中\"\nTeam  ".into()),
        ..HtmlOptions::default()
    };
    let authored = publications(&options);
    assert_eq!(authored, publications(&options));
    for (old, new) in defaults.iter().zip(&authored) {
        let metadata = entry_text(new, "OEBPS/content.opf");
        assert_well_formed_xml(&metadata);
        assert!(
            metadata
                .contains("<dc:creator>  Ada &amp; &lt;Research&gt; \"É中\"\nTeam  </dc:creator>")
        );
        assert_eq!(metadata.matches("<dc:creator>").count(), 1);
        assert_ne!(identifier(old), identifier(new));
        assert!(!entry_text(old, "OEBPS/content.opf").contains("<dc:creator>"));
        for member in [
            "OEBPS/chapter-1.xhtml",
            "OEBPS/nav.xhtml",
            "OEBPS/style.css",
        ] {
            assert_eq!(entry_text(old, member), entry_text(new, member));
        }
    }
    let another = publications(&HtmlOptions {
        author: Some("Another author".into()),
        ..options
    });
    for (a, b) in authored.iter().zip(&another) {
        assert_ne!(identifier(a), identifier(b));
    }
}

#[test]
fn book_creator_inherits_first_chapter_and_explicit_author_wins() {
    let book = build_book(&[
        BookInput {
            path: "first.md".into(),
            source: format!("---\nauthor: Source author\n---\n{SOURCE}"),
        },
        BookInput {
            path: "second.md".into(),
            source: "---\nauthor: Later chapter\n---\n# Second\n".into(),
        },
    ])
    .unwrap();
    for (author, expected) in [
        (None, "Source author"),
        (Some("Book author"), "Book author"),
    ] {
        let bytes = franken_markdown::epub::render_book_epub(
            &book,
            &HtmlOptions {
                author: author.map(str::to_owned),
                ..HtmlOptions::default()
            },
        )
        .unwrap();
        let metadata = entry_text(&bytes, "OEBPS/content.opf");
        assert!(metadata.contains(&format!("<dc:creator>{expected}</dc:creator>")));
        assert!(!metadata.contains("Later chapter"));
    }
}

#[test]
fn forbidden_author_xml_characters_fail_before_an_archive_is_returned() {
    let book = build_book(&[BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }])
    .unwrap();
    for character in ['\0', '\u{000b}', '\u{fffe}'] {
        let options = HtmlOptions {
            author: Some(format!("Author{character}Name")),
            ..HtmlOptions::default()
        };
        for result in [
            render_epub(&parse_markdown(SOURCE), &options),
            franken_markdown::epub::render_book_epub(&book, &options),
        ] {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("content.opf"), "{error}");
        }
    }
}

#[test]
fn html_author_is_safe_and_identical_in_buffered_streaming_and_wasm_output() {
    let options = HtmlOptions {
        author: Some("É中 & \"quoted\" <script>alert(1)</script>".into()),
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    };
    let document = parse_markdown(SOURCE);
    let html = franken_markdown::render_html_document(&document, &options).unwrap();
    let mut streamed = Vec::new();
    franken_markdown::render_html_document_to_writer(&document, &options, &mut streamed).unwrap();
    assert_eq!(streamed, html.as_bytes());
    assert!(html.contains("<meta name=\"author\" content=\"É中 &amp; &quot;quoted&quot; &lt;script&gt;alert(1)&lt;/script&gt;\">"));
    assert!(!html.contains("<script>"));
    let wasm = franken_markdown::wasm::render_html(
        SOURCE,
        &franken_markdown::wasm::WasmRenderOptions {
            author: options.author,
            custom_css: options.custom_css,
            ..franken_markdown::wasm::WasmRenderOptions::default()
        },
    )
    .unwrap();
    assert_eq!(wasm.bytes, streamed);
}

#[cfg(feature = "cli")]
#[test]
fn native_html_and_epub_use_frontmatter_and_explicit_author_precedence() {
    use std::{fs, process::Command};
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("fmd-authors-{}-{stamp}", std::process::id()));
    fs::create_dir_all(root.join("book")).unwrap();
    let source = format!("---\nauthor: Source author\n---\n{SOURCE}");
    fs::write(root.join("chapter.md"), &source).unwrap();
    fs::write(root.join("book/chapter.md"), &source).unwrap();
    fs::write(
        root.join("book/book.toml"),
        "author = \"Manifest author\"\nchapters = [\"chapter.md\"]\n",
    )
    .unwrap();
    for (book, explicit, expected) in [
        (false, false, "Source author"),
        (false, true, "Flag author & team"),
        (true, false, "Manifest author"),
        (true, true, "Flag author & team"),
    ] {
        for target in ["html", "epub"] {
            let output = format!("{}-{explicit}.{target}", if book { "book" } else { "doc" });
            let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
            command.current_dir(&root).arg("--no-config");
            if book {
                command.args(["book", "book"]);
                command.args([
                    if target == "html" {
                        "--out-dir"
                    } else {
                        "--out"
                    },
                    &output,
                ]);
            } else {
                command.args(["render", "chapter.md", "--out", &output]);
            }
            command.args(["--to", target]);
            if explicit {
                command.args(["--author", "Flag author & team"]);
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let escaped = expected.replace('&', "&amp;");
            if target == "html" {
                let path = if book {
                    root.join(output).join("chapter.html")
                } else {
                    root.join(output)
                };
                let html = fs::read_to_string(path).unwrap();
                assert!(html.contains(&format!("<meta name=\"author\" content=\"{escaped}\">")));
            } else {
                let bytes = fs::read(root.join(output)).unwrap();
                let metadata = entry_text(&bytes, "OEBPS/content.opf");
                assert!(metadata.contains(&format!("<dc:creator>{escaped}</dc:creator>")));
            }
        }
    }
}
