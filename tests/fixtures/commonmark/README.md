# CommonMark spec fixtures (dev-only test data)

`spec.json` is the **official CommonMark conformance suite**, vendored as test
data only. It is never compiled into the library or any shipped artifact — it is
read at test time by `scripts/commonmark-conformance.sh`.

- **Source:** https://spec.commonmark.org/0.31.2/spec.json
- **Spec version:** CommonMark 0.31.2
- **Vendored:** 2026-06-29
- **Examples:** 652, across 26 sections
- **Format:** a JSON array of `{markdown, html, example, start_line, end_line, section}`.

## Why it's here, and what the harness measures

`franken_markdown` is an intentionally *styled* renderer: its HTML emitter wraps
output in `<main class="fmd">`, adds heading `id` anchors, and emits
syntax-highlight `<span>`s in code. CommonMark's reference HTML is bare. So the
conformance harness normalizes fmd's output (strips the document wrapper, heading
`id=` attributes, and `tok-*` highlight spans) before comparing it to the spec's
expected HTML.

The resulting number is therefore a **normalized-HTML match rate** against the
official suite. It is a *lower bound* on parser correctness — a mismatch can be a
real parse gap **or** an emitter-formatting difference. The per-example gap ledger
distinguishes them. The number is committed as a ratcheted floor so conformance
can only go up.

Do not edit `spec.json`; refresh it from the upstream URL if bumping the spec
version, and update this provenance block.
