//! YAML-specific resumable lexer: bounded incremental classification with
//! sound safe-cut release (FCB-022, YAML route / fcb-9vx.19).
//!
//! Same architecture as the TOML/C routes: the batch engine is the
//! classifier; chunks are appended to a held suffix, and only spans ending
//! at a safe-cut byte are released. YAML safe-cut bytes are `: - " '` which
//! settle the function-call detection and value key boundaries.

use crate::highlight::{highlight, Span};

/// Errors from the YAML resumable seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexYamlError {
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

impl LexYamlError {
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
    matches!(byte, b':' | b'-' | b'"' | b'\'' | b'\n')
}

/// A span end is a safe cut when its final byte is a terminal delimiter AND
/// the span does not end at EOF (unresolved suffix).
fn is_safe_cut_span(pending: &str, span: &Span) -> bool {
    if span.end == pending.len() {
        return false;
    }
    let bytes = pending.as_bytes();
    bytes
        .get(span.end.wrapping_sub(1))
        .is_some_and(|b| is_safe_cut_byte(*b))
}

/// A resumable, bounded YAML lexer.
#[derive(Debug)]
pub struct ResumableYamlLexer {
    pending: String,
    base: usize,
    spans: Vec<Span>,
    finished: bool,
    max_pending_bytes: usize,
}

impl ResumableYamlLexer {
    /// Default cap for the held suffix: 64 kibibytes.
    pub const DEFAULT_MAX_PENDING_BYTES: usize = 64 * 1024;

    /// Create a YAML lexer with the default held-suffix cap.
    pub fn new() -> Self {
        Self::with_limits(Self::DEFAULT_MAX_PENDING_BYTES)
    }

    /// Create a YAML lexer with an explicit held-suffix cap.
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

    /// Feed one chunk of YAML source.
    pub fn feed(&mut self, chunk: &str) -> Result<FeedReportYaml, LexYamlError> {
        if self.finished {
            return Err(LexYamlError::AlreadyFinished);
        }
        self.pending.push_str(chunk);
        if self.pending.len() > self.max_pending_bytes {
            return Err(LexYamlError::SuffixTooLong {
                held: self.pending.len(),
                cap: self.max_pending_bytes,
            });
        }
        let before = self.spans.len();
        self.release_through_safe_cut();
        Ok(FeedReportYaml {
            spans_emitted: self.spans.len() - before,
            pending_bytes: self.pending.len(),
            unresolved: !self.pending.is_empty(),
        })
    }

    /// Flush the held suffix at EOF.
    pub fn finish(&mut self) -> Result<(), LexYamlError> {
        if self.finished {
            return Err(LexYamlError::AlreadyFinished);
        }
        let tail = highlight("yaml", &self.pending.clone());
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
        let spans = highlight("yaml", &self.pending);
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

/// What one [`ResumableYamlLexer::feed`] produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedReportYaml {
    /// Spans newly released by this feed.
    pub spans_emitted: usize,
    /// Held suffix bytes after the feed.
    pub pending_bytes: usize,
    /// Whether the unresolved suffix is nonempty.
    pub unresolved: bool,
}

/// The versioned YAML capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YamlCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for YAML.
    pub incremental: bool,
    /// Basic (double-quote) and literal (single-quote) strings.
    pub basic_and_literal_strings: bool,
    /// Key/value colon boundaries and list-item dashes as safe cuts.
    pub colon_and_dash_cuts: bool,
}

/// The YAML capability row published by this module.
pub const YAML_CAPABILITY_V1: YamlCapabilityV1 = YamlCapabilityV1 {
    version: 1,
    incremental: true,
    basic_and_literal_strings: true,
    colon_and_dash_cuts: true,
};
