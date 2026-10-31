//! Complete-font policy must reach native render, config and book exports.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{
    FontAssetSlot, FontAssets, FontEmbedding, HtmlFontFormat, HtmlOptions, crc32, parse_markdown,
    render_epub, render_html_document, zlib_decompress,
};

const FONT: &[u8] = include_bytes!("fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");
const TEXT: &str = "# Typography\n\noffice AVATAR z\u{0301}\u{0307}.\n";

fn workspace() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-full-fonts-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("chapters")).unwrap();
    fs::write(root.join("body.otf"), FONT).unwrap();
    fs::write(root.join("chapters/one.md"), TEXT).unwrap();
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .current_dir(root)
        .env("FMD_CONFIG", root.join("config"))
        .env_remove("SOURCE_DATE_EPOCH")
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn options(mode: FontEmbedding) -> HtmlOptions {
    HtmlOptions {
        font_assets: FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, FONT.to_vec())
            .unwrap(),
        html_font_format: HtmlFontFormat::Ttf,
        font_embedding: mode,
        ..HtmlOptions::default()
    }
}

fn html(root: &Path, extra: &[&str]) -> Vec<u8> {
    let mut args = vec![
        "--text",
        TEXT,
        "--html-font-format",
        "ttf",
        "--pdf-font",
        "body-regular=body.otf",
    ];
    args.extend_from_slice(extra);
    success(run(root, &args))
}

fn body_font(html: &[u8]) -> Vec<u8> {
    let text = std::str::from_utf8(html).unwrap();
    let encoded = text
        .split_once("data:font/otf;base64,")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    let mut out = Vec::new();
    let (mut value, mut bits) = (0u32, 0);
    for byte in encoded.bytes().take_while(|&b| b != b'=') {
        let digit = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
            .iter()
            .position(|&b| b == byte)
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

fn assert_epub_font(archive: &[u8], expected: &[u8]) {
    let u16_at = |at| u16::from_le_bytes(archive[at..at + 2].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(archive[at..at + 4].try_into().unwrap());
    let mut at = 0;
    while archive.get(at..at + 4) == Some(b"PK\x03\x04") {
        let name_end = at + 30 + usize::from(u16_at(at + 26));
        let start = name_end + usize::from(u16_at(at + 28));
        let end = start + u32_at(at + 18) as usize;
        if &archive[at + 30..name_end] == b"OEBPS/fonts/font-1.otf" {
            assert_eq!(u32_at(at + 22) as usize, expected.len());
            assert_eq!(u32_at(at + 14), crc32(expected));
            if u16_at(at + 8) == 0 {
                assert_eq!(&archive[start..end], expected);
            } else {
                assert_eq!(u16_at(at + 8), 8);
                let (mut a, mut b) = (1u32, 0u32);
                for byte in expected {
                    a = (a + u32::from(*byte)) % 65521;
                    b = (b + a) % 65521;
                }
                let mut zlib = vec![0x78, 0x9c];
                zlib.extend_from_slice(&archive[start..end]);
                zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
                assert_eq!(zlib_decompress(&zlib, expected.len()).unwrap(), expected);
            }
            return;
        }
        at = end;
    }
    panic!("EPUB did not contain the supplied OpenType font");
}

#[test]
fn native_html_full_preserves_font_bytes_and_default_remains_subset() {
    let root = workspace();
    let compact = html(&root, &[]);
    assert_eq!(compact, html(&root, &["--font-embedding", "subset"]));
    assert!(body_font(&compact).len() < FONT.len());
    let full = html(&root, &["--font-embedding", "full"]);
    assert_eq!(body_font(&full), FONT);
    assert_eq!(
        full,
        render_html_document(&parse_markdown(TEXT), &options(FontEmbedding::Full))
            .unwrap()
            .into_bytes()
    );
}

#[test]
fn persistent_font_policy_round_trips_and_explicit_flags_override_it() {
    let root = workspace();
    let compact = html(&root, &[]);
    assert_eq!(
        success(run(&root, &["config", "get", "font_embedding"])),
        b"subset\n"
    );
    success(run(&root, &["config", "set", "font_embedding", "full"]));
    assert_eq!(
        success(run(&root, &["config", "get", "font_embedding"])),
        b"full\n"
    );
    assert_eq!(body_font(&html(&root, &[])), FONT);
    assert_eq!(html(&root, &["--font-embedding", "subset"]), compact);
    assert_eq!(html(&root, &["--no-config"]), compact);
    let saved = fs::read(root.join("config")).unwrap();
    let invalid = run(&root, &["config", "set", "font_embedding", "unknown"]);
    assert_eq!(invalid.status.code(), Some(64));
    assert_eq!(fs::read(root.join("config")).unwrap(), saved);
}

#[test]
fn native_epub_full_reaches_supplied_and_bundled_fonts() {
    let root = workspace();
    for supplied in [false, true] {
        let mut args = vec![
            "--text",
            TEXT,
            "--to",
            "epub",
            "--out",
            "document.epub",
            "--font-embedding",
            "full",
        ];
        if supplied {
            args.extend(["--pdf-font", "body-regular=body.otf"]);
        }
        success(run(&root, &args));
        let actual = fs::read(root.join("document.epub")).unwrap();
        let opts = if supplied {
            options(FontEmbedding::Full)
        } else {
            HtmlOptions {
                font_embedding: FontEmbedding::Full,
                ..HtmlOptions::default()
            }
        };
        assert_eq!(actual, render_epub(&parse_markdown(TEXT), &opts).unwrap());
        assert!(
            actual
                .windows(b"OEBPS/fonts/".len())
                .any(|part| part == b"OEBPS/fonts/")
        );
        if supplied {
            assert_epub_font(&actual, FONT);
        }
    }
}

#[test]
fn book_exports_apply_configured_policy_and_explicit_override() {
    let root = workspace();
    fs::write(root.join("config"), "font_embedding=full\n").unwrap();
    success(run(
        &root,
        &[
            "book",
            "chapters",
            "--to",
            "html",
            "--out-dir",
            "site",
            "--html-font-format",
            "ttf",
            "--pdf-font",
            "body-regular=body.otf",
        ],
    ));
    assert_eq!(
        body_font(&fs::read(root.join("site/one.html")).unwrap()),
        FONT
    );
    success(run(
        &root,
        &[
            "book",
            "chapters",
            "--to",
            "epub",
            "--out-dir",
            "publication",
            "--pdf-font",
            "body-regular=body.otf",
        ],
    ));
    assert_epub_font(
        &fs::read(root.join("publication/chapters.epub")).unwrap(),
        FONT,
    );
    success(run(
        &root,
        &[
            "book",
            "chapters",
            "--to",
            "html",
            "--out-dir",
            "compact",
            "--html-font-format",
            "ttf",
            "--font-embedding",
            "subset",
            "--pdf-font",
            "body-regular=body.otf",
        ],
    ));
    assert!(body_font(&fs::read(root.join("compact/one.html")).unwrap()).len() < FONT.len());
}

#[test]
fn book_rejects_explicit_html_epub_font_policy_for_pdf_only_output() {
    let root = workspace();
    let output = run(
        &root,
        &[
            "book",
            "chapters",
            "--to",
            "pdf",
            "--out-dir",
            "pdf",
            "--font-embedding",
            "full",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported_target_option"));
    assert!(!root.join("pdf/chapters.pdf").exists());
}
