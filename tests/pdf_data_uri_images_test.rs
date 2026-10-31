//! Source-carried image bytes survive PDF export without host-side mappings.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfEmitOptions, PdfImageAsset, PdfOptions, PdfPageEmission, RenderWarning,
    parse_markdown, render_html, render_pdf, render_pdf_document_emitted, render_warnings,
};
use std::fmt::Write as _;

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAE0lEQVR4nGP4z8DwHwyBNIhgAAA/0gX7yxemzgAAAABJRU5ErkJggg==";
const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"16\"><rect width=\"32\" height=\"16\" fill=\"#ff0000\"/></svg>";

fn fixture_base64(source: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut value = 0u32;
    let mut bits = 0;
    for byte in source.trim().bytes().take_while(|&byte| byte != b'=') {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("invalid fixture encoding"),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((value >> bits) as u8);
        }
    }
    bytes
}

fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let jpeg: String = include_str!("fixtures/jpeg-quadrants.hex")
        .split_whitespace()
        .collect();
    vec![
        ("image/png", fixture_base64(PNG)),
        (
            "image/jpeg",
            jpeg.as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect(),
        ),
        ("image/svg+xml", SVG.to_vec()),
        (
            "image/gif",
            fixture_base64(include_str!("fixtures/gif/animated-colors.base64")),
        ),
    ]
}

fn uri(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        franken_markdown::html::base64_encode(bytes)
    )
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        write!(out, "%{byte:02X}").unwrap();
    }
    out
}

// Read actual PDF object dictionaries and streams by their declared byte
// lengths. Binary image samples may themselves contain stream delimiters.
fn stream(object: &[u8]) -> (&str, &[u8]) {
    let start = object
        .windows(8)
        .position(|bytes| bytes == b"\nstream\n")
        .unwrap();
    let header = std::str::from_utf8(&object[..start]).unwrap();
    let length: usize = header
        .split_once("/Length ")
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    (header, &object[start + 8..start + 8 + length])
}

fn image_streams(pdf: &[u8]) -> Vec<(&str, &[u8])> {
    let marker = b"<< /Type /XObject /Subtype /Image ";
    let mut rest = pdf;
    let mut images = Vec::new();
    while let Some(start) = rest.windows(marker.len()).position(|part| part == marker) {
        rest = &rest[start..];
        let (header, samples) = stream(rest);
        images.push((header, samples));
        let consumed = header.len() + 8 + samples.len();
        rest = &rest[consumed..];
    }
    images
}

fn page_content(pdf: &[u8]) -> String {
    let mut result = String::new();
    for reference in String::from_utf8_lossy(pdf).split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let marker = format!("\n{number} 0 obj\n");
        let start = pdf
            .windows(marker.len())
            .position(|bytes| bytes == marker.as_bytes())
            .unwrap()
            + marker.len();
        let (header, bytes) = stream(&pdf[start..]);
        let decoded;
        let bytes = if header.contains("/Filter /FlateDecode") {
            decoded = franken_markdown::zlib_decompress(bytes, 1 << 20).unwrap();
            decoded.as_slice()
        } else {
            assert!(!header.contains("/Filter"));
            bytes
        };
        result.push_str(std::str::from_utf8(bytes).unwrap());
    }
    assert!(!result.is_empty());
    result
}

fn assert_embedded_samples(pdf: &[u8], mime: &str, original: &[u8]) {
    let images = image_streams(pdf);
    match mime {
        "image/png" | "image/gif" => {
            assert_eq!(images.len(), 2, "RGB image and its transparency mask");
            let (rgb, samples) = images
                .iter()
                .find(|(header, _)| header.contains("/DeviceRGB"))
                .unwrap();
            assert!(rgb.contains("/Width 2 /Height 2"));
            assert!(rgb.contains("/SMask "));
            assert!(rgb.contains("/Filter /FlateDecode"));
            assert_eq!(
                franken_markdown::zlib_decompress(samples, 1024).unwrap(),
                [255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]
            );
            let (_, alpha) = images
                .iter()
                .find(|(header, _)| header.contains("/DeviceGray"))
                .unwrap();
            assert_eq!(
                franken_markdown::zlib_decompress(alpha, 1024).unwrap(),
                [255, 255, 255, 0]
            );
        }
        "image/jpeg" => {
            assert_eq!(images.len(), 1);
            let (header, samples) = images[0];
            assert!(header.contains("/Filter /DCTDecode"));
            assert!(header.contains("/Width 32 /Height 16"));
            assert_eq!(samples, original, "JPEG samples stay lossless");
        }
        "image/svg+xml" => {
            assert!(images.is_empty(), "SVG stays vector drawing");
            assert!(page_content(pdf).contains("1.000 0.000 0.000 rg 0 0 32 16 re f"));
        }
        _ => panic!("unexpected fixture MIME"),
    }
}

#[test]
fn source_data_images_keep_real_samples_transparency_vectors_alt_and_links() {
    for (mime, bytes) in fixtures() {
        let destination = uri(mime, &bytes);
        let source = format!("[![Embedded figure]({destination})](https://example.com/figure)");
        let options = PdfOptions::default();
        let pdf = render_pdf(&source, &options).unwrap();
        assert_embedded_samples(&pdf, mime, &bytes);
        let raw = String::from_utf8_lossy(&pdf);
        assert!(raw.contains("/Alt (Embedded figure)"));
        assert!(raw.contains("/URI (https://example.com/figure)"));
        assert!(render_warnings(&parse_markdown(&source), &options).is_empty());
        let mapped = PdfOptions {
            image_assets: vec![PdfImageAsset::new(destination, bytes)],
            ..options
        };
        assert_eq!(pdf, render_pdf(&source, &mapped).unwrap());
    }
}

#[test]
fn inline_table_and_raw_html_images_share_one_decoded_resource() {
    for (mime, bytes) in fixtures() {
        let destination = uri(mime, &bytes);
        let source = format!(
            "Before ![Inline image]({destination}) after.\n\n\
             | Kind | Figure |\n| --- | --- |\n\
             | Table | <img src=\"{destination}\" alt=\"Cell image\"> |\n\n\
             <img src=\"{destination}\" alt=\"Raw figure\">\n"
        );
        let options = PdfOptions::default();
        let pdf = render_pdf(&source, &options).unwrap();
        assert_embedded_samples(&pdf, mime, &bytes);
        let raw = String::from_utf8_lossy(&pdf);
        for alt in ["Inline image", "Cell image", "Raw figure"] {
            assert!(raw.contains(&format!("/Alt ({alt})")), "{mime}: {alt}");
        }
        assert!(render_warnings(&parse_markdown(&source), &options).is_empty());
        if mime != "image/svg+xml" {
            assert_eq!(page_content(&pdf).matches("/Im1 Do").count(), 3);
        }
    }
}

#[test]
fn percent_encoded_bytes_and_base64_have_canonical_output_for_every_format() {
    for (mime, bytes) in fixtures() {
        let canonical = uri(mime, &bytes);
        let render = |destination: &str| {
            render_pdf(
                &format!("![Encoded image]({destination})"),
                &PdfOptions::default(),
            )
            .unwrap()
        };
        let expected = render(&canonical);
        let encoded = franken_markdown::html::base64_encode(&bytes);
        for alternate in [
            format!("data:{mime},{}", percent_encode(&bytes)),
            format!("data:{mime};base64,{}", percent_encode(encoded.as_bytes())),
            format!(
                "DATA:{};CHARSET=UTF-8;BASE64,{encoded}",
                mime.to_ascii_uppercase()
            ),
        ] {
            assert_eq!(expected, render(&alternate), "{mime}");
        }
    }
}

#[test]
fn first_host_override_wins_even_across_mime_types_or_when_malformed() {
    // A large valid inline SVG makes accidental fallback visible and makes a
    // diagnostic that echoes the original destination impractically large.
    let large_svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"16\">\
         <!--{}--><rect width=\"32\" height=\"16\" fill=\"red\"/></svg>",
        "source-payload-".repeat(1024)
    );
    let destination = uri("image/svg+xml", large_svg.as_bytes());
    let source = format!("![Fallback retained]({destination})");
    let png = fixture_base64(PNG);
    let valid_override = PdfOptions {
        image_assets: vec![
            PdfImageAsset::new(format!("  {destination}  "), png.clone()),
            PdfImageAsset::new(destination.clone(), Vec::new()),
        ],
        ..PdfOptions::default()
    };
    let pdf = render_pdf(&source, &valid_override).unwrap();
    assert_embedded_samples(&pdf, "image/png", &png);
    assert!(render_warnings(&parse_markdown(&source), &valid_override).is_empty());

    let invalid_override = PdfOptions {
        image_assets: vec![
            PdfImageAsset::new(destination.clone(), b"invalid image bytes".to_vec()),
            PdfImageAsset::new(destination, png),
        ],
        ..PdfOptions::default()
    };
    assert_eq!(
        render_pdf(&source, &invalid_override).unwrap(),
        render_pdf("Fallback retained", &PdfOptions::default()).unwrap(),
        "neither the valid inline SVG nor a later mapping may replace the first override"
    );
    let warnings = render_warnings(&parse_markdown(&source), &invalid_override);
    assert_eq!(warnings.len(), 1);
    let RenderWarning::UnsupportedImage(destination) = &warnings[0] else {
        panic!("mapped data URI must report an unsupported image");
    };
    assert!(destination.starts_with("data:image/svg+xml ("));
    assert!(destination.len() < 100);
    assert!(warnings[0].message().len() < 200);
    let report =
        franken_markdown::verify::verify_pdf(&parse_markdown(&source), &invalid_override).unwrap();
    assert!(
        report
            .findings
            .iter()
            .any(|finding| { finding.code == "unsupported_image" && finding.detail.len() < 200 })
    );
}

#[test]
fn malformed_mismatched_and_oversized_headers_keep_alt_and_bounded_diagnostics() {
    let mut invalid = vec![
        "data:image/png;base64,=AAA".to_owned(),
        "data:image/png;base64,AB==".to_owned(),
        "data:image/png;base64,AA==AAAA".to_owned(),
        "data:image/png;base64,%GG".to_owned(),
        "data:image/png,%".to_owned(),
        "data:image/png;base64;base64,AAAA".to_owned(),
        "data:image/png;label=🦀,AAAA".to_owned(),
        "data:imagé/png;base64,AAAA".to_owned(),
        "data:text/html;base64,PHN2Zy8+".to_owned(),
        format!("data:image/png-bogus;base64,{PNG}"),
        format!("data:image/png;{},AAAA", "x".repeat(256)),
        format!("data:image/png;base64,{}!", "A".repeat(12_000)),
    ];
    let fixtures = fixtures();
    for (index, (_, bytes)) in fixtures.iter().enumerate() {
        invalid.push(uri(fixtures[(index + 1) % fixtures.len()].0, bytes));
    }
    let mut source = String::new();
    let mut fallback = String::new();
    for (index, destination) in invalid.iter().enumerate() {
        writeln!(source, "![Fallback {index}]({destination})\n").unwrap();
        writeln!(fallback, "Fallback {index}\n").unwrap();
    }
    let options = PdfOptions::default();
    let doc = parse_markdown(&source);
    let warnings = render_warnings(&doc, &options);
    assert_eq!(warnings.len(), invalid.len());
    assert!(warnings.iter().all(|warning| {
        matches!(warning, RenderWarning::UnsupportedImage(label) if label.len() < 100)
            && warning.message().len() < 200
    }));
    assert_eq!(
        render_pdf(&source, &options).unwrap(),
        render_pdf(&fallback, &options).unwrap(),
        "every rejected image keeps its complete selectable alt text"
    );
    let browser = franken_markdown::wasm::render_pdf(
        &source,
        &franken_markdown::wasm::WasmRenderOptions::default(),
    )
    .unwrap();
    assert_eq!(browser.diagnostics.len(), invalid.len());
    assert!(
        browser
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message.len() < 200)
    );
}

#[test]
fn data_images_are_deterministic_across_emitters_and_the_wasm_adapter() {
    let source: String = fixtures()
        .into_iter()
        .map(|(mime, bytes)| format!("![Embedded image]({})\n\n", uri(mime, &bytes)))
        .collect();
    let options = PdfOptions::default();
    let doc = parse_markdown(&source);
    let expected = render_pdf(&source, &options).unwrap();
    assert_eq!(expected, render_pdf(&source, &options).unwrap());
    for emission in [PdfPageEmission::Monolithic, PdfPageEmission::Chunked] {
        assert_eq!(
            expected,
            render_pdf_document_emitted(
                &doc,
                &options,
                PdfEmitOptions {
                    emission,
                    ..PdfEmitOptions::default()
                }
            )
            .unwrap()
        );
    }
    let browser = franken_markdown::wasm::render_pdf(
        &source,
        &franken_markdown::wasm::WasmRenderOptions::default(),
    )
    .unwrap();
    assert!(browser.diagnostics.is_empty(), "{:?}", browser.diagnostics);
    assert_eq!(expected, browser.bytes);
}

#[test]
fn ordinary_url_diagnostics_and_the_core_html_data_url_policy_are_unchanged() {
    let destination = uri("image/png", &fixture_base64(PNG));
    let html = render_html(
        &format!("![Embedded alt]({destination})"),
        &HtmlOptions::default(),
    )
    .unwrap();
    assert!(html.contains("<p>Embedded alt</p>"));
    assert!(!html.contains("<img"));

    let destination = format!("https://example.invalid/{}", "long-path/".repeat(64));
    let doc = parse_markdown(&format!("![Remote alt]({destination})"));
    assert_eq!(
        render_warnings(&doc, &PdfOptions::default()),
        [RenderWarning::UnresolvedImage(destination.clone())]
    );
    let mapped = PdfOptions {
        image_assets: vec![PdfImageAsset::new(destination.clone(), Vec::new())],
        ..PdfOptions::default()
    };
    assert_eq!(
        render_warnings(&doc, &mapped),
        [RenderWarning::UnsupportedImage(destination)]
    );
}
