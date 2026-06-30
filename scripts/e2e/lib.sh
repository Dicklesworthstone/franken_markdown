#!/usr/bin/env bash
# scripts/e2e/lib.sh — structured-logging e2e harness for fmd (bead grn.4.1).
#
# A small, dependency-light Bash library that the e2e suites (grn.4.2–4.6) source
# to drive the REAL fmd binary and record every step in a uniform, machine-readable
# shape. It generalizes the ad-hoc logging already in scripts/cli-output-contract.sh
# and scripts/batch-throughput.sh.
#
# Each step is recorded as:
#     #  | label | argv | cwd | exit | stdout-digest | stderr-digest | verdict
# with the full per-step transcript (argv, stdout, stderr, assertion list) saved
# under tests/artifacts/e2e/<run-id>/steps/, plus a JSON summary
# (tests/artifacts/e2e/<run-id>/summary.json, schema fmd-e2e-v1).
#
# Design choices that matter:
#   * Assertions are NON-FATAL: they record a pass/fail verdict and bump counters
#     but never abort, so a suite runs to completion and reports EVERY failure.
#     The runner decides the process exit code at e2e_finish.
#   * Digests make output regressions visible without dumping bytes into the
#     ledger: stdout/stderr are summarized as sha256:<12hex> <bytes>B <lines>L.
#   * Plain text only; honors NO_COLOR / CI / TERM=dumb / non-tty (color stays off,
#     matching the rest of the repo's scripts) — gating is implemented + tested.
#
# Usage (as a library):
#     source scripts/e2e/lib.sh
#     e2e_init my-run-id
#     e2e_build_bin                       # builds release fmd, sets E2E_BIN (or set FMD_BIN)
#     e2e_run "render html" -- "$E2E_BIN" README.md --to html
#     e2e_expect_exit 0
#     e2e_expect_stdout_contains "<main"
#     e2e_finish                          # writes summary.json, exits 0/70
#
# Usage (self-test, CI): scripts/e2e/lib.sh --self-test
#
# Stable exit codes: 0 ok · 64 usage · 66 environment/build · 70 an assertion failed.

# ---------------------------------------------------------------------------
# When SOURCED, only define functions. When EXECUTED, run the CLI entrypoint.
# ---------------------------------------------------------------------------

# Resolve repo root relative to this file (scripts/e2e/lib.sh -> two levels up) so
# suites can source it from anywhere.
_E2E_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_REPO_ROOT="$(cd "${_E2E_LIB_DIR}/../.." && pwd)"

# --- color / tty gating (kept off to match repo convention, but correct) ----
_e2e_color_enabled() {
  [ -n "${NO_COLOR:-}" ] && return 1
  [ -n "${CI:-}" ] && return 1
  [ "${TERM:-}" = "dumb" ] && return 1
  [ "${E2E_NO_COLOR:-0}" = "1" ] && return 1
  [ -t 1 ] || return 1
  return 0
}

# --- small helpers ----------------------------------------------------------
_e2e_log() { printf '%s\n' "$*"; [ -n "${E2E_LEDGER:-}" ] && printf '%s\n' "$*" >>"$E2E_LEDGER"; }
_e2e_warn() { printf 'e2e: %s\n' "$*" >&2; }

# Content digest of a file: sha256:<12hex> <bytes>B <lines>L (deterministic).
_e2e_digest() {
  local f="$1" hash bytes lines
  if [ ! -f "$f" ]; then printf 'absent'; return 0; fi
  if command -v sha256sum >/dev/null 2>&1; then
    hash="$(sha256sum "$f" | cut -c1-12)"
  elif command -v shasum >/dev/null 2>&1; then
    hash="$(shasum -a 256 "$f" | cut -c1-12)"
  else
    hash="$(cksum "$f" | awk '{print $1}')"
  fi
  bytes="$(wc -c <"$f" | tr -d ' ')"
  lines="$(wc -l <"$f" | tr -d ' ')"
  printf 'sha256:%s %sB %sL' "$hash" "$bytes" "$lines"
}

# ---------------------------------------------------------------------------
# e2e_init <run-id> — set up the artifact tree and counters.
# ---------------------------------------------------------------------------
e2e_init() {
  E2E_RUN_ID="${1:-local}"
  E2E_ART="${E2E_REPO_ROOT}/tests/artifacts/e2e/${E2E_RUN_ID}"
  E2E_STEPS_DIR="${E2E_ART}/steps"
  rm -rf "$E2E_ART"
  mkdir -p "$E2E_STEPS_DIR"
  E2E_LEDGER="${E2E_ART}/ledger.txt"; : >"$E2E_LEDGER"
  E2E_MANIFEST="${E2E_ART}/manifest.tsv"; : >"$E2E_MANIFEST"
  E2E_SUMMARY_JSON="${E2E_ART}/summary.json"
  E2E_STEP_NO=0
  E2E_PASS=0; E2E_FAIL=0
  E2E_ASSERT_PASS=0; E2E_ASSERT_FAIL=0
  E2E_OPEN=0
  E2E_RUN_CWD="${E2E_REPO_ROOT}"
  _e2e_log "=== fmd e2e run=${E2E_RUN_ID} root=${E2E_REPO_ROOT} ==="
  _e2e_log "$(printf '%-3s | %-30s | %-4s | %-26s | %-26s | %s' '#' label exit stdout stderr verdict)"
}

# ---------------------------------------------------------------------------
# e2e_build_bin [feature...] — build the release fmd binary and set E2E_BIN.
# Honors FMD_BIN (skip the build) like the other check scripts.
# ---------------------------------------------------------------------------
e2e_build_bin() {
  if [ -n "${FMD_BIN:-}" ]; then
    E2E_BIN="$FMD_BIN"
  else
    local feats=()
    [ "$#" -gt 0 ] && feats=(--features "$(IFS=,; echo "$*")")
    local tdir="${FMD_TARGET_DIR:-${E2E_REPO_ROOT}/target/fmd-checks}"
    ( cd "$E2E_REPO_ROOT" && CARGO_TARGET_DIR="$tdir" cargo build --release --quiet --bin fmd "${feats[@]}" ) \
      || { _e2e_warn "failed to build fmd"; return 66; }
    E2E_BIN="${tdir}/release/fmd"
  fi
  [ -x "$E2E_BIN" ] || { _e2e_warn "fmd binary not executable at $E2E_BIN"; return 66; }
  # Absolutize so steps that run in a different cwd still find it.
  E2E_BIN="$(cd "$(dirname "$E2E_BIN")" && pwd)/$(basename "$E2E_BIN")"
  export E2E_BIN
  return 0
}

# Close the currently-open step: compute its verdict from its assertions, emit the
# ledger row, append its manifest record. Called at the start of the next step and
# at e2e_finish. Uses the still-current E2E_LAST_* state.
_e2e_close_step() {
  [ "${E2E_OPEN:-0}" -eq 1 ] || return 0
  local base="$E2E_LAST_BASE"
  local apass=0 afail=0
  if [ -f "${base}.asserts" ]; then
    apass="$(grep -c '^pass' "${base}.asserts" 2>/dev/null || true)"; apass="${apass:-0}"
    afail="$(grep -c '^fail' "${base}.asserts" 2>/dev/null || true)"; afail="${afail:-0}"
  fi
  local verdict=pass
  if [ "$afail" -ne 0 ]; then verdict=fail; E2E_FAIL=$((E2E_FAIL+1)); else E2E_PASS=$((E2E_PASS+1)); fi
  local sd ed sbytes ebytes
  sd="$(_e2e_digest "${base}.stdout")"
  ed="$(_e2e_digest "${base}.stderr")"
  sbytes="$(wc -c <"${base}.stdout" 2>/dev/null | tr -d ' ')"; sbytes="${sbytes:-0}"
  ebytes="$(wc -c <"${base}.stderr" 2>/dev/null | tr -d ' ')"; ebytes="${ebytes:-0}"
  _e2e_log "$(printf '%-3s | %-30.30s | %-4s | %-26.26s | %-26.26s | %s' \
    "$E2E_LAST_NO" "$E2E_LAST_LABEL" "$E2E_LAST_EXIT" "$sd" "$ed" "${verdict}(${apass}/$((apass+afail)))")"
  # Manifest: tab-separated; transcript file paths are repo-relative for portability.
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$E2E_LAST_NO" "$E2E_LAST_LABEL" "$E2E_LAST_CWD" "$E2E_LAST_EXIT" \
    "$sd" "$sbytes" "$ed" "$ebytes" "${base}.asserts" "$verdict" >>"$E2E_MANIFEST"
  E2E_OPEN=0
}

# ---------------------------------------------------------------------------
# e2e_run <label> -- <argv...> — run a command, capture stdout/stderr/exit.
# Per-step cwd defaults to repo root; override by setting E2E_RUN_CWD beforehand.
# ---------------------------------------------------------------------------
e2e_run() {
  _e2e_close_step
  local label="$1"; shift
  if [ "${1:-}" = "--" ]; then shift; fi
  E2E_STEP_NO=$((E2E_STEP_NO+1))
  E2E_LAST_NO="$(printf '%03d' "$E2E_STEP_NO")"
  local slug; slug="$(printf '%s' "$label" | tr -c 'A-Za-z0-9._-' '_' | cut -c1-44)"
  E2E_LAST_BASE="${E2E_STEPS_DIR}/${E2E_LAST_NO}-${slug}"
  E2E_LAST_LABEL="$label"
  E2E_LAST_CWD="${E2E_RUN_CWD:-$E2E_REPO_ROOT}"
  E2E_LAST_STDOUT="${E2E_LAST_BASE}.stdout"
  E2E_LAST_STDERR="${E2E_LAST_BASE}.stderr"
  printf '%s\n' "$*" >"${E2E_LAST_BASE}.argv"
  : >"${E2E_LAST_BASE}.asserts"
  ( cd "$E2E_LAST_CWD" && "$@" ) >"$E2E_LAST_STDOUT" 2>"$E2E_LAST_STDERR"
  E2E_LAST_EXIT=$?
  E2E_OPEN=1
}

# Record one assertion verdict against the current step.
# _e2e_assert <ok-rc> <description>
_e2e_assert() {
  local rc="$1"; shift
  local desc="$*"
  if [ "$rc" -eq 0 ]; then
    E2E_ASSERT_PASS=$((E2E_ASSERT_PASS+1))
    printf 'pass\t%s\n' "$desc" >>"${E2E_LAST_BASE}.asserts"
  else
    E2E_ASSERT_FAIL=$((E2E_ASSERT_FAIL+1))
    printf 'fail\t%s\n' "$desc" >>"${E2E_LAST_BASE}.asserts"
    _e2e_warn "ASSERT FAIL [${E2E_LAST_LABEL}]: ${desc}"
  fi
}

# --- assertion vocabulary (all non-fatal) -----------------------------------
e2e_expect_exit() {
  [ "${E2E_LAST_EXIT}" -eq "$1" ]
  _e2e_assert $? "exit == $1 (got ${E2E_LAST_EXIT})"
}
e2e_expect_exit_nonzero() {
  [ "${E2E_LAST_EXIT}" -ne 0 ]
  _e2e_assert $? "exit != 0 (got ${E2E_LAST_EXIT})"
}
e2e_expect_stdout_contains() {
  grep -qF -- "$1" "$E2E_LAST_STDOUT"
  _e2e_assert $? "stdout contains '$1'"
}
e2e_expect_stdout_matches() {
  grep -qE -- "$1" "$E2E_LAST_STDOUT"
  _e2e_assert $? "stdout matches /$1/"
}
e2e_expect_stdout_empty() {
  [ ! -s "$E2E_LAST_STDOUT" ]
  _e2e_assert $? "stdout empty"
}
e2e_expect_stdout_nonempty() {
  [ -s "$E2E_LAST_STDOUT" ]
  _e2e_assert $? "stdout non-empty"
}
e2e_expect_stderr_contains() {
  grep -qF -- "$1" "$E2E_LAST_STDERR"
  _e2e_assert $? "stderr contains '$1'"
}
e2e_expect_file() {
  [ -f "$1" ]
  _e2e_assert $? "file exists: $1"
}
e2e_expect_no_file() {
  [ ! -e "$1" ]
  _e2e_assert $? "no file: $1"
}
e2e_expect_file_contains() {
  # e2e_expect_file_contains <path> <substr>
  { [ -f "$1" ] && grep -qF -- "$2" "$1"; }
  _e2e_assert $? "file $1 contains '$2'"
}
e2e_expect_file_bytes_ge() {
  # e2e_expect_file_bytes_ge <path> <n>
  local n; n="$( [ -f "$1" ] && wc -c <"$1" | tr -d ' ' || echo 0 )"
  [ "${n:-0}" -ge "$2" ]
  _e2e_assert $? "file $1 >= $2 bytes (got ${n:-0})"
}
# Raw boolean assertion for suites that need a custom predicate.
# e2e_assert <description> -- <command...>   (command's exit is the verdict)
e2e_assert() {
  local desc="$1"; shift
  [ "${1:-}" = "--" ] && shift
  "$@"
  _e2e_assert $? "$desc"
}

# ---------------------------------------------------------------------------
# e2e_finish — close the last step, assemble summary.json, print totals, exit.
# ---------------------------------------------------------------------------
e2e_finish() {
  _e2e_close_step
  python3 - "$E2E_MANIFEST" "$E2E_SUMMARY_JSON" "$E2E_RUN_ID" \
            "$E2E_PASS" "$E2E_FAIL" "$E2E_ASSERT_PASS" "$E2E_ASSERT_FAIL" "$E2E_REPO_ROOT" <<'PY'
import json, sys, os
manifest, out, run_id, sp, sf, ap, af, root = sys.argv[1:9]
steps = []
if os.path.exists(manifest):
    for line in open(manifest):
        line = line.rstrip("\n")
        if not line:
            continue
        no, label, cwd, exit_, sdig, sbytes, edig, ebytes, asserts_file, verdict = line.split("\t")
        alist = []
        if os.path.exists(asserts_file):
            for a in open(asserts_file):
                a = a.rstrip("\n")
                if "\t" in a:
                    v, d = a.split("\t", 1)
                    alist.append({"verdict": v, "desc": d})
        # The full argv is captured per-step alongside the asserts file.
        argv_file = asserts_file[:-len(".asserts")] + ".argv" if asserts_file.endswith(".asserts") else ""
        argv = ""
        if argv_file and os.path.exists(argv_file):
            argv = open(argv_file).read().rstrip("\n")
        steps.append({
            "step": int(no), "label": label, "argv": argv,
            "cwd": os.path.relpath(cwd, root) if cwd.startswith(root) else cwd,
            "exit": int(exit_), "stdout_digest": sdig, "stdout_bytes": int(sbytes),
            "stderr_digest": edig, "stderr_bytes": int(ebytes),
            "verdict": verdict, "assertions": alist,
        })
summary = {
    "schema": "fmd-e2e-v1",
    "run_id": run_id,
    "totals": {
        "steps": len(steps),
        "steps_passed": int(sp), "steps_failed": int(sf),
        "assertions": int(ap) + int(af),
        "assertions_passed": int(ap), "assertions_failed": int(af),
    },
    "steps": steps,
}
with open(out, "w") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)
    fh.write("\n")
PY
  local total=$((E2E_PASS+E2E_FAIL))
  _e2e_log ""
  _e2e_log "e2e ${E2E_RUN_ID}: ${E2E_PASS}/${total} steps passed; assertions ${E2E_ASSERT_PASS} ok / ${E2E_ASSERT_FAIL} failed"
  _e2e_log "artifacts: ${E2E_ART}/ (ledger.txt, summary.json, steps/)"
  if [ "$E2E_FAIL" -ne 0 ]; then
    _e2e_log "e2e ${E2E_RUN_ID}: FAILED — ${E2E_FAIL} step(s) had a failing assertion."
    return 70
  fi
  _e2e_log "e2e ${E2E_RUN_ID}: ok."
  return 0
}

# ---------------------------------------------------------------------------
# --self-test: drive the harness through a known-pass and a known-fail step and
# verify the machinery recorded each correctly. Exits 0 iff the harness behaved.
# ---------------------------------------------------------------------------
_e2e_self_test() {
  e2e_init "selftest"
  # Known-pass step: a deterministic command with predictable stdout/exit.
  e2e_run "selftest-pass" -- printf 'HELLO-E2E'
  e2e_expect_exit 0
  e2e_expect_stdout_contains "HELLO-E2E"
  e2e_expect_stdout_nonempty
  # Known-fail step: a command that exits non-zero AND a deliberately wrong
  # assertion, so BOTH the exit check and a content check are exercised on the
  # failure path. These failures must be RECORDED, not abort the harness.
  e2e_run "selftest-fail" -- sh -c 'printf WORLD; exit 3'
  e2e_expect_exit 0                  # wrong on purpose: real exit is 3 -> records fail
  e2e_expect_stdout_contains "ABSENT" # wrong on purpose: stdout is WORLD -> records fail
  e2e_expect_exit 3                  # correct -> records pass
  # Close steps + write summary.json. e2e_finish returns 70 here BY DESIGN (the
  # injected fail), proving the harness reports failures; we swallow that and judge
  # the self-test on whether the recorded outcomes are exactly what we injected.
  e2e_finish || true

  # Now verify the machinery: exactly the injected outcomes must be present.
  local ok=0
  if [ "$E2E_PASS" -ne 1 ]; then _e2e_warn "self-test: expected 1 passing step, got ${E2E_PASS}"; ok=1; fi
  if [ "$E2E_FAIL" -ne 1 ]; then _e2e_warn "self-test: expected 1 failing step, got ${E2E_FAIL}"; ok=1; fi
  if [ "$E2E_ASSERT_PASS" -ne 4 ]; then _e2e_warn "self-test: expected 4 passing assertions, got ${E2E_ASSERT_PASS}"; ok=1; fi
  if [ "$E2E_ASSERT_FAIL" -ne 2 ]; then _e2e_warn "self-test: expected 2 failing assertions, got ${E2E_ASSERT_FAIL}"; ok=1; fi
  # The JSON summary must exist and be well-formed with the expected totals.
  python3 - "$E2E_SUMMARY_JSON" <<'PY' || ok=1
import json, sys
d = json.load(open(sys.argv[1]))
assert d["schema"] == "fmd-e2e-v1", d.get("schema")
t = d["totals"]
assert t["steps"] == 2, t
assert t["steps_passed"] == 1 and t["steps_failed"] == 1, t
assert t["assertions_passed"] == 4 and t["assertions_failed"] == 2, t
# The known-pass step must carry a real digest + byte count.
s0 = d["steps"][0]
assert s0["stdout_bytes"] == len("HELLO-E2E"), s0
assert s0["stdout_digest"].startswith("sha256:"), s0
print("self-test: summary.json well-formed (2 steps, 4/2 assertions, digests present)")
PY
  if [ "$ok" -eq 0 ]; then
    printf 'e2e: self-test PASS — known-pass and known-fail steps recorded correctly.\n'
    return 0
  fi
  printf 'e2e: self-test FAILED — harness machinery did not record outcomes as expected.\n' >&2
  return 70
}

# ---------------------------------------------------------------------------
# CLI entrypoint (only when executed directly, not when sourced).
# ---------------------------------------------------------------------------
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  set -uo pipefail
  cd "$E2E_REPO_ROOT"
  case "${1:-}" in
    --self-test) _e2e_self_test; exit $? ;;
    --help|-h)   sed -n '2,40p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *)
      printf 'e2e/lib.sh is a library meant to be sourced by an e2e suite.\n' >&2
      printf 'Run "scripts/e2e/lib.sh --self-test" to verify the harness machinery.\n' >&2
      exit 64
      ;;
  esac
fi
