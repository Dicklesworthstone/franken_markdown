//! Resumable lexical engine with bounded state (FCB-021.A).
//!
//! A streaming front end over the batch [`crate::highlight`] engine. Text is
//! fed in arbitrary chunks; the engine holds an *unresolved suffix* — the
//! final span of each partial classification, whose bytes may still be
//! extended by the next chunk — and releases every earlier span immediately.
//! At EOF, [`ResumableLexer::finish`] flushes the suffix, so the complete
//! output tiles the source exactly and matches whole-input classification
//! once adjacent same-kind spans are coalesced.
//!
//! Bounded state: the held suffix is capped at
//! [`ResumableLexer::max_pending_bytes`]; a feed that would exceed the cap is
//! refused with [`ResumeError::SuffixTooLong`] instead of growing without
//! bound (for example, an unterminated block comment or string).
//!
//! Chunk versus EOF: `feed` may end mid-token, mid-UTF-8 character, or
//! mid-delimiter — the unresolved suffix absorbs all three. Only malformed
//! UTF-8 (a decoding error, not a truncation) is a typed refusal.

use crate::highlight::{highlight, is_supported, Span};

/// Errors from the resumable lexical seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeError {
    /// The language has no supported lexical route; capability is reported
    /// truthfully instead of silently falling back to a generic pass.
    UnsupportedLanguage,
    /// The held suffix exceeded the configured bound.
    SuffixTooLong {
        /// Held bytes at refusal.
        held: usize,
        /// The configured cap.
        cap: usize,
    },
    /// The chunk contained malformed UTF-8 at this byte offset.
    InvalidUtf8 {
        /// Offset of the first invalid byte within the fed chunk.
        at: usize,
    },
    /// The lexer was already finished; no further feeds are accepted.
    AlreadyFinished,
}

impl ResumeError {
    /// Stable machine-readable code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedLanguage => "UNSUPPORTED_LANGUAGE",
            Self::SuffixTooLong { .. } => "SUFFIX_TOO_LONG",
            Self::InvalidUtf8 { .. } => "INVALID_UTF8",
            Self::AlreadyFinished => "ALREADY_FINISHED",
        }
    }
}

/// What one [`ResumableLexer::feed`] produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedReport {
    /// Spans newly released by this feed.
    pub spans_emitted: usize,
    /// Held suffix bytes (the unresolved tail still pending classification).
    pub pending_bytes: usize,
    /// Whether the held suffix is nonempty (more input may extend it).
    pub unresolved: bool,
}

/// A resumable, bounded lexical front end for one language.
///
/// The engine wraps the batch classifier without duplicating it: every feed
/// re-classifies only the held suffix plus the new chunk, releases every span
/// that ends before the unresolved suffix, and keeps its state bounded by the
/// configured pending cap.
#[derive(Debug)]
pub struct ResumableLexer {
    lang: String,
    pending: Vec<u8>,
    base: usize,
    spans: Vec<Span>,
    finished: bool,
    max_pending_bytes: usize,
}

impl ResumableLexer {
    /// Default cap for the held suffix: one mebibyte.
    pub const DEFAULT_MAX_PENDING_BYTES: usize = 1 << 20;

    /// Create a lexer for a supported language.
    ///
    /// Unsupported languages are refused up front so callers can publish a
    /// truthful capability row instead of discovering it mid-stream.
    pub fn new(lang: &str) -> Result<Self, ResumeError> {
        Self::with_limits(lang, Self::DEFAULT_MAX_PENDING_BYTES)
    }

    /// Create a lexer with an explicit held-suffix cap.
    pub fn with_limits(lang: &str, max_pending_bytes: usize) -> Result<Self, ResumeError> {
        if !is_supported(lang) {
            return Err(ResumeError::UnsupportedLanguage);
        }
        Ok(Self {
            lang: lang.to_owned(),
            pending: Vec::new(),
            base: 0,
            spans: Vec::new(),
            finished: false,
            max_pending_bytes: max_pending_bytes.max(1),
        })
    }

    /// The language this lexer was created for.
    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// Held suffix bytes (the unresolved tail).
    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }

    /// Whether [`ResumableLexer::finish`] has been called.
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Spans released so far, in order, tiling `[0, emitted_end)` of the
    /// source where `emitted_end == base + pending.len()` is the unflushed
    /// tail.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Feed one chunk of source bytes.
    ///
    /// The chunk may end anywhere: mid-token, mid-UTF-8 character, or mid-
    /// delimiter. Truncated tails are held; malformed bytes are a typed
    /// refusal naming the offset inside this chunk.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<FeedReport, ResumeError> {
        if self.finished {
            return Err(ResumeError::AlreadyFinished);
        }
        self.validate_chunk_utf8(chunk)?;
        self.pending.extend_from_slice(chunk);
        if self.pending.len() > self.max_pending_bytes {
            return Err(ResumeError::SuffixTooLong {
                held: self.pending.len(),
                cap: self.max_pending_bytes,
            });
        }
        let before = self.spans.len();
        self.lex_pending_release();
        Ok(FeedReport {
            spans_emitted: self.spans.len() - before,
            pending_bytes: self.pending.len(),
            unresolved: !self.pending.is_empty(),
        })
    }

    /// Flush the held suffix at EOF and seal the lexer.
    ///
    /// After a successful finish, [`ResumableLexer::spans`] tiles the
    /// complete fed source exactly. Further feeds are refused with
    /// [`ResumeError::AlreadyFinished`].
    pub fn finish(&mut self) -> Result<(), ResumeError> {
        if self.finished {
            return Err(ResumeError::AlreadyFinished);
        }
        let text = self.pending_text()?;
        let tail = highlight(&self.lang, &text);
        for span in tail {
            self.spans.push(Span {
                kind: span.kind,
                start: self.base + span.start,
                end: self.base + span.end,
            });
        }
        self.finished = true;
        self.pending.clear();
        Ok(())
    }

    /// Validate that appending `chunk` keeps the buffer valid UTF-8 or ends
    /// with a truncated (not malformed) character.
    fn validate_chunk_utf8(&self, chunk: &[u8]) -> Result<(), ResumeError> {
        // Validate only the potential partial-character head plus the chunk:
        // scanning the whole buffer per feed would be quadratic. The head
        // must start on a character boundary of the held buffer, or a
        // multi-byte character split across feeds would look malformed.
        let head = self.pending.len().min(3);
        let mut start = self.pending.len() - head;
        while start < self.pending.len() && self.pending[start] & 0xC0 == 0x80 {
            start += 1;
        }
        let mut probe = Vec::with_capacity(self.pending.len() - start + chunk.len());
        probe.extend_from_slice(&self.pending[start..]);
        probe.extend_from_slice(chunk);
        match std::str::from_utf8(&probe) {
            Ok(_) => Ok(()),
            Err(error) => {
                if error.error_len().is_some() {
                    Err(ResumeError::InvalidUtf8 {
                        at: start + error.valid_up_to(),
                    })
                } else {
                    // Truncated tail: held until more bytes arrive.
                    Ok(())
                }
            }
        }
    }

    fn pending_text(&self) -> Result<String, ResumeError> {
        String::from_utf8(self.pending.clone())
            .map_err(|_| ResumeError::InvalidUtf8 { at: 0 })
    }

    /// Lex the held suffix and release every span except the final one, whose
    /// bytes may still be extended by future input.
    fn lex_pending_release(&mut self) {
        let Ok(text) = self.pending_text() else {
            return; // truncated UTF-8 head: nothing to classify yet
        };
        let spans = highlight(&self.lang, &text);
        let Some(last) = spans.last() else {
            return; // empty input so far
        };
        let hold_from = last.start;
        let mut released = Vec::with_capacity(spans.len().saturating_sub(1));
        for span in &spans[..spans.len() - 1] {
            released.push(Span {
                kind: span.kind,
                start: self.base + span.start,
                end: self.base + span.end,
            });
        }
        self.spans.extend(released);
        self.base += hold_from;
        self.pending.drain(..hold_from);
    }
}
