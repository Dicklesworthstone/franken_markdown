//! Images inside running PDF text (badges, icons) are drawn as unbreakable
//! boxes in the paragraph instead of printing their alt text, tagged as
//! `/Figure` with their alt text, while a paragraph of large pictures keeps one
//! figure line per picture.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::{VerifyTextRun, verification_text_layer};
use franken_markdown::{
    PdfImageAsset, PdfOptions, RenderWarning, parse_markdown, render_pdf, render_warnings,
};

fn png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc_input);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    let row: Vec<u8> = std::iter::once(0)
        .chain(rgb.iter().copied().cycle().take(width as usize * 3))
        .collect();
    let raw = row.repeat(height as usize);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(
        &mut out,
        b"IDAT",
        &franken_markdown::compress::zlib_compress(&raw),
    );
    chunk(&mut out, b"IEND", &[]);
    out
}

fn badge(value: &str) -> Vec<u8> {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"90\" height=\"20\">\
         <rect width=\"90\" height=\"20\" fill=\"#007ec6\"/>\
         <text x=\"45\" y=\"14\" fill=\"#fff\" font-size=\"11\" text-anchor=\"middle\">{value}</text>\
         </svg>"
    )
    .into_bytes()
}

fn options() -> PdfOptions {
    PdfOptions {
        image_assets: vec![
            PdfImageAsset::new("icon.png", png(16, 16, [220, 40, 40])),
            PdfImageAsset::new("big1.png", png(400, 300, [40, 120, 220])),
            PdfImageAsset::new("big2.png", png(400, 300, [40, 180, 90])),
            PdfImageAsset::new("badge.svg", badge("QZX")),
        ],
        ..PdfOptions::default()
    }
}

fn runs(markdown: &str, opts: &PdfOptions) -> Vec<VerifyTextRun> {
    verification_text_layer(&parse_markdown(markdown), opts)
        .unwrap()
        .pages
        .into_iter()
        .flat_map(|page| page.runs)
        .filter(|run| !run.text.is_empty() || run.kind == "image")
        .collect()
}

fn image_xobjects(pdf: &[u8]) -> usize {
    String::from_utf8_lossy(pdf)
        .matches("/Subtype /Image")
        .count()
}

#[test]
fn image_in_prose_is_drawn_and_tagged_instead_of_printing_alt_text() {
    let markdown = "Press the ![stop icon](icon.png) button to stop.";
    let opts = options();
    let pdf = render_pdf(markdown, &opts).unwrap();
    assert_eq!(image_xobjects(&pdf), 1);
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/S /Figure"), "inline image is a /Figure");
    assert!(
        raw.contains("/Alt (stop icon)"),
        "figure keeps its alt text"
    );
    assert!(
        !render_warnings(&parse_markdown(markdown), &opts)
            .iter()
            .any(|warning| matches!(warning, RenderWarning::UnresolvedImage(_))),
    );
    // The paragraph stays one line of text with the image box inside it.
    let lines = runs(markdown, &opts);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].text, "Press the stop icon button to stop.");
}

#[test]
fn unresolved_inline_images_keep_their_alt_text() {
    let markdown = "A missing ![gone image](missing.png) picture.";
    let pdf = render_pdf(markdown, &options()).unwrap();
    assert_eq!(image_xobjects(&pdf), 0);
    let lines = runs(markdown, &options());
    assert_eq!(lines[0].text, "A missing gone image picture.");
}

#[test]
fn svg_badges_in_a_row_draw_their_own_text_glyphs() {
    let markdown = "![one](badge.svg) [![two](badge.svg)](https://example.com) ![three](badge.svg)";
    let pdf = render_pdf(markdown, &options()).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert!(
        raw.contains("/URI (https://example.com)"),
        "linked badge stays clickable"
    );
    assert_eq!(raw.matches("/S /Figure").count(), 3);
    let Some(extracted) = pdftotext(&pdf) else {
        return;
    };
    assert_eq!(extracted.matches("QZX").count(), 3, "{extracted}");
}

#[test]
fn images_in_table_cells_are_drawn() {
    let markdown = "| Status | Name |\n|---|---|\n| ![ok](icon.png) | first |\n";
    let pdf = render_pdf(markdown, &options()).unwrap();
    assert_eq!(image_xobjects(&pdf), 1);
    assert!(String::from_utf8_lossy(&pdf).contains("/Alt (ok)"));
}

#[test]
fn a_paragraph_of_large_pictures_keeps_one_figure_line_each() {
    let markdown = "![one](big1.png) ![two](big2.png)";
    let lines = runs(markdown, &options());
    assert_eq!(
        lines.iter().filter(|run| run.kind == "image").count(),
        2,
        "{lines:?}"
    );
    // Badge-sized images in the same shape stay inline on one text line.
    let lines = runs("![one](icon.png) ![two](icon.png)", &options());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].kind, "paragraph");
}

#[test]
fn taller_than_text_images_open_line_space_and_icons_do_not() {
    let opts = options();
    let gap = |markdown: &str| {
        let lines = runs(markdown, &opts);
        lines[1].y - lines[0].y
    };
    let plain = gap("Alpha beta.\n\nGamma delta.");
    let icon = gap("Alpha beta.\n\nGamma ![i](icon.png) delta.");
    let badge = gap("Alpha beta.\n\nGamma ![b](badge.svg) delta.");
    assert!((icon - plain).abs() < 0.01, "plain {plain} icon {icon}");
    assert!(
        badge.abs() > plain.abs() + 1.0,
        "plain {plain} badge {badge}"
    );
}

#[test]
fn inline_images_render_deterministically() {
    let markdown =
        "Text ![a](icon.png) and ![b](badge.svg) end.\n\n| x |\n|---|\n| ![c](icon.png) |\n";
    let a = render_pdf(markdown, &options()).unwrap();
    let b = render_pdf(markdown, &options()).unwrap();
    assert_eq!(a, b);
}

fn pdftotext(pdf: &[u8]) -> Option<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    Command::new("pdftotext").arg("-v").output().ok()?;
    let mut child = Command::new("pdftotext")
        .args(["-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(pdf).ok()?;
    let output = child.wait_with_output().ok()?;
    assert!(output.status.success());
    Some(String::from_utf8(output.stdout).unwrap())
}
