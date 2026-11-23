//! Chapter regressions reuse the parent test's independent xref, outline and
//! painted-glyph readers. No layout helper predicts an expected destination.

use super::*;

fn chapter(path: &str, title: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: format!("---\ntitle: {title}\n---\n{source}"),
    }
}

fn render(inputs: &[BookInput], toc: bool) -> Vec<u8> {
    BookRenderer::new(inputs)
        .unwrap()
        .render_pdf_with_options(
            &PdfOptions {
                toc,
                ..Default::default()
            },
            PdfASettings::OFF,
        )
        .unwrap()
}

#[test]
fn metadata_chapter_roots_exist_without_links_or_visible_title_headings() {
    for linked in [false, true] {
        let first = if linked {
            "# Alpha\n\nFirst body.\n\n[CHAPTERJUMP](two.md) [DETAILJUMP](two.md#beta-detail)"
        } else {
            "# Alpha\n\nFirst body."
        };
        let pdf = render(
            &[
                chapter("one.md", "Alpha Chapter", first),
                chapter(
                    "two.md",
                    "Beta Chapter",
                    "Second introduction.\n\n## Beta detail\n\nSecond detail.",
                ),
            ],
            false,
        );
        let objects = Objects::new(&pdf);
        let bookmarks = assert_tree(
            &objects,
            &["Alpha Chapter", "Alpha", "Beta Chapter", "Beta detail"],
            &[None, Some(0), None, Some(2)],
            &[1, 0, 1, 0],
        );
        assert_eq!(objects.pages().len(), 2);
        let runs = painted_runs(&objects);
        let body: String = runs.iter().map(|run| run.text.as_str()).collect();
        assert!(
            !body.contains("Alpha Chapter") && !body.contains("Beta Chapter"),
            "chapter metadata must not insert visible source headings"
        );
        assert_destination_at_run(
            bookmarks[2].destination,
            run_containing(&runs, "Second introduction."),
        );
        assert_destination_at_run(
            bookmarks[3].destination,
            run_containing(&runs, "Beta detail"),
        );
        assert!(bookmarks[2].destination.y > bookmarks[3].destination.y);
        if linked {
            assert_eq!(
                link_at_run(&objects, run_containing(&runs, "CHAPTERJUMP")),
                bookmarks[2].destination
            );
            assert_eq!(
                link_at_run(&objects, run_containing(&runs, "DETAILJUMP")),
                bookmarks[3].destination
            );
        }
        assert!(!String::from_utf8_lossy(&pdf).contains("fmd-book-pdf-nav/"));
    }
}

#[test]
fn heading_free_chapters_have_clickable_contents_with_real_printed_page_numbers() {
    let pdf = render(
        &[
            chapter("one.md", "ALPHA_TITLE", "FIRST_BODY"),
            chapter("two.md", "BETA_TITLE", "SECOND_BODY"),
        ],
        true,
    );
    let objects = Objects::new(&pdf);
    let bookmarks = assert_tree(
        &objects,
        &["ALPHA_TITLE", "BETA_TITLE"],
        &[None, None],
        &[0, 0],
    );
    let pages = objects.pages();
    let runs = painted_runs(&objects);
    for (index, (title, body)) in [("ALPHA_TITLE", "FIRST_BODY"), ("BETA_TITLE", "SECOND_BODY")]
        .iter()
        .enumerate()
    {
        let title_run = run_containing(&runs, title);
        assert_eq!(
            runs.iter().filter(|run| run.text.contains(title)).count(),
            1
        );
        let body_run = run_containing(&runs, body);
        assert_destination_at_run(bookmarks[index].destination, body_run);
        assert_eq!(
            link_at_run(&objects, title_run),
            bookmarks[index].destination
        );
        let printed = pages
            .iter()
            .position(|&page| page == body_run.page)
            .unwrap()
            + 1;
        let numeral = runs
            .iter()
            .find(|run| {
                run.page == title_run.page
                    && (run.y - title_run.y).abs() < 0.01
                    && run.x > title_run.x
                    && run.text == printed.to_string()
            })
            .expect("contents row must print its actual destination page");
        assert_eq!(link_at_run(&objects, numeral), bookmarks[index].destination);
    }
}

#[test]
fn empty_chapters_keep_valid_landings_without_manufacturing_body_pages() {
    let pdf = render(
        &[
            chapter("first.md", "Empty before", ""),
            chapter(
                "body.md",
                "Body chapter",
                "BODY_TEXT\n\n[BEFORE](first.md) [AFTER](last.md)",
            ),
            chapter("last.md", "Empty after", ""),
        ],
        false,
    );
    let objects = Objects::new(&pdf);
    let bookmarks = assert_tree(
        &objects,
        &["Empty before", "Body chapter", "Empty after"],
        &[None, None, None],
        &[0, 0, 0],
    );
    assert_eq!(objects.pages().len(), 1);
    let runs = painted_runs(&objects);
    let body = run_containing(&runs, "BODY_TEXT");
    assert_destination_at_run(bookmarks[0].destination, body);
    assert_eq!(bookmarks[0].destination, bookmarks[1].destination);
    assert_eq!(bookmarks[2].destination.page, body.page);
    assert_eq!(
        link_at_run(&objects, run_containing(&runs, "BEFORE")),
        bookmarks[0].destination
    );
    assert_eq!(
        link_at_run(&objects, run_containing(&runs, "AFTER")),
        bookmarks[2].destination
    );
    let body_text: String = runs.iter().map(|run| run.text.as_str()).collect();
    assert!(!body_text.contains("Empty") && !body_text.contains("Body chapter"));
}

#[test]
fn entirely_empty_books_still_emit_requested_chapter_contents() {
    let pdf = render(
        &[
            chapter("first.md", "Empty before", ""),
            chapter("last.md", "Empty after", ""),
        ],
        true,
    );
    let objects = Objects::new(&pdf);
    let bookmarks = assert_tree(
        &objects,
        &["Empty before", "Empty after"],
        &[None, None],
        &[0, 0],
    );
    assert_eq!(objects.pages().len(), 1);
    let runs = painted_runs(&objects);
    for bookmark in &bookmarks {
        let row = run_containing(&runs, &bookmark.title);
        assert_eq!(link_at_run(&objects, row), bookmark.destination);
    }
}

#[test]
fn chapter_title_fidelity_findings_describe_only_metadata_that_is_painted() {
    let book = BookRenderer::new(&[chapter("one.md", "Title \u{13000}", "Plain body.")]).unwrap();
    for toc in [false, true] {
        let options = PdfOptions {
            toc,
            ..Default::default()
        };
        let report = book
            .render_pdf_with_options_reported(&options, PdfASettings::OFF)
            .unwrap();
        assert_eq!(
            report.bytes,
            book.render_pdf_with_options(&options, PdfASettings::OFF)
                .unwrap()
        );
        assert_eq!(report.warnings.iter().any(|warning| matches!(warning,
            franken_markdown::RenderWarning::MissingGlyphs { sample, .. } if sample.contains('\u{13000}')
        )), toc, "a metadata bookmark needs no glyphs; a painted contents title does");
    }
}

#[test]
fn include_content_and_note_headings_keep_their_source_chapter_after_page_breaks() {
    let book = BookRenderer::from_sources(&[
        chapter("one.md", "First metadata", "FIRST_START\n\n{{#include parts/content.md}}\n\nCitation.[^n]\n\n[^n]: # Note heading\n\n    Note body."),
        chapter("two.md", "Second metadata", "SECOND_START\n\n<!-- pagebreak -->\n\n## Detail\n\n[INCLUDEDJUMP](one.md#detail) [LOCALJUMP](#detail)"),
    ], &[BookInput { path: "parts/content.md".into(), source: "## Detail\n\nINCLUDED_BODY".into() }]).unwrap();
    let source = book.book().clone();
    let mut options = PdfOptions {
        optimal_pagination: true,
        ..Default::default()
    };
    options.theme.page.baseline_grid =
        Some(franken_markdown::BaselineGrid::from_points(14.0).unwrap());
    let pdf = book
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    let objects = Objects::new(&pdf);
    let bookmarks = assert_tree(
        &objects,
        &[
            "First metadata",
            "Detail",
            "Note heading",
            "Second metadata",
            "Detail",
        ],
        &[None, Some(0), Some(0), None, Some(3)],
        &[2, 0, 0, 1, 0],
    );
    assert_eq!(
        objects.pages().len(),
        3,
        "authored breaks must not create extra chapters"
    );
    let runs = painted_runs(&objects);
    assert_destination_at_run(
        bookmarks[0].destination,
        run_containing(&runs, "FIRST_START"),
    );
    assert_destination_at_run(
        bookmarks[3].destination,
        run_containing(&runs, "SECOND_START"),
    );
    assert_eq!(
        link_at_run(&objects, run_containing(&runs, "INCLUDEDJUMP")),
        bookmarks[1].destination
    );
    assert_eq!(
        link_at_run(&objects, run_containing(&runs, "LOCALJUMP")),
        bookmarks[4].destination
    );
    assert_eq!(source.chapters.len(), book.book().chapters.len());
    for (before, after) in source.chapters.iter().zip(&book.book().chapters) {
        assert_eq!(before.doc, after.doc);
    }
    assert_eq!(
        pdf,
        book.render_pdf_with_options(&options, PdfASettings::OFF)
            .unwrap()
    );
}

#[test]
fn chapter_metadata_does_not_capture_author_heading_fragments_or_change_heading_tags() {
    let pdf = render(
        &[
            chapter(
                "one.md",
                "Repeated",
                "ONE_START\n\n## Repeated\n\n### Child\n\n[OTHER](two.md#repeated) [SELF](#repeated)",
            ),
            chapter(
                "two.md",
                "Repeated",
                "TWO_START\n\n## Repeated\n\n###### Deepest\n\n[ROOT](one.md)",
            ),
        ],
        true,
    );
    let objects = Objects::new(&pdf);
    let bookmarks = assert_tree(
        &objects,
        &[
            "Repeated", "Repeated", "Child", "Repeated", "Repeated", "Deepest",
        ],
        &[None, Some(0), Some(1), None, Some(3), Some(4)],
        &[2, 1, 0, 2, 1, 0],
    );
    let runs = painted_runs(&objects);
    assert_eq!(
        link_at_run(&objects, run_containing(&runs, "OTHER")),
        bookmarks[4].destination
    );
    assert_eq!(
        link_at_run(&objects, run_containing(&runs, "SELF")),
        bookmarks[1].destination
    );
    assert_eq!(
        link_at_run(&objects, run_containing(&runs, "ROOT")),
        bookmarks[0].destination
    );
    let dictionaries: String = (1..objects.offsets.len())
        .map(|id| objects.dictionary(id))
        .collect();
    assert_eq!(dictionaries.matches("/S /H2 ").count(), 2);
    assert_eq!(dictionaries.matches("/S /H6 ").count(), 1);
    assert!(
        !dictionaries.contains("/S /H1 "),
        "chapter metadata must not create a tagged heading"
    );
}
