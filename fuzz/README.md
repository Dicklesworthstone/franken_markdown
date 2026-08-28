# Fuzz crate (m7fs)

Isolated from the engine workspace. See `docs/FUZZING.md` for how to *run*
the three libFuzzer targets. This file is the **crash triage runbook**
(m7fs.2).

## When nightly libFuzzer exits non-zero

1. **Grab the artifact.** CI uploads `fuzz/artifacts/` and
   `tests/artifacts/fuzz/`. A crashing input is usually
   `fuzz/artifacts/<target>/crash-<hash>` or a `crash-*` file next to the
   corpus.
2. **Reproduce.** Confirm the bytes still panic the engine:
   `cargo test --test fuzz_triage -- --nocapture` after dropping the file
   (not named `drill.bin`) into `tests/fixtures/fuzz-regressions/`. That
   test `catch_unwind`s parse + HTML + PDF + zlib + font subset.
3. **Minimize.** `scripts/fuzz-triage.sh --minimize CRASH.bin --out MIN.bin`
   delta-debuges with the injected `!PANIC!` *drill* oracle. For a real
   engine panic, copy `ddmin` from `tests/fuzz_triage.rs` around a
   `catch_unwind` of the same APIs and shrink until the panic remains.
4. **Fix the root cause** in the engine. Do not silence the panic.
5. **Promote.** Copy `MIN.bin` into `tests/fixtures/fuzz-regressions/`
   under a descriptive name (`zlib-stored-overflow.bin`, …). The next
   `cargo test --test fuzz_triage` run treats every `*.bin` other than
   `drill.bin` as a permanent no-panic regression.
6. **Close the loop.** Re-run the fuzz target (`scripts/fuzz-smoke.sh
   --seconds 30`) and keep the minimized seed in `fuzz/corpus/<target>/`
   so libFuzzer starts from the interesting region.

## Drill (proves the pipeline without a live engine crash)

```bash
scripts/fuzz-triage.sh --self-test
scripts/fuzz-triage.sh --drill local
cargo test --test fuzz_triage -- --nocapture
```

The drill oracle panics iff the buffer contains `!PANIC!`. A padded
buffer is shrunk to exactly those seven bytes, which must match
`tests/fixtures/fuzz-regressions/drill.bin`. Artifacts land under
`tests/artifacts/fuzz/<run-id>/` (gitignored) after `fmd_validate_run_id`.
