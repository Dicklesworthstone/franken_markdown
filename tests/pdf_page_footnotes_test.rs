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
    assert_notes_follow_their_references(&PdfOptions::default());
}

#[test]
fn optimal_pagination_reserves_note_space_with_the_citing_line() {
    assert_notes_follow_their_references(&PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    });
}

fn assert_notes_follow_their_references(opts: &PdfOptions) {
    let report = report(&long_document(), opts);
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
fn page_notes_are_deterministic_across_emitters() {
    let doc = parse_markdown(&long_document());
    let opts = PdfOptions::default();
    let monolithic = render_pdf_document(&doc, &opts).unwrap();
    assert_eq!(render_pdf_document(&doc, &opts).unwrap(), monolithic);
    assert_eq!(
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap(),
        monolithic
    );
    let optimal = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    let planned = render_pdf_document(&doc, &optimal).unwrap();
    assert_eq!(render_pdf_document(&doc, &optimal).unwrap(), planned);
    assert_eq!(
        render_pdf_document_emitted(&doc, &optimal, PdfEmitOptions::default()).unwrap(),
        planned
    );
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

fn paragraph_note(id: &str, count: usize) -> String {
    format!(
        "[^{id}]: {}",
        (0..count)
            .map(|index| format!("{}{index:02} explanatory paragraph.", id.to_uppercase()))
            .collect::<Vec<_>>()
            .join("\n\n    "),
    )
}

fn crowded_notes(count: usize) -> String {
    format!(
        "Claim.[^a][^b][^c]\n\n{}\n",
        ["a", "b", "c"]
            .map(|id| paragraph_note(id, count))
            .join("\n\n"),
    )
}

/// Resolve the actual annotation destinations through the emitted page tree.
/// This detects a dropped link even when the source-level anchor audit passes.
fn emitted_destinations(pdf: &[u8]) -> Vec<(usize, f32)> {
    let text = String::from_utf8_lossy(pdf);
    let kids = text
        .split("/Type /Pages ")
        .nth(1)
        .expect("page tree")
        .split("/Kids [")
        .nth(1)
        .unwrap()
        .split(']')
        .next()
        .unwrap();
    let pages: Vec<usize> = kids
        .split_whitespace()
        .collect::<Vec<_>>()
        .chunks_exact(3)
        .map(|reference| reference[0].parse().unwrap())
        .collect();
    text.split("/Subtype /Link")
        .skip(1)
        .map(|annotation| {
            let object = annotation.split("endobj").next().unwrap();
            let destination: Vec<&str> = object
                .split("/Dest [")
                .nth(1)
                .expect("every emitted note mark must have a destination")
                .split(']')
                .next()
                .unwrap()
                .split_whitespace()
                .collect();
            let page: usize = destination[0].parse().unwrap();
            assert_eq!(&destination[1..5], &["0", "R", "/XYZ", "null"]);
            (
                pages.iter().position(|&object| object == page).unwrap() + 1,
                destination[5].parse().unwrap(),
            )
        })
        .collect()
}

fn assert_destinations_match_notes(pdf: &[u8], report: &VerifyReport, targets: &[&str]) {
    let destinations = emitted_destinations(pdf);
    assert_eq!(destinations.len(), targets.len());
    let mut expected: Vec<_> = targets
        .iter()
        .map(|target| {
            let (page, baseline, size) = locate(report, target);
            (page, baseline + size * 0.9)
        })
        .collect();
    for (page, y) in destinations {
        let index = expected
            .iter()
            .position(|&(expected_page, expected_y)| {
                page == expected_page && (y - expected_y).abs() < 0.01
            })
            .unwrap_or_else(|| {
                panic!("unexpected destination page={page} y={y}, expected {expected:?}")
            });
        expected.remove(index);
    }
    assert!(expected.is_empty());
}

#[test]
fn several_notes_on_one_line_fit_below_the_body_without_losing_content() {
    // On Letter, each 13-paragraph note is individually below the 40% limit.
    // Together they previously put 28 glyphs outside the page's top edge.
    let markdown = crowded_notes(13);
    for optimal_pagination in [false, true] {
        let opts = PdfOptions {
            optimal_pagination,
            ..PdfOptions::default()
        };
        let report = report(&markdown, &opts);
        let text: String = report
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .map(|run| run.text.as_str())
            .collect();
        for id in ['A', 'B', 'C'] {
            for index in 0..13 {
                let marker = format!("{id}{index:02}");
                assert_eq!(
                    text.matches(&marker).count(),
                    1,
                    "retain {marker} exactly once"
                );
            }
        }
        assert!(
            text.contains("Notes"),
            "the over-subscribed note becomes an endnote"
        );
        assert_eq!(locate(&report, "A00").0, locate(&report, "Claim").0);
        assert_eq!(locate(&report, "B00").0, locate(&report, "Claim").0);
        assert!(locate(&report, "A00").2 < 11.0);
        assert!(locate(&report, "B00").2 < 11.0);
        assert_eq!(locate(&report, "C00").2, 11.0);
        for page in &report.pages {
            let lowest_body = page
                .runs
                .iter()
                .filter(|run| !run.text.is_empty() && run.size >= 11.0)
                .map(|run| run.y - run.size * 0.3)
                .fold(f32::INFINITY, f32::min);
            for run in page.runs.iter().filter(|run| !run.text.is_empty()) {
                assert!(
                    run.y >= 72.0 && run.y + run.size <= 720.0,
                    "out of bounds: {run:?}"
                );
                if run.size < 11.0 {
                    assert!(
                        run.y + run.size < lowest_body,
                        "note overlaps body: {run:?}"
                    );
                }
            }
        }
        let pdf = render_pdf(&markdown, &opts).unwrap();
        assert_destinations_match_notes(&pdf, &report, &["A00", "B00", "C00"]);
        assert_eq!(pdf, render_pdf(&markdown, &opts).unwrap());
        assert_eq!(
            pdf,
            render_pdf_document_emitted(
                &parse_markdown(&markdown),
                &opts,
                PdfEmitOptions::monolithic(),
            )
            .unwrap(),
        );
    }
}

#[test]
fn oversized_notes_retain_actual_link_destinations_in_the_notes_section() {
    let markdown = format!("Claim.[^long]\n\n{}\n", paragraph_note("long", 40));
    for optimal_pagination in [false, true] {
        let opts = PdfOptions {
            optimal_pagination,
            ..PdfOptions::default()
        };
        let report = report(&markdown, &opts);
        assert!(report.page_count >= 2);
        assert_eq!(locate(&report, "LONG00").2, 11.0);
        assert!(locate(&report, "LONG39").0 >= locate(&report, "Notes").0);
        let pdf = render_pdf(&markdown, &opts).unwrap();
        assert_destinations_match_notes(&pdf, &report, &["LONG00"]);
        let raw = String::from_utf8_lossy(&pdf);
        assert_eq!(
            raw.matches("/Title (").count(),
            1,
            "only the Notes heading is a bookmark"
        );
    }
}

#[test]
fn note_to_note_links_resolve_through_endnote_fallback_and_cycles() {
    let markdown = "Claim.[^first]\n\n[^first]: FIRSTNOTE cites another note.[^second]\n\n[^second]: SECONDNOTE refers back.[^first]\n";
    for optimal_pagination in [false, true] {
        let opts = PdfOptions {
            optimal_pagination,
            ..PdfOptions::default()
        };
        let report = report(markdown, &opts);
        assert!(locate(&report, "FIRSTNOTE").2 < 11.0);
        assert_eq!(locate(&report, "SECONDNOTE").2, 11.0);
        let pdf = render_pdf(markdown, &opts).unwrap();
        assert_destinations_match_notes(&pdf, &report, &["FIRSTNOTE", "SECONDNOTE", "FIRSTNOTE"]);
    }
}

#[test]
fn note_admission_accounts_for_the_final_gap_before_an_endnote_heading() {
    let source = crowded_notes(8);
    for optimal_pagination in [false, true] {
        let mut opts = PdfOptions {
            optimal_pagination,
            ..PdfOptions::default()
        };
        // 485pt of body height: all three notes fit with their citation, but
        // the extra gap before a necessary Notes section needs more space.
        opts.theme.page.size.height_pt = 629.0;
        let ordinary = report(&source, &opts);
        assert!(locate(&ordinary, "C00").2 < 11.0);
        let with_endnotes = format!("{source}\n[^unreferenced]: UNREFERENCED note.\n");
        let report = report(&with_endnotes, &opts);
        assert!(locate(&report, "A00").2 < 11.0);
        assert!(locate(&report, "B00").2 < 11.0);
        assert_eq!(locate(&report, "C00").2, 11.0);
        assert_eq!(locate(&report, "UNREFERENCED").2, 11.0);
        let pdf = render_pdf(&with_endnotes, &opts).unwrap();
        assert_destinations_match_notes(&pdf, &report, &["A00", "B00", "C00"]);
    }
}

#[test]
fn nonparagraph_endnotes_anchor_their_preserved_note_label() {
    let code = (0..40)
        .map(|index| format!("    CODE{index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let markdown = format!("Claim.[^code]\n\n[^code]:\n\n    ```text\n{code}\n    ```\n");
    let opts = PdfOptions::default();
    let report = report(&markdown, &opts);
    let label = report
        .pages
        .iter()
        .find_map(|page| {
            page.runs
                .iter()
                .find(|run| run.text.trim() == "¹")
                .map(|run| (page.number, run.y, run.size))
        })
        .expect("the note label precedes its code block");
    assert_eq!(label.2, 11.0);
    assert!(locate(&report, "CODE39").0 >= label.0);
    let destinations = emitted_destinations(&render_pdf(&markdown, &opts).unwrap());
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].0, label.0);
    assert!((destinations[0].1 - (label.1 + label.2 * 0.9)).abs() < 0.01);
}

#[test]
fn ordinary_page_note_pdf_bytes_remain_unchanged() {
    // Captured from the existing CLI before mh6w (PDF source at ef89e0b).
    // The subsequent 1031326 merge changed only fmd-math. No golden refresh.
    let source = "Claim.[^one]\n\n[^one]: One short note.\n";
    for optimal_pagination in [false, true] {
        let opts = PdfOptions {
            optimal_pagination,
            metadata_epoch_seconds: Some(0),
            ..PdfOptions::default()
        };
        let pdf = render_pdf(source, &opts).unwrap();
        let digest = pdf.iter().fold(0xcbf29ce484222325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        assert_eq!(pdf.len(), 5012);
        assert_eq!(digest, 0x8c3f241426c69767);
    }
}
