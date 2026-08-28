#!/usr/bin/env bash
# m7fs.1: build the isolated cargo-fuzz crate and optionally run a short
# libFuzzer canary. The engine workspace is never a fuzz member — this
# script always uses --manifest-path fuzz/Cargo.toml.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

log_check() {
  local id="$1" subject="$2" outcome="$3"
  printf 'fuzz-smoke: check id=%s subject=%s outcome=%s\n' "$id" "$subject" "$outcome" >&2
}

fail_check() {
  log_check "$1" "$2" "FAIL"
  printf 'fuzz-smoke: %s\n' "$3" >&2
  exit "${4:-1}"
}

self_test() {
  local toml="fuzz/Cargo.toml"
  [[ -f "$toml" ]] || fail_check "m7fs.1.manifest" "$toml" "missing fuzz/Cargo.toml" 66
  log_check "m7fs.1.manifest" "$toml" "PASS"

  grep -q '^\[workspace\]$' "$toml" \
    || fail_check "m7fs.1.isolated" "empty [workspace]" "fuzz crate must be its own workspace root" 66
  log_check "m7fs.1.isolated" "empty [workspace]" "PASS"

  if grep -E 'members[[:space:]]*=' Cargo.toml | grep -q fuzz; then
    fail_check "m7fs.1.not-member" "Cargo.toml members" "fuzz/ must not join the engine workspace" 66
  fi
  log_check "m7fs.1.not-member" "Cargo.toml members" "PASS"

  for bin in fuzz_markdown_render fuzz_font_subset fuzz_zlib; do
    grep -q "name = \"$bin\"" "$toml" \
      || fail_check "m7fs.1.bin" "$bin" "missing [[bin]] $bin" 66
    [[ -f "fuzz/fuzz_targets/${bin}.rs" ]] \
      || fail_check "m7fs.1.target" "$bin" "missing fuzz target source" 66
    local corpus="fuzz/corpus/$bin"
    [[ -d "$corpus" ]] || fail_check "m7fs.1.corpus" "$corpus" "missing seed corpus dir" 66
    local count
    count="$(find "$corpus" -type f | wc -l | tr -d ' ')"
    [[ "$count" -ge 1 ]] || fail_check "m7fs.1.corpus" "$corpus" "empty seed corpus" 66
    log_check "m7fs.1.bin" "$bin seeds=$count" "PASS"
  done

  grep -q '^pub fn zlib_decompress' src/compress.rs \
    || fail_check "m7fs.1.zlib-pub" "zlib_decompress" "must be pub for the fuzz crate" 66
  log_check "m7fs.1.zlib-pub" "zlib_decompress" "PASS"
  log_check "m7fs.1.self-test" "source shape" "PASS"
}

usage() {
  cat <<'EOF'
Usage: scripts/fuzz-smoke.sh [--self-test] [--build-only] [--seconds N] [RUN_ID]

  --self-test   source-shape checks only (no compile, no libFuzzer)
  --build-only  compile the three fuzz binaries, do not run them
  --seconds N   libFuzzer -max_total_time per target (default 10)
  RUN_ID        artifact directory name under tests/artifacts/fuzz/
EOF
}

SECONDS_PER=10
BUILD_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --self-test) self_test; exit 0 ;;
    --build-only) BUILD_ONLY=1; shift ;;
    --seconds)
      SECONDS_PER="${2:?}"
      shift 2
      ;;
    -h|--help) usage; exit 0 ;;
    *)
      RUN_ID="$1"
      shift
      ;;
  esac
done

if [[ -z "${RUN_ID:-}" ]]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-fuzz-$(git rev-parse --short HEAD 2>/dev/null || echo head)"
fi
fmd_validate_run_id "fuzz-smoke" "$RUN_ID"

self_test

ARTIFACT_DIR="tests/artifacts/fuzz/$RUN_ID"
mkdir -p "$ARTIFACT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
  fail_check "m7fs.1.cargo" "cargo" "cargo not on PATH" 127
fi

log_check "m7fs.1.build" "cargo build --manifest-path fuzz/Cargo.toml --bins" "…"
if ! cargo build --manifest-path fuzz/Cargo.toml --bins --offline 2>"$ARTIFACT_DIR/build.stderr"; then
  # First run needs crates.io; retry online.
  if ! cargo build --manifest-path fuzz/Cargo.toml --bins >"$ARTIFACT_DIR/build.stdout" 2>"$ARTIFACT_DIR/build.stderr"; then
    log_check "m7fs.1.build" "compile" "FAIL"
    tail -n 40 "$ARTIFACT_DIR/build.stderr" >&2
    exit 1
  fi
fi
log_check "m7fs.1.build" "compile" "PASS"

if [[ "$BUILD_ONLY" -eq 1 ]]; then
  log_check "m7fs.1.run" "skipped --build-only" "SKIP"
  exit 0
fi

if ! command -v clang >/dev/null 2>&1 && [[ "$(uname -s)" == Linux ]]; then
  log_check "m7fs.1.clang" "clang missing; cannot link libFuzzer" "SKIP"
  exit 0
fi

run_one() {
  local bin="$1"
  local out="$ARTIFACT_DIR/$bin"
  mkdir -p "$out"
  local cmd=(cargo run --manifest-path fuzz/Cargo.toml --bin "$bin" --
    -max_total_time="$SECONDS_PER"
    -timeout=5
    "fuzz/corpus/$bin")
  log_check "m7fs.1.run" "$bin ${SECONDS_PER}s" "…"
  if "${cmd[@]}" >"$out/stdout.txt" 2>"$out/stderr.txt"; then
    log_check "m7fs.1.run" "$bin" "PASS"
  else
    log_check "m7fs.1.run" "$bin" "FAIL"
    tail -n 50 "$out/stderr.txt" >&2
    return 1
  fi
}

failed=0
for bin in fuzz_markdown_render fuzz_font_subset fuzz_zlib; do
  run_one "$bin" || failed=1
done
if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
log_check "m7fs.1.smoke" "three targets" "PASS"
