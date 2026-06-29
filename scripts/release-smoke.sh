#!/usr/bin/env bash
#
# release-smoke.sh <path-to-fmd> — smoke-test a built `fmd` binary before it is
# attached to a release (bead 08f). Exercises the agent-first entry points and
# both output formats, asserting stable exit codes and real output. Used by the
# release workflow on every platform's freshly built binary, and runnable
# locally against `target/release/fmd`.
#
# Exit codes: 0 ok, 64 usage, 66 binary missing/not executable, 70 a check failed.

set -euo pipefail

FMD="${1:-}"
if [ -z "$FMD" ]; then
  echo "usage: release-smoke.sh <path-to-fmd>" >&2
  exit 64
fi
if [ ! -x "$FMD" ]; then
  echo "release-smoke: '$FMD' is not an executable binary" >&2
  exit 66
fi

work="$(mktemp -d)"
cleanup() {
  # No `rm -rf`: the smoke test only writes flat files into $work.
  rm -f "$work"/* 2>/dev/null || true
  rmdir "$work" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  echo "release-smoke: FAIL — $*" >&2
  exit 70
}

echo "release-smoke: $FMD"

# 1. --version prints something and exits 0.
"$FMD" --version >/dev/null 2>&1 || fail "--version exited nonzero"

# 2. bare invocation prints help and exits without blocking (never a TUI).
"$FMD" --help >/dev/null 2>&1 || fail "--help exited nonzero"

# 3. capabilities --json is valid JSON (the versioned capabilities contract).
"$FMD" capabilities --json > "$work/caps.json" 2>/dev/null \
  || fail "capabilities --json exited nonzero"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$work/caps.json" \
  || fail "capabilities --json is not valid JSON"

# 4. doctor --json runs and emits JSON.
"$FMD" doctor --json > "$work/doctor.json" 2>/dev/null \
  || fail "doctor --json exited nonzero"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$work/doctor.json" \
  || fail "doctor --json is not valid JSON"

# 5. render a Markdown file to HTML.
printf '# Title\n\nHello **world** with a [link](https://example.com).\n' > "$work/in.md"
"$FMD" "$work/in.md" --out "$work/out.html" || fail "file -> HTML exited nonzero"
grep -q "<h1" "$work/out.html" || fail "HTML output is missing an <h1>"

# 6. render the same file to PDF (must be a real PDF).
"$FMD" "$work/in.md" --to pdf --out "$work/out.pdf" || fail "file -> PDF exited nonzero"
head -c 5 "$work/out.pdf" | grep -q "%PDF-" || fail "PDF output is missing the %PDF- header"

# 7. stdin path: `fmd - `.
printf '# From stdin\n' | "$FMD" - --out "$work/stdin.html" || fail "stdin -> HTML exited nonzero"
grep -q "<h1" "$work/stdin.html" || fail "stdin HTML output is missing an <h1>"

# 8. --text to stdout (stdout is data).
"$FMD" --text '# Hello from fmd' --out - 2>/dev/null | grep -q "<h1" \
  || fail "--text --out - did not write HTML to stdout"

# 9. a bad path fails with a nonzero (but not crashing) exit code.
if "$FMD" "$work/does-not-exist.md" --out "$work/none.html" >/dev/null 2>&1; then
  fail "a missing input file should exit nonzero"
fi

echo "release-smoke: ok — version, help, capabilities/doctor JSON, HTML+PDF, stdin, --text, error path"
