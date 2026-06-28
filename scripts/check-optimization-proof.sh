#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/check-optimization-proof.sh <OPTIMIZATION_PROOF.md>
       scripts/check-optimization-proof.sh --list-requirements

Validates the optimization proof checklist required before closing performance
beads. The proof file should usually live inside the perf artifact directory
that supports the bead closeout.
USAGE
}

list_requirements() {
  cat <<'REQ'
Required proof fields:
- # Optimization Proof
- Bead: <bead id>
- Change: <short description>
- Artifact directory: <path>
- ## Behavior Isomorphism Checklist
- [x] Ordering preserved: <rationale>
- [x] Tie-breaking preserved: <rationale>
- [x] Floating-point decisions preserved or moved to fixed-point: <rationale>
- [x] Scalar fallback preserved: <rationale>
- [x] RNG unchanged or not applicable: <rationale>
- [x] Golden checksums recorded: <rationale>
- [x] Determinism script passed: <rationale>
- [x] WASM/no-default proof recorded: <rationale>
- [x] Before/after p95 recorded: <rationale>
- [x] Rollback plan recorded: <rationale>
- ## Evidence
- Before p95: <number> <ns|us|ms|s|cycles> (source: <artifact>)
- After p95: <number> <ns|us|ms|s|cycles> (source: <artifact>)
- Golden checksum: <checksum or explicit unchanged-output reason>
- Determinism: <command and result>
- WASM/no-default: <command/result or explicit no-core-change reason>
- Rollback plan: <exact revert/disable plan>
REQ
}

fail() {
  printf 'fmd optimization proof: %s\n' "$*" >&2
  exit 1
}

require_pattern() {
  local pattern="$1"
  local label="$2"
  local fix="$3"
  if ! grep -Eq -- "$pattern" "$proof_file"; then
    fail "missing $label (fix: $fix)"
  fi
}

reject_unresolved_placeholders() {
  if grep -Eiq '\b(TODO|TBD)\b' "$proof_file"; then
    fail 'placeholder text remains (fix: replace TODO/TBD with concrete evidence)'
  fi

  # Only reject the exact angle-bracket placeholders emitted by
  # --list-requirements / docs/OPTIMIZATION_PROOF_CHECKLIST.md. Optimization
  # proofs may legitimately discuss HTML, PDF tags, or XML-like snippets such
  # as <span> while explaining behavior preservation.
  local angle_placeholder_re
  angle_placeholder_re='<(bead id|short description|short optimization description|path|run-id|rationale|why output ordering is unchanged|why equal-cost choices are unchanged|proof|path/test|why|artifact path|artifact paths|number|ns\|us\|ms\|s\|cycles|artifact|checksum or explicit unchanged-output reason|command and result|command/result or explicit no-core-change reason|command/result or no-core-change reason|exact revert/disable plan|exact revert/feature-disable plan)>'
  if grep -Eq -- "$angle_placeholder_re" "$proof_file"; then
    fail 'template placeholder remains (fix: replace the angle-bracket checklist placeholder with concrete evidence)'
  fi
}

if [ "$#" -ne 1 ]; then
  usage >&2
  exit 64
fi

case "$1" in
  -h|--help)
    usage
    exit 0
    ;;
  --list-requirements)
    list_requirements
    exit 0
    ;;
esac

proof_file="$1"
if [ ! -f "$proof_file" ]; then
  fail "proof file not found: $proof_file"
fi

require_pattern '^# Optimization Proof$' \
  'top-level "# Optimization Proof" heading' \
  'start the file with "# Optimization Proof"'
require_pattern '^Bead: [^[:space:]].+' \
  'Bead line' \
  'add "Bead: <bead id>"'
require_pattern '^Change: [^[:space:]].+' \
  'Change line' \
  'add "Change: <short optimization description>"'
require_pattern '^Artifact directory: [^[:space:]].+' \
  'Artifact directory line' \
  'add "Artifact directory: tests/artifacts/perf/<run-id>"'
require_pattern '^## Behavior Isomorphism Checklist$' \
  'behavior checklist heading' \
  'add "## Behavior Isomorphism Checklist"'

require_pattern '^- \[[xX]\] Ordering preserved: [^[:space:]].+' \
  'checked ordering-preserved item' \
  'add "- [x] Ordering preserved: <why output ordering is unchanged>"'
require_pattern '^- \[[xX]\] Tie-breaking preserved: [^[:space:]].+' \
  'checked tie-breaking item' \
  'add "- [x] Tie-breaking preserved: <why equal-cost choices are unchanged>"'
require_pattern '^- \[[xX]\] Floating-point decisions preserved or moved to fixed-point: [^[:space:]].+' \
  'checked floating-point item' \
  'add "- [x] Floating-point decisions preserved or moved to fixed-point: <proof>"'
require_pattern '^- \[[xX]\] Scalar fallback preserved: [^[:space:]].+' \
  'checked scalar-fallback item' \
  'add "- [x] Scalar fallback preserved: <path/test>"'
require_pattern '^- \[[xX]\] RNG unchanged or not applicable: [^[:space:]].+' \
  'checked RNG item' \
  'add "- [x] RNG unchanged or not applicable: <why>"'
require_pattern '^- \[[xX]\] Golden checksums recorded: [^[:space:]].+' \
  'checked golden-checksum item' \
  'add "- [x] Golden checksums recorded: <artifact path>"'
require_pattern '^- \[[xX]\] Determinism script passed: [^[:space:]].+' \
  'checked determinism item' \
  'add "- [x] Determinism script passed: scripts/check-determinism.sh passed"'
require_pattern '^- \[[xX]\] WASM/no-default proof recorded: [^[:space:]].+' \
  'checked WASM/no-default item' \
  'add "- [x] WASM/no-default proof recorded: <command/result or no-core-change reason>"'
require_pattern '^- \[[xX]\] Before/after p95 recorded: [^[:space:]].+' \
  'checked before/after p95 item' \
  'add "- [x] Before/after p95 recorded: <artifact paths>"'
require_pattern '^- \[[xX]\] Rollback plan recorded: [^[:space:]].+' \
  'checked rollback item' \
  'add "- [x] Rollback plan recorded: <exact revert/feature-disable plan>"'

require_pattern '^## Evidence$' \
  'evidence heading' \
  'add "## Evidence"'
require_pattern '^Before p95: [0-9]+([.][0-9]+)? (ns|us|ms|s|cycles)( |$).+' \
  'Before p95 evidence' \
  'add "Before p95: <number> <ns|us|ms|s|cycles> (source: <artifact>)"'
require_pattern '^After p95: [0-9]+([.][0-9]+)? (ns|us|ms|s|cycles)( |$).+' \
  'After p95 evidence' \
  'add "After p95: <number> <ns|us|ms|s|cycles> (source: <artifact>)"'
require_pattern '^Golden checksum: [^[:space:]].+' \
  'Golden checksum evidence' \
  'add "Golden checksum: sha256:<digest> ..." or an explicit unchanged-output reason'
require_pattern '^Determinism: [^[:space:]].+' \
  'Determinism evidence' \
  'add "Determinism: scripts/check-determinism.sh passed"'
require_pattern '^WASM/no-default: [^[:space:]].+' \
  'WASM/no-default evidence' \
  'add "WASM/no-default: scripts/check-wasm-core.sh passed" or an explicit no-core-change reason'
require_pattern '^Rollback plan: [^[:space:]].+' \
  'Rollback plan evidence' \
  'add "Rollback plan: revert <commit> ..."'

reject_unresolved_placeholders

printf 'fmd optimization proof: ok (%s)\n' "$proof_file"
