//! Dump the structural layout and resolved path contours of a formula —
//! the quick-look tool for adjudicating placement changes.
//!
//! ```sh
//! cargo run -p fmd-math --example layout_dump --features bundled-faces -- \
//!     '\begin{pmatrix} a & b \\ c & d \end{pmatrix}'
//! ```

#![allow(clippy::expect_used)]

fn main() {
    let src = std::env::args()
        .nth(1)
        .unwrap_or_else(|| r"\left( \frac{a}{x} \right)".to_owned());
    let engine = fmd_math::Engine::bundled().expect("bundled faces");
    match engine.typeset(&src, fmd_math::Style::Display) {
        Ok(layout) => {
            print!("{}", fmd_math::paths::layout_dump(&layout));
            match fmd_math::paths::resolve_paths(&engine, &layout) {
                Ok(contours) => print!("{}", fmd_math::paths::canonical_dump(&contours)),
                Err(e) => eprintln!("path resolution: {e}"),
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
