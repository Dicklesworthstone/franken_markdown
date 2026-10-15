# Live diagnostics for links between opened documents

The native `fmd-lsp` server now checks file-addressed fragments against the
client's other explicitly opened Markdown buffers. It refreshes the originating
document's diagnostics when a target is opened, edited, closed, or loses
synchronization. No filesystem traversal, network access, include expansion,
font loading or PDF rendering is performed.

For example, open these two buffers:

```markdown
<!-- file:///docs/intro.md -->
[Installation](setup.md#installation)
```

```markdown
<!-- file:///docs/setup.md -->
# Installation
```

Changing the target heading to `# Getting started` publishes a
`workspace_missing_anchor` warning for the link in `intro.md`, without editing
`intro.md`. Restoring the heading clears that warning. Every notification
contains the complete combined parser, internal-link and workspace-link
finding set, tagged with the **originating buffer's own version**, not the
changed target's version. Local findings are never replaced by a workspace-only
array.

## Shared publication semantics

Links come from the existing Rust parser and publication walk, not a Markdown
regular expression. Reference-style links, links in lists/quotes/tables and
published notes retain their real enclosing source block. Repeated references
to the same destination in one block produce one warning. Image paths, code
examples, raw HTML attributes, external URLs and unreferenced note bodies do
not become workspace edges.

Targets use their actual emitted heading and referenced-footnote anchors.
Unicode and percent-encoded fragments are decoded once. Duplicate heading IDs
retain the publication suffixes; collisions between emitted anchor classes are
reported as ambiguous. Checking a target never inserts a synthetic reference
that would accidentally make a previously unpublished note visible.

The existing opened-buffer URI resolver handles relative paths, explicit file
URIs, extensionless Markdown links and directory index links. It does not read
those paths. An unopened target or ambiguous file identity is **unchecked**,
not reported as a nonexistent file. Closing such a target clears prior anchor
findings because the server no longer has its authoritative text; that is not
proof that the link became valid. The same scope applies to `untitled:` buffers:
their own links are checked, but a relative file path cannot invent a file base.

## Diagnostic codes

| Code | Meaning |
| --- | --- |
| `workspace_missing_anchor` | The selected opened target has no emitted anchor with this ID. |
| `workspace_invalid_fragment` | The selected target's fragment is malformed percent-encoded UTF-8 or contains an encoded control. |
| `workspace_ambiguous_anchor` | More than one emitted target element has that ID. |
| `workspace_target_unsynchronized` | The target rejected an edit; its last accepted anchors are no longer authoritative. |
| `workspace_link_analysis_incomplete` | Analysis or reference admission prevented a complete cross-document check. |

Existing parser findings and the local `missing_anchor`, `invalid_fragment`,
`ambiguous_anchor`, `missing_footnote` and `link_analysis_incomplete` codes keep
their meanings. All locations remain the parser's top-level source-block ranges
in UTF-16 coordinates; these are not invented inline spans or safe rename edits.

## Revision ownership and limits

The diagnostic cache parses only newly accepted document versions. It retains
source spans, bounded link inventories and local findings, not a second source
body or whole AST. Target lookups reuse those retained anchors through
`DocumentLinkAnalysis::resolve_fragment`. A source's repeated destinations to
one file share the opened-buffer resolution for that refresh. Cached diagnostics
are reconciled on accepted opens/changes, rejected edits, closes and shutdown.
Changes in other buffers do not reparse the origin; unchanged diagnostic sets
are not republished. Navigation requests retain their existing separate behavior.

The server remains synchronous: it reevaluates cached edges on a document event,
not asynchronously and not through an incremental Markdown parser. The existing
64-open-document, 2 MiB/document and 16 MiB/source-session limits remain. A
file-aware inventory has a 16384-reference, 8192-byte/destination and 1 MiB total
destination limit. More than 32768 retained references suspends cross-document
checking with an explicit incomplete finding in affected sources until the
workspace returns below the budget. These are admission bounds, not measured
heap or CPU-time quotas.

An inventory-only refusal falls back to ordinary internal analysis on the same
parsed AST, retaining local findings and explicitly marking the workspace check
incomplete. Rejected edits discard the affected cache entry before dependents
are checked; a full replacement or close/reopen restores it. Reopening with the
same numeric version is still a new document incarnation. Stale version
notifications cannot restore old anchors.

Each publication keeps the existing 1024-finding and 512-Unicode-scalar/message
limits. Parser/local findings have priority; overflow ends with
`diagnostics_truncated`, never silent truncation. Changed-document publication
comes first, followed by affected origins in deterministic URI order.

## Verification status

Eight core fragment-resolution tests and 23 workspace/server regressions were
added. The latter exercise the real parser, resolver, server state machine and
a Content-Length-framed session, including target repair without source edits,
cache reuse, note publication, URI ambiguity, CRLF/Unicode positions, refusal,
close/reopen, truncation and global admission. The existing eleven internal-link
diagnostic tests remain in place.

**These Rust tests have not been executed in the authoring environment.** Rust,
Cargo, rustfmt, RCH and DSR are unavailable there. JSON-fixture decoding,
whitespace checks and source-blob verification were performed; those do not
establish compilation, a running server, or a passing repository suite. No
GitHub Actions workflow or substitute renderer was used.

Run the focused checks through the project's admitted Rust/DSR environment:

```sh
cargo test --locked --test document_fragment_resolution
cargo test --locked --features lsp --bin fmd-lsp
cargo test --locked --features lsp --test lsp_protocol_test
cargo check --locked --all-targets
cargo clippy --locked --features lsp --bin fmd-lsp -- -D warnings
cargo build --locked --no-default-features
cargo fmt --check
```
