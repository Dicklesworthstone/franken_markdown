//! PDF footnotes print at the foot of the page that carries their first body
//! reference, below the body text, at footnote size, with a working link from
//! the mark to the note. Notes that cannot sit at a page foot (unreferenced,
//! or taller than a page foot can hold) keep the trailing Notes section.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::verify::{self, VerifyReport};
use franken_markdown::{
    PdfEmitOptions, PdfOptions, parse_markdown, render_pdf, render_pdf_document,
    render_pdf_document_emitted,
};

/// Forty paragraphs over several pages, each with its own note.
fn long_document() -> String {
    let mut body = String::from("# Study\n\n");
    let mut notes = String::new();
    for index in 0..40 {
        body.push_str(&format!(
            "PARA{index} {} closes with a note.[^n{index}]\n\n",
            "lorem ipsum dolor sit amet ".repeat(12)
        ));
        notes.push_str(&format!(
            "[^n{index}]: NOTE{index} explains paragraph {index} briefly.\n"
        ));
    }
    body + &notes
}

fn report(markdown: &str, opts: &PdfOptions) -> VerifyReport {
    verify::verify_pdf(&parse_markdown(markdown), opts).unwrap()
}

/// `(page number, baseline y, size)` of the first run containing `needle`.
fn locate(report: &VerifyReport, needle: &str) -> (usize, f32, f32) {
    report
        .pages
        .iter()
        .find_map(|page| {
            page.runs
                .iter()
                .find(|run| run.text.contains(needle))
                .map(|run| (page.number, run.y, run.size))
        })
        .unwrap_or_else(|| panic!("{needle} not found"))
}

#[test]
fn each_note_sits_below_the_body_of_the_page_with_its_reference() {
    let opts = PdfOptions::default();
    let report = report(&long_document(), &opts);
    assert!(report.page_count > 3, "the fixture must really paginate");
    for index in 0..40 {
        let (note_page, note_y, note_size) = locate(&report, &format!("NOTE{index} "));
        // The mark ends the paragraph's last line.
        let mark_page = report
            .pages
            .iter()
            .find(|page| {
                page.runs.iter().any(|run| {
                    run.text.contains("closes with a note.")
                        && body_index_before(&report, page.number, run.y) == Some(index)
                })
            })
            .map(|page| page.number);
        assert_eq!(mark_page, Some(note_page), "note {index}");
        let page = &report.pages[note_page - 1];
        let lowest_body = page
            .runs
            .iter()
            .filter(|run| !run.text.is_empty() && !run.text.contains("NOTE"))
            .map(|run| run.y)
            .fold(f32::INFINITY, f32::min);
        assert!(note_y < lowest_body, "note {index} is below the body text");
        let (_, _, body_size) = locate(&report, &format!("PARA{index} "));
        assert!(note_size < body_size, "footnote size is smaller than body");
    }
    let text: String = report
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect();
    assert!(!text.contains("Notes"), "no trailing Notes section");
}

/// The paragraph whose `PARA` line is the nearest at or above `y` on `page`.
fn body_index_before(report: &VerifyReport, page: usize, y: f32) -> Option<usize> {
    report.pages[page - 1]
        .runs
        .iter()
        .filter(|run| run.y >= y)
        .filter_map(|run| {
            let rest = run.text.strip_prefix("PARA")?;
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            Some((run.y, digits.parse::<usize>().ok()?))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, index)| index)
        // A paragraph continued from the previous page starts there.
        .or_else(|| (page > 1).then(|| body_index_before(report, page - 1, f32::MIN))?)
}

#[test]
fn marks_link_to_their_notes_without_adding_bookmarks() {
    let pdf = render_pdf(
        "# Only heading\n\nClaim.[^a]\n\n[^a]: The note.\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/Subtype /Link").count(), 1);
    let annot = raw.split("/Subtype /Link").nth(1).unwrap();
    let annot = &annot[..annot.find("endobj").unwrap()];
    assert!(annot.contains("/Dest ["), "the mark resolves: {annot}");
    assert!(raw.contains("/Count 1 >>"), "one bookmark: the heading");
    assert!(!raw.contains("/Title ()"), "notes are not bookmarks");
}

#[test]
fn unreferenced_and_oversized_notes_keep_the_trailing_notes_section() {
    let opts = PdfOptions::default();
    let tall = (0..40)
        .map(|index| format!("TALL{index} line of a very long note."))
        .collect::<Vec<_>>()
        .join("\n\n    ");
    let markdown = format!(
        "Short[^short] and tall[^tall].\n\n[^short]: SHORTNOTE.\n\n[^tall]: {tall}\n\n[^orphan]: ORPHANNOTE.\n"
    );
    let report = report(&markdown, &opts);
    let (short_page, short_y, _) = locate(&report, "SHORTNOTE");
    let (heading_page, heading_y, _) = locate(&report, "Notes");
    assert_eq!(short_page, 1);
    assert!(
        heading_page > 1 || heading_y > short_y,
        "the page note sits below the Notes section on page 1"
    );
    let (tall_page, _, _) = locate(&report, "TALL39");
    let (orphan_page, _, _) = locate(&report, "ORPHANNOTE");
    assert!(tall_page >= heading_page && orphan_page >= heading_page);
}

#[test]
fn page_notes_are_deterministic_across_emitters_and_optimal_pagination() {
    let doc = parse_markdown(&long_document());
    let opts = PdfOptions::default();
    let monolithic = render_pdf_document(&doc, &opts).unwrap();
    assert_eq!(render_pdf_document(&doc, &opts).unwrap(), monolithic);
    assert_eq!(
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap(),
        monolithic
    );
    // The page planners do not model note space, so optimal pagination keeps
    // the greedy breaks for documents with page notes.
    let optimal = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    assert_eq!(render_pdf_document(&doc, &optimal).unwrap(), monolithic);
}

#[test]
fn endnote_preparation_still_prints_every_note_in_a_notes_section() {
    let doc = parse_markdown("Claim.[^a]\n\n[^a]: ENDNOTEBODY.\n");
    let report = verify::verify_pdf(&doc.with_endnotes(), &PdfOptions::default()).unwrap();
    let (heading_page, heading_y, _) = locate(&report, "Notes");
    let (note_page, note_y, _) = locate(&report, "ENDNOTEBODY");
    assert_eq!((heading_page, note_page), (1, 1));
    assert!(note_y < heading_y);
}
