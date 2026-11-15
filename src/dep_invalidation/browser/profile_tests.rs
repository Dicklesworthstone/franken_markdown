//! Real grammar, layout and transaction coverage for immutable Flow profiles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::Profile;
use crate::dep_invalidation::DependencyGraph;
use crate::display::DisplayRect;
use crate::flow_display::DisplayBlock;

fn create(source: &str, profile: Option<Profile>) -> BrowserFlowSession {
    BrowserFlowSession::new_with_profile(source, "sans", FlowLayoutOptions::default(), profile)
        .unwrap()
}

fn snapshot(state: &BrowserFlowSession) -> String {
    state
        .snapshot_json(state.revision(), state.layout_revision(), 0, 2048, true)
        .unwrap()
}

fn assert_commonmark(state: &BrowserFlowSession, text: &str) {
    assert_eq!(state.profile(), Some(Profile::CommonMarkGfm));
    let engine = state.session.engine();
    assert_eq!(engine.profile(), Some(Profile::CommonMarkGfm));
    assert_eq!(
        engine.blocks()[0],
        DisplayBlock::Paragraph { text: text.into() }
    );
    let runs = engine.inline_runs_for_block(0).unwrap();
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|run| run.style.bold));
    assert!(snapshot(state).contains("\"profile\":\"commonmark-gfm\""));
}

#[test]
fn profile_selects_grammar_before_assigning_block_and_inline_ownership() {
    let source = "**Term\n: definition**\n";
    let commonmark = create(source, Some(Profile::CommonMarkGfm));
    assert_commonmark(&commonmark, "Term : definition");
    assert_eq!(commonmark.session.engine().blocks().len(), 1);
    assert_eq!(
        commonmark.session.engine().source_span_for_block(0),
        Some(SourceSpan::new(0, source.len() - 1))
    );

    let extended = create(source, Some(Profile::GfmPlus));
    assert_eq!(extended.session.engine().blocks().len(), 2);
    assert_ne!(commonmark.session.display(), extended.session.display());

    // Inline ownership crosses the would-be definition boundary in containers
    // too. The same grammar reaches lists, quotations and relocated note bodies.
    let nested = create(
        "> **Quote\n> : description**\n\n- **List\n  : description**\n\nUse[^n].\n\n[^n]: **Note\n    : description**\n",
        Some(Profile::CommonMarkGfm),
    );
    for expected in [
        "Quote : description",
        "List : description",
        "Note : description",
    ] {
        let index = nested
            .session
            .engine()
            .blocks()
            .iter()
            .position(|block| match block {
                DisplayBlock::Paragraph { text }
                | DisplayBlock::Quote { text }
                | DisplayBlock::ListItem { text, .. } => text == expected,
                _ => false,
            })
            .unwrap_or_else(|| panic!("missing {expected:?}"));
        let runs = nested
            .session
            .engine()
            .inline_runs_for_block(index)
            .unwrap();
        assert!(!runs.is_empty());
        assert!(runs.iter().all(|run| run.style.bold), "{expected:?}");
    }
}

#[test]
fn page_break_comments_follow_the_selected_profile_without_invented_source_spans() {
    let source = "before\n\n<!-- pagebreak -->\n\nafter\n";
    for profile in [None, Some(Profile::CommonMarkGfm), Some(Profile::GfmPlus)] {
        let state = create(source, profile);
        let engine = state.session.engine();
        let marker = engine.blocks().get(1).unwrap();
        if profile == Some(Profile::CommonMarkGfm) {
            assert!(
                matches!(marker, DisplayBlock::Paragraph { text } if text.trim() == "<!-- pagebreak -->")
            );
        } else {
            assert_eq!(marker, &DisplayBlock::Rule);
        }
        assert_eq!(
            engine
                .source_span_for_block(1)
                .unwrap()
                .slice(source)
                .unwrap()
                .trim(),
            "<!-- pagebreak -->"
        );
    }
}

#[test]
fn legacy_construction_and_implicit_wire_bytes_remain_unchanged() {
    let source = "Term\n: definition\n\n<!-- pagebreak -->\n\n# Heading\n";
    let legacy = BrowserFlowSession::new(source, "sans", Default::default()).unwrap();
    let implicit = create(source, None);
    let extended = create(source, Some(Profile::GfmPlus));
    assert_eq!(legacy.profile(), None);
    assert_eq!(snapshot(&legacy), snapshot(&implicit));
    assert!(!snapshot(&legacy).contains("\"profile\""));
    assert_eq!(
        snapshot(&legacy),
        snapshot(&extended).replace(",\"profile\":\"gfm-plus\"", "")
    );
    assert_eq!(legacy.session.display(), extended.session.display());
}

#[test]
fn edits_replacements_reflows_and_asset_transactions_preserve_the_selected_grammar() {
    let source = "**Term\n: definition**\n\n![one](one.png)\n\n![two](two.png)\n";
    let mut state = create(source, Some(Profile::CommonMarkGfm));
    assert_commonmark(&state, "Term : definition");
    let assets = state.session.pending_assets().to_vec();
    assert_eq!(assets.len(), 2);
    state
        .session
        .provide_assets(
            assets
                .iter()
                .map(|asset| AssetResult {
                    request_id: asset.id,
                    generation: asset.generation,
                    width: 20,
                    height: 10,
                    bytes: Some(vec![1, 2, 3]),
                })
                .collect(),
            |text, size, role, style| state.fonts.shape(text, size, role, style),
        )
        .unwrap();
    assert_commonmark(&state, "Term : definition");
    assert_eq!(state.session.engine().resolved_assets().len(), 2);

    state
        .set_code_highlighting(state.revision(), state.layout_revision(), true)
        .unwrap();
    state
        .reflow(
            state.revision(),
            state.layout_revision(),
            FlowLayoutOptions {
                viewport_width: 350.0,
                ..FlowLayoutOptions::default()
            },
        )
        .unwrap();
    state
        .edit_utf16(
            state.revision(),
            2,
            6,
            "Changed",
            FlowAssetReuse::HostVerifiedUnchanged,
        )
        .unwrap();
    assert_commonmark(&state, "Changed : definition");
    assert_eq!(state.session.engine().resolved_assets().len(), 2);
    let replaced = state.source().replace("definition", "updated");
    state
        .replace_source(
            state.revision(),
            &replaced,
            FlowAssetReuse::HostVerifiedUnchanged,
        )
        .unwrap();
    assert_commonmark(&state, "Changed : updated");
    state.reload_assets(state.revision()).unwrap();
    assert_commonmark(&state, "Changed : updated");
    assert_eq!(state.session.pending_assets().len(), 2);
    assert!(state.session.engine().resolved_assets().is_empty());
    assert!(state.code_highlighting());

    let before = snapshot(&state);
    let revision = state.revision();
    assert!(
        state
            .edit_utf16(revision - 1, 0, 1, "x", FlowAssetReuse::Invalidate)
            .is_err()
    );
    assert!(
        state
            .reflow(
                revision,
                state.layout_revision(),
                FlowLayoutOptions {
                    viewport_width: 0.0,
                    ..FlowLayoutOptions::default()
                }
            )
            .is_err()
    );
    assert_eq!(snapshot(&state), before);
    assert_commonmark(&state, "Changed : updated");
}

#[test]
fn grammar_identity_is_present_on_every_browser_read_receipt() {
    let state = create("text\n\n![image](asset.png)", Some(Profile::CommonMarkGfm));
    let revision = state.revision();
    let layout = state.layout_revision();
    let item = state
        .session
        .display()
        .items()
        .iter()
        .position(|item| matches!(item, DisplayItem::Text(text) if text.text == "text"))
        .unwrap();
    for json in [
        state.snapshot_json(revision, layout, 0, 100, true).unwrap(),
        state.reading_json(revision, layout, 0, 100).unwrap(),
        state.asset_inventory_json(revision, layout, 0, 100).unwrap(),
        state.pending_assets_json(revision, 0, 100).unwrap(),
        state
            .viewport_json(
                revision,
                layout,
                DisplayRect::new(0.0, 0.0, 800.0, 600.0),
                0,
                100,
                true,
            )
            .unwrap(),
        state.hit_test_json(revision, layout, 1.0, 1.0).unwrap(),
        state.select_item_json(revision, layout, item, 0, 4).unwrap(),
    ] {
        assert_eq!(
            json.matches("\"profile\":\"commonmark-gfm\"").count(),
            1,
            "{json}"
        );
    }
}

#[test]
fn dependency_reuse_requires_matching_grammar_even_for_identical_plain_source() {
    let source = "plain\n\n![image](asset.png)\n";
    let commonmark = DependencyGraph::scan_with_profile(source, Some(Profile::CommonMarkGfm));
    let same = DependencyGraph::scan_with_profile(source, Some(Profile::CommonMarkGfm));
    assert_eq!(commonmark.compare(&same).reusable.len(), 2);
    for other in [None, Some(Profile::GfmPlus)] {
        let changes = commonmark.compare(&DependencyGraph::scan_with_profile(source, other));
        assert!(changes.global_context_changed);
        assert!(changes.reusable.is_empty());
        assert_eq!(changes.dirty_blocks, vec![0, 1]);
        assert_eq!(changes.removed_blocks, vec![0, 1]);
    }
}
