# qw1.8.2 — safe hardware-counter profiling workflow

Evidence for `scripts/perf-counters.sh` (bead qw1.8.2).

- `self-test.txt` — the controlled restore-cycle proof (`--self-test`): captures
  mock sysctls, applies tuned values, restores, and verifies they equal the
  originals. No root, no real OS change. This is the acceptance test that
  "sysctls return to original values".
- `tuning.json` — old/current value of every touched sysctl plus `restore_status`.
  In this default read-only run `requested=false`, `applied=false`,
  `restore_status=not_tuned`, and every `old == current` (the host is untouched).
- `hardware_counter_summary.jsonl` — schema record (`fmd-perf-artifact-v1`):
  counter set, availability, stdout/stderr paths, and restore status.
- `perf-stat.stdout` / `perf-stat.stderr` — `perf stat` output. On a host with
  `kernel.perf_event_paranoid=4` and no `--tune`, counters are restricted, so
  `available=false` and the stderr explains the fallback (the script still
  exits 0 — it is a profiling aid, not a gate).

Run it yourself:

    scripts/perf-counters.sh                 # read-only preflight + perf stat
    scripts/perf-counters.sh --self-test     # prove the restore cycle (no root)
    sudo -v && scripts/perf-counters.sh --tune -- target/release/fmd README.md --to pdf --out /tmp/r.pdf
