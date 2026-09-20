//! Reading-text styling from the exact flow projection used for layout.
//! One shaped reading root is emitted per DisplayBlock; table cells are its
//! only children. Refuse any mismatch rather than guess from source or pixels.

use super::Json;
use crate::display::{AccessibleReadingNode, AccessibleReadingRole};
use crate::flow_display::{DisplayBlock, FlowInlineRun, ResumableFlowDisplay, active_link_target};
use std::fmt::{self, Write};

pub(super) fn fields(
    w: &mut Json,
    node: &AccessibleReadingNode,
    engine: &ResumableFlowDisplay,
    index: usize,
    cell: Option<usize>,
) -> fmt::Result {
    let block = engine.blocks().get(index).ok_or(fmt::Error)?;
    let mut runs: &[FlowInlineRun] = &[];
    let mut offset = 0;
    let mut image_link = None;
    if let Some(column) = cell {
        let (cells, role) = match block {
            DisplayBlock::TableHeader { cells } => (cells, AccessibleReadingRole::TableHeaderCell),
            DisplayBlock::TableRow { cells } => (cells, AccessibleReadingRole::TableCell),
            _ => return Err(fmt::Error),
        };
        if node.role != role || !node.children.is_empty()
            || cells.get(column).map(String::as_str) != Some(node.text.as_str())
        {
            return Err(fmt::Error);
        }
        runs = engine.inline_runs_for_cell(index, column).ok_or(fmt::Error)?;
    } else {
        match block {
            DisplayBlock::Heading { level, text } => {
                if node.role != (AccessibleReadingRole::Heading { level: *level }) || node.text != *text {
                    return Err(fmt::Error);
                }
                runs = engine.inline_runs_for_block(index).ok_or(fmt::Error)?;
            }
            DisplayBlock::Paragraph { text } | DisplayBlock::ListItem { text, .. }
            | DisplayBlock::Quote { text } => {
                let role = match block {
                    DisplayBlock::ListItem { .. } => AccessibleReadingRole::ListItem,
                    DisplayBlock::Quote { .. } => AccessibleReadingRole::BlockQuote,
                    _ => AccessibleReadingRole::Paragraph,
                };
                if node.role != role { return Err(fmt::Error); }
                // Reflow adds this fixed task marker to reading text, never to
                // inline coordinates. Do not shift ordinary literal '[x]' text.
                offset = if node.text == *text { 0 }
                    else if node.text.strip_prefix("[x] ") == Some(text.as_str())
                        || node.text.strip_prefix("[ ] ") == Some(text.as_str()) { 4 }
                    else { return Err(fmt::Error); };
                runs = engine.inline_runs_for_block(index).ok_or(fmt::Error)?;
            }
            DisplayBlock::TableHeader { cells } | DisplayBlock::TableRow { cells } => {
                let role = if matches!(block, DisplayBlock::TableHeader { .. }) {
                    AccessibleReadingRole::TableHeaderRow
                } else { AccessibleReadingRole::TableRow };
                if node.role != role || node.children.len() != cells.len() { return Err(fmt::Error); }
            }
            DisplayBlock::UnresolvedAsset(asset) => {
                if node.role != AccessibleReadingRole::Image || node.text != asset.alt_text { return Err(fmt::Error); }
                image_link = engine.image_link_for_block(index);
            }
            DisplayBlock::CodeBlock { source, .. } => {
                if node.role != AccessibleReadingRole::CodeBlock || node.text != *source { return Err(fmt::Error); }
            }
            DisplayBlock::Rule => {
                if node.role != AccessibleReadingRole::ThematicBreak || !node.text.is_empty() { return Err(fmt::Error); }
            }
        }
        if !matches!(block, DisplayBlock::TableHeader { .. } | DisplayBlock::TableRow { .. })
            && !node.children.is_empty() { return Err(fmt::Error); }
    }
    if cell.is_none() && matches!(block, DisplayBlock::Heading { .. }) {
        // Use the exact destination assigned during AST projection. Source
        // spans and repeated heading text cannot identify a unique target.
        let id = engine.heading_id_for_block(index).ok_or(fmt::Error)?;
        w.write_str(",\"anchorId\":")?;
        w.string(id)?;
    }
    list_path(w, engine, index, cell)?;
    w.write_str(",\"inlineRuns\":[")?;
    let mut previous_end = offset;
    for (i, run) in runs.iter().enumerate() {
        let start = run.range.start.checked_add(offset).ok_or(fmt::Error)?;
        let end = run.range.end.checked_add(offset).ok_or(fmt::Error)?;
        if start < previous_end || end <= start || node.text.get(start..end).is_none() { return Err(fmt::Error); }
        previous_end = end;
        if i != 0 { w.write_char(',')?; }
        write!(w, "{{\"startByte\":{start},\"endByte\":{end},\"style\":{{\"bold\":{},\"italic\":{},\"code\":{},\"strikethrough\":{}}},\"link\":",
            run.style.bold, run.style.italic, run.style.code, run.style.strikethrough)?;
        link(w, run.link.as_deref())?;
        w.write_char('}')?;
    }
    w.write_str("],\"imageLink\":")?;
    link(w, image_link)
}

fn list_path(w: &mut Json, engine: &ResumableFlowDisplay, index: usize, cell: Option<usize>) -> fmt::Result {
    w.write_str(",\"listPath\":[")?;
    // The row owns list membership; its nested cells must not repeat ancestry.
    if cell.is_none() {
        for (i, item) in engine.list_path_for_block(index).ok_or(fmt::Error)?.iter().enumerate() {
            if i != 0 { w.write_char(',')?; }
            w.write_str("{\"listId\":")?; w.identity(item.list_id)?;
            write!(w, ",\"ordered\":{},\"start\":", item.ordered)?; w.identity(item.start)?;
            write!(w, ",\"itemIndex\":{},\"task\":", item.item_index)?;
            if let Some(checked) = item.task { write!(w, "{checked}")?; }
            else { w.write_str("null")?; }
            w.write_char('}')?;
        }
    }
    w.write_char(']')
}

fn link(w: &mut Json, target: Option<&str>) -> fmt::Result {
    let Some(target) = target else { return w.write_str("null"); };
    w.write_str("{\"target\":")?; w.string(target)?;
    w.write_str(",\"activeTarget\":")?;
    if let Some(active) = active_link_target(target) { w.string(active)?; }
    else { w.write_str("null")?; }
    w.write_char('}')
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use super::super::encode;

    fn projected(source: &str) -> (ResumableFlowDisplay, crate::DisplayList) {
        let mut engine = ResumableFlowDisplay::new(source, 1);
        engine.process_all().unwrap();
        let list = engine.to_display_list();
        (engine, list)
    }

    #[test]
    fn unicode_nested_formatting_and_blocked_links_survive_serialization() {
        let (engine, list) = projected("**é *two*** [x](javascript:bad) [ok](https://example.com)");
        let output = encode(|w| fields(w, &list.reading_order()[0], &engine, 0, None)).unwrap();
        assert!(output.contains("\"startByte\":0,\"endByte\":3"));
        assert!(output.contains("\"bold\":true,\"italic\":true"));
        assert!(output.contains("\"target\":\"javascript:bad\",\"activeTarget\":null"));
        assert!(output.contains("\"activeTarget\":\"https://example.com\""));
    }

    #[test]
    fn task_prefix_and_table_cell_coordinates_are_local_reading_bytes() {
        let (engine, list) = projected("- [x] **é**\n\n| *head* |\n| --- |\n| [cell](#head) |\n");
        let mut task = list.reading_order()[0].clone();
        // Pin the shaped reflow transcript independently of estimate-only text.
        task.text = "[x] é".to_owned();
        let output = encode(|w| fields(w, &task, &engine, 0, None)).unwrap();
        assert!(output.contains("\"startByte\":4,\"endByte\":6"));
        let header = &list.reading_order()[1].children[0];
        assert!(encode(|w| fields(w, header, &engine, 1, Some(0))).unwrap().contains("\"italic\":true"));
        let cell = &list.reading_order()[2].children[0];
        assert!(encode(|w| fields(w, cell, &engine, 2, Some(0))).unwrap().contains("\"activeTarget\":\"#head\""));
    }

    #[test]
    fn image_links_survive_even_with_empty_alt_text() {
        let (engine, list) = projected("[![](x.png)](https://example.com)");
        let output = encode(|w| fields(w, &list.reading_order()[0], &engine, 0, None)).unwrap();
        assert!(output.contains(",\"inlineRuns\":[],\"imageLink\":{\"target\":\"https://example.com\""));
    }

    #[test]
    fn wrong_projection_or_cell_is_rejected_not_silently_misbound() {
        let (engine, list) = projected("**one**\n\ntwo");
        assert!(encode(|w| fields(w, &list.reading_order()[1], &engine, 0, None)).is_err());
        assert!(encode(|w| fields(w, &list.reading_order()[0], &engine, 0, Some(0))).is_err());
    }

    #[test]
    fn full_browser_page_uses_absolute_indexes_and_is_deterministic() {
        use crate::flow_display::FlowLayoutOptions;
        use super::super::{BrowserFlowSession, reading};
        let state = BrowserFlowSession::new("plain\n\n**bold** [ref](#plain)", "sans", FlowLayoutOptions::default()).unwrap();
        let page = reading(&state, 1..2).unwrap();
        assert!(page.contains("\"offset\":1"));
        assert!(page.contains("\"bold\":true"));
        assert!(page.contains("\"activeTarget\":\"#plain\""));
        assert_eq!(page, reading(&state, 1..2).unwrap());
    }

    #[test]
    fn list_membership_is_serialized_on_continuations_and_not_repeated_on_cells() {
        let (engine, list) = projected("7. first\n\n   next\n\n   | A |\n   | --- |\n   | B |\n8. last\n");
        for index in [0, 1] {
            let output = encode(|w| fields(w, &list.reading_order()[index], &engine, index, None)).unwrap();
            assert!(output.contains("\"listPath\":[{\"listId\":\"1\",\"ordered\":true,\"start\":\"7\",\"itemIndex\":0,\"task\":null}]"));
        }
        let index = engine.blocks().iter().position(|b| matches!(b, DisplayBlock::TableHeader { .. })).unwrap();
        let output = encode(|w| fields(w, &list.reading_order()[index].children[0], &engine, index, Some(0))).unwrap();
        assert!(output.contains("\"listPath\":[]"));
    }

    #[test]
    fn heading_and_note_destinations_match_display_anchors_without_guessing() {
        let (engine, list) = projected("# Repeat\n\n# Repeat-2\n\n# Repeat\n\nUse [^n].\n\n[^n]: **Note**\n");
        let mut ids = Vec::new();
        for (index, node) in list.reading_order().iter().enumerate() {
            let json = encode(|w| fields(w, node, &engine, index, None)).unwrap();
            if let Some(id) = engine.heading_id_for_block(index) {
                ids.push(id);
                assert!(json.contains(&format!("\"anchorId\":\"{id}\"")));
                assert!(list.anchors().any(|anchor| anchor.is_heading && anchor.anchor_id == id
                    && anchor.bounds == node.bounds && anchor.source_span == node.source_span));
            } else {
                assert!(!json.contains("\"anchorId\""));
            }
        }
        assert_eq!(ids, vec!["repeat", "repeat-2", "repeat-3", "fmd:note:1"]);
        assert!(engine.heading_id_for_block(usize::MAX).is_none());
    }

    #[test]
    fn shaped_browser_footnotes_have_matching_citations_and_destinations() {
        use crate::flow_display::FlowLayoutOptions;
        use super::super::{BrowserFlowSession, reading};
        let state = BrowserFlowSession::new("Use [^note].\n\n[^note]: A **styled** note.\n",
            "sans", FlowLayoutOptions::default()).unwrap();
        let page = reading(&state, 0..3).unwrap();
        assert!(page.contains("\"activeTarget\":\"#fmd:note:1\""));
        assert!(page.contains("\"anchorId\":\"fmd:note:1\""));
        assert!(page.contains("\"bold\":true"));
        let display = state.session.display();
        let target = display.anchors().find(|anchor| anchor.is_heading
            && anchor.anchor_id == "fmd:note:1").unwrap();
        let citation = display.anchors().find(|anchor| !anchor.is_heading
            && anchor.anchor_id == "#fmd:note:1").unwrap();
        assert!(target.bounds.y > citation.bounds.y);
        assert_eq!(page, reading(&state, 0..3).unwrap());
    }

}
