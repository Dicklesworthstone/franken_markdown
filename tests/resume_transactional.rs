//! End-to-end regressions through the public streaming and checkpoint APIs.
//! These checks compare actual token streams, not snapshots of helper code.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, highlight};
use franken_markdown::resume::{
    CHECKPOINT_VERSION, CheckpointError, CommentState, LexerCheckpoint, MAX_SUFFIX_BYTES,
    ResumableLexer, ResumeError, StringState, coalesce_spans, highlight_chunked,
};

fn assert_tiles(source: &str, spans: &[Span]) {
    let mut end = 0;
    for span in spans {
        assert_eq!(span.start, end, "gap or overlap at {end}");
        assert!(span.start < span.end && span.end <= source.len());
        assert!(source.is_char_boundary(span.start) && source.is_char_boundary(span.end));
        end = span.end;
    }
    assert_eq!(end, source.len());
}

fn assert_same_source(lang: &str, source: &str, spans: &[Span]) {
    assert_tiles(source, spans);
    assert_eq!(coalesce_spans(spans), coalesce_spans(&highlight(lang, source)));
}

fn empty_checkpoint(offset: u64) -> LexerCheckpoint {
    LexerCheckpoint {
        version: CHECKPOINT_VERSION,
        lang: "rust".into(),
        source_revision: 7,
        byte_offset: offset,
        comment_state: CommentState::None,
        string_state: StringState::None,
        interpolation_depth: 0,
        is_eof: false,
        unresolved_suffix: Vec::new(),
    }
}

#[test]
fn repeated_budget_refusals_do_not_grow_or_poison_the_buffer() {
    let mut lexer = ResumableLexer::with_limits("rust", 64).unwrap();
    lexer.feed(b"/* held").unwrap();
    let checkpoint = lexer.checkpoint(9);
    let output = lexer.spans().to_vec();
    let received = lexer.received_bytes();
    let emitted = lexer.emitted_bytes();
    for _ in 0..20 {
        let error = lexer.feed(&[b'x'; 100]).unwrap_err();
        assert!(matches!(error, ResumeError::SuffixTooLong { held, cap: 64 } if held > 64));
        assert_eq!(lexer.checkpoint(9), checkpoint);
        assert_eq!(lexer.spans(), output);
        assert_eq!(lexer.received_bytes(), received);
        assert_eq!(lexer.emitted_bytes(), emitted);
        assert!(lexer.pending_bytes() <= lexer.max_pending_bytes());
    }
    lexer.feed(b" */ let x = 1;").unwrap();
    lexer.finish().unwrap();
    assert_same_source("rust", "/* held */ let x = 1;", lexer.spans());
}

#[test]
fn invalid_utf8_offsets_are_chunk_relative_and_retries_are_lossless() {
    let mut lexer = ResumableLexer::new("rust").unwrap();
    lexer.feed(b"let x = 1; ").unwrap();
    let checkpoint = lexer.checkpoint(3);
    let output = lexer.spans().to_vec();
    assert_eq!(lexer.feed(b"ok\xff"), Err(ResumeError::InvalidUtf8 { at: 2 }));
    assert_eq!(lexer.checkpoint(3), checkpoint);
    assert_eq!(lexer.spans(), output);
    lexer.feed(b"let y = 2;").unwrap();
    lexer.finish().unwrap();
    assert_same_source("rust", "let x = 1; let y = 2;", lexer.spans());
}

#[test]
fn invalid_continuation_reports_the_new_byte_without_losing_the_pending_scalar() {
    let source = "let value = \"😀\";";
    let scalar = source.find('😀').unwrap();
    let split = scalar + 2;
    let mut lexer = ResumableLexer::new("rust").unwrap();
    lexer.feed(&source.as_bytes()[..split]).unwrap();
    let checkpoint = lexer.checkpoint(1);
    assert_eq!(lexer.feed(&[0x98, b'x']), Err(ResumeError::InvalidUtf8 { at: 1 }));
    assert_eq!(lexer.checkpoint(1), checkpoint);
    lexer.feed(&source.as_bytes()[split..]).unwrap();
    lexer.finish().unwrap();
    assert_same_source("rust", source, lexer.spans());
}

#[test]
fn premature_finish_can_be_retried_after_completing_utf8() {
    let source = "let value = \"😀\";";
    let split = source.find('😀').unwrap() + 3;
    let mut lexer = ResumableLexer::new("rust").unwrap();
    lexer.feed(&source.as_bytes()[..split]).unwrap();
    let checkpoint = lexer.checkpoint(1);
    assert!(matches!(lexer.finish(), Err(ResumeError::InvalidUtf8 { .. })));
    assert!(!lexer.is_finished());
    assert_eq!(lexer.checkpoint(1), checkpoint);
    lexer.feed(&source.as_bytes()[split..]).unwrap();
    lexer.finish().unwrap();
    assert_same_source("rust", source, lexer.spans());
}

#[test]
fn finished_checkpoint_points_to_the_end_of_all_accepted_source() {
    let source = "let value = 1;";
    let mut lexer = ResumableLexer::new("rust").unwrap();
    lexer.feed(source.as_bytes()).unwrap();
    lexer.finish().unwrap();
    assert_eq!(lexer.emitted_bytes(), source.len());
    assert_eq!(lexer.received_bytes(), source.len());
    let checkpoint = lexer.try_checkpoint(42).unwrap();
    assert!(checkpoint.is_eof && checkpoint.unresolved_suffix.is_empty());
    assert_eq!(checkpoint.byte_offset, source.len() as u64);
    let bytes = checkpoint.try_to_bytes().unwrap();
    let decoded = LexerCheckpoint::from_bytes(&bytes).unwrap();
    let mut restored = ResumableLexer::from_checkpoint(&decoded, 42, source.len() as u64).unwrap();
    assert_eq!(restored.received_bytes(), source.len());
    assert_eq!(restored.emitted_bytes(), source.len());
    assert_eq!(restored.feed(b"late"), Err(ResumeError::AlreadyFinished));
    assert_eq!(restored.finish(), Err(ResumeError::AlreadyFinished));
}

#[test]
fn drained_output_keeps_absolute_offsets_and_never_reappears() {
    let source = "let a = 1; let b = 2; let c = 3;";
    let mut lexer = ResumableLexer::new("rust").unwrap();
    let mut output = Vec::new();
    for chunk in source.as_bytes().chunks(5) {
        let report = lexer.feed(chunk).unwrap();
        let before = lexer.checkpoint(2);
        let spans = lexer.take_spans();
        assert_eq!(spans.len(), report.spans_emitted);
        output.extend(spans);
        assert!(lexer.spans().is_empty());
        assert!(lexer.take_spans().is_empty());
        assert_eq!(lexer.checkpoint(2), before);
    }
    lexer.finish().unwrap();
    output.extend(lexer.take_spans());
    assert_same_source("rust", source, &output);
}

#[test]
fn checkpoints_can_roundtrip_and_resume_at_every_byte_including_inside_unicode() {
    let source = "let name = \"café 😀\";\n";
    let mut lexer = ResumableLexer::new("rust").unwrap();
    let mut output = Vec::new();
    for (index, byte) in source.bytes().enumerate() {
        lexer.feed(&[byte]).unwrap();
        output.extend(lexer.take_spans());
        let checkpoint = lexer.try_checkpoint(88).unwrap();
        assert_eq!(checkpoint.byte_offset as usize + checkpoint.unresolved_suffix.len(), index + 1);
        let bytes = checkpoint.try_to_bytes().unwrap();
        let decoded = LexerCheckpoint::from_bytes(&bytes).unwrap();
        lexer = ResumableLexer::from_checkpoint(&decoded, 88, decoded.byte_offset).unwrap();
        assert_eq!(lexer.received_bytes(), index + 1);
    }
    lexer.finish().unwrap();
    output.extend(lexer.take_spans());
    assert_same_source("rust", source, &output);
    assert_eq!(lexer.try_checkpoint(88).unwrap().byte_offset, source.len() as u64);
}

#[test]
fn malformed_restoration_is_rejected_before_source_can_be_discarded() {
    let mut checkpoint = empty_checkpoint(0);
    checkpoint.unresolved_suffix = vec![0xff];
    assert!(matches!(
        ResumableLexer::from_checkpoint(&checkpoint, 7, 0),
        Err(CheckpointError::InvalidUtf8)
    ));
    checkpoint.unresolved_suffix = b"not emitted".to_vec();
    checkpoint.is_eof = true;
    assert!(matches!(
        ResumableLexer::from_checkpoint(&checkpoint, 7, 0),
        Err(CheckpointError::InvalidEofState)
    ));
}

#[test]
fn restoration_honors_the_receiving_hosts_budget() {
    let mut lexer = ResumableLexer::new("rust").unwrap();
    lexer.feed(b"/* unresolved").unwrap();
    let checkpoint = lexer.try_checkpoint(7).unwrap();
    assert!(matches!(
        ResumableLexer::from_checkpoint_with_limits(&checkpoint, 7, checkpoint.byte_offset, 4),
        Err(CheckpointError::PayloadTooLarge { cap: 4, .. })
    ));
    let restored = ResumableLexer::from_checkpoint_with_limits(
        &checkpoint, 7, checkpoint.byte_offset, 64,
    ).unwrap();
    assert_eq!(restored.max_pending_bytes(), 64);
    assert_eq!(restored.received_bytes(), lexer.received_bytes());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn source_offset_overflow_is_a_transactional_refusal() {
    let checkpoint = empty_checkpoint(u64::MAX);
    let mut lexer = ResumableLexer::from_checkpoint(&checkpoint, 7, u64::MAX).unwrap();
    assert_eq!(lexer.feed(b"x"), Err(ResumeError::OffsetOverflow));
    assert_eq!(lexer.checkpoint(7), checkpoint);
    assert!(lexer.spans().is_empty());
}

#[cfg(target_pointer_width = "32")]
#[test]
fn wide_checkpoint_offsets_cannot_wrap_on_wasm32() {
    let offset = u64::from(u32::MAX) + 1;
    let checkpoint = empty_checkpoint(offset);
    assert!(matches!(
        ResumableLexer::from_checkpoint(&checkpoint, 7, offset),
        Err(CheckpointError::OffsetOverflow { .. })
    ));
}

#[test]
fn checked_snapshot_refuses_an_unrepresentable_replay_suffix() {
    let mut lexer = ResumableLexer::with_limits("rust", 2048).unwrap();
    let source = format!("/*{}", "x".repeat(MAX_SUFFIX_BYTES));
    lexer.feed(source.as_bytes()).unwrap();
    assert!(matches!(lexer.try_checkpoint(1), Err(CheckpointError::PayloadTooLarge {
        cap: MAX_SUFFIX_BYTES, ..
    })));
    assert_eq!(lexer.received_bytes(), source.len());
    lexer.feed(b"*/").unwrap();
    lexer.finish().unwrap();
    assert_same_source("rust", &format!("{source}*/"), lexer.spans());
}

#[test]
fn language_metadata_selects_the_same_streaming_policy_as_the_plain_alias() {
    for (canonical, metadata, source) in [
        ("javascript", " Language-JavaScript,linenums ", "const x = value / 2;"),
        ("jsx", " LANGUAGE-JSX,linenums ", "const x = <A>{42}</A>;"),
        ("html", " language-HTML linenums ", "<script>const x = 1;</script>"),
    ] {
        for step in 1..=source.len() {
            let expected = highlight_chunked(canonical, source, step).unwrap();
            let actual = highlight_chunked(metadata, source, step).unwrap();
            assert_eq!(coalesce_spans(&actual), coalesce_spans(&expected), "{metadata}, chunk {step}");
            assert_tiles(source, &actual);
        }
    }
}

#[test]
fn long_stream_can_release_output_without_accumulating_span_history() {
    let source = "let x = 1;\n".repeat(512);
    let mut lexer = ResumableLexer::with_limits("rust", 128).unwrap();
    let mut output = Vec::new();
    for chunk in source.as_bytes().chunks(17) {
        lexer.feed(chunk).unwrap();
        assert!(lexer.pending_bytes() <= 128);
        output.extend(lexer.take_spans());
        assert!(lexer.spans().is_empty());
    }
    lexer.finish().unwrap();
    output.extend(lexer.take_spans());
    assert_same_source("rust", &source, &output);
}
