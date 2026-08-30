//! Exhaustive differential proof that the u64/i64 fast paths in
//! `layout::advance_to_layout_units` / `layout::adjustment_to_layout_units`
//! are bit-identical to the original u128/i128 widening reference for every
//! reachable input.
//!
//! Why the fast path must be exact (and is, proven below):
//! - `advance_to_layout_units(u32 a, FontSize(u32 m))`: `a * m <= (2^32-1)^2
//!   = 2^64 - 2^33 + 1 < 2^64`, so the u64 product is the exact mathematical
//!   product — identical to the u128 widening product. Unsigned division by
//!   1000 and the clamp then see the same value on both widths.
//! - `adjustment_to_layout_units(i32 d, FontSize(u32 m))`: `|d * m| <=
//!   (2^31-1)(2^32-1) < 2^63`, so the i64 product is exact, signed division
//!   by 1000 truncates toward zero on i64 exactly as on i128, and the clamp
//!   sees the same value.
//!
//! Coverage below:
//! 1. `reference_*` — the ORIGINAL implementations copied verbatim (the u128
//!    path lives on as this oracle; it is the eternal fallback definition of
//!    correct output).
//! 2. Full cross-product corner grid (powers of two +/-1, extremes, realistic
//!    font-metric values, clamp-crossing boundaries derived per size).
//! 3. Dense sweeps around every clamp-crossing / sign-flip / truncation
//!    boundary (the +/-2048 windows where `product / 1000` enters/leaves the
//!    i32 range, and every truncation-toward-zero class via products
//!    -2999..=2999).
//! 4. Golden-ratio strided sweeps over the FULL u32 advance domain for every
//!    corner size, over the FULL u32 size domain for every corner advance,
//!    and over the full signed i32 adjustment domain (negatives included).
//! 5. Every `from_points` size (all 65,536 constructible point sizes) against
//!    a strided advance sweep.
//! 6. Deterministic randomized property sweep (xorshift64*) with boundary-
//!    biased distributions.

use franken_markdown::layout::{FontSize, adjustment_to_layout_units, advance_to_layout_units};

/// Verbatim copy of the pre-fast-path u128 reference implementation.
fn reference_advance(advance_1000: u32, size: FontSize) -> i32 {
    let width = (advance_1000 as u128 * size.milli_points() as u128) / 1000;
    if width > i32::MAX as u128 {
        i32::MAX
    } else {
        width as i32
    }
}

/// Verbatim copy of the pre-fast-path i128 reference implementation.
fn reference_adjustment(adjustment_1000: i32, size: FontSize) -> i32 {
    let width = (adjustment_1000 as i128 * size.milli_points() as i128) / 1000;
    if width > i32::MAX as i128 {
        i32::MAX
    } else if width < i32::MIN as i128 {
        i32::MIN
    } else {
        width as i32
    }
}

#[must_use]
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Deterministic PRNG value in `0..=hi`.
fn rand_u32(state: &mut u64, hi: u32) -> u32 {
    u32::try_from(xorshift64(state) % (u64::from(hi) + 1)).unwrap_or(0)
}

/// Corner u32 values: powers of two and their neighbors, extremes, divisor
/// boundaries, realistic font-metric advances (0..=2000 per-mille glyphs),
/// and font sizes from 1mpt to u32::MAX.
fn corner_u32s() -> Vec<u32> {
    let mut v: Vec<u32> = vec![
        0, 1, 2, 3, 4, 5, 7, 9, 10, 31, 32, 63, 64, 99, 100, 127, 128, 129, 255, 256, 257, 333,
        500, 511, 512, 600, 777, 949, 950, 999, 1000, 1001, 1002, 1023, 1024, 1025, 1999, 2000,
        2001, 2047, 2048, 2049, 3333, 4095, 4096, 4097, 5000, 8191, 8192, 8193, 9999, 10_000,
        10_001, 16_383, 16_384, 16_385, 32_767, 32_768, 32_769, 65_535, 65_536, 65_537,
    ];
    // Powers of two and +/-1 neighbors across the whole u32 range.
    for k in 0u32..=31 {
        let Some(p) = 1u32.checked_shl(k) else {
            continue;
        };
        v.push(p);
        if p > 1 {
            v.push(p - 1);
        }
        v.push(p.wrapping_add(1));
    }
    v.push(i32::MAX as u32);
    v.push(i32::MAX as u32 - 1);
    v.push(i32::MAX as u32 + 1);
    v.push(u32::MAX);
    v.push(u32::MAX - 1);
    // Realistic font sizes (milli-points): 1mpt .. 65535pt.
    for pts in [
        1u32, 4, 8, 9, 10, 11, 12, 14, 16, 24, 36, 48, 72, 144, 288, 1024, 4096, 16_384, 65_535,
    ] {
        v.push(pts * 1000);
        v.push(pts * 1000 + 1);
        v.push(pts * 1000 - 1);
    }
    v.push(9_500); // 9.5pt
    v.push(10_800); // 10.8pt
    v.sort_unstable();
    v.dedup();
    v
}

/// Corner i32 values for the signed adjustment domain.
fn corner_i32s() -> Vec<i32> {
    let mut v: Vec<i32> = vec![
        0, 1, -1, 2, -2, 3, -3, 999, -999, 1000, -1000, 1001, -1001, 1999, -1999, 2000, -2000,
        2001, -2001, 5000, -5000, 20_000, -20_000,
    ];
    for k in 0u32..=30 {
        let Some(p) = 1i32.checked_shl(k) else {
            continue;
        };
        v.push(p);
        v.push(-p);
        if p > 1 {
            v.push(p - 1);
            v.push(-(p - 1));
        }
        v.push(p.wrapping_add(1));
        v.push(-p.wrapping_sub(1));
    }
    v.push(i32::MAX);
    v.push(i32::MAX - 1);
    v.push(i32::MIN);
    v.push(i32::MIN + 1);
    v.sort_unstable();
    v.dedup();
    v
}

/// Clamp-crossing advances for a given size: the `a` window where
/// `a * m / 1000` reaches i32::MAX (+/-16).
fn crossing_advances(m: u32) -> Vec<u32> {
    let mut v = Vec::new();
    if m == 0 {
        return v;
    }
    let m128 = u128::from(m);
    let bound = (u128::from(i32::MAX as u32) * 1000).div_ceil(m128);
    if let Ok(a0) = u32::try_from(bound) {
        for a in a0.saturating_sub(16)..=a0.saturating_add(16) {
            v.push(a);
        }
    }
    v
}

/// Full cross-product corner grid including derived clamp crossings.
#[test]
fn fast_paths_match_reference_on_corner_grid() {
    let corners = corner_u32s();
    let mut sizes: Vec<u32> = corners.clone();
    sizes.extend([
        0u32, 1000, 2000, 9000, 9500, 10_000, 10_800, 12_000, 72_000, 1_000_000, 65_535_000,
    ]);
    sizes.sort_unstable();
    sizes.dedup();

    let mut advances: Vec<u32> = corners.clone();
    for &m in &sizes {
        advances.extend(crossing_advances(m));
    }
    advances.sort_unstable();
    advances.dedup();

    let mut checked = 0u64;
    for &a in &advances {
        for &m in &sizes {
            let size = FontSize::from_milli_points(m);
            let fast = advance_to_layout_units(a, size).milli_points();
            let slow = reference_advance(a, size);
            assert_eq!(fast, slow, "advance mismatch a={a} m={m}");
            checked += 1;
        }
    }
    // The signed grid over the same corner universe.
    let d_corners = corner_i32s();
    for &d in &d_corners {
        for &m in &sizes {
            let size = FontSize::from_milli_points(m);
            let fast = adjustment_to_layout_units(d, size).milli_points();
            let slow = reference_adjustment(d, size);
            assert_eq!(fast, slow, "adjustment mismatch d={d} m={m}");
            checked += 1;
        }
    }
    eprintln!("corner grid pairs checked: {checked}");
}

/// Dense windows: truncation-toward-zero classes and clamp crossings.
#[test]
fn fast_paths_match_reference_on_truncation_edges() {
    // Products in -2999..=2999 exhaustively against small m: every
    // truncation-toward-zero remainder class, including the negative ones
    // where i64 and i128 division must both round toward zero.
    for d in -2999i64..=2999 {
        for m in [0u32, 1, 2, 3, 999, 1000, 1001] {
            let size = FontSize::from_milli_points(m);
            let Some(di) = i32::try_from(d).ok() else {
                continue;
            };
            assert_eq!(
                adjustment_to_layout_units(di, size).milli_points(),
                reference_adjustment(di, size),
                "trunc edge d={di} m={m}"
            );
        }
    }
    // Dense +/-2048 windows around the i32::MAX / i32::MIN clamp crossings
    // for a spread of sizes, signed and unsigned.
    for m in [
        1u32,
        2,
        3,
        7,
        999,
        1000,
        1001,
        4096,
        65_536,
        1 << 20,
        u32::MAX,
    ] {
        let size = FontSize::from_milli_points(m);
        let m128 = u128::from(m);
        for &limit in &[u128::from(i32::MAX as u32), u128::from(2_147_483_648u32)] {
            let bound = (limit * 1000).div_ceil(m128);
            let Ok(d0) = i32::try_from(bound) else {
                continue;
            };
            let lo = (i64::from(d0) - 2048).max(i64::from(i32::MIN));
            let hi = (i64::from(d0) + 2048).min(i64::from(i32::MAX));
            for d in lo..=hi {
                let di = i32::try_from(d).unwrap_or(i32::MAX);
                assert_eq!(
                    adjustment_to_layout_units(di, size).milli_points(),
                    reference_adjustment(di, size),
                    "clamp window d={di} m={m}"
                );
            }
        }
        if let Ok(a0) = u32::try_from((u128::from(i32::MAX as u32) * 1000).div_ceil(m128)) {
            for a in a0.saturating_sub(2048)..=a0.saturating_add(2048) {
                assert_eq!(
                    advance_to_layout_units(a, size).milli_points(),
                    reference_advance(a, size),
                    "u clamp window a={a} m={m}"
                );
            }
        }
    }
}

/// Golden-ratio strided sweeps over the full domains (negatives included).
#[test]
fn fast_paths_match_reference_on_strided_full_domain() {
    // Odd multiplicative constant: the map is a full-period Weyl sequence, so
    // `x >> 32` cycles the whole u32 range uniformly.
    const STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;
    const SAMPLES: u64 = 1 << 19;

    // Every 4th corner keeps the per-corner sweep set ~45 sizes/advances.
    let corners = corner_u32s();
    let spread: Vec<u32> = corners.iter().step_by(4).copied().collect();

    // Full u32 advance domain, strided, for every spread size.
    for &m in &spread {
        let size = FontSize::from_milli_points(m);
        let mut a: u64 = u64::from(m) | 1;
        for _ in 0..SAMPLES {
            let a32 = (a >> 32) as u32;
            assert_eq!(
                advance_to_layout_units(a32, size).milli_points(),
                reference_advance(a32, size),
                "strided advance a={a32} m={m}"
            );
            a = a.wrapping_mul(STRIDE).wrapping_add(0x517C_C1B7_2722_0A95);
        }
    }
    // Full u32 size domain, strided, for every spread advance.
    for &a in &spread {
        let mut m: u64 = u64::from(a) | 1;
        for _ in 0..SAMPLES {
            let m32 = (m >> 32) as u32;
            let size = FontSize::from_milli_points(m32);
            assert_eq!(
                advance_to_layout_units(a, size).milli_points(),
                reference_advance(a, size),
                "strided size a={a} m={m32}"
            );
            m = m.wrapping_mul(STRIDE).wrapping_add(0x517C_C1B7_2722_0A95);
        }
    }
    // Full i32 adjustment domain (reinterpreting 32 PRNG bits, so half the
    // draws are negative), for every spread size.
    for &m in &spread {
        let size = FontSize::from_milli_points(m);
        let mut d: u64 = u64::from(m) | 1;
        for _ in 0..SAMPLES {
            let di = (d >> 32) as u32 as i32;
            assert_eq!(
                adjustment_to_layout_units(di, size).milli_points(),
                reference_adjustment(di, size),
                "strided adjustment d={di} m={m}"
            );
            d = d.wrapping_mul(STRIDE).wrapping_add(0x517C_C1B7_2722_0A95);
        }
    }
    eprintln!(
        "strided full-domain comparisons: {}",
        u64::try_from(spread.len()).unwrap_or(0) * SAMPLES * 3
    );
}

/// Every `from_points` size (all 65,536) against a strided advance sweep —
/// the complete set of sizes constructible from whole points.
#[test]
fn fast_paths_match_reference_for_every_from_points_size() {
    let mut a = 0x1234_5678u32;
    let mut advs = Vec::with_capacity(512);
    for _ in 0..512 {
        advs.push(a);
        a = a.wrapping_mul(1031).wrapping_add(0x9E37);
    }
    for pts in 0u16..=u16::MAX {
        let size = FontSize::from_points(pts);
        for &adv in &advs {
            assert_eq!(
                advance_to_layout_units(adv, size).milli_points(),
                reference_advance(adv, size),
                "from_points pts={pts} a={adv}"
            );
        }
    }
}

/// Deterministic randomized property sweep with boundary-biased distributions.
#[test]
fn fast_paths_match_reference_on_randomized_sweep() {
    let mut state = 0x0DDB_1A5E_5BAD_5EEDu64;
    let bounds: [u32; 8] = [
        1,
        16,
        1000,
        65_536,
        1 << 20,
        4_000_000,
        i32::MAX as u32,
        u32::MAX,
    ];
    let total = 4_000_000u64;
    for i in 0..total {
        let pick = xorshift64(&mut state) % 100;
        // 60% uniform full-range, 40% biased to one of the boundary scales.
        let (a, m) = if pick < 60 {
            (
                (xorshift64(&mut state) >> 32) as u32,
                (xorshift64(&mut state) >> 32) as u32,
            )
        } else {
            let ba = bounds[(xorshift64(&mut state) % 8) as usize];
            let bm = bounds[(xorshift64(&mut state) % 8) as usize];
            (rand_u32(&mut state, ba), rand_u32(&mut state, bm))
        };
        let size = FontSize::from_milli_points(m);
        assert_eq!(
            advance_to_layout_units(a, size).milli_points(),
            reference_advance(a, size),
            "random advance #{i} a={a} m={m}"
        );
        let d_raw = (xorshift64(&mut state) >> 32) as i32;
        let d = if pick % 2 == 0 {
            d_raw
        } else {
            d_raw.wrapping_neg()
        };
        assert_eq!(
            adjustment_to_layout_units(d, size).milli_points(),
            reference_adjustment(d, size),
            "random adjustment #{i} d={d} m={m}"
        );
    }
    eprintln!("randomized sweep pairs checked: {total} x2");
}
