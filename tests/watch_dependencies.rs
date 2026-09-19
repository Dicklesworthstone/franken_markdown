//! Real parser + filesystem coverage for native preview dependencies.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::watch::{
    ChangeEvent, ChangeKind, ManualClock, PollWatcher, collect_watch_paths, referenced_local_paths,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-watch-dependencies-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf { self.0.join(name) }
    fn write(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

#[test]
fn full_collapsed_shortcut_and_inline_images_share_one_dependency() {
    let dir = Directory::new();
    let source = "![first][figure] ![figure][] ![figure] ![last](figure.png)\n\n[figure]: figure.png \"title\"\n";
    assert_eq!(referenced_local_paths(source, &dir.0), [dir.path("figure.png")]);
}

#[test]
fn shared_ast_finds_images_in_headings_quotes_lists_tables_and_link_content() {
    let dir = Directory::new();
    let source = "# ![heading](head.png)\n\n> **![quote](quote.png)**\n\n- *![list](list.png)*\n\n| H |\n| --- |\n| ~~![table](table.png)~~ |\n\n[![linked](linked.png)](https://example.com)\n";
    assert_eq!(referenced_local_paths(source, &dir.0), [
        dir.path("head.png"), dir.path("quote.png"), dir.path("list.png"),
        dir.path("table.png"), dir.path("linked.png"),
    ]);
}

#[test]
fn code_examples_unused_definitions_and_html_do_not_create_dependencies() {
    let dir = Directory::new();
    for name in ["inline.png", "fenced.png", "indented.png", "html.png", "unused.png"] {
        dir.write(name, b"not a render dependency");
    }
    let source = "`![x](inline.png)`\n\n```md\n![x](fenced.png)\n```\n\n    ![x](indented.png)\n\n<img src=\"html.png\">\n\n[unused]: unused.png\n\n![real](real.png)\n";
    assert_eq!(referenced_local_paths(source, &dir.0), [dir.path("real.png")]);
}

#[test]
fn escaped_parentheses_spaces_unicode_and_url_suffixes_resolve_to_native_paths() {
    let dir = Directory::new();
    let source = "![a](plot\\(one\\).png \"caption\") ![b](<space name.png>) ![c](caf%C3%A9%20plot.svg?v=1#panel) ![d](name%23part.png#view)\n";
    assert_eq!(referenced_local_paths(source, &dir.0), [
        dir.path("plot(one).png"), dir.path("space name.png"),
        dir.path("café plot.svg"), dir.path("name#part.png"),
    ]);
}

#[test]
fn existing_local_links_are_retained_but_missing_links_and_remote_assets_are_not() {
    let dir = Directory::new();
    let linked = dir.write("chapter.md", b"# Chapter\n");
    let source = "[chapter](chapter.md?view=1#part) [missing](absent.md) ![remote](https://example.com/a.png) ![unc](//host/a.png) ![missing image](missing.png)";
    assert_eq!(referenced_local_paths(source, &dir.0), [linked, dir.path("missing.png")]);
}

#[test]
fn directory_targets_are_not_fingerprinted_or_reported_as_assets() {
    let dir = Directory::new();
    let target = dir.path("image.png");
    std::fs::create_dir(&target).unwrap();
    assert!(referenced_local_paths("![directory](image.png)", &dir.0).is_empty());
    let clock = ManualClock::new();
    let mut watcher = PollWatcher::new(vec![target], Duration::ZERO, clock);
    assert!(watcher.poll().is_empty());
}

#[test]
fn missing_reference_image_creation_rebuilds_without_a_source_edit() {
    let dir = Directory::new();
    let source = "# Figure\n\n![caption][picture]\n\n[picture]: later.png\n";
    let input = dir.write("doc.md", source.as_bytes());
    let clock = ManualClock::new();
    let paths = collect_watch_paths(&input, &referenced_local_paths(source, &dir.0));
    let mut watcher = PollWatcher::new(paths, Duration::from_millis(300), clock.clone());
    assert!(watcher.poll().is_empty());
    let image = dir.write("later.png", b"new image bytes");
    assert!(watcher.poll().is_empty());
    clock.advance(Duration::from_millis(300));
    assert_eq!(watcher.poll(), [ChangeEvent { path: image, kind: ChangeKind::Created }]);
    assert_eq!(std::fs::read_to_string(input).unwrap(), source);
}

#[test]
fn reference_image_edits_atomic_saves_removal_and_recovery_are_observed() {
    let dir = Directory::new();
    let image = dir.write("figure.png", b"original");
    let paths = referenced_local_paths("![figure][]\n\n[figure]: figure.png\n", &dir.0);
    let mut watcher = PollWatcher::new(paths, Duration::ZERO, ManualClock::new());
    let temp = dir.write("save.tmp", b"replacement");
    std::fs::rename(temp, &image).unwrap();
    assert_eq!(watcher.poll(), [ChangeEvent { path: image.clone(), kind: ChangeKind::Modified }]);
    std::fs::remove_file(&image).unwrap();
    assert_eq!(watcher.poll(), [ChangeEvent { path: image.clone(), kind: ChangeKind::Removed }]);
    dir.write("figure.png", b"restored");
    assert_eq!(watcher.poll(), [ChangeEvent { path: image, kind: ChangeKind::Modified }]);
    assert!(watcher.poll().is_empty());
}

#[test]
fn unicode_parent_directories_are_preserved_without_canonicalizing_missing_files() {
    let base = Path::new("guide/chapters");
    assert_eq!(referenced_local_paths("![p](../images/caf%C3%A9.png)", base),
        [base.join("../images/café.png")]);
}
