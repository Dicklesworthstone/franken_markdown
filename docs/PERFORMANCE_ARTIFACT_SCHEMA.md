# Performance Artifact Schema

Status: v1 contract for optimization beads
Owner: Beads track `qw1.8`
Primary users: PDF, parser, layout, SIMD, and Asupersync batch perf work

## Purpose

Performance work in this project must be comparable across agents, machines,
and optimization waves. This schema defines the minimum artifact bundle and
JSONL record shapes required before a bead may claim a speedup, regression, or
bottleneck shift.

The goal is not benchmark theater. The goal is to make every performance claim
answerable from artifacts alone:

- what was run,
- on what host and toolchain,
- against what git state,
- with which feature flags and build profile,
- what outputs were produced,
- what checksums prove behavior preservation,
- which stage is hot,
- which hypothesis was tested,
- which proof obligations passed or failed,
- what should run next.

## Schema Version

Use this exact identifier in every run-level manifest:

```text
fmd-perf-artifact-v1
```

Any incompatible field rename or semantic change must create a new version.
Adding optional fields is allowed when old readers can ignore them.

## Artifact Directory Contract

A complete run lives under:

```text
tests/artifacts/perf/<run-id>/
```

Required files:

| File | Purpose |
|---|---|
| `README.md` | Human index for the run |
| `SCHEMA.md` | Short stamped copy of the schema/version used |
| `schema_manifest.json` | Machine-readable schema/version/file map |
| `DEFINE.md` | Scenario, metric, budget, scope, and variance envelope |
| `fingerprint.json` | Git, host, OS, toolchain, build profile, and dirty status |
| `inprocess.jsonl` | Primary perf samples from the in-process harness |
| `BASELINE.md` | Sorted p50/p95/p99/max baseline table |
| `hotspot_table.md` | Ranked p95 hotspot interpretation |
| `hypothesis.md` | Hypotheses accepted/rejected by the run |
| `golden_checksums.txt` | SHA-256 ledger for behavior-preservation outputs |

Conditionally required files:

| File | Required when |
|---|---|
| `hyperfine.json` and `hyperfine.txt` | CLI wall-clock scenarios run |
| `time.stderr` | RSS/peak memory probe run |
| `perf-stat.stdout` and `perf-stat.stderr` | Hardware counters attempted |
| `tuning.json` | Any perf/sysctl preflight is recorded |

Future wave-specific scripts may add extra files, but must keep these names for
shared records whenever the corresponding data exists.

## JSONL Rules

JSONL files must be:

- UTF-8,
- one JSON object per line,
- deterministic key names,
- no trailing comma,
- no comments,
- no pretty-printing inside JSONL,
- stable units in field names or `unit`,
- append-only within one run phase.

All time durations in JSONL use integer nanoseconds unless the field name says
otherwise. Byte counts use integer bytes. Status fields use lowercase strings.

## Required Record Types

### `run_start`

Emitted once per run, before scenarios.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `run_start` |
| `schema_version` | string | `fmd-perf-artifact-v1` |
| `run_id` | string | Artifact directory name |
| `created_at_utc` | string | RFC3339 UTC timestamp |
| `git_sha` | string | Full commit SHA |
| `dirty_status` | string | `git status --short --branch` text |
| `command` | string | Command used to start the run |
| `artifact_dir` | string | Run directory |

### `host_fingerprint`

Emitted once per run, or represented by `fingerprint.json`.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `host_fingerprint` |
| `target_triple` | string | Build target |
| `rustc` | string | `rustc -vV` or equivalent |
| `cargo` | string | Cargo version |
| `os` | string | `uname` or platform summary |
| `cpu` | string | CPU model or `lscpu` summary |
| `feature_flags` | string array | Cargo features |
| `build_profile` | string | Example: `release-perf` |

### `build_profile`

Required when the run builds binaries.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `build_profile` |
| `profile` | string | Cargo profile |
| `rustflags` | string | Effective `RUSTFLAGS` |
| `debug` | string | Debug info mode |
| `strip` | boolean | Whether symbols were stripped |
| `frame_pointers` | boolean | Whether frame pointers were forced |

### `scenario_start`

Required before a multi-stage scenario when stage records follow.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `scenario_start` |
| `scenario` | string | Stable scenario id |
| `category` | string | `render-pdf`, `parse`, `batch`, etc. |
| `input_bytes` | number | Input size |
| `iterations` | number | Planned iterations |
| `notes` | string | Human context |

### `stage_summary`

Required for attribution beads such as PDF/parser stage timing.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `stage_summary` |
| `scenario` | string | Stable scenario id |
| `stage` | string | Stable stage id |
| `count` | number | Number of samples or invocations |
| `p50_ns` | number | Median |
| `p95_ns` | number | p95 |
| `p99_ns` | number | p99 |
| `max_ns` | number | Max |
| `unit` | string | Usually `ns` |
| `notes` | string | Interpretation or caveat |

### `perf_sample`

Primary scenario-level sample. Existing `examples/fmd_perf_harness.rs` already
emits this shape.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `perf_sample` |
| `scenario` | string | Stable scenario id |
| `category` | string | Scenario category |
| `iterations` | number | Completed iterations |
| `input_bytes` | number | Input bytes |
| `output_bytes` | number | Output bytes |
| `min_ns` | number | Minimum |
| `mean_ns` | number | Arithmetic mean |
| `p50_ns` | number | Median |
| `p95_ns` | number | p95 |
| `p99_ns` | number | p99 |
| `max_ns` | number | Max |
| `notes` | string | Human context |

### `hardware_counter_summary`

Required when hardware counters are attempted, including unavailable cases.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `hardware_counter_summary` |
| `scenario` | string | Scenario id or `perf-smoke` |
| `available` | boolean | Whether counters ran |
| `counter_set` | string array | Requested counters |
| `stdout_path` | string | Artifact path |
| `stderr_path` | string | Artifact path |
| `restore_status` | string | `restored`, `not_tuned`, or `unknown` |
| `notes` | string | Error or caveat |

### `golden_checksum`

Can be represented by `golden_checksums.txt`; JSONL producers should use:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `golden_checksum` |
| `path` | string | Artifact path relative to `golden/` |
| `sha256` | string | Hex SHA-256 |
| `bytes` | number | File size when cheap to provide |

### `hypothesis_evaluated`

Required for performance closeouts that accept or reject a hypothesis.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `hypothesis_evaluated` |
| `hypothesis` | string | Stable id |
| `result` | string | `supports`, `rejects`, or `inconclusive` |
| `evidence_path` | string | Artifact path |
| `notes` | string | Reasoning |

### `proof_obligation`

Required before closing an optimization bead.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `proof_obligation` |
| `bead_id` | string | Bead being closed |
| `obligation` | string | Stable proof item |
| `status` | string | `pass`, `fail`, or `not_applicable` |
| `evidence_path` | string | Artifact path or command |
| `notes` | string | Caveat or explanation |

### `run_complete`

Emitted once per run.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `run_complete` |
| `run_id` | string | Artifact run id |
| `status` | string | `pass`, `fail`, or `partial` |
| `artifact_dir` | string | Run directory |
| `summary_path` | string | Usually `BASELINE.md` |
| `top_hotspot` | string | Scenario id |
| `notes` | string | Summary |

### `next_target_recommendation`

Required for attribution beads that unblock implementation beads.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `next_target_recommendation` |
| `recommended_bead_id` | string | Next bead |
| `reason` | string | Evidence-based reason |
| `evidence_path` | string | Artifact path |
| `confidence` | string | `low`, `medium`, or `high` |

## Current Gauntlet Mapping

`scripts/perf-gauntlet.sh` now stamps each run with this schema:

| Current artifact | Schema role |
|---|---|
| `schema_manifest.json` | Run-level schema/version/file map |
| `SCHEMA.md` | Human-readable schema stamp |
| `fingerprint.json` | `run_start`, `host_fingerprint`, `build_profile` fields |
| `inprocess.jsonl` | `perf_sample` records |
| `golden/pdf-large-stages.jsonl` | `scenario_start`, `stage_summary`, and `proof_obligation` records for PDF stage attribution |
| `golden/pdf-large-recommendation.jsonl` | `next_target_recommendation` from PDF stage attribution |
| `golden/parser-large-stages.jsonl` | `scenario_start`, `stage_summary`, and `proof_obligation` records for parser stage/allocation attribution |
| `golden/parser-large-spanned-stages.jsonl` | `scenario_start`, `stage_summary`, and `proof_obligation` records for source-span/diagnostic parser attribution |
| `golden/parser-large-recommendation.jsonl` | `next_target_recommendation` from parser stage attribution |
| `golden_checksums.txt` | `golden_checksum` ledger |
| `hypothesis.md` | `hypothesis_evaluated` notes |
| `BASELINE.md` | Human table derived from `perf_sample` records |
| `hotspot_table.md` | Ranked interpretation and top-hotspot evidence |
| `perf-stat.*` plus `tuning.json` | `hardware_counter_summary` evidence |
| `README.md` | `run_complete` human entry point |

Wave-specific scripts should preserve this mapping and add JSONL records for
their extra stage attribution.

## Closeout Rules For Perf Beads

A performance bead close reason must cite:

- artifact directory,
- git SHA or dirty-status note,
- before and after p50/p95/p99 when claiming speedup,
- output bytes,
- relevant golden checksum path,
- variance-envelope judgment,
- exact commands run,
- whether WASM/no-default remained green when core code changed.

If evidence is inconclusive, close the bead only as "hypothesis rejected" or
"measurement added"; do not claim a speedup.

## Examples

Minimal `perf_sample`:

```json
{"type":"perf_sample","scenario":"pdf-large","category":"render-pdf","iterations":10,"input_bytes":160958,"output_bytes":838001,"min_ns":60025862,"mean_ns":60913833,"p50_ns":60937455,"p95_ns":62231099,"p99_ns":62231099,"max_ns":62231099,"notes":"render pre-parsed large mixed Markdown document to PDF"}
```

Minimal `proof_obligation`:

```json
{"type":"proof_obligation","bead_id":"br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.2","obligation":"deterministic_output","status":"pass","evidence_path":"scripts/check-determinism.sh","notes":"HTML and PDF repeated-run bytes matched"}
```

Minimal `next_target_recommendation`:

```json
{"type":"next_target_recommendation","recommended_bead_id":"br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.2","reason":"pdf_object_serialization has the highest p95 stage after attribution","evidence_path":"golden/pdf-large-stages.jsonl","confidence":"high"}
```
