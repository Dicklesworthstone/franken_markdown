# Coverage-guided fuzzing (m7fs.1)

The engine crate stays zero-dep. Coverage-guided fuzzing lives in a **separate
package** at `fuzz/`, which is *not* a workspace member and is never built by
`cargo test` / `cargo clippy --all-targets`.

Three libFuzzer targets cover the trust boundaries:

| Target | API | Input cap |
|---|---|---|
| `fuzz_markdown_render` | `parse_markdown` + `render_html_document` + `render_pdf_document` | 4 KiB |
| `fuzz_font_subset` | `Font::parse` + `subset` + outline lookups | 64 KiB |
| `fuzz_zlib` | `zlib_decompress(data, 64 KiB)` | inflater `max_out` |

Seed corpora live under `fuzz/corpus/<target>/` and are tracked. Crashes and
coverage dumps go to `fuzz/artifacts/` (gitignored).

## Local run

```bash
# Source-shape only (no compiler, no clang):
scripts/fuzz-smoke.sh --self-test

# Compile the three binaries (needs a rustc that can link libFuzzer; Linux CI
# uses clang):
scripts/fuzz-smoke.sh --build-only

# 10 s canary per target, artifacts under tests/artifacts/fuzz/<run-id>/:
scripts/fuzz-smoke.sh --seconds 10

# Or cargo-fuzz, if installed:
cargo install cargo-fuzz
cd fuzz
cargo fuzz run fuzz_zlib -- -max_total_time=30
```

`scripts/fuzz-smoke.sh` validates the run id through
`scripts/validate-run-id.sh` before creating any artifact directory.

## CI

`.github/workflows/fuzz.yml` builds the fuzz crate on a nightly schedule and
on `workflow_dispatch`. A PR canary compiles the bins (`--build-only`); a
scheduled job runs 30 s per target. A crash fails the job and uploads
`fuzz/artifacts/`.

This is the README-dev surface for m7fs.1. The engine's existing deterministic
LCG harness (`tests/parser_fuzz.rs`) stays; it is not a substitute for
coverage-guided search.

## Crash triage (m7fs.2)

Runbook: `fuzz/README.md`. Pipeline drill (injected `!PANIC!` marker, no
engine panic required):

```bash
scripts/fuzz-triage.sh --self-test
scripts/fuzz-triage.sh --drill
cargo test --test fuzz_triage -- --nocapture
```

Minimized nightly crashes other than `drill.bin` live in
`tests/fixtures/fuzz-regressions/` and are replayed under `catch_unwind`.
