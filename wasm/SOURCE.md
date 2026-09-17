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

## Live editor controls

`demo/flow-canvas.html` includes Open Markdown, an editable filename, Prepare
Markdown and a separate source-download link. Local recovery has an explicit
Autosave checkbox, Save draft now, Refresh recovery, Restore stored draft and
Forget stored draft. Both replacing source from a file and restoring a draft ask
for confirmation. Refresh reads only; autosave starts unchecked on each page
lifecycle. No existing draft is silently taken over by a new editor tab.

`demo/flow-source.js` is a separate module entrypoint with no renderer or generated
WASM imports. Source open/download remains usable when storage is unavailable,
and source plus recovery remain independent of preview startup or shaping errors.
Opening/restoring emits `fmd-document-replaced` before the ordinary input event;
the preview revokes local image grants, invalidates exports and recreates its
native session before those new document references can resolve. Programmatic
image-reference insertion emits input too, reaching autosave and source-download
invalidation instead of updating only the Canvas preview.

Page hiding disposes draft connections and revokes source download URLs without
claiming a last-second save succeeded. Back/forward-cache reentry reconnects
storage with autosave off and preserves untouched imported source bytes in memory.
Only already acknowledged draft transactions are reported as saved; closing a tab
before the next acknowledgment may lose changes. After a storage error or conflict,
Refresh recovery reconnects and offers the currently stored version. The local
editor is never replaced by a save acknowledgment.

Run all source/recovery checks (including DOM-control and package inventories):

```sh
node --test wasm/tests/flow_document.test.mjs wasm/tests/flow_draft_*.test.mjs wasm/tests/flow_source_*.test.mjs
```

The source-entry tests load the actual separate entrypoint with no WASM artifact,
unavailable IndexedDB, explicit EventTarget element/window doubles, and native
Node File/Blob/object URLs. Inventory tests check that both package assemblers
ship the new entrypoint and its dependencies. These are not browser screenshots
or native IndexedDB proof.

A separate native-browser gate is provided:

```sh
CHROMIUM_PATH=chromium node scripts/check-flow-documents.mjs
```

It uses a temporary browser profile, an owned debugging pipe and a loopback test
server, without third-party testing dependencies. The browser checks exercise
native IndexedDB transaction conflicts, aborted writes, tombstones, corrupt
records, disposal/version changes, source round trips and an actual page reload.
They create and remove only uniquely named test databases, never the editor's
real draft database. The temporary profile is retained for inspection. In an
isolated test container without sandbox support, the explicit
`FMD_BROWSER_NO_SANDBOX=1` option is available; it is not the default. Browser
policy, launch or navigation failures fail the gate, never count as passing or
fall back to test doubles. The source/storage gate is independent of the existing
Rust/generated-WASM render-parity gates.
