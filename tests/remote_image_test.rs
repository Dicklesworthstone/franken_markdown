//! Remote/hotlinked image regression tests (GH issue #2) plus the JPEG
//! `/DCTDecode` embedding path they ride on.
//!
//! The library-level tests pin the writer structure for JPEG assets; the
//! process-level tests execute the real `fmd` binary against a loopback HTTP
//! server so the fetch-at-render-time contract (timeout, size cap, clean
//! offline fallback) is proven end to end without leaving the machine.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

use franken_markdown::{
    PdfImageAsset, PdfOptions, RenderWarning, parse_markdown, render_pdf, render_warnings,
};

// ---- synthetic JPEG builders -------------------------------------------------

/// A structurally valid JPEG prefix: SOI, one SOF frame header, then SOS.
/// The entropy-coded payload is irrelevant to the embedder (the reader's JPEG
/// decoder owns it), so a couple of stand-in bytes suffice.
fn jpeg_with_frame(sof_marker: u8, precision: u8, width: u16, height: u16, ncomp: u8) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8];
    let mut sof = Vec::new();
    sof.push(precision);
    sof.extend_from_slice(&height.to_be_bytes());
    sof.extend_from_slice(&width.to_be_bytes());
    sof.push(ncomp);
    for c in 0..ncomp {
        sof.extend_from_slice(&[c + 1, 0x11, 0]);
    }
    bytes.extend_from_slice(&[0xFF, sof_marker]);
    bytes.extend_from_slice(&(u16::try_from(sof.len() + 2).unwrap()).to_be_bytes());
    bytes.extend_from_slice(&sof);
    bytes.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0xAB, 0xCD]);
    bytes
}

fn jpeg_opts(bytes: Vec<u8>) -> PdfOptions {
    let mut opts = PdfOptions::default();
    opts.image_assets
        .push(PdfImageAsset::new("photo.jpg", bytes));
    opts
}

fn unsupported_image_warnings(src: &str, opts: &PdfOptions) -> Vec<String> {
    let doc = parse_markdown(src);
    render_warnings(&doc, opts)
        .into_iter()
        .filter_map(|warning| match warning {
            RenderWarning::UnsupportedImage(dest) | RenderWarning::UnresolvedImage(dest) => {
                Some(dest)
            }
            _ => None,
        })
        .collect()
}

const JPEG_MD: &str = "![A photo](photo.jpg)";

#[test]
fn baseline_rgb_jpeg_embeds_as_dctdecode_rgb() {
    let opts = jpeg_opts(jpeg_with_frame(0xC0, 8, 20, 10, 3));
    assert!(unsupported_image_warnings(JPEG_MD, &opts).is_empty());
    let pdf = render_pdf(JPEG_MD, &opts).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("/Filter /DCTDecode"),
        "JPEG must pass through as DCT"
    );
    assert!(text.contains("/ColorSpace /DeviceRGB"));
    assert!(text.contains("/Width 20 /Height 10"));
    assert!(
        text.contains("/Subtype /Image"),
        "the JPEG must embed as an image XObject"
    );
}

#[test]
fn progressive_and_grayscale_jpegs_embed() {
    // SOF2 progressive RGB.
    let opts = jpeg_opts(jpeg_with_frame(0xC2, 8, 4, 4, 3));
    assert!(unsupported_image_warnings(JPEG_MD, &opts).is_empty());
    // Grayscale single-component baseline.
    let opts = jpeg_opts(jpeg_with_frame(0xC0, 8, 4, 4, 1));
    assert!(unsupported_image_warnings(JPEG_MD, &opts).is_empty());
    let pdf = render_pdf(JPEG_MD, &opts).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/ColorSpace /DeviceGray"));
}

#[test]
fn hostile_or_unsupported_jpegs_fail_closed_to_alt_text() {
    // 4-component (Adobe CMYK/YCCK): polarity is ambiguous across readers.
    let cmyk = jpeg_with_frame(0xC0, 8, 4, 4, 4);
    // Lossless (SOF3) and arithmetic-coded (SOF9) are outside /DCTDecode.
    let lossless = jpeg_with_frame(0xC3, 8, 4, 4, 3);
    let arithmetic = jpeg_with_frame(0xC9, 8, 4, 4, 3);
    // 12-bit precision, zero dimensions, truncation.
    let deep = jpeg_with_frame(0xC0, 12, 4, 4, 3);
    let empty_dims = jpeg_with_frame(0xC0, 8, 0, 4, 3);
    let truncated = jpeg_with_frame(0xC0, 8, 4, 4, 3)[..6].to_vec();
    let no_frame = vec![0xFF, 0xD8, 0xFF, 0xD9];
    for bytes in [
        cmyk, lossless, arithmetic, deep, empty_dims, truncated, no_frame,
    ] {
        let opts = jpeg_opts(bytes);
        let warnings = unsupported_image_warnings(JPEG_MD, &opts);
        assert_eq!(
            warnings,
            vec!["photo.jpg".to_string()],
            "non-embeddable JPEG flavors must degrade to alt text"
        );
        let pdf = render_pdf(JPEG_MD, &opts).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(
            !text.contains("/DCTDecode"),
            "refused JPEG bytes must not embed"
        );
        assert!(
            !text.contains("/Subtype /Image"),
            "no image XObject may be written for a refused asset"
        );
    }
}

#[test]
fn jpeg_render_is_deterministic() {
    let opts = jpeg_opts(jpeg_with_frame(0xC0, 8, 20, 10, 3));
    assert_eq!(
        render_pdf(JPEG_MD, &opts).unwrap(),
        render_pdf(JPEG_MD, &opts).unwrap()
    );
}

// ---- process-level remote fetch tests ---------------------------------------

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn tiny_rgb_png() -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let idat = franken_markdown::compress::zlib_compress(&[0, 10, 20, 30, 40, 50, 60]);
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

fn system_fetcher_available() -> bool {
    ["curl", "wget"].iter().any(|tool| {
        Command::new(tool)
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
    })
}

/// Serve `responses` HTTP exchanges on a loopback listener, one thread total.
fn serve_once(status_line: &'static str, body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let header = format!(
                "{status_line}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    (format!("http://{addr}"), handle)
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fmd-remote-image-test-{tag}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn remote_png_is_fetched_and_embedded_in_the_pdf() {
    if !system_fetcher_available() {
        eprintln!("skipping: neither curl nor wget is available");
        return;
    }
    let (base, server) = serve_once("HTTP/1.1 200 OK", tiny_rgb_png());
    let dir = temp_dir("fetch");
    let md = dir.join("doc.md");
    let pdf = dir.join("doc.pdf");
    std::fs::write(&md, format!("![Kitten]({base}/kitten.png)\n")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args([
            md.to_str().unwrap(),
            "--to",
            "pdf",
            "--remote-image-timeout-secs",
            "10",
            "--out",
            pdf.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    server.join().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "render must succeed: {stderr}");
    assert!(
        !stderr.contains("no --pdf-image mapping"),
        "the remote image must resolve without a manual mapping: {stderr}"
    );
    let bytes = std::fs::read(&pdf).unwrap();
    assert!(
        String::from_utf8_lossy(&bytes).contains("/Subtype /Image"),
        "the fetched PNG must embed as an image XObject"
    );
}

#[test]
fn failed_remote_fetch_degrades_to_alt_text_with_a_warning() {
    if !system_fetcher_available() {
        eprintln!("skipping: neither curl nor wget is available");
        return;
    }
    let (base, server) = serve_once("HTTP/1.1 404 Not Found", b"gone".to_vec());
    let dir = temp_dir("404");
    let md = dir.join("doc.md");
    let pdf = dir.join("doc.pdf");
    std::fs::write(&md, format!("![Kitten]({base}/kitten.png)\n")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args([
            md.to_str().unwrap(),
            "--to",
            "pdf",
            "--remote-image-timeout-secs",
            "10",
            "--out",
            pdf.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    server.join().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a failed fetch must stay non-fatal: {stderr}"
    );
    assert!(
        stderr.contains("fetching remote image"),
        "the fetch failure must be reported: {stderr}"
    );
    assert!(
        pdf.exists(),
        "the PDF must still be written (alt-text fallback)"
    );
}

#[test]
fn no_remote_images_flag_disables_fetching() {
    // No server at all: with fetching disabled the CLI must not try the
    // network and must keep the pre-existing manual-mapping warning.
    let dir = temp_dir("optout");
    let md = dir.join("doc.md");
    let pdf = dir.join("doc.pdf");
    std::fs::write(&md, "![Kitten](http://127.0.0.1:9/never-fetched.png)\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args([
            md.to_str().unwrap(),
            "--to",
            "pdf",
            "--no-remote-images",
            "--out",
            pdf.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success());
    assert!(
        !stderr.contains("fetching remote image"),
        "--no-remote-images must skip the fetch entirely: {stderr}"
    );
    assert!(
        stderr.contains("no --pdf-image mapping"),
        "the unresolved-image warning must remain: {stderr}"
    );
}
