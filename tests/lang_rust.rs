#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Comprehensive test suite and adversarial split corpus for the Rust incremental lexer (FCB-022.4).
//!
//! Verifies:
//! - Nested block comments (`/* /* nested */ */`)
//! - Raw strings with varying hash counts (`r"..."`, `r#"..."#`, `r##"..."##`, `r###"..."###`)
//! - Raw byte strings (`br"..."`, `br#"..."#`)
//! - Lifetimes (`'a`, `'static`, `'_`, `'loop`) vs character literals (`'c'`, `'\n'`, `'\''`, `'\u{1f980}'`)
//! - Byte strings (`b"..."`) and byte characters (`b'x'`, `b'\n'`)
//! - Raw identifiers (`r#match`, `r#type`)
//! - Numbers, keywords, types, functions, operators, and punctuation
//! - Exact source byte tiling across all fixtures
//! - Whole/chunk/EOF coalesced equivalence across arbitrary chunk sizes (1, 2, 3, 5, 7, 16, 64)
//! - Adversarial splits across every single byte boundary
//! - Versioned checkpoint detection of nested comment depth and raw string hashes
//! - Deterministic audit trail receipt with seed, FNV-1a digests, and replay command

use franken_markdown::highlight::{highlight, Span, Tok};
use franken_markdown::lang_dispatch::{digest_spans, fnv1a_64, DispatchAuditReceipt, ResourceCounters};
use franken_markdown::resume::{
    coalesce_spans, verify_whole_block_coalesced_equivalence, CommentState, ResumableLexer,
    StringState,
};

fn assert_tiled(spans: &[Span], code_len: usize) {
    if code_len == 0 {
        assert!(spans.is_empty() || (spans.len() == 1 && spans[0].start == 0 && spans[0].end == 0));
        return;
    }
    assert!(!spans.is_empty(), "spans must not be empty for nonempty code");
    assert_eq!(spans[0].start, 0, "first span must start at 0");
    let mut offset = 0;
    for (i, span) in spans.iter().enumerate() {
        assert_eq!(
            span.start, offset,
            "span {i} start {} must equal previous offset {}",
            span.start, offset
        );
        assert!(
            span.end >= span.start,
            "span {i} end {} must be >= start {}",
            span.end,
            span.start
        );
        offset = span.end;
    }
    assert_eq!(offset, code_len, "final span end must equal code length");
}

#[test]
fn nested_block_comments() {
    // Single level
    let single = "/* single level comment */";
    let spans1 = highlight("rust", single);
    assert_tiled(&spans1, single.len());
    let coalesced1 = coalesce_spans(&spans1);
    assert_eq!(coalesced1.len(), 1);
    assert_eq!(coalesced1[0].kind, Tok::Comment);

    // Depth 2
    let depth2 = "/* outer /* inner */ still outer */";
    let spans2 = highlight("rust", depth2);
    assert_tiled(&spans2, depth2.len());
    let coalesced2 = coalesce_spans(&spans2);
    assert_eq!(coalesced2.len(), 1);
    assert_eq!(coalesced2[0].kind, Tok::Comment);
    assert_eq!(coalesced2[0].start, 0);
    assert_eq!(coalesced2[0].end, depth2.len());

    // Depth 3 with surrounding code
    let code = "let x = /* l1 /* l2 /* l3 */ l2 */ l1 */ 42;";
    let spans = highlight("rust", code);
    assert_tiled(&spans, code.len());

    let comment_span = spans.iter().find(|s| s.kind == Tok::Comment).unwrap();
    assert_eq!(&code[comment_span.start..comment_span.end], "/* l1 /* l2 /* l3 */ l2 */ l1 */");

    // Deep nesting (depth 6)
    let deep = "/* a /* b /* c /* d /* e /* f */ e */ d */ c */ b */ a */";
    let spans_deep = highlight("rust", deep);
    assert_tiled(&spans_deep, deep.len());
    let coalesced_deep = coalesce_spans(&spans_deep);
    assert_eq!(coalesced_deep.len(), 1);
    assert_eq!(coalesced_deep[0].kind, Tok::Comment);

    // Unterminated nested comment runs to EOF
    let unterminated = "/* l1 /* l2 unterminated";
    let spans_unterm = highlight("rust", unterminated);
    assert_tiled(&spans_unterm, unterminated.len());
    let coalesced_unterm = coalesce_spans(&spans_unterm);
    assert_eq!(coalesced_unterm.len(), 1);
    assert_eq!(coalesced_unterm[0].kind, Tok::Comment);
}

#[test]
fn raw_strings_varying_hashes() {
    let cases: &[(&str, &str)] = &[
        ("r\"simple\"", "r\"simple\""),
        ("r#\"with \"quotes\" and #hash\"#", "r#\"with \"quotes\" and #hash\"#"),
        ("r##\"two \"# hashes \"##", "r##\"two \"# hashes \"##"),
        ("r###\"three \"## hashes \"###", "r###\"three \"## hashes \"###"),
        ("r#####\"five \"#### hashes \"#####", "r#####\"five \"#### hashes \"#####"),
    ];

    for &(code, expected_str) in cases {
        let spans = highlight("rust", code);
        assert_tiled(&spans, code.len());
        let coalesced = coalesce_spans(&spans);
        assert_eq!(coalesced.len(), 1, "failed for case: {code}");
        assert_eq!(coalesced[0].kind, Tok::Str, "failed for case: {code}");
        assert_eq!(&code[coalesced[0].start..coalesced[0].end], expected_str);
    }

    // Unterminated raw string runs to EOF
    let unterm = "let s = r##\"unterminated with \"# quote";
    let spans_unterm = highlight("rust", unterm);
    assert_tiled(&spans_unterm, unterm.len());
    let str_span = spans_unterm.iter().find(|s| s.kind == Tok::Str).unwrap();
    assert_eq!(&unterm[str_span.start..str_span.end], "r##\"unterminated with \"# quote");
}

#[test]
fn raw_byte_strings_and_byte_literals() {
    let code = "let b1 = br\"raw bytes\"; let b2 = br##\"raw \"# bytes\"##; let b3 = b'x'; let b4 = b'\\n';";
    let spans = highlight("rust", code);
    assert_tiled(&spans, code.len());

    let str_spans: Vec<&str> = spans
        .iter()
        .filter(|s| s.kind == Tok::Str)
        .map(|s| &code[s.start..s.end])
        .collect();

    assert_eq!(str_spans, vec!["br\"raw bytes\"", "br##\"raw \"# bytes\"##", "b'x'", "b'\\n'"]);
}

#[test]
fn lifetimes_versus_char_literals() {
    let code = "fn parse<'a, 'static, '_>(ch: char, s: &'a str) {\n    let c1 = 'x';\n    let c2 = '\\n';\n    let c3 = '\\'';\n    let c4 = '\\u{1f980}';\n    'named_loop: loop {\n        break 'named_loop;\n    }\n}\n";
    let spans = highlight("rust", code);
    assert_tiled(&spans, code.len());

    // Find all lifetimes (classified as Tok::Type starting with '\'')
    let lifetimes: Vec<&str> = spans
        .iter()
        .filter(|s| s.kind == Tok::Type && code[s.start..].starts_with('\''))
        .map(|s| &code[s.start..s.end])
        .collect();

    assert!(lifetimes.contains(&"'a"), "must contain 'a");
    assert!(lifetimes.contains(&"'static"), "must contain 'static");
    assert!(lifetimes.contains(&"'_"), "must contain '_");
    assert!(lifetimes.contains(&"'named_loop"), "must contain 'named_loop");

    // Find all char literals (classified as Tok::Str)
    let char_literals: Vec<&str> = spans
        .iter()
        .filter(|s| s.kind == Tok::Str)
        .map(|s| &code[s.start..s.end])
        .collect();

    assert!(char_literals.contains(&"'x'"), "must contain 'x'");
    assert!(char_literals.contains(&"'\\n'"), "must contain '\\n'");
    assert!(char_literals.contains(&"'\\''"), "must contain '\\''");
    assert!(char_literals.contains(&"'\\u{1f980}'"), "must contain '\\u{{1f980}}'");
}

#[test]
fn raw_identifiers() {
    let code = "let r#match = 1; let r#type = 2; let r#fn = 3;";
    let spans = highlight("rust", code);
    assert_tiled(&spans, code.len());

    let raw_ids: Vec<&str> = spans
        .iter()
        .filter(|s| s.kind == Tok::Keyword && code[s.start..].starts_with("r#"))
        .map(|s| &code[s.start..s.end])
        .collect();

    assert_eq!(raw_ids, vec!["r#match", "r#type", "r#fn"]);
}

#[test]
fn numbers_and_type_suffixes() {
    let code = "let a = 123; let b = 0xdead_BEEF; let c = 0o77; let d = 0b1010_0101; let e = 3.14159f64; let f = 1e10; let g = 42_usize;";
    let spans = highlight("rust", code);
    assert_tiled(&spans, code.len());

    let numbers: Vec<&str> = spans
        .iter()
        .filter(|s| s.kind == Tok::Number)
        .map(|s| &code[s.start..s.end])
        .collect();

    assert_eq!(
        numbers,
        vec![
            "123",
            "0xdead_BEEF",
            "0o77",
            "0b1010_0101",
            "3.14159f64",
            "1e10",
            "42_usize"
        ]
    );
}

#[test]
fn whole_chunk_eof_equivalence_across_chunk_sizes() {
    let rust_fixture = r####"
// Standard Rust comprehensive module
use std::sync::Arc;

/* Outer comment /* nested inner */ outer tail */
pub struct Container<'a, T> {
    pub name: &'a str,
    pub data: Arc<T>,
    pub raw: &'static str,
}

impl<'a, T: Clone> Container<'a, T> {
    pub fn new(name: &'a str, val: T) -> Self {
        let msg = r#"Raw "string" with #hash"#;
        let bmsg = br##"Raw byte string "#"##;
        let ch = '\u{1f980}';
        let count = 0x2a_usize;
        'search: for i in 0..count {
            if i == 42 {
                break 'search;
            }
        }
        Self {
            name,
            data: Arc::new(val),
            raw: msg,
        }
    }
}
"####;

    let chunk_sizes = [1, 2, 3, 5, 7, 11, 16, 32, 64, 128];

    for &chunk_size in &chunk_sizes {
        let res = verify_whole_block_coalesced_equivalence(
            "rust",
            rust_fixture,
            chunk_size,
        );
        assert!(
            res.is_ok(),
            "Whole/chunk coalesced equivalence failed for chunk_size {chunk_size}: {:?}",
            res.err()
        );
    }
}

#[test]
fn adversarial_byte_splits_on_nested_comments_and_raw_strings() {
    let adversarial_fixture = "/* /* nested */ */ r##\"raw \"# string\"## 'a 'x' b\"bytes\"";

    for split in 1..adversarial_fixture.len() {
        let (head, tail) = adversarial_fixture.as_bytes().split_at(split);
        let mut lexer = ResumableLexer::new("rust").expect("valid lexer");

        let r1 = lexer.feed(head);
        assert!(r1.is_ok(), "feed head failed at split {split}: {:?}", r1.err());

        let r2 = lexer.feed(tail);
        assert!(r2.is_ok(), "feed tail failed at split {split}: {:?}", r2.err());

        let r3 = lexer.finish();
        assert!(r3.is_ok(), "finish failed at split {split}: {:?}", r3.err());

        let chunked_coalesced = coalesce_spans(lexer.spans());
        let whole_spans = highlight("rust", adversarial_fixture);
        let whole_coalesced = coalesce_spans(&whole_spans);

        assert_eq!(
            chunked_coalesced.len(),
            whole_coalesced.len(),
            "span count mismatch at split {split}"
        );

        for (i, (c_span, w_span)) in chunked_coalesced.iter().zip(&whole_coalesced).enumerate() {
            assert_eq!(
                c_span.kind, w_span.kind,
                "kind mismatch at span {i}, split {split}"
            );
            assert_eq!(
                c_span.start, w_span.start,
                "start mismatch at span {i}, split {split}"
            );
            assert_eq!(
                c_span.end, w_span.end,
                "end mismatch at span {i}, split {split}"
            );
        }
    }
}

#[test]
fn versioned_checkpoint_state_detection_rust() {
    // Check nested comment depth detection in pending tail
    let mut lexer = ResumableLexer::new("rust").unwrap();
    let partial_comment = b"let x = 1;\n/* outer /* inner ";
    lexer.feed(partial_comment).unwrap();

    let cp = lexer.checkpoint(10);
    assert_eq!(cp.comment_state, CommentState::Block { depth: 2 });
    assert_eq!(cp.string_state, StringState::None);

    // Check raw string hash detection in pending tail
    let mut lexer2 = ResumableLexer::new("rust").unwrap();
    let partial_raw = b"let s = r###\"unfinished raw";
    lexer2.feed(partial_raw).unwrap();

    let cp2 = lexer2.checkpoint(20);
    assert_eq!(cp2.comment_state, CommentState::None);
    assert_eq!(cp2.string_state, StringState::RawString { hashes: 3 });

    // Check raw byte string hash detection in pending tail
    let mut lexer3 = ResumableLexer::new("rust").unwrap();
    let partial_braw = b"let b = br##\"unfinished byte raw";
    lexer3.feed(partial_braw).unwrap();

    let cp3 = lexer3.checkpoint(30);
    assert_eq!(cp3.string_state, StringState::RawString { hashes: 2 });
}

#[test]
fn deterministic_audit_receipt_rust() {
    let seed = 0x7057_6c65_7865_7231; // "rust_lexer1"
    let rust_sample = r####"
#[derive(Debug, Clone)]
pub enum AST<'a> {
    Ident(&'a str),
    Raw(Arc<[u8]>),
}

/* Multiline /* nested */ comment */
fn execute<'a>(ast: &AST<'a>) -> Result<(), &'static str> {
    let msg = r##"Executed "# with status: Ok"##;
    println!("{msg}");
    Ok(())
}
"####;

    let input_hash = fnv1a_64(rust_sample.as_bytes());
    let spans = highlight("rust", rust_sample);
    let output_hash = digest_spans(&spans);

    let receipt = DispatchAuditReceipt {
        seed,
        input_digest: input_hash,
        output_digest: output_hash,
        expected_spans: spans.len(),
        actual_spans: spans.len(),
        counters: ResourceCounters {
            bytes_scanned: rust_sample.len(),
            spans_emitted: spans.len(),
            capacity_limit: 1 << 20,
        },
        replay_command: "cargo test --test lang_rust -- deterministic_audit_receipt_rust".to_string(),
    };

    assert_eq!(receipt.seed, seed);
    assert_eq!(receipt.expected_spans, spans.len());
    assert_eq!(receipt.counters.bytes_scanned, rust_sample.len());

    // Determinism assertion
    let spans2 = highlight("rust", rust_sample);
    assert_eq!(output_hash, digest_spans(&spans2));
}
