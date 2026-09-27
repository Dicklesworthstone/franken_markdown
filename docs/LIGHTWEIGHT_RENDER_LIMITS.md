# Lightweight interactive preview: bounded rendering

The self-contained JavaScript preview (`src/interactive_renderer.js`) is a
reduced Markdown renderer, not the Rust/WASM engine. Its synchronous parsing
must not perform unlimited work on an edited or opened document. This policy
applies only to that fallback. Native previews, the Rust parser, publication
output and source-download limits are unchanged.

## Runtime contract

Each `parseMarkdownClient` invocation owns a fresh, deterministic budget:

| Resource | Maximum |
|---|---:|
| Input source | 32 Mi UTF-16 code units |
| Charged processing / HTML assembly work | 32 Mi units |
| One returned or accumulated HTML string | 32 Mi UTF-16 code units |
| Admitted lines, delimiter entries, table cells and other tracked structures | 262,144 |

The existing recursive-rendering depth policy remains 64. A source below the
input-size ceiling can still exceed work or structure limits. Work units are
charges for scans, transformations, searches and HTML assembly; they are not
elapsed milliseconds or an exact count of VM instructions. These checks do not
claim to bound the browser's later HTML parsing, image decoding or DOM layout.

Exhaustion throws an `Error` with `code = "FMD_RENDER_LIMIT"` and a `limit` of
`source`, `work`, `output` or `structure`. No partial HTML is returned. The error
explains that source remains unchanged and downloadable, and directs large or
expensive documents to the native renderer. A subsequent render receives a
fresh budget, even after a previous failure.

The existing controller evaluates `parseMarkdownClient` before assigning
`preview.innerHTML` and updates `lastRenderedSource` only after success. Thus a
parser failure preserves the last successful preview and flows through the
existing status/error handler. The editable source is not replaced, and no
network access, browser storage, or alternate parser is introduced.

## Removed unbounded paths

Bracket pairs are indexed once per inline buffer, with the original escaped-
delimiter semantics. Missing pairs no longer trigger a new full-suffix scan
for each opening bracket. Interior spaces and blank-line runs inside lists are
consumed once. Heading closing hashes and email-autolink validation use linear
checks instead of ambiguous backtracking patterns. Quote collection no longer
spreads all child lines into a function's argument list.

Remaining potentially repetitive delimiter searches debit the shared work
budget as they advance. Recursive blocks, footnote traversal, reference
expansion and table materialization use the same invocation-wide budget.
Line-array and delimiter/cell allocation are admitted before growing those
structures. Embedded image bindings are checked before validating or copying
large data URIs; small reference-based sources cannot expand HTML unchecked.
HTML accumulators check their length before concatenating another fragment.

## Regression checks

Run the established test entry point:

```sh
node --test tests/interactive_renderer.test.mjs
node --check src/interactive_renderer.js
```

The 23 pre-existing cases retain their assertions. Added regressions exercise
50,000 unmatched brackets, 100,000 interior spaces, long heading whitespace,
130,000 quote lines, nested and escaped pairs, many short links, invalid dotted
email autolinks, source/line/delimiter/table/output admission, repeated searches,
reference expansion, footnotes and clean state after failure.

The VM timeout in the tests is only a hang guard. Exhaustion tests require the
renderer's own `FMD_RENDER_LIMIT` code and resource kind; a VM timeout or generic
`RangeError` does not pass. These Node checks are not browser end-to-end,
CommonMark conformance, Rust, WASM, visual-PDF or performance-gate certification.
