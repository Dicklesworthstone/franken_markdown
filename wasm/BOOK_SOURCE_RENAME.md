# Rename book sources while preserving references

The [native source-refactoring contract](../docs/BOOK_SOURCE_RENAME.md) describes
the Rust API and its semantic checks.

The book workbench can move a chapter or an include-only source to a new logical
book path and update its references as one source transaction. Use **Rename a
source and update references** in **Arrange and edit**:

1. Select the source and enter its new book-relative path in the rename panel.
2. Select **Prepare rename**. The native engine checks the complete source graph
   and the panel lists the files it would change.
3. Select **Apply rename** to install the whole result. **Book-wide undo** restores
   the old paths and source together; redo restores the rename. Save your book
   to keep the result.

Preparation leaves the current source untouched. Cancellation, a stale source
revision, a changed image-key inventory or destination, text composition, suspension or a native error
cannot apply a partial rename. A new attempt requires a fresh capture. Applying
uses the existing source-tree transaction, so chapter order and include-only
roles remain intact, prepared publications become stale, and source history
observes one edit. It does not reopen a project or restore revoked resources.

The ordinary **Book-relative source path** input remains a literal path edit.
Use the separate rename panel when references should follow the move. Neither
operation moves, deletes or overwrites a file on the host filesystem. Existing
image keys and image/font bytes remain unchanged. The panel supplies their
complete key inventory, without reading the bytes, so native image edits keep
addressing the same supplied keys. This matters because the renderer tries an
origin-qualified image key before falling back to the literal destination key.

## Native ownership of the edits

The native operation updates incoming chapter links and reference definitions,
self-links with file paths, relative outgoing links and image destinations, and
active include directives. Named snippet and line selectors, queries, fragments,
reference titles, source BOMs, line endings and all bytes outside edited tokens
are retained. Changed Markdown destinations may use angle delimiters or encoded
spelling to preserve their parsed meaning. Extensionless and directory chapter
aliases resolve against the complete original source set before the move.

The normal Markdown parser supplies destination spans. The normal transclusion
engine supplies active include spans and protects code examples. The operation
reparses and re-expands the resulting source and compares semantic structure
before returning it. The browser does not search for link-shaped text, use a
second Markdown parser or rewrite code examples. External URLs and local
fragments keep their original targets. Raw HTML is governed by the existing
native safe-HTML policy; this is not an arbitrary HTML or JavaScript refactorer.

The complete source graph must be supplied. Missing or cyclic includes,
ambiguous aliases or shared reference ownership, colliding paths, path/file
prefix conflicts, budget exhaustion and changes whose source ownership cannot
be established fail explicitly. The engine returns no partial edited source
set. The API moves one existing source, not a whole directory or an image/font
resource. Folder restructuring can be performed as separately reviewed moves.

## Direct API

```js
import { renameBookSource } from "@franken-suite/franken-markdown/book";

const result = await renameBookSource(
  [
    { path: "start.md", source: "[Install](guide/install.md#setup)" },
    { path: "guide/install.md", source: "# Setup\n\n![Diagram](../images/setup.svg)" },
  ],
  "guide/install.md",
  "manual/setup.md",
  { includeSources: [], profile: "commonmark-gfm", imageDestinations: ["images/setup.svg"] },
);

// The host decides when to install this complete, immutable proposal.
console.log(result.report.files, result.report.includeSources);
console.log(result.report.changedSourceCount, result.report.editCount);
```

Inputs are captured before asynchronous initialization. Only source records,
include sources, the selected authoring dialect and `imageDestinations` strings
are read; `images`, font, CSS, paper and other presentation getters are not read.
The operation always uses
include expansion. `expandIncludes: false` is rejected rather than silently
changing the requested grammar. Canonical relative source paths are required:
no absolute paths, dot segments, backslashes or URI schemes. Filenames remain
literal source identities; the engine owns URL encoding in rewritten links.

`imageDestinations` is the **complete** inventory of image keys currently supplied
to the book renderer. Omitting it means no images are supplied. It does not grant
access to bytes or load any image. The adapter captures an immutable string array,
trims keys as image admission does, and rejects duplicate trimmed keys. Literal,
root-relative, encoded and URL-shaped keys are supported; source-path restrictions
do not apply to image keys. At most 4096 keys are accepted, each at most 4096 UTF-8
bytes before trimming and 16 MiB in total. The host must keep this inventory
unchanged until applying the plan, alongside its source revision.

With `![Diagram](diagram.svg)` in `guide/old.md`, supplying
`guide/diagram.svg` binds the origin-qualified key; supplying only `diagram.svg`
binds the literal fallback. Moving the chapter can require different rewritten
Markdown in those two books. The native operation uses the supplied inventory
to preserve the actual binding, including when the new directory would otherwise
shadow the old literal key. Images that were unresolved remain unresolved.
Source filenames and asset keys occupy separate namespaces; renaming a chapter
does not rename an asset that happens to share its path.

The result contains owned UTF-8 JSON bytes and a deeply frozen `report` with
schema `fmd-book-source-rename-v1`, `oldPath`, `newPath`, complete ordered `files`
and `includeSources`, `changedSourceCount` and `editCount`. The source count
includes a path-only change; the edit count counts actual destination/include
token replacements. A validated same-path request returns an exact no-op.
`sourceLength` describes the captured input source bytes, excluding path text.

The public adapter validates the returned source count, ordering, roles, paths,
endpoints and aggregate byte budget against its private capture. It never accepts
new files, options or resource grants from a rename report. JSON output is capped
at 128 MiB, source and path text at 64 MiB, and token edits at 16,384. The browser
workbench retains its smaller existing source/path/count limits. These are
logical admission bounds, not exact native heap or wall-clock guarantees.

## Worker API and lifecycle

```js
import { createBookWorker } from "@franken-suite/franken-markdown/book-worker";

const worker = createBookWorker();
try {
  const result = await worker.renameSource(files, "old.md", "guide/new.md", {
    includeSources, profile: "gfm-plus", imageDestinations,
  }, { signal: abortController.signal });
  // Check source revision and image keys, then apply result.report atomically.
} finally {
  worker.dispose();
}
```

Source rename is a one-shot operation. A worker timeout or cancellation physically
terminates native work; it does not merely stop awaiting it. The reply is checked
again against the captured paths and exact input source byte length. Image keys
are copied with the source request; no image/font buffers are transferred. It neither
retains a book renderer nor starts a publication or image/font load. The demo
uses a dedicated client so cancelling a rename cannot cancel preview, proofing
or publication. Source and destination composition remain fenced through their
queued completion task. Reopening a page never replays a prepared operation.

Both APIs require a matching generated WASM package exposing the eight-argument
`renameBookSource` binding, including the image-key inventory.
An older engine fails with `UNSUPPORTED_BOOK_SOURCE_RENAME`; there is no lexical
or JavaScript fallback. Updating source files does not rebuild previously
generated packages or upgrade saved documents automatically.

## Verification

```sh
node --experimental-vm-modules --test wasm/tests/book_source_rename*.test.mjs
```

The JavaScript tests execute production capture, report validation, worker
transport, source collection, history and rename controls. They include a real
Node worker thread with an explicit native ABI adapter, exact source ownership,
undo/redo, image-key capture and staleness, bad receipts, old capabilities, cancellation, silent edits, project
replacement, IME task fencing, host rejection and peer edits after application.
DOM and native adapters are identified as such; these tests do not establish
browser rendering or generated-WASM correctness. Native Rust tests separately
exercise the actual parser/expander, incoming and outgoing destinations, aliases,
include selectors, source preservation, ambiguity and rejection behavior.
