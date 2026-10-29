//! TOCs have one document owner across containers and diff fragments.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, html, parse_markdown};

fn fragment(source: &str, automatic: bool) -> String {
    html::render_fragment(
        &parse_markdown(source).blocks,
        &HtmlOptions {
            toc: automatic,
            ..HtmlOptions::default()
        },
    )
}

fn navs(html: &str) -> Vec<&str> {
    html.split("<nav class=\"toc\">")
        .skip(1)
        .map(|tail| tail.split_once("</nav>").unwrap().0)
        .collect()
}

fn targets(nav: &str) -> Vec<&str> {
    nav.split("href=\"#")
        .skip(1)
        .map(|tail| tail.split_once('"').unwrap().0)
        .collect()
}

#[test]
fn automatic_toc_is_once_per_body_with_nested_and_footnote_headings() {
    let source = "# Same\n\n> ## Same\n> Quoted body.\n\n- ### Same-2\n\n## Same\n\nCitation.[^n]\n\n[^n]: ### Same\n\n    Note body.\n";
    let html = fragment(source, true);
    let navs = navs(&html);
    assert_eq!(navs.len(), 1, "recursive containers must not add TOCs");
    let ids = ["same", "same-2", "same-2-2", "same-3"];
    assert_eq!(targets(navs[0]), ids);
    for id in ids.into_iter().chain(["same-4"]) {
        assert_eq!(html.matches(&format!("id=\"{id}\"")).count(), 1);
    }
    assert!(html.contains("Quoted body.") && html.contains("Note body."));
}

#[test]
fn a_nested_first_heading_consumes_the_only_automatic_toc() {
    for container in ["> ## First\n> Body.\n", "- ## First\n\n  Body.\n"] {
        let html = fragment(&format!("{container}\n# Last\n"), true);
        let navs = navs(&html);
        assert_eq!(navs.len(), 1);
        assert_eq!(targets(navs[0]), ["first", "last"]);
        assert!(html.find("<nav ").unwrap() < html.find("id=\"first\"").unwrap());
    }
}

#[test]
fn nested_explicit_markers_control_placement_and_depth_for_the_whole_body() {
    for automatic in [false, true] {
        let source = "# Main\n\n> [[TOC:2]]\n>\n> ## Quoted\n\n- ### List\n\n## End\n";
        let html = fragment(source, automatic);
        let navs = navs(&html);
        assert_eq!(navs.len(), 1);
        assert_eq!(targets(navs[0]), ["main", "quoted", "end"]);
        assert!(html.find("<blockquote>").unwrap() < html.find("<nav ").unwrap());
        assert!(!html.contains("[[TOC"));
    }
}

#[test]
fn tight_loose_and_nested_list_markers_render_navigation() {
    for marker in [
        "- [[TOC]]\n",
        "- Intro.\n\n  [[TOC]]\n",
        "- Intro.\n  - [[TOC]]\n",
        "> - [TOC]\n",
    ] {
        for automatic in [false, true] {
            let html = fragment(&format!("# Start\n\n{marker}\n## End\n"), automatic);
            let navs = navs(&html);
            assert_eq!(navs.len(), 1, "marker {marker:?}");
            assert_eq!(targets(navs[0]), ["start", "end"]);
            assert!(html.find("<li>").unwrap() < html.find("<nav ").unwrap());
            assert!(!html.contains("[TOC]"));
        }
    }
}

#[test]
fn note_markers_use_the_body_outline_without_owning_body_placement() {
    let source =
        "# Main\n\nCitation.[^n]\n\n[^n]: [[TOC:1]]\n\n    ## Note heading\n\n    Note body.\n";
    let html = fragment(source, false);
    let note_navs = navs(&html);
    assert_eq!(note_navs.len(), 1);
    assert_eq!(targets(note_navs[0]), ["main"]);
    assert!(html.find("class=\"footnotes\"").unwrap() < html.find("<nav ").unwrap());
    // Unreferenced definitions are not rendered by HTML and cannot take away
    // an explicitly requested automatic TOC in the document body.
    let html = fragment("# Main\n\n[^unused]: [[TOC]]\n", true);
    assert_eq!(navs(&html).len(), 1);
    assert!(!html.contains("class=\"footnotes\""));
    let html = fragment("Citation.[^n]\n\n[^n]: # Only a note\n", true);
    assert!(navs(&html).is_empty());
}

#[test]
fn multiple_explicit_markers_remain_intentional_and_code_stays_literal() {
    let html = fragment("[[TOC]]\n\n# Main\n\n> [[TOC]]\n", true);
    assert_eq!(navs(&html).len(), 2);
    let html = fragment("`[[TOC]]`\n\n```text\n[[TOC]]\n```\n\n# Main\n", true);
    assert_eq!(navs(&html).len(), 1);
    assert!(html.contains("<code>[[TOC]]</code>"));
    let html = fragment("# Main\n\n> ## Nested\n", false);
    assert!(navs(&html).is_empty());
}

#[test]
fn diff_fragments_share_automatic_toc_state_and_revision_anchor_prefixes() {
    let old = parse_markdown("> ## First\n> Before.\n\n# Last\n");
    let new = parse_markdown("> ## First\n> After.\n\n# Last\n");
    let html = franken_markdown::diff::compute_diff(&old, &new, "old", "new").to_html_with_options(
        &HtmlOptions {
            toc: true,
            custom_css: Some(String::new()),
            ..HtmlOptions::default()
        },
    );
    let navs = navs(&html);
    assert_eq!(navs.len(), 2, "one TOC per complete revision");
    let all: Vec<_> = navs.into_iter().flat_map(targets).collect();
    for version in ["old", "new"] {
        for id in ["first", "last"] {
            let full = format!("diff-{version}-{id}");
            assert_eq!(all.iter().filter(|&&target| target == full).count(), 1);
            assert_eq!(html.matches(&format!("id=\"{full}\"")).count(), 1);
        }
    }
}
