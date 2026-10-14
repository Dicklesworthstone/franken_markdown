# Book-wide source undo and redo

The workbench's **Book-wide source undo** controls sit beside the source editor.
They recover chronological text changes across chapters and include-only files,
including ordinary typing and atomic, multi-file find/replace transactions.
Typing after a replacement no longer prevents recovering that replacement: undo
the newer edit first, then undo the batch. The existing replacement-specific
history remains separate; this timeline observes the source changes it installs.

Use **Undo source edit** / **Redo source edit**, or Ctrl/Cmd+Z and
Ctrl/Cmd+Shift+Z in the source textarea (Ctrl+Y also redoes). Other text fields keep
their own native undo behavior. A successful undo navigates to an affected source
and positions the caret at the changed range. Navigation failure is reported
separately and does not pretend the completed source transaction was rolled back.

## Scope and ownership

Only source text is replayed. Book metadata, publishing profiles, paper geometry,
image bytes, font assignments and access grants are not read or restored by the
history controller. Presentation/resource changes preserve this source timeline.
Replay uses the existing checkpoint-checked `controls.applySources` transaction;
normal collection observers invalidate obsolete previews and exports. No renderer
or worker is started just to undo an edit.

Undispatched textarea/path/role edits are refused rather than overwritten. The
controller does not call `captureProject`, serialize a book or clone resource
payloads to perform undo. The collection still validates the complete proposed
source transaction, so rejection does not consume history. The history plan is
acknowledged only after the host installs its exact source set and the returned
revision remains current. A newer concurrent edit is preserved; untrustworthy
history is retired rather than replayed against the wrong source.

Source additions, removals, renames, role/order changes and different-source
project recovery establish a fresh history boundary. Successful project imports
also explicitly reset history through the application's existing import callback.
An identical-source configuration change is safe to retain because undo changes
text only. History is session-only, not an autosave or persistent backup. Clear
source history does not edit the book. Page suspension clears the history and
resume starts from the retained current source; disposal releases subscriptions.

## Grouping and memory

Contiguous typing/deletion in one source, with the same input kind and no more
than a one-second gap, groups into one step. Caret movement, clipboard edits,
source navigation and separate operations break the group. Composition partial
inputs and a final composition commit form one step; undo is blocked while the
editor or source host is composing/importing. Cancelable browser history input
is intercepted; an uncancelable native history change is observed as a source
edit, not applied a second time.

The engine keeps one current source snapshot and reversible replacement deltas,
not a whole-book snapshot per keystroke. Default limits are **100 entries** and
**16 MiB of UTF-16 delta payloads**, shared by undo and redo. These are logical
payload limits, not a bound on JavaScript heap usage: the current snapshot,
string representation, temporary strings and object overhead are excluded.
Oldest entries are evicted visibly. An individually over-budget edit establishes
an explicit boundary rather than retaining an incomplete inverse. Unchanged
captures consume no entries and preserve redo; a new edit after undo discards
the divergent redo branch.

The engine preserves exact code units, including BOMs, mixed CR/LF/CRLF endings,
astral characters and intermediate incomplete Unicode drafts. Collection replay
admission still owns valid-source limits; the history engine does not introduce
a competing Markdown parser or normalization policy.

## Verification

```sh
node --test wasm/tests/book_source_history*.test.mjs
```

The implementation run passes **44 Node tests**: 20 engine tests, 21 controller
checks and three package/orchestration checks. The engine suite includes 4,000
seeded differential edit/undo/redo operations against a full-snapshot oracle.
A two-source, 4 MiB fixture retains four delta bytes for two typed characters.
Controller tests cover input capture ordering, typing/clipboard/composition,
source navigation, multi-file replay, revision conflicts, failed-write retries,
settings/resource preservation, budget reporting, shortcuts and lifecycle.

Tests import the complete production engine/controller through normal ESM,
without rewriting or extracting implementation bodies. DOM and source-host
transaction doubles are explicit. Package checks execute each actual demo-copy
loop in an isolated fixture, byte-check the shipped history module and import
and exercise that copied module; unrelated copied entries are placeholders.
Application-orchestration tests execute the complete application body with named
factory doubles. The manifest and both assemblers include the new module, and
both native package gates invoke the new test suite.

These results do not claim real-browser event/OS IME behavior, generated WASM,
worker rendering, IndexedDB, native Rust execution, full application startup or
a complete package build. The authoring host has no Rust/Cargo/rch/DSR tools and
no installed Chromium executable. Existing native/package gates remain required
for those separate verification scopes. No runtime dependencies, package version,
WASM size budgets or workflows change.
