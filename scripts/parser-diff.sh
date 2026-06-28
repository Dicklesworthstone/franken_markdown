#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$repo_root/target/fmd-checks}"

echo "fmd parser harness: focused conformance"
cargo test --test parser_conformance

echo "fmd parser harness: metamorphic pseudo-fuzz"
cargo test --test parser_metamorphic

echo "fmd parser harness: approved differential fixtures"
cargo test --test parser_differential -- --nocapture

echo "fmd parser harness: ok"
