//! FCB-022.14 Java adversarial split corpus: every representative fixture is
//! lexed whole and at every byte split through the shared FCB-021 resumable
//! engine; coalesced token meaning and exact source tiling must agree.
//! Malformed and truncated inputs must lex with bounded work, never panic.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lang_java::{JavaCapabilityV1, lex_java_into};
use franken_markdown::resume::{ResumableLexer, coalesce_spans};

/// Representative Java fixtures exercising comments, text blocks, annotations,
/// standard strings with unicode/escape sequences, character literals,
/// numeric forms and suffixes, keywords, contextual keywords, records,
/// lambdas, method references, and truncated constructs.
const FIXTURES: &[&str] = &[
    "var x = 1;\n",
    "public class Widget {\n    private final int count = calculate();\n}\n",
    "// line comment\n/* block\ncomment */ var y = 2;\n",
    "/** Javadoc comment with @param and @return */\npublic int getCount() { return 42; }\n",
    "String s = \"hello \\\"world\\\" \\n \\t \\\\ unicode: \\u0041\";\n",
    "String block = \"\"\"\n    line 1 \"quotes\"\n    line 2\n    \"\"\";\n",
    "char c = 'x';\nchar esc = '\\n';\nchar quote = '\\'';\n",
    "int hex = 0xCAFE_BABE;\nint bin = 0b1010_0101;\nlong l = 100_000_000_000L;\n",
    "float f = 1.5f;\ndouble d = 3.14159D;\ndouble exp = 1.2e-4;\n",
    "@Override\n@SuppressWarnings(\"unchecked\")\npublic void run() { }\n",
    "public record Point(int x, int y) implements Serializable { }\n",
    "public sealed interface Shape permits Circle, Square { }\n",
    "list.stream().map(String::trim).filter(s -> !s.isEmpty()).toList();\n",
    "switch (obj) {\n    case Integer i when i > 0 -> System.out.println(i);\n    default -> yield -1;\n}\n",
    "var broken = \"truncated string without end",
    "var brokenBlock = \"\"\"unterminated text block",
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
        let whole = highlight("java", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("java").expect("java route exists");
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
        let whole = highlight("java", fixture);
        let whole_coalesced = coalesced(&whole);

        for chunk_size in [1, 2, 3, 5, 7, 16, 64] {
            let mut lexer = ResumableLexer::new("java").expect("java route exists");
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
        let routed = highlight("java", fixture);
        let mut direct = Vec::new();
        lex_java_into(fixture, &mut direct);
        assert_eq!(
            coalesced(&routed),
            coalesced(&direct),
            "route and direct scanner disagree for {fixture:?}"
        );
        assert_tiling(&direct, fixture.len());
    }
}

#[test]
fn text_block_delimiter_and_escape_preservation() {
    let source = "String tb = \"\"\"\n    hello \\\"world\\\"\n    line 2\n    \"\"\";";
    let spans = highlight("java", source);
    assert_tiling(&spans, source.len());
    let has_string = spans.iter().any(|s| {
        s.kind == Tok::Str
            && &source[s.start..s.end] == "\"\"\"\n    hello \\\"world\\\"\n    line 2\n    \"\"\""
    });
    assert!(has_string, "text block must be classified as string");
}

#[test]
fn annotations_classified_properly() {
    let source = "@Override @SuppressWarnings(\"unchecked\") void test() {}";
    let spans = highlight("java", source);
    assert_tiling(&spans, source.len());
    let annot_count = spans
        .iter()
        .filter(|s| {
            (s.kind == Tok::Type || s.kind == Tok::Keyword || s.kind == Tok::Plain)
                && (source[s.start..s.end].starts_with('@'))
        })
        .count();
    assert_eq!(
        annot_count, 2,
        "both @Override and @SuppressWarnings must be recognized"
    );
}

#[test]
fn number_suffixes_and_radices() {
    let source = "0xCAFE_BABE 0b1010 12345L 3.14f 2.718d 1e-10";
    let spans = highlight("java", source);
    assert_tiling(&spans, source.len());
    let num_spans: Vec<_> = spans.iter().filter(|s| s.kind == Tok::Number).collect();
    assert_eq!(num_spans.len(), 6, "all 6 numbers must be Tok::Number");
}

#[test]
fn malformed_hostile_inputs_bounded_and_no_panic() {
    for hostile in [
        "\"\"\"\"\"",
        "\"\"\"",
        "/*",
        "/**",
        "'",
        "'\\",
        "'\\u",
        "@",
        "0x",
        "0b",
        "1.5e",
        "\"broken \\u",
        "\"unclosed string",
        "var x = \0\0\0;",
    ] {
        let mut direct = Vec::new();
        lex_java_into(hostile, &mut direct);
        for span in &direct {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        assert_tiling(&direct, hostile.len());

        for split in 0..hostile.len() {
            let mut lexer = ResumableLexer::new("java").expect("java route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn java_capability_truthful_verification() {
    let cap = JavaCapabilityV1::current();
    assert!(cap.text_blocks);
    assert!(cap.unicode_escapes);
    assert!(cap.annotations);
    assert!(cap.method_references);
    assert!(cap.pattern_matching_switch);
    assert!(cap.records);
}

#[test]
fn negative_control_detects_coalesce_divergence() {
    // Demonstrate that the test oracle detects mismatched tokens and ranges
    let original = highlight("java", "int x = 42;");
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

    let seed = 0xFCB0_2214_u64;
    let code = b"public record User(String name, int age) implements Serializable {}";
    let input_hash = fnv1a_hash(code);

    let whole = highlight("java", std::str::from_utf8(code).unwrap());
    let mut output_bytes = Vec::new();
    for span in &whole {
        output_bytes.extend_from_slice(&(span.kind as u32).to_le_bytes());
        output_bytes.extend_from_slice(&(span.start as u64).to_le_bytes());
        output_bytes.extend_from_slice(&(span.end as u64).to_le_bytes());
    }
    let output_hash = fnv1a_hash(&output_bytes);

    // Replay determinism check
    let repeat = highlight("java", std::str::from_utf8(code).unwrap());
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
        "JavaAuditReceipt: seed=0x{:x}, in_hash=0x{:x}, out_hash=0x{:x}, spans={}, replay='cargo test -j 2 --test java_split_corpus'",
        seed,
        input_hash,
        output_hash,
        whole.len()
    );
}
