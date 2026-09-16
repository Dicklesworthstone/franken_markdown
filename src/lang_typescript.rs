#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::if_same_then_else)]

//! TypeScript incremental lexer (FCB-022 · fcb-9vx.7).
//!
//! Reuses the qualified upstream JavaScript lexical transitions per the
//! language composition contract. Classifies TypeScript source into byte-exact
//! [`Span`]s that tile the input exactly. Handles, under declared capability:
//!
//! - **TypeScript keywords & type contexts**: `type`, `interface`, `namespace`,
//!   `declare`, `abstract`, `implements`, `keyof`, `readonly`, `infer`, `is`,
//!   `asserts`, `satisfies`, `as`, `override`, `public`, `private`, `protected`.
//! - **Builtin & utility types**: `any`, `unknown`, `never`, `void`, `string`,
//!   `number`, `boolean`, `symbol`, `bigint`, `object`, `undefined`, `unique`,
//!   plus standard mapped/utility types (`Record`, `Partial`, `Promise`, etc.).
//! - **Generics vs comparisons**: conservative ambiguous classification under
//!   declared capability (no speculative compiler AST claims).
//! - **Template literal types**: `` type Event = `on${string}`; ``
//! - **Shared JavaScript lexical transitions**: strings, template literals with
//!   nested interpolation stacks, regexes, comments (`//`, `/* */`, Annex B),
//!   numbers with `_` separators and bigint `n`.

use crate::highlight::Span;
use crate::lang_javascript::lex_javascript_composed_into;

/// TypeScript-specific keywords layered on top of JavaScript keywords.
pub const TYPESCRIPT_KEYWORDS: &[&str] = &[
    "type",
    "interface",
    "namespace",
    "module",
    "declare",
    "abstract",
    "implements",
    "keyof",
    "readonly",
    "infer",
    "is",
    "asserts",
    "satisfies",
    "as",
    "override",
    "public",
    "private",
    "protected",
    "global",
    "from",
];

/// TypeScript-specific built-in types layered on top of JavaScript types.
pub const TYPESCRIPT_TYPES: &[&str] = &[
    "any",
    "unknown",
    "never",
    "void",
    "string",
    "number",
    "boolean",
    "symbol",
    "bigint",
    "object",
    "undefined",
    "unique",
    "Record",
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
    "Exclude",
    "Extract",
    "NonNullable",
    "Parameters",
    "ConstructorParameters",
    "ReturnType",
    "InstanceType",
    "Uppercase",
    "Lowercase",
    "Capitalize",
    "Uncapitalize",
    "Awaited",
];

/// The versioned TypeScript capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeScriptCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for TypeScript.
    pub incremental: bool,
    /// Type keywords and type annotations supported.
    pub type_keywords: bool,
    /// Generics vs comparisons handled conservatively under declared capability.
    pub generics_vs_comparisons: bool,
    /// Template literal types supported.
    pub template_literal_types: bool,
    /// Type assertions (as / satisfies) supported.
    pub satisfies_and_as_casts: bool,
}

/// The TypeScript capability row published by this module.
pub const TYPESCRIPT_CAPABILITY_V1: TypeScriptCapabilityV1 = TypeScriptCapabilityV1 {
    version: 1,
    incremental: true,
    type_keywords: true,
    generics_vs_comparisons: true,
    template_literal_types: true,
    satisfies_and_as_casts: true,
};

/// Lex TypeScript source into exact tiling spans.
///
/// Composes over the qualified upstream JavaScript lexical transitions without
/// duplicating comment, string, template, or numeric state machines.
pub fn lex_typescript_into(code: &str, spans: &mut Vec<Span>) {
    lex_javascript_composed_into(code, spans, TYPESCRIPT_KEYWORDS, TYPESCRIPT_TYPES);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::Tok;

    /// The source-tiling oracle: spans must exactly tile [0, len) with no
    /// gaps, overlaps, or reordering.
    fn assert_tiling(code: &str, spans: &[Span]) {
        let mut cursor = 0usize;
        for span in spans {
            assert_eq!(
                span.start, cursor,
                "gap/overlap at {cursor}: span {:?} {}-{}",
                span.kind, span.start, span.end
            );
            assert!(span.end > span.start, "empty span at {cursor}");
            cursor = span.end;
        }
        assert_eq!(cursor, code.len(), "spans do not reach end of input");
    }

    #[test]
    fn type_alias_and_interface_declarations() {
        let code = "type Id = string | number;\ninterface Point<T> { x: T; y: T; }";
        let mut spans = Vec::new();
        lex_typescript_into(code, &mut spans);
        assert_tiling(code, &spans);

        let kinds: Vec<(Tok, &str)> = spans.iter().map(|s| (s.kind, &code[s.start..s.end])).collect();
        assert!(kinds.contains(&(Tok::Keyword, "type")));
        assert!(kinds.contains(&(Tok::Type, "string")));
        assert!(kinds.contains(&(Tok::Type, "number")));
        assert!(kinds.contains(&(Tok::Keyword, "interface")));
    }

    #[test]
    fn generics_vs_comparisons_conservative_resolution() {
        let code = "function identity<T>(arg: T): T { return arg; }\nconst ok = a < b && b > c;";
        let mut spans = Vec::new();
        lex_typescript_into(code, &mut spans);
        assert_tiling(code, &spans);

        let operators: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Operator)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(operators.contains(&"<"));
        assert!(operators.contains(&">"));
        assert!(operators.contains(&"&&"));
    }

    #[test]
    fn template_literal_types_and_values() {
        let code = "type Event = `on${string}`;\nconst name = `Hello, ${user}!`;";
        let mut spans = Vec::new();
        lex_typescript_into(code, &mut spans);
        assert_tiling(code, &spans);

        let str_spans: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(str_spans.iter().any(|s| s.starts_with("`on")));
        assert!(str_spans.iter().any(|s| s.starts_with("`Hello")));
    }

    #[test]
    fn type_assertions_as_and_satisfies() {
        let code = "const obj = { val: 42 } as const;\nconst res = data satisfies Record<string, unknown>;";
        let mut spans = Vec::new();
        lex_typescript_into(code, &mut spans);
        assert_tiling(code, &spans);

        let keywords: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Keyword)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(keywords.contains(&"as"));
        assert!(keywords.contains(&"satisfies"));
        let types: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Type)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(types.contains(&"Record"));
        assert!(types.contains(&"string"));
        assert!(types.contains(&"unknown"));
    }

    #[test]
    fn type_predicates_and_assertions() {
        let code = "function isStr(x: unknown): x is string { return typeof x === \"string\"; }";
        let mut spans = Vec::new();
        lex_typescript_into(code, &mut spans);
        assert_tiling(code, &spans);

        let keywords: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Keyword)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(keywords.contains(&"is"));
        assert!(keywords.contains(&"typeof"));
    }

    #[test]
    fn class_access_modifiers_and_abstract() {
        let code = "abstract class Base {\n  public readonly id: string;\n  private secret: number;\n  protected abstract run(): void;\n}";
        let mut spans = Vec::new();
        lex_typescript_into(code, &mut spans);
        assert_tiling(code, &spans);

        let keywords: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Keyword)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(keywords.contains(&"abstract"));
        assert!(keywords.contains(&"public"));
        assert!(keywords.contains(&"readonly"));
        assert!(keywords.contains(&"private"));
        assert!(keywords.contains(&"protected"));
    }

    #[test]
    fn capability_row_is_versioned() {
        assert_eq!(TYPESCRIPT_CAPABILITY_V1.version, 1);
        assert!(TYPESCRIPT_CAPABILITY_V1.incremental);
        assert!(TYPESCRIPT_CAPABILITY_V1.type_keywords);
        assert!(TYPESCRIPT_CAPABILITY_V1.generics_vs_comparisons);
        assert!(TYPESCRIPT_CAPABILITY_V1.template_literal_types);
        assert!(TYPESCRIPT_CAPABILITY_V1.satisfies_and_as_casts);
    }

    #[test]
    fn negative_control_tiling_gap_detected() {
        let code = "type X = number;";
        let broken = vec![
            Span { kind: Tok::Keyword, start: 0, end: 4 },
            // gap from 4..7 omitted!
            Span { kind: Tok::Type, start: 9, end: 15 },
            Span { kind: Tok::Punct, start: 15, end: 16 },
        ];
        let result = std::panic::catch_unwind(|| {
            assert_tiling(code, &broken);
        });
        assert!(result.is_err(), "tiling oracle must reject gaps");
    }
}
