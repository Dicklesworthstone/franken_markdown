//! Admit page notes using the same final grid metrics as the body and painter.
//!
//! A heading's descender clearance can add a whole pitch to every note row.
//! Measuring notes in isolation and only applying the shared grid afterwards
//! therefore admits reservations that cannot fit, even on an ordinary page.

use super::{
    Block, ENDNOTES_GAP_BEFORE, Inline, LayoutCx, Line, PAGE_NOTE_MAX_SHARE, PageNoteLayout,
    admit_page_notes, baseline_grid, heading_gap_before, layout_block, layout_pdf_note_body,
    line_leading,
};

/// Each pass is linear in the retained lines. Migration only goes from the
/// page foot to endnotes, and each endnote is laid out at full size once. An
/// unusually long cascade uses complete endnotes instead of unbounded retries.
const MAX_ADMISSION_PASSES: usize = 8;

pub(super) fn finish(
    notes: &mut [PageNoteLayout<'_>],
    body: &mut Vec<Line>,
    mut separator: Line,
    cx: &mut LayoutCx<'_>,
) {
    let Some(grid) = cx.opts.theme.page.baseline_grid else {
        return;
    };
    for note in &mut *notes {
        note.at_foot = !note.lines.is_empty();
    }
    let mut heading = Vec::new();
    let mut endnotes: Vec<Option<Vec<Line>>> = (0..notes.len()).map(|_| None).collect();
    let mut has_endnotes = false;

    // The extra iteration materializes the full-size endnotes selected by
    // the work-budget fallback; it does not perform another admission pass.
    for pass in 0..=MAX_ADMISSION_PASSES {
        if !has_endnotes && notes.iter().any(|note| !note.at_foot) {
            has_endnotes = true;
            // The heading is kept in a separate region until admission is
            // stable. Apply its real preceding spacing to the original last
            // body line now, before quantization and reservation checks.
            if let Some(last) = body.last_mut() {
                baseline_grid::add_natural_gap(last, ENDNOTES_GAP_BEFORE + heading_gap_before(2));
            }
            layout_block(
                &Block::Heading {
                    level: 2,
                    inlines: vec![Inline::Text("Notes".to_string())],
                },
                0.0,
                &mut heading,
                cx,
            );
        }
        for (note, endnote) in notes.iter_mut().zip(&mut endnotes) {
            if !note.at_foot && endnote.is_none() {
                // Every prepared note begins with its numbered paragraph.
                // Its full body, containers and source links stay intact.
                note.lines.clear();
                let mut lines = Vec::new();
                layout_pdf_note_body(note.body, &mut lines, cx);
                if let Some(first) = lines.first_mut() {
                    first.flow.note_destination = note.number;
                }
                *endnote = Some(lines);
            }
        }
        if !notes.iter().any(|note| note.at_foot) {
            break;
        }

        let mut regions = vec![
            body.as_mut_slice(),
            heading.as_mut_slice(),
            std::slice::from_mut(&mut separator),
        ];
        regions.extend(
            notes
                .iter_mut()
                .filter(|note| note.at_foot)
                .map(|note| note.lines.as_mut_slice()),
        );
        regions.extend(endnotes.iter_mut().flatten().map(Vec::as_mut_slice));
        baseline_grid::apply_regions(&mut regions, grid, cx.faces);
        drop(regions);

        let page = baseline_grid::page_for_lines(cx.page, body);
        let capacity = (page.top_y() - page.bottom).max(0.0);
        let separator_height = line_leading(&separator) + separator.gap_after;
        let previous: Vec<_> = notes.iter().map(|note| note.at_foot).collect();
        for note in notes.iter_mut().filter(|note| note.at_foot) {
            note.height = baseline_grid::height(note.lines.iter());
            if note.height > capacity * PAGE_NOTE_MAX_SHARE {
                // Clearing only the temporary foot layout prevents a later
                // pass from readmitting this note. Its source body survives
                // and is laid out as an endnote on the next iteration.
                note.lines.clear();
            }
        }
        // The actual Notes-heading gap, when present, is already included in
        // `body`; do not estimate or add it a second time here.
        admit_page_notes(notes, body, capacity, separator_height, false);
        if notes
            .iter()
            .zip(previous)
            .all(|(note, was_at_foot)| note.at_foot == was_at_foot)
        {
            break;
        }
        if pass + 1 == MAX_ADMISSION_PASSES {
            for note in &mut *notes {
                note.at_foot = false;
            }
        }
    }

    body.append(&mut heading);
    for mut lines in endnotes.into_iter().flatten() {
        body.append(&mut lines);
    }
    if notes.iter().any(|note| note.at_foot) {
        body.push(separator);
        for note in notes.iter_mut().filter(|note| note.at_foot) {
            body.append(&mut note.lines);
        }
    }
}
