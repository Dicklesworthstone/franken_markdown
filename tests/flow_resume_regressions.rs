//! Cross-boundary regressions for resumable code blocks and host-supplied assets.
//! These compare eager, delayed, and out-of-order asset completion using only
//! the public API; a batch boundary must not change the final display output.

#![forbid(unsafe_code)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use franken_markdown::{
    AssetRequest, AssetResult, DisplayBlock, FlowDisplayError, ResumableFlowDisplay,
};

const SOURCE: &str = concat!(
    "# Preview\n\n",
    "~~~text\n![literal](never-load.png)\n# not a heading\n~~~\n\n",
    "![one](a.png)\nmiddle\n![two](b.png)\nafter\n"
);

fn resolved(request: &AssetRequest) -> AssetResult {
    let ordinal = u32::try_from(request.id.0).expect("small fixture request id");
    AssetResult {
        request_id: request.id,
        generation: request.generation,
        width: 300 + ordinal,
        height: 40 + ordinal,
        bytes: Some(vec![1, 2, 3]),
    }
}

#[test]
fn eager_and_out_of_order_assets_produce_identical_resumed_display_lists() {
    let mut whole = ResumableFlowDisplay::with_generation(SOURCE, usize::MAX, 9);
    whole.process_all().unwrap();
    let requests = whole.unresolved_assets().to_vec();
    assert_eq!(requests.len(), 2, "the fenced image is not a request");
    for request in requests.iter().rev() {
        whole.provide_asset(resolved(request)).unwrap();
    }
    let expected = whole.to_display_list();

    for batch in 1..=SOURCE.lines().count() + 1 {
        let mut stepped = ResumableFlowDisplay::with_generation(SOURCE, batch, 9);
        let mut step_count = 0;
        while let Some(step) = stepped.step().unwrap() {
            step_count += 1;
            // Source scanning and completed-block emission have independent
            // quotas. A container can continue emitting after source EOF.
            assert!(step_count <= SOURCE.lines().count() + whole.blocks().len());
            assert!(step.blocks.len() <= batch);
            for request in step.unresolved_assets {
                assert_ne!(request.url, "never-load.png");
                stepped.provide_asset(resolved(&request)).unwrap();
            }
        }
        assert!(stepped.is_finished());
        assert!(stepped.unresolved_assets().is_empty());
        assert_eq!(stepped.blocks(), whole.blocks(), "batch={batch}");
        let actual = stepped.to_display_list();
        assert_eq!(actual.items(), expected.items(), "batch={batch}");
        assert_eq!(actual.reading_order(), expected.reading_order(), "batch={batch}");
        assert_eq!(actual.total_bounds(), expected.total_bounds(), "batch={batch}");
    }
}

#[test]
fn generation_change_during_a_buffered_fence_preserves_code_and_refreshes_assets() {
    let source = concat!(
        "![one](a.png)\n",
        "```text\n![literal](never-load.png)\n```\n",
        "![two](b.png)\n"
    );
    let mut engine = ResumableFlowDisplay::new(source, 1);
    let first = engine.step().unwrap().unwrap();
    let old_request = first.unresolved_assets[0].clone();
    let old_result = resolved(&old_request);
    engine.provide_asset(old_result.clone()).unwrap();

    let opening_fence = engine.step().unwrap().unwrap();
    assert!(opening_fence.blocks.is_empty());
    assert!(opening_fence.has_more);
    engine.set_generation(2);
    assert!(engine.resolved_assets().is_empty());
    assert_eq!(engine.unresolved_assets().len(), 1);
    assert_eq!(engine.unresolved_assets()[0].id, old_request.id);
    assert_eq!(engine.unresolved_assets()[0].generation, 2);
    assert_eq!(
        engine.provide_asset(old_result),
        Err(FlowDisplayError::StaleAssetGeneration {
            expected: 2,
            actual: 1,
        })
    );

    while let Some(step) = engine.step().unwrap() {
        assert!(step.unresolved_assets.iter().all(|request| {
            request.generation == 2 && request.url != "never-load.png"
        }));
    }
    let code: Vec<_> = engine
        .blocks()
        .iter()
        .filter_map(|block| match block {
            DisplayBlock::CodeBlock { source, .. } => Some(source.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(code, vec!["![literal](never-load.png)\n"]);
    let refreshed = engine.unresolved_assets().to_vec();
    assert_eq!(refreshed.len(), 2);
    for request in refreshed.iter().rev() {
        engine.provide_asset(resolved(request)).unwrap();
    }
    assert!(engine.unresolved_assets().is_empty());
    assert_eq!(engine.resolved_assets().len(), 2);
    assert!(engine.resolved_assets().iter().all(|result| result.generation == 2));
}

#[test]
fn eof_flush_keeps_both_closed_and_unterminated_code_in_reading_order() {
    let source = concat!(
        "before\n```text\nclosed\n```\nbetween\n",
        "~~~text\n![literal](never-load.png)\nlast line"
    );
    let expected = vec![
        DisplayBlock::Paragraph {
            text: "before".to_owned(),
        },
        DisplayBlock::CodeBlock {
            language: Some("text".to_owned()),
            source: "closed\n".to_owned(),
        },
        DisplayBlock::Paragraph {
            text: "between".to_owned(),
        },
        DisplayBlock::CodeBlock {
            language: Some("text".to_owned()),
            source: "![literal](never-load.png)\nlast line\n".to_owned(),
        },
    ];
    for batch in 1..=source.lines().count() + 1 {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        assert_eq!(engine.process_all().unwrap(), expected, "batch={batch}");
        assert_eq!(engine.blocks(), expected.as_slice());
        assert!(engine.unresolved_assets().is_empty());
        assert!(engine.is_finished());
        assert!(engine.step().unwrap().is_none());
    }
}
