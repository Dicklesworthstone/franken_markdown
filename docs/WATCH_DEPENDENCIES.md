# Native watch: parsed dependencies and stable change bursts

`fmd watch` now discovers image dependencies through the shared Markdown AST,
not a scan for `](...)`. Full, collapsed and shortcut reference images, images
inside headings, quotes, lists, tables and links, and escaped image destinations
participate. Code examples, unused reference definitions, math and raw HTML do
not authorize additional file reads.

Image paths are retained before the file exists. Creating an image later,
editing it, saving by temporary-file rename, deleting it and restoring it all
participate in the existing polling/debounce pipeline. Existing local ordinary
links retain their historical watch behavior; missing ordinary links do not
allocate dependency entries. This is a native filesystem watcher, not an
external-link checker or a network fetcher.

URL query/fragment suffixes are removed before one percent-decoding pass, so
`plot.png?v=2#panel` watches `plot.png`, while `plot%23one.png` watches the literal
`plot#one.png`. Encoded UTF-8, spaces and parentheses are preserved. Schemes,
UNC/network spellings, backslashes, colons, controls, malformed escapes and
invalid UTF-8 do not become local paths. Local absolute POSIX paths retain
filesystem semantics; this is not a confinement boundary. Canonicalization is
not used: missing files and parent components keep their native path meaning.
Non-regular files are filtered before fingerprinting; this does not provide
race-free protection against an adversary replacing files during a poll.

## Automatic dependency refresh

`PollWatcher::new` and `add_path` recognize explicitly supplied `.md` and
`.markdown` roots, case-insensitively. The existing native CLI uses this watcher,
so each supplied Markdown root gets dependency tracking without a new flag.
Images added to a source later are discovered after its change settles. An
implicitly discovered path is removed when no source refers to it anymore.
Shared dependencies are watched once and remain until their last reference is
removed. Caller-supplied paths remain pinned even when not referenced; adding
an already watched path remains a no-op. `paths()` includes the active implicit
paths and retains insertion order for surviving entries.

Only explicit Markdown roots are parsed. A linked Markdown file is watched as
a file, not recursively crawled for more links or images. Directory discovery,
include expansion, raw-HTML dependencies and CSS `@import` graphs are not added
by this change. No renderer options, output-file policy, preview HTTP routes,
SSE wire format or browser/WASM code is changed.

Graphs are cached by the content fingerprint. Unchanged inputs do not reparse
on each tick. Refresh reads at most 64 MiB plus an overflow sentinel and checks
that the captured UTF-8 source matches the polling fingerprint. An unstable,
missing, oversized or invalid UTF-8 source retains its last successful graph;
`dependency_failures()` exposes affected explicit roots, and subsequent polls
retry. The CLI does not yet print this new accessor as a separate diagnostic.
The limit applies to graph discovery, not all fingerprint I/O or renderer work.

## Debounce correctness

A burst is compared with the contents before the burst. A first creation stays
`Created` even if followed by several writes before the quiet window. A
creation followed by deletion, or an edit followed by exact restoration, does
not emit a net change. Restoration after an already emitted deletion retains
the existing `Modified` convention.

Old dependency edges are retained while their source is settling. This matters
when an image changes during a temporary edit that removes its reference: an
undo of that source edit must not hide the image change. Graph retirement and
event emission use the same clock sample, including when filesystem reads span
a debounce deadline.

## Regression coverage

`tests/watch_dependencies.rs` has 18 real-parser/filesystem cases, using the
existing manual clock to drive the actual watcher without timing sleeps.
`src/watch/dependencies.rs` adds five URL/snapshot tests, including a same-size
racing save, bounded reads, invalid UTF-8 and retry without poisoning the cache.
The prior watch and preview-server tests remain intact.

Run in the repository's configured remote Rust environment:

```sh
rch exec -- cargo test --features cli --test watch_dependencies
rch exec -- cargo test --features cli --lib watch::
rch exec -- cargo clippy --all-targets --features cli -- -D warnings
cargo fmt --check
```

These Rust tests were added but not run in the authoring environment: `cargo`,
`rustc` and `rch` are absent. Lexical checks and GitHub diff review are not
compilation or runtime verification. The existing CLI rendering integration
still needs execution against a built binary.
