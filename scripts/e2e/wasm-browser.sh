#!/usr/bin/env bash
# scripts/e2e/wasm-browser.sh — e2e: headless WASM browser parity suite (s115.7).
#
# Exercises the pure rendering core under wasm32:
# - Verifies std-only no-default-features compilation for core and fmd-math.
# - Runs check-wasm-core.sh.
# - Validates wasm package build and parity when wasm-bindgen/node are present.
#
# Usage: scripts/e2e/wasm-browser.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-wasm-browser}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"

# --- WASM core std-only checks ---
e2e_run "wasm: check-wasm-core execution" -- \
  "${E2E_REPO_ROOT}/scripts/check-wasm-core.sh"
e2e_expect_exit 0
e2e_expect_stdout_contains "fmd wasm-core check: ok"

# --- WASM package verification (if tools are available) ---
if command -v wasm-bindgen >/dev/null 2>&1 && command -v node >/dev/null 2>&1; then
  e2e_run "wasm: check-wasm-package headless parity run" -- \
    "${E2E_REPO_ROOT}/scripts/check-wasm-package.sh" "e2e-wasm"
  e2e_expect_exit 0
  e2e_expect_stdout_contains "wasm-package: ok"
else
  e2e_run "wasm: skip package gate (wasm-bindgen or node missing)" -- \
    echo "wasm-package gate skipped on current environment (missing tools)"
  e2e_expect_exit 0
fi

e2e_finish
exit $?
