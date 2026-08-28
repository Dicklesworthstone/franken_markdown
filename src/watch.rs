//! Std-only poll watcher for native `fmd watch` (j3e0.1).
//!
//! The render core stays synchronous and filesystem-free. This module is
//! compiled only with the `cli` feature: it stats/reads caller-supplied paths,
//! hashes contents, and emits debounce-coalesced change events. No `notify`
//! crate, no extra dependencies, no threads of its own — the CLI loop sleeps
//! and calls [`PollWatcher::poll`].

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Default poll/debounce window in milliseconds (`--interval`).
pub const DEFAULT_INTERVAL_MS: u64 = 300;

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Source of "now" so tests can drive debounce without sleeping.
pub trait Clock {
    fn now(&self) -> Instant;
}

/// Wall clock used by the CLI loop.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Test clock: clones share an offset so the watcher and the test advance
/// together.
#[derive(Clone)]
pub struct FakeClock {
    origin: Instant,
    offset: Rc<Cell<Duration>>,
}

impl FakeClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
            offset: Rc::new(Cell::new(Duration::ZERO)),
        }
    }

    pub fn advance(&self, d: Duration) {
        self.offset.set(self.offset.get().saturating_add(d));
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.origin + self.offset.get()
    }
}

/// What changed on a watched path after the debounce quiet window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEvent {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

/// Kind of filesystem change relative to the last emitted fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    hash: u64,
}

/// Poll-based watcher: content hash, not inode, so write-temp-then-rename
/// (atomic save) looks like a modification of the destination path.
pub struct PollWatcher<C> {
    paths: Vec<PathBuf>,
    debounce: Duration,
    clock: C,
    fingerprints: BTreeMap<PathBuf, Option<Fingerprint>>,
    ever_seen: BTreeSet<PathBuf>,
    pending: BTreeMap<PathBuf, (ChangeKind, Instant)>,
}

impl<C: Clock> PollWatcher<C> {
    /// Prime fingerprints without emitting. Existing files are not "created"
    /// on the first [`poll`].
    #[must_use]
    pub fn new(paths: Vec<PathBuf>, debounce: Duration, clock: C) -> Self {
        let mut watcher = Self {
            paths,
            debounce,
            clock,
            fingerprints: BTreeMap::new(),
            ever_seen: BTreeSet::new(),
            pending: BTreeMap::new(),
        };
        watcher.scan(false);
        watcher
    }

    /// Watch an additional path (local CSS/image discovered after a rebuild).
    pub fn add_path(&mut self, path: PathBuf) {
        if !self.paths.iter().any(|p| p == &path) {
            let fp = fingerprint(&path);
            if fp.is_some() {
                self.ever_seen.insert(path.clone());
            }
            self.fingerprints.insert(path.clone(), fp);
            self.paths.push(path);
        }
    }

    /// Paths currently in the watch set, in insertion order.
    #[must_use]
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Pending (not yet debounced) change count — for verbose logs.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// One poll. Events fire only after [`debounce`] of quiet on that path.
    pub fn poll(&mut self) -> Vec<ChangeEvent> {
        self.scan(true)
    }

    fn scan(&mut self, emit: bool) -> Vec<ChangeEvent> {
        let now = self.clock.now();
        let paths = self.paths.clone();
        for path in &paths {
            let fp = fingerprint(path);
            let prev = self.fingerprints.get(path).copied().flatten();
            let had_file = self.ever_seen.contains(path);
            self.fingerprints.insert(path.clone(), fp);
            if fp.is_some() {
                self.ever_seen.insert(path.clone());
            }
            if !emit {
                continue;
            }
            let kind = match (prev, fp) {
                (None, Some(_)) if had_file => ChangeKind::Modified,
                (None, Some(_)) => ChangeKind::Created,
                (Some(_), None) => ChangeKind::Removed,
                (Some(a), Some(b)) if a != b => ChangeKind::Modified,
                _ => continue,
            };
            self.pending.insert(path.clone(), (kind, now));
        }

        let debounce = self.debounce;
        let mut out = Vec::new();
        self.pending.retain(|path, (kind, t)| {
            let waited = now.checked_duration_since(*t).unwrap_or(Duration::ZERO);
            if waited >= debounce {
                out.push(ChangeEvent {
                    path: path.clone(),
                    kind: *kind,
                });
                false
            } else {
                true
            }
        });
        out
    }
}

/// Deduplicate `input` plus extra CSS/asset paths, preserving order.
#[must_use]
pub fn collect_watch_paths(input: &Path, extras: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(extras.len() + 1);
    push_unique(&mut out, input.to_path_buf());
    for extra in extras {
        push_unique(&mut out, extra.clone());
    }
    out
}

/// Local `](dest)` / image destinations that exist as files under `base_dir`.
#[must_use]
pub fn referenced_local_paths(markdown: &str, base_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            i += 2;
            let start = i;
            while i < bytes.len() && !matches!(bytes[i], b')' | b'\n' | b' ' | b'"') {
                i += 1;
            }
            if let Ok(dest) = std::str::from_utf8(&bytes[start..i]) {
                let dest = dest.trim();
                if is_local_dest(dest) {
                    let path = base_dir.join(dest);
                    if path.is_file() {
                        push_unique(&mut out, path);
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

fn push_unique(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !out.iter().any(|p| p == &path) {
        out.push(path);
    }
}

fn is_local_dest(dest: &str) -> bool {
    if dest.is_empty() || dest.starts_with('#') {
        return false;
    }
    let lower = dest.to_ascii_lowercase();
    !lower.contains("://") && !lower.starts_with("mailto:") && !lower.starts_with("data:")
}

fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 8192];
    let mut hash = FNV_OFFSET;
    let mut len = 0u64;
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return None,
        };
        len = len.saturating_add(n as u64);
        for &b in &buf[..n] {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    Some(Fingerprint { len, hash })
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn log_check(id: &str, subject: &str, ok: bool) {
        eprintln!(
            "check id={id} subject={subject} outcome={}",
            if ok { "PASS" } else { "FAIL" }
        );
        assert!(ok, "{id}: {subject}");
    }

    fn fresh_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fmd-watch-{tag}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn primed(path: &Path, debounce: Duration, clock: FakeClock) -> PollWatcher<FakeClock> {
        PollWatcher::new(vec![path.to_path_buf()], debounce, clock)
    }

    #[test]
    fn in_place_write_emits_modified_after_debounce() {
        let dir = fresh_dir("in-place");
        let path = dir.join("doc.md");
        std::fs::write(&path, "# hi\n").unwrap();
        let clock = FakeClock::new();
        let mut w = primed(&path, Duration::from_millis(300), clock.clone());
        log_check(
            "j3e0.1.in_place.prime",
            "no event on prime",
            w.poll().is_empty(),
        );

        std::fs::write(&path, "# hello\n").unwrap();
        let immediate = w.poll();
        log_check(
            "j3e0.1.in_place.debounce",
            "no event before quiet window",
            immediate.is_empty(),
        );

        clock.advance(Duration::from_millis(300));
        let events = w.poll();
        log_check(
            "j3e0.1.in_place.emit",
            "modified after debounce",
            events
                == [ChangeEvent {
                    path: path.clone(),
                    kind: ChangeKind::Modified,
                }],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_into_place_is_modified_not_created() {
        let dir = fresh_dir("rename");
        let path = dir.join("doc.md");
        let tmp = dir.join("doc.md.tmp");
        std::fs::write(&path, "old\n").unwrap();
        let clock = FakeClock::new();
        let mut w = primed(&path, Duration::ZERO, clock.clone());

        std::fs::write(&tmp, "new\n").unwrap();
        std::fs::rename(&tmp, &path).unwrap();
        let events = w.poll();
        log_check(
            "j3e0.1.rename.kind",
            "atomic save is Modified",
            events.len() == 1 && events[0].kind == ChangeKind::Modified && events[0].path == path,
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn touch_without_content_change_is_silent() {
        let dir = fresh_dir("touch");
        let path = dir.join("doc.md");
        std::fs::write(&path, "same\n").unwrap();
        let clock = FakeClock::new();
        let mut w = primed(&path, Duration::ZERO, clock.clone());

        let file = std::fs::File::options().write(true).open(&path).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        file.set_modified(later).unwrap();
        drop(file);

        let events = w.poll();
        log_check(
            "j3e0.1.touch.silent",
            "mtime-only touch is not a change",
            events.is_empty(),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rapid_writes_coalesce_to_one_event() {
        let dir = fresh_dir("coalesce");
        let path = dir.join("doc.md");
        std::fs::write(&path, "a\n").unwrap();
        let clock = FakeClock::new();
        let mut w = primed(&path, Duration::from_millis(300), clock.clone());

        std::fs::write(&path, "b\n").unwrap();
        let _ = w.poll();
        clock.advance(Duration::from_millis(100));
        std::fs::write(&path, "c\n").unwrap();
        let _ = w.poll();
        clock.advance(Duration::from_millis(300));
        let events = w.poll();
        log_check(
            "j3e0.1.coalesce",
            "one event after quiet",
            events.len() == 1 && events[0].kind == ChangeKind::Modified,
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_then_created_emits_created() {
        let dir = fresh_dir("create");
        let path = dir.join("doc.md");
        let clock = FakeClock::new();
        let mut w = primed(&path, Duration::ZERO, clock.clone());
        std::fs::write(&path, "new\n").unwrap();
        let events = w.poll();
        log_check(
            "j3e0.1.create",
            "new file is Created",
            events.len() == 1 && events[0].kind == ChangeKind::Created,
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_emits_removed() {
        let dir = fresh_dir("delete");
        let path = dir.join("doc.md");
        std::fs::write(&path, "x\n").unwrap();
        let clock = FakeClock::new();
        let mut w = primed(&path, Duration::ZERO, clock.clone());
        std::fs::remove_file(&path).unwrap();
        let events = w.poll();
        log_check(
            "j3e0.1.delete",
            "delete is Removed",
            events.len() == 1 && events[0].kind == ChangeKind::Removed,
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_paths_includes_css_without_duplicates() {
        let md = PathBuf::from("doc.md");
        let css = PathBuf::from("t.css");
        let paths = collect_watch_paths(&md, &[css.clone(), css.clone(), md.clone()]);
        log_check(
            "j3e0.1.collect",
            "input then css, unique",
            paths == [md, css],
        );
    }

    #[test]
    fn referenced_local_paths_skips_urls_and_missing_files() {
        let dir = fresh_dir("assets");
        let img = dir.join("pic.png");
        std::fs::write(&img, b"\x89PNG").unwrap();
        let md = "see [a](https://ex.test/x.png) and ![p](pic.png) and [m](mailto:a@b) and [n](nope.png)";
        let found = referenced_local_paths(md, &dir);
        log_check("j3e0.1.assets", "only existing local dest", found == [img]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
