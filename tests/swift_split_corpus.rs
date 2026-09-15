//! FCB-022.15 Swift adversarial split corpus: every representative fixture is
//! lexed whole and at every byte split through the shared FCB-021 resumable
//! engine; coalesced token meaning and exact source tiling must agree.
//! Malformed and truncated inputs must lex with bounded work, never panic.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lang_swift::{SwiftCapabilityV1, lex_swift_into};
use franken_markdown::resume::{ResumableLexer, coalesce_spans};

/// Representative Swift fixtures exercising comments, nested block comments,
/// multiline strings, raw strings with varying hash counts, standard strings
/// with interpolation, backtick-escaped identifiers, attributes, directives,
/// number forms, keywords, types, function calls, and truncated constructs.
const FIXTURES: &[&str] = &[
    "let x = 1\n",
    "public struct Widget {\n    private let count: Int = calculate()\n}\n",
    "// line comment\n/* block\ncomment */ var y = 2\n",
    "/* outer /* nested block /* level 3 */ comment */ back to outer */ let z = 3\n",
    "let s = \"hello \\\"world\\\" \\n \\t unicode: \\u{0041}\"\n",
    "let multiline = \"\"\"\n    line 1 \"quotes\"\n    line 2\n    \"\"\"\n",
    "let raw = #\"Hello \"world\" \\n not escaped \\#(name)\"#\n",
    "let rawMulti = #\"\"\"\n    raw multiline \"\"\"\n    \"\"\"#\n",
    "let interp = \"Count: \\(count + 1) items\"\n",
    "let hex = 0xCAFE_BABE\nlet bin = 0b1010_0101\nlet oct = 0o755\nlet floatNum = 3.14159\nlet exp = 1.2e-4\n",
    "@objc @Published @MainActor var name: String = \"test\"\n",
    "#if os(iOS)\nlet platform = \"iOS\"\n#else\nlet platform = \"macOS\"\n#endif\n",
    "func test(`default`: Int) -> Int { return `default` * 2 }\n",
    "numbers.map { $0 * 2 }.filter { $0 > 10 }\n",
    "var broken = \"truncated string without end",
    "var brokenBlock = \"\"\"unterminated text block",
    "/* unterminated nested /* comment",
];

fn coalesced(spans: &[Span]) -> Vec<(Tok, usize, usize)> {
    coalesce_spans(spans)
        .iter()
        .map(|span| (span.kind, span.start, span.end))
        .collect()
}

fn assert_tiling(spans: &[Span], total_len: usize) {
    let mut next = 0usize;
    for span in spans {
        assert_eq!(span.start, next, "gap/overlap at {}", span.start);
        assert!(span.end > span.start, "empty span at {}", span.start);
        next = span.end;
    }
    assert_eq!(next, total_len, "spans must tile entire input");
}

#[test]
fn every_split_matches_whole_run_on_all_fixtures() {
    for &fixture in FIXTURES {
        let whole = highlight("swift", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("swift").expect("swift route exists");
            let (a, b) = fixture.as_bytes().split_at(split);
            if !a.is_empty() {
                lexer.feed(a).expect("first feed succeeds");
            }
            if !b.is_empty() {
                lexer.feed(b).expect("second feed succeeds");
            }
            lexer.finish().expect("finish succeeds");
            assert_tiling(lexer.spans(), fixture.len());
            let split_coalesced = coalesced(lexer.spans());
            assert_eq!(
                split_coalesced, whole_coalesced,
                "fixture {fixture:?} diverges at byte split {split}"
            );
        }
    }
}

#[test]
fn variable_chunk_sizes_match_whole_run() {
    for &fixture in FIXTURES {
        let whole = highlight("swift", fixture);
        let whole_coalesced = coalesced(&whole);

        for chunk_size in [1, 2, 3, 5, 7, 16, 64] {
            let mut lexer = ResumableLexer::new("swift").expect("swift route exists");
            let bytes = fixture.as_bytes();
            let mut offset = 0;
            while offset < bytes.len() {
                let end = (offset + chunk_size).min(bytes.len());
                lexer.feed(&bytes[offset..end]).expect("feed chunk");
                offset = end;
            }
            lexer.finish().expect("finish chunked lex");
            assert_tiling(lexer.spans(), fixture.len());
            let chunked_coalesced = coalesced(lexer.spans());
            assert_eq!(
                chunked_coalesced, whole_coalesced,
                "fixture {fixture:?} diverges with chunk size {chunk_size}"
            );
        }
    }
}

#[test]
fn scanner_direct_matches_highlight_route() {
    for &fixture in FIXTURES {
        let routed = highlight("swift", fixture);
        let mut direct = Vec::new();
        lex_swift_into(fixture, &mut direct);
        assert_eq!(
            coalesced(&routed),
            coalesced(&direct),
            "route and direct scanner disagree for {fixture:?}"
        );
        assert_tiling(&direct, fixture.len());
    }
}

#[test]
fn nested_comments_properly_counted() {
    let source = "/* level 1 /* level 2 /* level 3 */ level 2 */ level 1 */ let final_val = 1;";
    let spans = highlight("swift", source);
    assert_tiling(&spans, source.len());
    let comment_span = spans
        .iter()
        .find(|s| s.kind == Tok::Comment)
        .expect("comment span");
    assert_eq!(
        &source[comment_span.start..comment_span.end],
        "/* level 1 /* level 2 /* level 3 */ level 2 */ level 1 */"
    );
}

#[test]
fn raw_and_multiline_strings_identified() {
    let source = "#\"raw \"quotes\" here\"#\n\"\"\"\nmultiline\n\"\"\"";
    let spans = highlight("swift", source);
    assert_tiling(&spans, source.len());
    let str_spans: Vec<_> = spans.iter().filter(|s| s.kind == Tok::Str).collect();
    assert_eq!(
        str_spans.len(),
        2,
        "both raw and multiline strings must be Tok::Str"
    );
}

#[test]
fn attributes_and_hash_directives_classified() {
    let source = "@MainActor #if DEBUG let x = 1 #endif";
    let spans = highlight("swift", source);
    assert_tiling(&spans, source.len());
    let has_attr = spans.iter().any(|s| {
        (s.kind == Tok::Type || s.kind == Tok::Keyword) && &source[s.start..s.end] == "@MainActor"
    });
    assert!(has_attr, "@MainActor attribute must be classified");
    let has_hash_if = spans
        .iter()
        .any(|s| s.kind == Tok::Keyword && &source[s.start..s.end] == "#if");
    assert!(has_hash_if, "#if directive must be classified");
}

#[test]
fn backtick_escaped_identifiers() {
    let source = "func test(`default`: Int) { let `class` = 42 }";
    let spans = highlight("swift", source);
    assert_tiling(&spans, source.len());
    let backtick_spans: Vec<_> = spans
        .iter()
        .filter(|s| source[s.start..s.end].starts_with('`'))
        .collect();
    assert_eq!(
        backtick_spans.len(),
        2,
        "both `default` and `class` must be recognized"
    );
}

#[test]
fn malformed_hostile_inputs_bounded_and_no_panic() {
    for hostile in [
        "\"\"\"\"\"",
        "\"\"\"",
        "#\"\"\"",
        "##\"unterminated",
        "/*",
        "/* /* /*",
        "`unclosed backtick",
        "@",
        "#",
        "0x",
        "0b",
        "0o",
        "1.5e",
        "\"broken \\(",
        "\"broken \\u{",
        "var x = \0\0\0;",
    ] {
        let mut direct = Vec::new();
        lex_swift_into(hostile, &mut direct);
        for span in &direct {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        assert_tiling(&direct, hostile.len());

        for split in 0..hostile.len() {
            let mut lexer = ResumableLexer::new("swift").expect("swift route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn swift_capability_truthful_verification() {
    let cap = SwiftCapabilityV1::current();
    assert!(cap.multiline_strings);
    assert!(cap.raw_strings);
    assert!(cap.nested_comments);
    assert!(cap.string_interpolation);
    assert!(cap.backtick_identifiers);
    assert!(cap.attributes);
}

#[test]
fn negative_control_detects_coalesce_divergence() {
    // Demonstrate that the test oracle detects mismatched tokens and ranges
    let original = highlight("swift", "let count: Int = 42");
    let mut corrupted = original.clone();
    // Intentionally corrupt token kind
    if let Some(first) = corrupted.first_mut() {
        first.kind = Tok::Comment;
    }
    assert_ne!(
        coalesced(&original),
        coalesced(&corrupted),
        "negative control: corruption must be detected by coalesced comparison"
    );
}

#[test]
fn deterministic_audit_trail_receipt() {
    // FNV-1a 64-bit hasher
    fn fnv1a_hash(data: &[u8]) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for &byte in data {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    let seed = 0xFCB0_2215_u64;
    let code = b"public actor DataService: Identifiable, Sendable { private let id: UUID }";
    let input_hash = fnv1a_hash(code);

    let whole = highlight("swift", std::str::from_utf8(code).unwrap());
    let mut output_bytes = Vec::new();
    for span in &whole {
        output_bytes.extend_from_slice(&(span.kind as u32).to_le_bytes());
        output_bytes.extend_from_slice(&(span.start as u64).to_le_bytes());
        output_bytes.extend_from_slice(&(span.end as u64).to_le_bytes());
    }
    let output_hash = fnv1a_hash(&output_bytes);

    // Replay determinism check
    let repeat = highlight("swift", std::str::from_utf8(code).unwrap());
    let mut repeat_bytes = Vec::new();
    for span in &repeat {
        repeat_bytes.extend_from_slice(&(span.kind as u32).to_le_bytes());
        repeat_bytes.extend_from_slice(&(span.start as u64).to_le_bytes());
        repeat_bytes.extend_from_slice(&(span.end as u64).to_le_bytes());
    }
    let repeat_hash = fnv1a_hash(&repeat_bytes);

    assert_eq!(
        output_hash, repeat_hash,
        "Deterministic output hash across runs"
    );
    assert_ne!(input_hash, 0);
    assert_ne!(output_hash, 0);

    eprintln!(
        "SwiftAuditReceipt: seed=0x{:x}, in_hash=0x{:x}, out_hash=0x{:x}, spans={}, replay='cargo test -j 2 --test swift_split_corpus'",
        seed,
        input_hash,
        output_hash,
        whole.len()
    );
}
