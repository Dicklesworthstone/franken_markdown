# Replace and remove native workspace images

Newly generated native workspaces include **Document images** beside Save HTML.
The inventory lists embedded destinations and byte sizes, including resources
that the current source does not use. It does not guess reference counts or
parse Markdown a second time.

Select a binding and choose a local static PNG or JPEG, then **Replace selected
image**. The image bytes change at the existing destination; Markdown source,
alt text, document settings and fonts do not change. Every reference to that
binding can use the replacement. The file extension is not trusted: the existing
image header checks and browser decoder validate the contents before native
worker preflight. Files are limited to 8 MiB, 16,384 pixels per side and 24 million
pixels. Reading and browser decoding share a ten-second deadline.

**Remove selected image** asks for confirmation. Removal reclaims embedded
resource capacity but does not remove Markdown references. Remaining references
may produce missing-image diagnostics. The original local file is never changed
or deleted. Save HTML before removing resources that may be needed later;
textarea/source undo does not restore embedded resources. There is no automatic
pruning, remote fetch, filesystem discovery or browser-storage write.

Use **Save HTML** to retain committed image changes. Save Markdown retains source
only; it does not carry this resource store. Reopened dialog shells are rebuilt
from the committed runtime data after parsing, with no selected local file or
pending operation restored. Resource-only edits activate the unload warning.
A download request is not treated as proof that the workspace was saved.

## Transaction and cancellation behavior

The runtime preflights the proposed complete image set in the same disposable
worker slot used for document settings, fonts and exports. Saved runtime JSON
and the preview change together only after successful preflight. File-reading,
image-decoding, worker or publication failures keep the original resources.
Cancellation uses an operation-owned handle: an old dialog cannot cancel a newer
font/settings transaction. Source edits, selection changes, composition, view
changes, read-only state, close/Escape and page suspension fence pending work.
Retries are explicit, with no main-thread rendering fallback or hidden job queue.

The manager never writes the source textarea or its lossless raw-source block.
Its image-preflight preview uses the current textarea view, as the existing font
authoring controls do; source saving/export remains owned by the main controller.
No-op byte replacements keep the document revision and saved resources unchanged
and do not start a native worker.

## Native workspace API

```js
// Read-only metadata; stable identity until the image store changes.
const inventory = runtime.imageAssets; // [{destination, bytes}, ...]

const operation = runtime.beginImageChanges([
  {destination: 'figures/chart.png', bytes: replacementBytes},
  {destination: 'figures/old.png', remove: true},
], markdown, previewElement, {scale: 1}, isCurrent);

await operation.promise; // Returns the committed immutable inventory.
operation.cancel();     // False after this exact operation releases its slot.
```

Provide one through eight changes. Destinations must already exist, are trimmed
and must be unique within the batch. Removal and replacement are mutually
exclusive. Replacement bytes must use an ordinary, non-shared ArrayBuffer or
exact typed-array/DataView range; admission rejects accessors, unsupported fields,
sparse arrays, detached/shared buffers and duplicate keys. Byte views are copied
before a staged transaction escapes to the caller.

Each replacement is at most 8 MiB and a batch at most 16 MiB. The 128 MiB combined
image/font budget applies to the final resource set, so replacing at capacity is
possible and removal frees capacity for later imports. Surviving binding order
is stable. Existing append-only `stageImages` insertion remains compatible.

At the data-only renderer layer, `stageImageChanges` exposes `images`, `changed`,
`commit(publish)` and revision-checked `rollback(publish)`. It does not decode image
pixels or perform rendering itself. Hosts must use native preflight before
publishing untrusted images; the browser workspace uses `beginImageChanges` for
that reason. Committed changes advance `documentRevision`, invalidating retained
PDF proofs and stale worker transactions through the existing revision contract.

## Rebuilding and verification

Both the matching WASM shell and standalone runtime must be rebuilt. The controls
are appended to the already-bundled `src/interactive_import.js`; no new runtime
module or package dependency is needed. Older runtimes without image-change
support do not expose the controls. Already-exported HTML is unchanged.

```sh
node --test wasm/native_workspace_image_changes.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/native_workspace_image_changes_browser.py
```

The browser harness requires Playwright and Pillow and writes to a fresh temporary
directory. It runs the production image controls, bootstrap, resource store and
worker transport with real Chromium FileReader, PNG/JPEG decoding and Workers.
Its shell/native bindings are explicit adapters. Node tests exercise real resource
transactions with scoped DOM/transport/ABI adapters for publication boundaries.
These checks do not claim compiled Rust/WASM image-codec validation, a full release
build or full-controller Save HTML validation. Browser reopen coverage reparses
a DOM snapshot carrying the committed runtime data and transient dialog shell.
