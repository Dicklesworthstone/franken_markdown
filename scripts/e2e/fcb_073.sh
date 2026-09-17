#!/bin/sh
# FCB-073.V joint verification campaign: FrankenMarkdown nested source
# provenance + spanned API compatibility + source mapping.
#
# Supported route: strict-remote execution via RCH from the repository
# toplevel:
#
#   RCH_REQUIRE_REMOTE=1 rch exec --base HEAD --clean-overlay --no-overlay -- \
#     cargo test --test nested_provenance_test
#
# The campaign exercises:
#   - Nested inline and generated content provenance oracle
#   - Invalid ranges and transclusion sources
#   - No invented contiguous literal Markdown
#   - Spanned API compatibility and source mapping
set -eu
cd "$(dirname "$0")/../.."
export RCH_CARGO_WRAPPER_BYPASS=1
export RCH_ENABLED=0
exec cargo test --test nested_provenance_test -- --nocapture
