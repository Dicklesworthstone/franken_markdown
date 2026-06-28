# Optimization Proof
Bead: br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-example
Change: Replaced repeated PDF decimal formatting with a deterministic append-only writer.
Artifact directory: tests/artifacts/perf/example-run

## Behavior Isomorphism Checklist
- [x] Ordering preserved: PDF object order, xref order, page order, and glyph order are byte-compared against the golden output.
- [x] Tie-breaking preserved: no ranking or line-breaking decisions changed; writer only serializes already-decided values.
- [x] Floating-point decisions preserved or moved to fixed-point: point values are emitted from the same fixed precision as before.
- [x] Scalar fallback preserved: this is scalar-only code and the default path remains the fallback.
- [x] RNG unchanged or not applicable: renderer has no RNG in this path.
- [x] Golden checksums recorded: tests/artifacts/perf/example-run/golden/pdf-large.sha256 records the byte output.
- [x] Determinism script passed: scripts/check-determinism.sh passed on the same checkout.
- [x] WASM/no-default proof recorded: scripts/check-wasm-core.sh passed because core serialization changed.
- [x] Before/after p95 recorded: tests/artifacts/perf/example-run/inprocess-before.jsonl and inprocess-after.jsonl contain the scenario samples.
- [x] Rollback plan recorded: revert the writer commit or disable the optimized writer behind the bead-local feature flag.

## Evidence
Before p95: 61000000 ns (source: tests/artifacts/perf/example-run/inprocess-before.jsonl)
After p95: 52000000 ns (source: tests/artifacts/perf/example-run/inprocess-after.jsonl)
Golden checksum: sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef from tests/artifacts/perf/example-run/golden/pdf-large.sha256
Determinism: scripts/check-determinism.sh passed.
WASM/no-default: scripts/check-wasm-core.sh passed.
Rollback plan: revert the writer commit and rerun scripts/check-determinism.sh plus the PDF tests.
