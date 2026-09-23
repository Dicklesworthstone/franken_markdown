#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn narrow_options() -> PdfOptions {
    let mut opts = PdfOptions::default();
    opts.theme.page.size.width_pt = 240.0;
    opts.theme.page.size.height_pt = 220.0;
    opts.theme.page.margins = crate::theme::PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    opts
}

fn linked_to(line: &Line, id: &str) -> bool {
    line.segs
        .iter()
        .any(|seg| matches!(&seg.link, Some(LinkTarget::Fragment(target)) if target == id))
}

fn assert_within_columns(lines: &[&Line], page: PageGeom) {
    for line in lines {
        let mut previous_right = page.left;
        for seg in &line.segs {
            assert!(
                seg.x + 0.01 >= previous_right,
                "overlapping TOC runs: {}",
                seg.text
            );
            assert!(
                seg.x + seg.width <= page.right_x() + 0.01,
                "TOC run escapes the right margin: {}",
                seg.text
            );
            previous_right = seg.x + seg.width;
        }
    }
}

#[test]
fn long_titles_wrap_before_the_page_number_and_keep_all_text() {
    let title = "A complete guide to deterministic document rendering with fonts, figures, tables and accessible links";
    let doc = crate::parse_markdown(&format!("[[TOC]]\n\n# {title}\n\nBody."));
    let opts = narrow_options();
    let page = PageGeom::from_theme(&opts.theme);
    let faces = Faces::load(&opts).unwrap();
    let lines = layout(&doc.blocks, &opts, &faces, page);
    let id = &collect_pdf_toc_entries(&doc.blocks)[0].id;
    let toc: Vec<_> = lines.iter().filter(|line| linked_to(line, id)).collect();
    assert!(toc.len() >= 3, "long TOC entries must span measured lines");
    assert_within_columns(&toc, page);
    assert!(toc.iter().all(|line| line.flow.group == toc[0].flow.group));
    let text: String = toc
        .iter()
        .enumerate()
        .flat_map(|(index, line)| {
            let title_runs = if index + 1 == toc.len() {
                line.segs.len() - 1
            } else {
                line.segs.len()
            };
            line.segs[..title_runs]
                .iter()
                .filter(|seg| seg.fill != Fill::Muted)
                .flat_map(|seg| seg.text.chars().filter(|ch| !ch.is_whitespace()))
        })
        .collect();
    assert_eq!(
        text,
        title
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
    );
    let pages = paginate_lines(&lines, page);
    let heading_page = pages
        .iter()
        .position(|placed| placed.iter().any(|p| p.line.flow.kind == FlowKind::Heading))
        .unwrap()
        + 1;
    assert_eq!(
        toc.last().unwrap().segs.last().unwrap().text,
        heading_page.to_string()
    );
}

#[test]
fn deep_long_unbroken_titles_fit_narrow_pages_at_large_type_sizes() {
    let title = "UnbrokenIdentifier".repeat(15);
    let doc = crate::parse_markdown(&format!("[[TOC]]\n\n###### {title}\n"));
    let mut opts = narrow_options();
    opts.theme.page.size.width_pt = 160.0;
    opts.theme = opts.theme.with_font_scale(crate::FontScale::from_factor(2.0));
    let page = PageGeom::from_theme(&opts.theme);
    let faces = Faces::load(&opts).unwrap();
    let lines = layout(&doc.blocks, &opts, &faces, page);
    let id = &collect_pdf_toc_entries(&doc.blocks)[0].id;
    let toc: Vec<_> = lines.iter().filter(|line| linked_to(line, id)).collect();
    assert!(toc.len() > 10);
    assert_within_columns(&toc, page);
    assert!(
        paginate_lines(&lines, page).len() > 1,
        "an oversized entry must paginate"
    );
}

#[test]
fn multi_page_toc_numbers_match_heading_destinations() {
    let mut source = "[[TOC]]\n\n".to_string();
    for index in 0..24 {
        source.push_str(&format!(
            "## Chapter {index}: A detailed introduction to rendering long technical documents\n\n"
        ));
    }
    let doc = crate::parse_markdown(&source);
    let opts = narrow_options();
    let page = PageGeom::from_theme(&opts.theme);
    let faces = Faces::load(&opts).unwrap();
    let lines = layout(&doc.blocks, &opts, &faces, page);
    let metadata = heading_metadata(&lines);
    let pages = paginate_lines(&lines, page);
    let mut actual_pages = BTreeMap::new();
    for (index, placed) in pages.iter().enumerate() {
        for p in placed {
            if let Some(heading) = metadata.get(&p.line.flow.group) {
                actual_pages.entry(heading.id.clone()).or_insert(index + 1);
            }
        }
    }
    assert!(pages.len() >= 10);
    for entry in collect_pdf_toc_entries(&doc.blocks) {
        let toc: Vec<_> = lines
            .iter()
            .filter(|line| linked_to(line, &entry.id))
            .collect();
        assert!(toc.len() >= 2);
        assert_within_columns(&toc, page);
        assert_eq!(
            toc.last().unwrap().segs.last().unwrap().text,
            actual_pages[&entry.id].to_string(),
            "wrong TOC destination for {}",
            entry.title
        );
    }
    assert_eq!(
        crate::render_pdf(&source, &opts).unwrap(),
        crate::render_pdf(&source, &opts).unwrap()
    );
}
