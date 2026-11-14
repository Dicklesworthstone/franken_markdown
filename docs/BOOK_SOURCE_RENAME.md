# Rename a book source without breaking its references

`book::rename_book_source` prepares a complete source transaction for moving one
chapter or include file. It preserves chapter order, source roles and source
bytes outside the exact destination tokens that need to change.

When a host supplies image assets, use `book::rename_book_source_with_images`
and pass their complete destination-key inventory. This reads only the key
strings, never image bytes. The simpler API specifies that no image assets are
supplied. Keep this inventory unchanged until the proposed sources are applied.

Supply an explicitly captured, expanding book and the selected authoring
profile. Apply the returned `files` and `include_sources` only while the original
source revision is still current. `BookWorkspace::replace_sources_at_revision`
is the native commit boundary for the prepared collection. Browser workbenches
use their source checkpoint and whole-book history transaction; see
[the browser rename API](../wasm/BOOK_SOURCE_RENAME.md).

```rust
let revision = workspace.source_revision();
let image_keys: Vec<String> = workspace.options().pdf_image_assets.iter()
    .map(|asset| asset.destination.clone()).collect();
let prepared = franken_markdown::book::rename_book_source_with_images(
    &chapters, &include_sources, "guide/old.md", "manual/guide.md", profile, &image_keys,
)?;
workspace.replace_sources_at_revision(
    &prepared.files, &prepared.include_sources, revision,
)?;
```

Incoming Markdown links, reference definitions, extensionless chapter aliases
and directory-index aliases follow the renamed source. Local links and images
inside a moved source keep their original targets, including downloads and
directories outside the chapter set. Fragment-only links retain their chapter
context. Queries and fragments retain their decoded values, and URL paths are
decoded once before being encoded for their new location. External URLs remain
unchanged.

Source filenames and image asset keys are separate identities. Renaming a
source never renames a granted image key with the same spelling. The operation
uses the renderer's qualified-key precedence and literal-key fallback, including
trimmed destination spelling. A relative image may become root-qualified to
preserve its exact granted key when a move would otherwise select another
image. Missing images remain unresolved; a move cannot silently acquire a
different supplied asset.

The real include engine identifies active `{{#include ...}}` path tokens.
Incoming includes follow a renamed chapter or include source; outgoing includes
are rebased when their parent moves. Quoted filenames, line ranges and named
snippet selectors retain their meaning. Include paths are literal filenames,
so `%20` inside an include filename is not a URL escape. Code examples containing
links or include directives remain literal.

## Complete admission and semantic checks

The operation uses the same authoring profile for destination recognition,
include protection, parsing and final expansion. A token-probe parse proves
whether each Markdown destination supplies a link, an image, both or neither.
Before returning, it compares complete source ASTs and re-expands every chapter
to compare complete chapter ASTs. Only the intended logical file destinations
may change; literal text, labels, titles, formatting and block structure agree.

If one source is included in incompatible syntax contexts, a shared definition
has different chapter-link and image-resource meanings, or an affected raw HTML
URL lacks an admitted Markdown edit, the operation refuses the entire proposal.
It also refuses a move that would capture an existing unrelated or unresolved
destination at the new path, or that cannot retain an alias's original identity.
These cases require an explicit source edit; they never return a partially
repaired book.

Paths must be canonical relative book paths: no leading slash, backslash,
colon, control character, empty component or `.`/`..` component. Source paths
must be distinct, including case-insensitive collisions and file/directory
prefix collisions. Existing chapter-output collision rules also apply.
Names may contain Unicode, spaces, literal percent signs, `#` and `?`; destination
serialization protects their filename meaning.

Limits are 4,096 captured sources, 4,096 UTF-8 bytes per path and 64 MiB of source
and path bytes, with the ordinary whole-book expansion limits. Source inspection
admits at most 32,768 destination/include tokens, 16,384 token replacements and
8 MiB of replacement text. JSON serialization refuses output over 128 MiB.
The browser workbench applies its existing tighter input limits first.
Image inventories contain at most 4,096 nonempty keys, each at most 4,096 UTF-8
bytes; keys are trimmed and must then be distinct. They are destination keys,
so URI schemes and rooted names are allowed instead of applying source-path
restrictions to them.

`BookSourceRename` exposes the ordered chapter/include arrays,
`changed_source_count` (path or text changed) and `edit_count` (replaced source
tokens only). Its `to_json()` result uses schema `fmd-book-source-rename-v1` and
the fields `oldPath`, `newPath`, `files`, `includeSources`, `changedSourceCount`
and `editCount`. The `wasm-book` package exposes the same immutable operation as
`renameBookSource`.

The operation performs no filesystem or network I/O, never moves a host file,
and does not mutate a retained renderer or rename image/font asset keys.

## Regression coverage

```sh
cargo test --no-default-features --lib book::source_rename
```

Regressions cover incoming/outgoing references, shared includes, selected
snippets, code protection, Unicode and one-pass URL decoding, path and alias
collisions, atomic refusal, source limits, and actual PDF/EPUB/site exports using
existing asset keys. These tests do not establish filesystem move behavior or
native device acceptance.
