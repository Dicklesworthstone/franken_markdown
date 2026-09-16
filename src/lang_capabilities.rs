//! Truthful per-language lexical capability matrix and publication (FCB-022.B).
//!
//! Enforces the capability ladder (Plan §11.5), separates source-exact spans
//! from provisional lexical choices and compiler semantics, manages conservative
//! classifications for ambiguous syntactic constructs, and guarantees that unknown
//! languages degrade cleanly to plain text without refusing to open files (§11.4).

use crate::highlight::{Span, Tok};
use crate::lang_dispatch::{
    DispatchRequest, DispatchResult, LanguageId, LanguageRegistry, QualificationStatus,
};

/// The semantic capability ladder (Plan §11.5).
///
/// A highlighter is not a compiler. Every source presentation surface must
/// operate strictly at or below its qualified tier on this ladder:
/// - `Bytes`: Exact source representation (copy, line navigation, byte offsets).
/// - `Lexical`: Qualified token classification (comments, strings, keywords).
/// - `Structural`: Parser-supported syntax entities (outlines, heading trees).
/// - `ResolvedLocal`: Proven relationships within the parser's modeled scope (unique import link).
/// - `ExternalSemantic`: Compiler/LSP facts with provider and configuration.
/// - `Heuristic`: Unqualified candidate links or suggestions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapabilityLevel {
    /// Exact source representation. Line navigation, range copy, byte offsets.
    Bytes = 0,
    /// Qualified token classification. Comment/string boundaries, keyword coloring.
    Lexical = 1,
    /// Parser-supported syntax entities (e.g. item outline, heading tree).
    Structural = 2,
    /// Proven relationship within the parser's modeled scope (e.g. uniquely resolved import).
    ResolvedLocal = 3,
    /// External compiler/LSP facts with provider and project configuration.
    ExternalSemantic = 4,
    /// Candidate-only heuristics (e.g. same-name identifier match).
    Heuristic = 5,
}

impl CapabilityLevel {
    /// Canonical display name for this capability level.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bytes => "Bytes",
            Self::Lexical => "Lexical",
            Self::Structural => "Structural",
            Self::ResolvedLocal => "ResolvedLocal",
            Self::ExternalSemantic => "ExternalSemantic",
            Self::Heuristic => "Heuristic",
        }
    }

    /// The permitted claim string defined by Plan §11.5.
    #[must_use]
    pub const fn permitted_claim(self) -> &'static str {
        match self {
            Self::Bytes => "Exact source representation: range copy, exact literal match, line navigation.",
            Self::Lexical => "Qualified token classification: comment/string boundaries, keyword coloring.",
            Self::Structural => "Parser-supported syntax entities: Rust item outline, Markdown heading tree.",
            Self::ResolvedLocal => "A proven relationship within the parser's modeled scope.",
            Self::ExternalSemantic => "Facts supplied by an explicitly enabled, independently qualified provider.",
            Self::Heuristic => "A candidate only: same-name identifier link, approximate related-file suggestion.",
        }
    }

    /// Whether this capability level is strictly lexical or byte-level.
    #[must_use]
    pub const fn is_lexical_or_below(self) -> bool {
        matches!(self, Self::Bytes | Self::Lexical)
    }

    /// Whether this capability level permits claiming semantic or compiler knowledge.
    #[must_use]
    pub const fn allows_compiler_claim(self) -> bool {
        matches!(self, Self::ResolvedLocal | Self::ExternalSemantic)
    }
}

/// A truthful, versioned capability declaration row for a single language route (Plan §11.4, §11.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageCapabilityRow {
    /// Format version of the capability descriptor.
    pub version: u32,
    /// Canonical language ID, or `None` for plain fallback.
    pub language_id: Option<LanguageId>,
    /// Canonical lowercase language name or "plain".
    pub canonical_name: &'static str,
    /// Qualification status of this language route.
    pub status: QualificationStatus,
    /// Maximum permitted capability level for this highlighting route (strictly `Lexical` or `Bytes`).
    pub max_level: CapabilityLevel,
    /// Whether classified spans guarantee exact source byte tiling without additions,
    /// deletions, or reorderings.
    pub source_exact_spans: bool,
    /// Whether chunk-safe incremental classification is supported and verified.
    pub incremental: bool,
    /// Whether chunked coalesced equivalence has been verified across split corpora.
    pub coalesced_equivalence: bool,
    /// Whether any classification choices are provisional or heuristic.
    pub provisional_choices: bool,
    /// Whether ambiguous constructs (e.g. `<` generic vs tag, `/` regex vs division) are handled
    /// conservatively rather than guessing compiler semantics.
    pub conservative_ambiguities: bool,
    /// Multiline constructs truthfully handled by the incremental state machine.
    pub multiline_constructs: &'static [&'static str],
    /// Human-readable qualification notes.
    pub notes: &'static str,
}

impl LanguageCapabilityRow {
    /// Format version constant for capability rows.
    pub const CURRENT_VERSION: u32 = 1;

    /// Returns `true` if this capability row forbids any compiler-semantic claim.
    #[must_use]
    pub fn verifies_no_compiler_claim(&self) -> bool {
        !self.max_level.allows_compiler_claim() && self.max_level.is_lexical_or_below()
    }
}

/// Failure during capability audit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityAuditError {
    /// A highlighter route illegally claimed compiler or external semantic capabilities.
    CompilerClaimForbidden {
        language: &'static str,
        claimed_level: CapabilityLevel,
    },
    /// A qualified route failed to guarantee source-exact span tiling.
    SourceExactSpanViolation {
        language: &'static str,
    },
    /// Invalid span tiling detected during span validation.
    SpanTilingError {
        language: &'static str,
        reason: String,
    },
}

impl std::fmt::Display for CapabilityAuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CompilerClaimForbidden {
                language,
                claimed_level,
            } => write!(
                f,
                "highlighter for '{language}' claimed forbidden level '{:?}' (a highlighter is not a compiler)",
                claimed_level
            ),
            Self::SourceExactSpanViolation { language } => write!(
                f,
                "route for '{language}' does not guarantee source-exact spans"
            ),
            Self::SpanTilingError { language, reason } => {
                write!(f, "span tiling error for '{language}': {reason}")
            }
        }
    }
}

impl std::error::Error for CapabilityAuditError {}

/// Authoritative release capability matrix for all supported language routes (Plan §11.4).
#[derive(Clone, Debug)]
pub struct CapabilityMatrix {
    rows: Vec<LanguageCapabilityRow>,
    plain: LanguageCapabilityRow,
}

impl Default for CapabilityMatrix {
    fn default() -> Self {
        Self::release_20()
    }
}

impl CapabilityMatrix {
    /// Construct the authoritative release capability matrix reflecting all 20 required
    /// languages plus the plain unknown-language fallback route.
    #[must_use]
    pub fn release_20() -> Self {
        let plain = LanguageCapabilityRow {
            version: LanguageCapabilityRow::CURRENT_VERSION,
            language_id: None,
            canonical_name: "plain",
            status: QualificationStatus::Plain,
            max_level: CapabilityLevel::Bytes,
            source_exact_spans: true,
            incremental: true,
            coalesced_equivalence: true,
            provisional_choices: false,
            conservative_ambiguities: false,
            multiline_constructs: &[],
            notes: "Plain source fallback for unknown or uncolored files. Never refuses to open.",
        };

        let rows = vec![
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Rust),
                canonical_name: "rust",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["raw_strings", "nested_block_comments", "lifetimes"],
                notes: "Rust incremental lexer with raw string delimiter tracking, nested block comments, and split corpus qualification.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Python),
                canonical_name: "python",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["triple_quotes", "fstrings"],
                notes: "Python incremental lexer with triple-quoted strings, escapes, and numeric forms.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::JavaScript),
                canonical_name: "javascript",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: true,
                multiline_constructs: &["template_literals", "block_comments", "regex_vs_division"],
                notes: "JavaScript incremental lexer with template literal interpolation, conservative regex/division hold, and split corpus qualification.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::TypeScript),
                canonical_name: "typescript",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: true,
                multiline_constructs: &["template_literals", "block_comments", "type_annotations"],
                notes: "TypeScript incremental lexer with type keyword classification, template strings, and conservative ambiguity handling.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Jsx),
                canonical_name: "jsx",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: true,
                multiline_constructs: &["jsx_tags", "embedded_expressions", "fragments", "entities"],
                notes: "JSX incremental lexer with tag/text mode transitions, embedded JS expression containers, fragments, and adversarial split corpus qualification.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Tsx),
                canonical_name: "tsx",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: true,
                multiline_constructs: &["jsx_tags", "embedded_expressions", "fragments", "typescript_expressions", "spread_operators"],
                notes: "TSX incremental lexer with JSX+TypeScript composition, generic bracket vs tag disambiguation, spread holds, and split corpus qualification.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::C),
                canonical_name: "c",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["preprocessor_continuation", "block_comments"],
                notes: "C incremental lexer with preprocessor directives, escaped newlines, and block comments.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Cpp),
                canonical_name: "cpp",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["preprocessor_continuation", "block_comments", "raw_strings"],
                notes: "C++ incremental lexer with raw string literals, preprocessor continuation, and split corpus qualification.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::CSharp),
                canonical_name: "csharp",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["verbatim_strings", "interpolated_strings", "raw_strings", "preprocessor_continuation"],
                notes: "C# incremental lexer with verbatim/interpolated/raw string variants and preprocessor directives.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Go),
                canonical_name: "go",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["raw_backtick_literals", "block_comments"],
                notes: "Go incremental lexer with raw backtick string literals, runes, and comments.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Java),
                canonical_name: "java",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["text_blocks", "block_comments", "annotations"],
                notes: "Java incremental lexer with multiline text blocks, annotations, and split corpus qualification.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Swift),
                canonical_name: "swift",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["nested_block_comments", "raw_multiline_strings", "attributes"],
                notes: "Swift incremental lexer with nested block comments, multiline raw strings, and attributes.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Shell),
                canonical_name: "shell",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["heredocs", "multiline_quotes", "variable_prefixes"],
                notes: "Shell incremental lexer supporting Bash, sh, and zsh with heredoc delimiter tracking and variable assignment prefixes.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Json),
                canonical_name: "json",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["multiline_objects", "multiline_arrays"],
                notes: "JSON/JSONC incremental lexer with string escapes, unicode escapes, and literal keywords.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Toml),
                canonical_name: "toml",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["basic_and_literal_multiline_strings", "table_headers"],
                notes: "TOML incremental lexer with basic/literal multiline strings, structured keys, and comments.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Yaml),
                canonical_name: "yaml",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["block_scalars", "nested_indentation"],
                notes: "YAML incremental lexer with literal/folded block scalars, comments, and document markers.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Sql),
                canonical_name: "sql",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["multiline_comments", "multiline_string_literals"],
                notes: "SQL incremental lexer with standard ANSI SQL keywords, dash-dash comments, block comments, and literals.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Html),
                canonical_name: "html",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["tags", "comments", "cdata", "doctype", "inert_script_style"],
                notes: "HTML incremental lexer with doctype, comments, CDATA, tags, and inert script/style regions.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Css),
                canonical_name: "css",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["block_comments", "selectors", "at_rules", "declarations"],
                notes: "CSS incremental lexer with selectors, declarations, at-rules, url/string literals, and block comments.",
            },
            LanguageCapabilityRow {
                version: LanguageCapabilityRow::CURRENT_VERSION,
                language_id: Some(LanguageId::Markdown),
                canonical_name: "markdown",
                status: QualificationStatus::Implemented,
                max_level: CapabilityLevel::Lexical,
                source_exact_spans: true,
                incremental: true,
                coalesced_equivalence: true,
                provisional_choices: false,
                conservative_ambiguities: false,
                multiline_constructs: &["code_fences", "headings", "blockquotes", "html_comments"],
                notes: "Markdown incremental lexer with code fence info strings, headings, blockquotes, and inline code delimiters.",
            },
        ];

        Self { rows, plain }
    }

    /// Number of registered language capability rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the matrix has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The plain unknown-language fallback capability row.
    #[must_use]
    pub fn plain_fallback(&self) -> LanguageCapabilityRow {
        self.plain.clone()
    }

    /// Look up the capability row for a query string. Returns plain fallback if unknown.
    #[must_use]
    pub fn lookup_or_fallback(&self, query: &str) -> LanguageCapabilityRow {
        let trimmed = query.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("plain") {
            return self.plain.clone();
        }

        if let Some(id) = LanguageId::from_query(trimmed) {
            if let Some(row) = self.lookup_id(id) {
                return row;
            }
        }

        self.plain.clone()
    }

    /// Look up the capability row by canonical [`LanguageId`].
    #[must_use]
    pub fn lookup_id(&self, id: LanguageId) -> Option<LanguageCapabilityRow> {
        self.rows
            .iter()
            .find(|r| r.language_id == Some(id))
            .cloned()
    }

    /// Slice of all capability rows in canonical order.
    #[must_use]
    pub fn rows(&self) -> &[LanguageCapabilityRow] {
        &self.rows
    }

    /// Count of languages with `Implemented` qualification status.
    #[must_use]
    pub fn implemented_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.status == QualificationStatus::Implemented)
            .count()
    }

    /// Count of languages with `Provisional` qualification status.
    #[must_use]
    pub fn provisional_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.status == QualificationStatus::Provisional)
            .count()
    }

    /// Count of languages with `Missing` qualification status.
    #[must_use]
    pub fn missing_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.status == QualificationStatus::Missing)
            .count()
    }

    /// Audit invariant: verify that NO highlighter row in the matrix claims compiler
    /// or external semantic capability (Plan §11.5).
    pub fn audit_no_compiler_claims(&self) -> Result<(), CapabilityAuditError> {
        for row in &self.rows {
            if !row.verifies_no_compiler_claim() {
                return Err(CapabilityAuditError::CompilerClaimForbidden {
                    language: row.canonical_name,
                    claimed_level: row.max_level,
                });
            }
        }
        if !self.plain.verifies_no_compiler_claim() {
            return Err(CapabilityAuditError::CompilerClaimForbidden {
                language: self.plain.canonical_name,
                claimed_level: self.plain.max_level,
            });
        }
        Ok(())
    }

    /// Audit invariant: verify that ALL implemented routes guarantee source-exact spans (Plan §11.1, §11.7).
    pub fn audit_source_exact_spans(&self) -> Result<(), CapabilityAuditError> {
        for row in &self.rows {
            if row.status == QualificationStatus::Implemented && !row.source_exact_spans {
                return Err(CapabilityAuditError::SourceExactSpanViolation {
                    language: row.canonical_name,
                });
            }
        }
        if !self.plain.source_exact_spans {
            return Err(CapabilityAuditError::SourceExactSpanViolation {
                language: self.plain.canonical_name,
            });
        }
        Ok(())
    }
}

/// Validate that a set of spans exactly tiles the source buffer `[0..code_len]`
/// with zero gaps and zero overlaps.
///
/// Invariants verified:
/// 1. Every span must have `start < end`.
/// 2. `spans[0].start == 0` (if `code_len > 0`).
/// 3. For all consecutive spans `i` and `i+1`, `spans[i].end == spans[i+1].start`.
/// 4. `spans.last().end == code_len`.
/// 5. If `code_len == 0`, spans must be empty.
pub fn validate_span_tiling(code_len: usize, spans: &[Span]) -> Result<(), String> {
    if code_len == 0 {
        if spans.is_empty() {
            return Ok(());
        }
        return Err(format!(
            "empty input produced {} non-empty spans",
            spans.len()
        ));
    }

    if spans.is_empty() {
        return Err(format!(
            "input of length {code_len} produced 0 spans (expected complete tiling)"
        ));
    }

    if spans[0].start != 0 {
        return Err(format!(
            "first span starts at {} instead of 0",
            spans[0].start
        ));
    }

    for (i, span) in spans.iter().enumerate() {
        if span.start >= span.end {
            return Err(format!(
                "span {i} has invalid empty/inverted bounds: [{}..{}]",
                span.start, span.end
            ));
        }

        if i > 0 {
            let prev = &spans[i - 1];
            if prev.end != span.start {
                if prev.end < span.start {
                    return Err(format!(
                        "gap between span {} [{}..{}] and span {} [{}..{}] (missing bytes {}..{})",
                        i - 1,
                        prev.start,
                        prev.end,
                        i,
                        span.start,
                        span.end,
                        prev.end,
                        span.start
                    ));
                }
                return Err(format!(
                    "overlap between span {} [{}..{}] and span {} [{}..{}] (overlapping bytes {}..{})",
                    i - 1,
                    prev.start,
                    prev.end,
                    i,
                    span.start,
                    span.end,
                    span.start,
                    prev.end
                ));
            }
        }
    }

    let last_end = spans[spans.len() - 1].end;
    if last_end != code_len {
        return Err(format!(
            "final span ends at {last_end} instead of input length {code_len}"
        ));
    }

    Ok(())
}

/// Dispatch code over the registry, with graceful fallback to plain source representation
/// if the language is unknown, empty, or uncolored.
///
/// Guaranteed: "Unknown languages remain readable as plain source... The application
/// must not refuse to open a file because it cannot color it." (Plan §11.4)
pub fn dispatch_or_plain_fallback(
    registry: &LanguageRegistry,
    req: DispatchRequest<'_>,
) -> DispatchResult {
    // Attempt standard registered dispatch first
    if let Ok(result) = registry.dispatch(req) {
        return result;
    }

    // Degrade cleanly to source-exact single Plain span covering the input bytes
    let spans = if req.code.is_empty() {
        Vec::new()
    } else {
        vec![Span {
            kind: Tok::Plain,
            start: 0,
            end: req.code.len(),
        }]
    };

    DispatchResult {
        language_id: LanguageId::from_query(req.language_query).unwrap_or(LanguageId::Rust),
        status: QualificationStatus::Plain,
        spans,
        bytes_processed: req.code.len(),
        pending_suffix_bytes: 0,
        is_finished: true,
        source_revision: req.source_revision,
    }
}
