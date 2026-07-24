//! Span-map demo: typeset a formula and print every primitive's source
//! provenance — the §11.3 contract, inspectable from the command line.
//!
//! ```sh
//! cargo run -p fmd-math --example span_probe -- '\frac{a}{b}'
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn main() {
    let src_owned = std::env::args().nth(1);
    let src = src_owned.as_deref().unwrap_or(r"\frac{a}{b} + \sqrt{x}");
    let engine = fmd_math::Engine::bundled().expect("bundled faces");
    match engine.typeset(src, fmd_math::Style::Display) {
        Ok(layout) => {
            println!("source: {src}");
            for g in &layout.glyphs {
                println!(
                    "  glyph {:?} face {} at ({:.3}, {:.3}) size {:.2} ← bytes {}..{} {:?}",
                    g.ch,
                    g.face.0,
                    g.x,
                    g.y,
                    g.size,
                    g.span.start,
                    g.span.end,
                    &src[g.span.start..g.span.end]
                );
            }
            for r in &layout.rules {
                println!(
                    "  rule at ({:.3}, {:.3}) {:.3}×{:.3} ← bytes {}..{}",
                    r.x, r.y, r.width, r.height, r.span.start, r.span.end
                );
            }
        }
        Err(e) => println!("{e}"),
    }
}
