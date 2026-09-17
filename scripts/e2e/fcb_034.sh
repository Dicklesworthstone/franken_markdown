#!/bin/sh
# FCB-034.V joint verification campaign: FrankenMarkdown math/diagram display
# output + FCB renderer mapping (math and diagram primitives).
#
# Supported route: strict-remote execution via RCH from each repository
# toplevel. The upstream owner is franken_markdown; the renderer mapping
# consumer is franken_code_browser. Run from the franken_markdown checkout:
#
#   RCH_REQUIRE_REMOTE=1 rch exec --base HEAD --clean-overlay --no-overlay -- \
#     sh scripts/e2e/fcb_034.sh [FCB_REPO]
#
# FCB_REPO defaults to ../franken_code_browser. The script runs:
#   1. franken_markdown math/diagram display tests (mathml + display suites)
#   2. franken_code_browser fcb-render FCB-034 production scenarios
#
# The scenario exercises: qualified math/diagram corpus, hostile expansion
# and unsupported forms, source anchors surviving native render, work
# budget/depth limits, color pipeline and scissor invariants, and negative
# controls with oracle accuracy. Receipts land under
# target/fcb_034_receipts/ when the renderer suite writes them.
set -eu
cd "$(dirname "$0")/../.."
export RCH_CARGO_WRAPPER_BYPASS=1
export RCH_ENABLED=0

echo "== franken_markdown: mathml_test =="
cargo test --test mathml_test
echo "== franken_markdown: math_and_diagram_display =="
cargo test --test math_and_diagram_display

FCB_REPO="${1:-../franken_code_browser}"
if [ -d "$FCB_REPO/crates/fcb-render" ]; then
    echo "== franken_code_browser: fcb_034_production =="
    (cd "$FCB_REPO" && cargo test -p fcb-render --test fcb_034_production)
else
    echo "FCB_REPO not found ($FCB_REPO); renderer-mapping leg skipped." >&2
    exit 2
fi
