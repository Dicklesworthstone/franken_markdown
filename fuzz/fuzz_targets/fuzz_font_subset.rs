//! Coverage-guided fuzz target: Font::parse + subset + glyph lookup (m7fs.1).
#![no_main]

use fmd_font::Font;
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let slice = if data.len() > MAX_INPUT {
        &data[..MAX_INPUT]
    } else {
        data
    };
    let Ok(font) = Font::parse(slice.to_vec()) else {
        return;
    };
    let keep: Vec<char> = slice.iter().take(16).map(|&b| char::from(b)).collect();
    let _ = font.subset(&keep);
    let gid = font.glyph_index('A');
    let _ = font.has_glyf_outlines();
    let _ = font.glyph_data(gid);
    let _ = font.glyph_bbox(gid);
    let _ = font.advance_width(gid);
});
