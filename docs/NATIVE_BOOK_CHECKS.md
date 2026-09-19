# Native book navigation checks

Check the expanded publishing book without creating any publication:

```sh
fmd book ./manual --check-links
fmd book ./manual --check-links --json
```

The command uses the same Rust navigation checker as the browser workbench. It
loads the native book manifest, chooses the published chapters (including the
`include_only` exclusions), expands includes under the existing source limits,
and checks the retained parsed book. It does not render pages or load images,
fonts, stylesheets, presentation configuration, or PDF timestamp metadata. It
does not construct publication destinations, create output directories, or
replace files.

`--check-links` rejects explicit output and presentation flags, including
`--out`, `--out-dir`, `--to`, `--css`, `--font`, `--title`, `--author`, `--lang`,
and `--max-pdf-image-bytes`. It also conflicts with `--deny-broken-links` and
`--robot-triage`. This prevents an apparent export command from silently
becoming a read-only operation. `--max-input-bytes`, `--json`, `--no-config`, and
`--no-color` remain accepted. Presentation configuration is not read in this
mode regardless of `--no-config`.

## Data, diagnostics, and exit status

With `--json`, stdout contains the unmodified `fmd-book-link-report-v1` core
report with `scope: "expanded-html-navigation"`. It is not a publication
receipt. Chapter results appear in reading order and include local checks,
external URLs, other unverified local resources, and every admitted finding.
The summary totals have the same schema as the browser report reader. The
whole check and bounded serialization finish before any report bytes are
written; a validation/expansion failure does not emit a partial report.

Without `--json`, stdout is empty. The summary and chapter-scoped findings go
to stderr with source-derived strings escaped for terminal display. The human
summary identifies unverified categories and states that no publication outputs
were written. Findings are not invented line/column positions: a reference may
originate in an included source.

Source-discovery warnings remain separate stderr diagnostics. In JSON mode,
each warning is one JSON line with `level: "warning"`,
`code: "book_input_warning"`, and an escaped `message`. Such warnings do not
alter the portable core report or become navigation findings.

| Exit | Meaning |
|---|---|
| 0 | Check completed with no findings from these navigation checks. |
| 65 | Check completed with findings. With `--json`, the complete report is still on stdout. |
| 64 | Invalid arguments or incompatible modes. |
| 66 | Source, manifest, role selection, UTF-8, or include loading/expansion failed. |
| 70 | Core validation or report admission failed, including budget exhaustion. No report is emitted. |
| 74 | Output I/O failure. |

For failures other than a completed findings report, JSON errors use the
existing stderr error envelope and stdout is empty. Report writing retains the
CLI's existing broken-pipe handling. As with other CLI diagnostics, write
failure can interrupt stderr output.

## Opt-in publication guard

Keep the existing rendering behavior, but refuse a publication containing
navigation findings:

```sh
fmd book ./manual --to html --out-dir ./site --deny-broken-links --json
fmd book ./manual --to pdf --out ./manual.pdf --deny-broken-links --json
fmd book ./manual --to epub --out ./manual.epub --deny-broken-links --json
```

The guard checks the **same retained parsed book** that the renderer will use;
it does not reread chapters or expand them twice. It runs before image loading,
stylesheet loading, rendering, output-directory creation, or staged publication
writes. A broken reference exits 65 with `book_link_findings`; a core-check
failure exits 70 with `book_check_error`. Existing publications remain untouched
on these failures. Run the separate `--check-links --json` command for all
findings; a rejected export does not mix a report into publication stdout.

A successful guarded JSON publication receipt has an additional `link_check`
field containing the complete core report. External URLs and other unverified
local resources remain visible in that report and do not cause a rejection.
The ordinary rendering options, asset checks, and transactional writer still
apply after the guard passes.

The guard is opt-in. Commands without either new flag retain their prior
rendering and receipt behavior. Guarding a clean book does not change its
publication bytes. The existing `unresolved_links` receipt field is a legacy
missing-Markdown-path tally; it is not an anchor-validation verdict. Use the
new report's findings and explicit scope for navigation checking.

## Scope and shared sources

See `BOOK_LINK_VALIDATION.md` for the core check's exact semantics and budgets,
and `NATIVE_BOOK_SOURCES.md` for include-only chapter selection. Known Markdown
chapter links, heading/footnote anchors, forward references, nested chapter
paths, and percent-encoded fragments are checked after expansion. A link to an
include-only file is not a link to a published chapter; link to the appropriate
heading in a chapter that incorporates it.

Zero findings is not universal publication conformance. The checker does not
fetch URLs, validate image availability, interpret arbitrary raw-HTML IDs, or
certify final PDF/EPUB destinations or accessibility. The native include
resolver's existing relative-link semantics are unchanged. A declared resource
role is not a filesystem sandbox or a new include allowlist.

## Verification

`tests/native_book_check_test.rs` contains executable regressions for both
binary names, exact equality with the core JSON, include-only expansion and
forward anchors, source-only phase isolation, unchanged existing files,
argument conflicts, failure codes, whole-report limits, human diagnostics,
strict refusal before assets/stylesheets/writes, strict/default EPUB byte
parity, legacy opt-in behavior, and separate discovery warnings. Focused unit
tests exercise the guard, output escaping, I/O errors and argument parsing.

Run with the configured DSR/Rust environment:

```sh
cargo test --test native_book_check_test --test native_book_sources_test
cargo test --lib book::native
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

The authoring environment has no local Rust toolchain or DSR executable. These
Rust tests were added but could not be executed there. Source review and blob
identity checks do not establish compilation, runtime behavior, generated-WASM
parity, or final PDF/EPUB conformance. No GitHub Actions are used.
