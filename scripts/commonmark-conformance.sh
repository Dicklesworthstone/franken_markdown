#!/usr/bin/env bash
# commonmark-conformance.sh — official CommonMark spec conformance harness (mwm.3).
#
# Runs every example in the vendored official suite (tests/fixtures/commonmark/
# spec.json) through fmd, normalizes fmd's styled HTML to a bare form (strips the
# <main> wrapper, heading id= anchors, and tok-* highlight spans), and compares it
# to the spec's expected HTML. Emits a per-example gap ledger + a section summary,
# computes a conformance match-rate, and enforces a committed ratcheted floor so
# the number can only go up.
#
# Each example is one of:
#   pass                  normalized fmd output matches the spec exactly
#   intentional_non_goal  a mismatch in a section fmd intentionally diverges on
#                         (raw HTML is escaped by default — a security policy)
#   known_gap             a real parse/emitter gap to close over time
#
# The match-rate is a LOWER BOUND on parser correctness: a known_gap may be a real
# parse bug OR an emitter-formatting difference (see tests/fixtures/commonmark/
# README.md). The ledger lets you triage per example.
#
# Usage:
#   scripts/commonmark-conformance.sh [run-id]        # run + enforce floor
#   scripts/commonmark-conformance.sh --update-floor  # set floor to current pass count
set -uo pipefail
cd "$(dirname "$0")/.." || exit
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$PWD/target/fmd-checks}"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

UPDATE_FLOOR=0
RUN_ID="local"
case "${1:-}" in
  --update-floor) UPDATE_FLOOR=1 ;;
  "") ;;
  *) RUN_ID="$1" ;;
esac
fmd_validate_run_id "commonmark-conformance" "$RUN_ID"

SPEC="tests/fixtures/commonmark/spec.json"
FLOOR_FILE="tests/fixtures/commonmark/conformance-floor.txt"
ART="tests/artifacts/conformance/${RUN_ID}"
mkdir -p "$ART"
[ -s "$SPEC" ] || { echo "missing $SPEC (vendor it first)"; exit 2; }

if [ -n "${FMD_BIN:-}" ]; then
  BIN="$FMD_BIN"
else
  echo "commonmark-conformance: building fmd (release)"
  cargo build --release --quiet --bin fmd
  BIN="$CARGO_TARGET_DIR/release/fmd"
fi
[ -x "$BIN" ] || { echo "fmd binary not found at $BIN"; exit 2; }

FLOOR=0
[ -s "$FLOOR_FILE" ] && FLOOR="$(tr -dc '0-9' <"$FLOOR_FILE")"

python3 - "$BIN" "$SPEC" "$ART" "$FLOOR" "$UPDATE_FLOOR" "$FLOOR_FILE" <<'PY'
import json, re, subprocess, sys
binp, spec_path, art, floor, update_floor, floor_file = sys.argv[1:7]
floor = int(floor or 0); update_floor = update_floor == "1"
examples = json.load(open(spec_path))

# Sections where fmd intentionally diverges from the spec: raw HTML is escaped by
# default (a documented security policy), so these examples are non-goals, not
# parse bugs. (Pass-through is opt-in via --allow-html and stays conservative.)
INTENTIONAL = {"HTML blocks", "Raw HTML"}

WRAP = re.compile(r'<main class="fmd">\n?(.*?)\n?</main>', re.S)
HID = re.compile(r'(<h[1-6])\s+id="[^"]*"')
TOK_OPEN = re.compile(r'<span class="tok-[^"]*">')

def normalize(full_html):
    m = WRAP.search(full_html)
    body = m.group(1) if m else full_html
    body = HID.sub(r'\1', body)            # drop fmd heading id= anchors
    body = TOK_OPEN.sub('', body)          # drop syntax-highlight span opens...
    body = body.replace('</span>', '')     # ...and their closes (fmd uses spans only for tok-*)
    return body

def render(md):
    # A hang or crash on any single example must count as a non-match (gap), never
    # take down the whole harness.
    try:
        p = subprocess.run([binp, '-', '--out', '-'], input=md.encode(),
                           capture_output=True, timeout=60)
    except Exception:
        return ""
    return p.stdout.decode('utf-8', 'replace')

def eq(a, b):
    # Ignore only trailing-newline count (a wrapper artifact); compare structure exactly.
    return a.rstrip('\n') == b.rstrip('\n')

rows = []
sec = {}  # section -> [pass, total]
npass = n_intentional = n_known_gap = 0
for ex in examples:
    ok = eq(normalize(render(ex['markdown'])), ex['html'])
    s = ex['section']
    if ok:
        status = 'pass'; npass += 1
    elif s in INTENTIONAL:
        status = 'intentional_non_goal'; n_intentional += 1
    else:
        status = 'known_gap'; n_known_gap += 1
    sec.setdefault(s, [0, 0]); sec[s][1] += 1; sec[s][0] += ok
    rows.append((ex['example'], s, status))

total = len(examples)
pct = 100.0 * npass / total if total else 0.0
in_scope = total - n_intentional
pct_scope = 100.0 * npass / in_scope if in_scope else 0.0

with open(f"{art}/ledger.tsv", "w") as f:
    f.write("example\tsection\tstatus\n")
    for ex_id, s, st in rows:
        f.write(f"{ex_id}\t{s}\t{st}\n")
with open(f"{art}/summary.md", "w") as f:
    f.write("# CommonMark 0.31.2 conformance (normalized match)\n\n")
    f.write(f"- **pass: {npass} / {total} ({pct:.1f}%)**\n")
    f.write(f"- in-scope match (excl. intentional non-goals): {npass} / {in_scope} ({pct_scope:.1f}%)\n")
    f.write(f"- intentional_non_goal: {n_intentional} (raw-HTML default-escape policy)\n")
    f.write(f"- known_gap: {n_known_gap}\n\n")
    f.write("| Section | Pass | Total |\n|---|---:|---:|\n")
    for s in sorted(sec):
        f.write(f"| {s} | {sec[s][0]} | {sec[s][1]} |\n")

print(f"commonmark-conformance: pass {npass}/{total} ({pct:.1f}%); "
      f"in-scope {npass}/{in_scope} ({pct_scope:.1f}%); "
      f"intentional_non_goal={n_intentional}; known_gap={n_known_gap}; floor={floor}")
print("section pass-rate (worst first):")
for s in sorted(sec, key=lambda s: sec[s][0] / sec[s][1]):
    tag = " [intentional]" if s in INTENTIONAL else ""
    print(f"  {sec[s][0]:>3}/{sec[s][1]:<3}  {s}{tag}")

if update_floor:
    open(floor_file, "w").write(f"{npass}\n")
    # Refresh the committed, human-reviewable snapshot alongside the floor.
    import shutil
    shutil.copy(f"{art}/summary.md", "tests/fixtures/commonmark/conformance-summary.md")
    print(f"commonmark-conformance: floor updated to {npass}; summary snapshot refreshed")
    sys.exit(0)

if npass < floor:
    print(f"commonmark-conformance: FAILED — pass {npass} < committed floor {floor} (regression).")
    sys.exit(1)

# Drift guard: the `features.commonmark_spec` capability flag must advertise the
# current floor, so the published number can never silently disagree with the
# harness. Check the specific field (not the whole JSON blob, where the digits
# could match incidentally).
caps_raw = subprocess.run([binp, "capabilities", "--json"], capture_output=True).stdout.decode("utf-8", "replace")
try:
    spec_flag = json.loads(caps_raw).get("features", {}).get("commonmark_spec", "")
except Exception:
    spec_flag = ""
if str(floor) not in spec_flag:
    print(f"commonmark-conformance: FAILED — capabilities features.commonmark_spec ({spec_flag!r}) "
          f"does not advertise the floor {floor}; update the flag in src/cli.rs.")
    sys.exit(1)

print(f"commonmark-conformance: ok — pass {npass} >= floor {floor}, capabilities advertises it. "
      f"Ledger: {art}/ledger.tsv")
PY
