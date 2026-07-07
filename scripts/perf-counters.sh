#!/usr/bin/env bash
#
# perf-counters.sh — safe hardware-counter profiling workflow (bead qw1.8.2).
#
# Runs `perf stat` over a target command and captures the output as a perf
# artifact bundle (per docs/PERFORMANCE_ARTIFACT_SCHEMA.md). Hardware counters
# are often blocked by kernel.perf_event_paranoid; with --tune (and sudo) this
# script relaxes the relevant sysctls *only for the run* and restores the
# original values on exit — even on failure — then verifies the restore.
#
# Safety contract:
#   * Default mode performs NO OS tuning (read-only preflight + best-effort
#     `perf stat`); it never needs root and never leaves the host modified.
#   * --tune captures the original sysctls, applies tuned values, and a trap
#     restores them at EXIT (success, error, or Ctrl-C), then re-reads them to
#     prove restoration. The restore status is recorded in the artifacts.
#   * If `perf` is unavailable, the script prints a clear fallback message,
#     records available=false, and exits 0 (it is a profiling aid, not a gate).
#   * --self-test proves the capture -> tune -> restore -> verify cycle against a
#     mock sysctl backend, with NO root and NO real OS changes, so CI can prove
#     the restore logic deterministically.
#
# Usage:
#   scripts/perf-counters.sh [--tune] [--run-id ID] [--counters LIST] [-- CMD ...]
#   scripts/perf-counters.sh --self-test
#
# Examples:
#   scripts/perf-counters.sh                       # preflight + perf stat fmd --version
#   scripts/perf-counters.sh -- target/release/fmd README.md --to pdf --out /tmp/r.pdf
#   sudo -v && scripts/perf-counters.sh --tune     # relax sysctls for the run, then restore
#   scripts/perf-counters.sh --self-test           # prove restore logic (no root)

set -euo pipefail

# Sysctls relaxed for hardware counters, paired with the values --tune applies.
SYSCTL_KEYS=(kernel.perf_event_paranoid kernel.kptr_restrict kernel.nmi_watchdog)
SYSCTL_TUNED=(-1 0 0)

DEFAULT_COUNTERS="cycles,instructions,branches,branch-misses,cache-references,cache-misses"

TUNE=0
SELF_TEST=0
OUT_DIR=""
RUN_ID=""
COUNTERS="$DEFAULT_COUNTERS"
CMD=()

# Backend indirection so the self-test can exercise the exact capture/restore
# logic against temp files instead of /proc/sys (no root, no real OS changes).
BACKEND="real"
MOCK_DIR=""
MOCK_FAIL_KEY=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --tune) TUNE=1; shift ;;
    --self-test) SELF_TEST=1; shift ;;
    --run-id) RUN_ID="${2:?--run-id needs an id}"; shift 2 ;;
    --out) RUN_ID="${2:?--out needs a run id}"; shift 2 ;; # legacy alias; still validated below
    --counters) COUNTERS="${2:?--counters needs a list}"; shift 2 ;;
    --) shift; CMD=("$@"); break ;;
    -h|--help) sed -n '2,40p' "$0"; exit 0 ;;
    *) echo "perf-counters: unknown argument '$1' (try --help)" >&2; exit 64 ;;
  esac
done

backend_read() { # key -> value (or 'unavailable')
  local key="$1"
  if [ "$BACKEND" = "mock" ]; then
    cat "$MOCK_DIR/$key" 2>/dev/null || printf 'unavailable'
  else
    local path="/proc/sys/${key//.//}"
    if [ -r "$path" ]; then cat "$path"; else printf 'unavailable'; fi
  fi
}

backend_write() { # key value -> applies (mock: temp file; real: sudo sysctl)
  local key="$1" val="$2"
  if [ "$BACKEND" = "mock" ]; then
    if [ "${MOCK_FAIL_KEY:-}" = "$key" ]; then
      return 77
    fi
    printf '%s\n' "$val" > "$MOCK_DIR/$key"
  else
    sudo sysctl -q -w "$key=$val"
  fi
}

json_escape() {
  local s="$1"
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  s=${s//$'\r'/\\r}
  s=${s//$'\t'/\\t}
  printf '%s' "$s"
}

json_string_array_from_csv() {
  local csv="$1"
  local old_ifs="$IFS"
  local -a items=()
  local item
  local sep=""
  IFS=',' read -r -a items <<< "$csv"
  IFS="$old_ifs"
  for item in "${items[@]}"; do
    printf '%s"%s"' "${sep:-}" "$(json_escape "$item")"
    sep=","
  done
}

declare -A SYSCTL_OLD=()
TUNING_APPLIED=0

capture_originals() {
  local key
  for key in "${SYSCTL_KEYS[@]}"; do
    SYSCTL_OLD[$key]="$(backend_read "$key")"
  done
}

apply_tuning() {
  local i
  for i in "${!SYSCTL_KEYS[@]}"; do
    backend_write "${SYSCTL_KEYS[$i]}" "${SYSCTL_TUNED[$i]}" || return "$?"
    TUNING_APPLIED=1
  done
}

restore_originals() {
  # Idempotent: safe to call from the EXIT trap and explicitly. Only restores
  # what was actually captured, and never writes an 'unavailable' sentinel.
  [ "$TUNING_APPLIED" -eq 1 ] || return 0
  local key val
  for key in "${SYSCTL_KEYS[@]}"; do
    val="${SYSCTL_OLD[$key]:-}"
    if [ -n "$val" ] && [ "$val" != "unavailable" ]; then
      backend_write "$key" "$val" 2>/dev/null || true
    fi
  done
}

# Re-read every tuned sysctl and compare to the captured original.
# Echoes: restored | unknown | not_tuned
verify_restored() {
  if [ "$TUNING_APPLIED" -ne 1 ]; then
    printf 'not_tuned'
    return 0
  fi
  local key status="restored"
  for key in "${SYSCTL_KEYS[@]}"; do
    if [ "$(backend_read "$key")" != "${SYSCTL_OLD[$key]}" ]; then
      status="unknown"
    fi
  done
  printf '%s' "$status"
}

# ---- self-test: prove the restore cycle without root ------------------------
run_self_test() {
  BACKEND="mock"
  MOCK_DIR="$(mktemp -d)"
  # Seed plausible "secured" originals.
  printf '4\n' > "$MOCK_DIR/kernel.perf_event_paranoid"
  printf '1\n' > "$MOCK_DIR/kernel.kptr_restrict"
  printf '1\n' > "$MOCK_DIR/kernel.nmi_watchdog"

  capture_originals
  apply_tuning

  # The relaxed values must be live mid-run.
  if [ "$(backend_read kernel.perf_event_paranoid)" != "-1" ] \
    || [ "$(backend_read kernel.kptr_restrict)" != "0" ] \
    || [ "$(backend_read kernel.nmi_watchdog)" != "0" ]; then
    echo "perf-counters self-test: FAIL — tuning did not apply" >&2
    trap - EXIT
    rm -rf -- "$MOCK_DIR"; exit 1
  fi

  restore_originals
  local status; status="$(verify_restored)"

  local key bad=0
  declare -A EXPECT=([kernel.perf_event_paranoid]=4 [kernel.kptr_restrict]=1 [kernel.nmi_watchdog]=1)
  for key in "${SYSCTL_KEYS[@]}"; do
    if [ "$(backend_read "$key")" != "${EXPECT[$key]}" ]; then
      echo "perf-counters self-test: FAIL — $key not restored to ${EXPECT[$key]}" >&2
      bad=1
    fi
  done
  if [ "$bad" -ne 0 ] || [ "$status" != "restored" ]; then
    echo "perf-counters self-test: FAIL — restore_status=$status" >&2
    rm -rf -- "$MOCK_DIR"
    trap - EXIT
    exit 1
  fi

  # A failed write after an earlier successful write must still restore the
  # earlier sysctl. This catches the real failure mode that can otherwise happen
  # under `set -e`: partial tuning aborts before normal run cleanup.
  TUNING_APPLIED=0
  printf '4\n' > "$MOCK_DIR/kernel.perf_event_paranoid"
  printf '1\n' > "$MOCK_DIR/kernel.kptr_restrict"
  printf '1\n' > "$MOCK_DIR/kernel.nmi_watchdog"
  capture_originals
  MOCK_FAIL_KEY="kernel.kptr_restrict"
  set +e
  apply_tuning
  local partial_rc=$?
  set -e
  MOCK_FAIL_KEY=""
  if [ "$partial_rc" -eq 0 ]; then
    echo "perf-counters self-test: FAIL — injected partial tuning failure did not fail" >&2
    rm -rf -- "$MOCK_DIR"
    trap - EXIT
    exit 1
  fi
  if [ "$(backend_read kernel.perf_event_paranoid)" != "-1" ]; then
    echo "perf-counters self-test: FAIL — partial tuning did not apply the first sysctl" >&2
    rm -rf -- "$MOCK_DIR"
    trap - EXIT
    exit 1
  fi
  restore_originals
  status="$(verify_restored)"
  bad=0
  for key in "${SYSCTL_KEYS[@]}"; do
    if [ "$(backend_read "$key")" != "${EXPECT[$key]}" ]; then
      echo "perf-counters self-test: FAIL — partial failure left $key at $(backend_read "$key")" >&2
      bad=1
    fi
  done
  if [ "$bad" -ne 0 ] || [ "$status" != "restored" ]; then
    echo "perf-counters self-test: FAIL — partial failure restore_status=$status" >&2
    rm -rf -- "$MOCK_DIR"
    trap - EXIT
    exit 1
  fi

  rm -rf -- "$MOCK_DIR"
  trap - EXIT
  echo "perf-counters self-test: ok — sysctls captured, tuned to (${SYSCTL_TUNED[*]}), restored to originals, and partial failure restored (status=$status)"
  exit 0
}

trap restore_originals EXIT

if [ "$SELF_TEST" -eq 1 ]; then
  run_self_test
  # shellcheck disable=SC2317
  exit $? # defense-in-depth: run_self_test already exits on every path
fi

# ---- real run ---------------------------------------------------------------
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

if [ -z "$RUN_ID" ]; then
  RUN_ID="perfcounters-$(date -u +%Y%m%dT%H%M%SZ)"
fi
fmd_validate_run_id "perf-counters" "$RUN_ID"
OUT_DIR="tests/artifacts/perf/$RUN_ID"
mkdir -p "$OUT_DIR"

# Default target: the fmd binary's --version (cheap and always present once
# built). Pass `-- CMD ...` to profile a real render instead. The `|| true`
# inside each substitution keeps a broken/absent cargo from tripping
# `set -e`/`pipefail` before the build fallback below can run.
target_dir() {
  { cargo metadata --format-version 1 --no-deps 2>/dev/null || true; } \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
}
if [ "${#CMD[@]}" -eq 0 ]; then
  BIN="target/release/fmd"
  [ -x "$BIN" ] || BIN="$(target_dir)/release/fmd"
  if [ ! -x "$BIN" ]; then
    echo "perf-counters: building release fmd for the default target..."
    cargo build --release --bin fmd >/dev/null 2>&1 || true
    BIN="$(target_dir)/release/fmd"
  fi
  CMD=("$BIN" --version)
fi

echo "perf-counters: preflight"
echo "  run id      : $RUN_ID"
echo "  artifact dir: $OUT_DIR"
echo "  counters    : $COUNTERS"
echo "  command     : ${CMD[*]}"
echo "  tune sysctls: $([ "$TUNE" -eq 1 ] && echo yes || echo 'no (read-only, default)')"
for key in "${SYSCTL_KEYS[@]}"; do
  echo "  $key = $(backend_read "$key")"
done

# Whether `perf` exists at all.
PERF_AVAILABLE=0
if command -v perf >/dev/null 2>&1; then
  PERF_AVAILABLE=1
fi

# Optional, opt-in OS tuning for this run only.
RESTORE_STATUS="not_tuned"
if [ "$TUNE" -eq 1 ]; then
  if [ "$PERF_AVAILABLE" -ne 1 ]; then
    echo "perf-counters: --tune requested but perf is unavailable; skipping OS tuning" >&2
  else
    echo "perf-counters: capturing and relaxing sysctls for this run (restored at exit)"
    capture_originals
    apply_tuning
  fi
fi

# Run perf stat (best effort). Counter availability is decided by exit status.
PERF_OK=0
if [ "$PERF_AVAILABLE" -eq 1 ]; then
  if perf stat -e "$COUNTERS" -- "${CMD[@]}" \
      > "$OUT_DIR/perf-stat.stdout" 2> "$OUT_DIR/perf-stat.stderr"; then
    PERF_OK=1
    echo "perf-counters: perf stat captured -> $OUT_DIR/perf-stat.stdout"
  else
    echo "perf-counters: perf stat ran but reported an error (counters likely restricted); see perf-stat.stderr" >&2
    : > "$OUT_DIR/perf-stat.stdout"
  fi
else
  {
    echo "perf is unavailable on this host (command not found)."
    echo "Install linux-tools / perf to capture hardware counters, or run on a"
    echo "host where perf is permitted. This is a profiling aid, not a gate."
  } > "$OUT_DIR/perf-stat.stderr"
  : > "$OUT_DIR/perf-stat.stdout"
fi

# Restore (also runs via the EXIT trap) and verify before writing artifacts.
restore_originals
RESTORE_STATUS="$(verify_restored)"

# tuning.json: what we touched and whether it was put back.
{
  echo "{"
  echo "  \"requested\": $([ "$TUNE" -eq 1 ] && echo true || echo false),"
  echo "  \"applied\": $([ "$TUNING_APPLIED" -eq 1 ] && echo true || echo false),"
  echo "  \"restore_status\": \"$RESTORE_STATUS\","
  echo "  \"sysctls\": {"
  last=$(( ${#SYSCTL_KEYS[@]} - 1 ))
  for i in "${!SYSCTL_KEYS[@]}"; do
    key="${SYSCTL_KEYS[$i]}"
    old="${SYSCTL_OLD[$key]:-$(backend_read "$key")}"
    cur="$(backend_read "$key")"
    comma=","; [ "$i" -eq "$last" ] && comma=""
    echo "    \"$key\": {\"old\": \"$old\", \"current\": \"$cur\"}$comma"
  done
  echo "  }"
  echo "}"
} > "$OUT_DIR/tuning.json"

# hardware_counter_summary JSONL record (schema fmd-perf-artifact-v1).
counter_json="$(json_string_array_from_csv "$COUNTERS")"
available_json=$([ "$PERF_OK" -eq 1 ] && echo true || echo false)
{
  printf '{"type":"hardware_counter_summary","scenario":"perf-counters-smoke",'
  printf '"available":%s,"counter_set":[%s],' "$available_json" "$counter_json"
  printf '"stdout_path":"perf-stat.stdout","stderr_path":"perf-stat.stderr",'
  printf '"restore_status":"%s",' "$RESTORE_STATUS"
  printf '"notes":"perf-counters.sh over: %s"}\n' "$(json_escape "${CMD[*]}")"
} > "$OUT_DIR/hardware_counter_summary.jsonl"

echo "perf-counters: done"
echo "  perf available : $([ "$PERF_AVAILABLE" -eq 1 ] && echo yes || echo no)"
echo "  counters ran   : $([ "$PERF_OK" -eq 1 ] && echo yes || echo no)"
echo "  restore status : $RESTORE_STATUS"
echo "  artifacts      : $OUT_DIR/{perf-stat.stdout,perf-stat.stderr,tuning.json,hardware_counter_summary.jsonl}"
