//! Inventory language coverage and implement shared language test dispatch (FCB-022.A).
//!
//! Provides the canonical inventory of the 20 required language routes, bounded
//! registration seam, duplicate/unknown query rejection, explicit tracking of
//! missing language qualifications, and capability dispatch over the real
//! FCB-021 [`crate::resume::ResumableLexer`] engine.

use crate::highlight::Span;
use crate::resume::{ResumeError, ResumableLexer};

/// Maximum number of language registrations supported by the bounded registry.
pub const MAX_REGISTERED_LANGUAGES: usize = 64;

/// Maximum number of aliases permitted per language route registration.
pub const MAX_ALIASES_PER_ROUTE: usize = 16;

/// Canonical identifiers for the 20 required language routes (Plan §11.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LanguageId {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Jsx,
    Tsx,
    C,
    Cpp,
    CSharp,
    Go,
    Java,
    Swift,
    Shell,
    Json,
    Toml,
    Yaml,
    Sql,
    Html,
    Css,
    Markdown,
}

impl LanguageId {
    /// All 20 required language routes in canonical order.
    pub const ALL: [Self; 20] = [
        Self::Rust,
        Self::Python,
        Self::JavaScript,
        Self::TypeScript,
        Self::Jsx,
        Self::Tsx,
        Self::C,
        Self::Cpp,
        Self::CSharp,
        Self::Go,
        Self::Java,
        Self::Swift,
        Self::Shell,
        Self::Json,
        Self::Toml,
        Self::Yaml,
        Self::Sql,
        Self::Html,
        Self::Css,
        Self::Markdown,
    ];

    /// Canonical lowercase identifier for the language.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Jsx => "jsx",
            Self::Tsx => "tsx",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Go => "go",
            Self::Java => "java",
            Self::Swift => "swift",
            Self::Shell => "shell",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
            Self::Sql => "sql",
            Self::Html => "html",
            Self::Css => "css",
            Self::Markdown => "markdown",
        }
    }

    /// Standard known aliases and file extensions for this language.
    #[must_use]
    pub const fn standard_aliases(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rust", "rs"],
            Self::Python => &["python", "py"],
            Self::JavaScript => &["javascript", "js", "mjs", "cjs"],
            Self::TypeScript => &["typescript", "ts"],
            Self::Jsx => &["jsx"],
            Self::Tsx => &["tsx"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "c++", "cc", "hpp"],
            Self::CSharp => &["csharp", "cs", "c#"],
            Self::Go => &["go", "golang"],
            Self::Java => &["java"],
            Self::Swift => &["swift"],
            Self::Shell => &["shell", "bash", "sh", "zsh"],
            Self::Json => &["json", "jsonc"],
            Self::Toml => &["toml"],
            Self::Yaml => &["yaml", "yml"],
            Self::Sql => &["sql"],
            Self::Html => &["html", "htm", "xhtml", "xml", "svg"],
            Self::Css => &["css", "scss", "sass"],
            Self::Markdown => &["markdown", "md", "mdown", "mkd"],
        }
    }

    /// Match a query string against canonical names and standard aliases.
    #[must_use]
    pub fn from_query(query: &str) -> Option<Self> {
        let trimmed = query.trim();
        for &id in &Self::ALL {
            if id.canonical_name().eq_ignore_ascii_case(trimmed) {
                return Some(id);
            }
            for &alias in id.standard_aliases() {
                if alias.eq_ignore_ascii_case(trimmed) {
                    return Some(id);
                }
            }
        }
        None
    }
}

/// Truthful implementation and qualification status of a language route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QualificationStatus {
    /// Fully qualified incremental lexer implementation available with verified
    /// chunked coalesced equivalence.
    Implemented,
    /// Baseline or provisional support available (e.g. shared generic keyword
    /// pass or tag lexer, awaiting specialized adversarial split verification).
    Provisional,
    /// Missing or not yet implemented. Kept strictly explicit.
    Missing,
    /// Plain fallback (unsupported / plain text).
    Plain,
}

impl QualificationStatus {
    /// Machine-readable code for the status.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Implemented => "IMPLEMENTED",
            Self::Provisional => "PROVISIONAL",
            Self::Missing => "MISSING",
            Self::Plain => "PLAIN",
        }
    }

    /// Whether this status provides callable lexical dispatch.
    #[must_use]
    pub const fn is_dispatchable(self) -> bool {
        matches!(self, Self::Implemented | Self::Provisional)
    }
}

/// Registration record for one language route within the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageRouteRegistration {
    /// Canonical language ID.
    pub language_id: LanguageId,
    /// Current truthful qualification status.
    pub status: QualificationStatus,
    /// Primary alias used when creating engine instances.
    pub primary_alias: &'static str,
    /// Additional recognized aliases.
    pub aliases: &'static [&'static str],
    /// Human-readable description or owner note.
    pub description: &'static str,
}

/// Errors produced during language registration or dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchError {
    /// Query does not match any registered language name or alias.
    UnknownLanguage { query: String },
    /// Attempted to register a canonical language ID that is already registered.
    DuplicateLanguage(LanguageId),
    /// Attempted to register an alias that collides with an already registered route.
    DuplicateAlias {
        alias: String,
        existing_lang: LanguageId,
    },
    /// Alias count exceeded the per-route maximum limit.
    TooManyAliases { count: usize, max: usize },
    /// Registry capacity exceeded.
    RegistryFull { max: usize },
    /// Requested language route is explicitly marked as missing qualification.
    LanguageNotQualified {
        language_id: LanguageId,
        status: QualificationStatus,
    },
    /// Input size exceeded the provided work budget.
    BudgetExceeded { requested: usize, budget: usize },
    /// Upstream resumable lexer failure.
    LexerError(ResumeError),
}

impl DispatchError {
    /// Stable machine-readable error code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownLanguage { .. } => "UNKNOWN_LANGUAGE",
            Self::DuplicateLanguage(_) => "DUPLICATE_LANGUAGE",
            Self::DuplicateAlias { .. } => "DUPLICATE_ALIAS",
            Self::TooManyAliases { .. } => "TOO_MANY_ALIASES",
            Self::RegistryFull { .. } => "REGISTRY_FULL",
            Self::LanguageNotQualified { .. } => "LANGUAGE_NOT_QUALIFIED",
            Self::BudgetExceeded { .. } => "BUDGET_EXCEEDED",
            Self::LexerError(_) => "LEXER_ERROR",
        }
    }
}

/// Request for lexical dispatch over the real FCB-021 engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchRequest<'a> {
    /// Language name or alias to query.
    pub language_query: &'a str,
    /// Source code bytes to lex.
    pub code: &'a [u8],
    /// Maximum permitted byte budget for this dispatch.
    pub work_budget_bytes: usize,
    /// Optional source revision anchor.
    pub source_revision: Option<u64>,
}

/// Result of a successful language dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchResult {
    /// Resolved canonical language ID.
    pub language_id: LanguageId,
    /// Qualification status at dispatch time.
    pub status: QualificationStatus,
    /// Classified spans tiling the input.
    pub spans: Vec<Span>,
    /// Total bytes processed.
    pub bytes_processed: usize,
    /// Pending suffix bytes remaining (0 at completion).
    pub pending_suffix_bytes: usize,
    /// Whether the lexer was completed / finished.
    pub is_finished: bool,
    /// Preserved source revision anchor.
    pub source_revision: Option<u64>,
}

/// Bounded registry of language routes and capability dispatch seam.
#[derive(Clone, Debug)]
pub struct LanguageRegistry {
    routes: Vec<LanguageRouteRegistration>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageRegistry {
    /// Create an empty bounded language registry.
    #[must_use]
    pub const fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Construct the authoritative initial inventory of all 20 required language routes.
    ///
    /// Truthfully reflects the initial baseline:
    /// - Implemented: generic lexer languages with verified chunked coalesced equivalence.
    /// - Provisional: JSX/TSX (using JS/TS keyword baseline awaiting specialized state machine)
    ///   and Markdown (using fence machine awaiting incremental adversarial splits).
    /// - Implemented: C#, Java, Swift, HTML, CSS (incremental engines landed via fcb-9vx.12, 14, 15, 21, 22).
    /// - Missing: 0 (all 20 standard languages have qualified or provisional routes).
    #[must_use]
    pub fn standard_20_inventory() -> Self {
        let mut reg = Self::new();

        let entries: [LanguageRouteRegistration; 20] = [
            LanguageRouteRegistration {
                language_id: LanguageId::Rust,
                status: QualificationStatus::Implemented,
                primary_alias: "rust",
                aliases: LanguageId::Rust.standard_aliases(),
                description: "Rust incremental lexical engine with raw strings and block comments",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Python,
                status: QualificationStatus::Implemented,
                primary_alias: "python",
                aliases: LanguageId::Python.standard_aliases(),
                description: "Python incremental lexical engine with comments and quotes",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::JavaScript,
                status: QualificationStatus::Implemented,
                primary_alias: "javascript",
                aliases: LanguageId::JavaScript.standard_aliases(),
                description: "JavaScript incremental lexical engine with template literals",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::TypeScript,
                status: QualificationStatus::Implemented,
                primary_alias: "typescript",
                aliases: LanguageId::TypeScript.standard_aliases(),
                description: "TypeScript incremental lexical engine with type keywords",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Jsx,
                status: QualificationStatus::Provisional,
                primary_alias: "jsx",
                aliases: LanguageId::Jsx.standard_aliases(),
                description: "JSX baseline dispatch (provisional; dedicated tag-state in fcb-9vx.8)",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Tsx,
                status: QualificationStatus::Provisional,
                primary_alias: "tsx",
                aliases: LanguageId::Tsx.standard_aliases(),
                description: "TSX baseline dispatch (provisional; dedicated tag-state in fcb-9vx.9)",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::C,
                status: QualificationStatus::Implemented,
                primary_alias: "c",
                aliases: LanguageId::C.standard_aliases(),
                description: "C incremental lexical engine with preprocessor directives",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Cpp,
                status: QualificationStatus::Implemented,
                primary_alias: "cpp",
                aliases: LanguageId::Cpp.standard_aliases(),
                description: "C++ incremental lexical engine with preprocessor directives",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::CSharp,
                status: QualificationStatus::Implemented,
                primary_alias: "csharp",
                aliases: LanguageId::CSharp.standard_aliases(),
                description: "C# incremental lexical engine with comments, preprocessor directives, verbatim/interpolated/raw strings and adversarial split verification",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Go,
                status: QualificationStatus::Implemented,
                primary_alias: "go",
                aliases: LanguageId::Go.standard_aliases(),
                description: "Go incremental lexical engine with raw backtick literals",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Java,
                status: QualificationStatus::Implemented,
                primary_alias: "java",
                aliases: LanguageId::Java.standard_aliases(),
                description: "Java incremental lexical engine with comments, text blocks, annotations and adversarial split verification",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Swift,
                status: QualificationStatus::Implemented,
                primary_alias: "swift",
                aliases: LanguageId::Swift.standard_aliases(),
                description: "Swift incremental lexical engine with nested comments, raw/multiline strings, attributes and adversarial split verification",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Shell,
                status: QualificationStatus::Implemented,
                primary_alias: "bash",
                aliases: LanguageId::Shell.standard_aliases(),
                description: "Shell incremental lexical engine (Bash, sh, zsh)",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Json,
                status: QualificationStatus::Implemented,
                primary_alias: "json",
                aliases: LanguageId::Json.standard_aliases(),
                description: "JSON incremental lexical engine with string boundaries",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Toml,
                status: QualificationStatus::Implemented,
                primary_alias: "toml",
                aliases: LanguageId::Toml.standard_aliases(),
                description: "TOML incremental lexical engine with comments and strings",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Yaml,
                status: QualificationStatus::Implemented,
                primary_alias: "yaml",
                aliases: LanguageId::Yaml.standard_aliases(),
                description: "YAML incremental lexical engine with comments and scalars",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Sql,
                status: QualificationStatus::Implemented,
                primary_alias: "sql",
                aliases: LanguageId::Sql.standard_aliases(),
                description: "SQL incremental lexical engine with dash-dash comments",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Html,
                status: QualificationStatus::Implemented,
                primary_alias: "html",
                aliases: LanguageId::Html.standard_aliases(),
                description: "HTML incremental lexer with doctype, comments, CDATA, tags, and inert script/style",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Css,
                status: QualificationStatus::Implemented,
                primary_alias: "css",
                aliases: LanguageId::Css.standard_aliases(),
                description: "CSS incremental lexical engine with selectors, declarations, at-rules, strings, urls and comments",
            },
            LanguageRouteRegistration {
                language_id: LanguageId::Markdown,
                status: QualificationStatus::Provisional,
                primary_alias: "markdown",
                aliases: LanguageId::Markdown.standard_aliases(),
                description: "Markdown fence lexer (provisional; incremental splits in fcb-9vx.23)",
            },
        ];

        for entry in entries {
            let _ = reg.register(entry);
        }

        reg
    }

    /// Number of registered language routes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Whether the registry contains no routes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// Authoritative list of all registered routes.
    #[must_use]
    pub fn inventory(&self) -> &[LanguageRouteRegistration] {
        &self.routes
    }

    /// Register a new language route into the bounded registry.
    ///
    /// Validates capacity limits, duplicate canonical IDs, duplicate aliases,
    /// and alias length bounds.
    pub fn register(&mut self, route: LanguageRouteRegistration) -> Result<(), DispatchError> {
        if self.routes.len() >= MAX_REGISTERED_LANGUAGES {
            return Err(DispatchError::RegistryFull {
                max: MAX_REGISTERED_LANGUAGES,
            });
        }

        if route.aliases.len() > MAX_ALIASES_PER_ROUTE {
            return Err(DispatchError::TooManyAliases {
                count: route.aliases.len(),
                max: MAX_ALIASES_PER_ROUTE,
            });
        }

        // Check duplicate canonical ID.
        for existing in &self.routes {
            if existing.language_id == route.language_id {
                return Err(DispatchError::DuplicateLanguage(route.language_id));
            }
        }

        // Check duplicate alias across all registered routes.
        for alias in route.aliases {
            let trimmed = alias.trim();
            for existing in &self.routes {
                if existing.primary_alias.eq_ignore_ascii_case(trimmed) {
                    return Err(DispatchError::DuplicateAlias {
                        alias: (*alias).to_string(),
                        existing_lang: existing.language_id,
                    });
                }
                for existing_alias in existing.aliases {
                    if existing_alias.trim().eq_ignore_ascii_case(trimmed) {
                        return Err(DispatchError::DuplicateAlias {
                            alias: (*alias).to_string(),
                            existing_lang: existing.language_id,
                        });
                    }
                }
            }
        }

        self.routes.push(route);
        Ok(())
    }

    /// Look up a registration by name or alias.
    pub fn lookup(&self, query: &str) -> Result<&LanguageRouteRegistration, DispatchError> {
        let trimmed = query.trim();
        for route in &self.routes {
            if route.language_id.canonical_name().eq_ignore_ascii_case(trimmed)
                || route.primary_alias.eq_ignore_ascii_case(trimmed)
            {
                return Ok(route);
            }
            for alias in route.aliases {
                if alias.trim().eq_ignore_ascii_case(trimmed) {
                    return Ok(route);
                }
            }
        }
        Err(DispatchError::UnknownLanguage {
            query: query.to_string(),
        })
    }

    /// Returns all canonical language routes whose status is explicitly `Missing`.
    #[must_use]
    pub fn missing_routes(&self) -> Vec<LanguageId> {
        self.routes
            .iter()
            .filter(|r| r.status == QualificationStatus::Missing)
            .map(|r| r.language_id)
            .collect()
    }

    /// Returns all canonical language routes whose status is `Implemented`.
    #[must_use]
    pub fn implemented_routes(&self) -> Vec<LanguageId> {
        self.routes
            .iter()
            .filter(|r| r.status == QualificationStatus::Implemented)
            .map(|r| r.language_id)
            .collect()
    }

    /// Returns all canonical language routes whose status is `Provisional`.
    #[must_use]
    pub fn provisional_routes(&self) -> Vec<LanguageId> {
        self.routes
            .iter()
            .filter(|r| r.status == QualificationStatus::Provisional)
            .map(|r| r.language_id)
            .collect()
    }

    /// Dispatch input code over the real FCB-021 resumable lexical engine.
    ///
    /// Validates language support, refuses unqualified languages with typed
    /// refusal, enforces work budgets, and produces classified spans tiling the source.
    pub fn dispatch(&self, req: DispatchRequest<'_>) -> Result<DispatchResult, DispatchError> {
        let route = self.lookup(req.language_query)?;

        if route.status == QualificationStatus::Missing {
            return Err(DispatchError::LanguageNotQualified {
                language_id: route.language_id,
                status: route.status,
            });
        }

        if req.code.len() > req.work_budget_bytes {
            return Err(DispatchError::BudgetExceeded {
                requested: req.code.len(),
                budget: req.work_budget_bytes,
            });
        }

        let mut lexer = ResumableLexer::with_limits(route.primary_alias, req.work_budget_bytes)
            .map_err(DispatchError::LexerError)?;

        lexer.feed(req.code).map_err(DispatchError::LexerError)?;
        lexer.finish().map_err(DispatchError::LexerError)?;

        let spans = lexer.spans().to_vec();
        let bytes_processed = req.code.len();

        Ok(DispatchResult {
            language_id: route.language_id,
            status: route.status,
            spans,
            bytes_processed,
            pending_suffix_bytes: 0,
            is_finished: true,
            source_revision: req.source_revision,
        })
    }
}

/// Resource counters for bounded test receipts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceCounters {
    /// Bytes fed into the engine.
    pub bytes_scanned: usize,
    /// Spans produced by the engine.
    pub spans_emitted: usize,
    /// Bounded capacity ceiling.
    pub capacity_limit: usize,
}

/// Bounded test receipt for deterministic audit trails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchAuditReceipt {
    /// Deterministic test seed.
    pub seed: u64,
    /// FNV-1a digest of input bytes.
    pub input_digest: u64,
    /// FNV-1a digest of output span classes and ranges.
    pub output_digest: u64,
    /// Expected span count.
    pub expected_spans: usize,
    /// Actual span count.
    pub actual_spans: usize,
    /// Bounded resource counters.
    pub counters: ResourceCounters,
    /// Replayable test command string.
    pub replay_command: String,
}

/// Compute 64-bit FNV-1a hash over bytes.
#[must_use]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Compute FNV-1a hash over classified spans.
#[must_use]
pub fn digest_spans(spans: &[Span]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for span in spans {
        let tag = match span.kind {
            crate::highlight::Tok::Plain => 0u8,
            crate::highlight::Tok::Keyword => 1u8,
            crate::highlight::Tok::Type => 2u8,
            crate::highlight::Tok::Func => 3u8,
            crate::highlight::Tok::Str => 4u8,
            crate::highlight::Tok::Number => 5u8,
            crate::highlight::Tok::Comment => 6u8,
            crate::highlight::Tok::Operator => 7u8,
            crate::highlight::Tok::Punct => 8u8,
        };
        hash ^= u64::from(tag);
        hash = hash.wrapping_mul(FNV_PRIME);

        for b in span.start.to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for b in span.end.to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn inventory_contains_all_20_required_languages() {
        let registry = LanguageRegistry::standard_20_inventory();
        assert_eq!(registry.len(), 20);

        for lang in &LanguageId::ALL {
            let route = registry.lookup(lang.canonical_name()).expect("registered");
            assert_eq!(route.language_id, *lang);
        }
    }

    #[test]
    fn duplicate_registration_rejected() {
        let mut registry = LanguageRegistry::new();
        let r1 = LanguageRouteRegistration {
            language_id: LanguageId::Rust,
            status: QualificationStatus::Implemented,
            primary_alias: "rust",
            aliases: &["rs"],
            description: "Rust",
        };
        assert!(registry.register(r1.clone()).is_ok());

        let err = registry.register(r1).unwrap_err();
        assert_eq!(err, DispatchError::DuplicateLanguage(LanguageId::Rust));
        assert_eq!(err.code(), "DUPLICATE_LANGUAGE");
    }

    #[test]
    fn duplicate_alias_rejected() {
        let mut registry = LanguageRegistry::new();
        let r1 = LanguageRouteRegistration {
            language_id: LanguageId::Rust,
            status: QualificationStatus::Implemented,
            primary_alias: "rust",
            aliases: &["rs"],
            description: "Rust",
        };
        registry.register(r1).unwrap();

        let r2 = LanguageRouteRegistration {
            language_id: LanguageId::Python,
            status: QualificationStatus::Implemented,
            primary_alias: "python",
            aliases: &["rs"], // Duplicate alias!
            description: "Python",
        };
        let err = registry.register(r2).unwrap_err();
        assert_eq!(
            err,
            DispatchError::DuplicateAlias {
                alias: "rs".to_string(),
                existing_lang: LanguageId::Rust,
            }
        );
        assert_eq!(err.code(), "DUPLICATE_ALIAS");
    }

    #[test]
    fn unknown_language_query_rejected() {
        let registry = LanguageRegistry::standard_20_inventory();
        let err = registry.lookup("fortran").unwrap_err();
        assert_eq!(err.code(), "UNKNOWN_LANGUAGE");
    }

    #[test]
    fn missing_qualification_refused_on_dispatch() {
        let mut registry = LanguageRegistry::new();
        registry
            .register(LanguageRouteRegistration {
                language_id: LanguageId::CSharp,
                status: QualificationStatus::Missing,
                primary_alias: "csharp",
                aliases: LanguageId::CSharp.standard_aliases(),
                description: "Missing qualification test route",
            })
            .unwrap();
        let req = DispatchRequest {
            language_query: "csharp",
            code: b"class Program {}",
            work_budget_bytes: 1024,
            source_revision: Some(42),
        };
        let err = registry.dispatch(req).unwrap_err();
        assert_eq!(
            err,
            DispatchError::LanguageNotQualified {
                language_id: LanguageId::CSharp,
                status: QualificationStatus::Missing,
            }
        );
        assert_eq!(err.code(), "LANGUAGE_NOT_QUALIFIED");
    }

    #[test]
    fn real_engine_dispatch_for_implemented_language() {
        let registry = LanguageRegistry::standard_20_inventory();
        let code = b"fn main() {\n    let x = 42;\n}\n";
        let req = DispatchRequest {
            language_query: "rust",
            code,
            work_budget_bytes: 1024,
            source_revision: Some(101),
        };
        let res = registry.dispatch(req).expect("dispatch succeeds");
        assert_eq!(res.language_id, LanguageId::Rust);
        assert_eq!(res.status, QualificationStatus::Implemented);
        assert_eq!(res.bytes_processed, code.len());
        assert_eq!(res.source_revision, Some(101));
        assert!(!res.spans.is_empty());
    }

    #[test]
    fn budget_exceeded_refused() {
        let registry = LanguageRegistry::standard_20_inventory();
        let code = b"fn main() {}";
        let req = DispatchRequest {
            language_query: "rust",
            code,
            work_budget_bytes: 4, // Too small!
            source_revision: None,
        };
        let err = registry.dispatch(req).unwrap_err();
        assert_eq!(
            err,
            DispatchError::BudgetExceeded {
                requested: code.len(),
                budget: 4,
            }
        );
        assert_eq!(err.code(), "BUDGET_EXCEEDED");
    }
}
