//! Running header/footer chrome (GH #13): opt-in, byte-stable defaults, sugar
//! equivalence, artifact tagging, first-page skip, margin fit errors, and a
//! committed structural golden.
//!
//! Regenerate the golden after an intentional output change:
//!   UPDATE_GOLDEN=1 cargo test --test pdf_running_test

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use franken_markdown::{
    PdfOptions, PdfRunningBand, PdfRunningContent, RenderError, render_pdf, zlib_decompress,
};

const EPOCH: u64 = 1_700_000_000;

fn base() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(EPOCH),
        ..PdfOptions::default()
    }
}

/// Enough paragraphs for several Letter pages.
fn long_doc() -> String {
    let mut md = String::from("# Widget Spec\n\n");
    for i in 0..60 {
        md.push_str(&format!(
            "Paragraph {i} explains the widget in enough words to wrap across the measure \
             and fill the page steadily so pagination produces several pages.\n\n"
        ));
    }
    md
}

fn full_chrome() -> PdfRunningContent {
    PdfRunningContent {
        header: PdfRunningBand {
            left: Some("{title}".into()),
            center: None,
            right: Some("{date}".into()),
            rule: true,
            image: None,
        },
        footer: PdfRunningBand {
            left: Some("Confidential {foo}".into()),
            center: Some("{page} / {pages}".into()),
            right: Some("{author}".into()),
            rule: true,
            image: None,
        },
        skip_first_page: false,
    }
}

/// Decompressed page content streams, in page order (they are the streams
/// that carry text objects).
fn content_streams(pdf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = pdf;
    while let Some(start) = find(rest, b"stream\n") {
        let body = &rest[start + 7..];
        let Some(end) = find(body, b"endstream") else {
            break;
        };
        let raw = body[..end].strip_suffix(b"\n").unwrap_or(&body[..end]);
        if let Some(bytes) = zlib_decompress(raw, 64 << 20) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if text.contains("BT") && text.contains(" Tf") {
                out.push(text);
            }
        }
        rest = &body[end + b"endstream".len()..];
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

#[test]
fn chrome_that_draws_nothing_keeps_default_bytes() {
    let md = long_doc();
    let default = render_pdf(&md, &base()).unwrap();
    let mut blank = base();
    blank.running.skip_first_page = true;
    blank.running.header.left = Some(String::new());
    blank.running.footer.center = Some("{date}".into());
    blank.metadata_epoch_seconds = None;
    let mut default_no_epoch = base();
    default_no_epoch.metadata_epoch_seconds = None;
    assert_eq!(
        render_pdf(&md, &blank).unwrap(),
        render_pdf(&md, &default_no_epoch).unwrap(),
        "an empty {{date}} slot with no epoch must not draw or change bytes"
    );
    for stream in content_streams(&default) {
        assert!(!stream.contains("/Pagination"));
    }
}

#[test]
fn page_numbers_sugar_equals_an_explicit_page_footer_and_explicit_center_wins() {
    let md = long_doc();
    let sugar = render_pdf(
        &md,
        &PdfOptions {
            page_numbers: true,
            ..base()
        },
    )
    .unwrap();
    let mut explicit = base();
    explicit.running.footer.center = Some("{page}".into());
    assert_eq!(sugar, render_pdf(&md, &explicit).unwrap());

    let mut custom = base();
    custom.running.footer.center = Some("Page {page}".into());
    let custom_bytes = render_pdf(&md, &custom).unwrap();
    custom.page_numbers = true;
    assert_eq!(
        custom_bytes,
        render_pdf(&md, &custom).unwrap(),
        "page_numbers must not paint a second folio over an explicit footer center"
    );
}

#[test]
fn chrome_is_deterministic_and_only_pagination_artifacts() {
    let md = long_doc();
    let mut opts = base();
    opts.title = Some("Widget Spec".into());
    opts.author = Some("Acme".into());
    opts.running = full_chrome();
    let first = render_pdf(&md, &opts).unwrap();
    assert_eq!(first, render_pdf(&md, &opts).unwrap());

    // A different epoch changes only {date}: the bytes differ.
    let later = render_pdf(
        &md,
        &PdfOptions {
            metadata_epoch_seconds: Some(EPOCH + 86_400 * 40),
            ..opts.clone()
        },
    )
    .unwrap();
    assert_ne!(first, later);

    let streams = content_streams(&first);
    assert!(
        streams.len() >= 3,
        "fixture must span several pages, got {}",
        streams.len()
    );
    for stream in &streams {
        assert_eq!(count(stream, "/Subtype /Header>> BDC"), 1);
        assert_eq!(count(stream, "/Subtype /Footer>> BDC"), 1);
        assert_eq!(
            count(stream, "BDC") + count(stream, "BMC"),
            count(stream, "EMC"),
            "marked content must stay balanced"
        );
    }

    // The structure tree (reading order) is identical to a render without chrome.
    let plain = render_pdf(
        &md,
        &PdfOptions {
            title: opts.title.clone(),
            author: opts.author.clone(),
            ..base()
        },
    )
    .unwrap();
    let text = |pdf: &[u8]| String::from_utf8_lossy(pdf).into_owned();
    assert_eq!(
        count(&text(&first), "/StructElem"),
        count(&text(&plain), "/StructElem")
    );
    assert_eq!(count(&text(&first), "/MCID"), count(&text(&plain), "/MCID"));
    assert_eq!(
        content_streams(&plain).len(),
        streams.len(),
        "chrome must not change pagination"
    );
}

#[test]
fn skip_first_page_leaves_page_one_bare() {
    let md = long_doc();
    let mut opts = base();
    opts.running = full_chrome();
    opts.running.skip_first_page = true;
    opts.page_numbers = true;
    let streams = content_streams(&render_pdf(&md, &opts).unwrap());
    assert!(!streams[0].contains("/Pagination"));
    assert!(streams[1].contains("/Subtype /Header") && streams[1].contains("/Subtype /Footer"));
}

#[test]
fn bands_that_do_not_fit_their_margin_fail_closed() {
    let mut opts = base();
    opts.theme.page.margins.bottom_pt = 20.0;
    opts.running.footer.left = Some("{page}".into());
    opts.running.footer.rule = true;
    let err = render_pdf("# x", &opts).unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    let message = err.to_string();
    assert!(
        message.contains("pdf_running_footer_does_not_fit"),
        "{message}"
    );
    assert!(message.contains("margin_bottom_pt"), "{message}");

    // The legacy folio alone keeps rendering in the same tight margin.
    let legacy = PdfOptions {
        page_numbers: true,
        running: PdfRunningContent::default(),
        ..opts
    };
    assert!(render_pdf("# x", &legacy).is_ok());
}

// ---- golden -----------------------------------------------------------------

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Byte length + fingerprint of the whole PDF, plus the chrome operators of
/// the first two pages for a reviewable diff.
fn snapshot(pdf: &[u8]) -> String {
    let mut out = format!(
        "bytes={}\nfingerprint={:016x}\n",
        pdf.len(),
        fingerprint(pdf)
    );
    for (idx, stream) in content_streams(pdf).iter().take(2).enumerate() {
        out.push_str(&format!("--- page {} chrome ---\n", idx + 1));
        let mut rest = stream.as_str();
        while let Some(start) = rest.find("/Artifact <</Type /Pagination") {
            let band = &rest[start..];
            let end = band.find("EMC\n").map_or(band.len(), |end| end + 4);
            out.push_str(&band[..end]);
            rest = &band[end..];
        }
    }
    out
}

#[test]
fn running_chrome_matches_committed_golden() {
    let mut opts = base();
    opts.title = Some("Widget Spec".into());
    opts.author = Some("Acme".into());
    opts.running = full_chrome();
    opts.running.skip_first_page = true;
    let snap = snapshot(&render_pdf(&long_doc(), &opts).unwrap());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/output/running-chrome.pdf.snapshot");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &snap).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("missing {path:?}; run UPDATE_GOLDEN=1 to create it; got:\n{snap}")
    });
    assert_eq!(
        want, snap,
        "running chrome golden drifted (UPDATE_GOLDEN=1 after review)"
    );
}

// ---- CLI ---------------------------------------------------------------------

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fmd-running-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn cli_flags_reach_the_renderer_and_match_the_library() {
    let dir = scratch_dir("flags");
    let out = dir.join("chrome.pdf");
    let md = long_doc();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_fmd"))
        .env("SOURCE_DATE_EPOCH", EPOCH.to_string())
        .args([
            "--no-config",
            "--text",
            &md,
            "--to",
            "pdf",
            "--title",
            "Widget Spec",
            "--author",
            "Acme",
        ])
        .args([
            "--pdf-header-left",
            "{title}",
            "--pdf-header-right",
            "{date}",
            "--pdf-header-rule",
        ])
        .args([
            "--pdf-footer-left",
            "Confidential {foo}",
            "--pdf-footer-center",
            "{page} / {pages}",
        ])
        .args([
            "--pdf-footer-right",
            "{author}",
            "--pdf-footer-rule",
            "--out",
        ])
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success());
    let mut opts = base();
    opts.title = Some("Widget Spec".into());
    opts.author = Some("Acme".into());
    opts.running = full_chrome();
    assert_eq!(
        std::fs::read(&out).unwrap(),
        render_pdf(&md, &opts).unwrap()
    );
}

#[test]
fn cli_band_that_does_not_fit_exits_70_naming_the_margin() {
    let dir = scratch_dir("tight");
    let config = dir.join("config");
    std::fs::write(&config, "margin_top_pt=12\n").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fmd"))
        .env("FMD_CONFIG", &config)
        .args([
            "--text",
            "# x",
            "--to",
            "pdf",
            "--pdf-header-center",
            "{page}",
            "--out",
        ])
        .arg(dir.join("tight.pdf"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(70));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("margin_top_pt"), "{stderr}");
}

/// Font resource names selected (`/Fn size Tf`) inside the page's header
/// artifact.
fn header_fonts(stream: &str) -> Vec<String> {
    let start = stream
        .find("/Subtype /Header>> BDC")
        .expect("header artifact");
    let section = &stream[start..];
    let section = &section[..section.find("\nEMC\n").unwrap_or(section.len())];
    let tokens: Vec<&str> = section.split_whitespace().collect();
    let mut fonts: Vec<String> = tokens
        .windows(3)
        .filter(|w| w[2] == "Tf" && w[0].starts_with('/'))
        .map(|w| w[0].to_owned())
        .collect();
    fonts.sort();
    fonts.dedup();
    fonts
}

#[test]
fn header_glyphs_missing_from_the_body_face_use_the_fallback_face_and_title_tokens_stay_literal() {
    // An ASCII-only body, so the symbol face is embedded only if the chrome
    // routes its glyph there; the title carries PDF string delimiters and a
    // token that must not expand.
    let mut opts = base();
    opts.title =
        Some("Sum \u{2248} \u{2192} \u{21d2} \u{2260} \u{2211} \u{221e} (a\\b) {pages}".into());
    opts.running.header.center = Some("{title}".into());
    let md = long_doc();
    assert!(md.is_ascii());
    let pdf = render_pdf(&md, &opts).unwrap();
    let streams = content_streams(&pdf);
    assert!(!streams.is_empty());
    for stream in &streams {
        let fonts = header_fonts(stream);
        assert!(fonts.len() >= 2, "header drew with {fonts:?} only");
    }
    assert_eq!(render_pdf(&md, &opts).unwrap(), pdf);
}
