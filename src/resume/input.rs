//! Transactional byte ingestion, separate from token hold policy.
//!
//! A large transport chunk need not fit in the replay buffer. Process it in
//! bounded windows on staged state, publishing neither offsets nor spans until
//! the entire feed succeeds. An unresolved construct that fills the window
//! still causes a bounded refusal. Existing output history is never cloned.

use super::{FeedReport, ResumableLexer, ResumeError};

pub(super) fn feed(lexer: &mut ResumableLexer, chunk: &[u8]) -> Result<FeedReport, ResumeError> {
    if lexer.finished {
        return Err(ResumeError::AlreadyFinished);
    }
    let projected = lexer.pending.len().checked_add(chunk.len())
        .ok_or(ResumeError::OffsetOverflow)?;
    lexer.base.checked_add(projected).ok_or(ResumeError::OffsetOverflow)?;
    validate_append_utf8(&lexer.pending, chunk)?;
    let before = lexer.spans.len();
    if projected <= lexer.max_pending_bytes {
        // Ordinary feeds need no staging clone. All typed refusals have already
        // happened, and the existing byte capacity can be reused directly.
        ingest_validated(lexer, chunk);
    } else {
        let mut staged = ResumableLexer {
            lang: lexer.lang.clone(),
            pending: lexer.pending.clone(),
            base: lexer.base,
            spans: Vec::new(),
            finished: false,
            max_pending_bytes: lexer.max_pending_bytes,
        };
        let mut consumed = 0usize;
        while consumed < chunk.len() {
            let available = staged.max_pending_bytes - staged.pending.len();
            if available == 0 {
                // The next byte would exceed the working bound. No bytes or
                // provisional output from this feed escape the staged state.
                return Err(ResumeError::SuffixTooLong {
                    held: staged.max_pending_bytes.saturating_add(1),
                    cap: staged.max_pending_bytes,
                });
            }
            let length = available.min(chunk.len() - consumed);
            ingest_validated(&mut staged, &chunk[consumed..consumed + length]);
            consumed += length;
        }
        lexer.base = staged.base;
        lexer.pending = staged.pending;
        lexer.spans.append(&mut staged.spans);
    }
    Ok(FeedReport {
        spans_emitted: lexer.spans.len() - before,
        pending_bytes: lexer.pending.len(),
        unresolved: !lexer.pending.is_empty(),
    })
}

/// Call only after validating the complete input and checking source extent.
/// Splitting a valid feed may leave a partial scalar, but cannot make its bytes
/// malformed. The lexical layer releases only complete UTF-8/token boundaries.
fn ingest_validated(lexer: &mut ResumableLexer, chunk: &[u8]) {
    debug_assert!(chunk.len() <= lexer.max_pending_bytes - lexer.pending.len());
    lexer.pending.extend_from_slice(chunk);
    lexer.lex_pending_release();
}

/// Inspect at most one boundary scalar on the stack, then validate the new
/// chunk directly. Error offsets are relative to that chunk, never to pending.
fn validate_append_utf8(pending: &[u8], chunk: &[u8]) -> Result<(), ResumeError> {
    let mut start = pending.len().saturating_sub(4);
    while start < pending.len() && pending[start] & 0xc0 == 0x80 {
        start += 1;
    }
    let partial = match std::str::from_utf8(&pending[start..]) {
        Ok(_) => &[][..],
        Err(error) if error.error_len().is_none() => &pending[start + error.valid_up_to()..],
        Err(_) => return Err(ResumeError::InvalidUtf8 { at: 0 }),
    };
    let mut consumed = 0usize;
    if !partial.is_empty() {
        let mut scalar = [0u8; 4];
        scalar[..partial.len()].copy_from_slice(partial);
        let mut length = partial.len();
        loop {
            match std::str::from_utf8(&scalar[..length]) {
                Ok(_) => break,
                Err(error) if error.error_len().is_some() => {
                    return Err(ResumeError::InvalidUtf8 { at: consumed.saturating_sub(1) });
                }
                Err(_) => {
                    let Some(&byte) = chunk.get(consumed) else { return Ok(()); };
                    let Some(slot) = scalar.get_mut(length) else {
                        return Err(ResumeError::InvalidUtf8 { at: consumed });
                    };
                    *slot = byte;
                    length += 1;
                    consumed += 1;
                }
            }
        }
    }
    match std::str::from_utf8(&chunk[consumed..]) {
        Err(error) if error.error_len().is_some() => {
            Err(ResumeError::InvalidUtf8 { at: consumed + error.valid_up_to() })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::highlight::highlight;
    use crate::resume::coalesce_spans;

    #[test]
    fn one_large_feed_can_exceed_the_replay_window_many_times() {
        let source = "let x = 1;\n".repeat(512);
        let mut lexer = ResumableLexer::with_limits("rust", 64).unwrap();
        let report = feed(&mut lexer, source.as_bytes()).unwrap();
        assert_eq!(report.spans_emitted, lexer.spans().len());
        assert!(report.pending_bytes <= 64);
        assert_eq!(lexer.received_bytes(), source.len());
        lexer.finish().unwrap();
        assert_eq!(coalesce_spans(lexer.spans()), coalesce_spans(&highlight("rust", &source)));
    }

    #[test]
    fn a_late_oversized_construct_rolls_back_provisional_output_and_offsets() {
        let mut lexer = ResumableLexer::with_limits("rust", 64).unwrap();
        feed(&mut lexer, b"let existing = 0; ").unwrap();
        let before = lexer.checkpoint(5);
        let output = lexer.spans().to_vec();
        let rejected = format!("{}/*{}", "let x = 1; ".repeat(40), "x".repeat(100));
        assert!(matches!(feed(&mut lexer, rejected.as_bytes()), Err(ResumeError::SuffixTooLong { .. })));
        assert_eq!(lexer.checkpoint(5), before);
        assert_eq!(lexer.spans(), output);
        feed(&mut lexer, b"let replacement = 2;").unwrap();
        lexer.finish().unwrap();
        let accepted = "let existing = 0; let replacement = 2;";
        assert_eq!(coalesce_spans(lexer.spans()), coalesce_spans(&highlight("rust", accepted)));
    }

    #[test]
    fn large_feeds_preserve_unicode_across_internal_byte_windows() {
        let source = "let x = \"café 😀\";\n".repeat(40);
        for cap in 24..=40 {
            let mut lexer = ResumableLexer::with_limits("rust", cap).unwrap();
            feed(&mut lexer, source.as_bytes()).unwrap();
            lexer.finish().unwrap();
            assert_eq!(coalesce_spans(lexer.spans()), coalesce_spans(&highlight("rust", &source)), "cap {cap}");
        }
    }

    #[test]
    fn invalid_bytes_at_the_end_of_a_large_feed_do_not_commit_its_valid_prefix() {
        let mut lexer = ResumableLexer::with_limits("rust", 32).unwrap();
        feed(&mut lexer, b"let ").unwrap();
        let before = lexer.checkpoint(6);
        let output = lexer.spans().to_vec();
        let mut rejected = b"x = 1; let ".repeat(100);
        let invalid_at = rejected.len();
        rejected.push(0xff);
        assert_eq!(feed(&mut lexer, &rejected), Err(ResumeError::InvalidUtf8 { at: invalid_at }));
        assert_eq!(lexer.checkpoint(6), before);
        assert_eq!(lexer.spans(), output);
    }

    #[test]
    fn huge_unterminated_tokens_keep_the_live_buffer_within_its_limit() {
        let mut lexer = ResumableLexer::with_limits("rust", 32).unwrap();
        for _ in 0..10 {
            let rejected = format!("/*{}", "x".repeat(4096));
            assert_eq!(feed(&mut lexer, rejected.as_bytes()), Err(ResumeError::SuffixTooLong { held: 33, cap: 32 }));
            assert_eq!(lexer.pending_bytes(), 0);
            assert_eq!(lexer.received_bytes(), 0);
            assert!(lexer.spans().is_empty());
        }
    }

    #[test]
    fn utf8_boundary_validator_matches_whole_input_validity_at_every_split() {
        for source in ["", "ascii", "café", "日本語", "😀😀", "\u{10ffff}"] {
            for split in 0..=source.len() {
                assert_eq!(validate_append_utf8(&source.as_bytes()[..split], &source.as_bytes()[split..]), Ok(()));
            }
        }
        assert_eq!(validate_append_utf8(b"held", b"ab\xff"), Err(ResumeError::InvalidUtf8 { at: 2 }));
        assert_eq!(validate_append_utf8(&[0xf0, 0x9f], &[0x98, b'x']), Err(ResumeError::InvalidUtf8 { at: 1 }));
        assert_eq!(validate_append_utf8(&[0xc3], &[0xa9, b'!', 0xff]), Err(ResumeError::InvalidUtf8 { at: 2 }));
        assert_eq!(validate_append_utf8(&[0xf0], &[]), Ok(()));
    }
}
