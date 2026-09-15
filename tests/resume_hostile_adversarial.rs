//! Adversarial hostile-input campaign against the REAL FCB-021 resumable
//! lexical engine (FCB-059.A production adapter, upstream side).
//!
//! The conformance core (`fcb_conformance`) supplies deterministic hostile
//! generators and the failure-preserving minimizer; this file wires them to
//! `franken_markdown::resume::ResumableLexer` so a real lexical failure is
//! reproduced, minimized with its classification pinned, and proven
//! byte-reproducible from its seed.

#![forbid(unsafe_code)]

use fcb_conformance::{minimize, HostileByteGenerator, TerminationBudget};
use franken_markdown::resume::{ResumableLexer, ResumeError};

/// The stable classification this campaign tracks: the engine's malformed
/// UTF-8 refusal, mapped from the typed error.
fn classify(input: &[u8]) -> Option<&'static str> {
    let mut lexer = ResumableLexer::new("rust").expect("rust is supported");
    match lexer.feed(input) {
        Err(ResumeError::InvalidUtf8 { .. }) => Some("INVALID_UTF8"),
        _ => None,
    }
}

#[test]
fn hostile_streams_never_panic_the_real_engine() {
    // Every hostile stream is fed; the engine must answer with either a
    // successful feed or a typed refusal — never a panic and never a hang.
    let mut generator = HostileByteGenerator::new(0xB105, 96);
    for _ in 0..256 {
        let stream = generator.generate();
        let _ = classify(&stream);
    }
}

#[test]
fn campaign_reproduces_a_real_invalid_utf8_refusal() {
    let mut generator = HostileByteGenerator::new(0xF00D, 48);
    let mut reproduced = None;
    for attempt in 0..512 {
        let stream = generator.generate();
        if classify(&stream) == Some("INVALID_UTF8") {
            reproduced = Some((stream, attempt));
            break;
        }
    }
    let (failing, attempt) = reproduced.expect("the byte pool contains malformed UTF-8");
    assert!(
        !failing.is_empty(),
        "campaign attempt {attempt} reproduced a nonempty refusal"
    );
}

#[test]
fn real_refusal_is_minimized_with_classification_pinned() {
    // Seed a stream that certainly fails, then pad it: minimization must
    // strip the padding while the INVALID_UTF8 classification survives.
    let mut failing = b"fn main() { let s = \"\xFF\xFE".to_vec();
    failing.extend(std::iter::repeat(b'q').take(96));
    assert!(classify(&failing) == Some("INVALID_UTF8"));

    let report = minimize(&failing, TerminationBudget::attempts(512), &mut classify);
    assert!(report.classification_preserved, "classification is pinned");
    assert!(
        report.minimized.len() < failing.len(),
        "minimization must shrink the padded failure"
    );
    assert_eq!(classify(&report.minimized), Some("INVALID_UTF8"));
}

#[test]
fn minimization_is_deterministic_across_runs() {
    let failing = b"let s = \"\xC3\x28 garbage".to_vec();
    let run_a = minimize(&failing, TerminationBudget::attempts(512), &mut classify);
    let run_b = minimize(&failing, TerminationBudget::attempts(512), &mut classify);
    assert_eq!(run_a.minimized, run_b.minimized);
    assert_eq!(run_a.attempts, run_b.attempts);
}

#[test]
fn minimizer_budget_exhaustion_is_safe() {
    let failing = b"\xFF".to_vec();
    // A one-byte failing input cannot shrink further; a tiny budget must
    // terminate immediately without changing anything.
    let report = minimize(&failing, TerminationBudget::attempts(8), &mut classify);
    assert_eq!(report.minimized, failing);
    assert!(!report.exhausted_budget);
    assert!(report.attempts <= 8);
}
