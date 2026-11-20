//! Authoring dialects must select source ownership before MCP rendering or audit.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, BookRenderer, render_book_site};
use franken_markdown::mcp::{
    ERROR_INVALID_OPTIONS, INVALID_PARAMS, JsonValue as Json, base64_encode, handle_tool_call,
    parse_json, tools_list_result,
};
use franken_markdown::{
    HtmlOptions, PdfASettings, PdfOptions, Profile, SvgOptions, apply_profile,
    parse_markdown_with_profile,
};

type Args = BTreeMap<String, Json>;
type Failure = (i32, String, &'static str);
const SOURCE: &str = "*Term\n: definition*\n\n<!-- pagebreak -->\n\nTail.\n";
const TOOLS: [&str; 6] = [
    "fmd.render_html",
    "fmd.render_pdf",
    "fmd.verify",
    "fmd.render_file",
    "fmd.render_book",
    "fmd.verify_book",
];
static NEXT: AtomicU64 = AtomicU64::new(0);

fn text(value: &str) -> Json {
    Json::String(value.to_string())
}

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected array"),
    }
}

fn args(source: &str, profile: Option<Profile>) -> Args {
    let mut args = BTreeMap::from([("markdown".into(), text(source))]);
    if let Some(profile) = profile {
        args.insert("profile".into(), text(profile.as_str()));
    }
    args
}

fn book_args(source: &str, profile: Option<Profile>) -> Args {
    let mut args = args(source, profile);
    args.remove("markdown");
    args.insert(
        "chapters".into(),
        Json::Array(vec![object([
            ("path", text("chapter.md")),
            ("markdown", text(source)),
        ])]),
    );
    args
}

fn call(name: &str, args: Args) -> Result<Json, Failure> {
    handle_tool_call(
        Some(&object([
            ("name", text(name)),
            ("arguments", Json::Object(args)),
        ])),
        1024 * 1024,
    )
}

fn content(value: &Json) -> &str {
    assert_eq!(value.get("isError"), Some(&Json::Bool(false)));
    array(value.get("content").unwrap())[0]
        .get("text")
        .and_then(Json::as_str)
        .unwrap()
}

fn scratch() -> PathBuf {
    for _ in 0..1000 {
        let path = std::env::temp_dir().join(format!(
            "fmd-mcp-authoring-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create scratch directory: {error}"),
        }
    }
    panic!("scratch names exhausted")
}

#[test]
fn discovery_advertises_the_same_optional_dialects_on_all_source_tools() {
    let schema = tools_list_result();
    for name in TOOLS {
        let tool = array(schema.get("tools").unwrap())
            .iter()
            .find(|tool| tool.get("name").and_then(Json::as_str) == Some(name))
            .unwrap();
        let input = tool.get("inputSchema").unwrap();
        let profile = input.get("properties").unwrap().get("profile").unwrap();
        assert_eq!(profile.get("type"), Some(&text("string")), "{name}");
        assert_eq!(
            array(profile.get("enum").unwrap()),
            [text("commonmark-gfm"), text("gfm-plus")],
            "{name}"
        );
        assert!(profile.get("default").is_none(), "{name}");
        assert!(!array(input.get("required").unwrap()).contains(&text("profile")));
        assert_eq!(input.get("additionalProperties"), Some(&Json::Bool(false)));
    }
}

#[test]
fn unknown_profiles_fail_before_source_access_and_resource_decoding() {
    for name in TOOLS {
        for invalid in ["", " ", "gfm", "default", "GFM-PLUS", "gfm-plus ", "2b"] {
            let mut request = if name.ends_with("_book") {
                // A valid outer shape with malformed source entries would fail
                // source admission if the profile were not validated first.
                BTreeMap::from([("chapters".into(), Json::Array(vec![Json::Null]))])
            } else if name == "fmd.render_file" {
                BTreeMap::from([("path".into(), text("/absent/profile-test/source.md"))])
            } else {
                args(SOURCE, None)
            };
            request.insert("profile".into(), text(invalid));
            if name != "fmd.verify_book" {
                request.insert("images".into(), Json::Array(vec![Json::Null]));
            }
            let failure = call(name, request).unwrap_err();
            assert_eq!(failure.0, ERROR_INVALID_OPTIONS, "{name}: {invalid:?}");
            assert_eq!(failure.2, "invalid_profile", "{name}: {invalid:?}");
        }
    }
}

#[test]
fn wrong_profile_types_are_protocol_errors_instead_of_defaults() {
    for name in TOOLS {
        for invalid in [Json::Null, Json::Bool(false), Json::Number(1.0)] {
            let mut request = if name.ends_with("_book") {
                book_args(SOURCE, None)
            } else if name == "fmd.render_file" {
                BTreeMap::from([("path".into(), text("/absent/profile-test/source.md"))])
            } else {
                args(SOURCE, None)
            };
            request.insert("profile".into(), invalid);
            assert_eq!(call(name, request).unwrap_err().0, INVALID_PARAMS, "{name}");
        }
    }
}

#[test]
fn html_exports_select_the_grammar_before_parsing() {
    let mut rendered = Vec::new();
    for profile in [None, Some(Profile::GfmPlus), Some(Profile::CommonMarkGfm)] {
        let options = HtmlOptions {
            profile,
            ..HtmlOptions::default()
        };
        let result = call("fmd.render_html", args(SOURCE, profile)).unwrap();
        let html = content(&result);
        assert_eq!(
            html,
            franken_markdown::render_html(SOURCE, &options).unwrap()
        );
        if profile == Some(Profile::CommonMarkGfm) {
            assert!(html.contains("<p><em>Term\n: definition</em></p>"));
            assert!(!html.contains("<dl>"));
        } else {
            assert!(html.contains("<dl>"));
        }
        rendered.push(html.to_owned());
    }
    assert_eq!(rendered[0], rendered[1]);
    assert_ne!(rendered[1], rendered[2]);
}

#[test]
fn explicit_profiles_refuse_interactive_exports_before_source_or_resource_access() {
    let root = scratch();
    let output = root.join("published.html");
    std::fs::write(&output, b"existing HTML").unwrap();
    std::fs::write(output.with_extension("pdf"), b"existing PDF").unwrap();
    for profile in [Profile::CommonMarkGfm, Profile::GfmPlus] {
        let mut request = args(SOURCE, Some(profile));
        request.insert("interactiveHtml".into(), Json::Bool(true));
        // If profile admission is delayed, resource decoding fails first.
        request.insert("images".into(), Json::Array(vec![Json::Null]));
        let error = call("fmd.render_html", request).unwrap_err();
        assert_eq!(error.0, ERROR_INVALID_OPTIONS);
        assert_eq!(error.2, "profile_interactive_unsupported");
        for target in ["html", "both"] {
            let request = BTreeMap::from([
                (
                    "path".into(),
                    text(&root.join("missing.md").to_string_lossy()),
                ),
                ("out".into(), text(&output.to_string_lossy())),
                ("to".into(), text(target)),
                ("profile".into(), text(profile.as_str())),
                ("interactiveHtml".into(), Json::Bool(true)),
            ]);
            let error = call("fmd.render_file", request).unwrap_err();
            assert_eq!(error.0, ERROR_INVALID_OPTIONS);
            assert_eq!(error.2, "profile_interactive_unsupported");
        }
    }
    assert_eq!(std::fs::read(output).unwrap(), b"existing HTML");
    assert_eq!(
        std::fs::read(root.join("published.pdf")).unwrap(),
        b"existing PDF"
    );
}

#[test]
fn legacy_interactive_exports_preserve_the_existing_native_bytes() {
    let mut request = args(SOURCE, None);
    request.insert("interactiveHtml".into(), Json::Bool(true));
    request.insert("title".into(), text("Legacy"));
    let document = franken_markdown::parse_markdown(SOURCE);
    let expected = franken_markdown::interactive::render_interactive_html(
        &document,
        SOURCE,
        &HtmlOptions {
            title: Some("Legacy".into()),
            ..HtmlOptions::default()
        },
    );
    assert_eq!(
        content(&call("fmd.render_html", request).unwrap()),
        expected
    );
    let root = scratch();
    let path = root.join("source.md");
    std::fs::write(&path, SOURCE).unwrap();
    let request = BTreeMap::from([
        ("path".into(), text(&path.to_string_lossy())),
        ("to".into(), text("html")),
        ("title".into(), text("Legacy")),
        ("interactiveHtml".into(), Json::Bool(true)),
    ]);
    assert_eq!(
        content(&call("fmd.render_file", request).unwrap()),
        expected
    );
}

#[test]
fn pdf_exports_and_verification_use_the_same_requested_page_break_policy() {
    for (profile, pages) in [(Profile::CommonMarkGfm, 1), (Profile::GfmPlus, 2)] {
        let options = PdfOptions {
            profile: Some(profile),
            ..PdfOptions::default()
        };
        let expected = franken_markdown::render_pdf(SOURCE, &options).unwrap();
        let result = call("fmd.render_pdf", args(SOURCE, Some(profile))).unwrap();
        assert_eq!(content(&result), base64_encode(&expected));
        let result = call("fmd.verify", args(SOURCE, Some(profile))).unwrap();
        let report = parse_json(content(&result)).unwrap();
        assert_eq!(report.get("page_count").and_then(Json::as_u64), Some(pages));
    }
}

#[test]
fn verification_does_not_audit_literal_code_as_images_or_lose_cross_line_links() {
    let source = "`Term\n: ![](missing.png)`\n\n[Term\n: definition](#missing)\n";
    for profile in [Profile::CommonMarkGfm, Profile::GfmPlus] {
        let document = parse_markdown_with_profile(source, Some(profile));
        let options = PdfOptions {
            profile: Some(profile),
            ..PdfOptions::default()
        };
        let expected = franken_markdown::verify::verify_pdf_result(&document, &options).unwrap();
        let result = call("fmd.verify", args(source, Some(profile))).unwrap();
        assert_eq!(
            content(&result),
            franken_markdown::verify::to_json(&expected)
        );
        let report = parse_json(content(&result)).unwrap();
        let codes: Vec<_> = array(report.get("findings").unwrap())
            .iter()
            .map(|finding| finding.get("code").and_then(Json::as_str).unwrap())
            .collect();
        assert_eq!(
            codes.contains(&"missing_alt_text"),
            profile == Profile::GfmPlus
        );
        assert_eq!(
            codes.contains(&"unresolved_anchor"),
            profile == Profile::CommonMarkGfm
        );
    }
}

#[test]
fn every_file_format_matches_the_same_profiled_native_document() {
    let root = scratch();
    let path = root.join("source.md");
    std::fs::write(&path, SOURCE).unwrap();
    let profile = Some(Profile::CommonMarkGfm);
    let document = parse_markdown_with_profile(SOURCE, profile);
    let html_options = HtmlOptions {
        title: Some("Dialect".into()),
        profile,
        ..HtmlOptions::default()
    };
    let pdf_options = PdfOptions {
        title: Some("Dialect".into()),
        profile,
        ..PdfOptions::default()
    };
    let html = franken_markdown::render_html_document(&document, &html_options).unwrap();
    let pdf =
        base64_encode(&franken_markdown::render_pdf_document(&document, &pdf_options).unwrap());
    let epub = base64_encode(&franken_markdown::render_epub(&document, &html_options).unwrap());
    let svg = franken_markdown::svg::render_svg_with_resources(
        &apply_profile(&document, profile),
        &SvgOptions::default(),
        &Default::default(),
        &[],
    )
    .unwrap()
    .0;
    let svg = String::from_utf8(svg).unwrap();
    for target in ["html", "pdf", "both", "epub", "svg"] {
        let mut request = BTreeMap::from([
            ("path".into(), text(&path.to_string_lossy())),
            ("to".into(), text(target)),
            ("profile".into(), text("commonmark-gfm")),
        ]);
        if target != "svg" {
            request.insert("title".into(), text("Dialect"));
        }
        let result = call("fmd.render_file", request).unwrap();
        match target {
            "html" => assert_eq!(content(&result), html),
            "pdf" => assert_eq!(content(&result), pdf),
            "epub" => assert_eq!(content(&result), epub),
            "svg" => assert_eq!(content(&result), svg),
            "both" => {
                let receipt = parse_json(content(&result)).unwrap();
                let outputs = array(receipt.get("outputs").unwrap());
                assert_eq!(outputs.len(), 2);
                assert_eq!(
                    outputs[0].get("data").and_then(Json::as_str),
                    Some(html.as_str())
                );
                assert_eq!(
                    outputs[1].get("data").and_then(Json::as_str),
                    Some(pdf.as_str())
                );
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), SOURCE);
}

#[test]
fn invalid_file_profiles_leave_existing_source_and_outputs_untouched() {
    let root = scratch();
    let source = root.join("source.md");
    let output = root.join("published.html");
    std::fs::write(&source, SOURCE).unwrap();
    std::fs::write(&output, b"existing HTML").unwrap();
    std::fs::write(output.with_extension("pdf"), b"existing PDF").unwrap();
    let request = BTreeMap::from([
        ("path".into(), text(&source.to_string_lossy())),
        ("out".into(), text(&output.to_string_lossy())),
        ("to".into(), text("both")),
        ("profile".into(), text("unrecognized")),
    ]);
    assert_eq!(
        call("fmd.render_file", request).unwrap_err().2,
        "invalid_profile"
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
    assert_eq!(std::fs::read(output).unwrap(), b"existing HTML");
    assert_eq!(
        std::fs::read(root.join("published.pdf")).unwrap(),
        b"existing PDF"
    );
}

#[test]
fn book_include_expansion_obeys_the_profile_before_resolving_literal_examples() {
    let source = "# Chapter\n\n`Term\n: definition\n{{#include absent.md}}\ntail`\n";
    let chapters = [BookInput {
        path: "chapter.md".into(),
        source: source.into(),
    }];
    let profile = Some(Profile::CommonMarkGfm);
    let renderer = BookRenderer::from_sources_with_profile(&chapters, &[], profile).unwrap();
    let expected = renderer.validate_links_with_sources().unwrap().to_json();
    assert_eq!(
        content(&call("fmd.verify_book", book_args(source, profile)).unwrap()),
        expected
    );
    for tool in ["fmd.verify_book", "fmd.render_book"] {
        let mut plus = book_args(source, Some(Profile::GfmPlus));
        if tool == "fmd.render_book" {
            plus.insert("to".into(), text("all"));
        }
        assert_eq!(call(tool, plus).unwrap_err().2, "invalid_book_sources");
    }

    let mut request = book_args(source, profile);
    request.insert("to".into(), text("all"));
    let result = call("fmd.render_book", request).unwrap();
    let receipt = parse_json(content(&result)).unwrap();
    let outputs = array(receipt.get("outputs").unwrap());
    assert_eq!(outputs.len(), 3);
    let html_options = HtmlOptions {
        profile,
        ..HtmlOptions::default()
    };
    let pdf_options = PdfOptions {
        profile,
        ..PdfOptions::default()
    };
    for (output, (format, expected)) in outputs.iter().zip([
        (
            "pdf",
            renderer
                .render_pdf_with_options(&pdf_options, PdfASettings::OFF)
                .unwrap(),
        ),
        (
            "epub",
            franken_markdown::epub::render_book_epub(renderer.book(), &html_options).unwrap(),
        ),
        (
            "site",
            render_book_site(renderer.book(), &html_options).unwrap(),
        ),
    ]) {
        assert_eq!(output.get("format").and_then(Json::as_str), Some(format));
        assert_eq!(
            output.get("data").and_then(Json::as_str),
            Some(base64_encode(&expected).as_str())
        );
    }
}

#[test]
fn strict_book_navigation_reports_links_created_by_the_requested_grammar() {
    let source = "# Chapter\n\n[Term\n: definition](#missing)\n";
    for (profile, findings) in [(Profile::CommonMarkGfm, 1), (Profile::GfmPlus, 0)] {
        let result = call("fmd.verify_book", book_args(source, Some(profile))).unwrap();
        let report = parse_json(content(&result)).unwrap();
        assert_eq!(
            report
                .get("summary")
                .unwrap()
                .get("findings")
                .and_then(Json::as_u64),
            Some(findings)
        );
    }
    let mut request = book_args(source, Some(Profile::CommonMarkGfm));
    request.insert("to".into(), text("all"));
    request.insert("strictLinks".into(), Json::Bool(true));
    assert_eq!(
        call("fmd.render_book", request).unwrap_err().2,
        "book_links_failed"
    );
}
