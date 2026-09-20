#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::display::{DisplayClip, DisplaySemanticAnchor, DisplayTextRun};
use crate::span::SourceSpan;

fn text(bounds: DisplayRect, value: &str) -> DisplayItem {
    DisplayItem::Text(DisplayTextRun {
        bounds, text: value.into(), font_run: None, color_role: "text".into(),
        source_span: SourceSpan::new(3, 9), font_size: 14.0,
    })
}
fn anchor(bounds: DisplayRect) -> DisplayItem {
    DisplayItem::Anchor(DisplaySemanticAnchor {
        bounds, anchor_id: "target".into(), is_heading: false, level: 0,
        source_span: SourceSpan::new(3, 9),
    })
}
fn clip(list: &mut DisplayList, bounds: DisplayRect, child_count: usize) {
    list.push_item(DisplayItem::Clip(DisplayClip { bounds, child_count }));
}
fn ids(page: &DisplayViewportPage<'_>) -> Vec<usize> {
    page.items.iter().map(|item| item.index).collect()
}

#[test]
fn nested_and_crossing_scopes_survive_cursor_pages() {
    let mut list = DisplayList::new();
    clip(&mut list, DisplayRect::new(0.0, 0.0, 30.0, 30.0), 3); // items 1..=3
    list.push_item(text(DisplayRect::new(0.0, 0.0, 100.0, 100.0), "first"));
    clip(&mut list, DisplayRect::new(10.0, 10.0, 60.0, 60.0), 3); // items 3..=5
    for word in ["both", "inner only", "last"] {
        list.push_item(text(DisplayRect::new(0.0, 0.0, 100.0, 100.0), word));
    }
    let view = DisplayRect::new(0.0, 0.0, 100.0, 100.0);
    let a = list.viewport_page(view, 0, 1, DisplayQueryMode::Visible).unwrap();
    assert_eq!(ids(&a), [1]);
    assert_eq!(a.next_index, Some(2));
    assert_eq!(a.items[0].clip, DisplayRect::new(0.0, 0.0, 30.0, 30.0));
    let b = list.viewport_page(view, a.next_index.unwrap(), 1, DisplayQueryMode::Visible).unwrap();
    assert_eq!(ids(&b), [3]);
    assert_eq!(b.items[0].clip, DisplayRect::new(10.0, 10.0, 20.0, 20.0));
    let c = list.viewport_page(view, b.next_index.unwrap(), 2, DisplayQueryMode::Visible).unwrap();
    assert_eq!(ids(&c), [4, 5]);
    assert_eq!(c.items[0].clip, DisplayRect::new(10.0, 10.0, 60.0, 60.0));
    assert_eq!(c.next_index, None);
    assert!(std::ptr::eq(c.items[0].item, &list.items()[4]));
}

#[test]
fn hits_ignore_clips_hidden_links_and_noninteractive_overlays() {
    let mut list = DisplayList::new();
    list.push_item(text(DisplayRect::new(0.0, 0.0, 100.0, 100.0), "under"));
    clip(&mut list, DisplayRect::new(10.0, 10.0, 20.0, 20.0), 2);
    list.push_item(text(DisplayRect::new(0.0, 0.0, 100.0, 100.0), "clipped"));
    list.push_item(anchor(DisplayRect::new(0.0, 0.0, 100.0, 100.0)));
    assert!(std::ptr::eq(list.hit_test(5.0, 5.0).unwrap(), &list.items()[0]));
    assert!(std::ptr::eq(list.hit_test(15.0, 15.0).unwrap(), &list.items()[3]));
    let found = list.hit_test_filtered(15.0, 15.0, |item| matches!(item, DisplayItem::Text(_))).unwrap();
    assert_eq!(found.map(|(id, _)| id), Some(2));
    assert!(list.hit_test_filtered(30.0, 30.0, |item| matches!(item, DisplayItem::Anchor(_))).unwrap().is_none());
    assert!(list.hit_test(f32::NAN, 0.0).is_none());
}

#[test]
fn text_line_context_does_not_change_baselines_when_scrolling() {
    let mut list = DisplayList::new();
    list.push_item(text(DisplayRect::new(0.0, 50.0, 20.0, 20.0), "left"));
    clip(&mut list, DisplayRect::new(0.0, 0.0, 0.0, 0.0), 1);
    list.push_item(text(DisplayRect::new(900.0, 50.0, 20.0, 20.0), "hidden face"));
    list.push_item(anchor(DisplayRect::new(900.0, 50.0, 20.0, 20.0)));
    let view = DisplayRect::new(0.0, 40.0, 100.0, 50.0);
    assert_eq!(ids(&list.viewport_page(view, 0, 20, DisplayQueryMode::Visible).unwrap()), [0]);
    let page = list.viewport_page(view, 0, 20, DisplayQueryMode::TextLines).unwrap();
    assert_eq!(ids(&page), [0, 2]);
    assert_eq!(page.items[1].clip.width, 0.0);
    assert_eq!(page.items[1].clip.height, 0.0);
}

#[test]
fn malformed_bounds_and_scopes_fail_before_any_partial_result() {
    let view = DisplayRect::new(0.0, 0.0, 20.0, 20.0);
    let mut list = DisplayList::new();
    list.push_item(text(view, "good"));
    clip(&mut list, view, usize::MAX);
    assert_eq!(list.viewport_page(view, 0, 1, DisplayQueryMode::Visible).unwrap_err(),
        DisplayQueryError::InvalidClipRange { index: 1 });
    assert!(list.hit_test(1.0, 1.0).is_none());
    for bounds in [DisplayRect::new(0.0, 0.0, -1.0, 1.0),
        DisplayRect::new(f32::MAX, 0.0, f32::MAX, 1.0),
        DisplayRect::new(0.0, f32::NAN, 1.0, 1.0)] {
        let mut list = DisplayList::new();
        list.push_item(text(bounds, "bad"));
        assert!(matches!(list.viewport_page(view, 0, 1, DisplayQueryMode::Visible),
            Err(DisplayQueryError::InvalidItemBounds { .. })));
    }
}

#[test]
fn clip_depth_empty_viewports_and_cursor_limits_are_explicit() {
    let view = DisplayRect::new(0.0, 0.0, 20.0, 20.0);
    let mut list = DisplayList::new();
    for i in 0..=MAX_DISPLAY_CLIP_DEPTH { clip(&mut list, view, MAX_DISPLAY_CLIP_DEPTH + 1 - i); }
    list.push_item(text(view, "nested"));
    assert!(matches!(list.viewport_page(view, 0, 1, DisplayQueryMode::Visible),
        Err(DisplayQueryError::ClipDepthExceeded { .. })));
    let empty = DisplayList::new();
    assert!(empty.viewport_page(view, 0, 1, DisplayQueryMode::Visible).unwrap().items.is_empty());
    assert!(matches!(empty.viewport_page(view, 1, 1, DisplayQueryMode::Visible), Err(DisplayQueryError::InvalidCursor)));
    assert!(matches!(empty.viewport_page(view, 0, 0, DisplayQueryMode::Visible), Err(DisplayQueryError::InvalidLimit)));
    assert!(matches!(empty.viewport_page(view, 0, 2049, DisplayQueryMode::Visible), Err(DisplayQueryError::InvalidLimit)));
    let mut list = DisplayList::new();
    list.push_item(text(view, "one"));
    assert!(list.viewport_page(DisplayRect::default(), 0, 1, DisplayQueryMode::Visible).unwrap().items.is_empty());
    assert!(list.viewport_page(view, list.items().len(), 1, DisplayQueryMode::Visible).unwrap().items.is_empty());
}

#[test]
fn cache_is_nonsemantic_and_append_invalidates_precompiled_scopes() {
    let view = DisplayRect::new(0.0, 0.0, 20.0, 20.0);
    let mut list = DisplayList::new();
    clip(&mut list, view, 1);
    assert!(list.viewport_page(view, 0, 1, DisplayQueryMode::Visible).is_err());
    list.push_item(text(view, "now complete"));
    let cold = list.clone();
    assert_eq!(ids(&list.viewport_page(view, 0, 1, DisplayQueryMode::Visible).unwrap()), [1]);
    assert_eq!(list, cold);
    list.push_item(text(view, "appended"));
    assert_eq!(ids(&list.viewport_page(view, 0, 10, DisplayQueryMode::Visible).unwrap()), [1, 2]);
    assert_eq!(cold.items().len(), 2);
    fn send_sync<T: Send + Sync>() {}
    send_sync::<DisplayList>();
}

#[test]
fn deep_scroll_skips_offscreen_subtrees_instead_of_scanning_every_item() {
    let mut list = DisplayList::new();
    for i in 0..100_000 { list.push_item(text(DisplayRect::new(0.0, i as f32 * 20.0, 100.0, 20.0), "x")); }
    let view = DisplayRect::new(0.0, 1_900_000.0, 100.0, 100.0);
    let page = list.viewport_page(view, 0, 256, DisplayQueryMode::TextLines).unwrap();
    assert_eq!(ids(&page), [95_000, 95_001, 95_002, 95_003, 95_004]);
    assert!(page.visited_entries <= 2 * LEAF_ITEMS);
    assert_eq!(page.next_index, None);
    assert_eq!(list.hit_test_filtered(5.0, 1_900_005.0, |_| true).unwrap().map(|(id, _)| id), Some(95_000));
}

#[test]
fn deterministic_unordered_rectangles_match_exhaustive_clipped_reference() {
    let mut seed = 17u64;
    let mut next = || { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1); (seed >> 32) as u32 };
    let mut list = DisplayList::new();
    for i in 0..300 {
        let bounds = DisplayRect::new((next() % 100) as f32, (next() % 1000) as f32, 40.0, 30.0);
        if i % 9 == 0 { clip(&mut list, bounds, (299 - i).min(3)); }
        else { list.push_item(if i % 2 == 0 { text(bounds, "utf8-é🙂") } else { anchor(bounds) }); }
    }
    // Final zero-child clips and original item IDs are included in admission.
    let source = list.items();
    for _ in 0..200 {
        let view = DisplayRect::new((next() % 100) as f32, (next() % 1000) as f32, 50.0, 60.0);
        for mode in [DisplayQueryMode::Visible, DisplayQueryMode::TextLines] {
            let mut expected = Vec::new();
            for (i, item) in source.iter().enumerate() {
                if matches!(item, DisplayItem::Clip(_)) { continue; }
                let mut clip = view;
                for (j, previous) in source[..i].iter().enumerate() {
                    if let DisplayItem::Clip(scope) = previous {
                        if i - j <= scope.child_count { clip = intersection(clip, scope.bounds); }
                    }
                }
                let bounds = item.bounds();
                let visible = if mode == DisplayQueryMode::TextLines && matches!(item, DisplayItem::Text(_)) {
                    bounds.y < view.bottom() && bounds.bottom() > view.y
                } else { nonempty(intersection(bounds, clip)) };
                if visible { expected.push(i); }
            }
            let mut actual = Vec::new();
            let mut cursor = 0;
            loop {
                let page = list.viewport_page(view, cursor, 3, mode).unwrap();
                actual.extend(ids(&page));
                if let Some(next) = page.next_index { assert!(next > cursor); cursor = next; } else { break; }
            }
            assert_eq!(actual, expected);
        }
    }
}
