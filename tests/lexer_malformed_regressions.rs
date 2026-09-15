use franken_markdown::{highlight::Span, lang_go::lex_go_into, lang_python::lex_python_into};

fn assert_tiles(code: &str, spans: &[Span]) {
    let mut end = 0;
    for span in spans {
        assert_eq!(span.start, end);
        assert!(span.end > span.start);
        assert!(
            code.get(span.start..span.end).is_some(),
            "{code:?}: {span:?}"
        );
        end = span.end;
    }
    assert_eq!(end, code.len());
}

#[test]
fn python_all_whitespace_makes_progress() {
    for ch in ['\u{000b}', '\u{000c}', '\u{0085}', '\u{2003}', '\t', '\n'] {
        for code in [ch.to_string(), format!("x{ch}={ch}1")] {
            let mut spans = Vec::new();
            lex_python_into(&code, &mut spans);
            assert_tiles(&code, &spans);
        }
    }
}

#[test]
fn go_hex_escapes_preserve_utf8_boundaries() {
    for quote in ['"', '\''] {
        for (escape, width) in [('x', 2), ('u', 4), ('U', 8)] {
            for prefix in 0..=width {
                for suffix in ["€", "😀", "é", "", "g"] {
                    let body = format!("{quote}\\{escape}{}{suffix}", "0".repeat(prefix));
                    for code in [body.clone(), format!("{body}{quote} + value")] {
                        let mut spans = Vec::new();
                        lex_go_into(&code, &mut spans);
                        assert_tiles(&code, &spans);
                    }
                }
            }
        }
    }
}

#[test]
fn shared_and_language_whitespace_routes_make_progress() {
    for lang in [
        "rust",
        "python",
        "javascript",
        "json",
        "bash",
        "powershell",
        "go",
        "c",
        "toml",
        "yaml",
        "sql",
        "java",
        "swift",
        "csharp",
    ] {
        for ch in ['\u{000b}', '\u{000c}', '\u{2003}'] {
            let code = format!("x{ch}={ch}1");
            eprintln!("whitespace route {lang}: {code:?}");
            let spans = franken_markdown::highlight::highlight(lang, &code);
            assert_tiles(&code, &spans);
        }
    }
}
