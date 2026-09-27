#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn small_options() -> PdfOptions {
    let mut opts = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = 400.0;
    opts.theme.page.size.height_pt = 220.0;
    opts.theme.page.margins = crate::theme::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

fn hard_lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("Body line {index}"))
        .collect::<Vec<_>>()
        .join("  \n")
}

/// Read actual emitted page object references, without depending on the
/// renderer's object-number allocation or its in-memory page count.
fn emitted_page_objects(pdf: &[u8]) -> Vec<usize> {
    let text = String::from_utf8_lossy(pdf);
    let pages = text.split("/Type /Pages ").nth(1).expect("page tree");
    let kids = pages
        .split("/Kids [")
        .nth(1)
        .unwrap()
        .split(']')
        .next()
        .unwrap();
    kids.split_whitespace()
        .collect::<Vec<_>>()
        .chunks_exact(3)
        .map(|reference| {
            assert_eq!(&reference[1..], &["0", "R"]);
            reference[0].parse().unwrap()
        })
        .collect()
}

fn emitted_heading_page(pdf: &[u8], title: &str) -> usize {
    let pages = emitted_page_objects(pdf);
    let text = String::from_utf8_lossy(pdf);
    let title_entry = format!("/Title {} /Parent ", pdf_text_string(title));
    let outline = text
        .split(&title_entry)
        .nth(1)
        .expect("emitted outline for heading")
        .split("endobj")
        .next()
        .unwrap();
    let object: usize = outline
        .split("/Dest [")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    pages.iter().position(|&page| page == object).unwrap() + 1
}

#[test]
fn optimal_toc_numbers_match_emitted_heading_destinations() {
    let mut source = String::from("[[TOC]]\n\n");
    for index in 0..8 {
        source.push_str(&format!("## Chapter {index}\n\n{}\n\n", hard_lines(1)));
    }
    let doc = crate::parse_markdown(&source);
    let mut opts = small_options();
    opts.theme.page.size.height_pt = 175.0;
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let lines = layout(&doc.blocks, &opts, &faces, page);
    let greedy_pages = paginate_lines(&lines, page);
    let optimal_pages = paginate_lines_for_options(&lines, page, &opts);
    let metadata = heading_metadata(&lines);
    let heading_pages = |pages: &[Vec<Placed<'_>>]| {
        let mut map = BTreeMap::new();
        for (page_index, placed) in pages.iter().enumerate() {
            for p in placed {
                if let Some(heading) = metadata.get(&p.line.flow.group) {
                    map.entry(heading.id.clone()).or_insert(page_index + 1);
                }
            }
        }
        map
    };
    assert_ne!(
        heading_pages(&greedy_pages),
        heading_pages(&optimal_pages),
        "fixture must expose the difference between greedy and optimal pages"
    );

    let chunked = crate::render_pdf(&source, &opts).unwrap();
    let monolithic = render_with_emit(
        &doc,
        &opts,
        crate::PdfASettings::default(),
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        chunked, monolithic,
        "both PDF emitters follow the same plan"
    );
    assert_eq!(chunked, crate::render_pdf(&source, &opts).unwrap());

    for entry in collect_pdf_toc_entries(&doc.blocks) {
        let printed_page = lines
            .iter()
            .filter(|line| {
                line.segs.iter().any(
                    |seg| matches!(&seg.link, Some(LinkTarget::Fragment(id)) if *id == entry.id),
                )
            })
            .filter_map(|line| line.segs.last())
            .next_back()
            .unwrap()
            .text
            .parse::<usize>()
            .unwrap();
        assert_eq!(
            printed_page,
            emitted_heading_page(&chunked, &entry.title),
            "TOC and actual destination disagree for {}",
            entry.title
        );
    }
}

#[test]
fn optimal_fit_measures_the_pages_that_will_actually_be_emitted() {
    // Ten hard lines fit the greedy page, but the exact-height
    // planner retains the paragraph's trailing gap and needs a second page.
    // The old fitter returned early after counting only greedy pages.
    let source = hard_lines(10);
    let doc = crate::parse_markdown(&source);
    let mut opts = small_options();
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let lines = layout(&doc.blocks, &opts, &faces, page);
    assert_eq!(paginate_lines(&lines, page).len(), 1);
    assert_eq!(paginate_lines_for_options(&lines, page, &opts).len(), 2);
    assert_eq!(
        emitted_page_objects(&crate::render_pdf(&source, &opts).unwrap()).len(),
        2
    );

    opts.fit_to_pages = Some(1);
    let (effective, effective_page) = effective_pdf_options(&doc, &opts, &faces);
    assert!(effective.base_font_size.unwrap() < 11.0);
    let fitted_lines = layout(&doc.blocks, &effective, &faces, effective_page);
    assert_eq!(
        paginate_lines_for_options(&fitted_lines, effective_page, &effective).len(),
        1
    );
    let pdf = crate::render_pdf(&source, &opts).unwrap();
    assert_eq!(
        emitted_page_objects(&pdf).len(),
        1,
        "fitted PDF meets its target"
    );

    let verified = verification_text_layer(&doc, &opts).unwrap();
    assert_eq!(verified.page_count, 1);
    assert_eq!(
        verified.pages[0].runs.len(),
        10,
        "fitting retains every line"
    );
    assert!(verified.pages[0].runs.iter().all(|run| run.size < 11.0));
    let mut explicit = effective;
    explicit.fit_to_pages = None;
    assert_eq!(
        verified,
        verification_text_layer(&doc, &explicit).unwrap(),
        "verification reports the fitted positions and font sizes"
    );
}

#[test]
fn verification_uses_optimal_pages_before_fitting() {
    let source = hard_lines(10);
    let doc = crate::parse_markdown(&source);
    let mut opts = small_options();
    let verified = verification_text_layer(&doc, &opts).unwrap();
    let pdf = crate::render_pdf(&source, &opts).unwrap();
    assert_eq!(verified.page_count, 2);
    assert_eq!(verified.page_count, emitted_page_objects(&pdf).len());
    assert_eq!(
        verified
            .pages
            .iter()
            .map(|page| page.runs.len())
            .sum::<usize>(),
        10
    );

    opts.optimal_pagination = false;
    opts.fit_to_pages = Some(0);
    let verified = verification_text_layer(&doc, &opts).unwrap();
    let pdf = crate::render_pdf(&source, &opts).unwrap();
    assert_eq!(verified.page_count, 1);
    assert_eq!(verified.page_count, emitted_page_objects(&pdf).len());
}
