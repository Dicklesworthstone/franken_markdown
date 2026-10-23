# Rename headings and their Markdown links

In `fmd-lsp`, start **Rename Symbol** on a top-level ATX heading (`# Heading`)
or a one-line setext heading. The replacement is the heading's visible title.
The server returns one versioned workspace edit containing that title and every
matching real Markdown link in the editor's opened, synchronized buffers.

For example, renaming `# Installation` to `Getting started` updates
`[install](#installation)` and an opened chapter's
`[install](../guide.md?view=print#installation)` to the actual newly emitted ID,
`getting-started`. Link labels, paths, queries, titles, angle brackets and the
heading's level, indentation and closing markers retain their original bytes.
Reference-style uses update their supplying destination once; the link labels
and `[label]: ...` declarations are otherwise preserved. Unicode fragments use
percent-encoded UTF-8. Source ranges are UTF-16, including BOM/CRLF documents.

New titles are literal text, not Markdown expressions. Punctuation is escaped
as necessary, so renaming to `Use [brackets]` cannot create an unintended link.
Renaming a formatted heading replaces its complete inline title; using its
unchanged visible title is a no-op that retains existing formatting. A replacement
must contain 1–1024 UTF-8 bytes, without control characters, Unicode line/paragraph
separators or outer whitespace.

## Semantic source ownership

The provider reuses the normal parser's exact destination-token inventory.
That inventory preserves source positions through nested lists, quotes, tables,
notes, multiline reference definitions, entities and escapes. Code, HTML
attributes and image descriptions are never scanned as Markdown links.

A destination token does not prove which reference uses it. Two bounded bulk
probe parses identify the real `Link` nodes supplied by each token, then restoring
their original destinations must reproduce the entire original AST. Unused and
shadowed definitions are therefore unchanged. An image-only token is unchanged;
a shared definition used by both a link and an image refuses the rename instead
of changing the image's resource URL. Real links in dormant note bodies are also
updated, so later activating a note does not restore its former heading target.

The complete proposed source is then reparsed and must match exactly the intended
heading and link edits. A real use without an independently editable source token,
including the current autolink limitation, refuses the whole request. There is no
partial successful edit that leaves a known affected Markdown link behind.

The additive `parse::destination_spans` API exposes the same sorted, deduplicated
UTF-8 token ranges to other Rust consumers. `DestinationSpan::fragment_range`
uses the parser's escape/entity rules to locate the fragment's original source
suffix while retaining its path and query spelling. Tokens include images and
accepted but possibly unused reference definitions; callers must establish their
semantic role before making edits.

## Anchor and workspace checks

The shared publication analyzer computes both old and new heading IDs. A rename
refuses when it would change another heading's collision suffix, remove a
footnote anchor or collide with an emitted note. Renaming the last of several
equal headings can succeed when the other headings retain their existing IDs.

Only explicitly opened text participates. Relative paths, directory indexes,
extensionless Markdown names and explicit `file:` destinations use the existing
workspace resolver. Ambiguous target paths or edited file URI aliases refuse;
opaque buffers such as `untitled:` retain only their document-local authority.
The server neither discovers nor reads unopened files, expands includes, follows
network links nor writes source. Its coverage is the opened-buffer set, not the
entire on-disk project. External callers of an unopened chapter are outside it.

Every opened buffer must be synchronized. Rejected incremental updates require
resynchronization before rename; no stale buffer is silently omitted. Returned
`documentChanges` carry each original client URI and its exact current version.
The client must advertise versioned `documentChanges` support. The editor applies
the proposed operation; server state changes only through subsequent ordinary
document notifications.

## Scope and limits

Rename starts on the heading declaration. Existing navigation and reference
search still work from links; link-origin rename, independently mapped headings
inside containers and multiline setext titles are outside this provider. Footnote
labels retain their separate [rename contract](LSP_FOOTNOTES.md).

Raw HTML IDs and `href` attributes are outside the LSP's default-safe Markdown
navigation contract and remain unchanged. The default sanitizer drops HTML `id`
attributes; this provider does not infer a host's trusted raw-HTML mode or parse a
second HTML DOM. Code, math, images and unrelated literal text remain unchanged.

The existing limits of 64 buffers, 2 MiB per source and 16 MiB aggregate source
also apply to the proposed result. A rename examines at most 32,768 links and
1 MiB of aggregate destination text, with 8192 bytes per destination. Source
inventory work admits at most 8192 tokens; temporary probes are capped at 4 MiB.
The result is capped at 4096 edits, 2 MiB of replacement text and 1 MiB of URI
text. A work, source or response limit returns `RequestFailed`, never truncation.

## Verification

The provider's regression suite exercises actual parsed sources and applies the
returned edits, covering ATX/setext titles, Unicode and CRLF, reference precedence,
all containers, cross-file paths, encoded fragments, shared image definitions,
duplicate headings, footnote collisions, stale/aliased buffers, live revisions,
work/source limits and dispatch alongside footnote rename.

```sh
cargo test --features lsp --bin fmd-lsp --test lsp_protocol_test
cargo clippy --features lsp --lib --bin fmd-lsp --test lsp_protocol_test -- -D warnings
```
