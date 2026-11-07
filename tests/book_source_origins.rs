//! Explicit-source books retain each included destination's defining file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, BookWorkspace, resolve_book_destination};
use franken_markdown::{Block, Inline};

const BLUE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#2468ac"/></svg>"##;
const RED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#cc2244"/></svg>"##;
const ROOT: &str = "# Start\n\n{{#include ../parts/body.md}}\n";
const BODY: &str =
    "![Chart](plot.svg \"Picture\")\n\n[Next](next.md#next \"Chapter\") [Local](#start)\n";
const EXPANDED: &str = "# Start\n\n![Chart](../parts/plot.svg \"Picture\")\n\n[Next](../parts/next.md#next \"Chapter\") [Local](#start)\n";

fn file(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn chapters(root: &str) -> Vec<BookInput> {
    vec![
        file("guide/start.md", root),
        file("parts/next.md", "# Next\n"),
    ]
}

fn captured() -> BookRenderer {
    BookRenderer::from_sources(&chapters(ROOT), &[file("parts/body.md", BODY)]).unwrap()
}

fn expected() -> BookRenderer {
    BookRenderer::new(&chapters(EXPANDED)).unwrap()
}

fn images(renderer: &mut BookRenderer) {
    renderer.set_image("parts/plot.svg", BLUE.to_vec()).unwrap();
    renderer.set_image("guide/plot.svg", RED.to_vec()).unwrap();
}

fn embedded(bytes: &[u8]) -> String {
    format!(
        "data:image/svg+xml;base64,{}",
        franken_markdown::html::base64_encode(bytes)
    )
}

#[test]
fn included_destinations_bind_to_the_fragment_directory() {
    let actual = captured();
    let expected = expected();
    assert_eq!(
        actual.book().chapters[0].doc,
        expected.book().chapters[0].doc
    );
    assert_eq!(actual.validate_links().unwrap().finding_count(), 0);
    assert_eq!(
        actual.source_length(),
        ROOT.len() + BODY.len() + "# Next\n".len()
    );
}

#[test]
fn included_image_uses_its_origin_even_when_the_chapter_has_a_same_named_asset() {
    let mut renderer = captured();
    images(&mut renderer);
    let html = renderer.render_chapter_html(0).unwrap();
    assert!(
        html.contains(&embedded(BLUE)),
        "included image must use parts/plot.svg"
    );
    assert!(
        !html.contains(&embedded(RED)),
        "guide/plot.svg belongs to another source"
    );
    assert!(html.contains("href=\"parts__next.html#next\""));
    assert!(html.contains("href=\"#start\""));
}

#[test]
fn selected_nested_reference_definitions_keep_their_own_origins() {
    let root =
        "# Start\n\n[Root next][n] ![Root chart][img]\n\n{{#include ../parts/body.md:shown}}\n";
    let body = "{{#include missing.md}}\n<!-- ANCHOR: shown -->\n{{#include deep/references.txt:2:7}}\n<!-- ANCHOR_END: shown -->\n";
    let references = "```md\r\n[n]: <../next.md#next> 'Next'\r\n[img]: <plot.svg?x=1#view> 'Image'\r\n\r\n[Included next][n] ![Included chart][img]\r\n\r\n[Local](#start)\r\n```\r\n";
    let expanded = "# Start\n\n[Root next][n] ![Root chart][img]\n\n[n]: <../parts/next.md#next> 'Next'\r\n[img]: <../parts/deep/plot.svg?x=1#view> 'Image'\r\n\r\n[Included next][n] ![Included chart][img]\r\n\r\n[Local](#start)\r\n";
    let renderer = BookRenderer::from_sources(
        &chapters(root),
        &[
            file("parts/body.md", body),
            file("parts/deep/references.txt", references),
        ],
    )
    .unwrap();
    let expected = BookRenderer::new(&chapters(expanded)).unwrap();
    assert_eq!(
        renderer.book().chapters[0].doc,
        expected.book().chapters[0].doc
    );
    assert_eq!(renderer.validate_links().unwrap().finding_count(), 0);
}

#[test]
fn origin_aware_books_match_manually_authored_pdf_epub_site_and_preview() {
    let mut actual = captured();
    let mut expected = expected();
    images(&mut actual);
    images(&mut expected);
    assert_eq!(actual.render_pdf().unwrap(), expected.render_pdf().unwrap());
    assert_eq!(
        actual.render_epub().unwrap(),
        expected.render_epub().unwrap()
    );
    assert_eq!(
        actual.render_site().unwrap(),
        expected.render_site().unwrap()
    );
    assert_eq!(
        actual.render_chapter_preview(0).unwrap(),
        expected.render_chapter_preview(0).unwrap()
    );
}

#[test]
fn html_attributes_unicode_urls_and_literal_examples_keep_their_meaning() {
    let root = "# Start\n\n{{#include \"../parts #中/body.txt\"}}\n";
    let body = "<p><img src='percent%2520%23plot.svg?x=1&amp;y=2#view' alt='Chart'><a href='../parts/next.md#next' title='Keep &quot;title&quot;'>Next</a></p>\n\n[Local](#start) [Remote](https://example.com/x?a=1&b=2)\n\n`![literal](plot.svg)`\n\n```text\n[code](next.md)\n```\n\n<!-- <img src='plot.svg'> -->\n";
    let expanded = "# Start\n\n<p><img src='../parts%20%23%E4%B8%AD/percent%2520%23plot.svg?x=1&amp;y=2#view' alt='Chart'><a href='../parts/next.md#next' title='Keep &quot;title&quot;'>Next</a></p>\n\n[Local](#start) [Remote](https://example.com/x?a=1&b=2)\n\n`![literal](plot.svg)`\n\n```text\n[code](next.md)\n```\n\n<!-- <img src='plot.svg'> -->\n";
    let mut actual =
        BookRenderer::from_sources(&chapters(root), &[file("parts #中/body.txt", body)]).unwrap();
    let mut expected = BookRenderer::new(&chapters(expanded)).unwrap();
    let key = "parts #中/percent%20#plot.svg?x=1&y=2#view";
    actual.set_image(key, BLUE.to_vec()).unwrap();
    expected.set_image(key, BLUE.to_vec()).unwrap();
    let html = actual.render_chapter_html(0).unwrap();
    assert_eq!(html, expected.render_chapter_html(0).unwrap());
    assert!(html.contains(&embedded(BLUE)));
    assert!(html.contains("href=\"parts__next.html#next\""));
    assert!(html.contains("href=\"#start\""));
    assert!(html.contains("href=\"https://example.com/x?a=1&amp;b=2\""));
    assert!(html.contains("![literal](plot.svg)"));
    assert!(!html.contains("![literal](../parts"));
    assert!(html.contains("[code](next.md)"));
    assert!(!html.contains("[code](../parts"));
}

#[test]
fn workspace_edits_rebase_changed_sources_and_preserve_other_state() {
    let sources = chapters(ROOT);
    let resources = [file("parts/body.md", BODY)];
    let mut workspace = BookWorkspace::from_sources(&sources, &resources).unwrap();
    workspace
        .set_image("parts/plot.svg", BLUE.to_vec())
        .unwrap();
    workspace
        .set_image("parts/revised.svg", RED.to_vec())
        .unwrap();
    workspace.options_mut().title = Some("Retained title".into());
    let untouched = workspace.book().chapters[1].doc.blocks.as_ptr();
    let asset = workspace.options().pdf_image_assets[0].bytes.as_ptr();
    let before = workspace.render_chapter_html(0).unwrap();
    assert!(before.contains(&embedded(BLUE)));

    let updated = "![Chart](revised.svg)\n\n[Next](next.md#next) [Local](#start)\n";
    let report = workspace
        .update_sources_at_revision(&[file("parts/body.md", updated)], 0)
        .unwrap();
    assert_eq!(report.reparsed_chapters, [0]);
    assert_eq!(report.changed_sources, 1);
    assert_eq!(report.revision, 1);
    assert_eq!(workspace.book().chapters[1].doc.blocks.as_ptr(), untouched);
    assert_eq!(
        workspace.options().pdf_image_assets[0].bytes.as_ptr(),
        asset
    );
    assert_eq!(workspace.options().title.as_deref(), Some("Retained title"));
    assert_eq!(workspace.validate_links().unwrap().finding_count(), 0);
    let after = workspace.render_chapter_html(0).unwrap();
    assert!(after.contains(&embedded(RED)));
    assert!(!after.contains(&embedded(BLUE)));

    let failure = workspace
        .update_sources_at_revision(&[file("parts/body.md", "{{#include ../../outside.md}}")], 1)
        .unwrap_err();
    assert!(failure.to_string().contains("include_escape"));
    assert_eq!(workspace.source_revision(), 1);
    assert_eq!(workspace.render_chapter_html(0).unwrap(), after);
    assert_eq!(
        workspace.options().pdf_image_assets[0].bytes.as_ptr(),
        asset
    );
    assert_eq!(sources[0].source, ROOT);
    assert_eq!(resources[0].source, BODY);
}

#[test]
fn moving_identical_snippet_text_changes_its_bound_resources_transactionally() {
    let mut sources = chapters(ROOT);
    sources.push(file("other/next.md", "# Next\n"));
    let mut workspace =
        BookWorkspace::from_sources(&sources, &[file("parts/body.md", BODY)]).unwrap();
    workspace
        .set_image("parts/plot.svg", BLUE.to_vec())
        .unwrap();
    workspace.set_image("other/plot.svg", RED.to_vec()).unwrap();
    assert!(
        workspace
            .render_chapter_html(0)
            .unwrap()
            .contains(&embedded(BLUE))
    );
    sources[0].source = "# Start\n\n{{#include ../other/body.md}}\n".into();
    let report = workspace
        .replace_sources_at_revision(&sources, &[file("other/body.md", BODY)], 0)
        .unwrap();
    assert!(report.changed);
    assert_eq!(report.revision, 1);
    let html = workspace.render_chapter_html(0).unwrap();
    assert!(html.contains(&embedded(RED)));
    assert!(!html.contains(&embedded(BLUE)));
    assert!(html.contains("href=\"other__next.html#next\""));
    assert_eq!(workspace.options().pdf_image_assets.len(), 2);
}

#[test]
fn an_escaping_fragment_url_never_rebinds_to_an_inside_chapter_or_image() {
    for destination in ["../../private", "%2e%2e/%2e%2e/private"] {
        let root = "# Start\n\n{{#include ../../parts/body.txt}}\n";
        let body =
            format!("[Private]({destination}.md#private)\n\n![Private]({destination}.svg)\n");
        let mut renderer = BookRenderer::from_sources(
            &[
                file("vol1/guide/start.md", root),
                file("private.md", "# Private\n"),
            ],
            &[file("parts/body.txt", &body)],
        )
        .unwrap();
        renderer.set_image("private.svg", RED.to_vec()).unwrap();
        let Block::Paragraph(inlines) = &renderer.book().chapters[0].doc.blocks[1] else {
            panic!("expected link paragraph")
        };
        let Inline::Link { dest, .. } = &inlines[0] else {
            panic!("expected source link")
        };
        assert!(
            resolve_book_destination("vol1/guide/start.md", dest).is_none(),
            "escaping source URL acquired an in-book target: {dest}"
        );
        assert!(
            !renderer
                .render_chapter_html(0)
                .unwrap()
                .contains(&embedded(RED)),
            "escaping image acquired unrelated host bytes"
        );
    }
}

#[cfg(feature = "mcp")]
mod mcp {
    use super::*;
    use franken_markdown::mcp::{
        DEFAULT_MAX_INPUT_BYTES, JsonValue as Json, base64_encode, handle_tool_call, parse_json,
    };

    fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
        Json::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    fn source(path: &str, markdown: &str) -> Json {
        object([
            ("path", Json::String(path.into())),
            ("markdown", Json::String(markdown.into())),
        ])
    }

    fn call(name: &str, arguments: Json) -> Json {
        let result = handle_tool_call(
            Some(&object([
                ("name", Json::String(name.into())),
                ("arguments", arguments),
            ])),
            DEFAULT_MAX_INPUT_BYTES,
        )
        .unwrap();
        assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
        let Json::Array(content) = result.get("content").unwrap() else {
            panic!("expected MCP content array")
        };
        parse_json(content[0].get("text").and_then(Json::as_str).unwrap()).unwrap()
    }

    fn sources() -> Json {
        object([
            (
                "chapters",
                Json::Array(vec![
                    source("guide/start.md", ROOT),
                    source("parts/next.md", "# Next\n"),
                ]),
            ),
            (
                "includeSources",
                Json::Array(vec![source("parts/body.md", BODY)]),
            ),
        ])
    }

    #[test]
    fn explicit_book_sources_publish_and_verify_against_the_defining_origin() {
        let report = call("fmd.verify_book", sources());
        assert_eq!(
            report
                .get("summary")
                .unwrap()
                .get("findings")
                .and_then(Json::as_u64),
            Some(0)
        );
        let mut request = sources();
        let Json::Object(fields) = &mut request else {
            panic!("expected object")
        };
        fields.insert("to".into(), Json::String("all".into()));
        fields.insert("strictLinks".into(), Json::Bool(true));
        fields.insert(
            "images".into(),
            Json::Array(vec![
                object([
                    ("destination", Json::String("parts/plot.svg".into())),
                    ("base64", Json::String(base64_encode(BLUE))),
                ]),
                object([
                    ("destination", Json::String("guide/plot.svg".into())),
                    ("base64", Json::String(base64_encode(RED))),
                ]),
            ]),
        );
        let receipt = call("fmd.render_book", request);
        let Json::Array(outputs) = receipt.get("outputs").unwrap() else {
            panic!("expected outputs")
        };
        let mut manual = expected();
        images(&mut manual);
        let expected = [
            manual.render_pdf().unwrap(),
            manual.render_epub().unwrap(),
            manual.render_site().unwrap(),
        ];
        assert_eq!(outputs.len(), expected.len());
        for (output, bytes) in outputs.iter().zip(expected) {
            assert_eq!(
                output.get("data").and_then(Json::as_str),
                Some(base64_encode(&bytes).as_str())
            );
        }
    }
}

#[cfg(feature = "wasm-book")]
#[test]
fn browser_binding_preserves_origins_across_live_source_updates() {
    use franken_markdown::book::browser::FmdBook;

    let mut browser = FmdBook::from_sources(
        vec!["guide/start.md".into(), "parts/next.md".into()],
        vec![ROOT.into(), "# Next\n".into()],
        vec!["parts/body.md".into()],
        vec![BODY.into()],
    )
    .unwrap();
    browser.set_image("parts/plot.svg", BLUE.to_vec()).unwrap();
    browser
        .set_image("parts/revised.svg", RED.to_vec())
        .unwrap();
    let mut manual = expected();
    manual.set_image("parts/plot.svg", BLUE.to_vec()).unwrap();
    manual.set_image("parts/revised.svg", RED.to_vec()).unwrap();
    assert_eq!(browser.render_pdf().unwrap(), manual.render_pdf().unwrap());
    assert_eq!(
        browser.render_epub().unwrap(),
        manual.render_epub().unwrap()
    );
    assert_eq!(
        browser.render_site().unwrap(),
        manual.render_site().unwrap()
    );

    browser
        .update_sources(
            vec!["parts/body.md".into()],
            vec![BODY.replace("plot.svg", "revised.svg")],
            0.0,
        )
        .unwrap();
    assert_eq!(browser.source_revision(), 1);
    let mut updated =
        BookRenderer::new(&chapters(&EXPANDED.replace("plot.svg", "revised.svg"))).unwrap();
    updated.set_image("parts/plot.svg", BLUE.to_vec()).unwrap();
    updated
        .set_image("parts/revised.svg", RED.to_vec())
        .unwrap();
    assert_eq!(
        browser.render_chapter_preview(0.0).unwrap(),
        updated.render_chapter_preview(0).unwrap()
    );
    assert_eq!(browser.render_pdf().unwrap(), updated.render_pdf().unwrap());
}

#[cfg(feature = "cli")]
mod native {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn workspace() -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "fmd-source-origins-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("book/parts")).unwrap();
        root
    }

    fn fmd(root: &Path, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "book", "book"])
            .args(arguments)
            .arg("--json")
            .env_remove("SOURCE_DATE_EPOCH")
            .current_dir(root)
            .output()
            .unwrap()
    }

    fn succeeded(output: Output) {
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn native_and_explicit_source_publications_match_and_protect_included_assets() {
        let root = workspace();
        fs::create_dir_all(root.join("book/guide")).unwrap();
        let sources = [
            ("guide/start.md", ROOT),
            ("parts/body.md", BODY),
            ("parts/next.md", "# Next\n"),
            (
                "book.toml",
                "order=['guide/start.md','parts/next.md']\ninclude_only=['parts/body.md']\n",
            ),
        ];
        for (path, contents) in sources {
            fs::write(root.join("book").join(path), contents).unwrap();
        }
        fs::write(root.join("book/parts/plot.svg"), BLUE).unwrap();
        fs::write(root.join("book/guide/plot.svg"), RED).unwrap();
        succeeded(fmd(
            &root,
            &[
                "--to",
                "both",
                "--out-dir",
                "publication",
                "--toc=false",
                "--pdf-page-numbers=false",
                "--deny-broken-links",
            ],
        ));

        let mut manual = expected();
        manual.options_mut().title = Some("Start".into());
        manual.set_image("parts/plot.svg", BLUE.to_vec()).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("publication/guide__start.html")).unwrap(),
            manual.render_chapter_html(0).unwrap()
        );
        assert_eq!(
            fs::read(root.join("publication/book.pdf")).unwrap(),
            manual.render_pdf().unwrap()
        );
        succeeded(fmd(
            &root,
            &[
                "--to",
                "epub",
                "--out",
                "publication.epub",
                "--toc=false",
                "--deny-broken-links",
            ],
        ));
        assert_eq!(
            fs::read(root.join("publication.epub")).unwrap(),
            manual.render_epub().unwrap()
        );

        let refused = fmd(&root, &["--to", "epub", "--out", "book/parts/plot.svg"]);
        assert_eq!(refused.status.code(), Some(73));
        assert!(
            String::from_utf8_lossy(&refused.stderr)
                .contains("output would overwrite a book input")
        );
        for (path, contents) in sources {
            assert_eq!(
                fs::read_to_string(root.join("book").join(path)).unwrap(),
                contents
            );
        }
        assert_eq!(fs::read(root.join("book/parts/plot.svg")).unwrap(), BLUE);
        assert_eq!(fs::read(root.join("book/guide/plot.svg")).unwrap(), RED);
    }

    #[test]
    fn native_rebasing_does_not_authorize_an_escaping_fragment_destination() {
        let root = workspace();
        fs::create_dir_all(root.join("book/vol1/guide")).unwrap();
        fs::write(
            root.join("book/vol1/guide/start.md"),
            "# Start\n\n{{#include ../../parts/body.txt}}\n",
        )
        .unwrap();
        fs::write(
            root.join("book/parts/body.txt"),
            "[Private](../../private.md#private)\n\n![Private](%2e%2e/%2e%2e/private.svg)\n",
        )
        .unwrap();
        fs::write(root.join("book/private.md"), "# Private\n").unwrap();
        fs::write(root.join("book/private.svg"), RED).unwrap();
        fs::write(
            root.join("book/book.toml"),
            "order=['vol1/guide/start.md','private.md']\n",
        )
        .unwrap();
        succeeded(fmd(&root, &["--to", "html", "--out-dir", "publication"]));
        let html = fs::read_to_string(root.join("publication/vol1__guide__start.html")).unwrap();
        assert!(
            !html.contains(&embedded(RED)),
            "escaping included URL loaded unrelated host bytes"
        );
        assert!(
            !html.contains("href=\"private.html#private\""),
            "escaping included URL bound to an unrelated chapter"
        );
    }
}
