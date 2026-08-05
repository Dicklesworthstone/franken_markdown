// probe
fn main() {
    let faces = [
        ("CM_REGULAR", fmd_font::bundled::CM_REGULAR),
        ("CM_ITALIC", fmd_font::bundled::CM_ITALIC),
        ("CM_BOLD", fmd_font::bundled::CM_BOLD),
        ("CM_BOLD_ITALIC", fmd_font::bundled::CM_BOLD_ITALIC),
        ("CM_TYPEWRITER", fmd_font::bundled::CM_TYPEWRITER),
        ("PLEX_REGULAR", fmd_font::bundled::PLEX_REGULAR),
        ("NOTO_SANS_MATH_SYMBOLS", fmd_font::bundled::NOTO_SANS_MATH_SYMBOLS),
    ];
    let chars = [
        ('\u{2640}', "FEMALE"),
        ('\u{2642}', "MALE/MARS"),
        ('\u{2641}', "EARTH"),
        ('\u{00A9}', "COPYRIGHT"),
        ('\u{2713}', "CHECK MARK (ding51)"),
        ('\u{2717}', "BALLOT X (ding55)"),
        ('\u{2640}', "FEMALE2"),
    ];
    for (ch, label) in chars {
        print!("U+{:04X} {}: ", ch as u32, label);
        for (name, bytes) in faces {
            let font = fmd_font::Font::parse(bytes.to_vec()).unwrap();
            let gid = font.glyph_index(ch);
            print!("{}={} ", name, gid);
        }
        println!();
    }
}
