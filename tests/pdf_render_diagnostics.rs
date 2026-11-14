//! Diagnostics belong to the transformed document and final emitted PDF.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use franken_markdown::ast::DefinitionItem;
use franken_markdown::text::{Font, bundled};
use franken_markdown::{
    Block, Document, FontAssetSlot, Inline, PageMargins, PdfASettings, PdfEmitOptions, PdfOptions,
    PdfPaginationStrategy, Profile, RenderWarning, parse_markdown, render_pdf,
    render_pdf_document_emitted, render_pdf_document_pdfa, render_pdf_document_reported,
    render_pdf_document_with_diagnostics, render_pdf_with_diagnostics,
};

fn options() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

// Inspect objects through the actual xref so binary font/image payloads cannot
// impersonate page, annotation or structure dictionaries.
fn objects(pdf: &[u8]) -> Vec<&[u8]> {
    let marker = b"startxref\n";
    let position = pdf
        .windows(marker.len())
        .rposition(|bytes| bytes == marker)
        .unwrap();
    let xref: usize = std::str::from_utf8(&pdf[position + marker.len()..])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut rows = std::str::from_utf8(&pdf[xref..]).unwrap().lines();
    assert_eq!(rows.next(), Some("xref"));
    let subsection: Vec<_> = rows.next().unwrap().split_whitespace().collect();
    assert_eq!(subsection[0], "0");
    let count: usize = subsection[1].parse().unwrap();
    assert_eq!(rows.next().unwrap().trim(), "0000000000 65535 f");
    let offsets: Vec<usize> = rows
        .take(count - 1)
        .map(|row| {
            let fields: Vec<_> = row.split_whitespace().collect();
            assert_eq!(&fields[1..], &["00000", "n"]);
            fields[0].parse().unwrap()
        })
        .collect();
    assert_eq!(offsets.len(), count - 1);
    offsets
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = offsets
                .iter()
                .copied()
                .filter(|&offset| offset > start)
                .min()
                .unwrap_or(xref);
            let header = format!("{} 0 obj\n", index + 1);
            assert!(pdf[start..end].starts_with(header.as_bytes()));
            &pdf[start + header.len()..end]
        })
        .collect()
}

fn dictionary(object: &[u8]) -> &str {
    let end = object
        .windows(8)
        .position(|bytes| bytes == b"\nstream\n")
        .unwrap_or(object.len());
    std::str::from_utf8(&object[..end]).unwrap()
}

fn number_after(dictionary: &str, key: &str) -> usize {
    dictionary
        .split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn page_contents(pdf: &[u8]) -> Vec<String> {
    let objects = objects(pdf);
    objects
        .iter()
        .map(|object| dictionary(object))
        .filter(|dictionary| dictionary.starts_with("<< /Type /Page "))
        .map(|page| {
            let object = objects[number_after(page, "/Contents ") - 1];
            let dictionary = dictionary(object);
            let start = object
                .windows(8)
                .position(|bytes| bytes == b"\nstream\n")
                .unwrap()
                + 8;
            let payload = &object[start..start + number_after(dictionary, "/Length ")];
            let bytes = if dictionary.contains("/FlateDecode") {
                franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
            } else {
                assert!(!dictionary.contains("/Filter"));
                payload.to_vec()
            };
            String::from_utf8(bytes).unwrap()
        })
        .collect()
}

fn overflowing_formula(warnings: &[RenderWarning]) -> (String, u32, u32) {
    let fits: Vec<_> = warnings
        .iter()
        .filter_map(|warning| match warning {
            RenderWarning::MathOverflow {
                source,
                width_pt,
                measure_pt,
            } => Some((source.clone(), *width_pt, *measure_pt)),
            _ => None,
        })
        .collect();
    assert_eq!(fits.len(), 1, "one actual formula: {warnings:?}");
    fits.into_iter().next().unwrap()
}

#[test]
fn ordinary_diagnostics_retain_bytes_and_report_real_missing_resources() {
    let source = "# Result\n\n![Retained alternative](missing.png)\n\n$\\unknowncommand{x}$\n";
    let opts = options();
    let doc = parse_markdown(source);
    let original = doc.clone();
    let output = render_pdf_with_diagnostics(source, &opts).unwrap();
    assert_eq!(output.bytes, render_pdf(source, &opts).unwrap());
    let report = render_pdf_document_reported(&doc, &opts, PdfEmitOptions::default()).unwrap();
    assert_eq!(output.bytes, report.bytes);
    assert_eq!(output.pagination, report.pagination);
    assert_eq!(page_contents(&output.bytes).len(), 1);
    assert_eq!(
        output
            .warnings
            .iter()
            .filter(|warning| matches!(warning, RenderWarning::UnresolvedImage(dest) if dest == "missing.png"))
            .count(),
        1
    );
    assert!(output.warnings.iter().any(|warning| {
        matches!(warning, RenderWarning::MathFallback { source, .. } if source == r"\unknowncommand{x}")
    }));
    assert_eq!(doc, original, "diagnostics never rewrite source ownership");
    assert!(
        render_pdf_with_diagnostics("Clean text.", &opts)
            .unwrap()
            .warnings
            .is_empty()
    );
}

#[test]
fn profile_diagnostics_use_the_face_that_actually_paints_definition_terms() {
    let doc = Document {
        blocks: vec![Block::DefinitionList(vec![DefinitionItem {
            terms: vec![vec![Inline::Text("TermZ".into())]],
            definitions: vec![vec![Inline::Text("definition".into())]],
        }])],
    };
    let original = doc.clone();
    // The regular face contains these letters; a valid restricted bold face
    // does not. GFM-plus renders terms in bold, CommonMark renders source prose.
    let bold = Font::parse_static(bundled::PLEX_BOLD).unwrap();
    let mut opts = options();
    opts.font_assets
        .set_slot(FontAssetSlot::BodyBold, bold.subset(&[' ']).unwrap())
        .unwrap();
    opts.profile = Some(Profile::GfmPlus);
    let rich = render_pdf_document_with_diagnostics(
        &doc,
        &opts,
        PdfASettings::OFF,
        PdfEmitOptions::default(),
    )
    .unwrap();
    assert!(rich.warnings.iter().any(|warning| {
        matches!(warning, RenderWarning::MissingGlyphs { count: 5, sample } if sample == "TermZ")
    }), "the negative control must really use the restricted face: {:?}", rich.warnings);

    opts.profile = Some(Profile::CommonMarkGfm);
    let plain = render_pdf_document_with_diagnostics(
        &doc,
        &opts,
        PdfASettings::OFF,
        PdfEmitOptions::default(),
    )
    .unwrap();
    assert!(
        plain.warnings.is_empty(),
        "unused bold face cannot cause a warning: {:?}",
        plain.warnings
    );
    assert_eq!(
        plain.bytes,
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap()
    );
    assert_eq!(doc, original);
}

#[test]
fn shadowed_note_definitions_do_not_report_resources_absent_from_the_output() {
    let mut doc = parse_markdown("Citation[^n].\n\n[^n]: The retained note.\n");
    doc.blocks.push(Block::FootnoteDefinition {
        id: "n".into(),
        blocks: parse_markdown("![Shadowed](never-rendered.png)\n").blocks,
    });
    let original = doc.clone();
    let opts = options();
    let rendered = render_pdf_document_with_diagnostics(
        &doc,
        &opts,
        PdfASettings::OFF,
        PdfEmitOptions::default(),
    )
    .unwrap();
    assert!(
        rendered.warnings.is_empty(),
        "first definition wins in the emitted PDF: {:?}",
        rendered.warnings
    );
    assert_eq!(
        rendered.bytes,
        render_pdf("Citation[^n].\n\n[^n]: The retained note.\n", &opts).unwrap()
    );
    assert_eq!(doc, original);
}

#[test]
fn fitted_math_diagnostics_follow_the_final_font_size_and_measure() {
    // Keep the full source within the warning excerpt limit without changing
    // the mathematical spacing or the deliberately oversized geometry.
    let formula = ["x"; 40].join("+");
    let rows = (0..10)
        .map(|row| format!("Body row {row}.  \n"))
        .collect::<String>();
    let source = format!("{rows}${formula}$\n");
    let mut opts = options();
    opts.theme.page.size.width_pt = 360.0;
    opts.theme.page.size.height_pt = 240.0;
    opts.theme.page.margins = PageMargins {
        top_pt: 48.0,
        right_pt: 48.0,
        bottom_pt: 48.0,
        left_pt: 48.0,
    };
    let ordinary = render_pdf_with_diagnostics(&source, &opts).unwrap();
    assert!(page_contents(&ordinary.bytes).len() > 1);
    let (_, original_width, original_measure) = overflowing_formula(&ordinary.warnings);
    assert_eq!(original_measure, 264);

    opts.fit_to_pages = Some(1);
    let fitted = render_pdf_with_diagnostics(&source, &opts).unwrap();
    let content = page_contents(&fitted.bytes);
    assert_eq!(
        content.len(),
        1,
        "the fixture must exercise a successful fit"
    );
    let (actual_source, width, measure) = overflowing_formula(&fitted.warnings);
    assert_eq!(actual_source, formula);
    assert!(
        width < original_width,
        "diagnose the final reduced font size"
    );
    assert!(
        measure > original_measure,
        "diagnose the actual fitted margins"
    );
    assert!(width > measure, "the formula still requires a local fit");
    let sizes: Vec<f32> = pdf_content::operations(&content[0])
        .into_iter()
        .filter(|operation| operation.operator == "Tf")
        .map(|operation| operation.operands[1].parse().unwrap())
        .collect();
    assert!(!sizes.is_empty() && sizes.iter().all(|&size| size < 11.0));
    assert_eq!(fitted.bytes, render_pdf(&source, &opts).unwrap());
    let monolithic = render_pdf_document_with_diagnostics(
        &parse_markdown(&source),
        &opts,
        PdfASettings::OFF,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(fitted.bytes, monolithic.bytes);
    assert_eq!(fitted.pagination, monolithic.pagination);
    assert_eq!(fitted.warnings, monolithic.warnings);
}

#[test]
fn ordinary_note_links_keep_diagnostics_and_actual_bound_destinations() {
    let source = "Citation[^evidence]. Read [the note](#fn-evidence).\n\n[^evidence]: ![Lost figure](note-missing.png)\n";
    let doc = parse_markdown(source);
    let mut opts = options();
    opts.footnote_backlinks = true;
    let rendered = render_pdf_document_with_diagnostics(
        &doc,
        &opts,
        PdfASettings::OFF,
        PdfEmitOptions::default(),
    )
    .unwrap();
    assert_eq!(
        rendered.warnings,
        vec![RenderWarning::UnresolvedImage("note-missing.png".into())]
    );
    let objects = objects(&rendered.bytes);
    let links: Vec<_> = objects
        .iter()
        .map(|object| dictionary(object))
        .filter(|dictionary| dictionary.contains("/Subtype /Link"))
        .collect();
    assert!(
        links.len() >= 3,
        "citation, ordinary note link, and return link"
    );
    for link in links {
        assert!(link.contains("/Dest ["), "source link must bind: {link}");
        assert!(!link.contains("/S /URI"));
        let page = number_after(link, "/Dest [");
        assert!(dictionary(objects[page - 1]).starts_with("<< /Type /Page "));
    }
    assert!(!String::from_utf8_lossy(&rendered.bytes).contains("fmd-book-pdf-nav/"));
    let monolithic = render_pdf_document_with_diagnostics(
        &doc,
        &opts,
        PdfASettings::OFF,
        PdfEmitOptions::monolithic(),
    )
    .unwrap();
    assert_eq!(rendered.bytes, monolithic.bytes);
    assert_eq!(rendered.warnings, monolithic.warnings);
    assert_eq!(rendered.pagination, monolithic.pagination);
    assert_eq!(rendered.bytes, render_pdf(source, &opts).unwrap());
}

#[test]
fn pagination_warning_and_strict_refusal_describe_the_same_failed_plan() {
    let mut opts = options();
    opts.optimal_pagination = true;
    opts.theme.page.size.width_pt = 400.0;
    opts.theme.page.size.height_pt = 112.0;
    opts.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    // Three lines cannot split into two fragments that each retain two lines.
    let source = "A  \nB  \nC";
    let fallback = render_pdf_with_diagnostics(source, &opts).unwrap();
    assert_eq!(
        fallback.pagination.page_breaks.strategy,
        PdfPaginationStrategy::Legacy
    );
    assert!(page_contents(&fallback.bytes).len() > 1);
    let reason = fallback.pagination.page_breaks.fallback.as_ref().unwrap();
    assert_eq!(reason.code(), "infeasible");
    assert_eq!(
        fallback.warnings,
        vec![RenderWarning::PaginationFallback {
            strategy: PdfPaginationStrategy::Legacy,
            reason: reason.clone(),
        }]
    );
    opts.strict_pagination = true;
    let error = render_pdf_with_diagnostics(source, &opts).unwrap_err();
    assert!(
        matches!(error, franken_markdown::RenderError::Pagination(actual) if &actual == reason)
    );
}

#[test]
fn archival_output_and_original_source_validation_survive_diagnostic_collection() {
    let source = "Citation[^n]. [Read](#fn-n).\n\n[^n]: ![Absent](archival-missing.png)\n";
    let doc = parse_markdown(source);
    let opts = options();
    let rendered = render_pdf_document_with_diagnostics(
        &doc,
        &opts,
        PdfASettings::a2b_strict(),
        PdfEmitOptions::default(),
    )
    .unwrap();
    assert_eq!(
        rendered.bytes,
        render_pdf_document_pdfa(&doc, &opts, PdfASettings::a2b_strict()).unwrap()
    );
    assert_eq!(
        rendered.warnings,
        vec![RenderWarning::UnresolvedImage(
            "archival-missing.png".into()
        )]
    );
    assert!(
        objects(&rendered.bytes)
            .iter()
            .any(|object| dictionary(object).contains("/OutputIntents"))
    );

    let mut malicious = doc;
    // A duplicate definition is absent from publication. Strict archival
    // validation must nevertheless inspect the original caller-owned source.
    malicious.blocks.push(Block::FootnoteDefinition {
        id: "n".into(),
        blocks: vec![Block::Paragraph(vec![Inline::Link {
            dest: "javascript:alert(1)".into(),
            title: None,
            content: vec![Inline::Text("hidden action".into())],
        }])],
    });
    for emit in [PdfEmitOptions::default(), PdfEmitOptions::monolithic()] {
        let error = render_pdf_document_with_diagnostics(
            &malicious,
            &opts,
            PdfASettings::a2b_strict(),
            emit,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("pdf_a_javascript_uri"),
            "{error}"
        );
    }
}
