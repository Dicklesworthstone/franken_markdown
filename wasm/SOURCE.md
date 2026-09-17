# Original Markdown and local draft recovery

The source-document modules in `wasm/demo/` operate independently of rendering:
`flow_document.mjs` reads bounded UTF-8 files and prepares Markdown downloads;
`flow_draft_store.mjs` stores one opt-in source draft in IndexedDB;
`flow_draft_session.mjs` coordinates autosave, explicit recovery and conflicts.
They never persist image bytes, file handles, authorization grants, worker tokens,
rendered HTML or exported PDFs. The 4 MiB source admission and malformed-Unicode
checks are shared with the existing flow facade.

Opening a file is transactional with respect to the editor: encoding, size and
filename are checked before replacement. Edits made during reading or replacement
confirmation invalidate the pending open. The host's replacement callback must
revoke the previous document's image authorization before notifying the preview.
Markdown downloads work even when rendering fails and require a separate user
click; preparing a Blob is not a claim that the user saved a file to disk.

UTF-8 BOMs and line endings round-trip through file reading and Blob generation.
Since HTML textarea values normalize newlines to LF, source controls retain the
original string while the imported view is unchanged. Edited text uses the
textarea's LF convention; restoring the exact imported view also recovers its
original bytes. No Unicode normalization or replacement decoding is performed.

Draft saving is disabled until explicitly enabled. An existing draft is offered
for recovery, never silently loaded or overwritten. Restoring rereads storage,
checks for intervening editor changes, and requires host confirmation. Save and
forget compare the observed revision inside a single IndexedDB readwrite
transaction. Another tab's change pauses autosave with `DRAFT_CONFLICT`, leaving
both that stored version and the current local editor untouched. Refresh recovery
before explicitly restoring or forgetting the observed version. Forget writes an
incremented empty record so stale writers cannot recreate an old revision.

Only transaction completion acknowledges persistence. Quota, timeout, corruption,
closure and abort failures leave the source in the editor and pause saving. One
physical write is retained; subsequent edits coalesce to the latest document,
not a queue of complete snapshots. Disabling cancels future scheduled saves; an
already-dispatched transaction may still commit. Forget removes the acknowledged
stored source, not the current editor. Browser storage may be cleared or evicted;
a local draft is not an encrypted vault, cloud sync, or a substitute for backups.

Run the focused tests with:

```sh
node --test wasm/tests/flow_document.test.mjs wasm/tests/flow_draft_session.test.mjs wasm/tests/flow_draft_store.test.mjs
```

The source tests use File/Blob and explicit element doubles. Draft-session tests
use a store double; store tests drive the actual adapter with explicit IndexedDB
event-contract doubles. They do not assert native browser storage or Rust/WASM
renderer acceptance. No source persistence requires a linked service or network.
