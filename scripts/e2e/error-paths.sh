#!/usr/bin/env bash
# scripts/e2e/error-paths.sh — e2e: error-path & robustness (bead grn.4.4).
#
# Negative-path coverage against the real fmd binary with EXACT exit-code and
# actionable-message assertions, plus robustness checks that hostile input is
# rejected cleanly (no panic / no crash). Every cell asserts the documented exit
# code from the README "Exit codes" table:
#   0 success · 64 usage · 66 input · 70 render · 73 output-write · 74 stdout-write
#
# Usage: scripts/e2e/error-paths.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-error-paths}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"

# Assert the step never panicked or aborted (no Rust panic backtrace; exit is a
# documented code, never 101/134/139). Use after any robustness step.
assert_no_panic() {
  e2e_assert "no rust panic in stderr" -- sh -c "! grep -qiE 'panic|RUST_BACKTRACE|aborted' '$E2E_LAST_STDERR'"
  e2e_assert "exit is a documented code (not a crash)" -- \
    sh -c 'case '"$E2E_LAST_EXIT"' in 0|64|66|70|73|74) exit 0;; *) exit 1;; esac'
}

# --- usage errors (64) ------------------------------------------------------
e2e_run "unknown flag -> 64" -- "$E2E_BIN" --bogus-flag --text '# x'
e2e_expect_exit 64
e2e_expect_stderr_contains "unexpected argument"

e2e_run "bad flag value (--to bogus) -> 64" -- "$E2E_BIN" --text '# x' --to bogus
e2e_expect_exit 64
e2e_expect_stderr_contains "invalid value"

e2e_run "pdf --out - refused -> 64" -- "$E2E_BIN" --text '# x' --to pdf --out -
e2e_expect_exit 64

e2e_run "unknown config key -> 64" -- env FMD_CONFIG="${WORK}/c1" "$E2E_BIN" config set bogus_key 1
e2e_expect_exit 64
e2e_expect_stderr_contains "unknown config key"

e2e_run "config set + --no-config -> 64" -- "$E2E_BIN" config set font sans --no-config
e2e_expect_exit 64
e2e_expect_stderr_contains "no-config"

# --- input errors (66) ------------------------------------------------------
e2e_run "missing input file -> 66" -- "$E2E_BIN" "${WORK}/does-not-exist.md" --to html
e2e_expect_exit 66

printf '# Hi \xff\xfe\x80 invalid\n' >"${WORK}/non-utf8.md"
e2e_run "non-UTF8 input -> 66, no panic" -- "$E2E_BIN" "${WORK}/non-utf8.md" --to html
e2e_expect_exit 66
e2e_expect_stderr_contains "not UTF-8"
assert_no_panic

printf 'way too many bytes for the cap' >"${WORK}/big.md"
e2e_run "oversized input (--max-input-bytes 4) -> 66" -- "$E2E_BIN" --max-input-bytes 4 "${WORK}/big.md" --to html
e2e_expect_exit 66

e2e_run "missing --css file -> 66" -- "$E2E_BIN" --text '# x' --css "${WORK}/nope.css" --to html
e2e_expect_exit 66
e2e_expect_stderr_contains "stylesheet"

e2e_run "missing --pdf-image file -> 66" -- "$E2E_BIN" --text '# x' --to pdf --out "${WORK}/img.pdf" --pdf-image a.png="${WORK}/nope.png"
e2e_expect_exit 66

# A malformed spec is a USAGE error (64): the flag argument is wrong, no file was
# consulted. (A missing/oversized file is the input error, 66, above.)
e2e_run "malformed --pdf-image spec (no '=') -> 64" -- "$E2E_BIN" --text '# x' --to pdf --out "${WORK}/img.pdf" --pdf-image noequalsign
e2e_expect_exit 64
e2e_expect_stderr_contains "MARKDOWN_DEST=PATH"

printf 'this is not a valid config line\n' >"${WORK}/junk.cfg"
e2e_run "malformed config -> 66, no panic" -- env FMD_CONFIG="${WORK}/junk.cfg" "$E2E_BIN" config show
e2e_expect_exit 66
e2e_expect_stderr_contains "expected key=value"
assert_no_panic

# --- output-write error (73) ------------------------------------------------
printf 'x' >"${WORK}/afile"
e2e_run "unwritable --out (under a file-as-dir) -> 73" -- "$E2E_BIN" --text '# x' --to html --out "${WORK}/afile/out.html"
e2e_expect_exit 73
e2e_expect_stderr_contains "writing"

# --- benign edge cases that must SUCCEED (0) --------------------------------
: >"${WORK}/empty.md"
e2e_run "empty input renders (0)" -- "$E2E_BIN" "${WORK}/empty.md" --to html
e2e_expect_exit 0
e2e_expect_stdout_contains "<html"
assert_no_panic

# A large-but-allowed input must render fine (exercises the size guard's pass path).
python3 -c "open('${WORK}/large.md','w').write('# Big\n\n' + ('lorem ipsum dolor sit amet. ' * 5000))"
e2e_run "large allowed input renders (0)" -- "$E2E_BIN" "${WORK}/large.md" --to html --out "${WORK}/large.html"
e2e_expect_exit 0
e2e_expect_file_bytes_ge "${WORK}/large.html" 50000
assert_no_panic

# --- robustness: a corrupt PNG supplied via --pdf-image is handled gracefully --
# fmd degrades rather than crashing: the unreadable image is dropped and the rest
# of the document still renders to a valid, deterministic PDF (exit 0, no panic).
printf '\x89PNG\r\n\x1a\n garbage-not-a-real-png' >"${WORK}/corrupt.png"
e2e_run "corrupt --pdf-image PNG handled without crashing" -- "$E2E_BIN" --text '![x](a.png)' --to pdf --out "${WORK}/corrupt.pdf" --pdf-image a.png="${WORK}/corrupt.png"
e2e_expect_exit 0
assert_no_panic
e2e_run "corrupt-image render still produced a valid PDF" -- sh -c "head -c5 '${WORK}/corrupt.pdf' | grep -q '%PDF-'"
e2e_expect_exit 0

e2e_finish
exit $?
