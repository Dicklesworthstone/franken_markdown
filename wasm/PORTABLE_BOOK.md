# Portable book backup and restore

The book publisher's **Portable backup and restore** panel saves the exact
chapter and include-only sources together with the currently authorized images
and supplied fonts. A downloaded `book.fmdbook.bundle.json` can reopen that
collection without asking you to find each resource again. No Markdown parsing,
image decoding, font instancing or publication rendering is done by this feature.

## Publisher workflow

Choose **Prepare portable backup (includes images and fonts)** in the publisher's
**Prepare and download** section, then use the existing **Download** link.
Preparation alone does not save a file. The bundle captures current editor text,
source paths and roles, order, presentation settings, image destinations and exact
bytes, and all supplied font slots, filenames and optional weight pins. Untouched
source retains its BOM and original LF, CRLF or CR line endings.

To reopen it, choose the file in **Reopen a portable book and review resource
authorization**. The workbench validates the entire file, shows its source and
resource metadata as text, and asks whether to replace the collection and approve
the embedded resources. Approval installs everything in one observable collection
revision. The publisher detaches its previous local-library binding through its
existing project-replacement hook; the imported book is not silently autosaved
over an unrelated named book. Prepare a fresh preview or publication afterward.
The native engine still decides whether each supplied font/image is supported.

**Cancel portable operation** cancels file/confirmation waits and cooperative
resource encoding or decoding. Editing, source composition, changing resources,
page suspension or disposal also retires pending work. Revision and raw editor
checks block stale installation and download activation, including changes that
did not dispatch an input event. Declining a restore leaves the current collection
intact. Suspension retains source in memory but revokes both images and fonts.

## Keep source-only saves when resource portability is not wanted

`book.fmdbook.json`, individual source downloads, and the local browser library
keep their existing source-only behavior. They do not acquire image/font bytes,
filesystem handles, URL-fetch permission, or portable-file authority. A source-only
restore still revokes the current resources. The portable file is a distinct,
explicit choice, not an automatic change to autosave policy.

The portable envelope identifies itself as `franken-markdown-portable-book`,
version 1, and contains an unchanged version-1 or version-2 source project plus
`images` and `fontAssets` arrays. Resources use canonical padded base64. Source-only
readers reject this envelope instead of silently dropping its resources.

## Trust and resource limits

Portable files are **unencrypted JSON**, not secure storage. They contain every
currently authorized image and supplied font, even resources not referenced by a
chapter. Share only resources you have permission to redistribute. No URLs are
fetched, no file access is granted, and opening the file does not install fonts
in the operating system or certify resource safety or embedding rights.

Admission preserves the existing limits: 128 chapters plus 128 include-only
sources, 4 MiB per source, and 16 MiB total source/path bytes; 128 images with
1 byte through 8 MiB each and 32 MiB total; five font slots with 1 byte through
8 MiB per slot and 32 MiB total. Escaped source JSON retains its 64 MiB ceiling.
The complete portable file is limited to 192 MiB to allow base64/JSON overhead.
Counts, paths, font metadata, duplicates and decoded resource sizes are checked
before large base64 decoding. Invalid UTF-8, noncanonical base64, unsupported
schemas and excess budgets reject the import before any collection mutation.

These are input and retained-data limits, not a whole-process heap ceiling.
`JSON.parse`, source validation and source/byte snapshot copying remain bounded
synchronous operations. Long base64 loops yield; waiting for a selected file or
asynchronous host confirmation can be aborted. Cancellation does not interrupt
JavaScript already executing synchronously or dismiss a native browser dialog.

## Embedding the collection API

The implementation remains in `demo/book_collection.mjs`; it is a workbench API,
not a new top-level package export. `portableDownload({ signal })` resolves to
`{ filename, blob, revision }`. Hosts control the actual save and object URL
lifetime, and must fence their own unsaved UI state.

`readPortableBookProject(file, { signal })` returns a frozen metadata review,
not mutable decoded resources. Record the target collection revision before
reading, obtain explicit user approval, and call
`collection.replacePortableProject(review, expectedRevision,
{ authorizeResources: true })`. A forged, discarded, consumed or stale review
cannot install. Call `discardPortableBookProject(review)` when abandoning a
review; it is harmless after ownership transferred on successful restore.

`createBookControls` exposes `preparePortable()` and
`importFiles([file], "portable")`. A host must supply its own explicit confirmation
handler for portable imports; the legacy source-only default is not resource
authorization. The production publisher supplies `window.confirm`.

## Verification

```sh
node --test wasm/tests/book_portable*.test.mjs
```

Codec/collection tests exercise exact round trips, private ownership, limits,
atomic restore, authorization, invalid input and cancellation. Controller tests
execute the production controller with DOM, object-URL and publication-worker
doubles. Package tests check the static helper import closure in the manifest
and both assembly scripts, then load that staged helper graph without WASM.
They do not establish compiled native rendering or visual acceptance. Existing
native/WASM publication gates remain separate and are not replaced by these tests.
