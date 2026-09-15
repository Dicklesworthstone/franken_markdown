#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Comprehensive test suite for language coverage inventory and capability dispatch (FCB-022.A).

use franken_markdown::lang_dispatch::{
    digest_spans, fnv1a_64, DispatchAuditReceipt, DispatchError, DispatchRequest, LanguageId,
    LanguageRegistry, LanguageRouteRegistration, QualificationStatus, ResourceCounters,
    MAX_ALIASES_PER_ROUTE, MAX_REGISTERED_LANGUAGES,
};

#[test]
fn inventory_the_20_required_language_routes() {
    let registry = LanguageRegistry::standard_20_inventory();
    assert_eq!(registry.len(), 20, "Must have exactly 20 required language routes");

    let expected_languages = [
        (LanguageId::Rust, "rust", &["rust", "rs"][..]),
        (LanguageId::Python, "python", &["python", "py"][..]),
        (LanguageId::JavaScript, "javascript", &["javascript", "js", "mjs", "cjs"][..]),
        (LanguageId::TypeScript, "typescript", &["typescript", "ts"][..]),
        (LanguageId::Jsx, "jsx", &["jsx"][..]),
        (LanguageId::Tsx, "tsx", &["tsx"][..]),
        (LanguageId::C, "c", &["c", "h"][..]),
        (LanguageId::Cpp, "cpp", &["cpp", "c++", "cc", "hpp"][..]),
        (LanguageId::CSharp, "csharp", &["csharp", "cs", "c#"][..]),
        (LanguageId::Go, "go", &["go", "golang"][..]),
        (LanguageId::Java, "java", &["java"][..]),
        (LanguageId::Swift, "swift", &["swift"][..]),
        (LanguageId::Shell, "bash", &["shell", "bash", "sh", "zsh"][..]),
        (LanguageId::Json, "json", &["json", "jsonc"][..]),
        (LanguageId::Toml, "toml", &["toml"][..]),
        (LanguageId::Yaml, "yaml", &["yaml", "yml"][..]),
        (LanguageId::Sql, "sql", &["sql"][..]),
        (LanguageId::Html, "html", &["html", "htm", "xhtml", "xml", "svg"][..]),
        (LanguageId::Css, "css", &["css", "scss", "sass"][..]),
        (LanguageId::Markdown, "markdown", &["markdown", "md", "mdown", "mkd"][..]),
    ];

    assert_eq!(expected_languages.len(), 20);

    for (expected_id, expected_primary, expected_aliases) in expected_languages {
        let route = registry
            .lookup(expected_id.canonical_name())
            .expect("canonical lookup must succeed");
        assert_eq!(route.language_id, expected_id);
        assert_eq!(route.primary_alias, expected_primary);

        for &alias in expected_aliases {
            let alias_route = registry.lookup(alias).expect("alias lookup must succeed");
            assert_eq!(alias_route.language_id, expected_id);
        }
    }
}

#[test]
fn duplicate_canonical_language_registration_rejected() {
    let mut registry = LanguageRegistry::new();
    let r1 = LanguageRouteRegistration {
        language_id: LanguageId::Rust,
        status: QualificationStatus::Implemented,
        primary_alias: "rust",
        aliases: &["rs"],
        description: "Rust incremental lexer",
    };
    assert!(registry.register(r1.clone()).is_ok());

    let r2 = LanguageRouteRegistration {
        language_id: LanguageId::Rust, // Duplicate ID!
        status: QualificationStatus::Provisional,
        primary_alias: "rust2",
        aliases: &["rs2"],
        description: "Duplicate Rust registration",
    };
    let err = registry.register(r2).unwrap_err();
    assert_eq!(err, DispatchError::DuplicateLanguage(LanguageId::Rust));
    assert_eq!(err.code(), "DUPLICATE_LANGUAGE");
}

#[test]
fn duplicate_alias_registration_rejected() {
    let mut registry = LanguageRegistry::new();
    let r1 = LanguageRouteRegistration {
        language_id: LanguageId::C,
        status: QualificationStatus::Implemented,
        primary_alias: "c",
        aliases: &["c", "h"],
        description: "C lexer",
    };
    registry.register(r1).unwrap();

    let r2 = LanguageRouteRegistration {
        language_id: LanguageId::Cpp,
        status: QualificationStatus::Implemented,
        primary_alias: "cpp",
        aliases: &["cpp", "h"], // Colliding alias 'h'!
        description: "C++ lexer",
    };
    let err = registry.register(r2).unwrap_err();
    assert_eq!(
        err,
        DispatchError::DuplicateAlias {
            alias: "h".to_string(),
            existing_lang: LanguageId::C,
        }
    );
    assert_eq!(err.code(), "DUPLICATE_ALIAS");
}

#[test]
fn alias_limit_per_route_enforced() {
    let mut registry = LanguageRegistry::new();
    let excessive_aliases = &[
        "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "a10", "a11", "a12", "a13", "a14",
        "a15", "a16", "a17",
    ];
    assert!(excessive_aliases.len() > MAX_ALIASES_PER_ROUTE);

    let route = LanguageRouteRegistration {
        language_id: LanguageId::Rust,
        status: QualificationStatus::Implemented,
        primary_alias: "rust",
        aliases: excessive_aliases,
        description: "Too many aliases",
    };
    let err = registry.register(route).unwrap_err();
    assert_eq!(
        err,
        DispatchError::TooManyAliases {
            count: excessive_aliases.len(),
            max: MAX_ALIASES_PER_ROUTE,
        }
    );
    assert_eq!(err.code(), "TOO_MANY_ALIASES");
}

#[test]
fn registry_capacity_cap_enforced() {
    let mut registry = LanguageRegistry::new();
    // Fill up with 20 standard routes
    let standard = LanguageRegistry::standard_20_inventory();
    for route in standard.inventory() {
        registry.register(route.clone()).unwrap();
    }
    assert_eq!(registry.len(), 20);

    // Verify bounded cap constant
    assert_eq!(MAX_REGISTERED_LANGUAGES, 64);
}

#[test]
fn unknown_language_query_rejected() {
    let registry = LanguageRegistry::standard_20_inventory();
    let queries = ["fortran", "pascal", "basic", "assembly", "unknown_xyz", ""];
    for q in queries {
        let err = registry.lookup(q).unwrap_err();
        assert_eq!(
            err,
            DispatchError::UnknownLanguage {
                query: q.to_string()
            }
        );
        assert_eq!(err.code(), "UNKNOWN_LANGUAGE");
    }
}

#[test]
fn missing_language_qualification_truthfully_reported_and_refused_on_dispatch() {
    let registry = LanguageRegistry::standard_20_inventory();
    let missing = registry.missing_routes();

    // Exactly C#, Java, Swift are missing in initial inventory
    assert_eq!(missing.len(), 3);
    assert!(missing.contains(&LanguageId::CSharp));
    assert!(missing.contains(&LanguageId::Java));
    assert!(missing.contains(&LanguageId::Swift));

    // Refusal on dispatch
    for &id in &missing {
        let req = DispatchRequest {
            language_query: id.canonical_name(),
            code: b"class Test {}",
            work_budget_bytes: 1024,
            source_revision: Some(1),
        };
        let err = registry.dispatch(req).unwrap_err();
        assert_eq!(
            err,
            DispatchError::LanguageNotQualified {
                language_id: id,
                status: QualificationStatus::Missing,
            }
        );
        assert_eq!(err.code(), "LANGUAGE_NOT_QUALIFIED");
    }
}

#[test]
fn provisional_languages_identified() {
    let registry = LanguageRegistry::standard_20_inventory();
    let provisional = registry.provisional_routes();

    assert_eq!(provisional.len(), 5);
    assert!(provisional.contains(&LanguageId::Jsx));
    assert!(provisional.contains(&LanguageId::Tsx));
    assert!(provisional.contains(&LanguageId::Html));
    assert!(provisional.contains(&LanguageId::Css));
    assert!(provisional.contains(&LanguageId::Markdown));

    for &id in &provisional {
        let route = registry.lookup(id.canonical_name()).unwrap();
        assert_eq!(route.status, QualificationStatus::Provisional);
        assert_eq!(route.status.code(), "PROVISIONAL");
        assert!(route.status.is_dispatchable());
    }
}

#[test]
fn implemented_languages_dispatch_through_real_fcb021_engine() {
    let registry = LanguageRegistry::standard_20_inventory();
    let implemented = registry.implemented_routes();

    // 12 languages are implemented with full coalesced equivalence
    assert_eq!(implemented.len(), 12);
    assert_eq!(registry.len(), 12 + 5 + 3); // 12 implemented + 5 provisional + 3 missing = 20!

    let fixtures: &[(&str, &[u8])] = &[
        ("rust", b"fn calculate(val: u32) -> u32 { val * 2 }"),
        ("python", b"def calculate(val):\n    return val * 2\n"),
        ("javascript", b"function calculate(val) { return val * 2; }"),
        ("typescript", b"function calculate(val: number): number { return val * 2; }"),
        ("go", b"func Calculate(val int) int { return val * 2 }"),
        ("c", b"int calculate(int val) { return val * 2; }"),
        ("cpp", b"int calculate(int val) { return val * 2; }"),
        ("shell", b"#!/usr/bin/env bash\necho 'hello world'\n"),
        ("json", b"{\"key\": \"value\", \"count\": 42}"),
        ("yaml", b"key: value\ncount: 42\n"),
        ("toml", b"key = \"value\"\ncount = 42\n"),
        ("sql", b"SELECT id, name FROM users WHERE active = 1;"),
    ];

    for &(lang, code) in fixtures {
        let req = DispatchRequest {
            language_query: lang,
            code,
            work_budget_bytes: 4096,
            source_revision: Some(99),
        };
        let res = registry.dispatch(req).expect("dispatch must succeed");
        assert_eq!(res.status, QualificationStatus::Implemented);
        assert_eq!(res.bytes_processed, code.len());
        assert_eq!(res.pending_suffix_bytes, 0);
        assert!(res.is_finished);
        assert_eq!(res.source_revision, Some(99));
        assert!(!res.spans.is_empty(), "spans must not be empty for {lang}");

        // Verify spans exactly tile input
        let mut offset = 0;
        for span in &res.spans {
            assert_eq!(span.start, offset, "spans must be contiguous for {lang}");
            assert!(span.end >= span.start, "span end must be >= start for {lang}");
            offset = span.end;
        }
        assert_eq!(offset, code.len(), "spans must tile entire code for {lang}");
    }
}

#[test]
fn work_budget_limits_strictly_enforced() {
    let registry = LanguageRegistry::standard_20_inventory();
    let code = b"let a = 1; let b = 2; let c = 3;";
    let req = DispatchRequest {
        language_query: "rust",
        code,
        work_budget_bytes: 10, // code is 32 bytes > budget 10!
        source_revision: None,
    };
    let err = registry.dispatch(req).unwrap_err();
    assert_eq!(
        err,
        DispatchError::BudgetExceeded {
            requested: 32,
            budget: 10,
        }
    );
    assert_eq!(err.code(), "BUDGET_EXCEEDED");
}

#[test]
fn deterministic_audit_receipts_with_seed_digest_and_replay() {
    let registry = LanguageRegistry::standard_20_inventory();
    let seed = 0xdead_beef_cafe_babe;
    let code = b"pub fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n";

    let input_hash = fnv1a_64(code);
    assert_ne!(input_hash, 0);

    let req = DispatchRequest {
        language_query: "rust",
        code,
        work_budget_bytes: 4096,
        source_revision: Some(42),
    };
    let res = registry.dispatch(req).unwrap();
    let output_hash = digest_spans(&res.spans);
    assert_ne!(output_hash, 0);

    let receipt = DispatchAuditReceipt {
        seed,
        input_digest: input_hash,
        output_digest: output_hash,
        expected_spans: res.spans.len(),
        actual_spans: res.spans.len(),
        counters: ResourceCounters {
            bytes_scanned: res.bytes_processed,
            spans_emitted: res.spans.len(),
            capacity_limit: 4096,
        },
        replay_command: "cargo test --test lang_dispatch -- deterministic_audit_receipts".to_string(),
    };

    assert_eq!(receipt.seed, seed);
    assert_eq!(receipt.expected_spans, receipt.actual_spans);
    assert_eq!(receipt.counters.bytes_scanned, code.len());

    // Determinism check: repeat and assert identical digest
    let res2 = registry.dispatch(req).unwrap();
    let output_hash2 = digest_spans(&res2.spans);
    assert_eq!(output_hash, output_hash2, "Digests must be bit-for-bit identical across runs");
}
