# Alien Artifact Performance Design

## Objective
Minimize render latency and PDF byte size while preserving deterministic output, WASM portability, zero-dependency core policy, and scalar-correctness proofs.

## Selected Families
- Certified rewrite pipelines: scalar implementation remains the specification for SIMD and serialization rewrites.
- Compositional latency algebra: `T_total <= T_parse + T_highlight + T_shape + T_linebreak + T_paginate + T_pdf`.
- Queueing theory: native batch worker count is sized by utilization, service variance, and deterministic receipt ordering.
- Convex/resource allocation: only compile to small policy tables; no solver dependency in the render core.

## Proof Obligations
- Golden checksum preservation for every optimization.
- Tie-break and ordering preservation for line-breaking optimizations.
- Scalar/SIMD differential equivalence before enabling accelerated scanners.
- WASM/no-default build remains green.
- Perf delta must clear the same-host variance envelope.

## Galaxy-Brain Cards
### Queueing
Equation: `rho = lambda / (c * mu)`. Substitution will use measured batch throughput and service time once Asupersync batch exists. Intuition: tails explode as rho approaches 1. Change decision if service-time CV exceeds 1.5.

### Latency Composition
Equation: `T_total <= sum(stage_p95) + coupling_margin`. Substitution comes from `BASELINE.md`. Intuition: optimize the largest certified stage first. Change decision if profiling shows file I/O or process startup dominates.

### SIMD Gate
Equation: `EV = impact * confidence / effort`. SIMD proceeds only when scanner p95 is top-5 and EV >= 2.0 after scalar baselines. Change decision if AVX2/NEON differential tests fail or gains stay within noise.
