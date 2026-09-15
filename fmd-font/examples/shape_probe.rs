//! Plain-text oracle interchange for explicitly segmented shaping runs.
use fmd_font::{
    Font,
    shaping::{Direction, ShapeOptions},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let f = Font::parse(std::fs::read(&args[1])?)?;
    let arabic = args[2] == "arab";
    let options = ShapeOptions {
        script: if arabic { *b"arab" } else { *b"latn" },
        direction: if arabic {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        },
        ..ShapeOptions::default()
    };
    let run = f.shape(&args[3], &options)?;
    for g in run.glyphs {
        println!(
            "{} {} {} {} {} {} {}",
            g.glyph_id,
            g.cluster.start,
            g.cluster.end,
            g.x_advance,
            g.y_advance,
            g.x_offset,
            g.y_offset
        );
    }
    Ok(())
}
