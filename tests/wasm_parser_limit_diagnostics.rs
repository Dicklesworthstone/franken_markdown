//! Browser-facing output preserves parser limits as coded source diagnostics.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::wasm::{WasmRenderOptions, render_html, render_pdf};

#[test]
fn browser_html_and_pdf_keep_native_parser_codes_and_byte_ranges() {
    let hostile = format!("{}leaf{}", "[".repeat(32), "](/target)".repeat(32));
    let source = format!("café\r\n\r\n{hostile}\r\n\r\n[good](/ok)");
    let native = franken_markdown::parse_markdown_spanned(&source);
    assert_eq!(native.diagnostics.len(), 1);
    let expected = &native.diagnostics[0];
    assert_eq!(expected.code, "inline_work_limit");
    assert_eq!(expected.span.slice(&source), Some(hostile.as_str()));

    for output in [
        render_html(&source, &WasmRenderOptions::default()).unwrap(),
        render_pdf(&source, &WasmRenderOptions::default()).unwrap(),
    ] {
        let actual = output
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == Some(expected.code))
            .expect("bounded parsing must be visible to the browser host");
        assert_eq!(actual.start, expected.span.start);
        assert_eq!(actual.end, expected.span.end);
        assert_eq!(actual.message, expected.message);
        assert_eq!(actual.severity, "warning");
        let json = output.diagnostics_json();
        assert!(json.contains("\"code\":\"inline_work_limit\""));
        assert!(json.contains(&format!(
            "\"start\":{},\"end\":{}",
            expected.span.start, expected.span.end
        )));
        assert!(!json.contains("\"scope\":\"document\""));
    }
}
