# Native book chapters and shared sources

`fmd book` now distinguishes published Markdown chapters from Markdown files
used only as shared include sources. In `book.toml`:

```toml
title = "Project handbook"
order = ["start.md", "guide/install.md"]
include_only = ["parts/warning.md", "parts/common-steps.md"]
```

A publishing chapter can still contain:

```markdown
{{#include parts/warning.md}}
```

The shared source expands into that chapter using the existing bounded include
resolver. It does not become a separate HTML page, search chapter, PDF chapter,
or EPUB spine item. The manifest changes only chapter membership; it does not
rewrite any source file or move an include's contents to another location.
Relative links and images inside a snippet resolve from the file that defines
them. For example, `parts/shared.md` can use `![Chart](chart.svg)` and
`[Next](next.md#setup)` when included by either `start.md` or
`guide/install.md`: the asset remains `parts/chart.svg`, and the chapter link
still names `parts/next.md#setup`. Nested includes and line/anchor selectors
retain that same origin. A reference-style link uses the origin of its
destination definition, even when the link itself appears in another file.

The native loader rebases actual Markdown destination tokens after expansion.
Code examples, raw HTML, labels and titles keep their source text; URL queries
and fragments remain attached. The resulting input is shared by publication
and `--check-links`, so checks resolve against the same published chapter
paths. Local resources remain subject to the existing book-root and regular-file
policies; expansion does not authorize external filesystem or network access.

## Selection rules

`order` (or its existing alias `chapters`) remains an ordering prefix, not an
exclusive selection list. Other discovered Markdown chapters follow in lexical
order. Adding `include_only` removes exactly those sources from the publishing
set. Omitting it preserves the previous behavior.

Entries are literal book-root-relative paths, not URLs or glob patterns. They
use the same path normalization as chapter ordering. Quotes protect commas,
`#`, and brackets in filenames; multiline arrays, comments, trailing commas,
Unicode, and the supported TOML string escapes work as for `order`.

Each entry must name a discovered regular Markdown file. Unknown paths,
normalized duplicates, paths outside the root, and a source declared in both
`order` and `include_only` are errors. There must remain at least one published
chapter. Discovery still skips dot-prefixed entries and symlinks. Non-Markdown
files and sources under hidden directories already stay out of automatic
chapter discovery; they do not need an `include_only` declaration.

This is a publication-role list, **not an include allowlist or filesystem
sandbox**. Existing in-root include access, selector handling, symlink refusal,
cycle detection, and source/expansion limits remain in force. The 4096 discovered
Markdown-source ceiling includes both chapters and include-only Markdown.

Unused declared sources have their regular-file identity and size checked but
are not read, UTF-8-decoded, expanded, or parsed. Referenced sources still incur
the existing per-read and whole-book byte budgets and must be valid UTF-8.
Declared sources remain protected from publication overwrites even when no
chapter references them. Input trees must remain under the caller's control
while the portable filesystem checks run; they are not race-free containment
against a hostile concurrent writer.

## Commands

```sh
fmd book ./manual --to html --out-dir ./site --json
fmd book ./manual --to pdf --out ./manual.pdf --json
fmd book ./manual --to epub --out ./manual.epub --json
```

Receipts, the sidebar, site search, and the EPUB spine count only publishing
chapters. An include-only path is not a navigable chapter URL: link to a heading
in a publishing chapter that actually includes the snippet.

A fresh destination has no standalone page for excluded sources. Publication
does **not** delete obsolete files from an existing output directory. Old pages
from a previous build can therefore remain on disk; use a fresh destination
when changing membership and deploying a clean site.

## Verification

`tests/native_book_sources_test.rs` adds executable regression coverage for
expansion without extra pages, legacy ordering, EPUB byte equality with the
selected core book, both executable names, invalid-role rejection, preservation
of existing outputs and unused sources, include failures, output-name
collisions, and symlink refusal. Unit cases in the manifest and input modules
cover quote-aware parsing, budgets, normalization, role conflicts and ordering.

Run with the configured DSR/Rust environment:

```sh
cargo test --test native_book_sources_test
cargo test --lib book::native
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

The authoring environment for this change has no local Rust toolchain or DSR
executable. These Rust tests are committed but were not executed there; no
native or generated-WASM pass is inferred from source inspection.

For read-only expanded-navigation reports and the opt-in fail-before-write
publication guard, see `NATIVE_BOOK_CHECKS.md`.
