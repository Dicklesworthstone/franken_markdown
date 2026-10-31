//! Full-font publishing through the real MCP dispatcher and renderers.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, BookRenderer, render_book_site};
use franken_markdown::mcp::{
    JsonValue as Json, base64_encode, handle_tool_call, parse_json, tools_list_result,
};
use franken_markdown::{FontAssetSlot, FontAssets, FontEmbedding, HtmlOptions};

const FONT: &[u8] = include_bytes!("fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");
const SOURCE: &str = "# Typography\n\noffice AVATAR z\u{301}\u{307}.\n";

fn text(value: &str) -> Json {
    Json::String(value.into())
}

fn fields<const N: usize>(values: [(&str, Json); N]) -> BTreeMap<String, Json> {
    values
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

fn args() -> BTreeMap<String, Json> {
    fields([(
        "fonts",
        Json::Array(vec![Json::Object(fields([
            ("slot", text("body-regular")),
            ("base64", text(&base64_encode(FONT))),
        ]))]),
    )])
}

fn call(name: &str, args: BTreeMap<String, Json>) -> Result<Json, (i32, String, &'static str)> {
    handle_tool_call(
        Some(&Json::Object(fields([
            ("name", text(name)),
            ("arguments", Json::Object(args)),
        ]))),
        8 * 1024 * 1024,
    )
}

fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected an array"),
    }
}

fn content(value: &Json) -> &str {
    assert_eq!(value.get("isError"), Some(&Json::Bool(false)));
    array(value.get("content").unwrap())[0]
        .get("text")
        .unwrap()
        .as_str()
        .unwrap()
}

fn decode_base64(encoded: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut value, mut bits) = (0u32, 0);
    for byte in encoded.bytes().take_while(|&byte| byte != b'=') {
        let digit = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
            .iter()
            .position(|&item| item == byte)
            .unwrap() as u32;
        value = (value << 6) | digit;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((value >> bits) as u8);
        }
    }
    out
}

fn workspace() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-mcp-full-fonts-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    root
}

fn options(policy: FontEmbedding) -> HtmlOptions {
    HtmlOptions {
        font_assets: FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, FONT.to_vec())
            .unwrap(),
        font_embedding: policy,
        ..HtmlOptions::default()
    }
}

// Independently inspect the ZIP entry and its uncompressed bytes, rather than
// trusting the renderer's own package reader or its font-policy receipt.
fn assert_epub_font(archive: &[u8]) {
    let u16_at = |at| u16::from_le_bytes(archive[at..at + 2].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(archive[at..at + 4].try_into().unwrap());
    let mut at = 0;
    while archive.get(at..at + 4) == Some(b"PK\x03\x04") {
        let name_end = at + 30 + usize::from(u16_at(at + 26));
        let start = name_end + usize::from(u16_at(at + 28));
        let end = start + u32_at(at + 18) as usize;
        if &archive[at + 30..name_end] == b"OEBPS/fonts/font-1.otf" {
            assert_eq!(u32_at(at + 22) as usize, FONT.len());
            assert_eq!(u32_at(at + 14), franken_markdown::crc32(FONT));
            if u16_at(at + 8) == 0 {
                assert_eq!(&archive[start..end], FONT);
            } else {
                assert_eq!(u16_at(at + 8), 8);
                let (mut a, mut b) = (1u32, 0u32);
                for byte in FONT {
                    a = (a + u32::from(*byte)) % 65521;
                    b = (b + a) % 65521;
                }
                let mut zlib = vec![0x78, 0x9c];
                zlib.extend_from_slice(&archive[start..end]);
                zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
                assert_eq!(
                    franken_markdown::zlib_decompress(&zlib, FONT.len()).unwrap(),
                    FONT
                );
            }
            return;
        }
        at = end;
    }
    panic!("EPUB did not contain the supplied complete OpenType font");
}

#[test]
fn in_memory_html_keeps_exact_full_font_and_compact_default() {
    let mut request = args();
    request.extend(fields([
        ("markdown", text(SOURCE)),
        ("htmlFontFormat", text("ttf")),
    ]));
    let compact = call("fmd.render_html", request.clone()).unwrap();
    request.insert("fontEmbedding".into(), text("subset"));
    assert_eq!(compact, call("fmd.render_html", request.clone()).unwrap());
    request.insert("fontEmbedding".into(), text(" FULL "));
    let full = call("fmd.render_html", request).unwrap();
    let font_uri = format!("data:font/otf;base64,{}", base64_encode(FONT));
    assert!(content(&full).contains(&font_uri));
    assert!(!content(&compact).contains(&font_uri));
}

#[test]
fn file_html_and_epub_preserve_source_font_bytes() {
    let root = workspace();
    let input = root.join("source.md");
    std::fs::write(&input, SOURCE).unwrap();
    for target in ["html", "epub"] {
        let output = root.join(format!("publication.{target}"));
        let mut request = args();
        request.extend(fields([
            ("path", text(input.to_str().unwrap())),
            ("out", text(output.to_str().unwrap())),
            ("to", text(target)),
            ("fontEmbedding", text("full")),
        ]));
        if target == "html" {
            request.insert("htmlFontFormat".into(), text("ttf"));
        }
        call("fmd.render_file", request).unwrap();
        let bytes = std::fs::read(output).unwrap();
        if target == "epub" {
            assert_epub_font(&bytes);
        } else {
            assert!(
                std::str::from_utf8(&bytes)
                    .unwrap()
                    .contains(&format!("data:font/otf;base64,{}", base64_encode(FONT)))
            );
        }
        assert_eq!(std::fs::read_to_string(&input).unwrap(), SOURCE);
    }
}

fn book_args(target: &str) -> BTreeMap<String, Json> {
    let mut request = args();
    request.extend(fields([
        (
            "chapters",
            Json::Array(vec![Json::Object(fields([
                ("path", text("chapter.md")),
                ("markdown", text(SOURCE)),
            ]))]),
        ),
        ("to", text(target)),
    ]));
    request
}

#[test]
fn book_site_and_epub_use_the_shared_full_policy() {
    let renderer = BookRenderer::new(&[BookInput {
        path: "chapter.md".into(),
        source: SOURCE.into(),
    }])
    .unwrap();
    for target in ["site", "epub"] {
        let mut request = book_args(target);
        request.insert("fontEmbedding".into(), text("full"));
        let result = call("fmd.render_book", request).unwrap();
        let receipt = parse_json(content(&result)).unwrap();
        let row = &array(receipt.get("outputs").unwrap())[0];
        let actual = decode_base64(row.get("data").unwrap().as_str().unwrap());
        let render = |policy| {
            if target == "site" {
                render_book_site(renderer.book(), &options(policy))
            } else {
                franken_markdown::epub::render_book_epub(renderer.book(), &options(policy))
            }
            .unwrap()
        };
        assert_eq!(actual, render(FontEmbedding::Full), "{target}");
        assert_ne!(actual, render(FontEmbedding::Subset), "{target}");
        if target == "epub" {
            assert_epub_font(&actual);
        }
    }
}

#[test]
fn combined_exports_change_html_epub_while_preserving_pdf_bytes() {
    let root = workspace();
    let input = root.join("source.md");
    std::fs::write(&input, SOURCE).unwrap();
    for book in [false, true] {
        let (name, mut request) = if book {
            ("fmd.render_book", book_args("all"))
        } else {
            let mut request = args();
            request.extend(fields([
                ("path", text(input.to_str().unwrap())),
                ("to", text("both")),
            ]));
            ("fmd.render_file", request)
        };
        request.insert("htmlFontFormat".into(), text("ttf"));
        let compact = parse_json(content(&call(name, request.clone()).unwrap())).unwrap();
        request.insert("fontEmbedding".into(), text("full"));
        let full = parse_json(content(&call(name, request).unwrap())).unwrap();
        let compact = array(compact.get("outputs").unwrap());
        let full = array(full.get("outputs").unwrap());
        assert_eq!(compact.len(), if book { 3 } else { 2 });
        assert_eq!(full.len(), compact.len());
        for (before, after) in compact.iter().zip(full) {
            assert_eq!(before.get("format"), after.get("format"));
            if before.get("format").and_then(Json::as_str) == Some("pdf") {
                assert_eq!(
                    before.get("data"),
                    after.get("data"),
                    "PDF bytes changed in {name}"
                );
            } else {
                assert_ne!(
                    before.get("data"),
                    after.get("data"),
                    "font policy dropped in {name}"
                );
            }
        }
    }
}

#[test]
fn invalid_or_unused_policies_refuse_before_io_or_replacement() {
    let root = workspace();
    let missing = root.join("missing.md");
    let output = root.join("previous.bin");
    std::fs::write(&output, b"previous publication").unwrap();
    for (target, policy, reason) in [
        ("html", "guess", "invalid_font_embedding"),
        ("epub", "guess", "invalid_font_embedding"),
        ("both", "guess", "invalid_font_embedding"),
        ("pdf", "full", "unsupported_target_option"),
        ("svg", "full", "unsupported_target_option"),
    ] {
        let request = fields([
            ("path", text(missing.to_str().unwrap())),
            ("out", text(output.to_str().unwrap())),
            ("to", text(target)),
            ("fontEmbedding", text(policy)),
        ]);
        assert_eq!(
            call("fmd.render_file", request).unwrap_err().2,
            reason,
            "{target}"
        );
        assert_eq!(std::fs::read(&output).unwrap(), b"previous publication");
        assert!(!missing.exists());
    }
    for value in [Json::Null, Json::Bool(true), Json::Number(1.0)] {
        let request = fields([("markdown", text(SOURCE)), ("fontEmbedding", value)]);
        assert_eq!(
            call("fmd.render_html", request).unwrap_err().2,
            "invalid_params"
        );
    }
    let mut request = book_args("pdf");
    request.insert("fontEmbedding".into(), text("full"));
    assert_eq!(
        call("fmd.render_book", request).unwrap_err().2,
        "unsupported_target_option"
    );
}

#[test]
fn discovery_advertises_the_same_target_contract_as_dispatch() {
    let discovery = tools_list_result();
    let tools = array(discovery.get("tools").unwrap());
    for name in [
        "fmd.render_html",
        "fmd.render_file",
        "fmd.render_book",
        "fmd.render_pdf",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Json::as_str) == Some(name))
            .unwrap();
        let property = tool
            .get("inputSchema")
            .unwrap()
            .get("properties")
            .unwrap()
            .get("fontEmbedding");
        if name == "fmd.render_pdf" {
            assert!(property.is_none());
        } else {
            let property = property.unwrap();
            assert_eq!(property.get("type").and_then(Json::as_str), Some("string"));
            if name == "fmd.render_file" {
                assert!(
                    property
                        .get("description")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .contains("Supported targets: html, both, epub")
                );
            }
        }
    }
}
