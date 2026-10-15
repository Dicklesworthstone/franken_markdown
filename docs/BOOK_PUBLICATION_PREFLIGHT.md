# Verify internal navigation before publishing an edition

`createBookPublicationWorker().render(files, formats, options, { strictLinks: true })`
checks the captured book's internal navigation before creating its native
publication renderer. The check and all selected PDF/EPUB/site exports share one
source/settings/resource capture and one deadline. There is no fallback to
unchecked output after a refusal, unsupported capability, malformed report,
analysis-budget failure, cancellation or timeout.

```js
import { createBookPublicationWorker } from '@franken-suite/franken-markdown/book-worker';
const publisher = createBookPublicationWorker();
try {
  const edition = await publisher.render([
    { path: 'start.md', source: '# Start\n\n[End](end.md#end)' },
    { path: 'end.md', source: '# End' },
  ], ['pdf', 'epub', 'site'], { title: 'Manual' }, { strictLinks: true });
  console.log(edition.linkReport.summary, edition.outputs);
} catch (cause) {
  if (cause.code === 'BOOK_LINKS_FAILED') console.log(cause.linkReport.chapters);
  else throw cause;
} finally {
  publisher.dispose();
}
```

The ordinary programmatic default remains non-strict. The switch is a publication
request policy, not a new rendering option, project-schema field or WASM ABI.
Non-strict editions keep their previous result shape. Successful strict editions
add the deeply frozen `linkReport`; report bytes do not count as publication
output. A failing check returns `BOOK_LINKS_FAILED`, a bounded message and the
complete decoded report on the error's `linkReport` field. No publication bytes
or partial selected-format results are returned.

## Scope and identity

This reuses `FmdBook.validateLinks` through the existing `links` worker route and
`parseBookLinkReport`. Rust alone expands includes and interprets Markdown. The
existing decoder verifies canonical chapter paths in reading order and recomputes
totals rather than trusting a worker-supplied summary. Use normalized book-relative
chapter paths, as required by the report decoder. Verification checks the exact
captured UTF-8 source-length metric, including include-only sources; each rendered
format must match that metric too.

The scope remains `expanded-html-navigation`: missing chapters/anchors, ambiguous
anchors, invalid local destinations/fragments and missing footnotes. External
URLs, downloads, images and other unchecked links remain explicitly outside the
check. A clean report does **not** claim PDF/UA, EPUB validity, network-link health,
asset completeness or publication-format conformance. Read the report's external
and unchecked totals when assessing coverage.

## Ownership and costs

Full input admission and copying of supplied asset byte views happen once before
verification, so later caller edits cannot change either phase. The checker gets
only copied chapter/include containers sharing immutable strings and the include
expansion policy. It receives no image/font buffers or presentation settings and
does not decode or load publication assets. A separate source-only worker is
released before the retained rendering worker is created. This uses an additional
native parse/startup in exchange for refusing broken books before expensive
publication. Subsequent formats still reuse one retained book with empty source
deltas, without retransmitting assets or recapturing source.

The existing four-MiB report limit is independent of the aggregate publication
output limit. One publication deadline covers capture, checker startup/analysis,
renderer startup and every output; verification does not reset the timer.
Cancellation disposes a checker immediately even while its endpoint is pending.
Late replies/errors cannot start a renderer or disturb a newer edition.

## Publishing workbench

**Publish a complete edition** now starts with **Require clean internal navigation
before publishing** selected. A failed check displays the chapter, finding code,
destination and explanation without creating any publication download. Repair the
source and prepare again, or explicitly deselect the checkbox to publish an
unchecked edition. Disabling verification is never an automatic error fallback.
An older publisher that silently ignores a strict request is refused before any
object URL is allocated; rebuild matching worker scripts instead.

The panel displays up to 100 findings as inert text. The full bounded native
report, not a shortened report or a recalculated optimistic result, remains on
`controls.state.linkReport`. Reports state their checked, external and unchecked
counts and expressly do not certify output conformance or asset completeness.
The existing standalone book-link-checking panel remains available for inspection.

`createBookEditionSession` accepts `strictLinks` (default `false` for standalone
compatibility), exposes `setStrictLinks(boolean)`, and includes `strictLinks` and
`linkReport` in its state. The workbench explicitly enables it by default. Changing
the policy retires pending work, the report, native downloads and derived delivery
ZIPs. The checkbox participates in the raw-editor checkpoint even when changed
programmatically without a DOM event. Reports from failed checks are invalidated
on edits and resource changes too; late failures cannot replace a newer verdict.
This is a session policy, not a persisted source-project setting.

Checks are neither rerun nor claimed again when packaging the same immutable
ready edition into its delivery ZIP. That ZIP's manifest continues to describe
byte integrity only. No new claim of PDF/EPUB conformance or signed verification
is inserted into the manifest.

## Validation

Run:

```sh
node --test wasm/tests/book_edition.test.mjs \
  wasm/tests/book_publication_links.test.mjs \
  wasm/tests/book_edition_verification.test.mjs
```

The unchanged existing edition suite contains 36 cases. The two new suites add
28 publication/preflight and 26 workbench/session cases, for 90 passing tests in
the development environment. The tests run the production
coordinator, source-only adapter, edition session and workbench controller with explicit native-transport and decoder
boundary doubles plus explicit DOM/source/URL adapters. They cover ordering,
all finding codes, input ownership, source metrics, cancellation/reentrancy,
deadlines, rejection/retry, policy changes, stale-report handling, safe bounded
finding display and derived-bundle invalidation.
These are JavaScript orchestration tests, not native Markdown-semantic tests,
generated-WASM execution, browser acceptance or publication conformance.
