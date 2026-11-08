//! Footnote identifiers are author data, even when they resemble the PDF
//! renderer's private destination names. Alpha-renaming an unreferenced note
//! must not discard its body or change its generated number.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{
    BaselineGrid, PdfASettings, PdfEmitOptions, PdfOptions, PdfPageEmission, parse_markdown,
    render_pdf, render_pdf_document_emitted, render_pdf_document_pdfa,
    render_pdf_document_profiled, render_pdf_document_reported, verify, wasm,
};

fn source(label: &str) -> String {
    format!(
        concat!(
            "# Study\n\nBODYPROOF.\n\n",
            "[^{label}]: NOTEALPHA **NOTESTYLE**.\n\n",
            "    - NOTELIST\n\n",
            "    ```text\n",
            "    NOTECODE = 42;\n",
            "    ```\n\n",
            "    NOTETAIL.\n"
        ),
        label = label
    )
}

#[test]
fn an_authored_matching_number_is_preserved_as_text() {
    for body in [
        "¹ Original text",
        "¹ **Original text**",
        "¹",
        "² Original text",
    ] {
        let authored = format!("[^fmd-fn-1]: {body}\n");
        let ordinary = format!("[^evidence]: {body}\n");
        assert!(
            render_pdf(&authored, &PdfOptions::default()).unwrap()
                == render_pdf(&ordinary, &PdfOptions::default()).unwrap()
        );
    }
}

fn assert_complete(source: &str, options: &PdfOptions) {
    let report = verify::verify_pdf(&parse_markdown(source), options).unwrap();
    let text: String = report
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect();
    for expected in [
        "BODYPROOF",
        "Notes",
        "¹",
        "NOTEALPHA",
        "NOTESTYLE",
        "NOTELIST",
        "NOTECODE",
        "NOTETAIL",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

#[test]
fn author_note_names_never_select_the_prepared_pdf_path() {
    let options = PdfOptions::default();
    let ordinary = source("evidence");
    assert_complete(&ordinary, &options);
    let expected = render_pdf(&ordinary, &options).unwrap();
    for id in [
        "fmd-fn-evidence",
        "fmd-fn-1",
        "fmd-fn-0",
        "fmd-fn-01",
        "fmd-fn-4294967295",
        "fmd-fn-4294967296",
    ] {
        let authored = source(id);
        assert_complete(&authored, &options);
        assert!(
            render_pdf(&authored, &options).unwrap() == expected,
            "renaming an unreferenced definition to {id} changed the PDF"
        );
    }
}

#[test]
fn all_pdf_entrypoints_preserve_the_complete_note_with_strict_grid_layout() {
    let mut options = PdfOptions {
        strict_pagination: true,
        footnote_backlinks: true,
        ..PdfOptions::default()
    };
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(13.125).unwrap());
    let authored = source("fmd-fn-evidence");
    let ordinary = source("evidence");
    let document = parse_markdown(&authored);
    let control = parse_markdown(&ordinary);
    let original = document.clone();
    assert_complete(&authored, &options);
    let expected = render_pdf(&ordinary, &options).unwrap();
    for emission in [PdfPageEmission::Chunked, PdfPageEmission::Monolithic] {
        let emit = PdfEmitOptions {
            emission,
            ..PdfEmitOptions::default()
        };
        assert!(render_pdf_document_emitted(&document, &options, emit).unwrap() == expected);
        let reported = render_pdf_document_reported(&document, &options, emit).unwrap();
        assert!(reported.bytes == expected);
        assert!(reported.pagination.page_breaks.is_exact());
    }
    assert!(
        render_pdf_document_profiled(&document, &options)
            .unwrap()
            .bytes
            == expected
    );
    let settings = PdfASettings::a2b_strict();
    assert!(
        render_pdf_document_pdfa(&document, &options, settings).unwrap()
            == render_pdf_document_pdfa(&control, &options, settings).unwrap()
    );
    let browser = wasm::WasmRenderOptions {
        theme: options.theme.clone(),
        strict_pagination: true,
        footnote_backlinks: true,
        ..wasm::WasmRenderOptions::default()
    };
    assert!(wasm::render_pdf(&authored, &browser).unwrap().bytes == expected);
    assert_eq!(document, original, "rendering must not modify source notes");
}

#[test]
fn chapter_scoping_and_explicit_note_links_survive_source_name_collisions() {
    fn book(label: &str) -> BookRenderer {
        BookRenderer::new(&[
            BookInput {
                path: "one.md".into(),
                source: format!(
                    "# One\n\nBody.[^{label}] [Other note](two.md#fn-{label})\n\n\
                     [^{label}]: FIRSTNOTE.\n"
                ),
            },
            BookInput {
                path: "two.md".into(),
                source: format!(
                    "# Two\n\n[First note](one.md#fn-{label})\n\n\
                     [^{label}]: SECONDNOTE.\n"
                ),
            },
        ])
        .unwrap()
    }
    let options = PdfOptions {
        footnote_backlinks: true,
        ..PdfOptions::default()
    };
    let expected = book("evidence")
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    let actual = book("fmd-fn-evidence")
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    assert!(
        actual == expected,
        "book links must retain chapter identity"
    );
}
