# Book source inspection

Open `demo/book.html` from the assembled package and use **Inspect source quality
and accessibility**, immediately before the publication controls. **Inspect all
source chapters** runs the existing Rust `documentStats` and `accessibilityAudit`
APIs for each chapter in reading order. No alternate Markdown parser is added.

The panel shows per-chapter structural findings, accessibility findings, word and
source counts, reading/speaking estimates, readability estimates and outlines.
Choose a chapter, filter findings by severity or failed check, and page through
50 items at a time. **Open chapter source** opens that chapter without altering
its source, image grants or replacement history. The current APIs do not provide
exact source spans for these findings; the UI does not guess spans from matching
heading text or error messages.

## What the checks mean

Source-structure findings are those produced by the Rust document-intelligence
API, including its heading, anchor and footnote checks. Accessibility findings
come from the engine's filtered authoring-time audit: missing alternative text,
heading-level skips, generic link text and missing table-header text. This ABI
runs with default PDF options; it does not accept the publisher's image/font
assets or presentation settings. Those inputs are not transmitted for inspection.

The scope is **original, unexpanded source chapters**, not the assembled book.
Transclusions can change which headings and links exist in publication. This
report therefore does not establish cross-chapter link correctness, successful
image embedding, PDF/UA compliance, EPUB conformance, or final visual quality.
Use the book preview and inspect the actual publication output as well. An
inspection result never blocks exporting.

The summary distinguishes `no-findings`, `findings`, and **`incomplete`**. A failed
or invalid engine response is a failed check, not an empty successful result.
Other checks and later chapters still run. Aggregate statistics cover only
chapters whose statistics completed; coverage is displayed explicitly. Missing
statistics are not replaced with zeros in the chapter view. Readability and
reading times are estimates from the existing engine, not universal quality
scores or precise timing predictions. Book reading times sum chapter estimates;
readability scores are not averaged into an invented book-wide grade.

## Source ownership and cancellation

Inspection is manual and uses a separate disposable worker. Cancelling it cannot
cancel a publication export or a preview. Edits, raw settings changes, chapter
selection, imports, source replacement and image revocation retire stale results
and their downloadable report. Source composition and imports disable inspection
and navigation until the active operation ends. Page suspension clears inspection;
returning from the back/forward cache does not restore a stale report.

Every captured chapter is fingerprinted with SHA-256 over its exact UTF-8 source,
including BOM and original line endings. The host revalidates chapter order,
paths, byte counts, fingerprints and report schema before displaying the report.
It also recomputes aggregate verdicts instead of trusting a response's claimed
summary. Editor checkpoints are checked after asynchronous validation and before
navigation or download activation. Late replies cannot replace newer results.

**Prepare inspection report**, followed by **Download inspection report**, saves
`book-inspection.json`. Preparation does not itself save a file. Reports contain
filenames, source fingerprints, headings, metrics and finding text, which can
include excerpts; review them before sharing. They omit complete source-body
fields, asset bytes, file handles, and the PDF audit's text-layer pages. This is
not an encrypted or anonymized report, nor a source backup.

## API and limits

The existing book worker accepts `render(files, "inspection", options, request)`.
For this source-only route, publication options are ignored and assets are not
copied or transferred. The result has format `book-inspection`, JSON MIME type,
UTF-8 bytes and the original source-byte count. Its schema is
`fmd-book-inspection-v1`, with `scope: "unexpanded-source-chapters"`, ordered
`chapters`, independent `statistics`/`accessibility` check states, and a summary.
Each failed check is represented as `status: "error"` with a bounded message.

The workbench admits up to 128 chapters, 4 MiB per chapter and 16 MiB total source
plus paths. A check admits at most 4096 findings, 4096 outline entries, 4096 UTF-8
bytes per detail, and 128 KiB of retained finding/outline text. The complete JSON
report is bounded to 2 MiB, including escaping. An exceeded check limit is an
explicit failed check; an exceeded whole-report limit refuses the report. The UI
shows at most 200 outline headings with an explicit count; the downloadable report
retains the full admitted outline. The worker deadline defaults to two minutes.
These are admission and lifecycle limits, not a bound on the Rust engine's
intermediate heap. Accessibility inspection can be expensive because its current
API materializes a default PDF verification view.

Web Crypto on a secure origin is required for fingerprints. Missing engine APIs,
engine failures and unavailable workers are surfaced without changing the editor
or storage. No network fetch, persistent storage, new dependency or GitHub Actions
change is introduced by inspection.

## Verification

```sh
node --test wasm/tests/book_inspection*.test.mjs
```

Tests exercise the production adapter, report validator, panel, controller,
worker client, request installer and ingress validation. Engine reports, DOM,
source host and loopback transport are explicit doubles; native Node Web Crypto,
UTF-8 encoding, structured-clone transfers, Blob and object-URL lifetimes are
used. Package tests inspect the actual entrypoint and both assemblers. These are
not generated-WASM or native-browser acceptance results. Run the repository's
normal build and browser gates before claiming real engine interoperability,
visual/accessibility acceptance or complete publication conformance.
