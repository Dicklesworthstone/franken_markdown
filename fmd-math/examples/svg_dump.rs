//! Render formulas' resolved path contours as one SVG (white ink on a dark
//! ground, one row per formula) — the visual check for adjudicating face
//! and placement changes against a LaTeX rendering of the same strings.
//!
//! ```sh
//! cargo run -p fmd-math --example svg_dump --features bundled-faces -- \
//!     '\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}' '\alpha \le \theta' > sheet.svg
//! ```

#![allow(clippy::expect_used)]

use std::fmt::Write as _;

use fmd_math::mbox::PathSeg;

fn main() {
    let sources: Vec<String> = std::env::args().skip(1).collect();
    let engine = fmd_math::Engine::bundled().expect("bundled faces");
    let scale = 48.0;
    let pad = 0.5;
    let mut rows = String::new();
    let mut y_cursor = pad;
    let mut max_width = 0.0_f64;
    for src in &sources {
        let layout = match engine.typeset(src, fmd_math::Style::Display) {
            Ok(layout) => layout,
            Err(e) => {
                eprintln!("{src}: {e}");
                std::process::exit(1);
            }
        };
        let contours = fmd_math::paths::resolve_paths(&engine, &layout).expect("paths");
        let baseline = y_cursor + layout.height;
        let mut d = String::new();
        // SVG is y-down; the layout is y-up with the baseline at 0.
        let pt = |(x, y): (f64, f64)| ((pad + x) * scale, (baseline - y) * scale);
        for contour in &contours {
            let (x, y) = pt(contour.start);
            let _ = write!(d, "M{x:.2} {y:.2}");
            for seg in &contour.segments {
                match seg {
                    PathSeg::Line { to } => {
                        let (x, y) = pt(*to);
                        let _ = write!(d, "L{x:.2} {y:.2}");
                    }
                    PathSeg::Quad { ctrl, to } => {
                        let (cx, cy) = pt(*ctrl);
                        let (x, y) = pt(*to);
                        let _ = write!(d, "Q{cx:.2} {cy:.2} {x:.2} {y:.2}");
                    }
                }
            }
            d.push('Z');
        }
        let _ = writeln!(rows, r#"<path fill="white" fill-rule="nonzero" d="{d}"/>"#);
        max_width = max_width.max(layout.width);
        y_cursor = baseline + layout.depth + pad;
    }
    let w = (max_width + 2.0 * pad) * scale;
    let h = y_cursor * scale;
    println!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.2} {h:.2}">"#
    );
    println!(r##"<rect width="100%" height="100%" fill="#333"/>"##);
    print!("{rows}");
    println!("</svg>");
}
