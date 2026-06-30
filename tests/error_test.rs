//! Unit coverage for the hand-rolled error type (`src/error.rs`), bead grn.2.8.
//!
//! Real inputs, no mocks: the `Io` variant is built from an authentic
//! `std::io::Error` produced by an actual failing filesystem operation, and every
//! variant is exercised through the real `Display`, `Error::source`, `code`, and
//! `From` impls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::RenderError;
use std::error::Error as _;

/// Produce a genuine `std::io::Error` by reading a path that cannot exist, so the
/// `Io` variant is tested against a real OS error rather than a synthesized one.
fn real_io_error() -> std::io::Error {
    let missing = std::env::temp_dir().join("fmd-error-test-this-path-does-not-exist-9f3a2b1c");
    std::fs::read(&missing).expect_err("reading a nonexistent path must fail")
}

#[test]
fn display_renders_each_variant_with_its_prefix() {
    let io = RenderError::from(real_io_error());
    assert!(io.to_string().starts_with("io error: "), "got {io}",);

    let nyi = RenderError::NotYetImplemented("pdf-foo");
    assert_eq!(
        nyi.to_string(),
        "not yet implemented: pdf-foo (tracked in beads)"
    );

    let bad = RenderError::InvalidInput("missing --out".to_string());
    assert_eq!(bad.to_string(), "invalid input: missing --out");
}

#[test]
fn code_is_a_stable_machine_selector_per_variant() {
    assert_eq!(RenderError::from(real_io_error()).code(), "io_error");
    assert_eq!(
        RenderError::NotYetImplemented("x").code(),
        "not_yet_implemented"
    );
    assert_eq!(
        RenderError::InvalidInput("y".to_string()).code(),
        "invalid_input"
    );
}

#[test]
fn source_chains_only_for_io_and_preserves_the_inner_error() {
    let io = RenderError::from(real_io_error());
    let src = io
        .source()
        .expect("Io must expose its inner error as the source");
    // The chained source is the real underlying std::io::Error.
    assert!(src.downcast_ref::<std::io::Error>().is_some());

    assert!(RenderError::NotYetImplemented("x").source().is_none());
    assert!(
        RenderError::InvalidInput("y".to_string())
            .source()
            .is_none()
    );
}

#[test]
fn from_io_error_maps_to_the_io_variant() {
    let err = real_io_error();
    let kind = err.kind();
    let render: RenderError = err.into();
    assert_eq!(render.code(), "io_error");
    // The `?`-friendly conversion preserves the original kind via the chained source.
    let inner = render
        .source()
        .and_then(|s| s.downcast_ref::<std::io::Error>())
        .expect("io source");
    assert_eq!(inner.kind(), kind);
}

#[test]
fn debug_is_available_for_diagnostics() {
    // The derived Debug must render each variant (used in `{:?}` diagnostics/asserts).
    assert!(format!("{:?}", RenderError::InvalidInput("z".to_string())).contains("InvalidInput"));
    assert!(format!("{:?}", RenderError::NotYetImplemented("w")).contains("NotYetImplemented"));
    assert!(format!("{:?}", RenderError::from(real_io_error())).contains("Io"));
}

#[test]
fn usable_as_a_boxed_std_error() {
    // Confirms the trait-object path (returning `Box<dyn Error>` from a fn).
    fn fails() -> Result<(), Box<dyn std::error::Error>> {
        Err(RenderError::InvalidInput("boom".to_string()).into())
    }
    let e = fails().unwrap_err();
    assert_eq!(e.to_string(), "invalid input: boom");
}
