//! PDF stream-decompression helper for integration tests.
//!
//! Slices each `stream ... endstream` payload to exactly `/Length N` bytes
//! (PDF syntax appends a newline before `endstream` that is NOT part of the
//! stream — including it corrupts the zlib Adler-32 trailer), then decompresses
//! with the crate's own validating `zlib_decompress`.

pub fn decompressed_content(pdf: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(pos) = pdf[i..]
        .windows(b"stream\n".len())
        .position(|w| w == b"stream\n")
    {
        let dict_start = i + pos;
        // /Length appears in the dict immediately before the stream keyword.
        let dict_window = &pdf[dict_start.saturating_sub(160)..dict_start];
        let Some(len) = dict_window
            .windows(b"/Length ".len())
            .rposition(|w| w == b"/Length ")
            .and_then(|p| {
                let digits = &dict_window[p + b"/Length ".len()..];
                let end = digits
                    .iter()
                    .position(|b| !b.is_ascii_digit())
                    .unwrap_or(digits.len());
                std::str::from_utf8(&digits[..end])
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
            })
        else {
            i = dict_start + b"stream\n".len();
            continue;
        };
        let start = dict_start + b"stream\n".len();
        let Some(raw) = pdf.get(start..start + len) else {
            break;
        };
        // Content streams are zlib-wrapped (start 0x78); unfiltered payloads
        // (font subsets) are skipped.
        if raw.first() == Some(&0x78) {
            if let Some(dec) = franken_markdown::zlib_decompress(raw, 1 << 24) {
                out.extend_from_slice(&dec);
            }
        }
        i = start + len;
    }
    out
}
