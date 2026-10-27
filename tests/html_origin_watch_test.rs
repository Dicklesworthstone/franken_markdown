//! Included HTML resources must retain their defining file's watch ownership.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::watch::{ChangeKind, ManualClock, PollWatcher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
const IMAGE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><rect width="80" height="40" fill="#22aa44"/></svg>"##;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-html-origin-watch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct RunningWatch(Child);
impl Drop for RunningWatch {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn refuse_in_place_export(root: &Path, log: &Path) {
    let stderr = std::fs::File::create(log).unwrap();
    let mut watch = RunningWatch(
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "watch"])
            .arg(root)
            .arg("--out")
            .arg(root)
            .args(["--to", "svg", "--interval", "10", "--json"])
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = watch.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(64));
            assert!(
                std::fs::read_to_string(log)
                    .unwrap()
                    .contains("refusing to overwrite watched input")
            );
            return;
        }
        let events = std::fs::read_to_string(log).unwrap();
        assert!(
            !events.contains("\"event\":\"watching\""),
            "unsafe watch reached publication: {events}"
        );
        assert!(Instant::now() < deadline, "watch did not refuse: {events}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn included_html_images_reserve_existing_and_missing_inputs_before_directory_exports() {
    for (present, html) in [
        (true, "<img src=chart.svg alt='Chart'>\n"),
        (false, "<img src=chart.svg alt='Chart'>\n"),
        (true, "<img src=\"\nchart.svg\n\" alt='Chart'>\n"),
        (false, "<img src=\"\nchart.svg\n\" alt='Chart'>\n"),
    ] {
        let dir = Directory::new();
        let root = dir.0.join("sources");
        dir.write(
            "sources/root.md",
            "# Root\n\n{{#include parts/snippet.txt}}\n",
        );
        // The included source deliberately has no Markdown extension. It must
        // gain resource ownership through its parent, not as a discovered root.
        dir.write("sources/parts/snippet.txt", html);
        dir.write("sources/parts/chart.md", "# A different document\n");
        let wrong_base = dir.write("sources/chart.svg", "wrong base must stay untouched");
        let actual = root.join("parts/chart.svg");
        if present {
            dir.write("sources/parts/chart.svg", IMAGE);
        }

        refuse_in_place_export(&root, &dir.0.join("watch.log"));

        assert!(
            !root.join("root.svg").exists(),
            "admission precedes every write"
        );
        assert_eq!(
            std::fs::read_to_string(wrong_base).unwrap(),
            "wrong base must stay untouched"
        );
        if present {
            assert_eq!(std::fs::read_to_string(actual).unwrap(), IMAGE);
        } else {
            assert!(
                !actual.exists(),
                "an export must not create a missing input"
            );
        }
    }
}

#[test]
fn included_html_images_track_their_real_paths_and_refresh_without_tracking_code() {
    let dir = Directory::new();
    let root = dir.write("root.md", "# Root\n\n{{#include parts/snippet.txt}}\n");
    let snippet = dir.write(
        "parts/snippet.txt",
        "<img src='chart%20one.svg?raw=1&amp;x=2#panel' alt='Chart'>\n\n```html\n<img src='literal.svg'>\n```\n",
    );
    let wrong = dir.write("chart one.svg", IMAGE);
    dir.write("parts/literal.svg", IMAGE);
    let actual = dir.0.join("parts/chart one.svg");
    let mut watcher = PollWatcher::new(vec![root.clone()], Duration::ZERO, ManualClock::new());
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&actual));
    assert!(!watcher.paths().contains(&wrong));
    assert!(!watcher.paths().contains(&dir.0.join("parts/literal.svg")));

    std::fs::write(&wrong, "wrong-base edit").unwrap();
    assert!(watcher.poll().is_empty());
    std::fs::write(&actual, IMAGE).unwrap();
    let events = watcher.poll();
    assert!(
        events
            .iter()
            .any(|event| event.path == actual && event.kind == ChangeKind::Created)
    );

    std::fs::write(&snippet, "<img src='replacement.svg' alt='Replacement'>\n").unwrap();
    assert!(watcher.poll().iter().any(|event| event.path == snippet));
    assert!(!watcher.paths().contains(&actual));
    let replacement = dir.0.join("parts/replacement.svg");
    assert!(watcher.paths().contains(&replacement));
    std::fs::write(&actual, "the old resource is no longer referenced").unwrap();
    assert!(watcher.poll().is_empty());
    std::fs::write(&replacement, IMAGE).unwrap();
    assert!(
        watcher
            .poll()
            .iter()
            .any(|event| event.path == replacement && event.kind == ChangeKind::Created)
    );
}
