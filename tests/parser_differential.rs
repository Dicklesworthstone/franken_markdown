//! Parser differential fixture runner. The approved references are checked-in
//! article-body snapshots under `tests/fixtures/parser/*.article.html`.
//!
//! This is deliberately not a production dependency on an external Markdown
//! parser. It is a small native dev harness that catches parser/render drift and
//! gives agents a stable place to add CommonMark/GFM edge cases.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use franken_markdown::{HtmlOptions, render_html};

fn render(src: &str) -> String {
    render_html(src, &HtmlOptions::default()).unwrap()
}

fn main_inner(html: &str) -> &str {
    let start_marker = "<main class=\"fmd\">\n";
    let end_marker = "</main>\n</body>";
    let start = html.find(start_marker).unwrap() + start_marker.len();
    let end = html.find(end_marker).unwrap();
    &html[start..end]
}

#[test]
fn approved_parser_fixtures_match_article_snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parser");
    let mut checked = 0usize;

    for entry in fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap();
        let rendered = render(&source);
        let actual = main_inner(&rendered);
        let expected_path = path.with_extension("article.html");
        let expected = fs::read_to_string(&expected_path).unwrap();

        assert_eq!(
            expected.trim_end(),
            actual.trim_end(),
            "parser fixture drift: {}",
            path.display()
        );
        eprintln!("parser fixture ok: {}", path.display());
        checked += 1;
    }

    eprintln!("parser fixture count: {checked}");
    assert!(checked >= 4, "expected at least four parser fixtures");
}
