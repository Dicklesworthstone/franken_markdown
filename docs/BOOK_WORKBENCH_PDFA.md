# Archival PDF profiles in the book workbench

The **Typography and publishing profile** panel now exposes three PDF policies:
ordinary PDF, PDF/A-2b emission, and PDF/A-2b emission with the strict writer
policy. These select the existing chapter-aware native writer described in
`BOOK_BROWSER_PDFA.md`; they do not independently validate or certify PDF/A.
Strict writer policy is separate from the edition's internal-link checks.

## Author, apply, export and save

Choose a policy in **PDF archival emission**, then **Apply publishing profile**.
Until Apply, proofs, exports and saves still use the previously applied settings.
A single selection represents both `pdfA` and `pdfAStrict`, avoiding an impossible
ordinary/strict combination. Missing or unknown selections fail rather than
silently becoming ordinary PDF. Unsupported native packages and strict writer
refusals keep the existing failure behavior: no ordinary-PDF fallback.

Apply changes only the publishing profile. It does not capture or rewrite the
source editor, change paper geometry, copy resource payloads or start rendering.
Changed profiles advance the existing render-configuration/resource revisions,
so collection observers invalidate old proofs, previews and prepared downloads.
A no-op Apply leaves revisions and outputs alone. Selective source editing keeps
the policy; changing the policy forces a complete worker capture instead of a
source-only update against obsolete settings.

Discard restores the applied policy. Native defaults are staged and require
Apply before removing the policy. Unrelated typography or template edits preserve
archival settings and unchanged imported CRLF template text. Concurrent settings,
resource or structural changes invalidate dirty drafts; pristine controls follow
the new profile. Import/composition blocks Apply. Suspension discards drafts,
and disposal removes subscriptions and event handlers.

## Source-project compatibility and resource authority

An enabled archival policy uses **source-project schema 5**. Versions 1–4 cannot
carry either archival field, even an explicit `off` or `false`. Older workbenches
therefore reject the new project instead of silently losing publication intent.
Invalid/future versions and inconsistent strictness remain errors.

Ordinary books keep the existing minimal schema: 1 for ordinary chapters, 2 for
include roles, 3 for explicit paper geometry, or 4 for non-archival publishing
profiles. Resetting archival settings can return to the appropriate earlier
schema. Existing versions are not migrated merely by opening the workbench.

The same collection serializer and normalizer carry the policy through source
project downloads, the local library's existing source save/read path, and the
project inside a portable backup. No second serializer, storage database version,
new resource grant or native parser is introduced. Portable wrapper schema 1 is
unchanged; its embedded source project uses schema 5.

Source-only restore retains settings and exact chapter/include text but revokes
image/font grants as before. Portable restore still requires explicit resource
authorization and a current collection revision; the admitted source and resources
are installed together. Saving an archival profile grants no filesystem, network
or resource access. Publication needs a matching rebuilt Rust/WASM package.

## Verification boundary

```sh
node --test wasm/tests/book_publication_archival*.test.mjs \
  wasm/tests/book_publication_project.test.mjs \
  wasm/tests/book_publication_controls.test.mjs
```

79 tests passed: 42 new authoring/persistence tests and 37 existing profile
regressions. The original profile-control suite is byte-identical to its previous
Git blob. The existing project suite changes only its unknown-future-version
expectation from 5 to 6, because 5 is now supported; other assertions are unchanged.

Production collection, option admission, profile UI, source/portable codecs,
resource stores and collection-capture selection run through normal imports.
DOM/source-host boundaries and image/font payloads are explicit test fixtures.
The tests exercise policy admission, atomicity, no-op/stale/reentrant operations,
exact-text/source-only/portable round trips, source-delta preservation, profile
changes, draft lifecycle and resource authorization. They do not render documents.

IndexedDB/browser acceptance, compiled Rust/WASM execution, visual output and
PDF/A conformance were not run here. Local-library routing was inspected; its
shared codec was exercised, not a browser database transaction. No native build,
new dependency, parser, package gate, Actions workflow or release was added.

## Retained PDF proofs and exact-byte downloads

The proof session captures the applied archival policy before invoking its worker
or notifying observers. A PDF header alone is not sufficient: an archival result
must acknowledge the exact profile and strictness before its bytes can become a
retained proof. Missing or mismatched receipts fail with `UNSUPPORTED_BOOK_PDFA`;
unsolicited archival fields on an ordinary proof fail with `INVALID_BOOK_PDF`.
No failed receipt creates a viewer/download URL or becomes the current proof.
A completed-but-invalid idle worker is released, and retry is explicit.

Successful proof objects retain `pdfA`/`pdfAStrict` with an owned Blob of the exact
returned byte view. The viewer status reports that receipt on initial display
and reuse, while explicitly distinguishing it from independent conformance
validation. It never labels a proof from the current draft or live option object.
Changing a draft alone does not relabel existing output. Applying a changed
profile releases the old proof and download URL; source-only edits keep the
correct native policy through the existing retained-worker transport.

```sh
node --test wasm/tests/book_publication_archival*.test.mjs \
  wasm/tests/book_publication_project.test.mjs \
  wasm/tests/book_publication_controls.test.mjs \
  wasm/tests/book_pdf_archival*.test.mjs wasm/tests/book_pdf_proof.test.mjs
```

116 tests passed together: 65 new tests across the authoring/persistence/proof
integration and 51 existing regressions. The 23 new proof cases include the real
profile/proof controllers, receipt failure, byte-view ownership, URL lifecycle,
policy/source races, suspension and an explicit old-package refusal. Before the
proof change, 20 of those 23 tests failed; all 23 pass after it. The existing 14
proof regressions and their source/worker helper remain byte-identical to main.

Two integration cases use real Node worker threads, transferable buffers and the
production collection/profile/proof/session/transport/cache modules. They exercise
include deltas, source-set reordering, source-project restore with resource
revocation, strictness changes and explicit ordinary-mode recovery after an old
native-package refusal. Only the native renderer is the existing explicit test
double; its header-prefixed JSON observations are envelope probes, not real PDF
artifacts. No compiled-WASM, native output fidelity, browser viewer, IndexedDB
transaction or conformance claim follows from these tests.
