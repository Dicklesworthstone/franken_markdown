//! TSX lexical classification (FCB-022, fcb-9vx.9).
//!
//! TSX is TypeScript plus JSX element syntax. This module composes the
//! qualified [`crate::lang_jsx::lex_jsx_composed_into`] state machine with
//! the TypeScript keyword and type tables so that TypeScript declarations
//! and type contexts classify correctly everywhere JSX is not active —
//! including inside embedded `{ ... }` expression containers, which the
//! JSX state machine routes through the composed JavaScript machinery.
//!
//! Consumes the sibling engines per the FCB-022 composition contract; the
//! JSX tag/attribute/children state machine and the JavaScript expression
//! machinery are consumed, not reimplemented.
//!
//! Capability row: [`TSX_CAPABILITY_V1`] — versioned, truthful: JSX
//! transitions, embedded expressions, fragments, entities, plus
//! TypeScript keyword/type classification in expression contexts.
//! Conservative unresolved contexts (generic-vs-JSX ambiguity) classify
//! as the JSX reading; nothing becomes compiler-proven.

use crate::highlight::Span;
use crate::lang_jsx::lex_jsx_composed_into;

/// TypeScript keyword core classified inside JSX expression containers
/// and non-JSX regions (upper-cased at comparison time).
pub static TYPESCRIPT_KEYWORDS: &[&str] = &[
    "as", "asserts", "async", "await", "abstract", "any", "boolean", "break", "case", "catch",
    "class", "const", "continue", "declare", "default", "delete", "do", "else", "enum", "export",
    "extends", "false", "finally", "for", "from", "function", "get", "if", "implements",
    "import", "in", "infer", "instanceof", "interface", "is", "keyof", "let", "namespace",
    "never", "new", "null", "number", "object", "override", "private", "protected", "public",
    "readonly", "return", "satisfies", "set", "static", "string", "super", "switch", "symbol",
    "this", "throw", "true", "try", "type", "typeof", "undefined", "unknown", "var", "void",
    "while", "yield",
];

/// TypeScript type-context core classified inside JSX expression
/// containers and non-JSX regions (upper-cased at comparison time).
pub static TYPESCRIPT_TYPES: &[&str] = &[
    "Array", "Boolean", "Date", "Error", "Function", "Map", "Number", "Object", "Promise",
    "ReadonlyArray", "Record", "Set", "String", "Symbol", "WeakMap", "WeakSet",
];

/// Versioned capability row for the TSX route (FCB-022 publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TsxCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for TSX.
    pub incremental: bool,
    /// JSX tag and attribute markup transitions supported.
    pub tag_and_attribute_transitions: bool,
    /// Embedded JavaScript expression containers (`{ ... }`) supported.
    pub embedded_expressions: bool,
    /// Fragment shorthand (`<> ... </>`) supported.
    pub fragments: bool,
    /// TypeScript keyword/type classification inside expression contexts.
    pub typescript_expressions: bool,
    /// Generic-vs-JSX ambiguity classifies conservatively (never guessed).
    pub conservative_unresolved: bool,
}

/// The TSX capability row published by this module.
pub const TSX_CAPABILITY_V1: TsxCapabilityV1 = TsxCapabilityV1 {
    version: 1,
    incremental: true,
    tag_and_attribute_transitions: true,
    embedded_expressions: true,
    fragments: true,
    typescript_expressions: true,
    conservative_unresolved: true,
};

/// Lex TSX source into exact tiling spans.
///
/// Composition: the JSX state machine owns tag/attribute/children
/// transitions; embedded expression containers route through the composed
/// JavaScript machinery extended with the TypeScript keyword/type tables;
/// non-JSX regions classify TypeScript declarations and type contexts.
pub fn lex_tsx_into(code: &str, spans: &mut Vec<Span>) {
    lex_jsx_composed_into(code, spans, TYPESCRIPT_KEYWORDS, TYPESCRIPT_TYPES);
}
