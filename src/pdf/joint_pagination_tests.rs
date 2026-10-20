#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

const PROSE: &str = "The old stone bridge stood beside the river while the town woke slowly. \
    A small boat crossed the clear water and a child watched from the steps. \
    The morning light reached the houses along the bank and the shop doors opened. \
    People walked to work, shared news with friends, and stopped to hear the birds. \
    By noon the market was full of fresh bread, bright fruit, and flowers from the hills.";

fn options_for(width: f32, height: f32) -> PdfOptions {
    let mut opts = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = width + 72.0;
    opts.theme.page.size.height_pt = height + 72.0;
    opts.theme.page.margins = crate::theme::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

fn unselected(
    doc: &Document,
    opts: &PdfOptions,
    faces: &Faces,
    page: PageGeom,
) -> (Vec<Line>, Candidates) {
    let mut cx = LayoutCx {
        opts,
        pending_page_break: false,
        type_scale: opts.type_scale(),
        faces,
        page,
        next_bg: 0,
        next_flow: 0,
        list_stack: Vec::new(),
        hyphen_cache: RefCell::new(HashMap::new()),
        width_cache: RefCell::new(WidthCache::default()),
        simple_paragraph_cache: SimpleParagraphLayoutCache::default(),
        table_layout_cache: TableLayoutCache::default(),
        math_renderer: crate::pdf::math::MathRenderer::default(),
        paragraph_scratch: ParagraphLayoutScratch::new(),
        paragraph_candidates: Candidates::new(opts, &doc.blocks),
        line_breaks: Vec::new(),
        line_toks: Vec::new(),
        glue_adjustments: Vec::new(),
        code_highlight_spans: Vec::new(),
        links: LinkIntern::default(),
        toc_entries: Vec::new(),
        toc_page_map: BTreeMap::new(),
    };
    let mut lines = Vec::new();
    layout_blocks(&doc.blocks, 0.0, &mut lines, &mut cx);
    layout_pdf_notes(&doc.blocks, &mut lines, &mut cx);
    (lines, cx.paragraph_candidates)
}

fn baseline_pdf(lines: &[Line], opts: &PdfOptions, faces: &Faces, page: PageGeom) -> Vec<u8> {
    serialize(
        lines,
        opts,
        faces,
        page,
        None,
        crate::PdfASettings::OFF,
        PdfEmitOptions::default(),
        &mut PdfProfiler::disabled(),
    )
    .unwrap()
}

fn compactable_prose() -> String {
    [PROSE; 3].join(" ")
}

/// Inspect emitted objects independently of the layout's page-count helpers.
fn emitted_pages(pdf: &[u8]) -> Vec<usize> {
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
    kids.split_whitespace()
        .collect::<Vec<_>>()
        .chunks_exact(3)
        .map(|reference| {
            assert_eq!(&reference[1..], &["0", "R"]);
            reference[0].parse().unwrap()
        })
        .collect()
}

/// The writer intentionally gives a split block fresh structure containers on
/// each page. Check the actual paragraph-to-root ancestry on every emitted
/// page, rather than assuming one container per source quote for the document.
fn assert_quote_structure_on_every_page(pdf: &[u8], depth: usize) {
    let text = String::from_utf8_lossy(pdf);
    let needle = " 0 obj\n<< /Type /StructElem /S /";
    let mut elements = BTreeMap::new();
    for (offset, _) in text.match_indices(needle) {
        let object: usize = text[..offset]
            .rsplit(|ch: char| !ch.is_ascii_digit())
            .find(|part| !part.is_empty())
            .unwrap()
            .parse()
            .unwrap();
        let body = text[offset + needle.len()..].split("\nendobj").next().unwrap();
        let tag = body.split_whitespace().next().unwrap();
        let reference = |key: &str| {
            body.split(key)
                .nth(1)
                .map(|tail| tail.split_whitespace().next().unwrap().parse::<usize>().unwrap())
        };
        elements.insert(object, (tag, reference(" /P ").unwrap(), reference(" /Pg ")));
    }
    let pages = emitted_pages(pdf);
    let mut quotes_by_page = BTreeMap::<usize, BTreeSet<usize>>::new();
    for &(tag, parent, page) in elements.values() {
        if tag != "P" {
            continue;
        }
        let page = page.expect("paragraph has an emitted page");
        assert!(pages.contains(&page));
        let mut cursor = parent;
        let mut ancestors = BTreeSet::new();
        let mut quotes = BTreeSet::new();
        loop {
            assert!(ancestors.insert(cursor), "acyclic structure ancestry");
            let &(tag, parent, _) = elements.get(&cursor).expect("existing structure parent");
            if tag == "Document" {
                break;
            }
            assert_eq!(tag, "BlockQuote", "only quote containers enclose this prose");
            quotes.insert(cursor);
            cursor = parent;
        }
        assert_eq!(quotes.len(), depth, "exact quote nesting on page {page}");
        quotes_by_page.entry(page).or_default().extend(quotes);
    }
    assert_eq!(quotes_by_page.len(), pages.len(), "every page contains quoted prose");
    for page in &pages {
        assert_eq!(quotes_by_page[page].len(), depth);
    }
    let used: BTreeSet<_> = quotes_by_page.values().flatten().copied().collect();
    assert_eq!(used.len(), depth * pages.len(), "fresh containers per page");
    let emitted: BTreeSet<_> = elements.iter()
        .filter_map(|(&object, &(tag, _, _))| (tag == "BlockQuote").then_some(object))
        .collect();
    assert_eq!(used, emitted, "no unreferenced or duplicate quote containers");
}

fn emitted_heading_page(pdf: &[u8], title: &str) -> usize {
    let pages = emitted_pages(pdf);
    let text = String::from_utf8_lossy(pdf);
    let entry = format!("/Title {} /Parent ", pdf_text_string(title));
    let destination: usize = text
        .split(&entry)
        .nth(1)
        .expect("heading outline")
        .split("endobj")
        .next()
        .unwrap()
        .split("/Dest [")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    pages.iter().position(|&page| page == destination).unwrap() + 1
}

/// Recover this fixture's prose, allowing only layout-inserted end hyphens.
/// The source contains no literal hyphens, so this cannot hide a lost word.
fn rejoin_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> String {
    let mut joined = String::new();
    for line in lines {
        let line = line.trim();
        joined.push_str(line);
        if line.ends_with(['-', '\u{00ad}', '\u{2010}']) {
            joined.pop();
        } else {
            joined.push(' ');
        }
    }
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn joint_pdf_selects_a_feasible_shorter_shape_and_saves_a_page() {
    let source = compactable_prose();
    let doc = crate::parse_markdown(&source);
    let opts = options_for(440.0, 196.0);
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (baseline, candidates) = unselected(&doc, &opts, &faces, page);
    assert_eq!(baseline.len(), 14);
    let shorter = candidates
        .paragraphs
        .values()
        .flat_map(|paragraph| &paragraph.alternatives)
        .find(|alternative| alternative.lines.len() == 13)
        .expect("the fixed paragraph has a feasible same-measure shorter shape");
    assert_eq!(shorter.extra_demerits, 13_455);
    assert!(shorter.extra_demerits < PAGE_COUNT_DEMERITS as i64);

    let baseline_plan = exact_height_page_breaks(&baseline, page).unwrap();
    assert_eq!(baseline_plan.ends.len(), 2);
    assert!(baseline_plan.ends[0] >= 2 && baseline.len() - baseline_plan.ends[0] >= 2);
    let original_pdf = baseline_pdf(&baseline, &opts, &faces, page);
    assert_eq!(emitted_pages(&original_pdf).len(), 2);

    // Exercise the production layout and public renderer, not just a synthetic
    // height-planner input. The candidate is installed before either consumes it.
    let selected = layout(&doc.blocks, &opts, &faces, page);
    assert_eq!(selected.len(), 13);
    assert_eq!(
        exact_height_page_breaks(&selected, page).unwrap().ends,
        [13]
    );
    assert!(selected.iter().all(|line| line.size == baseline[0].size));
    assert!(
        selected
            .iter()
            .all(|line| line_overshoot(line, &page).is_none())
    );
    for line in &selected[..selected.len() - 1] {
        let rightmost = line.segs.last().unwrap();
        assert!((rightmost.x + rightmost.width - (page.width - page.right)).abs() < 0.05);
    }
    let texts: Vec<String> = selected
        .iter()
        .map(|line| line.segs.iter().map(seg_display_text).collect())
        .collect();
    assert_eq!(rejoin_lines(texts.iter().map(String::as_str)), source);
    let pdf = crate::render_pdf(&source, &opts).unwrap();
    assert_eq!(emitted_pages(&pdf).len(), 1);
    assert_ne!(original_pdf, pdf);
}

#[test]
fn expensive_shorter_shape_does_not_buy_a_page_at_any_typographic_cost() {
    let source = [PROSE; 2].join(" ");
    let doc = crate::parse_markdown(&source);
    let mut opts = options_for(208.0, 700.0);
    let faces = Faces::load(&opts).unwrap();
    let (baseline, candidates) = unselected(&doc, &opts, &faces, PageGeom::from_theme(&opts.theme));
    assert_eq!(baseline.len(), 20);
    let shorter = candidates
        .paragraphs
        .values()
        .flat_map(|paragraph| &paragraph.alternatives)
        .find(|alternative| alternative.lines.len() == 19)
        .expect("the fixed paragraph also has an expensive shorter shape");
    assert!(shorter.extra_demerits > 100 * PAGE_COUNT_DEMERITS as i64);
    let shorter_height: f32 = shorter
        .lines
        .iter()
        .map(|line| line_leading(line) + line.gap_after)
        .sum();
    opts.theme.page.size.height_pt = 72.0 + shorter_height + 0.1;
    let page = PageGeom::from_theme(&opts.theme);
    assert!(select(&baseline, page, &candidates).is_none());
    assert_eq!(layout(&doc.blocks, &opts, &faces, page).len(), 20);
    assert_eq!(
        baseline_pdf(&baseline, &opts, &faces, page),
        crate::render_pdf(&source, &opts).unwrap(),
    );
}

#[test]
fn chosen_shape_preserves_links_metadata_text_and_fitting_without_scaling() {
    const URI: &str = "https://example.com/river";
    let prose = compactable_prose();
    let source = format!("[{prose}]({URI})");
    let doc = crate::parse_markdown(&source);
    let mut opts = options_for(440.0, 196.0);
    opts.title = Some(String::from("River study"));
    opts.author = Some(String::from("Proof reader"));
    opts.lang = Some(String::from("en-US"));
    opts.metadata_epoch_seconds = Some(0);
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let selected = layout(&doc.blocks, &opts, &faces, page);
    assert_eq!(selected.len(), 13);
    for (index, line) in selected.iter().enumerate() {
        assert_eq!(line.flow.kind, FlowKind::Paragraph);
        assert_eq!(line.flow.index, index);
        assert_eq!(line.flow.count, 13);
        for seg in &line.segs {
            assert_eq!(seg.link, Some(LinkTarget::Uri(URI.to_string())));
            assert_eq!(seg.fill, Fill::Link);
        }
    }

    let chunked = crate::render_pdf(&source, &opts).unwrap();
    assert_eq!(emitted_pages(&chunked).len(), 1);
    assert_eq!(chunked, crate::render_pdf(&source, &opts).unwrap());
    assert_eq!(
        chunked,
        crate::render_pdf_emitted(&source, &opts, PdfEmitOptions::monolithic()).unwrap(),
    );
    let pdf_text = String::from_utf8_lossy(&chunked);
    for expected in [
        "/URI (https://example.com/river)",
        "/Title (River study)",
        "/Author (Proof reader)",
        "/Lang (en-US)",
        "/CreationDate (D:19700101000000Z)",
        "/StructTreeRoot",
        "/S /P",
    ] {
        assert!(pdf_text.contains(expected), "missing {expected}");
    }
    let verified = verification_text_layer(&doc, &opts).unwrap();
    assert_eq!(verified.page_count, 1);
    assert_eq!(verified.pages[0].runs.len(), 13);
    assert_eq!(
        rejoin_lines(verified.pages[0].runs.iter().map(|run| run.text.as_str())),
        prose,
    );
    assert!(
        verified.pages[0]
            .runs
            .iter()
            .all(|run| run.size == 11.0 && run.overshoot.is_none())
    );

    opts.fit_to_pages = Some(1);
    assert_eq!(chunked, crate::render_pdf(&source, &opts).unwrap());
    assert_eq!(verified, verification_text_layer(&doc, &opts).unwrap());
}

#[test]
fn repeated_paragraphs_each_reflow_and_retain_forced_chapter_boundaries() {
    let paragraph = crate::parse_markdown(&compactable_prose()).blocks[0].clone();
    let doc = Document {
        blocks: vec![paragraph.clone(), Block::PageBreak, paragraph],
    };
    let opts = options_for(440.0, 196.0);
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (baseline, _) = unselected(&doc, &opts, &faces, page);
    assert_eq!(baseline.len(), 28);
    assert_eq!(
        emitted_pages(&baseline_pdf(&baseline, &opts, &faces, page)).len(),
        4
    );
    let selected = layout(&doc.blocks, &opts, &faces, page);
    assert_eq!(selected.len(), 26);
    assert_ne!(selected[0].flow.group, selected[13].flow.group);
    assert!(!selected[0].page_break_before);
    assert!(selected[13].page_break_before);
    assert_eq!(
        exact_height_page_breaks(&selected, page).unwrap().ends,
        [13, 26]
    );
    assert_eq!(
        emitted_pages(&crate::render_pdf_document(&doc, &opts).unwrap()).len(),
        2,
    );
}

#[test]
fn toc_page_numbers_and_destinations_follow_a_selected_shorter_paragraph() {
    let mut doc = crate::parse_markdown(&format!(
        "[[TOC]]\n\n{}\n\n# The river\n\nA short section.\n\n# The market\n\nOpen at noon.",
        compactable_prose(),
    ));
    doc.blocks.insert(1, Block::PageBreak);
    doc.blocks.insert(3, Block::PageBreak);
    // The following H1 adds its 11pt leading gap to the preceding paragraph,
    // including across the explicit page break. Retain that finalized gap.
    let opts = options_for(440.0, 207.0);
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let selected = layout(&doc.blocks, &opts, &faces, page);
    let paragraph = selected
        .iter()
        .find(|line| line.flow.kind == FlowKind::Paragraph && line.flow.count >= 4)
        .unwrap();
    assert_eq!(paragraph.flow.count, 13);
    let pdf = crate::render_pdf_document(&doc, &opts).unwrap();
    assert_eq!(emitted_pages(&pdf).len(), 3);
    for entry in collect_pdf_toc_entries(&doc.blocks) {
        let printed_page = selected
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
        assert_eq!(printed_page, 3);
        assert_eq!(printed_page, emitted_heading_page(&pdf, &entry.title));
    }
}

#[test]
fn searches_without_retained_alternatives_still_exhaust_the_document_work_budget() {
    let mut candidates = Candidates::new(&options_for(440.0, 196.0), &[]);
    let mut admitted = 0;
    while admitted < 100 && candidates.begin_measure(MAX_PARAGRAPH_ITEMS, 4) {
        admitted += 1;
    }
    assert!(admitted > 0 && admitted < 100);
    assert!(candidates.paragraphs.is_empty());
    assert!(candidates.measurement_work <= MAX_MEASUREMENT_WORK);
    assert!(!candidates.begin_measure(MAX_PARAGRAPH_ITEMS, 4));
    candidates.clear();
    assert!(candidates.begin_measure(MAX_PARAGRAPH_ITEMS, 4));
}

#[test]
fn default_rendering_preserves_the_original_measured_pdf() {
    let source = format!("# The river\n\n{PROSE}\n\n{PROSE}\n\n## Market\n\nOpen at noon.");
    let doc = crate::parse_markdown(&source);
    let mut opts = options_for(200.0, 220.0);
    opts.optimal_pagination = false;
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (baseline, candidates) = unselected(&doc, &opts, &faces, page);
    assert!(candidates.paragraphs.is_empty());
    let expected = baseline_pdf(&baseline, &opts, &faces, page);
    assert_eq!(
        expected,
        render(&doc, &opts, crate::PdfASettings::OFF).unwrap()
    );
    assert_eq!(
        expected,
        render_with_emit(
            &doc,
            &opts,
            crate::PdfASettings::OFF,
            PdfEmitOptions::monolithic(),
        )
        .unwrap()
    );
}

#[test]
fn extra_state_typography_modes_keep_their_measured_paragraph_shapes() {
    let doc = crate::parse_markdown(PROSE);
    for mode in 0..3 {
        let mut opts = options_for(200.0, 140.0);
        match mode {
            0 => opts.gradual_demerits = true,
            1 => opts.river_penalty = true,
            _ => opts.pareto_line_breaking = true,
        }
        let faces = Faces::load(&opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let (baseline, _) = unselected(&doc, &opts, &faces, page);
        assert_eq!(
            baseline_pdf(&baseline, &opts, &faces, page),
            render(&doc, &opts, crate::PdfASettings::OFF).unwrap(),
            "mode {mode} must retain its original KP state model",
        );
    }
}

#[test]
fn list_and_footnote_layouts_keep_their_existing_pdf_semantics() {
    for source in [
        format!("- {PROSE}"),
        format!("{PROSE} A note[^one].\n\n[^one]: Keep the note with its citation."),
        String::from("Short paragraph.\n\n| Head |\n| --- |\n| Cell |"),
    ] {
        let doc = crate::parse_markdown(&source);
        let opts = options_for(200.0, 140.0);
        let faces = Faces::load(&opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let (baseline, _) = unselected(&doc, &opts, &faces, page);
        assert_eq!(
            baseline_pdf(&baseline, &opts, &faces, page),
            render(&doc, &opts, crate::PdfASettings::OFF).unwrap(),
            "list, note and short-caption handling must retain its measured layout",
        );
    }
}

#[test]
fn quote_candidate_admission_requires_consistent_complete_ancestry() {
    let doc = crate::parse_markdown(&compactable_prose());
    let opts = options_for(440.0, 196.0);
    let faces = Faces::load(&opts).unwrap();
    let (baseline, candidates) = unselected(&doc, &opts, &faces, PageGeom::from_theme(&opts.theme));
    let paragraph = candidates.paragraphs.get(&baseline[0].flow.group).unwrap();
    assert!(eligible(&baseline, paragraph));
    assert!(!eligible(&[], paragraph));

    for depth in 1..=4 {
        let bars: Vec<_> = (0..depth)
            .map(|level| (level, 4.0 + level as f32 * 6.0))
            .collect();
        let mut quoted = baseline.clone();
        for line in &mut quoted {
            line.quote_bars.clone_from(&bars);
        }
        assert!(eligible(&quoted, paragraph));

        // Neither equal indentation with different owners nor a partial
        // ancestor path may merge distinct quotes into one replacement.
        let mut wrong_owner = quoted.clone();
        wrong_owner[1].quote_bars[0].0 += 100;
        assert!(!eligible(&wrong_owner, paragraph));
        let mut wrong_gutter = quoted.clone();
        wrong_gutter[1].quote_bars[0].1 += 1.0;
        assert!(!eligible(&wrong_gutter, paragraph));
        let mut missing_ancestor = quoted.clone();
        missing_ancestor[1].quote_bars.pop();
        assert!(!eligible(&missing_ancestor, paragraph));

        // Quote support does not admit list markers or a crossing of an
        // explicit chapter boundary into a paragraph alternative.
        quoted[1].page_break_before = true;
        assert!(!eligible(&quoted, paragraph));
        quoted[1].page_break_before = false;
        quoted[0].list_path.push(ListMark { list: 1, item: 1 });
        assert!(!eligible(&quoted, paragraph));
    }
}

#[test]
fn finalized_quote_shape_preserves_segments_and_nested_structure() {
    let source = format!("[{}](https://example.com/river)", compactable_prose());
    let doc = crate::parse_markdown(&source);
    let opts = options_for(440.0, 196.0);
    let faces = Faces::load(&opts).unwrap();
    let (mut baseline, candidates) =
        unselected(&doc, &opts, &faces, PageGeom::from_theme(&opts.theme));
    let bars = vec![(0, 12.0), (1, 24.0)];
    for line in &mut baseline {
        line.quote_bars.clone_from(&bars);
    }
    baseline[0].page_break_before = true;
    baseline.last_mut().unwrap().gap_after += 11.0;
    let paragraph = candidates.paragraphs.get(&baseline[0].flow.group).unwrap();
    let alternative = paragraph
        .alternatives
        .iter()
        .find(|shape| shape.lines.len() == 13)
        .unwrap();
    let selected = finalized_shape(&baseline, alternative);
    assert_eq!(selected.len(), 13);

    for (index, (line, measured)) in selected.iter().zip(&alternative.lines).enumerate() {
        assert_eq!(line.quote_bars, bars);
        assert_eq!(line.flow.group, baseline[0].flow.group);
        assert_eq!(line.flow.index, index);
        assert_eq!(line.flow.count, selected.len());
        assert_eq!(line.page_break_before, index == 0);
        let expected_gap = if index + 1 == selected.len() {
            baseline.last().unwrap().gap_after
        } else {
            0.0
        };
        assert_eq!(line.gap_after, expected_gap);
        assert_eq!(line.size, measured.size);
        assert_eq!(line.segs.len(), measured.segs.len());
        for (seg, original) in line.segs.iter().zip(&measured.segs) {
            assert_eq!(seg.x, original.x);
            assert_eq!(seg.width, original.width);
            assert_eq!(seg.text, original.text);
            assert_eq!(seg.text_hash, original.text_hash);
            assert_eq!(seg.slot, original.slot);
            assert_eq!(seg.link, original.link);
            assert_eq!(seg.fill, original.fill);
            assert_eq!(seg.strike, original.strike);
            assert_eq!(seg.task, original.task);
            assert_eq!(seg.expansion_permille, original.expansion_permille);
        }
    }
    // Installing decorations must not mutate the cached measured alternative.
    assert!(alternative.lines.iter().all(|line| line.quote_bars.is_empty()));
}

#[test]
fn quoted_joint_selection_saves_a_page_without_losing_quote_tags() {
    // Isolate container transfer from the paragraph solver with the existing
    // measured 14 -> 13-line fixture. Quotes are post-shaping decorations;
    // adding them must not disable this already certified page-saving shape.
    let source = compactable_prose();
    let doc = crate::parse_markdown(&source);
    let opts = options_for(440.0, 196.0);
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (mut baseline, candidates) = unselected(&doc, &opts, &faces, page);
    let bars = vec![(0, 12.0), (1, 24.0)];
    for line in &mut baseline {
        line.quote_bars.clone_from(&bars);
    }
    let original = baseline_pdf(&baseline, &opts, &faces, page);
    assert_eq!(emitted_pages(&original).len(), 2);
    let mut selected = baseline.clone();
    apply(&mut selected, page, &candidates);
    assert_eq!(selected.len(), 13);
    assert!(selected.iter().all(|line| line.quote_bars == bars));
    assert_eq!(exact_height_page_breaks(&selected, page).unwrap().ends, [13]);
    let texts: Vec<String> = selected
        .iter()
        .map(|line| line.segs.iter().map(seg_display_text).collect())
        .collect();
    assert_eq!(rejoin_lines(texts.iter().map(String::as_str)), source);

    let pdf = baseline_pdf(&selected, &opts, &faces, page);
    assert_eq!(emitted_pages(&pdf).len(), 1);
    assert_eq!(
        String::from_utf8_lossy(&pdf).matches("/S /BlockQuote").count(),
        2
    );
    assert_eq!(pdf, baseline_pdf(&selected, &opts, &faces, page));
    assert_eq!(
        pdf,
        serialize(
            &selected,
            &opts,
            &faces,
            page,
            None,
            crate::PdfASettings::OFF,
            PdfEmitOptions::monolithic(),
            &mut PdfProfiler::disabled(),
        )
        .unwrap()
    );
}

#[test]
fn parsed_nested_quotes_admit_candidates_and_keep_native_emission_in_sync() {
    let prose = compactable_prose();
    for depth in 1..=2 {
        let source = format!("{}{prose}", "> ".repeat(depth));
        let doc = crate::parse_markdown(&source);
        let opts = options_for(440.0, 196.0);
        let faces = Faces::load(&opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let (baseline, candidates) = unselected(&doc, &opts, &faces, page);
        assert_eq!(baseline[0].quote_bars.len(), depth);
        let paragraph = candidates
            .paragraphs
            .get(&baseline[0].flow.group)
            .expect("quoted prose retains measured paragraph alternatives");
        assert!(eligible(&baseline, paragraph));
        let selected = layout(&doc.blocks, &opts, &faces, page);
        assert!(
            selected
                .iter()
                .all(|line| line.quote_bars == baseline[0].quote_bars)
        );
        let pdf = crate::render_pdf(&source, &opts).unwrap();
        assert_quote_structure_on_every_page(&pdf, depth);
        assert_eq!(
            pdf,
            crate::render_pdf_emitted(&source, &opts, PdfEmitOptions::monolithic()).unwrap()
        );
        let verified = verification_text_layer(&doc, &opts).unwrap();
        assert_eq!(verified.page_count, emitted_pages(&pdf).len());
        assert_eq!(
            rejoin_lines(
                verified
                    .pages
                    .iter()
                    .flat_map(|page| &page.runs)
                    .map(|run| run.text.as_str())
            ),
            prose
        );
    }
}

#[test]
fn quoted_footnotes_keep_note_bodies_on_their_first_citation_page() {
    let source = format!(
        "> {} A note[^one].\n\n[^one]: The original note body remains intact.",
        compactable_prose()
    );
    let parsed = crate::parse_markdown(&source);
    let doc = crate::footnotes::for_pdf_paged(&parsed);
    let opts = options_for(440.0, 196.0);
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (baseline, _) = unselected(&doc, &opts, &faces, page);
    let baseline_notes = PageNotes::scan(&baseline).unwrap();
    let selected = layout(&doc.blocks, &opts, &faces, page);
    let notes = PageNotes::scan(&selected).unwrap();
    assert_eq!(notes.first_refs.len(), 1);
    let note_signature = |lines: &[Line]| {
        lines
            .iter()
            .map(|line| {
                (
                    line.flow.note,
                    line.size,
                    line.gap_after,
                    line.segs.iter().map(seg_display_text).collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        note_signature(&selected[notes.body_end..]),
        note_signature(&baseline[baseline_notes.body_end..])
    );
    assert!(
        selected[..notes.body_end]
            .iter()
            .all(|line| line.quote_bars == baseline[0].quote_bars)
    );
    let pages = paginate_lines_for_options(&selected, page, &opts);
    let (citation, range) = &notes.first_refs[0];
    let citation_page = pages
        .iter()
        .position(|placed| {
            placed
                .iter()
                .any(|p| std::ptr::eq(p.line, &selected[*citation]))
        })
        .unwrap();
    for note_line in &selected[range.clone()] {
        assert!(pages[citation_page].iter().any(|p| std::ptr::eq(p.line, note_line)));
    }
    let pdf = crate::render_pdf(&source, &opts).unwrap();
    assert_eq!(emitted_pages(&pdf).len(), pages.len());
    assert_eq!(verification_text_layer(&parsed, &opts).unwrap().page_count, pages.len());
}

#[test]
fn quotes_keep_baseline_layout_when_joint_selection_is_disabled() {
    let doc = crate::parse_markdown(&format!("> > {}", compactable_prose()));
    for mode in 0..4 {
        let mut opts = options_for(440.0, 196.0);
        match mode {
            0 => opts.optimal_pagination = false,
            1 => opts.gradual_demerits = true,
            2 => opts.river_penalty = true,
            _ => opts.pareto_line_breaking = true,
        }
        let faces = Faces::load(&opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let (baseline, candidates) = unselected(&doc, &opts, &faces, page);
        assert!(!candidates.enabled());
        assert!(candidates.paragraphs.is_empty());
        assert_eq!(
            baseline_pdf(&baseline, &opts, &faces, page),
            render(&doc, &opts, crate::PdfASettings::OFF).unwrap()
        );
    }
}
