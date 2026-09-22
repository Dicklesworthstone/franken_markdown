//! Origin-sensitive native authoring: includes, assets, navigation and watch.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::watch::{ChangeEvent, ChangeKind, ManualClock, PollWatcher};
use franken_markdown::{
    HtmlOptions, PdfImageAsset, parse_markdown, render_epub, render_html_document,
};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#2468ac"/></svg>"##;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-transclusion-origins-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .current_dir(&self.0)
            .arg("--no-config")
            .args(args)
            .env_remove("SOURCE_DATE_EPOCH")
            .output()
            .unwrap()
    }
    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}; stderr={}; stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

fn embedded_image() -> String {
    let html = render_html_document(
        &parse_markdown("![fixture](fixture.svg)"),
        &HtmlOptions {
            image_assets: vec![PdfImageAsset::new("fixture.svg", SVG.to_vec())],
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    let start = html.find("data:image/svg+xml;base64,").unwrap();
    let end = html[start..].find('"').unwrap();
    html[start..start + end].to_owned()
}

#[test]
fn included_images_and_links_use_the_defining_file_in_html_and_epub() {
    let dir = Workspace::new();
    dir.write("root.md", "# Root\n\n{{#include parts/chapter.md}}\n");
    dir.write("parts/chapter.md", "## Included\n\n![Chart](chart.svg \"Chart title\")\n\n[Next](next.md?print=1#next \"Link title\")\n");
    dir.write("parts/chart.svg", SVG);
    // A same-named wrong-base file must never override the included asset.
    dir.write("chart.svg", b"wrong asset in the root directory");
    succeeded(&dir.run(&["root.md", "--out", "root.html"]));
    let html = dir.text("root.html");
    assert!(html.contains(&embedded_image()));
    assert!(html.contains("href=\"parts/next.md?print=1#next\""));
    assert!(html.contains("title=\"Chart title\""));
    assert!(html.contains("title=\"Link title\""));
    succeeded(&dir.run(&["root.md", "--to", "epub", "--out", "root.epub"]));
    let expected_source = "# Root\n\n## Included\n\n![Chart](parts/chart.svg \"Chart title\")\n\n[Next](parts/next.md?print=1#next \"Link title\")\n";
    let expected = render_epub(
        &parse_markdown(expected_source),
        &HtmlOptions {
            image_assets: vec![PdfImageAsset::new("parts/chart.svg", SVG.to_vec())],
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(dir.0.join("root.epub")).unwrap(), expected);
}

#[test]
fn nested_anchor_and_line_selectors_keep_the_selected_fragments_origin() {
    let dir = Workspace::new();
    dir.write("root.md", "# Root\n\n{{#include parts/chapter.md:shown}}\n");
    dir.write("parts/chapter.md", "{{#include never-read.md}}\n<!-- ANCHOR: shown -->\n{{#include deep/cards.txt:2:6}}\n<!-- ANCHOR_END: shown -->\n");
    // Selected lines are Markdown in the expanded document even when the
    // original full file surrounds them with a code fence.
    dir.write(
        "parts/deep/cards.txt",
        "```md\n## Gallery\n\n![Chart](plot.svg#view)\n\n[Next](../next.md#target)\n```\n",
    );
    dir.write("parts/deep/plot.svg", SVG);
    succeeded(&dir.run(&["root.md", "--out", "root.html"]));
    let html = dir.text("root.html");
    assert!(html.contains(&embedded_image()));
    assert!(html.contains("href=\"parts/next.md#target\""));
    assert!(!html.contains("never-read"));
}

#[test]
fn reference_definitions_keep_their_origin_for_root_and_included_uses() {
    let dir = Workspace::new();
    dir.write(
        "root.md",
        "# Root\n\n![Root use][chart]\n\n{{#include parts/shared.md}}\n",
    );
    dir.write("parts/shared.md", "![Included use][chart]\n\n[chart]: <figure.svg?raw=1#panel> 'Shared title'\n\n`![literal](figure.svg)`\n\n```text\n[code](next.md)\n```\n\n<div>\n[html literal](next.md)\n</div>\n");
    dir.write("parts/figure.svg", SVG);
    succeeded(&dir.run(&["root.md", "--allow-html", "--out", "root.html"]));
    let html = dir.text("root.html");
    assert_eq!(html.matches(&embedded_image()).count(), 2);
    assert_eq!(html.matches("title=\"Shared title\"").count(), 2);
    assert!(html.contains("![literal](figure.svg)"));
    assert!(html.contains("[code](next.md)"));
    assert!(html.contains("[html literal](next.md)"));
    assert!(!html.contains("[code](parts/next.md)"));
}

#[test]
fn directory_names_and_destinations_are_uri_decoded_exactly_once_for_assets() {
    let dir = Workspace::new();
    dir.write(
        "root.md",
        "# Root\n\n{{#include \"parts #中/shared.md\"}}\n",
    );
    dir.write(
        "parts #中/shared.md",
        "![Chart](percent%2520%23plot.svg?raw=1#view)\n",
    );
    dir.write("parts #中/percent%20#plot.svg", SVG);
    succeeded(&dir.run(&["root.md", "--out", "root.html"]));
    assert!(dir.text("root.html").contains(&embedded_image()));
    // The same decoding policy applies to a direct file-input image.
    dir.write(
        "direct.md",
        "![Chart](parts%20%23%E4%B8%AD/percent%2520%23plot.svg)\n",
    );
    succeeded(&dir.run(&["direct.md", "--out", "direct.html"]));
    assert!(dir.text("direct.html").contains(&embedded_image()));
}

#[test]
fn watch_tracks_the_resolved_include_assets_and_refreshes_them_on_save() {
    let dir = Workspace::new();
    let root = dir.write("root.md", "# Root\n\n{{#include parts/chapter.md}}\n");
    let included = dir.write(
        "parts/chapter.md",
        "![Chart](first.svg)\n\n[Next](next.md)\n",
    );
    let linked = dir.write("parts/next.md", "# Next\n");
    let first = dir.0.join("parts/first.svg");
    let mut watcher = PollWatcher::new(vec![root.clone()], Duration::ZERO, ManualClock::new());
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&first));
    assert!(watcher.paths().contains(&linked));
    assert!(!watcher.paths().contains(&dir.0.join("first.svg")));
    dir.write("parts/first.svg", SVG);
    assert_eq!(
        watcher.poll(),
        [ChangeEvent {
            path: first.clone(),
            kind: ChangeKind::Created
        }]
    );
    dir.write("parts/chapter.md", "![Chart](second.svg)\n");
    assert_eq!(
        watcher.poll(),
        [ChangeEvent {
            path: included,
            kind: ChangeKind::Modified
        }]
    );
    assert!(!watcher.paths().contains(&first));
    assert!(!watcher.paths().contains(&linked));
    assert!(watcher.paths().contains(&dir.0.join("parts/second.svg")));
}

#[test]
fn book_checks_and_publication_resolve_included_links_against_their_source() {
    let dir = Workspace::new();
    dir.write(
        "book/chapters/start.md",
        "# Start\n\n{{#include ../parts/shared.md}}\n",
    );
    dir.write(
        "book/parts/shared.md",
        "![Chart](chart.svg)\n\n[Next](next.md#target) [Guide](../guide/#guide)\n",
    );
    dir.write("book/parts/next.md", "# Target\n");
    dir.write("book/parts/chart.svg", SVG);
    dir.write("book/guide/index.md", "# Guide\n");
    dir.write("book/book.toml", "order=['chapters/start.md','parts/next.md','guide/index.md']\ninclude_only=['parts/shared.md']\n");
    let checked = dir.run(&["book", "book", "--check-links", "--json"]);
    succeeded(&checked);
    assert!(String::from_utf8_lossy(&checked.stdout).contains("\"findings\":0"));
    assert!(!dir.0.join("book-site").exists());
    succeeded(&dir.run(&[
        "book",
        "book",
        "--deny-broken-links",
        "--to",
        "html",
        "--out-dir",
        "site",
        "--json",
    ]));
    let html = dir.text("site/chapters__start.html");
    assert!(html.contains("href=\"parts__next.html#target\""));
    assert!(html.contains("href=\"guide__index.html#guide\""));
    assert!(html.contains(&embedded_image()));
    assert!(!dir.0.join("site/parts__shared.html").exists());
}

#[test]
fn encoded_parent_traversal_does_not_escape_the_document_asset_root() {
    let dir = Workspace::new();
    dir.write("outside.svg", SVG);
    dir.write(
        "document/root.md",
        "![outside](%2e%2e/outside.svg)\n\n{{#include parts/shared.md}}\n",
    );
    dir.write(
        "document/parts/shared.md",
        "![outside](%2e%2e/%2e%2e/outside.svg)\n",
    );
    succeeded(&dir.run(&["document/root.md", "--out", "root.html"]));
    assert!(!dir.text("root.html").contains("data:image/svg+xml;base64,"));
}
