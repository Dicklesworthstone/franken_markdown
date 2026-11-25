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

#[cfg(feature = "mcp")]
#[test]
fn mcp_source_and_file_epub_preserve_creator_and_publication_identity() {
    use franken_markdown::mcp::{
        ERROR_RENDER_FAILED, INVALID_PARAMS, JsonValue as Json, base64_encode, handle_tool_call,
        parse_json,
    };
    use std::collections::BTreeMap;

    let text = |value: &str| Json::String(value.to_owned());
    let call = |name: &str, arguments: BTreeMap<String, Json>| {
        handle_tool_call(
            Some(&Json::Object(BTreeMap::from([
                ("name".into(), Json::String(name.to_owned())),
                ("arguments".into(), Json::Object(arguments)),
            ]))),
            1024 * 1024,
        )
    };
    let content = |value: Json| {
        assert_eq!(value.get("isError"), Some(&Json::Bool(false)));
        let Json::Array(items) = value.get("content").unwrap() else {
            panic!("expected MCP content")
        };
        items[0]
            .get("text")
            .and_then(Json::as_str)
            .unwrap()
            .to_owned()
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-mcp-epub-author-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("chapter.md");
    std::fs::write(&path, SOURCE).unwrap();

    let source_args = BTreeMap::from([
        ("markdown".into(), text(SOURCE)),
        ("title".into(), text("Publication")),
    ]);
    let mut file_args = BTreeMap::from([
        ("path".into(), text(&path.to_string_lossy())),
        ("title".into(), text("Publication")),
        ("to".into(), text("epub")),
    ]);
    let defaults = HtmlOptions {
        title: Some("Publication".into()),
        ..HtmlOptions::default()
    };
    let baseline = render_epub(&parse_markdown(SOURCE), &defaults).unwrap();
    let creator = "  Ada & <Research> \"É中\"\nTeam  ";
    let authored = render_epub(
        &parse_markdown(SOURCE),
        &HtmlOptions {
            author: Some(creator.into()),
            ..defaults
        },
    )
    .unwrap();
    let metadata = entry_text(&authored, "OEBPS/content.opf");
    assert_well_formed_xml(&metadata);
    assert!(
        metadata.contains("<dc:creator>  Ada &amp; &lt;Research&gt; \"É中\"\nTeam  </dc:creator>")
    );
    assert_ne!(identifier(&baseline), identifier(&authored));
    assert!(!entry_text(&baseline, "OEBPS/content.opf").contains("<dc:creator>"));
    for member in [
        "OEBPS/chapter-1.xhtml",
        "OEBPS/nav.xhtml",
        "OEBPS/style.css",
    ] {
        assert_eq!(entry_text(&baseline, member), entry_text(&authored, member));
    }

    for (name, request) in [
        ("fmd.render_epub", &source_args),
        ("fmd.render_file", &file_args),
    ] {
        assert_eq!(
            content(call(name, request.clone()).unwrap()),
            base64_encode(&baseline)
        );
        let mut request = request.clone();
        request.insert("author".into(), text(creator));
        let actual = content(call(name, request.clone()).unwrap());
        assert_eq!(actual, base64_encode(&authored), "{name}");
        assert_eq!(actual, content(call(name, request.clone()).unwrap()));
        request.insert("author".into(), text("Invalid\u{000b}creator"));
        let error = call(name, request).unwrap_err();
        assert_eq!(error.0, ERROR_RENDER_FAILED);
        assert!(error.1.contains("content.opf"), "{error:?}");
    }

    // Creator support is an EPUB addition. Existing HTML/PDF pairs still apply
    // author only to their PDF side, rather than changing the HTML metadata.
    file_args.insert("to".into(), text("both"));
    let before = parse_json(&content(
        call("fmd.render_file", file_args.clone()).unwrap(),
    ))
    .unwrap();
    file_args.insert("author".into(), text(creator));
    let after = parse_json(&content(
        call("fmd.render_file", file_args.clone()).unwrap(),
    ))
    .unwrap();
    let Json::Array(before) = before.get("outputs").unwrap() else {
        panic!("paired output")
    };
    let Json::Array(after) = after.get("outputs").unwrap() else {
        panic!("paired output")
    };
    assert_eq!(before[0].get("format"), Some(&text("html")));
    assert_eq!(before[1].get("format"), Some(&text("pdf")));
    assert_eq!(before[0].get("data"), after[0].get("data"));
    assert_ne!(before[1].get("data"), after[1].get("data"));

    let mut html = source_args;
    html.insert("author".into(), text(creator));
    assert_eq!(call("fmd.render_html", html).unwrap_err().0, INVALID_PARAMS);
    file_args.insert("to".into(), text("html"));
    assert_eq!(
        call("fmd.render_file", file_args).unwrap_err().2,
        "unsupported_target_option"
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), SOURCE);
}

#[test]
fn publication_titles_follow_visible_top_level_headings_in_source_order() {
    let sources = [
        "<H1>Printed &amp; <em>bound</em></H1>\n\n## Later heading",
        "# Printed &amp; <em>bound</em>\n\n## Later heading",
        "> # Quoted title\n\n<!-- <h1>Comment title</h1> -->\n\n\
         <pre>literal &lt;h1&gt;</pre>\n\n<h2>Printed &amp; bound</h2>",
    ];
    let expected = "<title>Printed &amp; bound</title>";
    for source in sources {
        for profile in [
            None,
            Some(franken_markdown::Profile::GfmPlus),
            Some(franken_markdown::Profile::CommonMarkGfm),
        ] {
            let options = HtmlOptions {
                profile,
                custom_css: Some(String::new()),
                ..HtmlOptions::default()
            };
            let doc = franken_markdown::parse_markdown_with_profile(source, profile);
            let html = franken_markdown::render_html_document(&doc, &options).unwrap();
            let mut streamed = Vec::new();
            franken_markdown::render_html_document_to_writer(&doc, &options, &mut streamed)
                .unwrap();
            assert_eq!(html.as_bytes(), streamed);
            assert!(html.contains(expected), "{source}");
            let bytes = render_epub(&doc, &options).unwrap();
            for member in ["OEBPS/nav.xhtml", "OEBPS/chapter-1.xhtml"] {
                let xhtml = entry_text(&bytes, member);
                assert_well_formed_xml(&xhtml);
                assert!(xhtml.contains(expected), "{source}: {member}");
            }
            let opf = entry_text(&bytes, "OEBPS/content.opf");
            assert!(
                opf.contains("<dc:title>Printed &amp; bound</dc:title>"),
                "{source}"
            );
            let nav = entry_text(&bytes, "OEBPS/nav.xhtml");
            assert!(nav.contains(">Printed &amp; bound</a>"), "{source}");
        }
    }
}

#[test]
fn inferred_titles_preserve_document_book_and_explicit_metadata_precedence() {
    let source = "<h1>First <strong>edition</strong></h1>\n\n# Later";
    let book = build_book(&[
        BookInput {
            path: "first.md".into(),
            source: source.into(),
        },
        BookInput {
            path: "second.md".into(),
            source: format!("---\ntitle: Source metadata\n---\n{source}"),
        },
        BookInput {
            path: "third.md".into(),
            source: "<blockquote><h2>Quoted heading</h2></blockquote>".into(),
        },
    ])
    .unwrap();
    let options = HtmlOptions {
        title: Some("Publication override".into()),
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    };
    let edition = franken_markdown::epub::render_book_epub(&book, &options).unwrap();
    for (index, expected) in ["First edition", "Source metadata", "third"].iter().enumerate() {
        let chapter = entry_text(&edition, &format!("OEBPS/chapter-{}.xhtml", index + 1));
        assert!(chapter.contains(&format!("<title>{expected}</title>")));
    }
    let doc = render_epub(&parse_markdown(source), &options).unwrap();
    for bytes in [&edition, &doc] {
        assert!(
            entry_text(bytes, "OEBPS/content.opf")
                .contains("<dc:title>Publication override</dc:title>")
        );
    }
    let nested = render_epub(&book.chapters[2].doc, &HtmlOptions::default()).unwrap();
    assert!(entry_text(&nested, "OEBPS/content.opf").contains("<dc:title>Document</dc:title>"));
}
