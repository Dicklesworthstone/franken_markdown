//! Publication JPEGs keep their samples, colour interpretation and placement.
//!
//! The 96x32 fixtures contain six 16-pixel-wide patches: red, green, blue,
//! white, black and 50% K. CMYK values are (0,255,255,0), (255,0,255,0),
//! (255,255,0,0), (0,0,0,0), (0,0,0,255), (0,0,0,128). Pillow 12.3/libjpeg-turbo
//! 3.1.4.1 encoded Adobe0 CMYK at quality100/subsampling0, with and without
//! progressive=true. ImageMagick's `-colorspace CMYK -sampling-factor 1x1
//! -quality 100` encoded Adobe2 YCCK from the same RGB patches; `-interlace
//! Plane` produced its progressive variant. No source photograph is licensed
//! or copied. Independent Poppler raster checks exercise the resulting colours.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfASettings, PdfEmitOptions, PdfImageAsset, PdfOptions, PdfPageEmission,
    PdfRunningImage, PdfRunningImagePosition, RenderError, RenderWarning, parse_markdown,
    render_epub, render_html, render_pdf, render_pdf_document_emitted, render_pdf_pdfa,
    render_warnings,
};

fn unhex(hex: &str) -> Vec<u8> {
    let hex: String = hex.split_whitespace().collect();
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn fixtures() -> Vec<(&'static str, u8, Vec<u8>)> {
    [
        ("CMYK", 0, include_str!("fixtures/jpeg-cmyk-swatches.hex")),
        ("YCCK", 1, include_str!("fixtures/jpeg-ycck-swatches.hex")),
        (
            "progressive CMYK",
            0,
            include_str!("fixtures/jpeg-cmyk-progressive-swatches.hex"),
        ),
        (
            "progressive YCCK",
            1,
            include_str!("fixtures/jpeg-ycck-progressive-swatches.hex"),
        ),
    ]
    .into_iter()
    .map(|(name, transform, hex)| (name, transform, unhex(hex)))
    .collect()
}

fn options(bytes: Vec<u8>) -> PdfOptions {
    PdfOptions {
        image_assets: vec![PdfImageAsset::new("photo.jpg", bytes)],
        ..PdfOptions::default()
    }
}

fn uri(bytes: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        franken_markdown::html::base64_encode(bytes)
    )
}

fn stream(object: &[u8]) -> (&str, &[u8]) {
    let start = object.windows(8).position(|s| s == b"\nstream\n").unwrap();
    let dictionary = std::str::from_utf8(&object[..start]).unwrap();
    let length: usize = dictionary
        .split_once("/Length ")
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    (dictionary, &object[start + 8..start + 8 + length])
}

fn images(pdf: &[u8]) -> Vec<(&str, &[u8])> {
    let marker = b"<< /Type /XObject /Subtype /Image ";
    let mut rest = pdf;
    let mut images = Vec::new();
    while let Some(start) = rest.windows(marker.len()).position(|s| s == marker) {
        let (dictionary, samples) = stream(&rest[start..]);
        images.push((dictionary, samples));
        rest = &rest[start + dictionary.len() + 8 + samples.len()..];
    }
    images
}

fn content(pdf: &[u8]) -> String {
    let mut out = String::new();
    for reference in String::from_utf8_lossy(pdf).split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let marker = format!("\n{number} 0 obj\n");
        let start = pdf
            .windows(marker.len())
            .position(|bytes| bytes == marker.as_bytes())
            .unwrap()
            + marker.len();
        let (dictionary, payload) = stream(&pdf[start..]);
        let decoded = if dictionary.contains("/Filter /FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 20).unwrap()
        } else {
            assert!(!dictionary.contains("/Filter"));
            payload.to_vec()
        };
        out.push_str(std::str::from_utf8(&decoded).unwrap());
    }
    out
}

fn assert_jpeg(pdf: &[u8], bytes: &[u8], transform: u8) {
    let image = images(pdf);
    assert_eq!(image.len(), 1, "one retained raster resource");
    let (dictionary, payload) = image[0];
    assert!(dictionary.contains("/Width 96 /Height 32 /ColorSpace /DeviceCMYK"));
    assert!(dictionary.contains("/BitsPerComponent 8 /Filter /DCTDecode"));
    assert!(dictionary.contains("/Decode [1 0 1 0 1 0 1 0]"));
    assert!(dictionary.contains(&format!("/ColorTransform {transform}")));
    assert_eq!(payload, bytes, "encoded JPEG and metadata remain unchanged");
}

#[test]
fn adobe_cmyk_and_ycck_keep_baseline_and_progressive_photographs() {
    let source = "[![Print photograph](photo.jpg)](https://example.com/photograph)";
    for (name, transform, bytes) in fixtures() {
        let opts = options(bytes.clone());
        let pdf = render_pdf(source, &opts).unwrap();
        assert_jpeg(&pdf, &bytes, transform);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Alt (Print photograph)"), "{name}");
        assert!(text.contains("/URI (https://example.com/photograph)"));
        assert_eq!(content(&pdf).matches("/Im1 Do").count(), 1);
        assert!(render_warnings(&parse_markdown(source), &opts).is_empty());
    }
}

#[test]
fn inline_heading_table_and_raw_html_photos_share_the_correct_raster() {
    let source = "# Heading ![Heading image](photo.jpg)\n\n\
                  Before [![Inline image](photo.jpg)](https://example.com/inline) after.\n\n\
                  | Colour | Meaning |\n| --- | --- |\n\
                  | ![Cell image](photo.jpg) | Retained |\n\n\
                  <img src=\"photo.jpg\" alt=\"Raw figure\">\n";
    for (_, transform, bytes) in fixtures() {
        let opts = options(bytes.clone());
        let pdf = render_pdf(source, &opts).unwrap();
        assert_jpeg(&pdf, &bytes, transform);
        assert_eq!(content(&pdf).matches("/Im1 Do").count(), 4);
        let raw = String::from_utf8_lossy(&pdf);
        for alt in ["Heading image", "Inline image", "Cell image", "Raw figure"] {
            assert!(raw.contains(&format!("/Alt ({alt})")), "{alt}");
        }
        assert!(raw.contains("/URI (https://example.com/inline)"));
        assert!(render_warnings(&parse_markdown(source), &opts).is_empty());
    }
}

fn svg(bytes: &[u8]) -> Vec<u8> {
    let uri = uri(bytes);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"192\" height=\"64\">\
         <image width=\"96\" height=\"32\" href=\"{uri}\"/>\
         <image x=\"96\" y=\"32\" width=\"96\" height=\"32\" href=\"{uri}\"/></svg>"
    )
    .into_bytes()
}

#[test]
fn svg_and_running_figures_keep_the_same_cmyk_decode_and_original_stream() {
    for (_, transform, bytes) in fixtures() {
        let opts = PdfOptions {
            image_assets: vec![PdfImageAsset::new("photo.svg", svg(&bytes))],
            ..PdfOptions::default()
        };
        let pdf = render_pdf("![Print illustration](photo.svg)", &opts).unwrap();
        assert_jpeg(&pdf, &bytes, transform);
        assert_eq!(content(&pdf).matches("/Im1 Do").count(), 2);

        for (destination, asset) in [("photo.jpg", bytes.clone()), ("photo.svg", svg(&bytes))] {
            let mut opts = PdfOptions {
                image_assets: vec![PdfImageAsset::new(destination, asset)],
                ..PdfOptions::default()
            };
            opts.running.header.image = Some(PdfRunningImage {
                dest: destination.into(),
                position: PdfRunningImagePosition::Left,
                height_pt: Some(18),
            });
            let pdf = render_pdf("Running publication image", &opts).unwrap();
            assert_jpeg(&pdf, &bytes, transform);
            assert!(content(&pdf).contains("/Type /Pagination /Subtype /Header"));
        }
    }
}

fn with_exif(bytes: &[u8], orientation: u16) -> Vec<u8> {
    let mut exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
    exif.extend_from_slice(&orientation.to_le_bytes());
    exif.extend_from_slice(&[0; 6]);
    let mut app1 = vec![0xff, 0xe1];
    app1.extend_from_slice(&u16::try_from(exif.len() + 2).unwrap().to_be_bytes());
    app1.extend(exif);
    let mut out = bytes.to_vec();
    out.splice(2..2, app1);
    out
}

#[test]
fn all_exif_orientations_keep_the_cmyk_pixels_and_display_geometry() {
    let corner_orders = [
        [0, 1, 2, 3],
        [1, 0, 3, 2],
        [2, 3, 0, 1],
        [3, 2, 1, 0],
        [0, 3, 2, 1],
        [1, 2, 3, 0],
        [2, 1, 0, 3],
        [3, 0, 1, 2],
    ];
    for (_, transform, bytes) in fixtures().into_iter().take(2) {
        for orientation in 1..=8 {
            let bytes = with_exif(&bytes, orientation);
            let pdf = render_pdf(
                "![Oriented print photo](photo.jpg)",
                &options(bytes.clone()),
            )
            .unwrap();
            assert_jpeg(&pdf, &bytes, transform);
            let raw = String::from_utf8_lossy(&pdf);
            let bounds: Vec<f32> = raw
                .split_once("/BBox [")
                .unwrap()
                .1
                .split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            let [left, bottom, right, top]: [f32; 4] = bounds.try_into().unwrap();
            let (width, height) = if orientation <= 4 {
                (72.0, 24.0)
            } else {
                (24.0, 72.0)
            };
            assert!((right - left - width).abs() < 0.025);
            assert!((top - bottom - height).abs() < 0.025);
            let operators = content(&pdf);
            let tokens: Vec<_> = operators.split_whitespace().collect();
            let matrix = tokens
                .windows(9)
                .find(|w| w[6] == "cm" && w[7] == "/Im1" && w[8] == "Do")
                .unwrap();
            let [a, b, c, d, e, f]: [f32; 6] = std::array::from_fn(|i| matrix[i].parse().unwrap());
            let corners = [(left, top), (right, top), (right, bottom), (left, bottom)];
            for ((x, y), target) in [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)]
                .into_iter()
                .zip(corner_orders[usize::from(orientation - 1)])
            {
                assert!((a * x + c * y + e - corners[target].0).abs() < 0.025);
                assert!((b * x + d * y + f - corners[target].1).abs() < 0.025);
            }
        }
    }
}

fn marker(bytes: &[u8], kind: u8) -> usize {
    bytes
        .windows(2)
        .position(|pair| pair == [0xff, kind])
        .unwrap()
}

fn adobe_segment(bytes: &[u8]) -> std::ops::Range<usize> {
    let start = marker(bytes, 0xee);
    let length = usize::from(u16::from_be_bytes([bytes[start + 2], bytes[start + 3]]));
    start..start + 2 + length
}

#[test]
fn ambiguous_or_malformed_four_channel_jpegs_keep_alt_text_and_diagnostics() {
    let bytes = fixtures()[0].2.clone();
    let range = adobe_segment(&bytes);
    let mut invalid = Vec::new();
    let mut missing = bytes.clone();
    missing.drain(range.clone());
    invalid.push(missing);
    for transform in [1, 3, 255] {
        let mut wrong = bytes.clone();
        wrong[range.start + 15] = transform;
        invalid.push(wrong);
    }
    let mut conflict = bytes.clone();
    let mut other = bytes[range.clone()].to_vec();
    other[15] = 2;
    conflict.splice(2..2, other);
    invalid.push(conflict);
    let mut truncated_adobe = bytes.clone();
    truncated_adobe.splice(2..2, [0xff, 0xee, 0, 7, b'A', b'd', b'o', b'b', b'e']);
    invalid.push(truncated_adobe);
    let sof = marker(&bytes, 0xc0);
    for (index, value) in [
        (sof + 4, 12),
        (sof + 13, bytes[sof + 10]),
        (sof + 11, 0),
        (sof + 12, 4),
    ] {
        let mut malformed = bytes.clone();
        malformed[index] = value;
        invalid.push(malformed);
    }
    let sos = marker(&bytes, 0xda);
    for (index, value) in [
        (sos + 4, 0),
        (sos + 7, bytes[sos + 5]),
        (sos + 5, 250),
        (sos + 6, 0x40),
    ] {
        let mut malformed = bytes.clone();
        malformed[index] = value;
        invalid.push(malformed);
    }
    invalid.push(bytes[..sos + 3].to_vec());
    invalid.push(bytes[..bytes.len() - 1].to_vec());
    let expected = render_pdf("Missing colour photograph", &PdfOptions::default()).unwrap();
    for (index, bytes) in invalid.into_iter().enumerate() {
        let source = "![Missing colour photograph](photo.jpg)";
        let opts = options(bytes);
        assert_eq!(render_pdf(source, &opts).unwrap(), expected, "case {index}");
        assert!(
            matches!(
                render_warnings(&parse_markdown(source), &opts).as_slice(),
                [RenderWarning::UnsupportedImage(destination)] if destination == "photo.jpg"
            ),
            "case {index}"
        );
    }
}

#[test]
fn adobe_marker_order_and_identical_duplicates_do_not_change_sample_interpretation() {
    for (_, transform, bytes) in fixtures() {
        let range = adobe_segment(&bytes);
        let app14 = bytes[range.clone()].to_vec();
        let mut after_frame = bytes.clone();
        after_frame.drain(range);
        let sos = marker(&after_frame, 0xda);
        after_frame.splice(sos..sos, app14.clone());
        let mut duplicate = bytes.clone();
        duplicate.splice(2..2, app14);
        for modified in [after_frame, duplicate] {
            let pdf = render_pdf(
                "![Reordered metadata](photo.jpg)",
                &options(modified.clone()),
            )
            .unwrap();
            assert_jpeg(&pdf, &modified, transform);
        }
    }
}

#[test]
fn data_uris_and_host_assets_match_across_emitters_and_the_wasm_adapter() {
    for (_, transform, bytes) in fixtures() {
        let source = format!("Before ![Portable photograph]({}) after.", uri(&bytes));
        let opts = PdfOptions::default();
        let expected = render_pdf(&source, &opts).unwrap();
        assert_jpeg(&expected, &bytes, transform);
        for emission in [PdfPageEmission::Monolithic, PdfPageEmission::Chunked] {
            assert_eq!(
                expected,
                render_pdf_document_emitted(
                    &parse_markdown(&source),
                    &opts,
                    PdfEmitOptions {
                        emission,
                        ..PdfEmitOptions::default()
                    }
                )
                .unwrap()
            );
        }
        let mapped = PdfOptions {
            image_assets: vec![PdfImageAsset::new(uri(&bytes), bytes.clone())],
            ..opts
        };
        assert_eq!(expected, render_pdf(&source, &mapped).unwrap());
        let browser = franken_markdown::wasm::render_pdf(&source, &Default::default()).unwrap();
        assert!(browser.diagnostics.is_empty());
        assert_eq!(expected, browser.bytes);

        // The same source remains portable in the other backends. EPUB stores
        // JPEG resources without recompression, while HTML/SVG embed the URI.
        assert!(
            render_html(&source, &HtmlOptions::default())
                .unwrap()
                .contains(&uri(&bytes))
        );
        let epub = render_epub(&parse_markdown(&source), &HtmlOptions::default()).unwrap();
        assert!(epub.windows(bytes.len()).any(|slice| slice == bytes));
        let (poster, _, warnings) = franken_markdown::svg::render_svg_with_diagnostics(
            &parse_markdown(&source),
            &Default::default(),
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(String::from_utf8(poster).unwrap().contains(&uri(&bytes)));
    }
}

fn assert_archival_refusal(source: &str, opts: &PdfOptions) {
    for setting in [PdfASettings::a2b(), PdfASettings::a2b_strict()] {
        let error = render_pdf_pdfa(source, opts, setting).unwrap_err();
        assert!(matches!(error, RenderError::InvalidInput(ref message)
            if message.starts_with("pdf_a_cmyk_image:") && message.contains("convert the image to RGB")));
    }
}

#[test]
fn archival_export_refuses_used_uncalibrated_cmyk_without_dropping_content() {
    for (_, _, bytes) in fixtures().into_iter().take(2) {
        let opts = options(bytes.clone());
        for source in [
            "![Used](photo.jpg)",
            "Before ![Used](photo.jpg) after.",
            "| Heading |\n| --- |\n| ![Used](photo.jpg) |",
        ] {
            assert_archival_refusal(source, &opts);
        }
        let svg_opts = PdfOptions {
            image_assets: vec![PdfImageAsset::new("photo.svg", svg(&bytes))],
            ..PdfOptions::default()
        };
        assert_archival_refusal("![Nested colour](photo.svg)", &svg_opts);
        for (destination, mut running) in [("photo.jpg", opts.clone()), ("photo.svg", svg_opts)] {
            running.running.header.image = Some(PdfRunningImage {
                dest: destination.into(),
                position: PdfRunningImagePosition::Left,
                height_pt: Some(18),
            });
            assert_archival_refusal("Body with running image", &running);
            running.running.skip_first_page = true;
            let skipped = render_pdf_pdfa(
                "One page, chrome skipped",
                &running,
                PdfASettings::a2b_strict(),
            )
            .unwrap();
            assert!(images(&skipped).is_empty());
        }
        let unused = render_pdf_pdfa(
            "No figure uses the mapping",
            &opts,
            PdfASettings::a2b_strict(),
        )
        .unwrap();
        assert!(images(&unused).is_empty());
        assert!(
            String::from_utf8_lossy(&unused).contains("<pdfaid:conformance>B</pdfaid:conformance>")
        );
        // The ordinary PDF path retains the photograph instead of imposing the
        // archival profile's restriction on all publication output.
        assert_eq!(
            images(&render_pdf("![Used](photo.jpg)", &opts).unwrap()).len(),
            1
        );
    }
}
