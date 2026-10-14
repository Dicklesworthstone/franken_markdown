#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

const PROSE: &str = "The old stone bridge stood beside the river while the town woke slowly. \
    A small boat crossed the clear water and a child watched from the steps. \
    The morning light reached the houses along the bank and the shop doors opened. \
    People walked to work, shared news with friends, and stopped to hear the birds. \
    By noon the market was full of fresh bread, bright fruit, and flowers from the hills.";

fn options_for_notes() -> PdfOptions {
    let mut opts = PdfOptions {
        optimal_pagination: true,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = 512.0;
    opts.theme.page.size.height_pt = 268.0;
    opts.theme.page.margins = crate::theme::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

/// Run the real shaping and note-layout stages, but leave joint selection to
/// the test. No font, paragraph, reservation or pagination implementation is
/// substituted. This mirrors the production pre-selection layout boundary.
fn unselected_notes(
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

fn prepared_layout(source: &str) -> Vec<Line> {
    let doc = crate::parse_markdown(source);
    let prepared = crate::footnotes::for_pdf_paged(&doc);
    let opts = options_for_notes();
    let faces = Faces::load(&opts).unwrap();
    unselected_notes(&prepared, &opts, &faces, PageGeom::from_theme(&opts.theme)).0
}

fn note_plan(lines: &[Line], page: PageGeom) -> PageBreakPlan {
    let notes = PageNotes::scan(lines).expect("prepared page-note region");
    exact_height_page_breaks_reserving(&lines[..notes.body_end], page, |line| {
        notes.line_reserve(lines, line)
    })
    .expect("a legal footnote-reserving plan")
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.segs.iter().map(seg_display_text).collect())
        .collect()
}

/// Controlled citation positions for reservation-unit tests only. The linked
/// segments and measured note bodies come from the real layout. These probe
/// lines are never passed to the renderer as claimed paragraph alternatives.
fn citation_positions(lines: &[Line], positions: &[&[u32]]) -> Vec<Line> {
    let notes = PageNotes::scan(lines).unwrap();
    let mut refs = BTreeMap::new();
    for seg in lines[..notes.body_end].iter().flat_map(|line| &line.segs) {
        if let Some(number) = seg_page_note(seg) {
            refs.entry(number).or_insert_with(|| seg.clone());
        }
    }
    positions
        .iter()
        .map(|numbers| {
            let mut line = cost_context(&lines[0]);
            line.segs = numbers.iter().map(|number| refs[number].clone()).collect();
            line
        })
        .collect()
}

fn assert_note_region_preserved(before: &[Line], after: &[Line]) {
    let old = PageNotes::scan(before).unwrap();
    let new = PageNotes::scan(after).unwrap();
    let old_tail = &before[old.body_end..];
    let new_tail = &after[new.body_end..];
    assert_eq!(texts(old_tail), texts(new_tail));
    assert_eq!(old_tail.len(), new_tail.len());
    for (a, b) in old_tail.iter().zip(new_tail) {
        assert_eq!(a.flow.note, b.flow.note);
        assert_eq!(a.flow.group, b.flow.group);
        assert_eq!(a.flow.index, b.flow.index);
        assert_eq!(a.flow.count, b.flow.count);
        assert_eq!(a.size, b.size);
        assert_eq!(a.gap_after, b.gap_after);
        assert_eq!(a.rule, b.rule);
        assert_eq!(a.table_cols, b.table_cols);
        assert_eq!(a.segs.len(), b.segs.len());
        for (a, b) in a.segs.iter().zip(&b.segs) {
            assert_eq!(a.x, b.x);
            assert_eq!(a.width, b.width);
            assert_eq!(a.slot, b.slot);
            assert_eq!(a.link, b.link);
        }
    }
    assert_eq!(
        note_references(&before[..old.body_end]).collect::<Vec<_>>(),
        note_references(&after[..new.body_end]).collect::<Vec<_>>(),
    );
}

#[test]
fn prepared_notes_enable_alternatives_without_admitting_raw_definition_ids() {
    let opts = options_for_notes();
    let doc = crate::parse_markdown("A note[^one].\n\n[^one]: A definition.");
    assert!(!Candidates::new(&opts, &doc.blocks).enabled());
    let prepared = crate::footnotes::for_pdf_paged(&doc);
    assert!(Candidates::new(&opts, &prepared.blocks).enabled());
    assert!(Candidates::new(&opts, &[]).enabled());
    for mode in 0..4 {
        let mut disabled = opts.clone();
        match mode {
            0 => disabled.optimal_pagination = false,
            1 => disabled.gradual_demerits = true,
            2 => disabled.river_penalty = true,
            _ => disabled.pareto_line_breaking = true,
        }
        assert!(!Candidates::new(&disabled, &prepared.blocks).enabled());
    }
}

#[test]
fn baseline_reservations_match_the_emitter_bit_for_bit() {
    let lines = prepared_layout(
        "One[^one] and two[^two], again[^one].  \nA third[^three].\n\n\
         [^one]: First note.\n\n[^two]: Second note with more text.\n\n[^three]: Third note.",
    );
    let notes = PageNotes::scan(&lines).unwrap();
    assert_eq!(notes.first_refs.len(), 3);
    let reserves = shape_note_reservations(&lines, &notes, 0, notes.body_end, &lines[..notes.body_end])
        .unwrap();
    for (index, reserve) in reserves.iter().enumerate() {
        assert_eq!(reserve.to_bits(), notes.line_reserve(&lines, index).to_bits());
    }
}

#[test]
fn moving_citations_reanchors_note_bodies_and_charges_each_body_once() {
    let lines = prepared_layout(
        "One[^one], two[^two], repeat[^one].\n\n[^one]: First note.\n\n[^two]: Second note.",
    );
    let notes = PageNotes::scan(&lines).unwrap();
    let shape = citation_positions(&lines, &[&[], &[1], &[], &[2, 1]]);
    let reserves = shape_note_reservations(&lines, &notes, 0, notes.body_end, &shape).unwrap();
    let separator = line_leading(&lines[notes.separator]) + lines[notes.separator].gap_after;
    let note_height = |number| {
        lines[notes.body_end..]
            .iter()
            .filter(|line| line.flow.note == number)
            .map(|line| line_leading(line) + line.gap_after)
            .sum::<f32>()
    };
    assert_eq!(reserves, [0.0, separator + note_height(1), 0.0, separator + note_height(2)]);
    // This is the same scan the emitter performs after selected shapes are
    // installed. It must agree even though all original line indices moved.
    let mut replaced = shape.clone();
    replaced.extend(lines[notes.body_end..].iter().cloned());
    let rescanned = PageNotes::scan(&replaced).unwrap();
    for (index, reserve) in reserves.iter().enumerate() {
        assert_eq!(reserve.to_bits(), rescanned.line_reserve(&replaced, index).to_bits());
    }
}

#[test]
fn multiple_first_citations_on_one_line_share_one_separator() {
    let lines = prepared_layout(
        "One[^one].  \nTwo[^two].\n\n[^one]: First note.\n\n[^two]: Second note.",
    );
    let notes = PageNotes::scan(&lines).unwrap();
    let shape = citation_positions(&lines, &[&[], &[1, 2], &[], &[]]);
    let reserves = shape_note_reservations(&lines, &notes, 0, notes.body_end, &shape).unwrap();
    assert_eq!(reserves[1].to_bits(), notes.reserve(&lines, 0, notes.body_end).to_bits());
    assert_eq!(reserves.iter().filter(|&&height| height > 0.0).count(), 1);
}

#[test]
fn references_already_charged_in_an_earlier_block_are_not_reserved_again() {
    let lines = prepared_layout(
        "First[^one].\n\nRepeat[^one] and new[^two].\n\n[^one]: First note.\n\n[^two]: Second note.",
    );
    let notes = PageNotes::scan(&lines).unwrap();
    let start = pdf_height_block_end(&lines[..notes.body_end], 0);
    let shape = citation_positions(&lines, &[&[1], &[], &[2], &[]]);
    let reserves = shape_note_reservations(&lines, &notes, start, notes.body_end, &shape).unwrap();
    assert_eq!(reserves[0], 0.0);
    assert_eq!(reserves[2].to_bits(), notes.reserve(&lines, start, notes.body_end).to_bits());
}

#[test]
fn missing_extra_and_reordered_citations_cannot_drop_reservations() {
    let lines = prepared_layout(
        "One[^one], two[^two].\n\n[^one]: First note.\n\n[^two]: Second note.",
    );
    let notes = PageNotes::scan(&lines).unwrap();
    for positions in [vec![1], vec![2, 1], vec![1, 1, 2], vec![]] {
        let shape = citation_positions(&lines, &[positions.as_slice()]);
        assert!(shape_note_reservations(&lines, &notes, 0, notes.body_end, &shape).is_none());
    }
}

#[test]
fn a_plain_paragraph_can_save_a_page_in_a_document_with_page_notes() {
    let prose = [PROSE; 3].join(" ");
    let mut doc = crate::parse_markdown(&format!(
        "{prose}\n\nA citation[^one].\n\n[^one]: Keep this complete note with its first citation.",
    ));
    doc.blocks.insert(1, Block::PageBreak);
    let prepared = crate::footnotes::for_pdf_paged(&doc);
    let opts = options_for_notes();
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (baseline, candidates) = unselected_notes(&prepared, &opts, &faces, page);
    let notes = PageNotes::scan(&baseline).unwrap();
    assert_eq!(notes.first_refs[0].0, 14);
    assert_eq!(note_plan(&baseline, page).ends.len(), 3);

    let selected = select(&baseline, page, &candidates).expect("joint choice saves the first page");
    assert_eq!(note_plan(&selected, page).ends.len(), 2);
    let shifted = PageNotes::scan(&selected).unwrap();
    assert_eq!(shifted.first_refs[0].0, 13);
    assert!(selected[13].page_break_before);
    assert_eq!(shifted.first_refs[0].1.start + 1, notes.first_refs[0].1.start);
    assert_note_region_preserved(&baseline, &selected);
    let public_lines = layout(&prepared.blocks, &opts, &faces, page);
    assert_eq!(texts(&selected), texts(&public_lines));
    assert_eq!(note_plan(&public_lines, page).ends, note_plan(&selected, page).ends);
}

#[test]
fn a_citing_paragraph_uses_its_real_shorter_shape_and_keeps_the_note_on_its_page() {
    // The prepared AST protocol represents a citation as a numbered fragment
    // link. Use an existing word as its visible label to retain the known
    // same-measure 14 -> 13-line fixture; no synthetic shape is offered.
    let prose = [PROSE; 3].join(" ");
    let source = format!(
        "{}\n\n[^fmd-fn-1]: Note body retained at the foot of the citing page.",
        prose.replacen("The", "[The](#fmd-fn-1)", 1),
    );
    let doc = crate::parse_markdown(&source);
    let mut opts = options_for_notes();
    let faces = Faces::load(&opts).unwrap();
    let (baseline, candidates) = unselected_notes(&doc, &opts, &faces, PageGeom::from_theme(&opts.theme));
    let notes = PageNotes::scan(&baseline).unwrap();
    let paragraph = &candidates.paragraphs[&baseline[0].flow.group];
    let shorter = paragraph.alternatives.iter().find(|shape| shape.lines.len() == 13).unwrap();
    let shape = finalized_shape(&baseline[..notes.body_end], shorter);
    let required = shape.iter().map(|line| line_leading(line) + line.gap_after).sum::<f32>()
        + notes.reserve(&baseline, 0, notes.body_end);
    opts.theme.page.size.height_pt = required + 72.25;
    let page = PageGeom::from_theme(&opts.theme);
    assert_eq!(notes.body_end, 14);
    assert_eq!(note_plan(&baseline, page).ends.len(), 2);
    let selected = select(&baseline, page, &candidates).expect("a cited paragraph can shorten");
    assert_eq!(note_plan(&selected, page).ends.len(), 1);
    assert_eq!(PageNotes::scan(&selected).unwrap().body_end, 13);
    assert_note_region_preserved(&baseline, &selected);
    let pages = paginate_lines_for_options(&selected, page, &opts);
    assert_eq!(pages.len(), 1);
    assert!(pages[0].iter().any(|placed| placed.line.flow.note == 1));
    assert!(pages[0].iter().any(|placed| placed.line.segs.iter().any(|seg| seg_page_note(seg) == Some(1))));
    assert!(selected.iter().all(|line| line_overshoot(line, &page).is_none()));
}

#[test]
fn real_footnote_source_preserves_text_links_emission_and_verification() {
    let source = format!(
        "{} A citation[^one].\n\n{} Repeated[^one] and second[^two].\n\n\
         [^one]: First complete note.\n\n[^two]: Second complete note.",
        [PROSE; 3].join(" "), [PROSE; 3].join(" "),
    );
    let doc = crate::parse_markdown(&source);
    let prepared = crate::footnotes::for_pdf_paged(&doc);
    let mut opts = options_for_notes();
    opts.metadata_epoch_seconds = Some(0);
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (baseline, _) = unselected_notes(&prepared, &opts, &faces, page);
    let selected = layout(&prepared.blocks, &opts, &faces, page);
    assert_note_region_preserved(&baseline, &selected);
    let notes = PageNotes::scan(&selected).unwrap();
    let pages = paginate_lines_for_options(&selected, page, &opts);
    for (citing_line, range) in &notes.first_refs {
        let number = selected[range.start].flow.note;
        let first_page = pages.iter().position(|placed| placed.iter().any(|p| {
            std::ptr::eq(p.line, &selected[*citing_line])
        })).unwrap();
        let note_pages: Vec<_> = pages.iter().enumerate().filter_map(|(index, placed)| {
            placed.iter().any(|p| p.line.flow.note == number).then_some(index)
        }).collect();
        assert_eq!(note_pages, [first_page]);
    }
    let chunked = crate::render_pdf(&source, &opts).unwrap();
    assert_eq!(chunked, crate::render_pdf(&source, &opts).unwrap());
    assert_eq!(chunked, crate::render_pdf_emitted(&source, &opts, PdfEmitOptions::monolithic()).unwrap());
    let verified = verification_text_layer(&doc, &opts).unwrap();
    assert_eq!(verified.page_count, pages.len());
    let verified_text = verified.pages.iter().flat_map(|page| &page.runs)
        .map(|run| run.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(verified_text.contains("First complete note."));
    assert!(verified_text.contains("Second complete note."));
}

#[test]
fn nonoptimal_page_notes_remain_on_the_original_layout_path() {
    let source = format!("{PROSE} A note[^one].\n\n[^one]: The original note.");
    let doc = crate::parse_markdown(&source);
    let prepared = crate::footnotes::for_pdf_paged(&doc);
    let mut opts = options_for_notes();
    opts.optimal_pagination = false;
    let faces = Faces::load(&opts).unwrap();
    let page = PageGeom::from_theme(&opts.theme);
    let (baseline, candidates) = unselected_notes(&prepared, &opts, &faces, page);
    assert!(candidates.paragraphs.is_empty());
    let selected = layout(&prepared.blocks, &opts, &faces, page);
    assert_eq!(texts(&baseline), texts(&selected));
    assert_note_region_preserved(&baseline, &selected);
}

#[test]
fn infeasible_note_space_cannot_be_evaded_by_selecting_an_alternative() {
    let prose = [PROSE; 3].join(" ");
    let source = format!(
        "{}\n\n[^fmd-fn-1]: First note line.  \n    Second note line.  \n    Third note line.",
        prose.replacen("The", "[The](#fmd-fn-1)", 1),
    );
    let doc = crate::parse_markdown(&source);
    let opts = options_for_notes();
    let faces = Faces::load(&opts).unwrap();
    let mut page = PageGeom::from_theme(&opts.theme);
    let (baseline, candidates) = unselected_notes(&doc, &opts, &faces, page);
    let notes = PageNotes::scan(&baseline).unwrap();
    assert!(!candidates.paragraphs[&baseline[0].flow.group].alternatives.is_empty());
    page.bottom = page.top_y() - MIN_CONTENT_DIM;
    assert!(notes.line_reserve(&baseline, 0) + line_leading(&baseline[0]) > MIN_CONTENT_DIM);
    assert!(select(&baseline, page, &candidates).is_none());
    let mut unchanged = baseline.clone();
    apply(&mut unchanged, page, &candidates);
    assert_eq!(texts(&baseline), texts(&unchanged));
    assert_note_region_preserved(&baseline, &unchanged);
}

#[test]
fn a_block_with_only_repeated_citations_has_no_new_reservation() {
    let lines = prepared_layout(
        "First[^one].\n\nRepeated[^one] twice[^one].\n\n[^one]: One body, not three.",
    );
    let notes = PageNotes::scan(&lines).unwrap();
    let start = pdf_height_block_end(&lines[..notes.body_end], 0);
    let shape = citation_positions(&lines, &[&[], &[1], &[], &[1]]);
    assert!(shape_note_reservations(&lines, &notes, start, notes.body_end, &shape).unwrap().is_empty());
}
