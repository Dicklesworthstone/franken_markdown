# DEFINE — native batch throughput (zmd.1.5)

Scenarios: html-100, pdf-100 (6 showcase files), mixed-both (README +
showcase + generated), budget-cap1 (--workers 1), and an unwritable-output
negative case. Each timed scenario runs 2 iterations; the primary metric is
p95 batch wall-clock.

Queueing view: mu = (inputs/workers)/p50_s (per-worker service rate),
lambda = inputs/p50_s (achieved throughput), c = workers. For a CLOSED batch all
jobs are offered at t=0, so rho = lambda/(c*mu) is definitionally ~1.0 (workers
saturated — the healthy state); the worker-budget policy's rho<=0.70/0.85 targets
apply to the OPEN/watch arrival model, not this script. The transferable capacity
number is mu; the meaningful scaling number is `parallel_speedup_pdf` in
summary.json (serial budget-cap1 p50 / parallel pdf-100 p50; ~workers is ideal).

Determinism is enforced by comparing two receipts byte-for-byte (out-dir path
normalized). Cancellation/budget refusal accounting is covered by the
deterministic lab test
`render_batch_cancelled_at_boundary_skips_all_and_leaks_no_output` (zmd.1.4).
