# fmd perf run optloop-after10-20260705T171210Z

Artifacts:

- `SCHEMA.md` - schema stamp for this run.
- `schema_manifest.json` - machine-readable schema/version/file mapping.
- `DEFINE.md` - scenario and budget definition.
- `fingerprint.json` - host, build, git, and toolchain fingerprint.
- `BASELINE.md` - p50/p95/p99 baseline table.
- `hotspot_table.md` - ranked in-process scenario costs.
- `hypothesis.md` - interpreted bottleneck hypotheses.
- `scaling_law.md` - multicore/batch scaling guidance.
- `ALIEN_ARTIFACT.md` - advanced-math artifact/proof plan.
- `golden/pdf-large-stages.jsonl` - PDF stage attribution records.
- `golden/pdf-large-recommendation.jsonl` - next PDF optimization target recommendation.
- `golden/parser-large-stages.jsonl` - parser stage/allocation attribution records.
- `golden/parser-large-spanned-stages.jsonl` - source-span/diagnostic parser attribution records.
- `golden/parser-large-recommendation.jsonl` - next parser optimization target recommendation.
- `golden_checksums.txt` - behavior-preservation checksums.
- `hyperfine.*` - CLI wall-clock baselines.
- `perf-stat.*` - hardware counter output when permitted.
- `time.stderr` - peak RSS probe.
