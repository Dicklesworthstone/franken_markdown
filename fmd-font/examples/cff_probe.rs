//! Independent-oracle interchange: one line per original glyph command.
use fmd_font::{Font, cff::Command};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let font = Font::parse(std::fs::read(&args[1])?)?;
    for gid in 0..font.num_glyphs {
        let outline = font.cff_outline(gid)?;
        println!("G {gid} {} {}", outline.advance, outline.lsb);
        for c in outline.commands {
            match c {
                Command::Move(p) => println!("M {} {}", p.x, p.y),
                Command::Line(p) => println!("L {} {}", p.x, p.y),
                Command::Curve(a, b, c) => {
                    println!("C {} {} {} {} {} {}", a.x, a.y, b.x, b.y, c.x, c.y)
                }
                Command::Close => println!("Z"),
            }
        }
    }
    if let Some(path) = args.get(2) {
        let subset = font.try_subset_glyphs(&[1, 5, 73, 100, 500, 1000, 2000, 3000], &[])?;
        std::fs::write(path, subset.bytes)?;
    }
    Ok(())
}
