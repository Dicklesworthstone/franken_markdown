//! Real ZIP/XHTML checks for timeline figures in single and book EPUBs.
use super::{assert_well_formed_xml, entry_text};
use franken_markdown::book::BookRenderer;
use franken_markdown::{BookInput, HtmlOptions, epub, parse_markdown};

fn source(direction: &str) -> String {
    format!(
        "# Release plan\n\n\x60\x60\x60timeline\ntimeline {direction}\ntitle Plan & deliver\nsection Research\nFirst : Read & write : Compare ideas\nsection Release\nNext : Review<br>Then ship\n     : Publish docs\n\x60\x60\x60\n"
    )
}

#[test]
fn timelines_reach_single_and_book_epubs_as_complete_declared_svg() {
    for direction in ["LR", "TD"] {
        let source = source(direction);
        let doc = parse_markdown(&source);
        let options = HtmlOptions::default();
        let single = epub::render_epub(&doc, &options).unwrap();
        assert_eq!(single, epub::render_epub(&doc, &options).unwrap());
        let renderer = BookRenderer::new(&[BookInput {
            path: "roadmap.md".into(),
            source: source.clone(),
        }])
        .unwrap();
        let book = renderer.render_epub().unwrap();
        assert_eq!(book, renderer.render_epub().unwrap());
        for archive in [single, book] {
            let chapter = entry_text(&archive, "OEBPS/chapter-1.xhtml");
            assert_well_formed_xml(&chapter);
            assert_eq!(
                chapter
                    .matches("class=\"fmd-diagram fmd-timeline\"")
                    .count(),
                1
            );
            for label in [
                "First",
                "Next",
                "Research",
                "Release",
                "Read &amp; write",
                "Compare ideas",
                "Review",
                "Then ship",
                "Publish docs",
            ] {
                assert!(
                    chapter.contains(&format!(">{label}</text>")),
                    "{label}: {chapter}"
                );
            }
            assert!(chapter.contains("<title>Plan &amp; deliver</title>"));
            assert!(!chapter.contains("language-timeline"));
            assert!(!chapter.contains("<script"));
            let manifest = entry_text(&archive, "OEBPS/content.opf");
            assert_well_formed_xml(&manifest);
            assert!(manifest.contains("properties=\"svg\""));
        }
    }
}

#[test]
fn timeline_entities_cannot_inject_active_markup_into_epub_xhtml() {
    let source = "# Safe labels\n\n\x60\x60\x60mermaid\ntimeline\ntitle &lt;script&gt;\nPeriod : &lt;img onerror=run()&gt; : A &#58; B\n\x60\x60\x60\n";
    let archive = epub::render_epub(&parse_markdown(source), &HtmlOptions::default()).unwrap();
    let chapter = entry_text(&archive, "OEBPS/chapter-1.xhtml");
    assert_well_formed_xml(&chapter);
    assert!(chapter.contains("class=\"fmd-diagram fmd-timeline\""));
    assert!(chapter.contains("&lt;script&gt;"));
    assert!(chapter.contains("&lt;img onerror=run()&gt;"));
    assert!(chapter.contains(">A : B</text>"));
    assert!(!chapter.contains("<script"));
    assert!(!chapter.contains("<img onerror"));
}

#[test]
fn refused_timeline_syntax_preserves_every_event_in_the_epub_code_block() {
    let source = "# Original source\n\n\x60\x60\x60timeline\ntimeline\nFirst : Keep this event\nSecond : Keep this too\nstyle First fill:red\n\x60\x60\x60\n";
    let archive = epub::render_epub(&parse_markdown(source), &HtmlOptions::default()).unwrap();
    let chapter = entry_text(&archive, "OEBPS/chapter-1.xhtml");
    assert_well_formed_xml(&chapter);
    assert!(chapter.contains("language-timeline"));
    assert!(!chapter.contains("class=\"fmd-diagram fmd-timeline\""));
    for line in [
        "First : Keep this event",
        "Second : Keep this too",
        "style First fill:red",
    ] {
        assert!(chapter.contains(line), "{line}: {chapter}");
    }
    let manifest = entry_text(&archive, "OEBPS/content.opf");
    assert!(!manifest.contains("properties=\"svg\""));
}
