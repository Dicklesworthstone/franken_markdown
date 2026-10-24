#!/usr/bin/env bash
# scripts/e2e/wasm-browser.sh — e2e: headless WASM browser parity suite (s115.7).
#
# Exercises the pure rendering core under wasm32:
# - Verifies std-only no-default-features compilation for core and fmd-math.
# - Runs check-wasm-core.sh.
# - Requires wasm-bindgen/node and validates the actual package build and parity.
#
# Usage: scripts/e2e/wasm-browser.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

fmd_validate_run_id "wasm-browser" "${1:-wasm-browser}"

# Check before initializing artifacts or building anything. This suite promises
# package parity: a missing prerequisite is unrun work, never a passing skip.
missing=()
for tool in wasm-bindgen node; do
  command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if [ "${#missing[@]}" -ne 0 ]; then
  printf 'wasm-browser: missing required tools: %s. Install them and rerun this suite; WASM package parity was not executed.\n' "${missing[*]}" >&2
  exit 66
fi

e2e_init "${1:-wasm-browser}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"

# --- WASM core std-only checks ---
e2e_run "wasm: check-wasm-core execution" -- \
  "${E2E_REPO_ROOT}/scripts/check-wasm-core.sh"
e2e_expect_exit 0
e2e_expect_stdout_contains "fmd wasm-core check: ok"

# --- Required WASM package verification ---
e2e_run "wasm: check-wasm-package headless parity run" -- \
  "${E2E_REPO_ROOT}/scripts/check-wasm-package.sh" "e2e-wasm"
e2e_expect_exit 0
e2e_expect_stdout_contains "wasm-package gate: ok"

e2e_finish
exit $?
