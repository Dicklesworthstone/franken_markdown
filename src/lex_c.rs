//! C-specific resumable lexer: bounded incremental classification with
//! sound safe-cut release (FCB-022, C route / fcb-9vx.10).
//!
//! The batch engine (`crate::highlight`, C rules) is the classifier; this
//! module appends chunks to a held suffix, re-classifies per feed, and only
//! releases spans that end at a **safe cut**. A safe cut is a span whose
//! final byte is a terminal delimiter (`; { } , " '`): such a byte settles
//! every lookahead the classifier performs (function-call detection crosses
//! whitespace and newlines; the `.` of a float literal waits for one more
//! byte; a one-byte `Str` span is an opening quote, not a terminator), so
//! the classification of everything before it can never change when more
//! input arrives.
//!
//! Between safe cuts everything is held as an unresolved suffix, bounded by
//! [`ResumableCLexer::DEFAULT_MAX_PENDING_BYTES`]; a feed that exceeds the
//! cap is refused with [`LexCError::SuffixTooLong`] rather than growing
//! unboundedly (the oracle's bounded-refusal requirement). `finish` flushes
//! the held suffix at EOF, so the sealed output tiles the source exactly and
//! coalesces to the whole-input classification.

use crate::highlight::{Span, Tok, highlight};

/// Errors from the C resumable seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexCError {
    /// The held suffix exceeded the configured cap (for example an
    /// unterminated block comment longer than the cap).
    SuffixTooLong {
        /// Held bytes at refusal.
        held: usize,
        /// The configured cap.
        cap: usize,
    },
    /// The chunk contained malformed UTF-8 at this offset within the chunk.
    InvalidUtf8 {
        /// Offset of the first invalid byte within the fed chunk.
        at: usize,
    },
    /// The lexer was already finished.
    AlreadyFinished,
}

impl LexCError {
    /// Stable machine-readable code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::SuffixTooLong { .. } => "SUFFIX_TOO_LONG",
            Self::InvalidUtf8 { .. } => "INVALID_UTF8",
            Self::AlreadyFinished => "ALREADY_FINISHED",
        }
    }
}

/// Whether a byte terminates a lookahead-safe cut position.
///
/// `; { }` settle word/function/dot lookaheads; `,` settles argument lists;
/// a closing quote settles the string or character literal. A byte inside a
/// string or comment is never a span end, so it is never tested here.
fn is_safe_cut_byte(byte: u8) -> bool {
    matches!(byte, b';' | b'{' | b'}' | b',' | b'"' | b'\'')
}

/// A span end is a safe cut when its final byte is a terminal delimiter AND
/// the span is complete: an EOF-truncated span (unterminated comment,
/// string, or word at the end of the held suffix) can always be extended by
/// more input, so it is never a safe cut; a one-byte `Str` span is an
/// opening quote; and a quote-terminated span must open with the same quote
/// it closes with.
fn is_safe_cut_span(pending: &str, span: &Span) -> bool {
    // An EOF-truncated span is never a safe cut.
    if span.end == pending.len() {
        return false;
    }
    let bytes = pending.as_bytes();
    let Some(last) = bytes.get(span.end.wrapping_sub(1)) else {
        return false;
    };
    if !is_safe_cut_byte(*last) {
        return false;
    }
    if span.kind == Tok::Str {
        if span.end - span.start < 2 {
            return false;
        }
        if (*last == b'"' || *last == b'\'') && bytes.get(span.start) != Some(last) {
            return false;
        }
    }
    true
}

/// A resumable, bounded C lexer.
#[derive(Debug)]
pub struct ResumableCLexer {
    pending: String,
    base: usize,
    spans: Vec<Span>,
    finished: bool,
    max_pending_bytes: usize,
}

impl ResumableCLexer {
    /// Default cap for the held suffix: 64 kibibytes.
    pub const DEFAULT_MAX_PENDING_BYTES: usize = 64 * 1024;

    /// Create a C lexer with the default held-suffix cap.
    pub fn new() -> Self {
        Self::with_limits(Self::DEFAULT_MAX_PENDING_BYTES)
    }

    /// Create a C lexer with an explicit held-suffix cap.
    pub fn with_limits(max_pending_bytes: usize) -> Self {
        Self {
            pending: String::new(),
            base: 0,
            spans: Vec::new(),
            finished: false,
            max_pending_bytes: max_pending_bytes.max(1),
        }
    }

    /// Held suffix bytes (the unresolved tail).
    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }

    /// Whether [`ResumableCLexer::finish`] has been called.
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Spans released so far, in order.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Feed one chunk of C source.
    ///
    /// The chunk may end anywhere — inside a preprocessor continuation, an
    /// escape sequence, a multibyte character or an unterminated comment.
    /// Truncated UTF-8 heads are held; malformed bytes are a typed refusal.
    pub fn feed(&mut self, chunk: &str) -> Result<FeedReportC, LexCError> {
        if self.finished {
            return Err(LexCError::AlreadyFinished);
        }
        self.pending.push_str(chunk);
        if self.pending.len() > self.max_pending_bytes {
            return Err(LexCError::SuffixTooLong {
                held: self.pending.len(),
                cap: self.max_pending_bytes,
            });
        }
        let before = self.spans.len();
        self.release_through_safe_cut();
        Ok(FeedReportC {
            spans_emitted: self.spans.len() - before,
            pending_bytes: self.pending.len(),
            unresolved: !self.pending.is_empty(),
        })
    }

    /// Flush the held suffix at EOF and seal the lexer.
    ///
    /// After this, [`ResumableCLexer::spans`] tiles the complete fed source.
    pub fn finish(&mut self) -> Result<(), LexCError> {
        if self.finished {
            return Err(LexCError::AlreadyFinished);
        }
        let tail = highlight("c", &self.pending.clone());
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

    /// Release every span that ends at a safe-cut position.
    fn release_through_safe_cut(&mut self) {
        let spans = highlight("c", &self.pending);
        let Some(cut_span) = spans
            .iter()
            .rev()
            .find(|s| is_safe_cut_span(&self.pending, s))
        else {
            return; // no safe cut yet: everything stays held
        };
        let cut = cut_span.end;
        let mut released = Vec::with_capacity(spans.len());
        for span in &spans {
            if span.end > cut {
                break;
            }
            released.push(Span {
                kind: span.kind,
                start: self.base + span.start,
                end: self.base + span.end,
            });
        }
        self.base += cut;
        self.spans.extend(released);
        self.pending.drain(..cut);
    }
}

/// What one [`ResumableCLexer::feed`] produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedReportC {
    /// Spans newly released by this feed.
    pub spans_emitted: usize,
    /// Held suffix bytes after the feed.
    pub pending_bytes: usize,
    /// Whether the unresolved suffix is nonempty.
    pub unresolved: bool,
}

/// The versioned C capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for C.
    pub incremental: bool,
    /// Preprocessor `\`-continuation is classified as part of the directive.
    pub preprocessor_continuation: bool,
    /// Escaped newlines inside string and character literals are handled.
    pub escaped_newline_in_literals: bool,
}

/// The C capability row published by this module.
pub const C_CAPABILITY_V1: CCapabilityV1 = CCapabilityV1 {
    version: 1,
    incremental: true,
    preprocessor_continuation: true,
    escaped_newline_in_literals: true,
};

impl Default for ResumableCLexer {
    fn default() -> Self {
        Self::new()
    }
}
