#!/usr/bin/env bash
# Run all checked-in JavaScript runtime and helper tests, without a Rust build.
# Usage: scripts/check-javascript.sh [--help]
# Exit: 0 tests passed; 64 usage; 66 missing runtime/test family; otherwise Node's exit.
set -uo pipefail

if [ "$#" -gt 1 ]; then
  echo "javascript-tests: expected no arguments (try --help)" >&2
  exit 64
fi
case "${1:-}" in
  --help|-h) sed -n '2,4p' "$0"; exit 0 ;;
  "") ;;
  *) echo "javascript-tests: unknown argument '$1' (try --help)" >&2; exit 64 ;;
esac

cd "$(dirname "$0")/.." || exit 66
if ! command -v node >/dev/null 2>&1; then
  echo "javascript-tests: required node executable not found. Install Node.js and rerun scripts/check-javascript.sh." >&2
  exit 66
fi

# Node deliberately skips nested CLI test runs while returning success. Such
# a run cannot qualify this gate, even when all files and tools are present.
if [ "${NODE_TEST_CONTEXT+x}" = x ]; then
  echo "javascript-tests: inherited NODE_TEST_CONTEXT would skip the independent test run. Run scripts/check-javascript.sh outside an active Node test runner." >&2
  exit 66
fi

tests=()
for directory in wasm wasm/tests scripts scripts/font-oracles; do
  family=("$directory"/*.test.mjs)
  # Check each family before Node can fall back to unrelated default discovery.
  # This also fails closed when the calling environment enables nullglob.
  if [ "${#family[@]}" -eq 0 ] || [ ! -f "${family[0]}" ]; then
    printf 'javascript-tests: required test family %s/*.test.mjs is missing or empty; tests were not executed.\n' "$directory" >&2
    exit 66
  fi
  tests+=("${family[@]}")
done
printf 'javascript-tests: executing %s test files across four required families\n' "${#tests[@]}"
exec node --experimental-vm-modules --test --test-concurrency=4 "${tests[@]}"
