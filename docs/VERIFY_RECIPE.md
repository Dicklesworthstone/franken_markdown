# Verify CI recipe (bead yo83.4)

`fmd verify` renders a document through the same layout+pagination pipeline as
the PDF writer and emits a stable-schema JSON report: per-page text runs,
internal-anchor audit, render warnings, and horizontal overflow findings, with
a content digest. This recipe wires it into CI so documentation drift and
broken anchors fail the build instead of quietly shipping.

## The deliberately-broken fixture

`docs/verify-fixtures/broken.md` exercises every finding class the verifier
reports:

- a dangling internal anchor (`[bad](#nope)`),
- a horizontal-overflow run (a long unbreakable token with `--pdf-` style
  narrow margins simulated by a wide unbreakable token),
- a missing-glyph character (a codepoint outside every bundled face, e.g. an
  emoji on the default profile).

`fmd verify` exits `1` when findings exist, so a CI step can gate on the exit
code alone; the JSON artifact preserves the full findings list for triage.

## Workflow

`.github/workflows/verify-docs.yml` (workflow_dispatch + push on docs paths):

1. checkout + stable toolchain,
2. build `fmd`,
3. run `fmd verify` over the fixture (expect exit 1 — asserted),
4. run `fmd verify` over a clean fixture (expect exit 0 — asserted),
5. upload both JSON reports as artifacts.

## Logging contract

`fmd verify` writes one JSON document to stdout (schema v1: verdict, findings
with stable codes, digest) and keeps stderr for progress lines. CI logs read
as: one finding = one code + detail line; the digest lets a job assert
"this document's verification is unchanged since the last run" cheaply.
