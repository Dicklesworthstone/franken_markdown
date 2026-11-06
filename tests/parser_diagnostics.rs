//! Diagnostics must describe the blocks the parser actually consumed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    DiagnosticSeverity, parse_markdown, parse_markdown_spanned, parse_markdown_spanned_profiled,
};

fn assert_clean(source: &str) {
    let parsed = parse_markdown_spanned(source);
    assert_eq!(parsed.to_document(), parse_markdown(source));
    assert!(
        parsed.diagnostics.is_empty(),
        "valid or literal source was diagnosed: {source:?}: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn accepted_multiline_reference_definitions_do_not_warn() {
    for source in [
        "[link]:\n  /target\n\nUse [link].\n",
        "[link]: /target \"a title\ncontinued here\"\n\nUse [link].\n",
        "[multi\nline]:\n/target\n\nUse [multi line].\n",
        "> [link]:\n> /target\n> \"A quoted title\"\n>\n> Use [link].\n",
        "- [link]:\n  /target\n\n  Use [link].\n",
    ] {
        assert_clean(source);
    }
}

#[test]
fn footnote_definitions_have_their_own_grammar() {
    for source in [
        "Use [^note].\n\n[^note]: First note with several words.\n",
        "[^note]:\n    First note with several words.\n\nUse [^note].\n",
        "Use [^note].\n\n[^note]: First paragraph.\n\n    ```rust\n    [literal]:\n    ```\n",
        "> Use [^note].\n>\n> [^note]: A quoted note.\n",
        "- Use [^note].\n\n  [^note]: A note in a list.\n",
    ] {
        assert_clean(source);
    }
}

#[test]
fn html_math_and_code_own_their_literal_contents() {
    for source in [
        "<div>\n[not-a-reference]:\n```rust\n</div>\n\nAfter.\n",
        "<!--\n[not-a-reference]:\n```rust\n-->\n\nAfter.\n",
        "<pre>\n\n[not-a-reference]:\n```rust\n</pre>\n\nAfter.\n",
        "$$\n[not-a-reference]:\n```rust\n$$\n\nAfter.\n",
        "    [not-a-reference]:\n    ```rust\n",
        "> ```rust\n> [not-a-reference]:\n> ```\n",
        "- ```rust\n  [not-a-reference]:\n  ```\n",
        "> <div>\n> [not-a-reference]:\n> ```rust\n> </div>\n",
    ] {
        assert_clean(source);
    }
}

#[test]
fn reference_shaped_paragraph_continuations_are_plain_text() {
    for source in [
        "Ordinary prose\n[not-a-definition]:\n",
        "> Quoted prose\n[not-a-definition]:\n",
        "- List prose\n[not-a-definition]:\n",
        "`literal code\n[not-a-definition]:`\n",
    ] {
        assert_clean(source);
    }
}

#[test]
fn definition_list_terms_have_their_own_grammar() {
    for source in [
        "[term]:\n: A valid definition.\n",
        "[term]:\nSecond term\n: A valid definition.\n",
        "> [term]:\n> : A quoted definition.\n",
        "- [term]:\n  : A listed definition.\n",
        "[^note]: [term]:\n    : A footnote definition.\n",
        "Term\n: First definition.\n\n[term]:\n: Second definition.\n",
        "> Term\n> : First definition.\n>\n> [term]:\n> : Second definition.\n",
        "- Term\n  : First definition.\n\n  [term]:\n  : Second definition.\n",
    ] {
        assert_clean(source);
    }
}

#[test]
fn accepted_references_keep_precedence_over_definition_list_terms() {
    for source in ["[term]:\n===\n", "[term]:\n:\n"] {
        assert_clean(source);
        assert!(parse_markdown(source).blocks.is_empty());
    }
    let source = "[term]: /target\n: An ordinary paragraph.\n\nUse [term].\n";
    assert_clean(source);
    assert_eq!(
        parse_markdown(source),
        parse_markdown(": An ordinary paragraph.\n\nUse [term](/target).\n")
    );
}

#[test]
fn malformed_references_still_warn_without_valid_definition_list_ownership() {
    for source in [
        "[broken]:\n\n: Separated by a blank line.\n",
        "[broken]:\n# Intervening heading\n: A later definition.\n",
        "[broken]:\n    : An indented non-marker.\n",
        "[broken]:\n:words without marker whitespace\n",
        "Term\n: A valid definition.\n\n[broken]:\n",
    ] {
        let parsed = parse_markdown_spanned(source);
        assert_eq!(
            parsed.diagnostics.len(),
            1,
            "{source:?}: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.diagnostics[0].span.slice(source), Some("[broken]:"));
        assert!(
            parsed.diagnostics[0]
                .message
                .contains("malformed link reference")
        );
        assert_eq!(parsed.to_document(), parse_markdown(source));
    }
}

#[test]
fn real_malformed_references_in_each_container_keep_exact_source_ranges() {
    let source = "[top]:\n\n> [quoted]:\n\n- [listed]:\n\n[^note]: [in-note]:\n";
    let parsed = parse_markdown_spanned(source);
    let slices: Vec<_> = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
            assert!(diagnostic.message.contains("malformed link reference"));
            diagnostic.span.slice(source).unwrap()
        })
        .collect();
    assert_eq!(slices, ["[top]:", "[quoted]:", "[listed]:", "[in-note]:"]);
    assert_eq!(parsed.to_document(), parse_markdown(source));
}

#[test]
fn unclosed_nested_fences_stop_at_their_actual_container_boundary() {
    let source = "> ```rust\n> let quoted = true;\n\nOutside.\n\n- ~~~rust\n  let listed = true;\n\nOutside again.\n";
    let parsed = parse_markdown_spanned(source);
    assert_eq!(parsed.diagnostics.len(), 2, "{:?}", parsed.diagnostics);
    for diagnostic in &parsed.diagnostics {
        assert!(diagnostic.message.contains("unclosed fenced code block"));
        assert!(diagnostic.message.contains("container"));
        assert!(!diagnostic.span.slice(source).unwrap().contains("Outside"));
    }
    assert_eq!(
        parsed.diagnostics[0].span.slice(source),
        Some("```rust\n> let quoted = true;\n")
    );
    assert_eq!(
        parsed.diagnostics[1].span.slice(source),
        Some("~~~rust\n  let listed = true;\n")
    );
}

#[test]
fn eof_fence_warning_preserves_the_original_full_span() {
    let source = "before\n\n  ```rust\nunterminated\n\n";
    let parsed = parse_markdown_spanned(source);
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(
        parsed.diagnostics[0].span.slice(source),
        Some("  ```rust\nunterminated\n\n")
    );
    assert!(parsed.diagnostics[0].message.contains("end of document"));
}

#[test]
fn copied_container_prefixes_and_crlf_preserve_original_utf8_byte_offsets() {
    let source = "\u{feff}---\r\ntitle: Café\r\n---\r\n\r\n-\t[broken]:\r\n\r\n>\t```rust\r\n>\tlet value = \"λ\";\r\n\r\nOutside.\r\n";
    let parsed = parse_markdown_spanned(source);
    assert_eq!(parsed.diagnostics.len(), 2, "{:?}", parsed.diagnostics);
    assert_eq!(parsed.diagnostics[0].span.slice(source), Some("[broken]:"));
    assert_eq!(
        parsed.diagnostics[1].span.slice(source),
        Some("```rust\r\n>\tlet value = \"λ\";\r\n")
    );
    let profiled = parse_markdown_spanned_profiled(source);
    assert_eq!(profiled.document, parsed);
}

#[test]
fn wasm_diagnostics_preserve_valid_content_and_real_findings() {
    let source =
        "[link]:\n/target\n\nUse [link] and [^note].\n\n[^note]: A real note.\n\n> [broken]:\n";
    let options = franken_markdown::wasm::WasmRenderOptions::default()
        .with_custom_css_bytes(b"body { color: black; }")
        .unwrap();
    let rendered = franken_markdown::wasm::render_html(source, &options).unwrap();
    assert!(rendered.html().unwrap().contains("href=\"/target\""));
    assert!(rendered.html().unwrap().contains("A real note."));
    assert_eq!(rendered.diagnostics.len(), 1, "{:?}", rendered.diagnostics);
    let warning = &rendered.diagnostics[0];
    assert_eq!(&source[warning.start..warning.end], "[broken]:");
    assert!(warning.message.contains("malformed link reference"));
}
