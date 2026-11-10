//! Exercise the real retained-book profile through its public browser API.
#![cfg(feature = "wasm-book")]

use franken_markdown::book::browser::FmdBook;
use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{PdfASettings, PdfOptions};

fn replace_profile(book: &mut FmdBook, enabled: bool) -> Result<(), String> {
    book.set_pdf_options(
        enabled.then(|| "footnote-backlinks".to_owned()),
        None,       // base font size
        None,       // heading scale
        None,       // table font size
        None,       // TOC depth
        None,       // page-fitting target
        false,      // code line numbers
        None,       // metadata epoch
        Vec::new(), // running header/footer slots
        false,      // header rule
        false,      // footer rule
        false,      // skip the first running band
    )
    .map_err(|_| format!("valid profile replacement failed (enabled={enabled})"))
}

fn rendered(book: &FmdBook) -> Result<Vec<u8>, String> {
    book.render_pdf()
        .map_err(|_| "valid retained-book PDF rendering failed".to_owned())
}

fn assert_pdf(actual: &[u8], expected: &[u8], state: &str) {
    assert!(actual.starts_with(b"%PDF-"), "{state}: expected a real PDF");
    // Avoid dumping embedded fonts and complete PDFs into failure logs.
    assert!(
        actual == expected,
        "{state}: PDF bytes differ (actual={}, expected={})",
        actual.len(),
        expected.len()
    );
}

#[test]
fn retained_footnote_profile_replacements_preserve_pdf_parity_and_sources() -> Result<(), String> {
    let inputs = [
        BookInput {
            path: "one.md".into(),
            source: "# First\n\nA claim[^n], then its repeated citation[^n].\n\n[^n]: First supporting evidence.\n".into(),
        },
        BookInput {
            path: "two.md".into(),
            source: "# Second\n\nAn independent claim[^n].\n\n[^n]: Second supporting evidence.\n".into(),
        },
    ];
    let native = BookRenderer::new(&inputs)
        .map_err(|error| format!("valid native fixture was rejected: {error}"))?;
    let expected_default = native
        .render_pdf_with_options(&PdfOptions::default(), PdfASettings::OFF)
        .map_err(|error| format!("default native PDF failed: {error}"))?;
    let expected_enabled = native
        .render_pdf_with_options(
            &PdfOptions {
                footnote_backlinks: true,
                ..PdfOptions::default()
            },
            PdfASettings::OFF,
        )
        .map_err(|error| format!("native PDF with return links failed: {error}"))?;
    assert!(
        expected_default != expected_enabled,
        "the fixture must change actual PDF bytes when return links are enabled"
    );

    let mut retained = FmdBook::new(
        inputs.iter().map(|input| input.path.clone()).collect(),
        inputs.iter().map(|input| input.source.clone()).collect(),
    )
    .map_err(|_| "valid browser fixture was rejected".to_owned())?;
    let original_identity = (
        retained.source_revision(),
        retained.source_length(),
        retained.chapter_count(),
    );
    assert_pdf(&rendered(&retained)?, &expected_default, "initial default");

    // Reuse one production FmdBook for both cycles. This catches sticky profile
    // state or stale output after replacement without relying on internals.
    for cycle in 1..=2 {
        for enabled in [true, false] {
            replace_profile(&mut retained, enabled)?;
            let expected = if enabled {
                &expected_enabled
            } else {
                &expected_default
            };
            let state = format!("cycle {cycle}, enabled={enabled}");
            assert_pdf(&rendered(&retained)?, expected, &state);
            assert_pdf(&rendered(&retained)?, expected, &state);
            assert_eq!(
                (
                    retained.source_revision(),
                    retained.source_length(),
                    retained.chapter_count(),
                ),
                original_identity,
                "profile replacement must preserve source identity"
            );
        }
    }
    Ok(())
}

#[test]
fn retained_continuation_profile_can_be_enabled_and_reset_without_stale_output()
-> Result<(), String> {
    let mut source = "# Retained notes\n\nClaim[^n].\n\n[^n]: Opening.\n".to_owned();
    for index in 0..80 {
        source.push_str(&format!(
            "\n    Evidence{index:03} continues across pages without being lost.\n"
        ));
    }
    let input = [BookInput {
        path: "notes.md".into(),
        source: source.clone(),
    }];
    let native = BookRenderer::new(&input).map_err(|error| error.to_string())?;
    let ordinary = native
        .render_pdf_with_options(&PdfOptions::default(), PdfASettings::OFF)
        .map_err(|error| error.to_string())?;
    let continued = native
        .render_pdf_with_options(
            &PdfOptions {
                split_footnotes: true,
                ..PdfOptions::default()
            },
            PdfASettings::OFF,
        )
        .map_err(|error| error.to_string())?;
    assert!(
        ordinary != continued,
        "fixture must require actual note continuation"
    );
    let mut retained = FmdBook::new(vec!["notes.md".into()], vec![source])
        .map_err(|_| "valid retained fixture rejected".to_owned())?;
    let identity = (
        retained.source_revision(),
        retained.source_length(),
        retained.chapter_count(),
    );
    for enabled in [true, false, true, false] {
        retained
            .set_pdf_options(
                enabled.then(|| "split-footnotes".to_owned()),
                None,
                None,
                None,
                None,
                None,
                false,
                None,
                Vec::new(),
                false,
                false,
                false,
            )
            .map_err(|_| "valid continuation profile rejected".to_owned())?;
        let expected = if enabled { &continued } else { &ordinary };
        assert_pdf(&rendered(&retained)?, expected, "continuation replacement");
        assert_pdf(&rendered(&retained)?, expected, "repeated export");
        assert_eq!(
            (
                retained.source_revision(),
                retained.source_length(),
                retained.chapter_count()
            ),
            identity
        );
    }
    Ok(())
}
