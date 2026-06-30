#!/usr/bin/env bash
# scripts/e2e/installer.sh — e2e: the install.sh installer, sandboxed (bead grn.4.6).
#
# Drives the real install.sh through the structured-logging harness in a throwaway
# sandbox dest, using its --from-source path against the LOCAL checkout (install.sh
# builds the current repo when run from its root — no network, no tag required),
# and asserts the installer produces a working `fmd` that renders.
#
# It does a release build, so run-all only includes it when E2E_RUN_INSTALLER=1.
# Run it directly any time: scripts/e2e/installer.sh [run-id]
#
# Exit: 0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-installer}"

# Preconditions for --from-source: cargo + git. Skip cleanly if absent (the
# installer e2e is meaningless without a toolchain).
if ! command -v cargo >/dev/null 2>&1 || ! command -v git >/dev/null 2>&1; then
  printf 'installer e2e: cargo/git not available — skipping.\n'
  e2e_finish
  exit $?
fi

WORK="${E2E_ART}/work"; mkdir -p "$WORK"
DEST="${WORK}/bin"
INSTALL="${E2E_REPO_ROOT}/install.sh"

# 1) --help is a clean usage surface (no build).
e2e_run "install.sh --help" -- bash "$INSTALL" --help
e2e_expect_exit 0
e2e_expect_stdout_contains "--from-source"
e2e_expect_stdout_contains "--dest"

# 2) Install from the local checkout into the sandbox dest (non-interactive).
e2e_run "install.sh --from-source --dest (sandbox)" -- \
  bash "$INSTALL" --from-source --dest "$DEST" --quiet --no-gum --force
e2e_expect_exit 0
e2e_expect_file "${DEST}/fmd"

# 3) The installed binary must run and report its surface.
e2e_run "installed fmd --help" -- "${DEST}/fmd" --help
e2e_expect_exit 0
e2e_expect_stdout_nonempty

# 4) The installed binary must actually render Markdown.
e2e_run "installed fmd renders html" -- "${DEST}/fmd" --text '# Installed OK' --to html
e2e_expect_exit 0
e2e_expect_stdout_contains "<main"
e2e_expect_stdout_contains "Installed OK"

# 5) The installed binary exposes the agent contract.
e2e_run "installed fmd capabilities --json" -- "${DEST}/fmd" capabilities --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"contract_version"'

e2e_finish
exit $?
