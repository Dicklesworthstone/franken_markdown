#!/usr/bin/env bash
# check-test-doubles.sh — gate against new, undocumented test doubles (bead grn.3.2).
#
# franken_markdown's testing doctrine is mock-free: tests exercise the real system
# with real inputs (real Markdown, real bundled fonts, real config files, the real
# binary). The ONE sanctioned exception is `StubMetrics` in tests/layout_test.rs —
# a deterministic, hand-computable metrics ORACLE for the Knuth-Plass line breaker,
# not a mock of the system under test (justified in docs/TESTING.md, bead grn.3.1).
#
# This gate scans the test + source tree for type-level test doubles
# (struct/enum/trait/type names containing Stub/Mock/Fake/Dummy) and fails if any
# appear that are not on the allowlist below. A new double must either be replaced
# with a real-input test, or — if it is a genuine oracle like StubMetrics —
# allowlisted here WITH a written justification and documented in docs/TESTING.md.
#
# Usage:
#   scripts/check-test-doubles.sh            # scan the repo, enforce the allowlist
#   scripts/check-test-doubles.sh --self-test  # verify the detector itself (CI)
#
# Exit codes: 0 ok · 1 a non-allowlisted double was found · 2 usage/env error.
set -uo pipefail
cd "$(dirname "$0")/.." || exit

# Allowlist of sanctioned doubles (by type name). Keep this SHORT and justified.
#   <TypeName>    one-line justification
ALLOWLIST_NAMES=(
  "StubMetrics"   # algorithm-isolation metrics oracle for layout (grn.3.1); see docs/TESTING.md
)

# Pattern for a type-level test double definition.
DOUBLE_RE='(struct|enum|trait|type)[[:space:]]+[A-Za-z0-9_]*(Stub|Mock|Fake|Dummy)[A-Za-z0-9_]*'

# scan_doubles <root> — print "file:line:Name" for every type-level double found
# under <root> (searches Rust sources only).
scan_doubles() {
  local root="$1"
  # grep -rnE over .rs files; extract the defined type name from each hit.
  grep -rnE --include='*.rs' "$DOUBLE_RE" "$root" 2>/dev/null | while IFS= read -r line; do
    # line = path:lineno:   <decl> Name...
    local loc="${line%%:*}"
    local rest="${line#*:}"
    local lineno="${rest%%:*}"
    # Pull the identifier following the struct/enum/trait/type keyword.
    local name
    name="$(printf '%s\n' "$line" | sed -nE "s/.*(struct|enum|trait|type)[[:space:]]+([A-Za-z0-9_]*(Stub|Mock|Fake|Dummy)[A-Za-z0-9_]*).*/\2/p")"
    [ -n "$name" ] && printf '%s:%s:%s\n' "$loc" "$lineno" "$name"
  done
}

is_allowlisted() {
  local name="$1"
  local a
  for a in "${ALLOWLIST_NAMES[@]}"; do
    [ "$name" = "$a" ] && return 0
  done
  return 1
}

# enforce <root> — scan and fail on any non-allowlisted double. Echoes findings.
enforce() {
  local root="$1"
  local found violations=0
  found="$(scan_doubles "$root")"
  if [ -z "$found" ]; then
    return 0
  fi
  local entry name
  while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    name="${entry##*:}"
    if is_allowlisted "$name"; then
      printf 'check-test-doubles: allowlisted double %s (%s)\n' "$name" "$entry"
    else
      printf 'check-test-doubles: VIOLATION — undocumented test double %s at %s\n' "$name" "$entry" >&2
      violations=$((violations + 1))
    fi
  done <<EOF
$found
EOF
  [ "$violations" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  # 1) A planted, non-allowlisted double MUST be detected (enforce fails).
  printf 'struct EvilMock;\n' >"$tmp/bad.rs"
  if enforce "$tmp" >/dev/null 2>&1; then
    echo "check-test-doubles: self-test FAILED — planted EvilMock was not detected" >&2
    exit 1
  fi
  # 2) An allowlisted-name double MUST pass.
  : >"$tmp/bad.rs"
  printf 'struct StubMetrics;\n' >"$tmp/ok.rs"
  if ! enforce "$tmp" >/dev/null 2>&1; then
    echo "check-test-doubles: self-test FAILED — allowlisted StubMetrics was rejected" >&2
    exit 1
  fi
  # 3) The real repo tree MUST pass.
  if ! enforce "tests" >/dev/null 2>&1 || ! enforce "src" >/dev/null 2>&1; then
    echo "check-test-doubles: self-test FAILED — the live tree has a non-allowlisted double" >&2
    exit 1
  fi
  echo "check-test-doubles: self-test ok (detects planted doubles; allowlist honored; tree clean)"
  exit 0
elif [ -n "${1:-}" ]; then
  echo "check-test-doubles: unknown argument '$1' (use --self-test or no args)" >&2
  exit 2
fi

echo "check-test-doubles: scanning tests/ and src/ for type-level test doubles"
ok=0
enforce "tests" || ok=1
enforce "src" || ok=1
if [ "$ok" -ne 0 ]; then
  echo "check-test-doubles: FAILED — add a real-input test instead, or allowlist a genuine oracle in this script + docs/TESTING.md." >&2
  exit 1
fi
echo "check-test-doubles: ok — every test double is on the justified allowlist."
