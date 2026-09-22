//! Complete document content and revision-scoped links in visual comparisons.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::diff::compute_diff;
use franken_markdown::{HtmlOptions, PdfImageAsset, parse_markdown};
use std::io::Write;
use std::process::{Command, Stdio};

fn compare(old: &str, new: &str) -> String {
    compute_diff(
        &parse_markdown(old),
        &parse_markdown(new),
        "before.md",
        "after.md",
    )
    .to_html_with_options(&HtmlOptions::default())
}

fn check_document(html: &str) {
    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/diff_html_check.py"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Python is required for independent HTML validation");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(html.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn technical_changes_keep_both_code_tables_and_rendered_formulas() {
    let old = "```rust\nlet old_value = 1;\n```\n\n| Name | Value |\n|---|---|\n| BeforeCell | 1 |\n\n$$\na^2\n$$\n";
    let new = "```rust\nlet new_value = 2;\n```\n\n| Name | Value |\n|---|---|\n| AfterCell | 2 |\n\n$$\nb^3\n$$\n";
    let html = compare(old, new);
    assert!(html.contains("old_value") && html.contains("new_value"));
    assert!(html.contains("BeforeCell") && html.contains("AfterCell"));
    assert_eq!(html.matches("<pre><code").count(), 2);
    assert_eq!(html.matches("<table>").count(), 2);
    assert_eq!(html.matches("<math ").count(), 2);
    assert!(html.contains("tok-"), "shared syntax highlighting missing");
    assert!(html.contains("diff-block-del") && html.contains("diff-block-ins"));
    check_document(&html);
}

#[test]
fn changed_headings_keep_their_real_levels_and_revision_anchors() {
    let html = compare("# Old title\n", "## New title\n");
    assert!(html.contains("<h1 id=\"diff-old-old-title\">"));
    assert!(html.contains("<h2 id=\"diff-new-new-title\">"));
    assert!(html.contains("<del class=\"diff-inline\">Old title</del>"));
    assert!(html.contains("<ins class=\"diff-inline\">New title</ins>"));
    check_document(&html);
}

#[test]
fn footnotes_retain_revision_definitions_repeated_refs_and_nested_citations() {
    let old =
        "First.[^a]\n\nAgain.[^a]\n\n[^a]: Old note with nested.[^b]\n\n[^b]: Old nested body.\n";
    let new =
        "First.[^a]\n\nAgain.[^a]\n\n[^a]: New note with nested.[^b]\n\n[^b]: New nested body.\n";
    let html = compare(old, new);
    for version in ["old", "new"] {
        assert_eq!(
            html.matches(&format!("id=\"diff-{version}-fnref-1\""))
                .count(),
            1
        );
        for id in ["a", "b"] {
            assert!(html.contains(&format!("id=\"diff-{version}-fn-{id}\"")));
            assert!(html.contains(&format!("href=\"#diff-{version}-fn-{id}\"")));
        }
    }
    for content in ["Old note", "New note", "Old nested body", "New nested body"] {
        assert!(html.contains(content), "lost note content: {content}");
    }
    check_document(&html);
}

#[test]
fn duplicate_headings_and_toc_links_stay_with_their_revision() {
    let source =
        "[[_TOC_]]\n\n# Repeat\n\n[First](#repeat) and [second](#repeat-2).\n\n# Repeat\n\nBody.\n";
    let html = compare(source, source);
    for version in ["old", "new"] {
        for id in ["repeat", "repeat-2"] {
            assert_eq!(
                html.matches(&format!("id=\"diff-{version}-{id}\"")).count(),
                1
            );
            assert!(html.contains(&format!("href=\"#diff-{version}-{id}\"")));
        }
    }
    check_document(&html);
}

#[test]
fn inline_images_math_links_and_hard_breaks_use_the_core_renderer() {
    let old =
        "# Section\n\nBefore ![diagram](figure.svg \"Diagram\")  \n*x* and $a$ [jump](#section).\n";
    let new = "# Section\n\nAfter ![diagram](figure.svg \"Diagram\")  \n**x** and $b$ [jump](#section).\n";
    let opts = HtmlOptions {
        image_assets: vec![PdfImageAsset::new(
            "figure.svg",
            include_bytes!("fixtures/pdf/linked-figure.svg").to_vec(),
        )],
        ..HtmlOptions::default()
    };
    let html = compute_diff(&parse_markdown(old), &parse_markdown(new), "old", "new")
        .to_html_with_options(&opts);
    assert_eq!(
        html.matches("<img src=\"data:image/svg+xml;base64,")
            .count(),
        2
    );
    assert_eq!(html.matches("<br>").count(), 2);
    assert_eq!(html.matches("<math ").count(), 2);
    assert!(html.contains("<em>x</em>") && html.contains("<strong>x</strong>"));
    assert!(!html.contains("[Image:"));
    check_document(&html);
}

#[test]
fn changed_unreferenced_definitions_are_visible_without_fake_backlinks() {
    let html = compare(
        "[^unused]: Old unpublished note.\n",
        "[^unused]: New unpublished note.\n",
    );
    assert!(html.contains("Old unpublished note.") && html.contains("New unpublished note."));
    assert_eq!(html.matches("class=\"diff-note-source\"").count(), 2);
    assert!(!html.contains("class=\"footnote-ref\""));
    check_document(&html);
}

#[test]
fn comparison_reuses_safe_urls_css_and_attribute_escaping() {
    let old =
        "Before [unsafe](javascript:alert(1)) <script>alert(2)</script> ![alt](javascript:bad).\n";
    let new =
        "After [unsafe](javascript:alert(3)) <script>alert(4)</script> ![alt](javascript:bad).\n";
    let opts = HtmlOptions {
        lang: Some("en\" onload=\"bad".into()),
        custom_css: Some(".fmd{color:purple}/*</StYle><script>bad</script>*/".into()),
        ..HtmlOptions::default()
    };
    let diff = compute_diff(
        &parse_markdown(old),
        &parse_markdown(new),
        "<script>before</script>",
        "after\"name",
    );
    let html = diff.to_html_with_options(&opts);
    assert!(html.contains("color:purple"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("href=\"javascript:") && !html.contains("src=\"javascript:"));
    assert_eq!(html, diff.to_html_with_options(&opts));
    check_document(&html);
}
