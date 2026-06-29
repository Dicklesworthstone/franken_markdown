#!/usr/bin/env bash
#
# perf-compare.sh — compare two perf artifact directories and report material
# changes (bead qw1.8.4).
#
# Reads the shared perf artifact bundle (docs/PERFORMANCE_ARTIFACT_SCHEMA.md)
# from a BEFORE and an AFTER run and reports, per scenario:
#   * p50 / p95 / p99 latency deltas (ns and %),
#   * output-byte delta,
#   * peak-RSS delta when `time.stderr` is present in both runs,
#   * the top-hotspot order and whether it shifted,
#   * a variance-envelope classification (noise / improvement / regression),
#   * and a recommended next target (the new top p95 hotspot).
#
# Missing scenarios are reported (added / removed), never fatal. A same-run
# comparison reports "no material change". With --json, a machine-readable
# summary is printed instead of the human table.
#
# Usage:
#   scripts/perf-compare.sh [--envelope PCT] [--json] BEFORE_DIR AFTER_DIR
#   scripts/perf-compare.sh --self-test
#
# Examples:
#   scripts/perf-compare.sh tests/artifacts/perf/fep.6.2-before tests/artifacts/perf/fep.6.2-after
#   scripts/perf-compare.sh --json before/ after/ > comparison.json

set -euo pipefail

ENVELOPE="3.0"
JSON=0
SELF_TEST=0
ARGS=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --envelope) ENVELOPE="${2:?--envelope needs a percent}"; shift 2 ;;
    --json) JSON=1; shift ;;
    --self-test) SELF_TEST=1; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    --) shift; ARGS+=("$@"); break ;;
    -*) echo "perf-compare: unknown argument '$1' (try --help)" >&2; exit 64 ;;
    *) ARGS+=("$1"); shift ;;
  esac
done

run_compare() { # before_dir after_dir
  ENVELOPE="$ENVELOPE" JSON="$JSON" python3 - "$1" "$2" <<'PY'
import json, os, sys

before_dir, after_dir = sys.argv[1], sys.argv[2]
envelope = float(os.environ.get("ENVELOPE", "3.0"))
as_json = os.environ.get("JSON", "0") == "1"

def load_samples(d):
    """scenario -> perf_sample dict, from inprocess.jsonl (last wins)."""
    out = {}
    path = os.path.join(d, "inprocess.jsonl")
    if os.path.isfile(path):
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if rec.get("type") == "perf_sample" and "scenario" in rec:
                    out[rec["scenario"]] = rec
    return out

def load_rss_kb(d):
    """Peak resident set size in KiB from /usr/bin/time -v output, or None."""
    path = os.path.join(d, "time.stderr")
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            if "Maximum resident set size" in line:
                try:
                    return int(line.rsplit(":", 1)[1].strip())
                except ValueError:
                    return None
    return None

before = load_samples(before_dir)
after = load_samples(after_dir)
rss_before = load_rss_kb(before_dir)
rss_after = load_rss_kb(after_dir)

def pct(b, a):
    if b == 0:
        return 0.0 if a == 0 else float("inf")
    return (a - b) / b * 100.0

def classify(p95_pct):
    # Latency: lower is better. A negative delta beyond the envelope is an
    # improvement; a positive one is a regression; otherwise it is noise.
    if p95_pct <= -envelope:
        return "improvement"
    if p95_pct >= envelope:
        return "regression"
    return "noise"

scenarios = sorted(set(before) | set(after))
rows = []
materials = []
for s in scenarios:
    b, a = before.get(s), after.get(s)
    if b and a:
        p95p = pct(b["p95_ns"], a["p95_ns"])
        cls = classify(p95p)
        row = {
            "scenario": s,
            "status": "compared",
            "classification": cls,
            "p50_before_ns": b["p50_ns"], "p50_after_ns": a["p50_ns"], "p50_pct": pct(b["p50_ns"], a["p50_ns"]),
            "p95_before_ns": b["p95_ns"], "p95_after_ns": a["p95_ns"], "p95_pct": p95p,
            "p99_before_ns": b["p99_ns"], "p99_after_ns": a["p99_ns"], "p99_pct": pct(b["p99_ns"], a["p99_ns"]),
            "output_bytes_before": b.get("output_bytes"), "output_bytes_after": a.get("output_bytes"),
            "output_bytes_delta": (a.get("output_bytes", 0) - b.get("output_bytes", 0)),
        }
        if cls != "noise":
            materials.append(row)
    elif b and not a:
        row = {"scenario": s, "status": "removed", "classification": "n/a",
               "p95_before_ns": b["p95_ns"], "p95_after_ns": None, "p95_pct": None}
    else:
        row = {"scenario": s, "status": "added", "classification": "n/a",
               "p95_before_ns": None, "p95_after_ns": a["p95_ns"], "p95_pct": None}
    rows.append(row)

def top_hotspot(samples):
    if not samples:
        return None
    return max(samples.items(), key=lambda kv: kv[1]["p95_ns"])[0]

before_top = top_hotspot(before)
after_top = top_hotspot(after)
hotspot_shifted = bool(before_top and after_top and before_top != after_top)

regressions = [r for r in materials if r["classification"] == "regression"]
improvements = [r for r in materials if r["classification"] == "improvement"]
no_material_change = not materials

# Recommend re-profiling/optimizing the new top p95 hotspot.
BEAD_HINTS = {
    "render-pdf": "PDF shaping/serialization perf (fep.6.x / pdf-perf-proof.sh)",
    "parse": "parser scanner/allocation perf (qw1.6.x / parser-perf.sh)",
    "line-break": "Knuth-Plass layout perf (qw1.7.x / layout-perf-proof.sh)",
    "hyphenation": "hyphenation cache perf (qw1.7.1 / layout-perf-proof.sh)",
    "render-html": "HTML emitter perf",
    "font-subset": "font subsetter perf",
}
reco = None
if after_top:
    cat = after.get(after_top, {}).get("category", "")
    reco = {
        "scenario": after_top,
        "category": cat,
        "p95_ns": after.get(after_top, {}).get("p95_ns"),
        "suggested_area": BEAD_HINTS.get(cat, "re-profile this scenario"),
        "reason": "highest p95 in the AFTER run" + (" (top hotspot shifted)" if hotspot_shifted else ""),
        "confidence": "high" if (after_top and (not before or before_top == after_top or hotspot_shifted)) else "medium",
    }

rss_delta_kb = (rss_after - rss_before) if (rss_before is not None and rss_after is not None) else None

summary = {
    "type": "perf_comparison",
    "schema_version": "fmd-perf-artifact-v1",
    "before_dir": before_dir,
    "after_dir": after_dir,
    "envelope_pct": envelope,
    "scenarios_compared": sum(1 for r in rows if r["status"] == "compared"),
    "scenarios_added": [r["scenario"] for r in rows if r["status"] == "added"],
    "scenarios_removed": [r["scenario"] for r in rows if r["status"] == "removed"],
    "no_material_change": no_material_change,
    "improvements": [r["scenario"] for r in improvements],
    "regressions": [r["scenario"] for r in regressions],
    "before_top_hotspot": before_top,
    "after_top_hotspot": after_top,
    "hotspot_shifted": hotspot_shifted,
    "rss_before_kb": rss_before,
    "rss_after_kb": rss_after,
    "rss_delta_kb": rss_delta_kb,
    "recommended_next_target": reco,
    "rows": rows,
}

if as_json:
    print(json.dumps(summary, indent=2))
    sys.exit(0)

def fmt_pct(p):
    if p is None:
        return "    —"
    if p == float("inf"):
        return "   +inf"
    return f"{p:+7.1f}%"

def ms(ns):
    return "—" if ns is None else f"{ns/1e6:9.3f}ms"

print(f"# perf-compare: {before_dir}  ->  {after_dir}")
print(f"variance envelope: ±{envelope:.1f}% on p95   (negative % = faster)")
print()
print("| scenario | class | p50 before→after (Δ%) | p95 before→after (Δ%) | p99 Δ% | out bytes Δ |")
print("|---|---|---|---|---|---|")
for r in rows:
    if r["status"] != "compared":
        print(f"| {r['scenario']} | **{r['status']}** | — | "
              f"{ms(r.get('p95_before_ns'))}→{ms(r.get('p95_after_ns'))} | — | — |")
        continue
    print(f"| {r['scenario']} | {r['classification']} | "
          f"{ms(r['p50_before_ns'])}→{ms(r['p50_after_ns'])} ({fmt_pct(r['p50_pct'])}) | "
          f"{ms(r['p95_before_ns'])}→{ms(r['p95_after_ns'])} ({fmt_pct(r['p95_pct'])}) | "
          f"{fmt_pct(r['p99_pct'])} | {r['output_bytes_delta']:+d} |")
print()
print(f"top p95 hotspot: before={before_top}  after={after_top}  "
      f"{'(SHIFTED)' if hotspot_shifted else '(unchanged)'}")
if rss_delta_kb is not None:
    print(f"peak RSS: {rss_before} KiB -> {rss_after} KiB ({rss_delta_kb:+d} KiB)")
if no_material_change:
    print("verdict: no material change (all scenarios within the variance envelope).")
else:
    if improvements:
        print(f"verdict: improvements in {', '.join(r['scenario'] for r in improvements)}")
    if regressions:
        print(f"verdict: REGRESSIONS in {', '.join(r['scenario'] for r in regressions)}")
if reco:
    print(f"recommended next target: {reco['scenario']} ({reco['category']}) — "
          f"{reco['suggested_area']} [{reco['reason']}, confidence={reco['confidence']}]")
PY
}

# ---- self-test: synthetic fixtures prove the classifier deterministically ----
run_self_test() {
  local tmp; tmp="$(mktemp -d)"
  mkdir -p "$tmp/before" "$tmp/after" "$tmp/regress"
  # before: two scenarios
  cat > "$tmp/before/inprocess.jsonl" <<'EOF'
{"type":"perf_sample","scenario":"pdf-large","category":"render-pdf","iterations":10,"input_bytes":100,"output_bytes":500,"min_ns":100,"mean_ns":100,"p50_ns":100000000,"p95_ns":100000000,"p99_ns":100000000,"max_ns":100000000,"notes":"x"}
{"type":"perf_sample","scenario":"parser-large","category":"parse","iterations":1,"input_bytes":100,"output_bytes":200,"min_ns":50,"mean_ns":50,"p50_ns":40000000,"p95_ns":40000000,"p99_ns":40000000,"max_ns":40000000,"notes":"x"}
EOF
  # after: pdf-large halved (improvement); parser-large unchanged; new scenario added
  cat > "$tmp/after/inprocess.jsonl" <<'EOF'
{"type":"perf_sample","scenario":"pdf-large","category":"render-pdf","iterations":10,"input_bytes":100,"output_bytes":500,"min_ns":50,"mean_ns":50,"p50_ns":50000000,"p95_ns":50000000,"p99_ns":50000000,"max_ns":50000000,"notes":"x"}
{"type":"perf_sample","scenario":"parser-large","category":"parse","iterations":1,"input_bytes":100,"output_bytes":200,"min_ns":50,"mean_ns":50,"p50_ns":40000000,"p95_ns":40200000,"p99_ns":40200000,"max_ns":40200000,"notes":"x"}
{"type":"perf_sample","scenario":"new-scn","category":"parse","iterations":1,"input_bytes":1,"output_bytes":1,"min_ns":1,"mean_ns":1,"p50_ns":1,"p95_ns":1,"p99_ns":1,"max_ns":1,"notes":"x"}
EOF
  # regress: pdf-large slower than before
  cat > "$tmp/regress/inprocess.jsonl" <<'EOF'
{"type":"perf_sample","scenario":"pdf-large","category":"render-pdf","iterations":10,"input_bytes":100,"output_bytes":500,"min_ns":120,"mean_ns":120,"p50_ns":120000000,"p95_ns":120000000,"p99_ns":120000000,"max_ns":120000000,"notes":"x"}
EOF

  local fail=0
  local saved_json="$JSON" saved_env="$ENVELOPE"
  JSON=1
  ENVELOPE="3.0"

  # 1. same-run comparison -> no material change.
  if ! run_compare "$tmp/before" "$tmp/before" \
      | python3 -c "import json,sys;sys.exit(0 if json.load(sys.stdin)['no_material_change'] else 1)"; then
    echo "perf-compare self-test: FAIL — same-run comparison reported a material change" >&2; fail=1
  fi

  # 2. improvement detected, regression absent, new scenario reported as added,
  #    and the recommendation points at the AFTER top hotspot.
  if ! run_compare "$tmp/before" "$tmp/after" \
      | python3 -c "import json,sys;d=json.load(sys.stdin);sys.exit(0 if ('pdf-large' in d['improvements'] and not d['regressions'] and 'new-scn' in d['scenarios_added'] and d['recommended_next_target']['scenario']=='pdf-large') else 1)"; then
    echo "perf-compare self-test: FAIL — improvement/added/recommendation detection wrong" >&2; fail=1
  fi

  # 3. regression detected.
  if ! run_compare "$tmp/before" "$tmp/regress" \
      | python3 -c "import json,sys;d=json.load(sys.stdin);sys.exit(0 if 'pdf-large' in d['regressions'] else 1)"; then
    echo "perf-compare self-test: FAIL — regression not detected" >&2; fail=1
  fi

  JSON="$saved_json"
  ENVELOPE="$saved_env"
  rm -rf "$tmp"
  if [ "$fail" -ne 0 ]; then
    echo "perf-compare self-test: FAIL" >&2; exit 1
  fi
  echo "perf-compare self-test: ok — same-run=no-change, improvement+regression+added all classified correctly"
}

if [ "$SELF_TEST" -eq 1 ]; then
  run_self_test
  exit 0
fi

if [ "${#ARGS[@]}" -ne 2 ]; then
  echo "perf-compare: expected BEFORE_DIR and AFTER_DIR (got ${#ARGS[@]}); try --help" >&2
  exit 64
fi
for d in "${ARGS[0]}" "${ARGS[1]}"; do
  if [ ! -d "$d" ]; then
    echo "perf-compare: not a directory: $d" >&2; exit 66
  fi
done
run_compare "${ARGS[0]}" "${ARGS[1]}"
