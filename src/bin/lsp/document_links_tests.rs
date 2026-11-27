#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::LineIndex;
use super::*;

const ORIGIN: &str = "file:///manual/guide/start.md";

fn links(source: &str) -> Vec<Json> {
    let Json::Array(links) = request(Parser::default(), ORIGIN, source).unwrap() else {
        panic!("document links must be an array");
    };
    links
}

fn pointed_source<'a>(source: &'a str, link: &Json) -> &'a str {
    let index = LineIndex::new(source);
    let range = link.get("range").unwrap();
    let offset = |side| {
        index
            .offset(
                source,
                super::super::Position::parse(range.get(side).unwrap()).unwrap(),
            )
            .unwrap()
    };
    &source[offset("start")..offset("end")]
}

#[test]
fn unopened_relative_files_preserve_uri_components_without_guessing_aliases() {
    for (origin, destination, expected) in [
        (
            ORIGIN,
            "../Other chapter.md?view=1#café",
            "file:///manual/Other%20chapter.md?view=1#caf%C3%A9",
        ),
        (
            ORIGIN,
            "literal%252F.md",
            "file:///manual/guide/literal%252F.md",
        ),
        (
            ORIGIN,
            "what%3Fever%23x.md#part",
            "file:///manual/guide/what%3Fever%23x.md#part",
        ),
        (ORIGIN, "caf%C3%A9.md", "file:///manual/guide/caf%C3%A9.md"),
        (ORIGIN, "chapter", "file:///manual/guide/chapter"),
        (ORIGIN, "chapter/", "file:///manual/guide/chapter/"),
        (ORIGIN, "/", "file:///"),
        (
            ORIGIN,
            "file://LOCALHOST/other/paper.md",
            "file:///other/paper.md",
        ),
        (
            "file:///c:/docs/start.md",
            "other.md",
            "file:///C:/docs/other.md",
        ),
        (
            "file:///C:/docs/start.md",
            "/images/a.png",
            "file:///C:/images/a.png",
        ),
        (
            "file:///D:/docs/start.md",
            "/C:/images/a.png",
            "file:///C:/images/a.png",
        ),
        (
            "file://HOST/share/start.md",
            "other.md",
            "file://host/share/other.md",
        ),
        (
            "untitled:one",
            "file:///manual/start.md",
            "file:///manual/start.md",
        ),
    ] {
        assert_eq!(
            target(origin, destination).unwrap().as_deref(),
            Some(expected),
            "{origin} -> {destination}"
        );
    }
}

#[test]
fn web_and_mail_links_are_encoded_without_losing_reserved_characters() {
    for (destination, expected) in [
        (
            "https://example.test/café?q=a b&keep=%2F#résumé",
            "https://example.test/caf%C3%A9?q=a%20b&keep=%2F#r%C3%A9sum%C3%A9",
        ),
        (
            "HTTP://example.test/a(b)?x=1&y=2",
            "HTTP://example.test/a(b)?x=1&y=2",
        ),
        ("https://[::1]/a", "https://[::1]/a"),
        (
            "mailto:reader@example.test?subject=Résumé",
            "mailto:reader@example.test?subject=R%C3%A9sum%C3%A9",
        ),
    ] {
        assert_eq!(
            target("untitled:one", destination).unwrap().as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn unsafe_ambiguous_and_malformed_destinations_have_no_click_target() {
    for destination in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,hello",
        "vbscript:run",
        "command:workbench.action.files.openFile",
        "vscode://file/etc/passwd",
        "java\tscript:alert(1)",
        "https://example.test/\nnext",
        "https:\\evil.test\\x",
        "https://",
        "https:///path",
        "https://bad host/path",
        "https:relative",
        "mailto:",
        "//example.test/path",
        "file:relative.md",
        "file://user@host/share/a.md",
        "bad%escape.md",
        "bad%2.md",
        "bad%.md",
        "bad%2Fname.md",
        "bad%5Cname.md",
        "bad%00name.md",
        "../../../../escape.md",
        "#heading",
        "?view=1#heading",
        "",
    ] {
        assert_eq!(
            target(ORIGIN, destination).unwrap(),
            None,
            "{destination:?}"
        );
    }
    assert_eq!(target("untitled:one", "relative.md").unwrap(), None);
}

#[test]
fn actual_parser_tokens_keep_unicode_crlf_nested_and_reference_ranges() {
    let source = concat!(
        "😀 [doc](<../Other chapter.md?view=1&amp;x=2#résumé> \"Keep title\")\r\n\r\n",
        "> - ![image](images/a\\(b\\).svg)\r\n\r\n",
        "[ref]: https://example.test/caf%C3%A9?q=1&amp;y=2 \"title\"\r",
        "\r[used][ref]\r"
    );
    let result = links(source);
    assert_eq!(result.len(), 3);
    let actual: Vec<_> = result
        .iter()
        .map(|link| {
            (
                pointed_source(source, link),
                link.get("target").and_then(Json::as_str).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (
                "../Other chapter.md?view=1&amp;x=2#résumé",
                "file:///manual/Other%20chapter.md?view=1&x=2#r%C3%A9sum%C3%A9"
            ),
            (
                "images/a\\(b\\).svg",
                "file:///manual/guide/images/a%28b%29.svg"
            ),
            (
                "https://example.test/caf%C3%A9?q=1&amp;y=2",
                "https://example.test/caf%C3%A9?q=1&y=2"
            ),
        ]
    );
    assert_eq!(
        result[0]
            .get("range")
            .unwrap()
            .get("start")
            .unwrap()
            .get("character")
            .and_then(Json::as_u64),
        Some(10)
    );
}

#[test]
fn literal_html_code_titles_abandoned_links_and_autolinks_do_not_acquire_ranges() {
    let source = concat!(
        "`[code](wrong.md)`\n\n```md\n[x](wrong.md)\n```\n\n",
        "<!-- <a href=\"wrong.md\"> -->\n\n`<img src=wrong.md>`\n\n",
        "[bad](unterminated\n\n[bad]: <foo<bar>\n\n",
        "[good](right.md \"[title](wrong.md)\")\n\n",
        "https://example.test/bare <https://example.test/angle>\n\n",
        "[bad](javascript:alert(1)) [bad](command:run)\n"
    );
    let result = links(source);
    assert_eq!(result.len(), 1, "{result:?}");
    assert_eq!(pointed_source(source, &result[0]), "right.md");
}

#[test]
fn html_href_and_src_select_original_values_and_decode_targets() {
    let source = concat!(
        "😀 See <a href=' ../Café.md?q=a&amp;b=2#top ' href='ignored.md'>chapter</a>.\r\n\r\n",
        "<div>\r\n<img alt=\"preview\" src=images/diagram.svg>\r\n</div>\r\n\r\n",
        "<span href='wrong.md'>label</span> <a href='command:run'>skip</a>\n\n",
        "[Next](<next page.md>)"
    );
    let result = links(source);
    let actual: Vec<_> = result
        .iter()
        .map(|link| {
            (
                pointed_source(source, link),
                link.get("target").and_then(Json::as_str).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (
                "../Café.md?q=a&amp;b=2#top",
                "file:///manual/Caf%C3%A9.md?q=a&b=2#top"
            ),
            (
                "images/diagram.svg",
                "file:///manual/guide/images/diagram.svg"
            ),
            ("next page.md", "file:///manual/guide/next%20page.md"),
        ]
    );
}

#[test]
fn selected_authoring_grammar_owns_every_destination() {
    let source =
        "[Term\n: definition](target.md)\n\n`Term\n: definition\n[code](hidden.md)\nend`\n";
    let parser = Parser::initialize(
        &super::super::parse_json(r#"{"initializationOptions":{"profile":"commonmark-gfm"}}"#)
            .unwrap(),
    )
    .unwrap();
    let Json::Array(result) = request(parser, ORIGIN, source).unwrap() else {
        panic!("array");
    };
    assert_eq!(result.len(), 1);
    assert_eq!(pointed_source(source, &result[0]), "target.md");
    assert!(links("[Term\n: definition](target.md)\n").is_empty());
}

#[test]
fn results_and_encoded_targets_have_explicit_budgets() {
    assert!(matches!(
        target(ORIGIN, &"a".repeat(MAX_TARGET_BYTES + 1)),
        Err((-32803, _))
    ));
    assert!(matches!(
        target(ORIGIN, &"猫".repeat(1200)),
        Err((-32803, _))
    ));
    let source = "[link](a.md)\n\n".repeat(MAX_LINKS + 1);
    assert!(matches!(
        request(Parser::default(), ORIGIN, &source),
        Err((-32803, _))
    ));
    assert_eq!(
        links(&"[link](a.md)\n\n".repeat(MAX_LINKS)).len(),
        MAX_LINKS
    );
}

#[test]
fn monotonic_coordinates_match_editor_positions_at_every_character_boundary() {
    let source = "a😀\r\n猫\rb\n";
    let index = LineIndex::new(source);
    let mut coordinates = Coordinates::new();
    for offset in source
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(source.len()))
    {
        assert_eq!(
            coordinates.position(source, offset).unwrap(),
            index.position(source, offset),
            "byte {offset}"
        );
    }
    assert!(matches!(coordinates.position(source, 0), Err((-32803, _))));
    assert!(matches!(
        coordinates.position(source, source.len() + 1),
        Err((-32803, _))
    ));
    assert_eq!(
        coordinates.position(source, source.len()).unwrap(),
        index.position(source, source.len())
    );

    let mut coordinates = Coordinates::new();
    assert!(matches!(coordinates.position(source, 2), Err((-32803, _))));
    assert_eq!(
        coordinates.position(source, 5).unwrap(),
        Position {
            line: 0,
            character: 3
        }
    );
}

#[test]
fn expanded_targets_cannot_amplify_a_small_document_beyond_the_response_budget() {
    let origin = format!("file:///{}/start.md", "x".repeat(7900));
    let destination = target(&origin, "a.md").unwrap().unwrap();
    assert!(destination.len() < MAX_TARGET_BYTES);
    let source = "[x](a.md)\n\n".repeat(200);
    assert_eq!(
        request(Parser::default(), &origin, &source).unwrap_err(),
        (-32803, "document links exceed the 1 MiB response budget")
    );
    let result = request(Parser::default(), &origin, "[x](a.md)").unwrap();
    assert!(result.to_json_string().len() <= MAX_RESPONSE_BYTES);
    let Json::Array(result) = result else {
        panic!("document links must be an array");
    };
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].get("target").and_then(Json::as_str),
        Some(destination.as_str())
    );
}

#[test]
fn many_links_after_a_long_unicode_prefix_keep_exact_utf16_ranges() {
    let mut source = "😀".repeat(16 * 1024);
    source.push(' ');
    source.push_str(&"[x](a.md) ".repeat(256));
    let result = links(&source);
    assert_eq!(result.len(), 256);
    for (index, link) in result.iter().enumerate() {
        let range = link.get("range").unwrap();
        let start = Position::parse(range.get("start").unwrap()).unwrap();
        let end = Position::parse(range.get("end").unwrap()).unwrap();
        assert_eq!(start.line, 0);
        assert_eq!(end.line, 0);
        assert_eq!(start.character, 32 * 1024 + 5 + index * 10);
        assert_eq!(end.character, start.character + 4);
        assert_eq!(pointed_source(&source, link), "a.md");
    }
}
