#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn fixture(encoded: &str) -> Vec<u8> {
    let encoded: String = encoded.split_whitespace().collect();
    super::super::decode_svg_base64_payload(&encoded).unwrap()
}

fn animated() -> Vec<u8> {
    fixture(include_str!(
        "../../tests/fixtures/gif/animated-colors.base64"
    ))
}

#[test]
fn animated_first_frame_retains_palette_colors_and_transparency() {
    let bytes = animated();
    let frame = decode_first_frame(&bytes).unwrap();
    assert_eq!((frame.width, frame.height), (2, 2));
    assert_eq!(frame.rgb, [255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]);
    assert_eq!(frame.alpha.unwrap(), [255, 255, 255, 0]);
    assert_eq!(dimensions(&bytes), Some((2, 2)));
}

#[test]
fn independent_interlaced_palette_fixture_crosses_lzw_code_widths() {
    // Pillow 12.3.0 encoded these 96x96 palette indices independently, using
    // interlacing and enough entropy to grow through 12-bit dictionary codes.
    let bytes = fixture(include_str!(
        "../../tests/fixtures/gif/interlaced-palette.base64"
    ));
    let frame = decode_first_frame(&bytes).unwrap();
    assert_eq!((frame.width, frame.height), (96, 96));
    assert!(frame.alpha.is_none());
    let mut state = 0x1234_5678u32;
    let expected: Vec<u8> = (0..96 * 96)
        .flat_map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let index = (state >> 24) as u8;
            [index, 255 - index, index.wrapping_mul(73)]
        })
        .collect();
    assert_eq!(frame.rgb, expected);
}

// A two-color image assembled from explicit GIF LZW codes. The byte packing
// is independent of dictionary expansion; each code's width is supplied by
// the caller so boundary/reset tests cannot share the decoder's width logic.
fn blocks(codes: &[(u16, u8)]) -> Vec<u8> {
    let mut raw = Vec::new();
    let mut bits = 0u32;
    let mut used = 0;
    for &(code, width) in codes {
        bits |= u32::from(code) << used;
        used += width;
        while used >= 8 {
            raw.push(bits as u8);
            bits >>= 8;
            used -= 8;
        }
    }
    if used != 0 {
        raw.push(bits as u8);
    }
    let mut result = Vec::new();
    // Three-byte sub-blocks force variable-width codes across framing bytes.
    for part in raw.chunks(3) {
        result.push(part.len() as u8);
        result.extend_from_slice(part);
    }
    result.push(0);
    result
}

fn tiny(width: u16, height: u16, codes: &[(u16, u8)]) -> Vec<u8> {
    let mut bytes = b"GIF87a".to_vec();
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[0x80, 0, 0, 255, 0, 0, 0, 255, 0]);
    bytes.extend_from_slice(&[0x2c, 0, 0, 0, 0]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[0, 2]);
    bytes.extend(blocks(codes));
    bytes.push(0x3b);
    bytes
}

#[test]
fn repeated_string_special_code_and_clear_reset_are_supported() {
    // 0, code6=(0,0), then a reset and one green pixel.
    let bytes = tiny(4, 1, &[(4, 3), (0, 3), (6, 3), (4, 3), (1, 3), (5, 3)]);
    let frame = decode_first_frame(&bytes).unwrap();
    assert_eq!(frame.rgb, [255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0]);
    assert!(frame.alpha.is_none());
}

#[test]
fn unused_transparent_index_does_not_require_a_palette_entry() {
    // The transparency marker is a value to skip, not an opaque color to
    // look up. Real encoders can name a marker beyond a reduced palette even
    // when no pixel uses it. Opaque pixel indices still require palette bytes.
    for transparent in [2, 15, 255] {
        let mut bytes = tiny(1, 1, &[(4, 3), (1, 3), (5, 3)]);
        bytes.splice(19..19, [0x21, 0xf9, 4, 1, 0, 0, transparent, 0]);
        let frame = decode_first_frame(&bytes).unwrap();
        assert_eq!(frame.rgb, [0, 255, 0]);
        assert!(frame.alpha.is_none());
    }
}

#[test]
fn dictionary_can_remain_full_until_an_explicit_clear() {
    let mut codes = vec![(4, 3), (0, 3)];
    // With an initial two-bit alphabet, table entries 6..4095 are created
    // by the following literal codes. Reading the last code in each range
    // grows the next input code width, independent of its numeric value.
    for (count, width) in [
        (2, 3),
        (8, 4),
        (16, 5),
        (32, 6),
        (64, 7),
        (128, 8),
        (256, 9),
        (512, 10),
        (1024, 11),
        (2048, 12),
    ] {
        codes.extend(std::iter::repeat_n((1, width), count));
    }
    codes.extend(std::iter::repeat_n((0, 12), 20));
    codes.extend([(4, 12), (1, 3), (5, 3)]);
    let blocks = blocks(&codes);
    let mut pixels = Vec::new();
    decode_lzw(&blocks, 2, 4112, |pixel| {
        pixels.push(pixel);
        Some(())
    })
    .unwrap();
    assert_eq!(pixels.len(), 4112);
    assert_eq!(pixels[0], 0);
    assert!(pixels[1..4091].iter().all(|&p| p == 1));
    assert!(pixels[4091..4111].iter().all(|&p| p == 0));
    assert_eq!(pixels[4111], 1);
}

#[test]
fn first_frame_offsets_and_local_palette_preserve_logical_screen() {
    let mut bytes = tiny(1, 1, &[(4, 3), (1, 3), (5, 3)]);
    bytes[6..10].copy_from_slice(&[3, 0, 2, 0]);
    bytes[20..24].copy_from_slice(&[1, 0, 1, 0]);
    bytes[28] = 0x80;
    bytes.splice(29..29, [0, 0, 255, 255, 255, 0]);
    let frame = decode_first_frame(&bytes).unwrap();
    assert_eq!((frame.width, frame.height), (3, 2));
    assert!(frame.alpha.is_none());
    let mut expected = [255, 0, 0].repeat(6);
    expected[12..15].copy_from_slice(&[255, 255, 0]);
    assert_eq!(frame.rgb, expected);

    // The same offset frame without a global table has no background color.
    bytes[10] = 0;
    bytes.drain(13..19);
    let frame = decode_first_frame(&bytes).unwrap();
    assert_eq!(frame.alpha.unwrap(), [0, 0, 0, 0, 255, 0]);
}

#[test]
fn truncated_invalid_and_oversized_images_fail_without_partial_pixels() {
    let bytes = tiny(1, 1, &[(4, 3), (1, 3), (5, 3)]);
    for end in 0..bytes.len() {
        assert!(decode_first_frame(&bytes[..end]).is_none(), "prefix {end}");
    }
    for (at, replacement) in [(6, 0), (10, 0), (11, 2), (20, 1), (24, 0), (29, 1), (29, 9)] {
        let mut damaged = bytes.clone();
        damaged[at] = replacement;
        assert!(decode_first_frame(&damaged).is_none(), "byte {at}");
    }
    let mut oversized = bytes.clone();
    oversized[6..10].copy_from_slice(&[0xff; 4]);
    assert!(dimensions(&oversized).is_none());
    assert!(decode_first_frame(&oversized).is_none());
    for codes in [
        vec![(4, 3), (5, 3)],                 // too few pixels
        vec![(4, 3), (1, 3), (1, 3), (5, 3)], // too many pixels
        vec![(4, 3), (6, 3), (5, 3)],         // undefined first string
        vec![(4, 3), (7, 3), (5, 3)],         // unknown dictionary entry
        vec![(4, 3), (3, 3), (5, 3)],         // out-of-palette literal
        vec![(4, 3), (1, 3)],                 // missing EOI
    ] {
        assert!(
            decode_first_frame(&tiny(1, 1, &codes)).is_none(),
            "{codes:?}"
        );
    }
}
