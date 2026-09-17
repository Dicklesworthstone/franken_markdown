//! Run with: cargo run --example flow_edit_session --no-default-features
//! No window system, filesystem assets, network or third-party render engine.

use franken_markdown::dep_invalidation::{
    FlowAssetReuse, FlowSession, FlowShapeCache, FlowShapeCacheLimits,
};
use franken_markdown::flow_display::{FlowDisplayLimits, FlowLayoutOptions};
use franken_markdown::{FontFamily, SourceSpan};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut fonts = FlowShapeCache::new(FontFamily::Sans, FlowShapeCacheLimits::default())?;
    let source = "# Editor\n\nOriginal paragraph with **bold** text.\n\n[Open][r]\n\n[r]: https://example.com/old\n";
    let mut session = FlowSession::new(
        source, 32, FlowDisplayLimits::default(), FlowLayoutOptions::default(),
        |text, size, role, style| fonts.shape(text, size, role, style),
    )?;

    let start = source.find("Original").ok_or("example source lacks edit marker")?;
    let update = session.edit(
        session.revision(), SourceSpan::new(start, start + "Original".len()), "Updated",
        FlowAssetReuse::Invalidate,
        |text, size, role, style| fonts.shape(text, size, role, style),
    )?;
    let options = FlowLayoutOptions { viewport_width: 360.0, ..session.layout_options() };
    session.reflow(options, |text, size, role, style| fonts.shape(text, size, role, style))?;

    println!("source revision: {}; layout revision: {}", session.revision(), session.layout_revision());
    println!("verified reusable blocks: {}; dirty blocks: {}", update.changes.reusable.len(), update.changes.dirty_blocks.len());
    println!("drawing items: {}; pending assets: {}", session.display().items().len(), session.pending_assets().len());
    let stats = fonts.stats();
    println!("shape cache: {} hits, {} misses, {} retained payload bytes", stats.hits, stats.misses, stats.retained_payload_bytes);
    Ok(())
}
