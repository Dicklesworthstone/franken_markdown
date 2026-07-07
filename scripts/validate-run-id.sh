#!/usr/bin/env bash
# Shared artifact run-id policy for helper scripts.
#
# Source this file from Bash scripts before deriving any tests/artifacts path
# from a caller-provided id. The grammar deliberately forbids slashes, absolute
# paths, whitespace, shell metacharacters, and dot-only names while keeping
# timestamp-style ids and bead/script labels ergonomic.

FMD_RUN_ID_PATTERN='^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$'

fmd_validate_run_id() {
  local label="${1:-script}"
  local value="${2:-}"
  local subject="${3:-run-id}"
  if [[ ! "$value" =~ $FMD_RUN_ID_PATTERN ]]; then
    printf '%s\n' "${label}: ${subject} must match ${FMD_RUN_ID_PATTERN}" >&2
    exit 64
  fi
}
