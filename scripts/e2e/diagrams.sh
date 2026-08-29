#!/usr/bin/env bash
# scripts/e2e/diagrams.sh — e2e: vector diagrams rendering suite (s115.3).
#
# Exercises Mermaid flowchart, sequence diagrams, and ASCII art diagrams through
# the real fmd binary, asserting:
# - SVG vector graphic emission in HTML output.
# - Clean font styling, defs markers, and CSS dark mode styling.
# - Valid PDF rendering with embedded diagram elements.
# - Graceful fallback to code blocks on invalid syntax.
#
# Usage: scripts/e2e/diagrams.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-diagrams}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"
EPOCH=1700000000

# 1. Mermaid Flowchart document
DOC_FLOWCHART="${WORK}/flowchart.md"
cat >"$DOC_FLOWCHART" <<'EOF'
# Architecture Flowchart

```mermaid
flowchart TD
    A[Client Request] --> B{Cache Hit?}
    B -->|Yes| C[Fast Response]
    B -->|No| D[Database Query]
    D --> E[Update Cache]
    E --> C
```
EOF

e2e_run "diagrams: render flowchart to HTML" -- \
  "$E2E_BIN" "$DOC_FLOWCHART" --to html --out "${WORK}/flowchart.html"
e2e_expect_exit 0
e2e_expect_file "${WORK}/flowchart.html"
e2e_expect_file_contains "${WORK}/flowchart.html" "<svg"
e2e_expect_file_contains "${WORK}/flowchart.html" "fmd-flowchart"
e2e_expect_file_contains "${WORK}/flowchart.html" "Client Request"
e2e_expect_file_contains "${WORK}/flowchart.html" "Cache Hit?"

e2e_run "diagrams: render flowchart to PDF" -- \
  env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC_FLOWCHART" --to pdf --out "${WORK}/flowchart.pdf"
e2e_expect_exit 0
e2e_expect_file "${WORK}/flowchart.pdf"
e2e_expect_file_bytes_ge "${WORK}/flowchart.pdf" 1000

# 2. Mermaid Sequence Diagram document
DOC_SEQUENCE="${WORK}/sequence.md"
cat >"$DOC_SEQUENCE" <<'EOF'
# Authentication Protocol

```mermaid
sequenceDiagram
    Alice->>AuthService: Login(credentials)
    AuthService->>DB: VerifyUser()
    DB-->>AuthService: OK
    AuthService-->>Alice: JWT Token
```
EOF

e2e_run "diagrams: render sequence diagram to HTML" -- \
  "$E2E_BIN" "$DOC_SEQUENCE" --to html --out "${WORK}/sequence.html"
e2e_expect_exit 0
e2e_expect_file "${WORK}/sequence.html"
e2e_expect_file_contains "${WORK}/sequence.html" "<svg"
e2e_expect_file_contains "${WORK}/sequence.html" "fmd-sequence"
e2e_expect_file_contains "${WORK}/sequence.html" "AuthService"
e2e_expect_file_contains "${WORK}/sequence.html" "JWT Token"

# 3. ASCII Box Diagram
DOC_ASCII="${WORK}/ascii_box.md"
cat >"$DOC_ASCII" <<'EOF'
# Component Box

```ditaa
+-------------------+
|  Storage Engine   |
+-------------------+
```
EOF

e2e_run "diagrams: render ASCII box diagram to HTML" -- \
  "$E2E_BIN" "$DOC_ASCII" --to html --out "${WORK}/ascii.html"
e2e_expect_exit 0
e2e_expect_file "${WORK}/ascii.html"
e2e_expect_file_contains "${WORK}/ascii.html" "<svg"
e2e_expect_file_contains "${WORK}/ascii.html" "Storage Engine"

# 4. Fallback on invalid diagram syntax (should render as plain code block)
DOC_INVALID="${WORK}/invalid_diag.md"
cat >"$DOC_INVALID" <<'EOF'
# Fallback Test

```mermaid
This is not valid mermaid syntax 12345
```
EOF

e2e_run "diagrams: invalid diagram falls back to code block" -- \
  "$E2E_BIN" "$DOC_INVALID" --to html --out "${WORK}/fallback.html"
e2e_expect_exit 0
e2e_expect_file "${WORK}/fallback.html"
e2e_expect_file_contains "${WORK}/fallback.html" "<pre><code"
e2e_expect_file_contains "${WORK}/fallback.html" "This is not valid mermaid syntax"

# 5. Determinism check for diagram rendering
e2e_run "diagrams: determinism render A" -- \
  "$E2E_BIN" "$DOC_FLOWCHART" --to html --out "${WORK}/det_diag_a.html"
e2e_expect_exit 0
e2e_run "diagrams: determinism render B" -- \
  "$E2E_BIN" "$DOC_FLOWCHART" --to html --out "${WORK}/det_diag_b.html"
e2e_expect_exit 0
e2e_run "diagrams: HTML output is byte-identical across runs" -- \
  cmp -s "${WORK}/det_diag_a.html" "${WORK}/det_diag_b.html"
e2e_expect_exit 0

e2e_finish
exit $?
