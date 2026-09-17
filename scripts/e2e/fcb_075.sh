#!/usr/bin/env bash
# scripts/e2e/fcb_075.sh — e2e: FCB-075 production verification.
#
# Verifies the FrankenMarkdown font/context/raster extensions (FCB-075.A) and
# the optional safe Mac font adapter (FCB-075.B) together through their real
# supported public implementation: the first-party cargo test suites under
# fmd-font/tests/. The adapter is exercised via SimulatedMacBridge, so the
# scenario is deterministic on every host (no CoreText/GPU/network needed).
#
# Oracle corpus covered: shared TextRunContext shaping + hit testing, exact
# logical selection via UTF-16/bidi domain conversions, fallback-font identity
# preservation, color-glyph RGBA rasters, dimension caps, and the negative
# controls (context budget, empty text, dimension overflow, disabled fallback).
#
# Route (documented exact invocation):
#   scripts/e2e/fcb_075.sh [run-id]
#
# Exit:  0 ok · 70 an assertion failed.
set -uo pipefail
# The suites run locally by design (deterministic, fast, no GPU/network);
# bypass the shared cargo shim exactly like the repo's own e2e_build_bin does.
export RCH_SHIM_LOCAL_IDE=1
export PATH="$HOME/.cargo/bin:$PATH"
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-fcb-075}"

# --- suite 1: shared context + exact logical selection ----------------------
# TextRunContext: shaping, hit testing, caret affinity, selection rectangles,
# UTF-8 <-> UTF-16 domain conversions, ligature/combining cluster mapping,
# empty-run and out-of-bounds negative controls.
e2e_run "text_run shared-context suite" -- \
  cargo test -p fmd-font --test text_run_test
e2e_expect_exit 0
e2e_expect_stdout_contains "test result: ok"
e2e_expect_stdout_contains "ligature_and_combining_mark_cluster_mapping"
e2e_expect_stdout_contains "rtl_text_run_cluster_coordinates_and_caret_affinity"
e2e_expect_stdout_contains "domain_conversions_ascii_multibyte_and_astral_surrogates"
e2e_expect_stdout_contains "negative_controls_empty_run_and_out_of_bounds_queries"

# --- suite 2: fallback-font extensions --------------------------------------
# NativeShapingRoute: mixed-script fallback identity preservation, disabled
# fallback refusal, context work budget enforcement, mid-scalar rejection,
# RTL platform-route layout + hit testing.
e2e_run "native_route fallback suite" -- \
  cargo test -p fmd-font --test native_route_test
e2e_expect_exit 0
e2e_expect_stdout_contains "test result: ok"
e2e_expect_stdout_contains "mixed_fallback_font_preserves_distinct_fallback_identities"
e2e_expect_stdout_contains "fallback_disabled_yields_fallback_required_error"
e2e_expect_stdout_contains "context_work_budget_enforcement_negative_control"

# --- suite 3: safe Mac adapter + raster extensions --------------------------
# MacFontAdapter through SimulatedMacBridge: capabilities honesty, full
# oracle corpus (CJK/RTL/joining/combining/emoji/tab/ligature), color-glyph
# BGRA->RGBA extraction, MAX_MAC_RASTER_DIMENSION caps, foreign-call failure
# and buffer-mismatch defenses.
e2e_run "macos adapter + raster suite" -- \
  cargo test -p fmd-font --test macos_font_adapter_test
e2e_expect_exit 0
e2e_expect_stdout_contains "test result: ok"
e2e_expect_stdout_contains "mac_adapter_capabilities_and_kind"
e2e_expect_stdout_contains "corpus_latin_shaping_preserves_primary_font"

# --- evidence: the three suites exercised the same oracle corpus ------------
# The manifest gains a row per closed step; step 3's row lands at e2e_finish,
# so two rows must already be recorded here.
e2e_assert "run manifest records completed suites" -- test "$(wc -l <"${E2E_MANIFEST}" | tr -d ' ')" -ge 2

e2e_finish
