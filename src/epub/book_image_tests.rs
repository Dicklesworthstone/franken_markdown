use super::*;
use super::super::{prepare_book, render_book_epub};
use franken_markdown::{BookInput, HtmlOptions, PdfImageAsset, build_book};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn resource(href: &str, media_type: &'static str, bytes: &[u8]) -> Resource {
    Resource { href: href.into(), media_type, bytes: bytes.to_vec() }
}

#[test]
fn same_bytes_share_the_first_resource_across_chapters() -> TestResult {
    let mut pool = SharedImages::default();
    let mut first = super::super::resources::prepare_with_prefix(
        "<img src='data:image/png;base64,AQID'/>", "chapter-1-",
    )?;
    let mut second = super::super::resources::prepare_with_prefix(
        "<img src='data:image/png;base64,AQID'/><img src='data:image/gif;base64,BAUG'/>",
        "chapter-2-",
    )?;
    pool.prepare(0, &mut first)?;
    pool.prepare(1, &mut second)?;
    assert!(first.resources.is_empty());
    assert!(second.resources.is_empty());
    assert!(second.body.contains("src='assets/chapter-1-image-1.png'"));
    assert!(second.body.contains("src='assets/chapter-2-image-2.gif'"));
    assert_eq!(pool.bytes, 6);
    let resources = pool.finish();
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].0, 0);
    assert_eq!(resources[1].0, 1);
    assert_eq!(resources[0].1.bytes, [1, 2, 3]);
    Ok(())
}

#[test]
fn fingerprint_collisions_and_different_mime_types_are_not_identity() -> TestResult {
    let mut pool = SharedImages::default();
    assert_eq!(pool.intern(0, resource("first", "image/png", b"abc"), 0)?, "first");
    assert_eq!(pool.intern(1, resource("other", "image/png", b"def"), 0)?, "other");
    assert_eq!(pool.intern(2, resource("gif", "image/gif", b"abc"), 0)?, "gif");
    assert_eq!(pool.intern(3, resource("alias", "image/png", b"abc"), 0)?, "first");
    assert_eq!(pool.bytes, 9);
    assert_eq!(pool.resources.len(), 3);
    Ok(())
}

#[test]
fn duplicate_admission_does_not_consume_an_exhausted_payload_budget() -> TestResult {
    let mut pool = SharedImages::default();
    pool.intern(0, resource("first", "image/png", b"a"), 0)?;
    // Exercise the admission boundary without allocating 128 MiB in a unit test.
    pool.bytes = MAX_IMAGE_BYTES;
    assert_eq!(pool.intern(1, resource("alias", "image/png", b"a"), 0)?, "first");
    assert!(pool.intern(2, resource("new", "image/png", b"b"), 0).is_err());
    assert_eq!(pool.resources.len(), 1);
    assert_eq!(pool.bytes, MAX_IMAGE_BYTES);
    Ok(())
}

#[test]
fn resource_limit_counts_unique_images_across_the_entire_pool() -> TestResult {
    let mut pool = SharedImages::default();
    for index in 0..MAX_RESOURCES {
        pool.intern(index, resource(&format!("image-{index}"), "image/png", &index.to_le_bytes()), index as u64)?;
    }
    assert_eq!(pool.intern(MAX_RESOURCES, resource("alias", "image/png", &0usize.to_le_bytes()), 0)?, "image-0");
    assert!(pool.intern(MAX_RESOURCES, resource("overflow", "image/png", &MAX_RESOURCES.to_le_bytes()), MAX_RESOURCES as u64).is_err());
    assert_eq!(pool.resources.len(), MAX_RESOURCES);
    Ok(())
}

#[test]
fn rewrites_only_real_source_attributes_not_text_titles_or_links() {
    let aliases = BTreeMap::from([("old.png".into(), "first.png".into())]);
    let original = "<p>old.png &lt;img src=\"old.png\"/&gt;</p>\n<a href='old.png'>keep</a><span title=\"<img src='old.png'/>\">x</span><img hidden data-src='old.png' alt='中文 > old.png' src = \"old.png\"/><img src='old.png'/><img src='unknown.png'/><imgish src='old.png'/>";
    let result = rewrite_sources(original, &aliases);
    let expected = original.replace("src = \"old.png\"", "src = \"first.png\"")
        .replace("/><img src='old.png'/>", "/><img src='first.png'/>");
    assert_eq!(result, expected);
    assert_eq!(rewrite_sources(&result, &aliases), result);
    assert_eq!(rewrite_sources("head <img src='unterminated", &aliases), "head <img src='unterminated");
}

fn svg(width: u8) -> Vec<u8> {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"10\"><rect width=\"{width}\" height=\"10\"/></svg>").into_bytes()
}

#[test]
fn book_exports_one_payload_and_manifest_item_for_all_source_aliases() -> TestResult {
    let book = build_book(&[
        BookInput { path: "a/one.md".into(), source: "# One\n\n![one](logo.svg)".into() },
        BookInput { path: "b/two.md".into(), source: "# Two\n\n![two](logo.svg)".into() },
        BookInput { path: "three.md".into(), source: "# Three\n\n![three](different.svg)".into() },
    ])?;
    let original = book.chapters[1].doc.clone();
    let opts = HtmlOptions {
        image_assets: vec![
            PdfImageAsset { destination: "a/logo.svg".into(), bytes: svg(10) },
            PdfImageAsset { destination: "b/logo.svg".into(), bytes: svg(10) },
            PdfImageAsset { destination: "different.svg".into(), bytes: svg(20) },
        ],
        ..HtmlOptions::default()
    };
    let prepared = prepare_book(&book, &opts)?;
    assert_eq!(prepared.chapters.iter().map(|c| c.content.resources.len()).collect::<Vec<_>>(), [1, 0, 1]);
    assert!(prepared.chapters[1].xhtml.contains("assets/chapter-1-image-1.svg"));
    assert!(!prepared.chapters[1].xhtml.contains("assets/chapter-2-image-1.svg"));
    assert_eq!(prepared.opf.matches("media-type=\"image/svg+xml\"").count(), 2);
    assert_eq!(prepared.opf.matches("properties=\"svg\"").count(), 3);
    assert_eq!(book.chapters[1].doc, original);
    assert_eq!(render_book_epub(&book, &opts)?, render_book_epub(&book, &opts)?);
    Ok(())
}

#[test]
fn changing_a_shared_image_changes_the_publication_identity() -> TestResult {
    let book = build_book(&[
        BookInput { path: "one.md".into(), source: "![one](logo.svg)".into() },
        BookInput { path: "two.md".into(), source: "![two](logo.svg)".into() },
    ])?;
    let mut opts = HtmlOptions {
        image_assets: vec![PdfImageAsset { destination: "logo.svg".into(), bytes: svg(10) }],
        ..HtmlOptions::default()
    };
    let first = prepare_book(&book, &opts)?;
    opts.image_assets[0].bytes = svg(20);
    let second = prepare_book(&book, &opts)?;
    assert_ne!(first.opf, second.opf);
    assert_eq!(second.chapters[0].content.resources.len(), 1);
    assert!(second.chapters[1].content.resources.is_empty());
    Ok(())
}

#[test]
fn archive_contains_each_shared_resource_path_only_once() -> TestResult {
    let inputs: Vec<_> = (0..6).map(|index| BookInput {
        path: format!("chapter-{index}.md"),
        source: "# Illustration\n\n![logo](logo.svg)".into(),
    }).collect();
    let book = build_book(&inputs)?;
    let opts = HtmlOptions {
        image_assets: vec![PdfImageAsset { destination: "logo.svg".into(), bytes: svg(10) }],
        ..HtmlOptions::default()
    };
    let bytes = render_book_epub(&book, &opts)?;
    let mut offset = 0usize;
    let mut names = Vec::new();
    while bytes.get(offset..offset + 4) == Some(b"PK\x03\x04".as_slice()) {
        let header = bytes.get(offset..offset + 30).ok_or("truncated ZIP header")?;
        let compressed = u32::from_le_bytes(header[18..22].try_into()?) as usize;
        let name_len = usize::from(u16::from_le_bytes(header[26..28].try_into()?));
        let extra_len = usize::from(u16::from_le_bytes(header[28..30].try_into()?));
        let name_start = offset + 30;
        let name_end = name_start + name_len;
        names.push(std::str::from_utf8(bytes.get(name_start..name_end).ok_or("missing ZIP name")?)?);
        offset = name_end + extra_len + compressed;
    }
    assert_eq!(bytes.get(offset..offset + 4), Some(b"PK\x01\x02".as_slice()));
    assert_eq!(names.first().copied(), Some("mimetype"));
    let images: Vec<_> = names.iter().copied().filter(|name| name.starts_with("OEBPS/assets/")).collect();
    assert_eq!(images, ["OEBPS/assets/chapter-1-image-1.svg"]);
    assert_eq!(names.iter().filter(|name| name.ends_with(".xhtml")).count(), 7);
    Ok(())
}
