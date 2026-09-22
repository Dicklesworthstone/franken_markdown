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

#[test]
fn polling_a_markdown_root_alone_discovers_and_refreshes_dependencies() {
    let dir = Directory::new();
    let input = dir.write("guide.MARKDOWN", b"![a][pic]\n\n[pic]: a.png\n");
    let mut watcher = PollWatcher::new(vec![input.clone()], Duration::ZERO, ManualClock::new());
    assert_eq!(watcher.paths(), &[input.clone(), dir.path("a.png")]);
    assert!(watcher.poll().is_empty());
    std::fs::write(&input, "![b](b.png)").unwrap();
    assert_eq!(watcher.poll(), [ChangeEvent { path: input.clone(), kind: ChangeKind::Modified }]);
    assert!(watcher.paths().contains(&dir.path("b.png")));
    assert!(!watcher.paths().contains(&dir.path("a.png")));
    dir.write("a.png", b"no longer referenced");
    assert!(watcher.poll().is_empty());
    let image = dir.write("b.png", b"newly referenced");
    assert_eq!(watcher.poll(), [ChangeEvent { path: image, kind: ChangeKind::Created }]);
}

#[test]
fn shared_dependencies_remain_until_the_last_source_drops_them() {
    let dir = Directory::new();
    let first = dir.write("one.md", b"![s](shared.png)");
    let second = dir.write("two.md", b"![s](shared.png)");
    let shared = dir.write("shared.png", b"shared image");
    let mut watcher = PollWatcher::new(vec![first.clone(), second.clone()], Duration::ZERO, ManualClock::new());
    assert_eq!(watcher.paths().iter().filter(|path| **path == shared).count(), 1);
    std::fs::write(&first, "no image").unwrap();
    watcher.poll();
    assert!(watcher.paths().contains(&shared));
    std::fs::write(&second, "no image").unwrap();
    watcher.poll();
    assert!(!watcher.paths().contains(&shared));
}

#[test]
fn explicitly_supplied_paths_remain_watched_when_not_referenced() {
    let dir = Directory::new();
    let input = dir.write("doc.md", b"![p](p.png)");
    let pinned = dir.write("p.png", b"old");
    let mut watcher = PollWatcher::new(vec![input.clone(), pinned.clone(), input.clone()], Duration::ZERO, ManualClock::new());
    assert_eq!(watcher.paths().iter().filter(|path| **path == input).count(), 1);
    std::fs::write(&input, "no image").unwrap();
    watcher.poll();
    dir.write("p.png", b"new");
    assert_eq!(watcher.poll(), [ChangeEvent { path: pinned, kind: ChangeKind::Modified }]);
}

#[test]
fn linked_markdown_is_observed_but_not_recursively_crawled() {
    let dir = Directory::new();
    dir.write("other.md", b"![private](not-a-root.png)");
    let root = dir.write("doc.md", b"[other](other.md)");
    let mut watcher = PollWatcher::new(vec![root], Duration::ZERO, ManualClock::new());
    assert!(watcher.paths().contains(&dir.path("other.md")));
    assert!(!watcher.paths().contains(&dir.path("not-a-root.png")));
    dir.write("not-a-root.png", b"not authorized by a root");
    assert!(watcher.poll().is_empty());
}

#[test]
fn nested_selected_includes_refresh_assets_without_editing_the_root() {
    let dir = Directory::new();
    std::fs::create_dir(dir.path("parts")).unwrap();
    let input = dir.write("doc.md", b"# Book\n\n{{#include parts/chapter.md:shown}}\n");
    dir.write("parts/chapter.md", b"<!-- ANCHOR: shown -->\n{{#include body.txt}}\n<!-- ANCHOR_END: shown -->\n{{#include ignored.md}}\n");
    let body = dir.write("parts/body.txt", b"![picture](first.png)\n");
    let mut watcher = PollWatcher::new(vec![input], Duration::ZERO, ManualClock::new());
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&body));
    assert!(watcher.paths().contains(&dir.path("first.png")));
    assert!(!watcher.paths().contains(&dir.path("parts/ignored.md")));
    std::fs::write(&body, "![picture](second.png)\n").unwrap();
    assert_eq!(watcher.poll(), [ChangeEvent { path: body, kind: ChangeKind::Modified }]);
    assert!(!watcher.paths().contains(&dir.path("first.png")));
    assert!(watcher.paths().contains(&dir.path("second.png")));
    let image = dir.write("second.png", b"new image");
    assert_eq!(watcher.poll(), [ChangeEvent { path: image, kind: ChangeKind::Created }]);
}

#[test]
fn missing_and_deleted_includes_recover_and_preserve_working_asset_edges() {
    let dir = Directory::new();
    let input = dir.write("doc.md", b"{{#include later.md}}\n");
    let mut watcher = PollWatcher::new(vec![input.clone()], Duration::ZERO, ManualClock::new());
    assert!(watcher.dependency_failures().contains(&input));
    assert!(watcher.paths().contains(&dir.path("later.md")));
    let included = dir.write("later.md", b"![p](before.png)\n");
    assert_eq!(watcher.poll(), [ChangeEvent { path: included.clone(), kind: ChangeKind::Created }]);
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&dir.path("before.png")));
    std::fs::remove_file(&included).unwrap();
    watcher.poll();
    assert!(watcher.dependency_failures().contains(&input));
    assert!(watcher.paths().contains(&dir.path("before.png")));
    std::fs::write(&included, "![p](after.png)\n").unwrap();
    watcher.poll();
    assert!(watcher.dependency_failures().is_empty());
    assert!(!watcher.paths().contains(&dir.path("before.png")));
    assert!(watcher.paths().contains(&dir.path("after.png")));
}

#[test]
fn literal_and_escaping_include_examples_never_become_dependencies() {
    let dir = Directory::new();
    let other = Directory::new();
    let escaped = other.write("secret.md", b"private");
    let source = format!("```md\n{{{{#include literal.md}}}}\n```\n\n    {{{{#include indented.md}}}}\n\n{{{{#include {} }}}}\n", escaped.display());
    let input = dir.write("doc.md", source.as_bytes());
    let watcher = PollWatcher::new(vec![input.clone()], Duration::ZERO, ManualClock::new());
    assert!(watcher.dependency_failures().contains(&input));
    assert_eq!(watcher.paths(), [input]);
}

#[test]
fn invalid_utf8_and_deleted_sources_retain_the_last_graph_until_recovery() {
    let dir = Directory::new();
    let input = dir.write("doc.md", b"![p](p.png)");
    let mut watcher = PollWatcher::new(vec![input.clone()], Duration::ZERO, ManualClock::new());
    std::fs::write(&input, [0xFF, 0xFE]).unwrap();
    watcher.poll();
    assert!(watcher.dependency_failures().contains(&input));
    assert!(watcher.paths().contains(&dir.path("p.png")));
    std::fs::remove_file(&input).unwrap();
    watcher.poll();
    assert!(watcher.paths().contains(&dir.path("p.png")));
    std::fs::write(&input, "![q](q.png)").unwrap();
    watcher.poll();
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&dir.path("q.png")));
    assert!(!watcher.paths().contains(&dir.path("p.png")));
}

#[test]
fn adding_an_explicit_markdown_root_discovers_its_images_without_an_edit() {
    let dir = Directory::new();
    let root = dir.write("doc.md", b"![new](new.png)");
    let mut watcher = PollWatcher::new(Vec::new(), Duration::ZERO, ManualClock::new());
    watcher.add_path(root.clone());
    watcher.add_path(root.clone());
    assert_eq!(watcher.paths(), &[root, dir.path("new.png")]);
    let image = dir.write("new.png", b"new");
    assert_eq!(watcher.poll(), [ChangeEvent { path: image, kind: ChangeKind::Created }]);
}

#[test]
fn creation_bursts_remain_created_and_canceled_creation_does_not_poison_history() {
    let dir = Directory::new();
    let path = dir.path("new.txt");
    let clock = ManualClock::new();
    let mut watcher = PollWatcher::new(vec![path.clone()], Duration::from_millis(300), clock.clone());
    dir.write("new.txt", b"first");
    assert!(watcher.poll().is_empty());
    dir.write("new.txt", b"second");
    assert!(watcher.poll().is_empty());
    std::fs::remove_file(&path).unwrap();
    assert!(watcher.poll().is_empty());
    assert_eq!(watcher.pending_len(), 0);
    dir.write("new.txt", b"third");
    assert!(watcher.poll().is_empty());
    dir.write("new.txt", b"fourth");
    assert!(watcher.poll().is_empty());
    clock.advance(Duration::from_millis(300));
    assert_eq!(watcher.poll(), [ChangeEvent { path, kind: ChangeKind::Created }]);
}

#[test]
fn edits_and_deletions_undone_within_the_window_emit_no_change() {
    let dir = Directory::new();
    let path = dir.write("file.txt", b"baseline");
    let clock = ManualClock::new();
    let mut watcher = PollWatcher::new(vec![path.clone()], Duration::from_millis(300), clock.clone());
    dir.write("file.txt", b"edit");
    watcher.poll();
    std::fs::remove_file(&path).unwrap();
    watcher.poll();
    dir.write("file.txt", b"baseline");
    watcher.poll();
    clock.advance(Duration::from_millis(300));
    assert!(watcher.poll().is_empty());
    assert_eq!(watcher.pending_len(), 0);
}

#[test]
fn transient_source_edit_cannot_hide_a_concurrent_image_change() {
    let dir = Directory::new();
    let source = b"![p](p.png)";
    let input = dir.write("doc.md", source);
    let image = dir.write("p.png", b"baseline image");
    let clock = ManualClock::new();
    let mut watcher = PollWatcher::new(vec![input.clone()], Duration::from_millis(300), clock.clone());
    std::fs::write(&input, "temporarily no image").unwrap();
    dir.write("p.png", b"changed image");
    assert!(watcher.poll().is_empty());
    assert!(watcher.paths().contains(&image));
    std::fs::write(&input, source).unwrap();
    assert!(watcher.poll().is_empty());
    clock.advance(Duration::from_millis(300));
    assert_eq!(watcher.poll(), [ChangeEvent { path: image, kind: ChangeKind::Modified }]);
}
