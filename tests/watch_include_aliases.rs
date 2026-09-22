//! Include aliases invalidate their canonical contents and nested origins.
#![cfg(all(feature = "cli", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::watch::{ChangeEvent, ChangeKind, ManualClock, PollWatcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-watch-include-alias-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    fn replace_alias(&self, name: &str, target: impl AsRef<Path>) -> PathBuf {
        let alias = self.path(name);
        let temporary = self.path(&format!(
            "alias-save-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::os::unix::fs::symlink(target, &temporary).unwrap();
        std::fs::rename(temporary, &alias).unwrap();
        alias
    }

    fn versions(&self) {
        self.write("a/chapter.txt", "{{#include body.txt}}\n");
        self.write("b/chapter.txt", "{{#include body.txt}}\n");
        self.write("a/body.txt", "![first](first.png)\n");
        self.write("b/body.txt", "![second](second.png)\n");
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_modified(events: &[ChangeEvent], path: &Path) {
    assert!(
        events
            .iter()
            .any(|event| event.path == path && event.kind == ChangeKind::Modified),
        "missing modification for {}: {events:?}",
        path.display()
    );
}

#[test]
fn identical_file_alias_targets_refresh_nested_origins_after_debounce() {
    let dir = Directory::new();
    dir.versions();
    let source = "{{#include selected.txt}}\n";
    let input = dir.write("doc.md", source);
    let alias = dir.replace_alias("selected.txt", "a/chapter.txt");
    let clock = ManualClock::new();
    let debounce = Duration::from_millis(300);
    let mut watcher = PollWatcher::new(vec![input.clone()], debounce, clock.clone());
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&alias));
    assert!(watcher.paths().contains(&dir.path("a/body.txt")));
    assert!(watcher.paths().contains(&dir.path("a/first.png")));
    assert!(watcher.poll().is_empty());

    dir.replace_alias("selected.txt", "b/chapter.txt");
    assert!(watcher.poll().is_empty());
    assert!(watcher.paths().contains(&dir.path("a/body.txt")));
    clock.advance(debounce);
    assert_modified(&watcher.poll(), &alias);
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&dir.path("b/body.txt")));
    assert!(watcher.paths().contains(&dir.path("b/second.png")));
    assert!(!watcher.paths().contains(&dir.path("a/body.txt")));
    assert!(!watcher.paths().contains(&dir.path("a/first.png")));

    dir.write("a/body.txt", "![unused](unused.png)\n");
    assert!(watcher.poll().is_empty());
    let body = dir.write("b/body.txt", "![third](third.png)\n");
    assert!(watcher.poll().is_empty());
    clock.advance(debounce);
    assert_modified(&watcher.poll(), &body);
    assert!(watcher.paths().contains(&dir.path("b/third.png")));
    assert!(!watcher.paths().contains(&dir.path("b/second.png")));
    assert_eq!(std::fs::read_to_string(input).unwrap(), source);
}

#[test]
fn retargeting_a_parent_directory_alias_refreshes_included_assets() {
    let dir = Directory::new();
    dir.versions();
    let input = dir.write("doc.md", "{{#include selected/chapter.txt}}\n");
    let alias = dir.replace_alias("selected", "a");
    let mut watcher = PollWatcher::new(vec![input], Duration::ZERO, ManualClock::new());
    assert!(watcher.paths().contains(&alias));
    assert!(watcher.paths().contains(&dir.path("a/body.txt")));
    assert!(watcher.poll().is_empty());

    dir.replace_alias("selected", "b");
    assert_modified(&watcher.poll(), &alias);
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&dir.path("b/body.txt")));
    assert!(watcher.paths().contains(&dir.path("b/second.png")));
    assert!(!watcher.paths().contains(&dir.path("a/body.txt")));
    assert!(!watcher.paths().contains(&dir.path("a/first.png")));
    assert!(watcher.poll().is_empty());
}

#[test]
fn escaping_alias_repairs_without_root_edits_or_external_content_watches() {
    let dir = Directory::new();
    let outside = Directory::new();
    let external = outside.write("chapter.txt", "![external](private.png)\n");
    let source = "{{#include selected.txt}}\n";
    let input = dir.write("doc.md", source);
    let inside = dir.write("inside.txt", "![inside](inside.png)\n");
    let alias = dir.replace_alias("selected.txt", &external);
    let mut watcher = PollWatcher::new(vec![input.clone()], Duration::ZERO, ManualClock::new());
    assert!(watcher.dependency_failures().contains(&input));
    assert!(watcher.paths().contains(&alias));
    assert!(!watcher.paths().contains(&external));
    assert!(watcher.paths().iter().all(|path| path.starts_with(&dir.0)));
    assert!(watcher.poll().is_empty());

    outside.write("chapter.txt", "external target changed\n");
    assert!(watcher.poll().is_empty());
    dir.replace_alias("selected.txt", "inside.txt");
    assert_modified(&watcher.poll(), &alias);
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&inside));
    assert!(watcher.paths().contains(&dir.path("inside.png")));
    assert!(!watcher.paths().contains(&external));
    assert_eq!(std::fs::read_to_string(input).unwrap(), source);
    assert!(watcher.poll().is_empty());
}

#[test]
fn an_alias_named_like_a_file_type_still_invalidates_on_regular_replacement() {
    let dir = Directory::new();
    let input = dir.write("doc.md", "{{#include selected.txt}}\n");
    let original = dir.write("regular-file", "old content\n");
    let alias = dir.replace_alias("selected.txt", "regular-file");
    let mut watcher = PollWatcher::new(vec![input], Duration::ZERO, ManualClock::new());
    assert!(watcher.paths().contains(&original));
    assert!(watcher.poll().is_empty());

    // A Unix symlink to "regular-file" has length 12. The replacement has
    // that same length, so a fingerprint must also distinguish entry types.
    let replacement = dir.write("regular-save.txt", "replacement!");
    assert_eq!(std::fs::symlink_metadata(&alias).unwrap().len(), 12);
    assert_eq!(std::fs::metadata(&replacement).unwrap().len(), 12);
    std::fs::rename(replacement, &alias).unwrap();
    assert_modified(&watcher.poll(), &alias);
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&alias));
    assert!(!watcher.paths().contains(&original));
}
