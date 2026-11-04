//! Flow-owned publication assets must not depend on preview image rectangles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![forbid(unsafe_code)]

use franken_markdown::dep_invalidation::{
    BrowserFlowSession, FlowAssetReuse, FlowSession,
};
use franken_markdown::flow_display::{
    AssetRequest, AssetRequestId, AssetResult, DisplayBlock, FlowDisplayError,
    FlowDisplayLimits, FlowLayoutOptions, ResumableFlowDisplay,
};
use franken_markdown::fonts::BundledFlowFonts;
use franken_markdown::{FontFamily, HtmlOptions, SourceSpan};
use std::collections::BTreeSet;

fn complete(source: &str, batch: usize) -> ResumableFlowDisplay {
    let mut engine = ResumableFlowDisplay::new(source, batch);
    while engine.step().unwrap().is_some() {}
    engine
}

fn payload(request: &AssetRequest, bytes: Option<Vec<u8>>) -> AssetResult {
    AssetResult {
        request_id: request.id,
        generation: request.generation,
        width: 1,
        height: 1,
        bytes,
    }
}

fn session(
    source: &str,
    limits: FlowDisplayLimits,
    fonts: &BundledFlowFonts,
) -> FlowSession {
    FlowSession::new(
        source,
        1,
        limits,
        FlowLayoutOptions::default(),
        |text, size, role, style| fonts.shape(text, size, role, style),
    )
    .unwrap()
}

#[test]
fn discovers_images_in_all_rendered_ast_contexts() {
    let cases = [
        ("# Logo ![brand](heading.png)", "heading.png"),
        ("| ![head](header.png) |\n| --- |\n| body |", "header.png"),
        (
            "| Diagram |\n| --- |\n| **[![plot](cell.png)](https://example.com)** |",
            "cell.png",
        ),
        ("> # Logo ![brand](quote.png)", "quote.png"),
        ("- # Logo ![brand](list.png)", "list.png"),
        ("Term ![term](term.png)\n: Description", "term.png"),
        ("Term\n: Definition ![art](definition.png)", "definition.png"),
        ("Note[^n].\n\n[^n]: # ![note](note.png)", "note.png"),
        ("[^unused]: ![unused](unused.png)", "unused.png"),
        ("Before ![body](body.png) after", "body.png"),
        ("# ![pixel](data:image/png;base64,aA==)", "data:image/png;base64,aA=="),
        ("Before <img src=\"inline.png\" alt=\"Inline\"> after", "inline.png"),
        ("<div>\n<img src=\"block.png\" alt=\"Block\">\n</div>", "block.png"),
        ("| Picture |\n| --- |\n| <img src=\"raw-cell.png\" alt=\"Cell\"> |", "raw-cell.png"),
    ];
    for (source, destination) in cases {
        let engine = complete(source, 1);
        let requests = engine.unresolved_assets();
        assert_eq!(requests.len(), 1, "{source}");
        assert_eq!(requests[0].url, destination, "{source}");
        let occurrences: Vec<_> = engine.asset_occurrences().collect();
        assert_eq!(occurrences.len(), 1, "{source}");
        assert_eq!(occurrences[0].0.id, requests[0].id);
        assert_eq!(occurrences[0].0.reference, destination);
        assert_eq!(occurrences[0].0.source_offset, occurrences[0].1.start);
        assert!(occurrences[0].1.slice(source).is_some());
    }
}

#[test]
fn heading_and_table_preview_remain_truthful_alt_text_without_phantom_images() {
    let source = "# Logo ![brand](same.png)\n\n| Diagram |\n| --- |\n| [![plot](same.png)](https://example.com) |";
    let engine = complete(source, 1);
    assert!(matches!(
        &engine.blocks()[0],
        DisplayBlock::Heading { text, .. } if text == "Logo brand"
    ));
    assert!(engine.blocks().iter().any(
        |block| matches!(block, DisplayBlock::TableRow { cells } if cells.len() == 1 && cells[0] == "plot")
    ));
    assert!(!engine.blocks().iter().any(
        |block| matches!(block, DisplayBlock::UnresolvedAsset(_))
    ));
    assert_eq!(engine.unresolved_assets().len(), 2);
    assert_eq!(
        engine.asset_occurrences().map(|(asset, _)| asset.id).collect::<BTreeSet<_>>().len(),
        2,
    );
    assert!(engine.unresolved_assets().iter().all(|asset| asset.url == "same.png"));
}

#[test]
fn raw_html_discovery_uses_real_tags_and_keeps_each_grant_once_across_image_splits() {
    let source = concat!(
        "Before <img SRC='a&amp;b.png' alt='A &amp; B'> ",
        "![visible](visible.png) after <img src='tail.png'>.\n\n",
        "<div title=\"<img src='quoted.png'>\">\n",
        "<!-- <img src='comment.png'> -->\n",
        "&lt;img src='escaped.png'&gt;\n",
        "<img src='' src='ignored-duplicate.png'>\n",
        "<img src='first.png' src='second.png' alt='first'>\n",
        "<pre><img src='pre.png'></pre>\n",
        "<code><img src='code-tag.png'></code>\n",
        "</div>\n\n",
        "\u{0060}<img src='inline-code.png'>\u{0060}\n\n",
        "~~~html\n<img src='fenced.png'>\n![also literal](fenced-markdown.png)\n~~~\n",
    );
    let engine = complete(source, 1);
    let references: BTreeSet<_> = engine
        .asset_occurrences()
        .map(|(asset, _)| asset.reference.as_str())
        .collect();
    assert_eq!(
        references,
        BTreeSet::from([
            "a&b.png", "visible.png", "tail.png", "first.png", "pre.png", "code-tag.png",
        ]),
    );
    assert_eq!(engine.unresolved_assets().len(), references.len());
    assert_eq!(engine.asset_occurrences().count(), references.len());
    assert_eq!(
        engine.unresolved_assets().iter().find(|asset| asset.url == "a&b.png").unwrap().alt_text,
        "A & B",
    );
    let ids: BTreeSet<_> = engine.unresolved_assets().iter().map(|asset| asset.id).collect();
    assert_eq!(ids.len(), references.len());
}

#[test]
fn generation_changes_reissue_hidden_assets_and_fence_later_resumed_blocks() {
    let source = "# ![heading](heading.png)\n\n| ![header](header.png) |\n| --- |\n| ![cell](cell.png) |\n";
    let mut engine = ResumableFlowDisplay::with_generation(source, 1, 7);
    let first = engine.step().unwrap().unwrap();
    assert_eq!(first.unresolved_assets.len(), 1);
    let old_request = first.unresolved_assets[0].clone();
    engine.provide_asset(payload(&old_request, Some(vec![1, 2]))).unwrap();
    assert_eq!(engine.retained_asset_bytes(), 2);

    engine.set_generation(8);
    assert!(engine.resolved_assets().is_empty());
    assert_eq!(engine.retained_asset_bytes(), 0);
    assert_eq!(engine.unresolved_assets()[0].id, old_request.id);
    assert_eq!(engine.unresolved_assets()[0].generation, 8);
    assert!(matches!(
        engine.provide_asset(payload(&old_request, Some(vec![1, 2]))),
        Err(FlowDisplayError::StaleAssetGeneration { expected: 8, actual: 7 }),
    ));
    while let Some(step) = engine.step().unwrap() {
        assert!(step.unresolved_assets.iter().all(|request| request.generation == 8));
    }
    let mut whole = ResumableFlowDisplay::with_generation(source, usize::MAX, 8);
    whole.process_all().unwrap();
    assert_eq!(engine.unresolved_assets(), whole.unresolved_assets());
    assert_eq!(
        engine.asset_occurrences().map(|(asset, span)| (asset.clone(), span)).collect::<Vec<_>>(),
        whole.asset_occurrences().map(|(asset, span)| (asset.clone(), span)).collect::<Vec<_>>(),
    );
}

#[test]
fn hidden_occurrence_count_and_string_budgets_reject_the_whole_projection() {
    for source in [
        "Before\n\n# ![a](a.png) ![b](b.png)",
        "Before\n\n| ![a](a.png) | ![b](b.png) |\n| --- | --- |",
        "Before\n\n<div><img src='a.png'><img src='b.png'></div>",
        "# ![a](a.png)\n\n| Header |\n| --- |\n| ![b](b.png) |",
    ] {
        let limits = FlowDisplayLimits {
            max_asset_requests: 1,
            ..FlowDisplayLimits::default()
        };
        let mut engine = ResumableFlowDisplay::try_with_limits(source, 1, 1, limits).unwrap();
        let error = engine.step().unwrap_err();
        assert!(matches!(error, FlowDisplayError::BudgetExceeded(_)), "{source}");
        assert!(engine.blocks().is_empty());
        assert!(engine.unresolved_assets().is_empty());
        assert_eq!(engine.asset_occurrences().count(), 0);
        assert_eq!(engine.step().unwrap_err(), error);
    }
    for source in [
        format!("# ![a]({}.png)", "x".repeat(96)),
        format!("| Header |\n| --- |\n| ![a]({}.png) |", "x".repeat(96)),
    ] {
        let limits = FlowDisplayLimits {
            max_output_bytes: 64,
            ..FlowDisplayLimits::default()
        };
        let mut engine = ResumableFlowDisplay::try_with_limits(&source, 1, 1, limits).unwrap();
        assert!(matches!(engine.step(), Err(FlowDisplayError::BudgetExceeded(_))));
        assert!(engine.blocks().is_empty());
        assert!(engine.unresolved_assets().is_empty());
    }
}

#[test]
fn inventory_pages_include_unpainted_occurrences_and_fence_source_and_layout() {
    let source = "# ![one](one.png)\n\n# ![two](two.png)\n\n# ![three](three.png)\n\n# ![four](four.png)";
    let mut browser = BrowserFlowSession::new(source, "sans", FlowLayoutOptions::default()).unwrap();
    let revision = browser.revision();
    let layout = browser.layout_revision();
    let first = browser.asset_inventory_json(revision, layout, 0, 2).unwrap();
    let second = browser.asset_inventory_json(revision, layout, 2, 2).unwrap();
    assert!(first.starts_with(r#"{"schemaVersion":1,"revision":"1","layoutRevision":"1","#));
    assert!(first.contains(r#""offset":0,"total":4,"nextOffset":2"#));
    assert!(first.contains(r#""requestId":"1","destination":"one.png","isResolved":false"#));
    assert!(second.contains(r#""offset":2,"total":4,"nextOffset":null"#));
    assert!(second.contains(r#""requestId":"4","destination":"four.png","isResolved":false"#));
    assert_eq!(first.matches(r#""kind":"image""#).count(), 2);
    assert_eq!(second.matches(r#""kind":"image""#).count(), 2);
    assert!(!browser.snapshot_json(revision, layout, 0, 2048, false).unwrap().contains(r#""kind":"image""#));
    assert!(browser.asset_inventory_json(revision, layout, 4, 2).unwrap().ends_with(r#""items":[]}"#));

    for (offset, limit) in [(5, 1), (0, 0), (0, 2049)] {
        assert_eq!(
            browser.asset_inventory_json(revision, layout, offset, limit).unwrap_err().code(),
            "INVALID_PAGE",
        );
    }
    assert_eq!(browser.asset_inventory_json(revision + 1, layout, 0, 1).unwrap_err().code(), "STALE_REVISION");
    assert_eq!(browser.asset_inventory_json(revision, layout + 1, 0, 1).unwrap_err().code(), "STALE_LAYOUT");

    browser.provide_assets(vec![
        AssetResult { request_id: AssetRequestId(1), generation: revision, width: 1, height: 1, bytes: Some(vec![1, 2]) },
        AssetResult { request_id: AssetRequestId(3), generation: revision, width: 1, height: 1, bytes: None },
    ]).unwrap();
    assert_eq!(browser.layout_revision(), layout + 1);
    assert_eq!(browser.asset_inventory_json(revision, layout, 0, 4).unwrap_err().code(), "STALE_LAYOUT");
    let resolved = browser.asset_inventory_json(revision, browser.layout_revision(), 0, 4).unwrap();
    assert!(resolved.contains(r#""total":4"#));
    assert_eq!(resolved.matches(r#""isResolved":true"#).count(), 2);
    assert_eq!(resolved.matches(r#""isResolved":false"#).count(), 2);
    assert_eq!(browser.asset_bytes(1, revision).unwrap(), Some(&[1, 2][..]));
    assert_eq!(browser.asset_bytes(3, revision).unwrap(), None);
    assert_eq!(browser.asset_bytes(2, revision).unwrap_err().code(), "UNKNOWN_ASSET_REQUEST");
    assert!(browser.pending_assets_json(revision, 0, 4).unwrap().contains(r#""total":2"#));

    browser.reload_assets(revision).unwrap();
    assert_eq!(browser.asset_bytes(1, revision).unwrap_err().code(), "STALE_REVISION");
    assert_eq!(browser.asset_bytes(1, browser.revision()).unwrap_err().code(), "UNKNOWN_ASSET_REQUEST");
    let reloaded = browser.asset_inventory_json(browser.revision(), browser.layout_revision(), 0, 4).unwrap();
    assert_eq!(reloaded.matches(r#""isResolved":false"#).count(), 4);
    assert_eq!(reloaded.matches(r#""kind":"image""#).count(), 4);
}


#[test]
fn raw_images_attach_once_when_paragraphs_begin_with_painted_images() {
    let source = "![first](first.png) <img src='raw-one.png'> ![second](second.png) <img src='raw-two.png'> ![third](third.png)";
    let engine = complete(source, 1);
    assert_eq!(engine.asset_occurrences().count(), 5);
    assert_eq!(engine.unresolved_assets().len(), 5);
    assert_eq!(
        engine.blocks().iter().filter(|block| matches!(block, DisplayBlock::UnresolvedAsset(_))).count(),
        3,
    );
    let destinations: BTreeSet<_> = engine
        .asset_occurrences()
        .map(|(asset, _)| asset.reference.as_str())
        .collect();
    assert_eq!(destinations, BTreeSet::from([
        "first.png", "second.png", "third.png", "raw-one.png", "raw-two.png",
    ]));
    assert_eq!(
        engine.asset_occurrences().map(|(asset, _)| asset.id).collect::<BTreeSet<_>>().len(),
        5,
    );
}

#[test]
fn host_verified_reuse_preserves_each_hidden_occurrence_without_granting_by_url() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let source = "# ![heading](same.png)\n\n| Images | More |\n| --- | --- |\n| ![one](same.png) | ![two](same.png) |\n\nEnd.";
    let mut doc = session(source, FlowDisplayLimits::default(), &fonts);
    assert_eq!(doc.pending_assets().len(), 3);
    let results = doc.pending_assets().iter().enumerate()
        .map(|(index, request)| payload(request, Some(vec![index as u8 + 1])))
        .collect();
    doc.provide_assets(results, |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
    assert!(doc.pending_assets().is_empty());
    assert_eq!(doc.engine().retained_asset_bytes(), 3);

    let update = doc.edit(
        doc.revision(),
        SourceSpan::new(0, 0),
        "![new](new.png)\n\n",
        FlowAssetReuse::HostVerifiedUnchanged,
        |t, s, r, i| fonts.shape(t, s, r, i),
    ).unwrap();
    assert_eq!(update.reused_assets.len(), 3);
    assert_eq!(doc.pending_assets().len(), 1);
    assert_eq!(doc.pending_assets()[0].url, "new.png");
    for (index, remap) in update.reused_assets.iter().enumerate() {
        assert_eq!(remap.new_id.0, remap.old_id.0 + 1);
        let result = doc.engine().resolved_assets().iter()
            .find(|result| result.request_id == remap.new_id).unwrap();
        assert_eq!(result.generation, doc.revision());
        assert_eq!(result.bytes.as_deref(), Some(&[index as u8 + 1][..]));
    }
    // Changing one cell invalidates the entire enclosing table's source/AST
    // attestation, even though all three original destinations still match.
    let changed = doc.source().replace("![one]", "![changed]");
    let update = doc.replace_source(
        doc.revision(),
        &changed,
        FlowAssetReuse::HostVerifiedUnchanged,
        |t, s, r, i| fonts.shape(t, s, r, i),
    ).unwrap();
    assert_eq!(update.reused_assets.len(), 1);
    assert_eq!(doc.engine().resolved_assets().len(), 1);
    assert_eq!(doc.engine().resolved_assets()[0].bytes.as_deref(), Some(&[1][..]));
    assert_eq!(doc.pending_assets().len(), 3);

    let next = format!("{}\n\nAnother paragraph.", doc.source());
    doc.replace_source(
        doc.revision(), &next, FlowAssetReuse::Invalidate,
        |t, s, r, i| fonts.shape(t, s, r, i),
    ).unwrap();
    assert!(doc.engine().resolved_assets().is_empty());
    assert_eq!(doc.engine().retained_asset_bytes(), 0);
    assert_eq!(doc.pending_assets().len(), 4);
}

#[test]
fn hidden_asset_batch_retention_is_atomic_and_preserves_previous_payloads() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let limits = FlowDisplayLimits {
        max_asset_requests: 3,
        max_asset_bytes: 2,
        max_retained_asset_bytes: 3,
        ..FlowDisplayLimits::default()
    };
    let source = "# ![a](a.png)\n\n| ![b](b.png) |\n| --- |\n| ![c](c.png) |";
    let mut doc = session(source, limits, &fonts);
    let requests = doc.pending_assets().to_vec();
    assert_eq!(requests.len(), 3);
    doc.provide_asset(payload(&requests[0], Some(vec![1])), |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
    let layout = doc.layout_revision();
    let display = doc.display().clone();
    let pending = doc.pending_assets().to_vec();
    assert!(doc.provide_assets(
        vec![
            payload(&requests[1], Some(vec![2])),
            payload(&requests[2], Some(vec![3, 4])),
        ],
        |t, s, r, i| fonts.shape(t, s, r, i),
    ).is_err());
    assert_eq!(doc.layout_revision(), layout);
    assert_eq!(doc.display(), &display);
    assert_eq!(doc.pending_assets(), pending.as_slice());
    assert_eq!(doc.engine().retained_asset_bytes(), 1);
    assert_eq!(doc.engine().resolved_assets().len(), 1);

    doc.provide_assets(
        vec![
            payload(&requests[1], Some(vec![2])),
            payload(&requests[2], Some(vec![3])),
        ],
        |t, s, r, i| fonts.shape(t, s, r, i),
    ).unwrap();
    assert!(doc.pending_assets().is_empty());
    assert_eq!(doc.engine().retained_asset_bytes(), 3);
    assert_eq!(doc.layout_revision(), layout + 1);
    assert_eq!(doc.engine().asset_occurrences().count(), 3);
    assert_eq!(doc.engine().resolved_assets()[0].bytes.as_deref(), Some(&[1][..]));
}

fn tiny_png(red: u8) -> Vec<u8> {
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc = u32::MAX;
        for &byte in kind.iter().chain(data) {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 0 {
                    crc >> 1
                } else {
                    (crc >> 1) ^ 0xedb8_8320
                };
            }
        }
        out.extend_from_slice(&(!crc).to_be_bytes());
        out
    }
    let ihdr = [0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0];
    let pixels = franken_markdown::compress::zlib_compress(&[0, red, 0x70, 0xc0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &pixels));
    png.extend(chunk(b"IEND", &[]));
    png
}

#[test]
fn heading_and_table_bytes_owned_by_flow_reach_html_pdf_and_epub_publication() {
    let source = "# ![Heading art](heading.png)\n\n| ![Header art](header.png) |\n| --- |\n| [![Cell art](cell.png)](https://example.com) |";
    let mut engine = complete(source, 1);
    let requests = engine.unresolved_assets().to_vec();
    assert_eq!(requests.len(), 3);
    let pngs: Vec<_> = (1..=3).map(tiny_png).collect();
    for (request, png) in requests.iter().zip(&pngs) {
        engine.provide_asset(payload(request, Some(png.clone()))).unwrap();
    }
    assert!(engine.unresolved_assets().is_empty());

    let mut options = franken_markdown::wasm::WasmRenderOptions {
        custom_css: Some(String::new()),
        ..franken_markdown::wasm::WasmRenderOptions::default()
    };
    for (asset, _) in engine.asset_occurrences() {
        let result = engine.resolved_assets().iter()
            .find(|result| result.request_id == asset.id).unwrap();
        options = options.with_pdf_image_asset(
            asset.reference.clone(), result.bytes.clone().unwrap(),
        ).unwrap();
    }
    let html = franken_markdown::wasm::render_html(source, &options).unwrap();
    assert_eq!(html.html().unwrap().matches("src=\"data:image/png;base64,").count(), 3);

    let pdf = franken_markdown::wasm::render_pdf(source, &options).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf.bytes);
    assert_eq!(pdf_text.matches("/Subtype /Image").count(), 3);
    for alt in ["Heading art", "Header art", "Cell art"] {
        assert!(pdf_text.contains(&format!("/Alt ({alt})")), "{alt}");
    }

    let doc = franken_markdown::parse_markdown(source);
    let epub = franken_markdown::render_epub(&doc, &HtmlOptions {
        image_assets: options.pdf_image_assets,
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    }).unwrap();
    // Raster PNG members are stored in EPUB, preserving the exact owned bytes.
    // Each source occurrence uses distinct pixels, so deduplication cannot mask
    // a missing heading, table header, or linked body-cell resource.
    for png in pngs {
        assert!(epub.windows(png.len()).any(|window| window == png.as_slice()));
    }
}
