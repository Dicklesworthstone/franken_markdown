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

use crate::highlight::{highlight, is_supported, Span, Tok};

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

    /// Create a snapshot checkpoint of this lexer's state at the current
    /// emitted boundary.
    pub fn checkpoint(&self, source_revision: u64) -> LexerCheckpoint {
        LexerCheckpoint {
            version: CHECKPOINT_VERSION,
            lang: self.lang.clone(),
            source_revision,
            byte_offset: self.base as u64,
            comment_state: self.detect_comment_state(),
            string_state: self.detect_string_state(),
            interpolation_depth: 0,
            is_eof: self.finished,
            unresolved_suffix: self.pending.clone(),
        }
    }

    /// Create a new lexer resuming from a validated checkpoint.
    ///
    /// Validates source correspondence against `expected_revision` and
    /// `expected_offset`. If they do not match, the checkpoint is refused
    /// instead of silently classifying mismatched bytes.
    pub fn from_checkpoint(
        checkpoint: &LexerCheckpoint,
        expected_revision: u64,
        expected_offset: u64,
    ) -> Result<Self, CheckpointError> {
        checkpoint.validate()?;
        if checkpoint.source_revision != expected_revision {
            return Err(CheckpointError::SourceCorrespondenceMismatch {
                expected_revision,
                found_revision: checkpoint.source_revision,
            });
        }
        if checkpoint.byte_offset != expected_offset {
            return Err(CheckpointError::OffsetMismatch {
                expected_offset,
                found_offset: checkpoint.byte_offset,
            });
        }
        Ok(Self {
            lang: checkpoint.lang.clone(),
            pending: checkpoint.unresolved_suffix.clone(),
            base: checkpoint.byte_offset as usize,
            spans: Vec::new(),
            finished: checkpoint.is_eof,
            max_pending_bytes: Self::DEFAULT_MAX_PENDING_BYTES,
        })
    }

    /// Inspect the pending suffix to detect active comment state at a checkpoint.
    pub fn detect_comment_state(&self) -> CommentState {
        let Ok(text) = self.pending_text() else {
            return CommentState::None;
        };
        let spans = highlight(&self.lang, &text);
        if let Some(last) = spans.last()
            && last.end == text.len()
            && last.kind == Tok::Comment
        {
            let tail = &text[last.start..];
            if tail.starts_with("/*") || tail.starts_with("<#") {
                return CommentState::Block { depth: 1 };
            } else {
                return CommentState::Line;
            }
        }
        CommentState::None
    }

    /// Inspect the pending suffix to detect active string state at a checkpoint.
    pub fn detect_string_state(&self) -> StringState {
        let Ok(text) = self.pending_text() else {
            return StringState::None;
        };
        let spans = highlight(&self.lang, &text);
        if let Some(last) = spans.last()
            && last.end == text.len()
            && last.kind == Tok::Str
        {
            let tail = &text[last.start..];
            if tail.starts_with('\'') {
                return StringState::SingleQuote;
            } else if tail.starts_with('"') {
                return StringState::DoubleQuote;
            } else if tail.starts_with('`') {
                return StringState::Backtick;
            } else if tail.starts_with("r#") || tail.starts_with("r\"") {
                let hashes = tail
                    .strip_prefix('r')
                    .unwrap_or("")
                    .chars()
                    .take_while(|&ch| ch == '#')
                    .count() as u8;
                return StringState::RawString { hashes };
            }
        }
        StringState::None
    }
}

/// Lexer checkpoint format version.
pub const CHECKPOINT_VERSION: u32 = 1;

/// Magic 4-byte identifier for serialized lexer checkpoints (`"FMDL"`).
pub const CHECKPOINT_MAGIC: [u8; 4] = *b"FMDL";

/// Maximum allowed serialized checkpoint size in bytes (1 KiB).
pub const MAX_CHECKPOINT_BYTES: usize = 1024;

/// Maximum comment nesting depth.
pub const MAX_COMMENT_DEPTH: u16 = 64;

/// Maximum string interpolation nesting depth.
pub const MAX_INTERPOLATION_DEPTH: u16 = 64;

/// Maximum length of language identifier string.
pub const MAX_LANG_LEN: usize = 64;

/// Maximum length of held unresolved suffix in a checkpoint.
pub const MAX_SUFFIX_BYTES: usize = 512;

/// Comment lexical state carried across checkpoint boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentState {
    /// Outside any comment.
    None,
    /// Inside a single-line comment.
    Line,
    /// Inside a (potentially nested) block comment.
    Block {
        /// Nesting depth (capped at [`MAX_COMMENT_DEPTH`]).
        depth: u16,
    },
}

/// String/literal lexical state carried across checkpoint boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringState {
    /// Outside any string literal.
    None,
    /// Inside a single-quoted string literal (`'...'`).
    SingleQuote,
    /// Inside a double-quoted string literal (`"..."`).
    DoubleQuote,
    /// Inside a template or backtick literal (`` `...` ``).
    Backtick,
    /// Inside a raw string literal with a given number of `#` hashes (e.g. `r##"..."##`).
    RawString {
        /// Number of hash delimiters required to terminate the raw string.
        hashes: u8,
    },
}

/// Errors validating or deserializing a [`LexerCheckpoint`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckpointError {
    /// Magic header does not match [`CHECKPOINT_MAGIC`].
    InvalidMagic,
    /// Checkpoint ABI/format version is not supported.
    UnsupportedVersion {
        /// Version found in payload.
        found: u32,
        /// Expected supported version.
        expected: u32,
    },
    /// Serialized payload exceeds [`MAX_CHECKPOINT_BYTES`].
    PayloadTooLarge {
        /// Byte count of the oversized payload.
        bytes: usize,
        /// Configured bound.
        cap: usize,
    },
    /// Premature end of serialized checkpoint payload.
    UnexpectedEof,
    /// Nesting depth exceeds allowed limit.
    DepthLimitExceeded {
        /// Depth encountered.
        depth: u16,
        /// Maximum allowed limit.
        max: u16,
    },
    /// Unknown comment state tag in serialized data.
    InvalidCommentTag(u8),
    /// Unknown string state tag in serialized data.
    InvalidStringTag(u8),
    /// Unknown end-of-file flag byte in serialized data.
    InvalidEofFlag(u8),
    /// Checkpoint language is not supported.
    UnsupportedLanguage(String),
    /// Source revision in checkpoint does not match expected revision.
    SourceCorrespondenceMismatch {
        /// Expected source capture revision.
        expected_revision: u64,
        /// Revision recorded in the checkpoint.
        found_revision: u64,
    },
    /// Checkpoint byte offset does not match expected resume offset.
    OffsetMismatch {
        /// Expected byte offset.
        expected_offset: u64,
        /// Offset recorded in the checkpoint.
        found_offset: u64,
    },
    /// Checkpoint language does not match expected language.
    LanguageMismatch {
        /// Expected language.
        expected: String,
        /// Language recorded in the checkpoint.
        found: String,
    },
    /// Unresolved suffix or language string in checkpoint contains invalid UTF-8.
    InvalidUtf8,
    /// Checkpoint payload contains trailing unparsed bytes.
    TrailingBytes,
}

impl CheckpointError {
    /// Stable machine-readable code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidMagic => "INVALID_MAGIC",
            Self::UnsupportedVersion { .. } => "UNSUPPORTED_VERSION",
            Self::PayloadTooLarge { .. } => "PAYLOAD_TOO_LARGE",
            Self::UnexpectedEof => "UNEXPECTED_EOF",
            Self::DepthLimitExceeded { .. } => "DEPTH_LIMIT_EXCEEDED",
            Self::InvalidCommentTag(_) => "INVALID_COMMENT_TAG",
            Self::InvalidStringTag(_) => "INVALID_STRING_TAG",
            Self::InvalidEofFlag(_) => "INVALID_EOF_FLAG",
            Self::UnsupportedLanguage(_) => "UNSUPPORTED_LANGUAGE",
            Self::SourceCorrespondenceMismatch { .. } => "SOURCE_CORRESPONDENCE_MISMATCH",
            Self::OffsetMismatch { .. } => "OFFSET_MISMATCH",
            Self::LanguageMismatch { .. } => "LANGUAGE_MISMATCH",
            Self::InvalidUtf8 => "INVALID_UTF8",
            Self::TrailingBytes => "TRAILING_BYTES",
        }
    }
}

/// Versioned, bounded state checkpoint for restarting lexical classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexerCheckpoint {
    /// Format/ABI version.
    pub version: u32,
    /// Language identifier.
    pub lang: String,
    /// Verified source capture / document revision.
    pub source_revision: u64,
    /// Exact byte offset into the captured source where this checkpoint applies.
    pub byte_offset: u64,
    /// Active comment state at the checkpoint boundary.
    pub comment_state: CommentState,
    /// Active string state at the checkpoint boundary.
    pub string_state: StringState,
    /// Interpolation brace nesting depth.
    pub interpolation_depth: u16,
    /// Explicit end-of-input flag.
    pub is_eof: bool,
    /// Unresolved suffix bytes held at this boundary.
    pub unresolved_suffix: Vec<u8>,
}

impl LexerCheckpoint {
    /// Serialize this checkpoint into a compact, versioned byte vector.
    ///
    /// The serialized format begins with [`CHECKPOINT_MAGIC`] and [`CHECKPOINT_VERSION`],
    /// followed by fixed-width little-endian fields and bounded length-prefixed strings.
    pub fn to_bytes(&self) -> Vec<u8> {
        let lang_bytes = self.lang.as_bytes();
        let suffix_bytes = &self.unresolved_suffix;
        let mut buf = Vec::with_capacity(36 + lang_bytes.len() + suffix_bytes.len());

        buf.extend_from_slice(&CHECKPOINT_MAGIC);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.source_revision.to_le_bytes());
        buf.extend_from_slice(&self.byte_offset.to_le_bytes());

        let (comment_tag, comment_depth) = match self.comment_state {
            CommentState::None => (0u8, 0u16),
            CommentState::Line => (1u8, 0u16),
            CommentState::Block { depth } => (2u8, depth),
        };
        buf.push(comment_tag);
        buf.extend_from_slice(&comment_depth.to_le_bytes());

        let (string_tag, string_hashes) = match self.string_state {
            StringState::None => (0u8, 0u8),
            StringState::SingleQuote => (1u8, 0u8),
            StringState::DoubleQuote => (2u8, 0u8),
            StringState::Backtick => (3u8, 0u8),
            StringState::RawString { hashes } => (4u8, hashes),
        };
        buf.push(string_tag);
        buf.push(string_hashes);

        buf.extend_from_slice(&self.interpolation_depth.to_le_bytes());
        buf.push(if self.is_eof { 1u8 } else { 0u8 });

        buf.extend_from_slice(&(lang_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(lang_bytes);

        buf.extend_from_slice(&(suffix_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(suffix_bytes);

        buf
    }

    /// Deserialize and validate a [`LexerCheckpoint`] from bytes.
    ///
    /// Validates magic header, format version, size limits, and nesting depth.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CheckpointError> {
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(CheckpointError::PayloadTooLarge {
                bytes: bytes.len(),
                cap: MAX_CHECKPOINT_BYTES,
            });
        }
        if bytes.len() < 36 {
            return Err(CheckpointError::UnexpectedEof);
        }

        let mut offset = 0;
        if bytes[offset..offset + 4] != CHECKPOINT_MAGIC {
            return Err(CheckpointError::InvalidMagic);
        }
        offset += 4;

        let version = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;
        if version != CHECKPOINT_VERSION {
            return Err(CheckpointError::UnsupportedVersion {
                found: version,
                expected: CHECKPOINT_VERSION,
            });
        }

        let source_revision = u64::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
        offset += 8;

        let byte_offset = u64::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
        offset += 8;

        let comment_tag = bytes[offset];
        offset += 1;
        let comment_depth = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;
        if comment_depth > MAX_COMMENT_DEPTH {
            return Err(CheckpointError::DepthLimitExceeded {
                depth: comment_depth,
                max: MAX_COMMENT_DEPTH,
            });
        }
        let comment_state = match comment_tag {
            0 => CommentState::None,
            1 => CommentState::Line,
            2 => CommentState::Block {
                depth: comment_depth,
            },
            other => return Err(CheckpointError::InvalidCommentTag(other)),
        };

        let string_tag = bytes[offset];
        offset += 1;
        let string_hashes = bytes[offset];
        offset += 1;
        let string_state = match string_tag {
            0 => StringState::None,
            1 => StringState::SingleQuote,
            2 => StringState::DoubleQuote,
            3 => StringState::Backtick,
            4 => StringState::RawString {
                hashes: string_hashes,
            },
            other => return Err(CheckpointError::InvalidStringTag(other)),
        };

        let interpolation_depth = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;
        if interpolation_depth > MAX_INTERPOLATION_DEPTH {
            return Err(CheckpointError::DepthLimitExceeded {
                depth: interpolation_depth,
                max: MAX_INTERPOLATION_DEPTH,
            });
        }

        let is_eof = match bytes[offset] {
            0 => false,
            1 => true,
            other => return Err(CheckpointError::InvalidEofFlag(other)),
        };
        offset += 1;

        let lang_len = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset += 2;
        if lang_len > MAX_LANG_LEN {
            return Err(CheckpointError::PayloadTooLarge {
                bytes: lang_len,
                cap: MAX_LANG_LEN,
            });
        }
        if offset + lang_len > bytes.len() {
            return Err(CheckpointError::UnexpectedEof);
        }
        let lang_str = std::str::from_utf8(&bytes[offset..offset + lang_len])
            .map_err(|_| CheckpointError::InvalidUtf8)?;
        if !is_supported(lang_str) {
            return Err(CheckpointError::UnsupportedLanguage(lang_str.to_string()));
        }
        let lang = lang_str.to_string();
        offset += lang_len;

        if offset + 2 > bytes.len() {
            return Err(CheckpointError::UnexpectedEof);
        }
        let suffix_len = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset += 2;
        if suffix_len > MAX_SUFFIX_BYTES {
            return Err(CheckpointError::PayloadTooLarge {
                bytes: suffix_len,
                cap: MAX_SUFFIX_BYTES,
            });
        }
        if offset + suffix_len > bytes.len() {
            return Err(CheckpointError::UnexpectedEof);
        }
        let unresolved_suffix = bytes[offset..offset + suffix_len].to_vec();
        offset += suffix_len;

        if offset != bytes.len() {
            return Err(CheckpointError::TrailingBytes);
        }

        let cp = Self {
            version,
            lang,
            source_revision,
            byte_offset,
            comment_state,
            string_state,
            interpolation_depth,
            is_eof,
            unresolved_suffix,
        };
        cp.validate()?;
        Ok(cp)
    }

    /// Validate invariant bounds and language support.
    pub fn validate(&self) -> Result<(), CheckpointError> {
        if self.version != CHECKPOINT_VERSION {
            return Err(CheckpointError::UnsupportedVersion {
                found: self.version,
                expected: CHECKPOINT_VERSION,
            });
        }
        if !is_supported(&self.lang) {
            return Err(CheckpointError::UnsupportedLanguage(self.lang.clone()));
        }
        if self.lang.len() > MAX_LANG_LEN {
            return Err(CheckpointError::PayloadTooLarge {
                bytes: self.lang.len(),
                cap: MAX_LANG_LEN,
            });
        }
        if self.unresolved_suffix.len() > MAX_SUFFIX_BYTES {
            return Err(CheckpointError::PayloadTooLarge {
                bytes: self.unresolved_suffix.len(),
                cap: MAX_SUFFIX_BYTES,
            });
        }
        if let CommentState::Block { depth } = self.comment_state
            && depth > MAX_COMMENT_DEPTH
        {
            return Err(CheckpointError::DepthLimitExceeded {
                depth,
                max: MAX_COMMENT_DEPTH,
            });
        }
        if self.interpolation_depth > MAX_INTERPOLATION_DEPTH {
            return Err(CheckpointError::DepthLimitExceeded {
                depth: self.interpolation_depth,
                max: MAX_INTERPOLATION_DEPTH,
            });
        }
        Ok(())
    }
}

/// A coalesced span representation where adjacent spans of identical kind
/// have been merged into a single contiguous token range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoalescedSpan {
    /// Token kind.
    pub kind: Tok,
    /// Inclusive byte start in the original source.
    pub start: usize,
    /// Exclusive byte end in the original source.
    pub end: usize,
}

/// Coalesce adjacent spans of identical [`Tok`] kind into contiguous ranges.
pub fn coalesce_spans(spans: &[Span]) -> Vec<CoalescedSpan> {
    let mut runs: Vec<CoalescedSpan> = Vec::new();
    for span in spans {
        match runs.last_mut() {
            Some(last) if last.kind == span.kind && last.end == span.start => {
                last.end = span.end;
            }
            _ => runs.push(CoalescedSpan {
                kind: span.kind,
                start: span.start,
                end: span.end,
            }),
        }
    }
    runs
}

/// Highlight `code` for `lang` by feeding it in chunks through [`ResumableLexer`].
///
/// This provides a drop-in streaming alternative to [`crate::highlight::highlight`],
/// returning spans that tile `code` exactly.
pub fn highlight_chunked(
    lang: &str,
    code: &str,
    chunk_size: usize,
) -> Result<Vec<Span>, ResumeError> {
    let mut lexer = ResumableLexer::new(lang)?;
    let step = chunk_size.max(1);
    let bytes = code.as_bytes();
    for chunk in bytes.chunks(step) {
        lexer.feed(chunk)?;
    }
    lexer.finish()?;
    Ok(lexer.spans().to_vec())
}

/// Verify that whole-block classification and chunked resumable classification
/// produce identical token meaning after coalescing adjacent same-kind spans.
pub fn verify_whole_block_coalesced_equivalence(
    lang: &str,
    code: &str,
    chunk_size: usize,
) -> Result<bool, ResumeError> {
    let whole_spans = highlight(lang, code);
    let chunked_spans = highlight_chunked(lang, code, chunk_size)?;
    let whole_coalesced = coalesce_spans(&whole_spans);
    let chunked_coalesced = coalesce_spans(&chunked_spans);
    Ok(whole_coalesced == chunked_coalesced)
}
