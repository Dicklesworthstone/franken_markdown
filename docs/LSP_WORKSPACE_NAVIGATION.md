# Cross-document editor navigation

`fmd-lsp` resolves Markdown links against documents the editor has already
opened with `textDocument/didOpen`. This extends the navigation described in
`LSP_LINK_NAVIGATION.md`; it does not scan a workspace, read files, fetch URLs,
or silently fall back to saved content.

## Definitions and fragment completion

Go to definition works anywhere in an explicit destination such as
`[guide](../guide.md#installation)`, including its filename. Full, collapsed and
shortcut reference uses also resolve to their target; see
[LSP_REFERENCE_NAVIGATION.md](LSP_REFERENCE_NAVIGATION.md). A destination with
no fragment, or an empty fragment, addresses the target buffer's start.
Typing `#inst` after an explicit filename offers that target's actual emitted
heading IDs, including collision suffixes. Accepting an item replaces only the
origin buffer's fragment; its filename, query, angle brackets, title, and
surrounding text are preserved. Unfinished destinations at the end of their
enclosing block can be completed without inserting closing delimiters into the
buffer. Reference labels receive no fragment completion edits.

Targets use their own heading and referenced-footnote namespace, not a merged
workspace namespace. Nested headings point to their authoritative enclosing
source block. Missing and ambiguous anchors are inert; unreferenced footnotes
do not become destinations just because another document links to them.
Positions and edits are UTF-16, including CRLF and astral Unicode characters.
The exact URI originally supplied by the client is returned.

## Logical file identity

Relative, parent-relative, root-relative, and explicit `file:` destinations can
match opened file buffers. Percent escapes are decoded exactly once per path
component. Encoded separators, malformed escapes, invalid UTF-8, controls, and
traversal above a URI or drive root are rejected. Drive letters and authorities
are case-normalized; other path characters are case-sensitive. `localhost`
and an empty authority name the same local file identity.

An exact open path wins. An extensionless path can otherwise match one `.md`
or `.markdown` file, or one directory index (`index.md`, `index.markdown`,
`README.md`, `README.markdown`). Explicit directory links consider only those
index files. Multiple matches or multiple open URI aliases for one identity
are ambiguous, never first-match-wins. No project root is inferred: parent links
may reach another explicitly opened buffer, but cannot authorize a file read.

Non-file schemes and protocol-relative URLs are not followed. Relative file
links in opaque buffers such as `untitled:` have no inferred directory.
Same-document links continue to work in those buffers.

## Revisions, limits, and scope

Every request uses current synchronized buffers. A target renamed by an accepted
`didChange` is immediately reflected. A target requiring full-text
resynchronization returns ContentModified (`-32801`); closing it removes it
from resolution. Unknown files yield null/empty results, not invented locations.

The existing 64-buffer, 16-MiB session, 2-MiB document, 64-KiB source-block,
8-KiB destination, 256-completion, and core analysis limits remain in force.
Parser-verified probes still exclude code, image destinations, HTML attributes,
and link titles. Origins inside quotes, lists, tables, definition lists and
referenced notes are supported as described in
[LSP_NESTED_NAVIGATION.md](LSP_NESTED_NAVIGATION.md). Multiline reference uses,
multiline/escaped explicit URL tokens and direct footnote-marker navigation
remain outside this implementation.

## Verification

`src/bin/lsp/workspace_links_tests.rs` adds twelve Rust regressions covering URI
identity, directory ambiguity, parser exclusions, definitions, completion edits,
Unicode, footnotes, revision changes, resynchronization, close, and dispatch.
The nested- and reference-link suites additionally cover cross-file origins.

```sh
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
```

These tests were authored but not executed in the toolchain-less authoring
environment. Source review and `git diff --check` are not Rust compilation or
protocol execution evidence.
