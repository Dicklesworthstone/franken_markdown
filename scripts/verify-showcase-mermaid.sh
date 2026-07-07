#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/validate-run-id.sh
source "scripts/validate-run-id.sh"

RUN_ID="showcase-mermaid"
SOURCE_MMD="examples/showcase-mermaid.mmd"
CONFIG_TOML="examples/showcase-frankenmermaid.toml"
EXPECTED_SVG="examples/showcase-mermaid.svg"

usage() {
  cat >&2 <<'USAGE'
usage: scripts/verify-showcase-mermaid.sh [--run-id RUN_ID]

Regenerate the checked-in frankenmermaid showcase SVG and byte-compare it with
examples/showcase-mermaid.svg. Set FRANKENMERMAID_BIN to force a specific
fm-cli/frankenmermaid executable; otherwise the script tries PATH and then a
sibling ../frankenmermaid checkout via cargo run.
USAGE
}

while (($#)); do
  case "$1" in
    --run-id)
      if (($# < 2)); then
        printf '%s\n' "verify-showcase-mermaid: --run-id requires a value" >&2
        exit 64
      fi
      RUN_ID="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf '%s\n' "verify-showcase-mermaid: unknown argument: $1" >&2
      usage
      exit 64
      ;;
  esac
done

fmd_validate_run_id "verify-showcase-mermaid" "$RUN_ID"

ART="tests/artifacts/svg-showcase/${RUN_ID}"
GENERATED_SVG="$ART/generated-no-spans.svg"
RENDER_JSON="$ART/frankenmermaid-render.json"

if [[ -n "${FRANKENMERMAID_BIN:-}" ]]; then
  FM_CMD=("$FRANKENMERMAID_BIN")
elif command -v fm-cli >/dev/null 2>&1; then
  FM_CMD=(fm-cli)
elif command -v frankenmermaid >/dev/null 2>&1; then
  FM_CMD=(frankenmermaid)
elif [[ -f "$ROOT/../frankenmermaid/Cargo.toml" ]]; then
  FM_CMD=(cargo run --manifest-path "$ROOT/../frankenmermaid/Cargo.toml" --bin fm-cli --)
else
  printf '%s\n' "verify-showcase-mermaid: could not find fm-cli, frankenmermaid, or ../frankenmermaid/Cargo.toml" >&2
  exit 127
fi

mkdir -p -- "$ART"

"${FM_CMD[@]}" render "$SOURCE_MMD" \
  --format svg \
  --config "$CONFIG_TOML" \
  --no-embed-source-spans \
  --output "$GENERATED_SVG" \
  --json >"$RENDER_JSON"

if ! cmp -s "$EXPECTED_SVG" "$GENERATED_SVG"; then
  printf '%s\n' "verify-showcase-mermaid: generated SVG differs from $EXPECTED_SVG" >&2
  printf '%s\n' "verify-showcase-mermaid: generated file: $GENERATED_SVG" >&2
  { cmp -l "$EXPECTED_SVG" "$GENERATED_SVG" | sed -n '1,20p' >&2; } || true
  exit 1
fi

printf '%s\n' "verify-showcase-mermaid: ok"
printf '%s\n' "verify-showcase-mermaid: artifact directory: $ART"
