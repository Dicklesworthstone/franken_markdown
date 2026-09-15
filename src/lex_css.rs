//! CSS-specific resumable lexer: bounded incremental classification with
//! sound safe-cut release (FCB-022, CSS route / fcb-9vx.22).
//!
//! Same safe-cut architecture as the C/TOML/YAML routes: the batch engine
//! (`crate::highlight`, CSS rules) is the classifier; chunks are appended to
//! a held suffix, and only spans ending at a safe-cut byte are released.
//! CSS safe-cut bytes are `; { } : " '` which settle declaration/block
//! boundaries, property-value colons and string literals.

use crate::highlight::{highlight, Span};

/// Errors from the CSS resumable seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexCssError {
    /// The held suffix exceeded the configured cap.
    SuffixTooLong {
        /// Held bytes at refusal.
        held: usize,
        /// The configured cap.
        cap: usize,
    },
    /// The lexer was already finished.
    AlreadyFinished,
}

impl LexCssError {
    /// Stable machine-readable code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::SuffixTooLong { .. } => "SUFFIX_TOO_LONG",
            Self::AlreadyFinished => "ALREADY_FINISHED",
        }
    }
}

/// Whether a byte terminates a lookahead-safe cut position.
fn is_safe_cut_byte(byte: u8) -> bool {
    matches!(byte, b';' | b'{' | b'}' | b':' | b',' | b'"' | b'\'')
}

/// A span end is a safe cut when its final byte is a terminal delimiter AND
/// the span does not end at EOF.
fn is_safe_cut_span(pending: &str, span: &Span) -> bool {
    if span.end == pending.len() {
        return false;
    }
    let bytes = pending.as_bytes();
    if span.kind == crate::highlight::Tok::Comment {
        return span.end.saturating_sub(span.start) >= 4
            && bytes.get(span.end.wrapping_sub(2)) == Some(&b'*')
            && bytes.get(span.end.wrapping_sub(1)) == Some(&b'/');
    }
    if span.kind == crate::highlight::Tok::Str {
        if span.end.saturating_sub(span.start) < 2 {
            return false;
        }
        let Some(last) = bytes.get(span.end.wrapping_sub(1)) else {
            return false;
        };
        return (*last == b'"' || *last == b'\'') && bytes.get(span.start) == Some(last);
    }
    bytes
        .get(span.end.wrapping_sub(1))
        .is_some_and(|b| is_safe_cut_byte(*b))
}

/// A resumable, bounded CSS lexer.
#[derive(Debug)]
pub struct ResumableCssLexer {
    pending: String,
    base: usize,
    spans: Vec<Span>,
    finished: bool,
    max_pending_bytes: usize,
}

impl ResumableCssLexer {
    /// Default cap for the held suffix: 64 kibibytes.
    pub const DEFAULT_MAX_PENDING_BYTES: usize = 64 * 1024;

    /// Create a CSS lexer with the default held-suffix cap.
    pub fn new() -> Self {
        Self::with_limits(Self::DEFAULT_MAX_PENDING_BYTES)
    }

    /// Create a CSS lexer with an explicit held-suffix cap.
    pub fn with_limits(max_pending_bytes: usize) -> Self {
        Self {
            pending: String::new(),
            base: 0,
            spans: Vec::new(),
            finished: false,
            max_pending_bytes: max_pending_bytes.max(1),
        }
    }

    /// Held suffix bytes.
    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }

    /// Whether the lexer has been sealed.
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Spans released so far.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Feed one chunk of CSS source.
    pub fn feed(&mut self, chunk: &str) -> Result<FeedReportCss, LexCssError> {
        if self.finished {
            return Err(LexCssError::AlreadyFinished);
        }
        self.pending.push_str(chunk);
        if self.pending.len() > self.max_pending_bytes {
            return Err(LexCssError::SuffixTooLong {
                held: self.pending.len(),
                cap: self.max_pending_bytes,
            });
        }
        let before = self.spans.len();
        self.release_through_safe_cut();
        Ok(FeedReportCss {
            spans_emitted: self.spans.len() - before,
            pending_bytes: self.pending.len(),
            unresolved: !self.pending.is_empty(),
        })
    }

    /// Flush the held suffix at EOF.
    pub fn finish(&mut self) -> Result<(), LexCssError> {
        if self.finished {
            return Err(LexCssError::AlreadyFinished);
        }
        let tail = highlight("css", &self.pending.clone());
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
        let spans = highlight("css", &self.pending);
        let Some(cut_span) = spans
            .iter()
            .rev()
            .find(|s| is_safe_cut_span(&self.pending, s))
        else {
            return;
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

/// What one [`ResumableCssLexer::feed`] produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedReportCss {
    /// Spans newly released by this feed.
    pub spans_emitted: usize,
    /// Held suffix bytes after the feed.
    pub pending_bytes: usize,
    /// Whether the unresolved suffix is nonempty.
    pub unresolved: bool,
}

/// The versioned CSS capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CssCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for CSS.
    pub incremental: bool,
    /// Comments, strings and nested qualified constructs.
    pub comments_strings_nested: bool,
}

/// The CSS capability row published by this module.
pub const CSS_CAPABILITY_V1: CssCapabilityV1 = CssCapabilityV1 {
    version: 1,
    incremental: true,
    comments_strings_nested: true,
};
