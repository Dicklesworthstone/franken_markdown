//! Version-one lexical checkpoints. The wire layout stays compatible with
//! existing checkpoints; validation rejects corrupt suffixes and impossible
//! EOF/source extents before a host resumes classification.

use crate::highlight::is_supported;

/// Lexer checkpoint format version.
pub const CHECKPOINT_VERSION: u32 = 1;
/// Magic identifier for serialized lexer checkpoints (`FMDL`).
pub const CHECKPOINT_MAGIC: [u8; 4] = *b"FMDL";
/// Maximum serialized checkpoint size (one kibibyte).
pub const MAX_CHECKPOINT_BYTES: usize = 1024;
/// Maximum comment nesting depth.
pub const MAX_COMMENT_DEPTH: u16 = 64;
/// Maximum string interpolation nesting depth.
pub const MAX_INTERPOLATION_DEPTH: u16 = 64;
/// Maximum language identifier length in bytes.
pub const MAX_LANG_LEN: usize = 64;
/// Maximum replay suffix length in a checkpoint.
pub const MAX_SUFFIX_BYTES: usize = 512;

/// Comment lexical state carried across checkpoint boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentState {
    /// Outside a comment.
    None,
    /// Inside a single-line comment.
    Line,
    /// Inside a block comment.
    Block {
        /// Nesting depth, bounded by [`MAX_COMMENT_DEPTH`].
        depth: u16,
    },
}

/// String/literal state carried across checkpoint boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringState {
    /// Outside a literal.
    None,
    /// Single-quoted literal.
    SingleQuote,
    /// Double-quoted literal.
    DoubleQuote,
    /// Template/backtick literal.
    Backtick,
    /// Raw string literal.
    RawString {
        /// Number of hash delimiters required to terminate the literal.
        hashes: u8,
    },
}

/// Errors validating or decoding a [`LexerCheckpoint`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckpointError {
    /// Incorrect magic header.
    InvalidMagic,
    /// Unsupported ABI version.
    UnsupportedVersion {
        /// Supplied version.
        found: u32,
        /// Supported version.
        expected: u32,
    },
    /// Payload or variable-length field exceeds its limit.
    PayloadTooLarge {
        /// Supplied byte count.
        bytes: usize,
        /// Allowed byte count.
        cap: usize,
    },
    /// Incomplete serialized payload.
    UnexpectedEof,
    /// Excessive nesting.
    DepthLimitExceeded {
        /// Supplied nesting depth.
        depth: u16,
        /// Allowed nesting depth.
        max: u16,
    },
    /// Unknown comment tag.
    InvalidCommentTag(u8),
    /// Unknown string tag.
    InvalidStringTag(u8),
    /// Unknown EOF flag.
    InvalidEofFlag(u8),
    /// Unsupported language.
    UnsupportedLanguage(String),
    /// Source revision does not match the caller's captured source.
    SourceCorrespondenceMismatch {
        /// Expected source revision.
        expected_revision: u64,
        /// Checkpoint source revision.
        found_revision: u64,
    },
    /// Resume offset does not match the caller's captured source.
    OffsetMismatch {
        /// Expected source offset.
        expected_offset: u64,
        /// Checkpoint source offset.
        found_offset: u64,
    },
    /// Checkpoint language differs from the expected language.
    LanguageMismatch {
        /// Expected language.
        expected: String,
        /// Checkpoint language.
        found: String,
    },
    /// Malformed UTF-8, or a truncated scalar at EOF. A non-EOF checkpoint
    /// may end partway through one otherwise well-formed UTF-8 scalar.
    InvalidUtf8,
    /// Trailing serialized bytes.
    TrailingBytes,
    /// A finished stream cannot retain bytes that were never emitted.
    InvalidEofState,
    /// The source extent cannot be represented by the wire or receiving host.
    OffsetOverflow {
        /// Checkpoint's emitted byte offset.
        byte_offset: u64,
    },
}

impl CheckpointError {
    /// Stable machine-readable error code.
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
            Self::InvalidEofState => "INVALID_EOF_STATE",
            Self::OffsetOverflow { .. } => "OFFSET_OVERFLOW",
        }
    }
}

/// Versioned, bounded restart state. The suffix is replayed from `byte_offset`;
/// it is not part of the already-emitted prefix. Revision/offset correspondence
/// is checked by the restoring host; this format is not an authentication tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexerCheckpoint {
    /// ABI version.
    pub version: u32,
    /// Language identifier.
    pub lang: String,
    /// Caller-supplied source revision.
    pub source_revision: u64,
    /// Start of the unresolved suffix, or the full source length at EOF.
    pub byte_offset: u64,
    /// Descriptive comment state.
    pub comment_state: CommentState,
    /// Descriptive string state.
    pub string_state: StringState,
    /// Interpolation nesting depth.
    pub interpolation_depth: u16,
    /// Whether the stream is finished.
    pub is_eof: bool,
    /// Unresolved source bytes, including any partial final UTF-8 scalar.
    pub unresolved_suffix: Vec<u8>,
}

impl LexerCheckpoint {
    /// Legacy unchecked serialization, retaining the version-one byte layout.
    /// Use [`Self::try_to_bytes`] when publishing a checkpoint: callers can
    /// construct this public model with fields that exceed the wire limits.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(36 + self.lang.len() + self.unresolved_suffix.len());
        bytes.extend_from_slice(&CHECKPOINT_MAGIC);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.source_revision.to_le_bytes());
        bytes.extend_from_slice(&self.byte_offset.to_le_bytes());
        let (tag, depth) = match self.comment_state {
            CommentState::None => (0, 0),
            CommentState::Line => (1, 0),
            CommentState::Block { depth } => (2, depth),
        };
        bytes.push(tag);
        bytes.extend_from_slice(&depth.to_le_bytes());
        let (tag, hashes) = match self.string_state {
            StringState::None => (0, 0),
            StringState::SingleQuote => (1, 0),
            StringState::DoubleQuote => (2, 0),
            StringState::Backtick => (3, 0),
            StringState::RawString { hashes } => (4, hashes),
        };
        bytes.push(tag);
        bytes.push(hashes);
        bytes.extend_from_slice(&self.interpolation_depth.to_le_bytes());
        bytes.push(u8::from(self.is_eof));
        bytes.extend_from_slice(&(self.lang.len() as u16).to_le_bytes());
        bytes.extend_from_slice(self.lang.as_bytes());
        bytes.extend_from_slice(&(self.unresolved_suffix.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&self.unresolved_suffix);
        bytes
    }

    /// Validate before allocating the serialized payload. On success the
    /// result is bounded, lossless, and accepted by [`Self::from_bytes`].
    pub fn try_to_bytes(&self) -> Result<Vec<u8>, CheckpointError> {
        self.validate()?;
        Ok(self.to_bytes())
    }

    /// Decode through a bounds-checked reader. Variable-length fields are
    /// bounded before copying; malformed/truncated payloads never index past
    /// the supplied bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CheckpointError> {
        bound(bytes.len(), MAX_CHECKPOINT_BYTES)?;
        if bytes.len() < 36 {
            return Err(CheckpointError::UnexpectedEof);
        }
        let mut reader = Reader { bytes, position: 0 };
        if reader.take(4)? != CHECKPOINT_MAGIC {
            return Err(CheckpointError::InvalidMagic);
        }
        let version = u32::from_le_bytes(reader.array()?);
        if version != CHECKPOINT_VERSION {
            return Err(CheckpointError::UnsupportedVersion { found: version, expected: CHECKPOINT_VERSION });
        }
        let source_revision = u64::from_le_bytes(reader.array()?);
        let byte_offset = u64::from_le_bytes(reader.array()?);
        let comment_tag = reader.byte()?;
        let depth = u16::from_le_bytes(reader.array()?);
        if depth > MAX_COMMENT_DEPTH {
            return Err(CheckpointError::DepthLimitExceeded { depth, max: MAX_COMMENT_DEPTH });
        }
        let comment_state = match comment_tag {
            0 => CommentState::None,
            1 => CommentState::Line,
            2 => CommentState::Block { depth },
            other => return Err(CheckpointError::InvalidCommentTag(other)),
        };
        let string_tag = reader.byte()?;
        let hashes = reader.byte()?;
        let string_state = match string_tag {
            0 => StringState::None,
            1 => StringState::SingleQuote,
            2 => StringState::DoubleQuote,
            3 => StringState::Backtick,
            4 => StringState::RawString { hashes },
            other => return Err(CheckpointError::InvalidStringTag(other)),
        };
        let interpolation_depth = u16::from_le_bytes(reader.array()?);
        let is_eof = match reader.byte()? {
            0 => false,
            1 => true,
            other => return Err(CheckpointError::InvalidEofFlag(other)),
        };
        let lang_length = usize::from(u16::from_le_bytes(reader.array()?));
        bound(lang_length, MAX_LANG_LEN)?;
        let lang = std::str::from_utf8(reader.take(lang_length)?)
            .map_err(|_| CheckpointError::InvalidUtf8)?.to_string();
        let suffix_length = usize::from(u16::from_le_bytes(reader.array()?));
        bound(suffix_length, MAX_SUFFIX_BYTES)?;
        let unresolved_suffix = reader.take(suffix_length)?.to_vec();
        if reader.position != bytes.len() {
            return Err(CheckpointError::TrailingBytes);
        }
        let checkpoint = Self {
            version, lang, source_revision, byte_offset, comment_state,
            string_state, interpolation_depth, is_eof, unresolved_suffix,
        };
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    /// Validate wire bounds, UTF-8 replayability, EOF consistency, and source
    /// extent. Host-width checks additionally happen during restoration.
    pub fn validate(&self) -> Result<(), CheckpointError> {
        if self.version != CHECKPOINT_VERSION {
            return Err(CheckpointError::UnsupportedVersion { found: self.version, expected: CHECKPOINT_VERSION });
        }
        bound(self.lang.len(), MAX_LANG_LEN)?;
        bound(self.unresolved_suffix.len(), MAX_SUFFIX_BYTES)?;
        if !is_supported(&self.lang) {
            return Err(CheckpointError::UnsupportedLanguage(self.lang.clone()));
        }
        if let CommentState::Block { depth } = self.comment_state {
            if depth > MAX_COMMENT_DEPTH {
                return Err(CheckpointError::DepthLimitExceeded { depth, max: MAX_COMMENT_DEPTH });
            }
        }
        if self.interpolation_depth > MAX_INTERPOLATION_DEPTH {
            return Err(CheckpointError::DepthLimitExceeded {
                depth: self.interpolation_depth, max: MAX_INTERPOLATION_DEPTH,
            });
        }
        if let Err(error) = std::str::from_utf8(&self.unresolved_suffix) {
            if self.is_eof || error.error_len().is_some() {
                return Err(CheckpointError::InvalidUtf8);
            }
        }
        if self.is_eof && (!self.unresolved_suffix.is_empty()
            || self.comment_state != CommentState::None
            || self.string_state != StringState::None
            || self.interpolation_depth != 0)
        {
            return Err(CheckpointError::InvalidEofState);
        }
        if self.byte_offset.checked_add(self.unresolved_suffix.len() as u64).is_none() {
            return Err(CheckpointError::OffsetOverflow { byte_offset: self.byte_offset });
        }
        Ok(())
    }
}

fn bound(bytes: usize, cap: usize) -> Result<(), CheckpointError> {
    if bytes > cap {
        Err(CheckpointError::PayloadTooLarge { bytes, cap })
    } else {
        Ok(())
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], CheckpointError> {
        let end = self.position.checked_add(length).ok_or(CheckpointError::UnexpectedEof)?;
        let value = self.bytes.get(self.position..end).ok_or(CheckpointError::UnexpectedEof)?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CheckpointError> {
        self.take(N)?.try_into().map_err(|_| CheckpointError::UnexpectedEof)
    }

    fn byte(&mut self) -> Result<u8, CheckpointError> {
        let [byte] = self.array()?;
        Ok(byte)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn checkpoint() -> LexerCheckpoint {
        LexerCheckpoint {
            version: CHECKPOINT_VERSION, lang: "rust".into(), source_revision: 17,
            byte_offset: 9, comment_state: CommentState::None, string_state: StringState::None,
            interpolation_depth: 0, is_eof: false, unresolved_suffix: b"let".to_vec(),
        }
    }

    #[test]
    fn version_one_layout_and_checked_roundtrip_are_preserved() {
        let checkpoint = checkpoint();
        let bytes = checkpoint.try_to_bytes().unwrap();
        assert_eq!(&bytes[..8], b"FMDL\x01\x00\x00\x00");
        assert_eq!(&bytes[32..38], b"\x04\x00rust");
        assert_eq!(bytes.len(), 36 + 4 + 3);
        assert_eq!(bytes, checkpoint.to_bytes());
        assert_eq!(LexerCheckpoint::from_bytes(&bytes).unwrap(), checkpoint);
    }

    #[test]
    fn every_truncated_wire_prefix_is_refused() {
        let bytes = checkpoint().try_to_bytes().unwrap();
        for end in 0..bytes.len() {
            assert!(LexerCheckpoint::from_bytes(&bytes[..end]).is_err(), "prefix {end}");
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(LexerCheckpoint::from_bytes(&trailing), Err(CheckpointError::TrailingBytes));
    }

    #[test]
    fn malformed_utf8_is_rejected_but_partial_scalars_can_be_resumed() {
        let mut checkpoint = checkpoint();
        for suffix in [vec![0xff], vec![0x80], vec![0xc0, 0xaf], vec![0xed, 0xa0]] {
            checkpoint.unresolved_suffix = suffix;
            assert_eq!(checkpoint.validate(), Err(CheckpointError::InvalidUtf8));
            assert_eq!(LexerCheckpoint::from_bytes(&checkpoint.to_bytes()), Err(CheckpointError::InvalidUtf8));
        }
        for suffix in [vec![0xc3], vec![0xf0, 0x9f], vec![0xf0, 0x9f, 0x98]] {
            checkpoint.unresolved_suffix = suffix;
            let bytes = checkpoint.try_to_bytes().unwrap();
            assert_eq!(LexerCheckpoint::from_bytes(&bytes).unwrap(), checkpoint);
        }
    }

    #[test]
    fn finished_checkpoints_cannot_discard_pending_source() {
        let mut checkpoint = checkpoint();
        checkpoint.is_eof = true;
        assert_eq!(checkpoint.validate(), Err(CheckpointError::InvalidEofState));
        checkpoint.unresolved_suffix.clear();
        assert!(checkpoint.validate().is_ok());
        checkpoint.comment_state = CommentState::Line;
        assert_eq!(checkpoint.validate(), Err(CheckpointError::InvalidEofState));
    }

    #[test]
    fn source_extent_overflow_is_refused() {
        let mut checkpoint = checkpoint();
        checkpoint.byte_offset = u64::MAX;
        assert_eq!(checkpoint.validate(), Err(CheckpointError::OffsetOverflow { byte_offset: u64::MAX }));
    }

    #[test]
    fn checked_encoder_rejects_oversized_fields_before_serialization() {
        let mut checkpoint = checkpoint();
        checkpoint.unresolved_suffix = vec![b'x'; MAX_SUFFIX_BYTES + 1];
        assert_eq!(checkpoint.try_to_bytes(), Err(CheckpointError::PayloadTooLarge {
            bytes: MAX_SUFFIX_BYTES + 1, cap: MAX_SUFFIX_BYTES,
        }));
        checkpoint.unresolved_suffix.clear();
        checkpoint.lang = "r".repeat(MAX_LANG_LEN + 1);
        assert_eq!(checkpoint.try_to_bytes(), Err(CheckpointError::PayloadTooLarge {
            bytes: MAX_LANG_LEN + 1, cap: MAX_LANG_LEN,
        }));
    }
}
