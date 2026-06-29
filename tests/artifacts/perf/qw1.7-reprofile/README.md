# qw1.7 — layout/hyphenation re-profile gate

Evidence for the qw1.7 re-profiling decision (after the PDF/parser waves).

- `inprocess.jsonl` — perf_sample records for pdf-large, parser-large,
  hyphen-corpus, and paragraph-1k (release profile; hyphen-corpus iters bounded
  because the scenario runs ~5.5s/iter on a 50k-word corpus).
- `fingerprint.json` — git SHA, build profile, rustc, CPU.
- `DECISION.md` — the proceed/defer/reject decision matrix for the qw1.7 child
  beads, with the current p95 ranking and rationale.

Reproduce:

    cargo build --release --example fmd_perf_harness
    for s in pdf-large parser-large hyphen-corpus paragraph-1k; do
      target/release/examples/fmd_perf_harness --scenario "$s" --iterations 10
    done
