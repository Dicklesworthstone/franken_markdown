//! FCB-022.21 HTML adversarial split corpus: every representative fixture is
//! lexed whole and at every byte split through the shared FCB-021 resumable
//! engine; coalesced token meaning and exact source tiling must agree.
//! Malformed and truncated inputs must lex with bounded work, never panic.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{highlight, Span, Tok};
use franken_markdown::lang_html::{lex_html_into, HtmlCapabilityV1};
use franken_markdown::resume::{coalesce_spans, ResumableLexer};

/// Representative HTML fixtures exercising DOCTYPE declarations, comments,
/// CDATA sections, XML declarations, tags, self-closing tags, attribute
/// variants (double, single, unquoted, boolean), character entities (named,
/// numeric, hex), embedded inert script/style blocks, SVG markup, and
/// truncated hostile constructs.
const FIXTURES: &[&str] = &[
    "<!DOCTYPE html>\n",
    "<!DOCTYPE html SYSTEM \"about:legacy-compat\">\n",
    "<!-- line comment -->\n<p>Hello world</p>\n",
    "<!-- multi-line\ncomment\nwith symbols <>&\" -->\n<div>Content</div>\n",
    "<![CDATA[ <sender>John</sender> ]]>\n",
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root><child>value</child></root>\n",
    "<div class=\"container\" id='main' data-value=123>\n    <p>Text</p>\n</div>\n",
    "<img src=\"icon.png\" alt=\"An image\" width=\"32\" height=\"32\" />\n",
    "<br/>\n<hr />\n<input type=\"checkbox\" checked disabled required>\n",
    "<p>Escaped &amp; &lt; &gt; &#38; &#x26; &#1234; entities here</p>\n",
    "<script>\n    const x = 1 < 2 && 3 > 0;\n    console.log(\"hello\");\n</script>\n",
    "<style>\n    body { color: red; background: url('bg.png'); }\n</style>\n",
    "<svg viewBox=\"0 0 100 100\" xmlns=\"http://www.w3.org/2000/svg\">\n    <circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"red\" />\n</svg>\n",
    "<tag unquoted=attr_val single='value' double=\"value\"></tag>\n",
    "<a href=\"https://example.com?q=1&amp;v=2\">Link</a>\n",
    "<!-- unterminated comment",
    "<![CDATA[ unterminated CDATA",
    "<!DOCTYPE unterminated doctype",
    "<?xml unterminated processing instruction",
    "<div class=\"unterminated tag without close",
    "<script>unterminated script block",
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
        let whole = highlight("html", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("html").expect("html route exists");
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
        let whole = highlight("html", fixture);
        let whole_coalesced = coalesced(&whole);

        for chunk_size in [1, 2, 3, 5, 7, 16, 64] {
            let mut lexer = ResumableLexer::new("html").expect("html route exists");
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
                "fixture {fixture:?} diverges at chunk size {chunk_size}"
            );
        }
    }
}

#[test]
fn scanner_direct_matches_highlight_route() {
    for &fixture in FIXTURES {
        let routed = highlight("html", fixture);
        let mut direct = Vec::new();
        lex_html_into(fixture, &mut direct);
        assert_tiling(&direct, fixture.len());
        assert_eq!(
            coalesced(&routed),
            coalesced(&direct),
            "route and direct scanner disagree for {fixture:?}"
        );
    }
}

#[test]
fn doctype_and_comments_classified() {
    let source = "<!DOCTYPE html>\n<!-- comment block -->\n<p>Body</p>";
    let spans = highlight("html", source);
    assert_tiling(&spans, source.len());
    let has_doctype = spans.iter().any(|s| s.kind == Tok::Keyword && &source[s.start..s.end] == "<!DOCTYPE html>");
    assert!(has_doctype, "DOCTYPE must be classified as Keyword");
    let has_comment = spans.iter().any(|s| s.kind == Tok::Comment && &source[s.start..s.end] == "<!-- comment block -->");
    assert!(has_comment, "comment must be classified as Comment");
}

#[test]
fn entities_and_cdata_classified() {
    let source = "<![CDATA[raw content]]>&amp;&#38;&#x26;";
    let spans = highlight("html", source);
    assert_tiling(&spans, source.len());
    let has_cdata = spans.iter().any(|s| s.kind == Tok::Str && &source[s.start..s.end] == "<![CDATA[raw content]]>");
    assert!(has_cdata, "CDATA must be classified as Str");
    let entity_spans: Vec<_> = spans.iter().filter(|s| s.kind == Tok::Keyword && source[s.start..s.end].starts_with('&')).collect();
    assert_eq!(entity_spans.len(), 3, "entities &amp;, &#38;, &#x26; must be classified as Keyword");
}

#[test]
fn embedded_script_and_style_inert_boundaries() {
    let source = "<script>\nconst a = 1 < 2;\n</script>\n<style>\nbody { margin: 0; }\n</style>";
    let spans = highlight("html", source);
    assert_tiling(&spans, source.len());

    // Both script and style tag keywords must be recognized
    let script_tags: Vec<_> = spans.iter().filter(|s| s.kind == Tok::Keyword && &source[s.start..s.end] == "script").collect();
    assert_eq!(script_tags.len(), 2, "open and close script tags");

    let style_tags: Vec<_> = spans.iter().filter(|s| s.kind == Tok::Keyword && &source[s.start..s.end] == "style").collect();
    assert_eq!(style_tags.len(), 2, "open and close style tags");

    // Inert script body must be Tok::Plain
    let has_script_body = spans.iter().any(|s| s.kind == Tok::Plain && source[s.start..s.end].contains("const a = 1 < 2;"));
    assert!(has_script_body, "inert script body must be Tok::Plain");
}

#[test]
fn malformed_hostile_inputs_bounded_and_no_panic() {
    for hostile in [
        "<!--",
        "<!-- --",
        "<![CDATA[",
        "<![CDATA[ unclosed",
        "<!",
        "<!DOCTYPE",
        "<?",
        "<?xml",
        "<",
        "</",
        "<123>",
        "<tag attr=\"unclosed string",
        "<tag attr='unclosed single",
        "<tag attr=",
        "&",
        "&#",
        "&#x",
        "&unterminated",
        "<script>",
        "<script>alert(1)",
        "<style>",
        "<style>body {",
        "<<<<<<<<<<<<<<<",
        ">>>>>>>>>>>>>>>",
        "<tag\0with\0nulls>",
    ] {
        let mut direct = Vec::new();
        lex_html_into(hostile, &mut direct);
        for span in &direct {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        assert_tiling(&direct, hostile.len());

        for split in 0..hostile.len() {
            let mut lexer = ResumableLexer::new("html").expect("html route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn html_capability_truthful_verification() {
    let cap = HtmlCapabilityV1::current();
    assert!(cap.tags);
    assert!(cap.attributes);
    assert!(cap.comments);
    assert!(cap.entities);
    assert!(cap.doctype);
    assert!(cap.cdata);
    assert!(cap.embedded_blocks);
}

#[test]
fn negative_control_detects_coalesce_divergence() {
    // Demonstrate that the test oracle detects mismatched tokens and ranges
    let original = highlight("html", "<div class=\"btn\">Click</div>");
    let mut corrupted = original.clone();
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

    let seed = 0xFCB_022_21_u64;
    let code = b"<!DOCTYPE html><html lang=\"en\"><head><title>FCB</title></head><body>&copy; 2026</body></html>";
    let input_hash = fnv1a_hash(code);

    let whole = highlight("html", std::str::from_utf8(code).unwrap());
    let mut output_bytes = Vec::new();
    for span in &whole {
        output_bytes.extend_from_slice(&(span.kind as u32).to_le_bytes());
        output_bytes.extend_from_slice(&(span.start as u64).to_le_bytes());
        output_bytes.extend_from_slice(&(span.end as u64).to_le_bytes());
    }
    let output_hash = fnv1a_hash(&output_bytes);

    // Replay determinism check
    let repeat = highlight("html", std::str::from_utf8(code).unwrap());
    let mut repeat_bytes = Vec::new();
    for span in &repeat {
        repeat_bytes.extend_from_slice(&(span.kind as u32).to_le_bytes());
        repeat_bytes.extend_from_slice(&(span.start as u64).to_le_bytes());
        repeat_bytes.extend_from_slice(&(span.end as u64).to_le_bytes());
    }
    let repeat_hash = fnv1a_hash(&repeat_bytes);

    assert_eq!(output_hash, repeat_hash, "Deterministic output hash across runs");
    assert_ne!(input_hash, 0);
    assert_ne!(output_hash, 0);

    eprintln!(
        "HtmlAuditReceipt: seed=0x{:x}, in_hash=0x{:x}, out_hash=0x{:x}, spans={}, replay='cargo test -j 2 --test html_split_corpus'",
        seed, input_hash, output_hash, whole.len()
    );
}
