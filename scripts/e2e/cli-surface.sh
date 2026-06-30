#!/usr/bin/env bash
# scripts/e2e/cli-surface.sh — e2e: full CLI command + flag surface (bead grn.4.2).
#
# Drives every command and global flag of the real fmd binary through the
# structured-logging harness (scripts/e2e/lib.sh): render aliases, the discovery
# commands (capabilities / doctor / robot-docs / --robot-triage) with and without
# --json, the config subcommands, --no-config interactions, and the stdout=data /
# stderr=diagnostics contract under NO_COLOR / CI / TERM=dumb.
#
# Usage: scripts/e2e/cli-surface.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-cli-surface}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"
DOC="${WORK}/doc.md"
printf '# Title One\n\nA paragraph with **bold** and `code`.\n\n- a\n- b\n' >"$DOC"

# --- render aliases ---------------------------------------------------------
e2e_run "render FILE -> stdout html" -- "$E2E_BIN" "$DOC" --to html
e2e_expect_exit 0
e2e_expect_stdout_contains "<main"
e2e_expect_stdout_contains "Title One"

E2E_STDIN="$DOC"
e2e_run "render '-' (stdin) -> stdout html" -- "$E2E_BIN" - --to html
e2e_expect_exit 0
e2e_expect_stdout_contains "Title One"

e2e_run "render --text -> stdout html" -- "$E2E_BIN" --text '# Inline Heading' --to html
e2e_expect_exit 0
e2e_expect_stdout_contains "Inline Heading"

# stdout is DATA, stderr is DIAGNOSTICS: a plain render must emit nothing on stderr.
e2e_run "render stdout/stderr separation" -- "$E2E_BIN" --text '# x' --to html
e2e_expect_exit 0
e2e_expect_stdout_nonempty
e2e_assert "stderr is empty on a clean render" -- test ! -s "$E2E_LAST_STDERR"

# --json on a FILE render writes the status envelope to STDERR; stdout stays clean.
# (Streaming HTML to stdout emits no envelope — there is no "wrote file" event.)
e2e_run "render --json envelope to stderr (file out)" -- \
  "$E2E_BIN" --text '# x' --to html --out "${WORK}/envelope.html" --json
e2e_expect_exit 0
e2e_expect_stdout_empty
e2e_expect_stderr_contains '"event":"wrote"'
e2e_expect_file "${WORK}/envelope.html"

# --- discovery commands -----------------------------------------------------
e2e_run "capabilities (human)" -- "$E2E_BIN" capabilities
e2e_expect_exit 0
e2e_expect_stdout_nonempty

e2e_run "capabilities --json" -- "$E2E_BIN" capabilities --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"contract_version"'
e2e_expect_stdout_contains '"exit_codes"'
e2e_assert "capabilities --json is valid JSON" -- \
  sh -c "python3 -c 'import json,sys; json.load(open(sys.argv[1]))' '$E2E_LAST_STDOUT'"

e2e_run "doctor" -- "$E2E_BIN" doctor
e2e_expect_exit 0
e2e_run "doctor --json" -- "$E2E_BIN" doctor --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"dependency_posture"'
e2e_assert "doctor --json is valid JSON" -- \
  sh -c "python3 -c 'import json,sys; json.load(open(sys.argv[1]))' '$E2E_LAST_STDOUT'"

e2e_run "robot-docs guide" -- "$E2E_BIN" robot-docs guide
e2e_expect_exit 0
e2e_expect_stdout_contains "fmd"

e2e_run "--robot-triage" -- "$E2E_BIN" --robot-triage
e2e_expect_exit 0
e2e_expect_stdout_contains '"recommended_next_actions"'
e2e_assert "--robot-triage is valid JSON" -- \
  sh -c "python3 -c 'import json,sys; json.load(open(sys.argv[1]))' '$E2E_LAST_STDOUT'"

# --- config subcommands (real temp config via FMD_CONFIG) --------------------
CFG="${WORK}/fmd.config"
rm -f "$CFG"
e2e_run "config path --json" -- env FMD_CONFIG="$CFG" "$E2E_BIN" config path --json
e2e_expect_exit 0
e2e_expect_stdout_contains "$CFG"

e2e_run "config set font serif --json" -- env FMD_CONFIG="$CFG" "$E2E_BIN" config set font serif --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"value":"serif"'
e2e_expect_file "$CFG"

e2e_run "config get font --json (reads persisted value)" -- env FMD_CONFIG="$CFG" "$E2E_BIN" config get font --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"value":"serif"'

e2e_run "config show --json" -- env FMD_CONFIG="$CFG" "$E2E_BIN" config show --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"config"'

# --no-config makes a render reproducible (ignores the persisted serif default).
e2e_run "config set + --no-config is rejected" -- env FMD_CONFIG="$CFG" "$E2E_BIN" config set font sans --no-config
e2e_expect_exit 64
e2e_expect_stderr_contains "no-config"

# --- color / CI / dumb-terminal parity --------------------------------------
e2e_run "bare fmd prints help, exits 0" -- "$E2E_BIN"
e2e_expect_exit 0
e2e_expect_stdout_nonempty

e2e_run "NO_COLOR + CI + TERM=dumb render still clean" -- \
  env NO_COLOR=1 CI=1 TERM=dumb "$E2E_BIN" --text '# x' --to html
e2e_expect_exit 0
e2e_expect_stdout_contains "<main"
e2e_assert "no ANSI escape bytes in output under NO_COLOR" -- \
  sh -c "! grep -q $'\\x1b\\[' '$E2E_LAST_STDOUT'"

e2e_finish
exit $?
