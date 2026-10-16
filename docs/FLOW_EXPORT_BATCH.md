# Atomic multi-format flow publication

Flow sessions expose `exportDocuments(requests, token)` for publishing up to four
unique HTML/PDF/EPUB/SVG outputs from one revision-fenced document capture. This
is additive; `exportDocument` keeps its existing contract.

The synchronous coordinator validates every request before touching the session,
owns source/options before its first await, scans and copies the retained image
inventory once, then runs the requested native renderers in caller order under
one physical export slot. PDF-only explicit images join that shared inventory.
A source/layout change, disposal, invalid renderer reply or later renderer failure
rejects the whole operation. Earlier bytes stay private and no partial result
array is returned.

Worker sessions negotiate `supportsExportBatches`. A supported batch is one RPC
and therefore one queue position, cancellation signal and deadline. An edit
enqueued after the call cannot run between formats. Legacy workers fail with
`UNSUPPORTED_WASM_PACKAGE`; the client never decomposes a requested transaction
into separately observable exports.

Each format retains its existing `maxOutputBytes` limit (maximum 64 MiB). The
combined returned batch is additionally capped at 128 MiB. Formats must be unique,
which keeps renderer-specific options, result order and MIME validation
unambiguous. Output buffers are copied at the worker ownership boundary and
transferred together only after the complete batch has passed validation.

Verification target:

```sh
node --test wasm/flow_export.test.mjs wasm/flow_export_worker.test.mjs \
  wasm/flow_export_batch.test.mjs
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext \
  wasm/flow_export_types_test.mts
```

The batch tests exercise the production export coordinator, worker protocol,
worker queue and transfer boundary. Native rendering in those tests is an
explicit double; generated-WASM parity remains the responsibility of the existing
package smoke gate.
