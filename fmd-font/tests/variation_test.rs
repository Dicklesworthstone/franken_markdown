//! Integration tests for the committed OFL variable-font fixture (`gk3v.1`).
//!
//! Assertions log check id / subject / outcome on stderr so a failing run
//! reads as a checklist.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::Font;

fn log_check(id: &str, subject: &str, ok: bool) {
    eprintln!(
        "check id={id} subject={subject} outcome={}",
        if ok { "PASS" } else { "FAIL" }
    );
    assert!(ok, "{id}: {subject}");
}

fn load_fixture() -> Font {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fonts/test-variable/FmdTestVF.ttf"
    );
    let bytes = std::fs::read(path).expect("read FmdTestVF.ttf");
    Font::parse(bytes).expect("parse FmdTestVF.ttf")
}

#[test]
fn committed_ofl_variable_font_axes_and_instances() {
    let font = load_fixture();
    let axes = font.axes();
    log_check(
        "gk3v.1.fixture.file.axes",
        "one wght axis",
        axes.len() == 1 && axes[0].tag == *b"wght",
    );
    let bounds = font.instance_bounds(*b"wght").expect("wght bounds");
    log_check(
        "gk3v.1.fixture.file.bounds",
        "100/400/900",
        (bounds.min - 100.0).abs() < 1e-4
            && (bounds.default - 400.0).abs() < 1e-4
            && (bounds.max - 900.0).abs() < 1e-4,
    );
    log_check(
        "gk3v.1.fixture.file.inst",
        "Regular + Bold",
        font.named_instances().len() == 2,
    );
    log_check(
        "gk3v.1.fixture.file.clamp.low",
        "below-min → -1",
        font.normalized_axis(*b"wght", -50.0)
            .is_some_and(|v| (v + 1.0).abs() < 1e-4),
    );
    log_check(
        "gk3v.1.fixture.file.clamp.high",
        "above-max → +1",
        font.normalized_axis(*b"wght", 5000.0)
            .is_some_and(|v| (v - 1.0).abs() < 1e-4),
    );
}

fn table_range(data: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    if data.len() < 12 {
        return None;
    }
    let n = u16::from_be_bytes(data[4..6].try_into().ok()?) as usize;
    for i in 0..n {
        let rec = 12 + i * 16;
        if data.get(rec..rec + 4)? == tag {
            let off = u32::from_be_bytes(data[rec + 8..rec + 12].try_into().ok()?) as usize;
            let len = u32::from_be_bytes(data[rec + 12..rec + 16].try_into().ok()?) as usize;
            return Some((off, len));
        }
    }
    None
}

fn lcg(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    (*state >> 33) as usize
}

/// cmap-shared glyph bbox L∞ in font units. Same-weight instances must be 0;
/// 400 vs 900 on the triangle fixture moves p0 by +50 x.
fn bbox_linf(a: &Font, b: &Font, probe: &str) -> i32 {
    let mut max = 0i32;
    for ch in probe.chars() {
        let gid = a.glyph_index(ch);
        if gid != b.glyph_index(ch) {
            continue;
        }
        let Some(ba) = a.glyph_bbox(gid) else {
            continue;
        };
        let Some(bb) = b.glyph_bbox(gid) else {
            continue;
        };
        for i in 0..4 {
            max = max.max((i32::from(ba[i]) - i32::from(bb[i])).unsigned_abs() as i32);
        }
    }
    max
}

#[test]
fn triangle_fixture_same_weight_bbox_delta_is_zero() {
    let bytes = fmd_font::variable_triangle_fixture();
    let font = Font::parse(bytes).expect("triangle VF");
    let a = font.instance(650.0).expect("instance 650");
    let b = font.instance(650.0).expect("instance 650 again");
    log_check(
        "gk3v.4.tol.bytes",
        "same weight is byte-identical",
        a.as_sfnt() == b.as_sfnt(),
    );
    let delta = bbox_linf(&a, &b, " ");
    log_check(
        "gk3v.4.tol.same",
        "cmap-shared bbox L∞ is 0 at the same weight",
        delta == 0,
    );
}

#[test]
fn triangle_fixture_peak_weight_moves_space_glyph_bbox() {
    let font = Font::parse(fmd_font::variable_triangle_fixture()).expect("triangle VF");
    let light = font.instance(400.0).expect("400");
    let peak = font.instance(900.0).expect("900");
    let delta = bbox_linf(&light, &peak, " ");
    // Private-point gvar: p0 +50 x at peak. Default bbox xMin 0 → 50.
    log_check(
        "gk3v.4.tol.cross",
        "400 vs 900 bbox L∞ is 50 font units",
        delta == 50,
    );
}

#[test]
fn hostile_fvar_lcg_mutation_never_panics() {
    let original = fmd_font::variable_triangle_fixture();
    let (off, len) = table_range(&original, b"fvar").expect("fvar table");
    let mut state = 0xF1A7_00FFu64;
    for round in 0..32 {
        let mut mutated = original.clone();
        for _ in 0..4 {
            let pos = off + (lcg(&mut state) % len);
            mutated[pos] ^= 1u8 << (lcg(&mut state) % 8);
        }
        let outcome = std::panic::catch_unwind(move || {
            if let Ok(f) = Font::parse(mutated) {
                let _ = f.instance(100.0);
                let _ = f.instance(400.0);
                let _ = f.instance(900.0);
            }
        });
        log_check(
            "gk3v.4.fvar.lcg",
            &format!("round {round} no panic"),
            outcome.is_ok(),
        );
    }
}

#[test]
fn dump_triangle_vf_when_requested() {
    let bytes = fmd_font::variable_triangle_fixture();
    log_check(
        "gk3v.4.dump.parse",
        "triangle VF parses",
        Font::parse(bytes.clone()).is_ok(),
    );
    if let Ok(path) = std::env::var("FMD_DUMP_TRIANGLE_VF") {
        std::fs::write(&path, &bytes).expect("dump triangle VF");
        log_check(
            "gk3v.4.dump.write",
            &path,
            std::path::Path::new(&path).is_file(),
        );
    }
}
