//! TEMPORARY diagnostic for bead 4vjj regeneration debugging. Not product
//! code; removed before commit.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::text::Font;

fn push16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}
fn push32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

/// Build a minimal sfnt whose cmap12 maps U+1D49C -> gid 1 (one simple glyph).
fn synth_font() -> Vec<u8> {
    // glyf: glyph0 empty, glyph1 minimal simple contour.
    let mut g1 = Vec::new();
    g1.extend_from_slice(&1i16.to_be_bytes());
    g1.extend_from_slice(&[0u8; 8]);
    push16(&mut g1, 0);
    push16(&mut g1, 0);
    g1.extend_from_slice(&[0x01, 0x00]);
    let mut glyf = Vec::new();
    glyf.extend_from_slice(&g1);
    while glyf.len() % 4 != 0 {
        glyf.push(0);
    }
    let mut loca = Vec::new();
    push32(&mut loca, 0);
    push32(&mut loca, 0);
    push32(&mut loca, glyf.len() as u32);

    // head (54): indexToLocFormat=1 long @50, unitsPerEm=1000 @18.
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    head[50..52].copy_from_slice(&1u16.to_be_bytes());

    // hhea (36): numberOfHMetrics=1 @34.
    let mut hhea = vec![0u8; 36];
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());

    // hmtx: one long metric advance=500 lsb=0.
    let mut hmtx = Vec::new();
    push16(&mut hmtx, 500);
    push16(&mut hmtx, 0);

    // maxp (v0.5, 6 bytes: version + numGlyphs).
    let mut maxp = Vec::new();
    push32(&mut maxp, 0x0000_5000);
    push16(&mut maxp, 2);

    // cmap12: U+1D49C..=U+1D49C -> gid 1.
    let mut sub = Vec::new();
    push16(&mut sub, 12);
    push16(&mut sub, 0);
    push32(&mut sub, (16 + 12) as u32);
    push32(&mut sub, 0);
    push32(&mut sub, 1);
    push32(&mut sub, 0x0001_D49C); // start (U+1D49C)
    push32(&mut sub, 0x0001_D49C); // end
    push32(&mut sub, 1); // startGlyphID
    let mut cmap = Vec::new();
    push16(&mut cmap, 0);
    push16(&mut cmap, 1);
    push16(&mut cmap, 3);
    push16(&mut cmap, 10);
    push32(&mut cmap, 12);
    cmap.extend_from_slice(&sub);

    let mut post = vec![0u8; 32];
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());

    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
        (b"post", post),
    ];
    let n = tables.len();
    let mut off = 12 + 16 * n;
    let mut dir = Vec::new();
    dir.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    push16(&mut dir, n as u16);
    // searchRange fields zeroed are tolerated by our reader.
    push16(&mut dir, 0);
    push16(&mut dir, 0);
    push16(&mut dir, (n as u16).wrapping_neg());
    let mut body: Vec<u8> = Vec::new();
    for (tag, data) in tables {
        dir.extend_from_slice(tag);
        push32(&mut dir, 0);
        push32(&mut dir, off as u32);
        push32(&mut dir, data.len() as u32);
        body.extend_from_slice(&data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        off = 12 + 16 * n + body.len();
    }
    dir.extend_from_slice(&body);
    dir
}

fn main() {
    // Part 1: real Noto behavior.
    let bytes = std::fs::read("/tmp/NotoSansMath-Regular.ttf").unwrap();
    let font = Font::parse(bytes).expect("parse source");
    println!("SOURCE: glyphs={}", font.num_glyphs);
    let a = '\u{1D49C}';
    println!("glyph_index(𝒜) = {}", font.glyph_index(a));

    match font.subset(&[a]) {
        Some(out) => {
            std::fs::write("/tmp/subset_one.bin", &out).unwrap();
            let rp = Font::parse(out).expect("reparse");
            println!(
                "REAL-FONT OUTPUT: glyphs={} index(𝒜)={}",
                rp.num_glyphs,
                rp.glyph_index(a)
            );
        }
        None => println!("subset returned None"),
    }

    // Part 2: synthetic minimal plane-1 font through the same API.
    let s = Font::parse(synth_font()).expect("synth parses");
    println!(
        "SYNTH: glyphs={} index(𝒜)={}",
        s.num_glyphs,
        s.glyph_index(a)
    );
    match s.subset(&[a]) {
        Some(out) => {
            std::fs::write("/tmp/subset_synth.bin", &out).unwrap();
            let rp = Font::parse(out).expect("reparse synth");
            println!(
                "SYNTH OUTPUT: glyphs={} index(𝒜)={}",
                rp.num_glyphs,
                rp.glyph_index(a)
            );
        }
        None => println!("synth subset None"),
    }
}
