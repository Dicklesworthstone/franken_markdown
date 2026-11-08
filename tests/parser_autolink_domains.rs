//! GFM domain recognition must not turn malformed address text into links.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn link(label: &str, destination: &str) -> Inline {
    Inline::Link {
        dest: destination.to_owned(),
        title: None,
        content: vec![Inline::Text(label.to_owned())],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn bare_http_schemes_require_host_text_before_path_query_or_fragment() {
    for source in [
        "http://",
        "https://",
        "https:///path",
        "https://?query",
        "https://#fragment",
        "https://:443/path",
        "https://.",
    ] {
        assert_eq!(
            parse_inlines(source),
            vec![Inline::Text(source.to_owned())],
            "{source:?}"
        );
        assert_eq!(fragment(source), format!("<p>{source}</p>\n"));
        assert!(destination_spans(source).is_empty());
    }
}

#[test]
fn url_domains_reject_underscores_only_in_the_final_two_segments() {
    // GFM 6.9 and github/cmark-gfm's test/extensions.txt distinguish hostname
    // segments from earlier subdomains. This is not an RFC/DNS label validator.
    for source in [
        "www.alpha.beta._gamma",
        "www.alpha._beta.gamma",
        "https://alpha.beta._gamma/path",
        "https://alpha._beta.gamma:8443/path",
        "https://alpha.beta_gamma/one.two/three",
        "http://underscore_only",
    ] {
        assert_eq!(
            parse_inlines(source),
            vec![Inline::Text(source.to_owned())],
            "{source:?}"
        );
        assert_eq!(fragment(source), format!("<p>{source}</p>\n"));
    }

    for source in [
        "www._alpha.beta.gamma",
        "https://_alpha.beta.gamma/route_with_tail",
        "https://sub_name.host.example:8443/a_b?arg=c_d#e_f",
    ] {
        let destination = if source.starts_with("www.") {
            format!("http://{source}")
        } else {
            source.to_owned()
        };
        assert_eq!(parse_inlines(source), vec![link(source, &destination)]);
    }
}

#[test]
fn domain_checks_preserve_short_unicode_and_reference_supported_hosts() {
    // cmark-gfm explicitly supports short scheme hosts and Unicode symbols.
    // Its extended URL grammar also permits consecutive dots; do not silently
    // replace that grammar with strict DNS validity or impose label lengths.
    for source in [
        "http://localhost",
        "http://localhost:8080/path",
        "https://a",
        "https://api-v2.example:8443/path",
        "https://例え.テスト/path",
        "http://🍄.example/",
        "http://x🍄.example/",
        "http://first..example/path",
    ] {
        assert_eq!(parse_inlines(source), vec![link(source, source)]);
    }
    let source = "www.first..example/path";
    assert_eq!(
        parse_inlines(source),
        vec![link(source, &format!("http://{source}"))]
    );
}

#[test]
fn path_and_formatting_underscores_do_not_become_domain_restrictions() {
    for source in [
        "https://host.example/path_with_underscores",
        "https://host.example/path_a.b_c/end",
        "https://host.example?query_a=b_c",
        "https://host.example#section_one",
    ] {
        assert_eq!(parse_inlines(source), vec![link(source, source)]);
    }
    for (source, expected) in [
        (
            "_https://host.example/a_b_",
            "<p><em><a href=\"https://host.example/a_b\">https://host.example/a_b</a></em></p>\n",
        ),
        (
            "~~www.sub_domain.host.example/a_b~~.",
            "<p><del><a href=\"http://www.sub_domain.host.example/a_b\">www.sub_domain.host.example/a_b</a></del>.</p>\n",
        ),
        (
            "(https://host.example/a(b)).",
            "<p>(<a href=\"https://host.example/a(b)\">https://host.example/a(b)</a>).</p>\n",
        ),
    ] {
        assert_eq!(fragment(source), expected, "{source:?}");
    }
}

#[test]
fn consecutive_email_dots_end_the_address_without_linking_a_broken_domain() {
    for source in ["a@host..test", "a@host...test", "a@host..é"] {
        assert_eq!(
            parse_inlines(source),
            vec![Inline::Text(source.to_owned())],
            "{source:?}"
        );
        assert_eq!(fragment(source), format!("<p>{source}</p>\n"));
    }
    for suffix in ["..tail", "...tail", ".", "..."] {
        let source = format!("a@host.example{suffix}");
        assert_eq!(
            parse_inlines(&source),
            vec![
                link("a@host.example", "mailto:a@host.example"),
                Inline::Text(suffix.to_owned()),
            ]
        );
    }
}

#[test]
fn email_domains_keep_gfm_hyphens_underscores_and_numeric_segments() {
    for source in [
        "first.last+tag@_host.-example",
        "first-last_tag@-host.example",
        "a@host._example",
        "a@host.2",
    ] {
        assert_eq!(
            parse_inlines(source),
            vec![link(source, &format!("mailto:{source}"))]
        );
    }
    for source in ["a@host.example-", "a@host.example_", "a@localhost", "a@."] {
        assert_eq!(
            parse_inlines(source),
            vec![Inline::Text(source.to_owned())],
            "{source:?}"
        );
    }
}

#[test]
fn rejected_autolinks_preserve_real_destinations_and_all_parser_entrypoints() {
    let paragraph = "α https://?query www.alpha._beta.gamma a@host..test [kept](/réel#one).";
    let source = format!(
        "{paragraph}\n\n{paragraph}\n\n> β https://sub_name.host.example/path [second](/二)\n\n- a@host.example..tail [third](/three \"title\")\n"
    );
    for newline in ["\n", "\r\n", "\r"] {
        let source = source.replace('\n', newline);
        let document = parse_markdown(&source);
        assert_eq!(parse_markdown_profiled(&source).document, document);
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(spanned.to_document(), document);
        assert!(spanned.diagnostics.is_empty(), "{:?}", spanned.diagnostics);
        for block in &spanned.blocks {
            assert!(block.span.slice(&source).is_some());
        }

        let destinations = destination_spans(&source);
        assert_eq!(destinations.len(), 4);
        for (destination, expected) in
            destinations
                .iter()
                .zip(["/réel#one", "/réel#one", "/二", "/three"])
        {
            assert_eq!(destination.dest, expected);
            assert_eq!(&source[destination.range.clone()], expected);
        }
        let html = fragment(&source);
        assert!(!html.contains("href=\"https://?query\""));
        assert!(!html.contains("href=\"http://www.alpha._beta.gamma\""));
        assert!(!html.contains("href=\"mailto:a@host..test\""));
        assert!(html.contains("<a href=\"mailto:a@host.example\">a@host.example</a>..tail"));
    }
}

#[test]
fn rejected_domain_rescans_share_the_inline_budget_and_reset_for_later_paragraphs() {
    // Each underscore admits another URL candidate, but every candidate has an
    // invalid penultimate segment. Rejection must not make scanning unbounded.
    let hostile = format!(
        "[discard](/discard) **prefix** {}bad.test suffix",
        "www.a_".repeat(4096)
    );
    let source = format!("{hostile}\n\n[kept](/kept) https://host.example\n\n{hostile}");
    assert_eq!(parse_inlines(&hostile), vec![Inline::Text(hostile.clone())]);
    assert_eq!(
        parse_markdown(&source).blocks,
        vec![
            Block::Paragraph(vec![Inline::Text(hostile.clone())]),
            Block::Paragraph(vec![
                link("kept", "/kept"),
                Inline::Text(" ".into()),
                link("https://host.example", "https://host.example"),
            ]),
            Block::Paragraph(vec![Inline::Text(hostile)]),
        ]
    );
    let destinations = destination_spans(&source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/kept");
    assert_eq!(&source[destinations[0].range.clone()], "/kept");
}
