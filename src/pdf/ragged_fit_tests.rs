//! Exercise the production parser, block policy, reusable optimizer, and PDF
//! segment painter together. Core line-break tests alone cannot detect a
//! renderer forgetting to pass its justification policy into the optimizer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn page_with_measure(opts: &PdfOptions, measure: f32) -> PageGeom {
    let mut page = PageGeom::from_theme(&opts.theme);
    page.width = page.left + measure + page.right;
    page.content_w = measure;
    page
}

fn text(line: &Line) -> String {
    line.segs.iter().map(|seg| seg.text.as_str()).collect()
}

fn visible_lines(lines: &[Line]) -> Vec<&Line> {
    lines.iter().filter(|line| !line.segs.is_empty()).collect()
}

fn painted_right(line: &Line, faces: &Faces) -> f32 {
    let seg = line.segs.last().expect("line has visible text");
    let natural = faces.shaped_width_points(seg.slot, seg.text.trim_end(), line.size);
    seg.x + natural * (1.0 + f32::from(seg.expansion_permille) / 1000.0)
}

fn assert_painted_fit(lines: &[Line], faces: &Faces, page: PageGeom, opts: &PdfOptions) {
    for line in visible_lines(lines) {
        let tail = line.segs.last().unwrap().text.trim_end();
        let credit = if line.flow.kind == FlowKind::Paragraph {
            crate::layout::protrusion_for_boundary_chars(
                None,
                tail.strip_suffix('-').unwrap_or(tail).chars().next_back(),
                font_size_of(line.size),
                opts.microtype,
            )
            .right
            .to_points_f32()
        } else {
            0.0
        };
        let edge = painted_right(line, faces);
        assert!(
            edge <= page.right_x() + credit + 0.05,
            "{:?} line overhangs by {:.3}pt: {:?}",
            line.flow.kind,
            edge - page.right_x() - credit,
            text(line),
        );
    }
}

#[test]
fn ragged_headings_fit_without_unpainted_glue_compression() -> crate::Result<()> {
    for family in [
        crate::theme::FontFamily::Sans,
        crate::theme::FontFamily::Serif,
    ] {
        for flags in 0..8 {
            let mut opts = PdfOptions {
                microtype: crate::layout::MicrotypeOptions::CONSERVATIVE,
                gradual_demerits: flags & 1 != 0,
                river_penalty: flags & 2 != 0,
                pareto_line_breaking: flags & 4 != 0,
                ..PdfOptions::default()
            };
            opts.theme.font = family;
            let faces = Faces::load(&opts)?;
            let size = opts.type_scale().h[2];
            // The first two words fit only if the breaker credits half the
            // interword shrink; the ragged PDF painter applies none of it.
            let prefix = faces.shaped_width_points(F_BOLD, "alphabet beta", size);
            let space = faces.shaped_width_points(F_BOLD, " ", size);
            let page = page_with_measure(&opts, prefix - space / 6.0);
            let doc = crate::parse_markdown("### alphabet beta gamma");
            let lines = layout(&doc.blocks, &opts, &faces, page);
            let visible = visible_lines(&lines);
            assert_eq!(visible.len(), 2, "the fixture must wrap into two lines");
            assert_eq!(text(visible[0]), "alphabet");
            assert_eq!(text(visible[1]), "beta gamma");
            assert_painted_fit(&lines, &faces, page, &opts);
            assert!(
                visible
                    .iter()
                    .all(|line| line.segs.iter().all(|seg| { seg.expansion_permille == 0 }))
            );
        }
    }
    Ok(())
}

#[test]
fn shared_pdf_scratch_switches_between_ragged_headings_and_justified_body() -> crate::Result<()> {
    let opts = PdfOptions::default();
    let faces = Faces::load(&opts)?;
    let size = opts.type_scale().h[2];
    let page = page_with_measure(
        &opts,
        faces.shaped_width_points(F_BOLD, "alphabet beta", size)
            - faces.shaped_width_points(F_BOLD, " ", size) / 6.0,
    );
    let blocks = [
        "### alphabet beta gamma",
        "The broad river runs through the town and past the old stone bridge. The boats wait near the bank.",
        "### alphabet beta gamma",
        "The broad river runs through the town and past the old stone bridge. The boats wait near the quay.",
    ];
    let doc = crate::parse_markdown(&blocks.join("\n\n"));
    let actual = layout(&doc.blocks, &opts, &faces, page);
    let mut groups: Vec<Vec<&Line>> = Vec::new();
    for line in visible_lines(&actual) {
        if groups
            .last()
            .is_none_or(|group| group[0].flow.group != line.flow.group)
        {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(line);
    }
    assert_eq!(groups.len(), blocks.len());
    let mut justified_lines = 0;
    for (markdown, group) in blocks.iter().zip(&groups) {
        // Every block in the shared workspace must have the same painting as
        // a fresh renderer. Distinct body tails prevent a paragraph-cache hit
        // from hiding a missing reset when a heading precedes a paragraph.
        let standalone = crate::parse_markdown(markdown);
        let expected = layout(&standalone.blocks, &opts, &faces, page);
        let expected = visible_lines(&expected);
        assert_eq!(group.len(), expected.len());
        for (actual_line, expected_line) in group.iter().zip(expected) {
            assert_eq!(text(actual_line), text(expected_line));
            assert!(
                (painted_right(actual_line, &faces) - painted_right(expected_line, &faces)).abs()
                    < 0.001
            );
        }
        if group[0].flow.kind == FlowKind::Paragraph {
            assert!(group.len() >= 3);
            for line in &group[..group.len() - 1] {
                let line_text = text(line);
                if line_text.split_whitespace().count() < 2 {
                    // A single fragment has no word space to stretch. Its
                    // natural right edge is still checked by assert_painted_fit.
                    continue;
                }
                justified_lines += 1;
                // These ordinary multiword lines still use real word-space
                // justification after the preceding ragged heading.
                assert!(
                    (painted_right(line, &faces) - page.right_x()).abs() < 0.05,
                    "unjustified body: {:?}",
                    line_text
                );
            }
        }
    }
    assert!(
        justified_lines >= 4,
        "fixture must exercise justification after both ragged headings"
    );
    assert_painted_fit(&actual, &faces, page, &opts);
    Ok(())
}

#[test]
fn paragraph_tails_hard_breaks_and_list_tails_fit_at_natural_width() -> crate::Result<()> {
    for microtype in [
        crate::layout::MicrotypeOptions::DISABLED,
        crate::layout::MicrotypeOptions::CONSERVATIVE,
    ] {
        let opts = PdfOptions {
            microtype,
            ..PdfOptions::default()
        };
        let faces = Faces::load(&opts)?;
        let size = opts.type_scale().body;
        let measure = faces.shaped_width_points(F_BODY, "alphabet beta", size)
            - faces.shaped_width_points(F_BODY, " ", size) / 6.0;
        for markdown in [
            "alphabet beta",
            "alphabet beta  \nshort end",
            "- alphabet beta",
        ] {
            let doc = crate::parse_markdown(markdown);
            let indent = if let Some(Block::List(list)) = doc.blocks.first() {
                let (_, marker_col) =
                    list_marker_layouts(list, &faces, size, &RefCell::new(WidthCache::default()));
                marker_col + 11.0 * (size / 11.0)
            } else {
                0.0
            };
            let page = page_with_measure(&opts, measure + indent);
            let lines = layout(&doc.blocks, &opts, &faces, page);
            assert!(visible_lines(&lines).len() >= 2, "must wrap {markdown:?}");
            assert_painted_fit(&lines, &faces, page, &opts);
        }
    }
    Ok(())
}

#[test]
fn multipage_report_keeps_ragged_headings_inside_the_print_measure() -> crate::Result<()> {
    let opts = PdfOptions::default();
    let faces = Faces::load(&opts)?;
    let mut page = page_with_measure(&opts, 468.0);
    page.height = 792.0;
    page.top = 72.0;
    page.bottom = 72.0;
    let section = "### Expert 2: Former Minas Gerais environmental regulator (Semad/Feam/SUPRAM, agency)\n\n\
        The company reported revenue, margin and cash flow as the court weighed the appeal \
        and analysts debated the refinancing. Management said the restart would come within \
        days, yet the regulator suspended five licences and the prosecutor sought penalties. \
        Investors weighed the refinancing, the offtake prepayments and the court calendar.\n\n\
        - The regulator reviewed the environmental record and the agency report.\n\n";
    let doc = crate::parse_markdown(&section.repeat(40));
    let lines = layout(&doc.blocks, &opts, &faces, page);
    let pages = paginate_lines(&lines, page);
    assert!(
        pages.len() >= 8,
        "exercise repeated blocks across a long report"
    );
    assert_painted_fit(&lines, &faces, page, &opts);
    assert_eq!(
        visible_lines(&lines)
            .iter()
            .filter(|line| { line.flow.kind == FlowKind::Heading && line.flow.index == 0 })
            .count(),
        40,
    );
    Ok(())
}
