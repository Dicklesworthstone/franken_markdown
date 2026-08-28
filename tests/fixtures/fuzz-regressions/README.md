# Promoted fuzz regressions (m7fs.2)

`*.bin` files here are minimized crashing (or drill) inputs.

- `drill.bin` is the synthetic minimizer fixture (`!PANIC!`). It is **not**
  fed to the engine; `tests/fuzz_triage.rs` skips that name.
- Every other `*.bin` is loaded by `engine_regression_bins_do_not_panic`
  and must not panic parse/HTML/PDF/zlib/font subset.

Promote with `scripts/fuzz-triage.sh` (see `fuzz/README.md`).
