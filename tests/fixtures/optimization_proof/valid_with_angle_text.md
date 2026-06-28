# Optimization Proof
Bead: br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-example
Change: Preserved literal HTML/PDF tag explanations such as <span> in proof text.
Artifact directory: tests/artifacts/perf/example-angle-text

## Behavior Isomorphism Checklist
- [x] Ordering preserved: output byte ordering is compared against the same golden artifact; mentioning <span> in the proof does not affect renderer behavior.
- [x] Tie-breaking preserved: no ranking, line-breaking, glyph-subset, or object-ordering decisions changed.
- [x] Floating-point decisions preserved or moved to fixed-point: no floating-point code changed for this proof-validator fixture.
- [x] Scalar fallback preserved: no accelerated code path changed for this proof-validator fixture.
- [x] RNG unchanged or not applicable: renderer and proof validation use no randomness in this path.
- [x] Golden checksums recorded: tests/artifacts/perf/example-angle-text/golden/pdf-large.sha256 records the byte output.
- [x] Determinism script passed: scripts/check-determinism.sh passed on the representative checkout.
- [x] WASM/no-default proof recorded: scripts/check-wasm-core.sh passed; this fixture intentionally exercises validator text handling only.
- [x] Before/after p95 recorded: tests/artifacts/perf/example-angle-text/inprocess-before.jsonl and inprocess-after.jsonl contain the scenario samples.
- [x] Rollback plan recorded: revert the proof-validator change and rerun this fixture plus the missing-after-p95 negative fixture.

## Evidence
Before p95: 61000000 ns (source: tests/artifacts/perf/example-angle-text/inprocess-before.jsonl)
After p95: 52000000 ns (source: tests/artifacts/perf/example-angle-text/inprocess-after.jsonl)
Golden checksum: sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef from tests/artifacts/perf/example-angle-text/golden/pdf-large.sha256
Determinism: scripts/check-determinism.sh passed.
WASM/no-default: scripts/check-wasm-core.sh passed.
Rollback plan: revert the proof-validator patch and rerun scripts/check-optimization-proof.sh on valid and invalid fixtures.
