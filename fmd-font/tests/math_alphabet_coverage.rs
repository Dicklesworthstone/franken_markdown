//! Runs only when the faces ship in-crate (workspace builds unify this on;
//! an isolated `-p fmd-font` run without the feature skips cleanly).
#![cfg(feature = "bundled-faces")]

//! Math-alphabet coverage of the bundled symbol fallback face (bead 4vjj):
//! `\mathcal` / `\mathbb` route script and double-struck letters through the
//! Unicode mathematical-alphanumeric planes (`fmd-math`'s
//! `faces::calligraphic_char` / `faces::blackboard_char`), whose face chain
//! prefers [`fmd_font::bundled::NOTO_SANS_MATH_SYMBOLS`]. This pins that the
//! committed curated subset actually maps every codepoint either crate layer
//! can emit — with the two sets of Letterlike exceptions called out as the
//! documented exceptions they are.
//!
//! The emit tables are mirrored from `fmd-math/src/faces.rs`; updating one
//! side without the other must fail here.

use fmd_font::{Font, bundled};

/// Script/calligraphic exceptions: the Letterlike slots
/// (`ℬ ℰ ℱ ℋ ℐ ℒ ℳ ℛ`, plus lowercase `ℯ ℊ ℴ`) that resolve in the
/// U+2100 block, never through plane 1.
const CALLIGRAPHIC_UPPER_EXCEPTIONS: [char; 8] = ['B', 'E', 'F', 'H', 'I', 'L', 'M', 'R'];
const CALLIGRAPHIC_LOWER_EXCEPTIONS: [char; 3] = ['e', 'g', 'o'];

/// Double-struck/blackboard exceptions (`ℂ ℍ ℕ ℙ ℚ ℝ ℤ`), likewise
/// Letterlike.
const BLACKBOARD_UPPER_EXCEPTIONS: [char; 7] = ['C', 'H', 'N', 'P', 'Q', 'R', 'Z'];

fn mirrored_calligraphic(ch: char) -> Option<char> {
    match ch {
        _ if CALLIGRAPHIC_UPPER_EXCEPTIONS.contains(&ch) => None,
        _ if CALLIGRAPHIC_LOWER_EXCEPTIONS.contains(&ch) => None,
        'A'..='Z' => Some(char::from_u32(0x1D49C + u32::from(ch) - u32::from('A'))?),
        'a'..='z' => Some(char::from_u32(0x1D4B6 + u32::from(ch) - u32::from('a'))?),
        _ => None,
    }
}

fn mirrored_blackboard(ch: char) -> Option<char> {
    match ch {
        _ if BLACKBOARD_UPPER_EXCEPTIONS.contains(&ch) => None,
        'A'..='Z' => Some(char::from_u32(0x1D538 + u32::from(ch) - u32::from('A'))?),
        'a'..='z' => Some(char::from_u32(0x1D552 + u32::from(ch) - u32::from('a'))?),
        '0'..='9' => Some(char::from_u32(0x1D7D8 + u32::from(ch) - u32::from('0'))?),
        _ => None,
    }
}

fn letterlike_exception_cps() -> impl Iterator<Item = u32> {
    // The four ranges backing every exception letter above.
    [(0x212C_u32, 0x2134_u32), (0x2102, 0x2119)].into_iter().flat_map(|(s, e)| s..=e)
}

#[test]
fn curated_subset_maps_every_emittable_math_alphabet_char() {
    let font =
        Font::parse(bundled::NOTO_SANS_MATH_SYMBOLS.to_vec()).expect("bundled face parses");

    let mut checked = 0usize;
    let mut gaps = Vec::new();
    let alphabet: Vec<char> = (b'0'..=b'9')
        .chain(b'A'..=b'Z')
        .chain(b'a'..=b'z')
        .map(|b| char::from(b))
        .collect();
    for ch in alphabet {
        let candidates: [Option<char>; 2] =
            [mirrored_calligraphic(ch), mirrored_blackboard(ch)];
        for mapped in candidates.into_iter().flatten() {
            checked += 1;
            if font.glyph_index(mapped) == 0 {
                gaps.push((ch, mapped));
            }
        }
    }
    // Mirrored emit totals: script 18 upper + 23 lower (their exceptions
    // resolve through the Letterlike block), double-struck 19 upper +
    // 26 lower letters, plus all 10 double-struck digits.
    assert_eq!(checked, 96, "emit-table mirror drifted");
    assert!(
        gaps.is_empty(),
        "symbol subset lacks {} math-alphabet glyph(s): {}",
        gaps.len(),
        gaps.iter().map(|(_, m)| format!("U+{:04X}", *m as u32)).collect::<String>()
    );
}

#[test]
fn letterlike_exceptions_still_map_in_the_2100_block() {
    let font =
        Font::parse(bundled::NOTO_SANS_MATH_SYMBOLS.to_vec()).expect("bundled face parses");

    for cp in letterlike_exception_cps() {
        // Every Letterlike codepoint carried by the curated range must keep
        // mapping; this is what makes the plane-1 holes harmless.
        if let Some(c) = char::from_u32(cp) {
            if CALLIGRAPHIC_UPPER_EXCEPTIONS
                .iter()
                .chain(BLACKBOARD_UPPER_EXCEPTIONS.iter())
                .any(|&e| e == c)
            {
                assert_ne!(
                    font.glyph_index(c),
                    0,
                    "exception letter {c:?} (U+{cp:04X}) lost"
                );
            }
        }
    }
}
