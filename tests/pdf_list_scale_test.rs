//! PDF list text, labels and hanging indents follow the resolved body size.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::pdf::{VerifyTextRun, verification_text_layer};
use franken_markdown::{
    PageMargins, PageSize, PdfImageAsset, PdfOptions, parse_markdown, render_pdf,
};

fn text_runs(source: &str, opts: &PdfOptions) -> Vec<VerifyTextRun> {
    verification_text_layer(&parse_markdown(source), opts)
        .unwrap()
        .pages
        .into_iter()
        .flat_map(|page| page.runs)
        .filter(|run| !run.text.is_empty())
        .collect()
}

#[test]
fn all_list_paragraphs_and_labels_use_the_resolved_body_size() {
    let source = "Outside paragraph.\n\n- Outer **strong** text.\n\n  Second paragraph.\n\n  - Nested item.\n- [x] Complete task.\n- [ ] Open task.\n\n3. Ordered item.\n";
    let default = PdfOptions::default();
    let explicit_default = PdfOptions {
        base_font_size: Some(11.0),
        ..PdfOptions::default()
    };
    assert_eq!(
        render_pdf(source, &default).unwrap(),
        render_pdf(source, &explicit_default).unwrap()
    );

    for size in [6.0, 16.5, 22.0, 24.0] {
        let opts = PdfOptions {
            base_font_size: Some(size),
            ..PdfOptions::default()
        };
        let runs = text_runs(source, &opts);
        let text = runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for expected in [
            "Outside", "Outer", "Second", "Nested", "Complete", "Open", "Ordered", "[x]", "[ ]",
            "3.",
        ] {
            assert!(
                text.contains(expected),
                "missing {expected} at {size}pt: {text}"
            );
        }
        for run in runs {
            assert_eq!(run.size, size, "unscaled list run: {run:?}");
            assert!(
                run.overshoot.is_none(),
                "scaled run exceeds measure: {run:?}"
            );
        }
    }
}

#[test]
fn hanging_and_nested_list_indents_scale_with_the_marker_gutter() {
    let source = "1. First.\n\n   Second.\n\n   - Nested.\n";
    let base = text_runs(source, &PdfOptions::default());
    let large = text_runs(
        source,
        &PdfOptions {
            base_font_size: Some(22.0),
            ..PdfOptions::default()
        },
    );
    for needle in ["1.", "Second.", "Nested."] {
        let before = base.iter().find(|run| run.text.contains(needle)).unwrap();
        let after = large.iter().find(|run| run.text.contains(needle)).unwrap();
        let left = PdfOptions::default().theme.page.margins.left_pt;
        assert!(
            (after.x - left - 2.0 * (before.x - left)).abs() < 0.03,
            "{needle}: hanging indent does not follow doubled type: {before:?} -> {after:?}"
        );
    }
    let marker = large.iter().find(|run| run.text.starts_with("1.")).unwrap();
    let continuation = large.iter().find(|run| run.text == "Second.").unwrap();
    let nested = large
        .iter()
        .find(|run| run.text.contains("Nested."))
        .unwrap();
    assert!(marker.x < continuation.x && continuation.x < nested.x);
}

#[test]
fn scaling_list_type_changes_real_wrapping_and_page_count_without_losing_items() {
    let source = "- alpha beta gamma delta epsilon zeta eta theta iota kappa.\n".repeat(18);
    let document = parse_markdown(&source);
    let mut previous = None;
    for size in [6.0, 11.0, 22.0] {
        let mut opts = PdfOptions {
            base_font_size: Some(size),
            ..PdfOptions::default()
        };
        opts.theme.page.size = PageSize {
            name: "list-scale-test",
            width_pt: 220.0,
            height_pt: 240.0,
        };
        opts.theme.page.margins = PageMargins {
            top_pt: 20.0,
            right_pt: 20.0,
            bottom_pt: 20.0,
            left_pt: 20.0,
        };
        let layer = verification_text_layer(&document, &opts).unwrap();
        let runs: Vec<_> = layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .filter(|run| !run.text.is_empty())
            .collect();
        assert_eq!(
            runs.iter().filter(|run| run.text.starts_with('•')).count(),
            18
        );
        assert!(
            runs.iter()
                .all(|run| run.size == size && run.overshoot.is_none())
        );
        if let Some((pages, lines)) = previous {
            assert!(
                layer.page_count > pages,
                "{size}pt lists must repaginate at their actual size"
            );
            assert!(
                runs.len() > lines,
                "{size}pt lists must wrap at their actual size"
            );
        }
        previous = Some((layer.page_count, runs.len()));
    }
}

#[test]
fn image_list_markers_scale_without_resizing_the_figure() {
    let source = "- [x] ![Architecture](figure.svg)";
    for size in [6.0, 22.0] {
        let opts = PdfOptions {
            base_font_size: Some(size),
            image_assets: vec![PdfImageAsset::new(
                "figure.svg",
                include_bytes!("fixtures/pdf/linked-figure.svg").to_vec(),
            )],
            ..PdfOptions::default()
        };
        let runs = text_runs(source, &opts);
        let marker = runs.iter().find(|run| run.kind == "image").unwrap();
        assert_eq!(marker.text, "[x]");
        assert_eq!(marker.size, size);
        let pdf = render_pdf(source, &opts).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        let bbox: Vec<f32> = text
            .split_once("/BBox [")
            .unwrap()
            .1
            .split_once(']')
            .unwrap()
            .0
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert!((bbox[2] - bbox[0] - 120.0).abs() < 0.02);
        assert!((bbox[3] - bbox[1] - 90.0).abs() < 0.02);
        assert!((marker.y - (bbox[3] - size * 1.32)).abs() < 0.02);
        assert!(marker.x < bbox[0]);
    }
}
