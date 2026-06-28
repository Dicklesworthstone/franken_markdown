# Optimization Proof Checklist

Every performance bead must leave enough evidence for a future agent to decide
whether the optimization preserved behavior and actually moved the measured hot
path. Use `scripts/check-optimization-proof.sh` on the proof file before closing
the bead.

## Required Command

```bash
scripts/check-optimization-proof.sh tests/artifacts/perf/<run-id>/OPTIMIZATION_PROOF.md
```

For close comments, cite the command and the artifact path:

```text
Optimization proof: scripts/check-optimization-proof.sh tests/artifacts/perf/<run-id>/OPTIMIZATION_PROOF.md passed.
```

## Template

Copy this into the perf artifact directory for the bead and replace every
placeholder with concrete evidence.

```markdown
# Optimization Proof
Bead: <bead id>
Change: <short optimization description>
Artifact directory: tests/artifacts/perf/<run-id>

## Behavior Isomorphism Checklist
- [x] Ordering preserved: <why output ordering is unchanged>
- [x] Tie-breaking preserved: <why equal-cost choices are unchanged>
- [x] Floating-point decisions preserved or moved to fixed-point: <proof>
- [x] Scalar fallback preserved: <path/test>
- [x] RNG unchanged or not applicable: <why>
- [x] Golden checksums recorded: <artifact path>
- [x] Determinism script passed: scripts/check-determinism.sh passed
- [x] WASM/no-default proof recorded: <command/result or no-core-change reason>
- [x] Before/after p95 recorded: <artifact paths>
- [x] Rollback plan recorded: <exact revert/feature-disable plan>

## Evidence
Before p95: <number> <ns|us|ms|s|cycles> (source: <artifact>)
After p95: <number> <ns|us|ms|s|cycles> (source: <artifact>)
Golden checksum: <checksum or explicit unchanged-output reason>
Determinism: <command and result>
WASM/no-default: <command/result or explicit no-core-change reason>
Rollback plan: <exact revert/disable plan>
```

## Rationale

The checklist is intentionally stricter than a narrative closeout. It catches
the common optimization failure modes before they become permanent:

- order changes that alter deterministic output bytes,
- different tie-breaking in paragraph, table, glyph, or object ordering,
- accidental floating-point drift where fixed-point was required,
- accelerated paths without scalar fallback,
- untracked randomness,
- speed claims without preserved golden output,
- core changes without WASM/no-default proof,
- benchmark claims without before/after p95 from the same scenario,
- no quick rollback plan when a later visual or determinism regression appears.

If an item is genuinely not applicable, mark it checked and write the concrete
reason. Do not leave `TODO`, `TBD`, or angle-bracket placeholders in a proof
file that will be used to close a bead.
