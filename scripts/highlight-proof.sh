#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "fmd highlight proof: approved token fixtures"
cargo test --test highlight_test approved_highlight_token_fixtures_match -- --nocapture

echo "fmd highlight proof: deterministic mixed-language stress"
cargo test --test highlight_test large_mixed_language_highlight_stress_is_deterministic -- --nocapture

echo "fmd highlight proof: PDF consumes shared token stream"
cargo test --test pdf_test pdf_code_blocks_use_shared_syntax_highlight_colors -- --nocapture

echo "fmd highlight proof: ok"
