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

/// Recursively collect every `*.md` file under `dir`, sorted by path. Used
/// by `fmd watch <dir>` to expand a directory input into a stable,
/// deterministic set of watch targets. Hidden files (`.foo.md`) and
/// files inside hidden directories (e.g. `.git/`) are skipped — agents
/// and CI users occasionally keep notes in `.scratch.md` that they do
/// not want watched. Symlinks are not followed: a symlinked file that
/// re-enters the directory would create a cycle, and a non-following
/// walker is the safe default for a CLI tool. Empty directories return
/// an empty `Vec` so the caller can fail with a clear error.
#[must_use]
pub fn expand_md_directory(dir: &Path) -> Vec<PathBuf> {
    fn walk(acc: &mut Vec<PathBuf>, dir: &Path) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                walk(acc, &path)?;
            } else if file_type.is_file()
                && std::path::Path::new(&name_str)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                acc.push(path);
            }
        }
        Ok(())
    }
    let mut acc = Vec::new();
    if walk(&mut acc, dir).is_err() {
        return Vec::new();
    }
    acc.sort();
    acc
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

// =====================================================================
// j3e0.2: loopback-only preview HTTP server with auto-reload
// =====================================================================

/// The auto-reload snippet injected into the served preview HTML. The
/// `EventSource` connection lives at `/events`; each `data: reload` line
/// from the server causes the page to refresh. This snippet is **not**
/// injected into `--out` files: it is only present in the in-memory
/// preview served over the loopback connection.
pub const RELOAD_SNIPPET: &str = "<script>(function(){var es=new EventSource('/events');es.onmessage=function(e){if(e.data==='reload')location.reload();};})();</script>";

/// What the preview server knows how to serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `GET /` — current rendered HTML with the reload snippet appended.
    Index,
    /// `GET /events` — `text/event-stream` with `data: reload` lines.
    Events,
    /// Anything else (404).
    NotFound,
}

/// Parse a single HTTP/1.1 request line + headers into a route. The
/// parser is intentionally minimal: a request method that isn't `GET`,
/// a path that doesn't start with `/`, or any path component that
/// contains `..` is treated as `NotFound` (directory-traversal hard
/// reject). The body is not consumed.
#[must_use]
pub fn route_for(req: &[u8]) -> Route {
    let Ok(text) = std::str::from_utf8(req) else {
        return Route::NotFound;
    };
    // `str::lines` treats LF, CRLF, and lone CR as record breaks, matching
    // what browsers and curl actually send.
    let Some(request_line) = text.lines().next() else {
        return Route::NotFound;
    };
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    if method != "GET" {
        return Route::NotFound;
    }
    // Strip the query before the traversal walk so `/?x=../y` is still `/`.
    let path = path.split('?').next().unwrap_or(path);
    if !path.starts_with('/') {
        return Route::NotFound;
    }
    for seg in path.split('/') {
        if seg == ".." {
            return Route::NotFound;
        }
    }
    if path == "/" {
        Route::Index
    } else if path == "/events" {
        Route::Events
    } else {
        Route::NotFound
    }
}

/// Render the response bytes for a given route. `events` is the
/// pre-encoded SSE body (without the trailing `data: reload\n\n` line,
/// which the caller appends each time a change is announced).
#[must_use]
pub fn render_response(route: Route, html: &str, events_header: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(html.len() + 256);
    match route {
        Route::Index => {
            // Inject the reload snippet just before `</body>` if present,
            // otherwise append it. The served preview must always refresh.
            let body = inject_reload_snippet(html);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                 Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                body.len()
            );
            out.extend_from_slice(header.as_bytes());
            out.extend_from_slice(body.as_bytes());
        }
        Route::Events => {
            // EventSource preamble + caller-supplied buffered events.
            out.extend_from_slice(events_header.as_bytes());
        }
        Route::NotFound => {
            let body = b"not found";
            let header = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            out.extend_from_slice(header.as_bytes());
            out.extend_from_slice(body);
        }
    }
    out
}

/// The SSE preamble sent to a freshly-connected `/events` client. Each
/// subsequent `data: reload\n\n` line is appended by the writer when a
/// file change is observed.
#[must_use]
pub fn sse_preamble() -> String {
    // EventSource is a long-lived stream. `Connection: close` tells HTTP/1.1
    // intermediaries the response is done; keep-alive is the correct signal.
    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
     Cache-Control: no-store\r\nConnection: keep-alive\r\n\r\n"
        .to_string()
}

fn inject_reload_snippet(html: &str) -> String {
    if let Some(idx) = rfind_body_close(html) {
        let mut out = String::with_capacity(html.len() + RELOAD_SNIPPET.len());
        out.push_str(&html[..idx]);
        out.push_str(RELOAD_SNIPPET);
        out.push_str(&html[idx..]);
        out
    } else {
        let mut out = String::with_capacity(html.len() + RELOAD_SNIPPET.len());
        out.push_str(html);
        out.push_str(RELOAD_SNIPPET);
        out
    }
}

/// Last `</body>` in `html`, ASCII-case-insensitive. Tag bytes are ASCII so
/// the returned index is always a UTF-8 char boundary.
fn rfind_body_close(html: &str) -> Option<usize> {
    const NEEDLE: &[u8] = b"</body>";
    let bytes = html.as_bytes();
    if bytes.len() < NEEDLE.len() {
        return None;
    }
    let mut last = None;
    for i in 0..=bytes.len() - NEEDLE.len() {
        if bytes[i..i + NEEDLE.len()].eq_ignore_ascii_case(NEEDLE) {
            last = Some(i);
        }
    }
    last
}

/// Bind a `TcpListener` on 127.0.0.1 with the OS-chosen port and return
/// `(listener, bound_port)`. The loopback-only constraint is structural
/// — the listener is created with `SocketAddrV4::new(LOCALHOST, 0)`,
/// not from a `to_socket_addrs` lookup, so a hostile hostname cannot
/// redirect the bind to an external interface.
#[cfg(feature = "cli")]
pub fn bind_loopback() -> std::io::Result<(std::net::TcpListener, u16)> {
    use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0);
    let listener = TcpListener::bind(addr)?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// A single SSE reload event ready to be written to a `/events` client.
#[must_use]
pub fn sse_reload_event() -> &'static str {
    "data: reload\n\n"
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

    // xjld: directory mode for `fmd watch <dir>`.

    #[test]
    fn expand_md_directory_finds_nested_markdown_sorted() {
        // xjld: a watch over a directory must surface every `*.md`
        // file in a deterministic order so a render re-emits the
        // same output bytes across runs. Nested directories count.
        let dir = fresh_dir("expand");
        std::fs::write(dir.join("z.md"), "# z\n").unwrap();
        std::fs::write(dir.join("a.md"), "# a\n").unwrap();
        let sub = dir.join("nested");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("m.md"), "# m\n").unwrap();
        let found = expand_md_directory(&dir);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        log_check(
            "xjld.expand.sorted",
            "lexicographic order across nested paths",
            names == ["a.md", "m.md", "z.md"],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expand_md_directory_skips_hidden_files_and_extensions() {
        // xjld: only `*.md` is watched. A `.scratch.md` note or a
        // `.txt` file must not appear in the watch set. Hidden
        // directories are also skipped (no descent into `.git/`).
        let dir = fresh_dir("hiddendir");
        std::fs::write(dir.join("keep.md"), "# k\n").unwrap();
        std::fs::write(dir.join(".scratch.md"), "# n\n").unwrap();
        std::fs::write(dir.join("readme.txt"), "txt\n").unwrap();
        let git = dir.join(".git");
        std::fs::create_dir(&git).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let found = expand_md_directory(&dir);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        log_check(
            "xjld.expand.filtered",
            "only keep.md survives filtering",
            names == ["keep.md"],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expand_md_directory_uppercase_extension_is_markdown() {
        // xjld: case-insensitive match. A `.MD` file is still
        // Markdown. The case-insensitive walk is a small but
        // important concession to cross-platform file systems
        // (HFS+ and Windows are case-insensitive by default).
        let dir = fresh_dir("caseins");
        std::fs::write(dir.join("lower.md"), "# l\n").unwrap();
        std::fs::write(dir.join("upper.MD"), "# u\n").unwrap();
        let found = expand_md_directory(&dir);
        log_check(
            "xjld.expand.case",
            "two markdown files (one .MD, one .md)",
            found.len() == 2,
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expand_md_directory_empty_returns_empty_vec() {
        // xjld: an empty directory or one with no `*.md` must
        // return an empty Vec so the CLI fails loudly with a
        // clear error (rather than silently watching nothing).
        let dir = fresh_dir("empty");
        let found = expand_md_directory(&dir);
        log_check(
            "xjld.expand.empty_dir",
            "no markdown files",
            found.is_empty(),
        );
        let only_txt = fresh_dir("only_txt");
        std::fs::write(only_txt.join("a.txt"), "x\n").unwrap();
        let found_txt = expand_md_directory(&only_txt);
        log_check(
            "xjld.expand.only_txt",
            "directory with only .txt returns empty",
            found_txt.is_empty(),
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&only_txt);
    }

    #[test]
    fn expand_md_directory_nonexistent_path_returns_empty() {
        // xjld: a missing path is not a panic; the function
        // returns empty so the CLI can surface a clear "not
        // found" error (the CLI does the final user-facing
        // translation).
        let bogus = PathBuf::from("/nonexistent/path/that/does/not/exist");
        let found = expand_md_directory(&bogus);
        log_check(
            "xjld.expand.missing",
            "missing path returns empty vec (no panic)",
            found.is_empty(),
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

    // ===============================================================
    // j3e0.2 — loopback-only preview HTTP server with auto-reload
    // ===============================================================

    fn req(path: &str) -> Vec<u8> {
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nUser-Agent: test\r\n\r\n").into_bytes()
    }

    #[test]
    fn route_for_gets_root_index() {
        log_check(
            "j3e0.2.route.index",
            "GET / -> Index",
            route_for(&req("/")) == Route::Index,
        );
    }

    #[test]
    fn route_for_gets_events_stream() {
        log_check(
            "j3e0.2.route.events",
            "GET /events -> Events",
            route_for(&req("/events")) == Route::Events,
        );
    }

    #[test]
    fn route_for_rejects_directory_traversal() {
        for hostile in ["/../etc/passwd", "/a/../b", "/foo/.."] {
            log_check(
                "j3e0.2.route.traversal",
                hostile,
                route_for(&req(hostile)) == Route::NotFound,
            );
        }
    }

    #[test]
    fn route_for_rejects_unknown_paths() {
        log_check(
            "j3e0.2.route.unknown",
            "GET /admin -> NotFound",
            route_for(&req("/admin")) == Route::NotFound,
        );
    }

    #[test]
    fn route_for_rejects_non_get_methods() {
        let post = b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let put = b"PUT /events HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        log_check(
            "j3e0.2.route.post",
            "POST / -> NotFound",
            route_for(post) == Route::NotFound,
        );
        log_check(
            "j3e0.2.route.put",
            "PUT /events -> NotFound",
            route_for(put) == Route::NotFound,
        );
    }

    #[test]
    fn route_for_ignores_query_strings() {
        log_check(
            "j3e0.2.route.query",
            "GET /?v=1 -> Index",
            route_for(&req("/?v=1")) == Route::Index,
        );
    }

    #[test]
    fn render_response_index_injects_reload_snippet() {
        let html = "<html><body><p>hi</p></body></html>";
        let resp = render_response(Route::Index, html, "");
        let text = std::str::from_utf8(&resp).expect("utf-8");
        log_check(
            "j3e0.2.index.200",
            "200 OK status",
            text.starts_with("HTTP/1.1 200 OK\r\n"),
        );
        log_check(
            "j3e0.2.index.snippet",
            "EventSource snippet present",
            text.contains("EventSource('/events')"),
        );
        log_check(
            "j3e0.2.index.before_close",
            "snippet injected before </body>",
            text.find("EventSource").unwrap() < text.find("</body>").unwrap(),
        );
        log_check(
            "j3e0.2.index.content_type",
            "Content-Type is text/html",
            text.contains("Content-Type: text/html; charset=utf-8"),
        );
        log_check(
            "j3e0.2.index.no_store",
            "Cache-Control: no-store so reload wins",
            text.contains("Cache-Control: no-store"),
        );
    }

    #[test]
    fn render_response_index_appends_when_no_body_tag() {
        let html = "<p>fragment</p>";
        let resp = render_response(Route::Index, html, "");
        let text = std::str::from_utf8(&resp).expect("utf-8");
        log_check(
            "j3e0.2.index.appended",
            "snippet appended when no </body>",
            text.contains("EventSource") && text.ends_with("</script>"),
        );
    }

    #[test]
    fn render_response_events_uses_sse_preamble() {
        let resp = render_response(Route::Events, "", &sse_preamble());
        let text = std::str::from_utf8(&resp).expect("utf-8");
        log_check(
            "j3e0.2.events.200",
            "200 OK status",
            text.starts_with("HTTP/1.1 200 OK\r\n"),
        );
        log_check(
            "j3e0.2.events.content_type",
            "Content-Type: text/event-stream",
            text.contains("Content-Type: text/event-stream"),
        );
        log_check(
            "j3e0.2.events.keep_alive",
            "SSE stream uses Connection: keep-alive",
            text.contains("Connection: keep-alive") && !text.contains("Connection: close"),
        );
    }

    #[test]
    fn injects_reload_snippet_before_uppercase_body_close() {
        let html = "<HTML><BODY><p>hi</p></BODY></HTML>";
        let resp = render_response(Route::Index, html, "");
        let text = std::str::from_utf8(&resp).expect("utf-8");
        log_check(
            "j3e0.2.index.upper_body",
            "snippet injected before </BODY>",
            text.find("EventSource").unwrap() < text.find("</BODY>").unwrap(),
        );
    }

    #[test]
    fn route_for_accepts_lf_only_request_line() {
        let req = b"GET /events HTTP/1.1\nHost: 127.0.0.1\n\n";
        log_check(
            "j3e0.2.route.lf",
            "LF-only GET /events -> Events",
            route_for(req) == Route::Events,
        );
    }

    #[test]
    fn route_for_query_does_not_trigger_traversal_reject() {
        log_check(
            "j3e0.2.route.query-dots",
            "GET /?x=../y -> Index",
            route_for(&req("/?x=../y")) == Route::Index,
        );
    }

    #[test]
    fn render_response_not_found_is_404() {
        let resp = render_response(Route::NotFound, "", "");
        let text = std::str::from_utf8(&resp).expect("utf-8");
        log_check(
            "j3e0.2.404.status",
            "404 Not Found status",
            text.starts_with("HTTP/1.1 404 Not Found\r\n"),
        );
        log_check(
            "j3e0.2.404.body",
            "404 body mentions 'not found'",
            text.contains("not found"),
        );
    }

    #[test]
    fn sse_reload_event_is_well_formed() {
        let ev = sse_reload_event();
        log_check(
            "j3e0.2.sse.data",
            "payload is 'data: '",
            ev.starts_with("data: "),
        );
        log_check(
            "j3e0.2.sse.terminator",
            "terminator is \\n\\n",
            ev.ends_with("\n\n"),
        );
    }

    #[test]
    fn bind_loopback_uses_localhost_only() {
        let (listener, port) = bind_loopback().expect("bind");
        log_check(
            "j3e0.2.bind.port_nonzero",
            "OS-chosen port is non-zero",
            port != 0,
        );
        let addr = listener.local_addr().expect("local_addr");
        log_check(
            "j3e0.2.bind.is_loopback",
            "bound to 127.0.0.1",
            addr.ip().is_loopback() && addr.ip().to_string() == "127.0.0.1",
        );
    }
}
