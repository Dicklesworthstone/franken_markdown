//! Resumable lexical engine with bounded replay state (FCB-021.A).
//!
//! Each feed reclassifies the unresolved suffix plus the new bytes. Only
//! complete spans before the language's hold boundary are released. EOF flushes
//! the suffix; arbitrary byte splits, including partial UTF-8 scalars, are
//! supported. Language-specific classification remains in the shared engine.
//!
//! Rejected feeds leave source bytes, offsets, output, and EOF state unchanged.
//! The replay-buffer limit is checked before copying input or allocating a
//! validation buffer. Callers should split feeds to fit that limit.
//!
//! Output accumulation is optional: drain [`ResumableLexer::take_spans`] after
//! each feed to consume spans without retaining the entire stream. Checkpoints
//! store replay bytes, not emitted output, and preserve absolute source offsets.

use crate::highlight::{Span, Tok, highlight, is_supported};

#[path = "resume/checkpoint.rs"]
mod checkpoint;

pub use checkpoint::{
    CHECKPOINT_MAGIC, CHECKPOINT_VERSION, CheckpointError, CommentState, LexerCheckpoint,
    MAX_CHECKPOINT_BYTES, MAX_COMMENT_DEPTH, MAX_INTERPOLATION_DEPTH, MAX_LANG_LEN,
    MAX_SUFFIX_BYTES, StringState,
};

/// Errors from the resumable lexical seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeError {
    /// No supported lexical route exists for the language.
    UnsupportedLanguage,
    /// Pending bytes plus this feed exceed the replay-buffer limit.
    SuffixTooLong {
        /// Attempted replay-buffer byte count. No rejected bytes were retained.
        held: usize,
        /// Configured replay-buffer limit.
        cap: usize,
    },
    /// Malformed UTF-8, or an incomplete final scalar when finishing.
    InvalidUtf8 {
        /// On feed, the offset within the rejected chunk. For a sequence
        /// started in a previous chunk, this is the first new byte proving it
        /// invalid. On finish, the offset is within the pending suffix.
        at: usize,
    },
    /// The stream was already finished.
    AlreadyFinished,
    /// Accepting the input would overflow absolute source byte offsets.
    OffsetOverflow,
}

impl ResumeError {
    /// Stable machine-readable code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedLanguage => "UNSUPPORTED_LANGUAGE",
            Self::SuffixTooLong { .. } => "SUFFIX_TOO_LONG",
            Self::InvalidUtf8 { .. } => "INVALID_UTF8",
            Self::AlreadyFinished => "ALREADY_FINISHED",
            Self::OffsetOverflow => "OFFSET_OVERFLOW",
        }
    }
}

/// Result of an accepted feed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedReport {
    /// Newly released spans, independent of previously drained output.
    pub spans_emitted: usize,
    /// Bytes still awaiting classification/finalization.
    pub pending_bytes: usize,
    /// Whether any source bytes remain pending.
    pub unresolved: bool,
}

/// A bounded replay front end over the batch classifier.
///
/// `base` is the absolute emitted boundary. Pending bytes start at that
/// boundary; draining output never changes it. The output vector is retained
/// for compatibility, but streaming consumers can transfer it with `take_spans`.
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
    /// Default replay-buffer cap: one mebibyte.
    pub const DEFAULT_MAX_PENDING_BYTES: usize = 1 << 20;

    /// Create a lexer for a supported language.
    pub fn new(lang: &str) -> Result<Self, ResumeError> {
        Self::with_limits(lang, Self::DEFAULT_MAX_PENDING_BYTES)
    }

    /// Create a lexer with an explicit replay-buffer limit, at least one byte.
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

    /// The language supplied at construction/restoration.
    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// Current unresolved source-byte count.
    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }

    /// Configured replay-buffer limit.
    pub const fn max_pending_bytes(&self) -> usize {
        self.max_pending_bytes
    }

    /// Absolute end of accepted source, including unresolved bytes.
    pub fn received_bytes(&self) -> usize {
        self.base + self.pending.len()
    }

    /// Absolute boundary up to which source has been emitted.
    pub const fn emitted_bytes(&self) -> usize {
        self.base
    }

    /// Whether EOF was successfully finalized.
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Undrained output spans with absolute source offsets. Initially these
    /// tile `[0, emitted_bytes())`; after restoration/draining, they tile only
    /// the subsequently emitted interval. Pending bytes are not included.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Transfer emitted output without cloning it or retaining its allocation.
    /// Further feeds keep absolute offsets and the unresolved lexical context.
    pub fn take_spans(&mut self) -> Vec<Span> {
        std::mem::take(&mut self.spans)
    }

    /// Accept one byte chunk, including a partial final UTF-8 scalar.
    ///
    /// Every typed refusal is transactional: the same lexer can accept a
    /// corrected or smaller chunk afterward. Oversized input is rejected before
    /// copying or scanning it; existing emitted output is never discarded.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<FeedReport, ResumeError> {
        if self.finished {
            return Err(ResumeError::AlreadyFinished);
        }
        let projected = self.pending.len().checked_add(chunk.len()).ok_or(
            ResumeError::SuffixTooLong { held: usize::MAX, cap: self.max_pending_bytes },
        )?;
        if projected > self.max_pending_bytes {
            return Err(ResumeError::SuffixTooLong { held: projected, cap: self.max_pending_bytes });
        }
        self.base.checked_add(projected).ok_or(ResumeError::OffsetOverflow)?;
        validate_append_utf8(&self.pending, chunk)?;

        // All fallible validation precedes the first mutation.
        self.pending.extend_from_slice(chunk);
        let before = self.spans.len();
        self.lex_pending_release();
        Ok(FeedReport {
            spans_emitted: self.spans.len() - before,
            pending_bytes: self.pending.len(),
            unresolved: !self.pending.is_empty(),
        })
    }

    /// Finalize EOF. An incomplete UTF-8 scalar is refused without sealing the
    /// stream: its remaining bytes may still be fed and finish retried.
    pub fn finish(&mut self) -> Result<(), ResumeError> {
        if self.finished {
            return Err(ResumeError::AlreadyFinished);
        }
        let text = self.pending_text()?;
        let tail = highlight(&self.lang, text);
        for span in tail {
            if span.start < span.end {
                self.spans.push(Span {
                    kind: span.kind,
                    start: self.base + span.start,
                    end: self.base + span.end,
                });
            }
        }
        // EOF checkpoints must point past the flushed suffix, not at its start.
        self.base += self.pending.len();
        self.pending.clear();
        self.finished = true;
        Ok(())
    }

    fn pending_text(&self) -> Result<&str, ResumeError> {
        std::str::from_utf8(&self.pending)
            .map_err(|error| ResumeError::InvalidUtf8 { at: error.valid_up_to() })
    }

    /// Normalize only for choosing a hold policy. Keep the caller's original
    /// language string in public state and checkpoints.
    fn language_key(&self) -> &str {
        let trimmed = self.lang.trim();
        let without_prefix = trimmed.get(.."language-".len())
            .filter(|prefix| prefix.eq_ignore_ascii_case("language-"))
            .map_or(trimmed, |_| &trimmed["language-".len()..]);
        let end = without_prefix.find(|ch: char| ch.is_whitespace() || ch == ',')
            .unwrap_or(without_prefix.len());
        &without_prefix[..end]
    }

    fn is_html_family(&self) -> bool {
        ["html", "htm", "xhtml", "xml", "svg"].iter()
            .any(|lang| self.language_key().eq_ignore_ascii_case(lang))
    }

    fn is_jsx_family(&self) -> bool {
        ["jsx", "tsx"].iter().any(|lang| self.language_key().eq_ignore_ascii_case(lang))
    }

    fn is_javascript_family(&self) -> bool {
        ["javascript", "js", "mjs", "cjs", "typescript", "ts"].iter()
            .any(|lang| self.language_key().eq_ignore_ascii_case(lang))
    }

    fn lex_pending_release(&mut self) {
        // A partial scalar at the tail does not prevent emitting complete
        // tokens earlier in the buffer. Its bytes remain beyond the hold point.
        let valid_length = match std::str::from_utf8(&self.pending) {
            Ok(text) => text.len(),
            Err(error) => error.valid_up_to(),
        };
        let Ok(text) = std::str::from_utf8(&self.pending[..valid_length]) else { return; };
        let spans = highlight(&self.lang, text);
        let Some(last) = spans.last() else { return; };
        let mut hold_from = last.start;
        if self.is_html_family() {
            if let Some(open_tag) = find_unclosed_html_tag(text, &spans) {
                hold_from = open_tag;
            } else if is_html_closed_construct(text, last) {
                hold_from = last.end;
            }
        } else if self.is_jsx_family() {
            hold_from = crate::lang_jsx::find_jsx_hold_from(text, &spans);
        } else if self.is_javascript_family() {
            hold_from = find_javascript_hold_from(text, &spans);
        }
        // A policy can point inside a multi-byte/multi-character token. Never
        // drain bytes that were not actually emitted as complete spans.
        let mut released_end = 0;
        for span in spans.iter().take_while(|span| span.end <= hold_from) {
            if span.start < span.end {
                self.spans.push(Span {
                    kind: span.kind,
                    start: self.base + span.start,
                    end: self.base + span.end,
                });
                released_end = span.end;
            }
        }
        self.base += released_end;
        self.pending.drain(..released_end);
    }

    /// Capture an in-memory snapshot. Large replay suffixes can exceed the
    /// portable checkpoint format; use `try_checkpoint` before persistence.
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

    /// Capture a checkpoint guaranteed to fit and round-trip through the wire
    /// format. Oversized suffixes are refused before cloning their bytes.
    pub fn try_checkpoint(&self, source_revision: u64) -> Result<LexerCheckpoint, CheckpointError> {
        if self.lang.len() > MAX_LANG_LEN {
            return Err(CheckpointError::PayloadTooLarge { bytes: self.lang.len(), cap: MAX_LANG_LEN });
        }
        if self.pending.len() > MAX_SUFFIX_BYTES {
            return Err(CheckpointError::PayloadTooLarge { bytes: self.pending.len(), cap: MAX_SUFFIX_BYTES });
        }
        let checkpoint = self.checkpoint(source_revision);
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    /// Restore using the default replay limit. Revision/offset correspondence
    /// must match the caller's captured source. Feed subsequent input starting
    /// at `byte_offset + unresolved_suffix.len()`, not at `byte_offset` again.
    pub fn from_checkpoint(
        checkpoint: &LexerCheckpoint,
        expected_revision: u64,
        expected_offset: u64,
    ) -> Result<Self, CheckpointError> {
        Self::from_checkpoint_with_limits(
            checkpoint, expected_revision, expected_offset, Self::DEFAULT_MAX_PENDING_BYTES,
        )
    }

    /// Restore with an explicit host replay budget. Checked host-width
    /// conversion prevents u64 offsets wrapping on 32-bit/WASM targets.
    pub fn from_checkpoint_with_limits(
        checkpoint: &LexerCheckpoint,
        expected_revision: u64,
        expected_offset: u64,
        max_pending_bytes: usize,
    ) -> Result<Self, CheckpointError> {
        checkpoint.validate()?;
        if checkpoint.source_revision != expected_revision {
            return Err(CheckpointError::SourceCorrespondenceMismatch {
                expected_revision, found_revision: checkpoint.source_revision,
            });
        }
        if checkpoint.byte_offset != expected_offset {
            return Err(CheckpointError::OffsetMismatch {
                expected_offset, found_offset: checkpoint.byte_offset,
            });
        }
        let overflow = || CheckpointError::OffsetOverflow { byte_offset: checkpoint.byte_offset };
        let base = usize::try_from(checkpoint.byte_offset).map_err(|_| overflow())?;
        base.checked_add(checkpoint.unresolved_suffix.len()).ok_or_else(overflow)?;
        let max_pending_bytes = max_pending_bytes.max(1);
        if checkpoint.unresolved_suffix.len() > max_pending_bytes {
            return Err(CheckpointError::PayloadTooLarge {
                bytes: checkpoint.unresolved_suffix.len(), cap: max_pending_bytes,
            });
        }
        Ok(Self {
            lang: checkpoint.lang.clone(),
            pending: checkpoint.unresolved_suffix.clone(),
            base,
            spans: Vec::new(),
            finished: checkpoint.is_eof,
            max_pending_bytes,
        })
    }

    /// Describe the final comment token in the replay suffix.
    pub fn detect_comment_state(&self) -> CommentState {
        let Ok(text) = self.pending_text() else { return CommentState::None; };
        let spans = highlight(&self.lang, text);
        if let Some(last) = spans.last()
            && last.end == text.len()
            && last.kind == Tok::Comment
        {
            let tail = &text[last.start..];
            if tail.starts_with("/*") || tail.starts_with("<#") {
                let mut depth: u16 = 0;
                let bytes = tail.as_bytes();
                let mut i = 0;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth = depth.saturating_add(1);
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth = depth.saturating_sub(1);
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                return CommentState::Block { depth: depth.max(1) };
            } else if tail.starts_with("<!--") {
                return CommentState::Block { depth: 1 };
            } else {
                return CommentState::Line;
            }
        }
        CommentState::None
    }

    /// Describe the final string token in the replay suffix.
    pub fn detect_string_state(&self) -> StringState {
        let Ok(text) = self.pending_text() else { return StringState::None; };
        let spans = highlight(&self.lang, text);
        if let Some(last) = spans.last()
            && last.end == text.len()
            && last.kind == Tok::Str
        {
            let tail = &text[last.start..];
            if tail.starts_with('\'') || tail.starts_with("b'") {
                return StringState::SingleQuote;
            } else if tail.starts_with('"') || tail.starts_with("b\"") {
                return StringState::DoubleQuote;
            } else if tail.starts_with('`') {
                return StringState::Backtick;
            } else if tail.starts_with("r#") || tail.starts_with("r\"")
                || tail.starts_with("br#") || tail.starts_with("br\"")
            {
                let stripped = if tail.starts_with("br") {
                    tail.strip_prefix("br").unwrap_or("")
                } else {
                    tail.strip_prefix('r').unwrap_or("")
                };
                let hashes = stripped.chars().take_while(|&ch| ch == '#').count() as u8;
                return StringState::RawString { hashes };
            }
        }
        StringState::None
    }
}

/// Validate only the boundary scalar plus the new chunk. No allocation scales
/// with attacker-supplied input. Pending bytes were validated by a prior feed
/// or checkpoint restoration and may end in at most three partial bytes.
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
    let mut consumed = 0;
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

/// Coalesced adjacent token spans of identical kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoalescedSpan {
    /// Token kind.
    pub kind: Tok,
    /// Inclusive source offset.
    pub start: usize,
    /// Exclusive source offset.
    pub end: usize,
}

/// Merge adjacent same-kind spans without bridging gaps.
pub fn coalesce_spans(spans: &[Span]) -> Vec<CoalescedSpan> {
    let mut runs: Vec<CoalescedSpan> = Vec::new();
    for span in spans {
        match runs.last_mut() {
            Some(last) if last.kind == span.kind && last.end == span.start => last.end = span.end,
            _ => runs.push(CoalescedSpan { kind: span.kind, start: span.start, end: span.end }),
        }
    }
    runs
}

/// Highlight source using fixed-size byte feeds through the resumable engine.
pub fn highlight_chunked(lang: &str, code: &str, chunk_size: usize) -> Result<Vec<Span>, ResumeError> {
    let mut lexer = ResumableLexer::new(lang)?;
    for chunk in code.as_bytes().chunks(chunk_size.max(1)) {
        lexer.feed(chunk)?;
    }
    lexer.finish()?;
    Ok(lexer.take_spans())
}

/// Compare whole-block and streamed token meaning after coalescing.
pub fn verify_whole_block_coalesced_equivalence(
    lang: &str,
    code: &str,
    chunk_size: usize,
) -> Result<bool, ResumeError> {
    let whole = highlight(lang, code);
    let chunked = highlight_chunked(lang, code, chunk_size)?;
    Ok(coalesce_spans(&whole) == coalesce_spans(&chunked))
}

/// Hold an unclosed HTML tag or embedded script/style block with its opener.
fn find_unclosed_html_tag(text: &str, spans: &[Span]) -> Option<usize> {
    let trailing_opener = spans.iter().find_map(|span| {
        (span.kind == Tok::Operator && matches!(&text[span.start..], "<" | "</"))
            .then_some(span.start)
    });
    let mut unclosed_start = None;
    let mut in_script_or_style: Option<(&str, usize)> = None;
    for (i, span) in spans.iter().enumerate() {
        let slice = &text[span.start..span.end];
        if span.kind == Tok::Operator && (slice == "<" || slice == "</") {
            if i + 1 < spans.len() && spans[i + 1].kind == Tok::Keyword {
                let tag_name = &text[spans[i + 1].start..spans[i + 1].end];
                if slice == "<" {
                    if tag_name.eq_ignore_ascii_case("script") {
                        in_script_or_style = Some(("script", span.start));
                    } else if tag_name.eq_ignore_ascii_case("style") {
                        in_script_or_style = Some(("style", span.start));
                    }
                } else if let Some((active_name, _)) = in_script_or_style {
                    if tag_name.eq_ignore_ascii_case(active_name) {
                        in_script_or_style = None;
                    }
                }
                unclosed_start = Some(span.start);
            }
        } else if span.kind == Tok::Operator && (slice == ">" || slice == "/>") {
            if slice == "/>" {
                in_script_or_style = None;
            }
            unclosed_start = None;
        }
    }
    if unclosed_start.is_some() {
        unclosed_start
    } else if let Some((_, start)) = in_script_or_style {
        Some(start)
    } else {
        trailing_opener
    }
}

/// Closed HTML constructs cannot be extended by a subsequent chunk.
fn is_html_closed_construct(text: &str, last: &Span) -> bool {
    let slice = &text[last.start..last.end];
    match last.kind {
        Tok::Operator => slice == ">" || slice == "/>",
        Tok::Comment => slice.starts_with("<!--") && slice.ends_with("-->"),
        Tok::Str => slice.starts_with("<![CDATA[") && slice.ends_with("]]>"),
        Tok::Keyword => {
            (slice.starts_with("<!") && slice.ends_with('>'))
                || (slice.starts_with("<?") && slice.ends_with("?>"))
                || (slice.starts_with('&') && slice.ends_with(';'))
        }
        _ => false,
    }
}

fn is_javascript_value_token(text: &str, span: &Span) -> bool {
    let slice = &text[span.start..span.end];
    match span.kind {
        Tok::Plain | Tok::Type | Tok::Number | Tok::Str => true,
        Tok::Punct => slice == ")" || slice == "]",
        Tok::Keyword => matches!(slice, "this" | "true" | "false" | "null" | "super"),
        _ => false,
    }
}

/// Preserve JavaScript's preceding-value context across slash/comparison and
/// whitespace boundaries. The JSX lexer also uses this shared hold policy.
pub(crate) fn find_javascript_hold_from(text: &str, spans: &[Span]) -> usize {
    let Some(last) = spans.last() else { return 0; };
    let mut hold_from = last.start;
    if text.ends_with("<!-") {
        hold_from = hold_from.min(text.len() - 3);
    } else if text.ends_with("<!") {
        hold_from = hold_from.min(text.len() - 2);
    } else if text.ends_with("..") {
        hold_from = hold_from.min(text.len() - 2);
    }
    if text[last.start..last.end].chars().all(char::is_whitespace) {
        let prev = spans.iter().rev().find(|span| {
            span.end <= last.start && !text[span.start..span.end].chars().all(char::is_whitespace)
        });
        hold_from = prev.map_or(0, |span| hold_from.min(span.start));
    }
    if text[hold_from..].trim_start().starts_with('/') {
        let slash = hold_from + text[hold_from..].len() - text[hold_from..].trim_start().len();
        let prev = spans.iter().rev().find(|span| {
            span.end <= slash && !text[span.start..span.end].chars().all(char::is_whitespace)
        });
        hold_from = prev.map_or(0, |span| hold_from.min(span.start));
    }
    if text[hold_from..].trim_start().starts_with('<')
        && !text[hold_from..].trim_start().starts_with("<!")
    {
        let lt = hold_from + text[hold_from..].len() - text[hold_from..].trim_start().len();
        let prev = spans.iter().rev().find(|span| {
            span.end <= lt && !text[span.start..span.end].chars().all(char::is_whitespace)
        });
        if let Some(prev) = prev {
            if is_javascript_value_token(text, prev) {
                hold_from = hold_from.min(prev.start);
            }
        }
    }
    hold_from
}
