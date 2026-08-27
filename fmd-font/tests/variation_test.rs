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
