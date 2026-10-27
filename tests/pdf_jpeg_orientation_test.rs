//! EXIF orientation must affect measured layout and actual JPEG painting.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PdfEmitOptions, PdfImageAsset, PdfOptions, PdfRunningImage, PdfRunningImagePosition,
    parse_markdown, render_pdf, render_pdf_document_emitted, render_warnings,
};

fn jpeg() -> Vec<u8> {
    let hex: String = include_str!("fixtures/jpeg-quadrants.hex")
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn with_exif(orientation: u16, little: bool) -> Vec<u8> {
    let short = |n: u16| {
        if little {
            n.to_le_bytes()
        } else {
            n.to_be_bytes()
        }
    };
    let long = |n: u32| {
        if little {
            n.to_le_bytes()
        } else {
            n.to_be_bytes()
        }
    };
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend_from_slice(if little { b"II" } else { b"MM" });
    exif.extend_from_slice(&short(42));
    exif.extend_from_slice(&long(8));
    exif.extend_from_slice(&short(1));
    exif.extend_from_slice(&short(0x0112));
    exif.extend_from_slice(&short(3));
    exif.extend_from_slice(&long(1));
    exif.extend_from_slice(&short(orientation));
    exif.extend_from_slice(&short(0));
    exif.extend_from_slice(&long(0));
    let mut bytes = jpeg();
    let mut app1 = vec![0xff, 0xe1];
    app1.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    app1.extend(exif);
    bytes.splice(2..2, app1);
    bytes
}

fn options(bytes: Vec<u8>) -> PdfOptions {
    PdfOptions {
        image_assets: vec![PdfImageAsset::new("photo.jpg", bytes)],
        ..PdfOptions::default()
    }
}

fn content(pdf: &[u8]) -> String {
    let text = String::from_utf8_lossy(pdf);
    let mut contents = String::new();
    for reference in text.split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let marker = format!("\n{number} 0 obj\n");
        let start = pdf
            .windows(marker.len())
            .position(|b| b == marker.as_bytes())
            .unwrap()
            + marker.len();
        let body = &pdf[start..];
        let stream = body.windows(8).position(|b| b == b"\nstream\n").unwrap();
        let dictionary = std::str::from_utf8(&body[..stream]).unwrap();
        let length: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let payload = &body[stream + 8..stream + 8 + length];
        let decoded = if dictionary.contains("/Filter /FlateDecode") {
            franken_markdown::zlib_decompress(payload, 16 * 1024 * 1024).unwrap()
        } else {
            assert!(!dictionary.contains("/Filter"));
            payload.to_vec()
        };
        contents.push_str(std::str::from_utf8(&decoded).unwrap());
    }
    assert!(!contents.is_empty());
    contents
}

fn image_matrices(content: &str) -> Vec<[f32; 6]> {
    let tokens: Vec<_> = content.split_whitespace().collect();
    tokens
        .windows(9)
        .filter_map(|window| {
            if window[6] == "cm" && window[7].starts_with("/Im") && window[8] == "Do" {
                Some(std::array::from_fn(|i| window[i].parse().unwrap()))
            } else {
                None
            }
        })
        .collect()
}

fn bbox(pdf: &[u8]) -> [f32; 4] {
    let text = String::from_utf8_lossy(pdf);
    let data = text
        .split_once("/BBox [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0;
    let numbers: Vec<f32> = data
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    numbers.try_into().unwrap()
}

// Where the encoded JPEG's top-left, top-right, bottom-right and bottom-left
// corners must appear after applying each of the eight EXIF orientations.
const CORNERS: [[usize; 4]; 8] = [
    [0, 1, 2, 3],
    [1, 0, 3, 2],
    [2, 3, 0, 1],
    [3, 2, 1, 0],
    [0, 3, 2, 1],
    [1, 2, 3, 0],
    [2, 1, 0, 3],
    [3, 0, 1, 2],
];

fn assert_corners(matrix: [f32; 6], rect: [f32; 4], orientation: usize) {
    let [a, b, c, d, e, f] = matrix;
    let [left, bottom, right, top] = rect;
    let expected = [(left, top), (right, top), (right, bottom), (left, bottom)];
    for ((x, y), target) in [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)]
        .into_iter()
        .zip(CORNERS[orientation - 1])
    {
        let actual = (a * x + c * y + e, b * x + d * y + f);
        assert!(
            (actual.0 - expected[target].0).abs() < 0.025,
            "{orientation}: {actual:?} vs {:?}",
            expected[target]
        );
        assert!(
            (actual.1 - expected[target].1).abs() < 0.025,
            "{orientation}: {actual:?} vs {:?}",
            expected[target]
        );
    }
}

#[test]
fn all_exif_orientations_preserve_jpeg_samples_and_transform_linked_figures() {
    let source = "[![Camera photo](photo.jpg)](https://example.com/photo)";
    for little in [true, false] {
        for orientation in 1..=8 {
            let bytes = with_exif(orientation, little);
            let opts = options(bytes.clone());
            let pdf = render_pdf(source, &opts).unwrap();
            let text = String::from_utf8_lossy(&pdf);
            assert!(text.contains("/Subtype /Image /Width 32 /Height 16"));
            assert!(text.contains("/Filter /DCTDecode"));
            assert!(pdf.windows(bytes.len()).any(|window| window == bytes));
            assert!(text.contains("/Alt (Camera photo)"));
            assert!(text.contains("/URI (https://example.com/photo)"));
            let rect = bbox(&pdf);
            let (width, height) = if orientation <= 4 {
                (24.0, 12.0)
            } else {
                (12.0, 24.0)
            };
            assert!((rect[2] - rect[0] - width).abs() < 0.025);
            assert!((rect[3] - rect[1] - height).abs() < 0.025);
            let matrices = image_matrices(&content(&pdf));
            assert_eq!(matrices.len(), 1);
            assert_corners(matrices[0], rect, usize::from(orientation));
            assert!(render_warnings(&parse_markdown(source), &opts).is_empty());
        }
    }
}

#[test]
fn inline_and_running_photos_measure_the_oriented_axes() {
    for orientation in 1..=8 {
        let opts = options(with_exif(orientation, true));
        let inline = render_pdf("Before ![Camera photo](photo.jpg) after.", &opts).unwrap();
        let rect = bbox(&inline);
        let aspect = (rect[2] - rect[0]) / (rect[3] - rect[1]);
        let expected = if orientation <= 4 { 2.0 } else { 0.5 };
        assert!(
            (aspect - expected).abs() < 0.01,
            "inline orientation {orientation}"
        );
        assert_corners(
            image_matrices(&content(&inline))[0],
            rect,
            usize::from(orientation),
        );

        let mut running = opts;
        running.running.header.image = Some(PdfRunningImage {
            dest: "photo.jpg".into(),
            position: PdfRunningImagePosition::Left,
            height_pt: Some(18),
        });
        let pdf = render_pdf("Body text", &running).unwrap();
        let matrix = image_matrices(&content(&pdf))[0];
        let [a, b, c, d, e, f] = matrix;
        let xs = [e, a + e, c + e, a + c + e];
        let ys = [f, b + f, d + f, b + d + f];
        let rect = [
            xs.into_iter().fold(f32::INFINITY, f32::min),
            ys.into_iter().fold(f32::INFINITY, f32::min),
            xs.into_iter().fold(f32::NEG_INFINITY, f32::max),
            ys.into_iter().fold(f32::NEG_INFINITY, f32::max),
        ];
        assert!((rect[3] - rect[1] - 18.0).abs() < 0.025);
        assert!((rect[2] - rect[0] - 18.0 * expected).abs() < 0.025);
        assert_corners(matrix, rect, usize::from(orientation));
        assert!(content(&pdf).contains("/Type /Pagination /Subtype /Header"));
    }
}

#[test]
fn nested_svg_jpegs_fit_oriented_dimensions_and_paint_in_downward_coordinates() {
    for orientation in 1..=8 {
        let jpeg = with_exif(orientation, true);
        let encoded = franken_markdown::html::base64_encode(&jpeg);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><image x="8" y="12" width="80" height="64" href="data:image/jpeg;base64,{encoded}"/></svg>"#
        );
        let opts = PdfOptions {
            image_assets: vec![PdfImageAsset::new("photo.svg", svg.into_bytes())],
            ..PdfOptions::default()
        };
        let pdf = render_pdf("![Nested photograph](photo.svg)", &opts).unwrap();
        let matrices = image_matrices(&content(&pdf));
        assert_eq!(matrices.len(), 1);
        // The outer SVG transform changes the coordinate system to y-down.
        // Reflect the measured local result back to y-up for the corner oracle.
        let [a, b, c, d, e, f] = matrices[0];
        let local_up = [a, -b, c, -d, e, -f];
        let rect = if orientation <= 4 {
            [8.0, -64.0, 88.0, -24.0]
        } else {
            [32.0, -76.0, 64.0, -12.0]
        };
        assert_corners(local_up, rect, usize::from(orientation));
        assert!(pdf.windows(jpeg.len()).any(|window| window == jpeg));
    }
}

#[test]
fn ignored_metadata_keeps_normal_paint_and_oriented_output_is_deterministic() {
    let source = "![Camera photo](photo.jpg)";
    let baseline = render_pdf(source, &options(jpeg())).unwrap();
    for orientation in [0, 1, 9, u16::MAX] {
        let pdf = render_pdf(source, &options(with_exif(orientation, true))).unwrap();
        assert_eq!(
            content(&pdf),
            content(&baseline),
            "orientation {orientation}"
        );
        assert_eq!(bbox(&pdf), bbox(&baseline));
    }
    let mut corrupt = with_exif(6, true);
    // APP1 remains correctly sized but its IFD0 offset points out of bounds.
    corrupt[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    let pdf = render_pdf(source, &options(corrupt)).unwrap();
    assert_eq!(content(&pdf), content(&baseline));

    let opts = options(with_exif(6, true));
    let pdf = render_pdf(source, &opts).unwrap();
    let document = parse_markdown(source);
    assert_eq!(pdf, render_pdf(source, &opts).unwrap());
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&document, &opts, PdfEmitOptions::monolithic()).unwrap()
    );
    assert_eq!(
        pdf,
        render_pdf_document_emitted(
            &document,
            &opts,
            PdfEmitOptions::chunked_with_ceiling(64 * 1024 * 1024)
        )
        .unwrap()
    );
}

#[test]
fn poppler_raster_preserves_all_four_quadrants_in_all_eight_orientations_when_available() {
    use std::process::Command;
    if Command::new("pdftoppm").arg("-v").output().is_err() {
        eprintln!("pdftoppm unavailable; structural orientation tests still run");
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("fmd-jpeg-orientation-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("oriented.pdf");
    let output = directory.join("oriented");
    for orientation in 1..=8 {
        let pdf = render_pdf(
            "![Camera photo](photo.jpg)",
            &options(with_exif(orientation, true)),
        )
        .unwrap();
        let rect = bbox(&pdf);
        std::fs::write(&path, pdf).unwrap();
        let result = Command::new("pdftoppm")
            .args(["-singlefile", "-r", "144"])
            .arg(&path)
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let ppm = std::fs::read(output.with_extension("ppm")).unwrap();
        let mut offset = 0;
        let mut tokens = Vec::new();
        while tokens.len() < 4 {
            while ppm[offset].is_ascii_whitespace() {
                offset += 1;
            }
            if ppm[offset] == b'#' {
                while ppm[offset] != b'\n' {
                    offset += 1;
                }
                continue;
            }
            let start = offset;
            while !ppm[offset].is_ascii_whitespace() {
                offset += 1;
            }
            tokens.push(std::str::from_utf8(&ppm[start..offset]).unwrap());
        }
        assert_eq!(tokens[0], "P6");
        assert_eq!(tokens[3], "255");
        let width: usize = tokens[1].parse().unwrap();
        let height: usize = tokens[2].parse().unwrap();
        offset += 1;
        let pixels = &ppm[offset..];
        assert_eq!(pixels.len(), width * height * 3);
        let palette = [
            [220u8, 30, 30],
            [30, 180, 45],
            [240, 200, 25],
            [25, 65, 220],
        ];
        let centers = [(0.25, 0.75), (0.75, 0.75), (0.75, 0.25), (0.25, 0.25)];
        for (source_corner, target) in CORNERS[usize::from(orientation) - 1]
            .into_iter()
            .enumerate()
        {
            let (u, v) = centers[target];
            let x = ((rect[0] + (rect[2] - rect[0]) * u) * 2.0) as usize;
            let y = height - ((rect[1] + (rect[3] - rect[1]) * v) * 2.0) as usize;
            let pixel = &pixels[(y * width + x) * 3..(y * width + x) * 3 + 3];
            for channel in 0..3 {
                assert!(
                    pixel[channel].abs_diff(palette[source_corner][channel]) < 35,
                    "orientation {orientation}, corner {source_corner}: {pixel:?}"
                );
            }
        }
    }
}
