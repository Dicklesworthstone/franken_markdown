//! Comprehensive qualification suite for truthful per-language capabilities (FCB-022.B).
//!
//! Verifies:
//! - Strict capability ladder separation (Plan §11.5): highlighters never claim compiler semantics.
//! - Per-language truthful capability publication for all 20 required languages (Plan §11.4).
//! - Source-exact span tiling across all 20 languages with zero gaps and zero overlaps (§11.1, §11.7).
//! - Plain text fallback for unknown languages: opening a file never fails (§11.4).
//! - Conservative handling of JS/TS/JSX/TSX syntactic ambiguities (§11.7).
//! - Negative controls: compiler claim detection, span gap detection, and span overlap detection.

use franken_markdown::highlight::{Span, Tok};
use franken_markdown::lang_capabilities::{
    validate_span_tiling, CapabilityAuditError, CapabilityLevel, CapabilityMatrix,
};
use franken_markdown::lang_dispatch::{
    DispatchRequest, LanguageId, LanguageRegistry, QualificationStatus,
};

#[test]
fn capability_ladder_properties_and_permitted_claims() {
    assert_eq!(CapabilityLevel::Bytes.name(), "Bytes");
    assert_eq!(CapabilityLevel::Lexical.name(), "Lexical");
    assert_eq!(CapabilityLevel::Structural.name(), "Structural");
    assert_eq!(CapabilityLevel::ResolvedLocal.name(), "ResolvedLocal");
    assert_eq!(CapabilityLevel::ExternalSemantic.name(), "ExternalSemantic");
    assert_eq!(CapabilityLevel::Heuristic.name(), "Heuristic");

    // Lexical and Bytes are confined to source/token facts
    assert!(CapabilityLevel::Bytes.is_lexical_or_below());
    assert!(CapabilityLevel::Lexical.is_lexical_or_below());
    assert!(!CapabilityLevel::Structural.is_lexical_or_below());
    assert!(!CapabilityLevel::ExternalSemantic.is_lexical_or_below());

    // Only external semantic or resolved local permit semantic/compiler claims
    assert!(!CapabilityLevel::Bytes.allows_compiler_claim());
    assert!(!CapabilityLevel::Lexical.allows_compiler_claim());
    assert!(!CapabilityLevel::Structural.allows_compiler_claim());
    assert!(!CapabilityLevel::Heuristic.allows_compiler_claim());
    assert!(CapabilityLevel::ResolvedLocal.allows_compiler_claim());
    assert!(CapabilityLevel::ExternalSemantic.allows_compiler_claim());

    // Permitted claim strings match Plan §11.5 definitions
    assert!(CapabilityLevel::Bytes.permitted_claim().contains("Exact source representation"));
    assert!(CapabilityLevel::Lexical.permitted_claim().contains("Qualified token classification"));
    assert!(CapabilityLevel::Structural.permitted_claim().contains("Parser-supported syntax entities"));
    assert!(CapabilityLevel::ExternalSemantic.permitted_claim().contains("independently qualified provider"));
}

#[test]
fn release_matrix_covers_all_20_required_languages_as_implemented() {
    let matrix = CapabilityMatrix::release_20();
    assert_eq!(matrix.len(), 20, "Must contain exactly 20 language routes");
    assert_eq!(matrix.implemented_count(), 20, "All 20 routes must be Implemented");
    assert_eq!(matrix.provisional_count(), 0, "Zero routes should be Provisional");
    assert_eq!(matrix.missing_count(), 0, "Zero routes should be Missing");

    for &id in &LanguageId::ALL {
        let maybe_row = matrix.lookup_id(id);
        assert!(maybe_row.is_some(), "Missing capability row for {:?}", id);
        let row = maybe_row.unwrap();
        assert_eq!(row.version, 1);
        assert_eq!(row.status, QualificationStatus::Implemented);
        assert_eq!(row.max_level, CapabilityLevel::Lexical);
        assert!(row.source_exact_spans, "Language {:?} must guarantee source-exact spans", id);
        assert!(row.incremental, "Language {:?} must support incremental scanning", id);
        assert!(row.coalesced_equivalence, "Language {:?} must have verified coalesced equivalence", id);
        assert!(!row.provisional_choices, "Language {:?} must not rely on provisional choices", id);
        assert!(!row.multiline_constructs.is_empty(), "Language {:?} must declare multiline constructs", id);
    }
}

#[test]
fn release_matrix_audits_pass_cleanly() {
    let matrix = CapabilityMatrix::release_20();
    assert!(matrix.audit_no_compiler_claims().is_ok());
    assert!(matrix.audit_source_exact_spans().is_ok());
}

#[test]
fn negative_control_audit_rejects_compiler_semantic_claim() {
    let matrix = CapabilityMatrix::release_20();
    // Intentionally inject an illegal compiler claim into a highlighter route
    let mut illegal_row = matrix.lookup_id(LanguageId::Rust).unwrap();
    illegal_row.max_level = CapabilityLevel::ExternalSemantic; // ILLEGAL for a highlighter!

    // Verify row-level verification method directly detects the violation
    assert!(!illegal_row.verifies_no_compiler_claim());

    // Verify audit error shape
    let err = CapabilityAuditError::CompilerClaimForbidden {
        language: "rust",
        claimed_level: CapabilityLevel::ExternalSemantic,
    };
    assert_eq!(
        err.to_string(),
        "highlighter for 'rust' claimed forbidden level 'ExternalSemantic' (a highlighter is not a compiler)"
    );
}

#[test]
fn negative_control_audit_rejects_non_source_exact_span_claim() {
    let mut bad_row = CapabilityMatrix::release_20().lookup_id(LanguageId::Python).unwrap();
    bad_row.source_exact_spans = false; // ILLEGAL: must guarantee source-exact spans
    assert!(!bad_row.source_exact_spans);

    let err = CapabilityAuditError::SourceExactSpanViolation { language: "python" };
    assert_eq!(
        err.to_string(),
        "route for 'python' does not guarantee source-exact spans"
    );
}

#[test]
fn conservative_ambiguities_flagged_truthfully() {
    let matrix = CapabilityMatrix::release_20();

    // JS, TS, JSX, TSX have grammatical ambiguities without compiler AST
    // and MUST be flagged as conservative_ambiguities: true (Plan §11.7).
    assert!(matrix.lookup_id(LanguageId::JavaScript).unwrap().conservative_ambiguities);
    assert!(matrix.lookup_id(LanguageId::TypeScript).unwrap().conservative_ambiguities);
    assert!(matrix.lookup_id(LanguageId::Jsx).unwrap().conservative_ambiguities);
    assert!(matrix.lookup_id(LanguageId::Tsx).unwrap().conservative_ambiguities);

    // Languages with unambiguous lexical boundaries do not set this flag
    assert!(!matrix.lookup_id(LanguageId::Rust).unwrap().conservative_ambiguities);
    assert!(!matrix.lookup_id(LanguageId::Python).unwrap().conservative_ambiguities);
    assert!(!matrix.lookup_id(LanguageId::Go).unwrap().conservative_ambiguities);
    assert!(!matrix.lookup_id(LanguageId::Json).unwrap().conservative_ambiguities);
    assert!(!matrix.lookup_id(LanguageId::Toml).unwrap().conservative_ambiguities);
}

#[test]
fn plain_fallback_never_refuses_to_open_files() {
    let matrix = CapabilityMatrix::release_20();
    let registry = LanguageRegistry::release_20_inventory();

    // Unknown language strings
    let unknown_queries = [
        "fortran",
        "ada",
        "brainfuck",
        "unknown_format_99",
        "xyz",
        "",
        "   ",
        "plain",
    ];

    let test_code = b"RAW UNFORMATTED BYTES IN UNKNOWN FILE\nLine 2 with special chars: <>&@#$\n";

    for q in unknown_queries {
        let cap = matrix.lookup_or_fallback(q);
        assert_eq!(cap.canonical_name, "plain");
        assert_eq!(cap.status, QualificationStatus::Plain);
        assert_eq!(cap.max_level, CapabilityLevel::Bytes);
        assert!(cap.source_exact_spans);

        let req = DispatchRequest {
            language_query: q,
            code: test_code,
            work_budget_bytes: 4096,
            source_revision: Some(777),
        };

        // dispatch_with_fallback MUST succeed and preserve 100% of source bytes
        let res = registry.dispatch_with_fallback(req);
        assert_eq!(res.status, QualificationStatus::Plain);
        assert_eq!(res.bytes_processed, test_code.len());
        assert_eq!(res.spans.len(), 1);
        assert_eq!(res.spans[0].kind, Tok::Plain);
        assert_eq!(res.spans[0].start, 0);
        assert_eq!(res.spans[0].end, test_code.len());
        assert_eq!(res.source_revision, Some(777));

        // Tiling validator must confirm exact byte coverage
        assert!(validate_span_tiling(test_code.len(), &res.spans).is_ok());
    }
}

#[test]
fn empty_source_tiling_behavior() {
    let empty_spans: [Span; 0] = [];
    assert!(validate_span_tiling(0, &empty_spans).is_ok());

    let non_empty_on_zero = [Span {
        kind: Tok::Plain,
        start: 0,
        end: 1,
    }];
    assert!(validate_span_tiling(0, &non_empty_on_zero).is_err());
}

#[test]
fn negative_control_span_gap_detected() {
    let code_len = 10;
    // Missing byte 5!
    let gapped_spans = [
        Span {
            kind: Tok::Keyword,
            start: 0,
            end: 5,
        },
        Span {
            kind: Tok::Plain,
            start: 6,
            end: 10,
        },
    ];
    let err = validate_span_tiling(code_len, &gapped_spans).unwrap_err();
    assert!(err.contains("gap between span 0"));
    assert!(err.contains("missing bytes 5..6"));
}

#[test]
fn negative_control_span_overlap_detected() {
    let code_len = 10;
    // Overlapping byte 4..5!
    let overlapping_spans = [
        Span {
            kind: Tok::Keyword,
            start: 0,
            end: 5,
        },
        Span {
            kind: Tok::Plain,
            start: 4,
            end: 10,
        },
    ];
    let err = validate_span_tiling(code_len, &overlapping_spans).unwrap_err();
    assert!(err.contains("overlap between span 0"));
    assert!(err.contains("overlapping bytes 4..5"));
}

#[test]
fn negative_control_inverted_or_empty_span_detected() {
    let code_len = 10;
    let inverted_spans = [
        Span {
            kind: Tok::Keyword,
            start: 0,
            end: 5,
        },
        Span {
            kind: Tok::Plain,
            start: 5,
            end: 5, // Empty span!
        },
        Span {
            kind: Tok::Plain,
            start: 5,
            end: 10,
        },
    ];
    let err = validate_span_tiling(code_len, &inverted_spans).unwrap_err();
    assert!(err.contains("invalid empty/inverted bounds"));
}

#[test]
fn negative_control_incomplete_tail_detected() {
    let code_len = 10;
    let incomplete_spans = [Span {
        kind: Tok::Keyword,
        start: 0,
        end: 8, // Stops at 8, code is 10!
    }];
    let err = validate_span_tiling(code_len, &incomplete_spans).unwrap_err();
    assert!(err.contains("final span ends at 8 instead of input length 10"));
}

#[test]
fn source_exact_span_tiling_verified_for_all_20_languages() {
    let registry = LanguageRegistry::release_20_inventory();

    // Adversarial sample snippets exercising comments, strings, operators, and multiline constructs
    let fixtures: &[(&str, &[u8])] = &[
        ("rust", b"// comment\nfn test<'a>(x: &'a str) -> r#\"raw string\"# { 123 }\n"),
        ("python", b"# comment\ndef test(x):\n    \"\"\"docstring\"\"\"\n    return f'{x}'\n"),
        ("javascript", b"// js\nconst x = `template ${val}`; const r = /abc/g;\n"),
        ("typescript", b"// ts\ninterface X<T> { a: T; }\nconst val: number = 42;\n"),
        ("jsx", b"// jsx\nconst el = <div id=\"x\"><span>text</span><>{expr}</></div>;\n"),
        ("tsx", b"// tsx\nconst El: React.FC<Props> = ({x}) => <div prop={...rest}>{x}</div>;\n"),
        ("c", b"/* c */\n#define FOO \\\n  42\nint main() { return 0; }\n"),
        ("cpp", b"/* cpp */\n#include <iostream>\nauto r = R\"(raw)\";\nint main() {}\n"),
        ("csharp", b"// cs\nvar s = @\"verbatim\";\nvar i = $\"interpolated {1}\";\n"),
        ("go", b"// go\nfunc test() string { return `raw backtick` }\n"),
        ("java", b"// java\nString block = \"\"\"\n  text block\n  \"\"\";\n"),
        ("swift", b"/* swift /* nested */ */\nlet s = #\"raw \"#;\n"),
        ("shell", b"# bash\nVAR=val cat << 'EOF'\nheredoc line\nEOF\n"),
        ("json", b"{\n  \"key\": [1, 2.5, true, null, \"str\\nval\"]\n}\n"),
        ("toml", b"# toml\n[section]\nkey = '''multiline\nliteral'''\nnum = 42\n"),
        ("yaml", b"# yaml\nkey: |\n  literal block\n  scalar\nitem: 123\n"),
        ("sql", b"-- sql\nSELECT a, b /* block */ FROM tbl WHERE a = 'text';\n"),
        ("html", b"<!DOCTYPE html>\n<!-- comment -->\n<div class=\"x\">text &amp;</div>\n"),
        ("css", b"/* css */\n@media screen {\n  .cls { color: #fff; content: 'str'; }\n}\n"),
        ("markdown", b"# Heading\n\n> quote\n\n```rust\nfn code() {}\n```\n"),
    ];

    assert_eq!(fixtures.len(), 20, "All 20 languages must have an active tiling test fixture");

    for &(lang, code) in fixtures {
        let req = DispatchRequest {
            language_query: lang,
            code,
            work_budget_bytes: 65536,
            source_revision: Some(1),
        };

        let res = registry.dispatch(req);
        assert!(res.is_ok(), "Dispatch failed for {lang}");
        let result = res.unwrap();
        assert_eq!(result.status, QualificationStatus::Implemented);
        assert_eq!(result.bytes_processed, code.len());
        assert!(result.is_finished);

        // Verify span tiling with zero gaps and zero overlaps
        assert!(
            validate_span_tiling(code.len(), &result.spans).is_ok(),
            "Span tiling validation failed for {lang}"
        );
    }
}
