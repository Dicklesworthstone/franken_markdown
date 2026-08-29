#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
fn main() {
    let pdf = std::fs::read("/tmp/dbg-lib.pdf").unwrap();
    // find FIRST real stream (preceded by dict with FlateDecode), not "endstream"
    let pat = b"stream\n";
    let mut i = 0;
    let mut n = 0;
    while let Some(pos) = pdf[i..].windows(pat.len()).position(|w| w == pat) {
        let start = i + pos + pat.len();
        let end_rel = pdf[start..]
            .windows(9)
            .position(|w| w == b"endstream")
            .unwrap();
        let raw = &pdf[start..start + end_rel];
        n += 1;
        println!(
            "stream {n}: raw len {} first bytes {:02x?}",
            raw.len(),
            &raw[..4]
        );
        match franken_markdown::zlib_decompress(raw, 1 << 24) {
            Some(dec) => println!(
                "  ok: {} bytes, Tz: {}",
                dec.len(),
                dec.windows(4).filter(|w| *w == b" Tz ").count()
            ),
            None => println!("  FAILED"),
        }
        i = start + end_rel + 9;
    }
}
