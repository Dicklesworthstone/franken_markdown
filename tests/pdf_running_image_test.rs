//! Decorative logos in PDF running bands use host-owned assets, preserve
//! ordinary body layout, and never enter the tagged reading order or links.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::verification_text_layer;
use franken_markdown::{
    Block, Document, PdfEmitOptions, PdfImageAsset, PdfOptions, PdfRunningImage,
    PdfRunningImagePosition, RenderError, parse_markdown, render_pdf, render_pdf_document,
    render_pdf_document_emitted,
};

const HEADER: &str = "/Artifact <</Type /Pagination /Subtype /Header>> BDC\n";
const FOOTER: &str = "/Artifact <</Type /Pagination /Subtype /Footer>> BDC\n";

fn base() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

fn image(dest: &str, position: PdfRunningImagePosition) -> PdfRunningImage {
    PdfRunningImage {
        dest: dest.into(),
        position,
        height_pt: Some(16),
    }
}

fn options(dest: &str, bytes: Vec<u8>) -> PdfOptions {
    let mut opts = base();
    opts.image_assets.push(PdfImageAsset::new(dest, bytes));
    opts.running.header.image = Some(image(dest, PdfRunningImagePosition::Left));
    opts.running.footer.image = Some(image(dest, PdfRunningImagePosition::Right));
    opts
}

fn three_pages() -> Document {
    let mut blocks = Vec::new();
    for page in 1..=3 {
        if page > 1 {
            blocks.push(Block::PageBreak);
        }
        blocks.extend(
            parse_markdown(&format!(
                "# Section {page}\n\nBody paragraph {page} keeps its original position.\n"
            ))
            .blocks,
        );
    }
    Document { blocks }
}

/// A valid RGB PNG with a 4:1 aspect ratio, assembled without test dependencies.
fn png() -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        let mut crc = 0xffff_ffff_u32;
        for &byte in kind.iter().chain(data) {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 == 0 { 0 } else { 0xedb8_8320 };
            }
        }
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&(!crc).to_be_bytes());
    }
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&8_u32.to_be_bytes());
    header.extend_from_slice(&2_u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut bytes, b"IHDR", &header);
    let row: Vec<u8> = std::iter::once(0)
        .chain([0x14, 0x78, 0xb8].into_iter().cycle().take(24))
        .collect();
    chunk(
        &mut bytes,
        b"IDAT",
        &franken_markdown::compress::zlib_compress(&row.repeat(2)),
    );
    chunk(&mut bytes, b"IEND", &[]);
    bytes
}

/// The real 3x2 progressive JPEG from interactive_import_browser_check.py.
/// Keep its entropy-coded scans, so a PDF reader can also render this fixture.
fn jpeg() -> Vec<u8> {
    let hex = concat!(
        "ffd8ffe000104a46494600010100000100010000ffdb00430008060607060508",
        "0707070909080a0c140d0c0b0b0c1912130f141d1a1f1e1d1a1c1c20242e2720",
        "222c231c1c2837292c30313434341f27393d38323c2e333432ffdb0043010909",
        "090c0b0c180d0d1832211c213232323232323232323232323232323232323232",
        "323232323232323232323232323232323232323232323232323232323232ffc2",
        "0011080002000303012200021101031101ffc400150001010000000000000000",
        "0000000000000005ffc40014010100000000000000000000000000000005ffda",
        "000c030100021003100000018e143fffc4001410010000000000000000000000",
        "0000000000ffda00080101000105027fffc40014110100000000000000000000",
        "000000000000ffda0008010301013f017fffc400141101000000000000000000",
        "00000000000000ffda0008010201013f017fffc4001410010000000000000000",
        "0000000000000000ffda0008010100063f027fffc40014100100000000000000",
        "000000000000000000ffda0008010100013f217fffda000c0301000200030000",
        "001003ffc40014110100000000000000000000000000000000ffda0008010301",
        "013f107fffc40014110100000000000000000000000000000000ffda00080102",
        "01013f107fffc40014100100000000000000000000000000000000ffda000801",
        "0100013f107fffd9",
    );
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn svg() -> Vec<u8> {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"96\" height=\"24\">\
         <title>Decorative logo</title>\
         <a href=\"https://example.com/logo-details\">\
         <rect width=\"96\" height=\"24\" fill=\"#1478b8\" opacity=\"0.5\"/>\
         <text x=\"4\" y=\"17\" font-size=\"14\" fill=\"white\">LogoZΩ</text>\
         <image x=\"72\" y=\"8\" width=\"20\" height=\"5\" \
         href=\"data:image/png;base64,{}\"/>\
         </a></svg>",
        franken_markdown::html::base64_encode(&png()),
    )
    .into_bytes()
}

/// Follow actual /Contents references and their declared lengths. Both
/// compressed and uncompressed streams occur in production output.
fn page_content(pdf: &[u8]) -> Vec<String> {
    let mut pages = Vec::new();
    for reference in String::from_utf8_lossy(pdf).split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let header = format!("\n{number} 0 obj\n");
        let start = pdf
            .windows(header.len())
            .position(|bytes| bytes == header.as_bytes())
            .unwrap()
            + header.len();
        let object = &pdf[start..];
        let stream = object
            .windows(b"\nstream\n".len())
            .position(|bytes| bytes == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..stream]).unwrap();
        let length: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let bytes = &object[stream + 8..stream + 8 + length];
        let decoded;
        let bytes = if dictionary.contains("/Filter /FlateDecode") {
            decoded = franken_markdown::zlib_decompress(bytes, 16 << 20).unwrap();
            decoded.as_slice()
        } else {
            assert!(!dictionary.contains("/Filter"));
            bytes
        };
        pages.push(std::str::from_utf8(bytes).unwrap().to_owned());
    }
    assert!(!pages.is_empty());
    pages
}

fn band<'a>(content: &'a str, marker: &str) -> &'a str {
    let after = content.split_once(marker).expect("pagination artifact").1;
    after.split_once("\nEMC\n").expect("artifact ends").0
}

/// Width, height, left and bottom from actual raster drawing transforms.
fn raster_boxes(content: &str) -> Vec<[f32; 4]> {
    let tokens: Vec<_> = content.split_whitespace().collect();
    tokens
        .windows(9)
        .filter(|window| window[6] == "cm" && window[7].starts_with("/Im") && window[8] == "Do")
        .map(|window| {
            assert_eq!([window[1], window[2]], ["0", "0"]);
            [0, 3, 4, 5].map(|index| window[index].parse().unwrap())
        })
        .collect()
}

#[test]
fn png_and_jpeg_logos_draw_on_each_page_and_share_one_raster_resource() {
    let doc = three_pages();
    for (dest, bytes, filter) in [
        ("logo.png", png(), "/FlateDecode"),
        ("logo.jpg", jpeg(), "/DCTDecode"),
    ] {
        let opts = options(dest, bytes);
        assert!(!opts.running.is_empty());
        let pdf = render_pdf_document(&doc, &opts).unwrap();
        let raw = String::from_utf8_lossy(&pdf);
        assert_eq!(raw.matches("/Subtype /Image").count(), 1);
        assert!(raw.contains(filter));
        assert!(!raw.contains("/S /Figure"));
        assert!(!raw.contains("/Subtype /Link"));
        let pages = page_content(&pdf);
        assert_eq!(pages.len(), 3);
        for page in &pages {
            assert_eq!(page.matches("/Im1 Do").count(), 2);
            for marker in [HEADER, FOOTER] {
                let content = band(page, marker);
                assert_eq!(content.matches("/Im1 Do").count(), 1);
                assert!(!content.contains("/MCID"));
                assert!(content.contains(" re W n"), "logo is clipped to its box");
                assert_eq!(raster_boxes(content).len(), 1);
            }
            assert_eq!(
                page.matches(" BDC").count() + page.matches(" BMC").count(),
                page.lines().filter(|line| *line == "EMC").count(),
            );
        }
        assert_eq!(pdf, render_pdf_document(&doc, &opts).unwrap());
        assert_eq!(
            pdf,
            render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap(),
        );
    }
}

#[test]
fn svg_logo_keeps_vector_paint_text_and_nested_resources_without_creating_links() {
    let opts = options("logo.svg", svg());
    let doc = parse_markdown("# Memo\n\n[Body link](https://example.com/body).\n");
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/Subtype /Image").count(), 1, "nested PNG only");
    assert!(
        raw.contains("/ExtGState"),
        "SVG opacity resource is embedded"
    );
    assert!(
        raw.contains("> <03A9>"),
        "logo-only Omega enters the font subset"
    );
    assert!(raw.contains("/URI (https://example.com/body)"));
    assert!(!raw.contains("https://example.com/logo-details"));
    assert_eq!(raw.matches("/Subtype /Link").count(), 1);
    assert!(!raw.contains("/S /Figure"));
    for marker in [HEADER, FOOTER] {
        let pages = page_content(&pdf);
        let content = band(&pages[0], marker);
        assert!(
            content.contains("0.078 0.471 0.722 rg"),
            "SVG vector fill: {content}"
        );
        assert!(content.contains(" gs"), "SVG opacity is applied");
        assert!(
            content.contains(" Tf"),
            "SVG text draws with its subset font"
        );
        assert!(content.contains("/Im1 Do"), "nested PNG resource is drawn");
        assert!(!content.contains("/MCID"));
        assert!(!content.contains("/Figure"));
    }
    assert_eq!(pdf, render_pdf_document(&doc, &opts).unwrap());
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap(),
    );
}

#[test]
fn logos_preserve_body_geometry_tagged_reading_order_and_body_drawing() {
    let doc = three_pages();
    let plain = render_pdf_document(&doc, &base()).unwrap();
    let plain_layer = verification_text_layer(&doc, &base()).unwrap();
    let plain_pages = page_content(&plain);
    let plain_raw = String::from_utf8_lossy(&plain);
    for (dest, bytes) in [("logo.png", png()), ("logo.svg", svg())] {
        let opts = options(dest, bytes);
        let pdf = render_pdf_document(&doc, &opts).unwrap();
        let raw = String::from_utf8_lossy(&pdf);
        assert_eq!(verification_text_layer(&doc, &opts).unwrap(), plain_layer);
        for token in ["/StructElem", "/S /H1", "/S /P ", "/MCID"] {
            assert_eq!(raw.matches(token).count(), plain_raw.matches(token).count());
        }
        let pages = page_content(&pdf);
        assert_eq!(pages.len(), plain_pages.len());
        if dest.ends_with(".png") {
            for (decorated, body) in pages.iter().zip(&plain_pages) {
                let body_before_logo = decorated.split_once(HEADER).unwrap().0;
                assert_eq!(
                    body_before_logo, body,
                    "raster chrome leaves body operators unchanged"
                );
            }
        }
    }
}

#[test]
fn a_raster_used_in_body_and_both_bands_is_embedded_once() {
    let opts = options("logo.png", png());
    let pdf = render_pdf("![Visible body figure](logo.png)", &opts).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/Subtype /Image").count(), 1);
    assert_eq!(raw.matches("/S /Figure").count(), 1);
    assert!(raw.contains("/Alt (Visible body figure)"));
    let pages = page_content(&pdf);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].matches("/Im1 Do").count(), 3);
}

#[test]
fn skipping_the_first_page_omits_paint_and_unneeded_one_page_resources() {
    let doc = three_pages();
    let mut opts = options("logo.png", png());
    opts.running.skip_first_page = true;
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    let pages = page_content(&pdf);
    assert_eq!(pages.len(), 3);
    assert!(!pages[0].contains("/Pagination"));
    assert!(!pages[0].contains(" Do"));
    for page in &pages[1..] {
        assert_eq!(page.matches("/Im1 Do").count(), 2);
    }
    let source = "One ordinary page.";
    let plain = render_pdf(source, &base()).unwrap();
    for (dest, bytes) in [
        ("logo.png", png()),
        ("logo.jpg", jpeg()),
        ("logo.svg", svg()),
    ] {
        let mut opts = options(dest, bytes);
        opts.running.skip_first_page = true;
        assert_eq!(
            render_pdf(source, &opts).unwrap(),
            plain,
            "unused {dest} adds no image/font/opacity resources or artifact bytes",
        );
    }
}

#[test]
fn requested_logo_sizes_fit_the_margin_and_preserve_aspect_and_alignment() {
    for height in [None, Some(16), Some(u16::MAX)] {
        let mut opts = options("logo.png", png());
        opts.running.header.image.as_mut().unwrap().height_pt = height;
        opts.running.footer.image.as_mut().unwrap().height_pt = height;
        for band in [&mut opts.running.header, &mut opts.running.footer] {
            band.left = Some("Long left title with many words ".repeat(10));
            band.center = Some("Centered publication title ".repeat(10));
            band.right = Some("Long right title with many words ".repeat(10));
            band.rule = true;
        }
        let pdf = render_pdf("Body stays clear of the logo.", &opts).unwrap();
        let pages = page_content(&pdf);
        for marker in [HEADER, FOOTER] {
            let boxes = raster_boxes(band(&pages[0], marker));
            assert_eq!(boxes.len(), 1);
            let [width, drawn_height, x, y] = boxes[0];
            assert!(width > 0.0 && drawn_height > 0.0);
            assert!((width / drawn_height - 4.0).abs() < 0.001);
            assert!(x >= 72.0 - 0.01 && x + width <= 540.0 + 0.01);
            if marker == HEADER {
                assert!((x - 72.0).abs() < 0.01);
                assert!(y >= 720.0 && y + drawn_height <= 792.0);
            } else {
                assert!((x + width - 540.0).abs() < 0.01);
                assert!(y >= 0.0 && y + drawn_height <= 72.0);
            }
            match height {
                None => assert!((drawn_height - 6.75).abs() < 0.01),
                Some(16) => assert!((drawn_height - 16.0).abs() < 0.01),
                Some(height) => {
                    assert!(
                        drawn_height < f32::from(height),
                        "oversized logo is reduced"
                    );
                }
            }
        }
    }
}

fn assert_error(opts: &PdfOptions, code: &str) {
    let error = render_pdf("Body.", opts).unwrap_err();
    assert!(matches!(error, RenderError::InvalidInput(_)));
    assert!(error.to_string().contains(code), "{error}");
}

#[test]
fn requested_logos_fail_explicitly_for_invalid_missing_or_unsupported_assets() {
    let mut missing = base();
    missing.running.header.image = Some(image("missing.png", PdfRunningImagePosition::Left));
    assert_error(&missing, "pdf_running_image_missing");
    for dest in [" ".to_string(), "x".repeat(4097)] {
        let mut opts = base();
        opts.running.header.image = Some(image(&dest, PdfRunningImagePosition::Left));
        assert_error(&opts, "pdf_running_image_invalid");
    }
    let mut zero = options("logo.png", png());
    zero.running.header.image.as_mut().unwrap().height_pt = Some(0);
    assert_error(&zero, "pdf_running_image_invalid");
    for bytes in [b"not an image".to_vec(), png()[..12].to_vec()] {
        assert_error(
            &options("broken.png", bytes),
            "pdf_running_image_unsupported",
        );
    }
}

#[test]
fn normalized_duplicate_assets_must_agree_and_matching_copies_do_not_change_bytes() {
    let source = "Body.";
    let opts = options("logo.png", png());
    let baseline = render_pdf(source, &opts).unwrap();
    let mut identical = opts.clone();
    identical
        .image_assets
        .push(PdfImageAsset::new(" \tlogo.png\u{2003}", png()));
    identical.running.header.image.as_mut().unwrap().dest = "\u{2003}logo.png\n".into();
    assert_eq!(render_pdf(source, &identical).unwrap(), baseline);
    identical.image_assets[1].bytes = jpeg();
    assert_error(&identical, "pdf_running_image_conflict");
}
