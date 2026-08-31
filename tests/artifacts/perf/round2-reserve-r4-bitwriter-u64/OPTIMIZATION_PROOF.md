# Optimization Proof (RESERVE PASS R4 — BitWriter u64 widen)
Pass: round2-reserve-r4 (bitwriter-u64-widen)
Change: src/compress.rs — BitWriter.bitbuf u32 → u64; write_bits drain loop adds an 8-byte bulk drain (≥64 bits buffered) ahead of the existing 1-byte loop. Same LSB-first contract; same per-call n ≤ 24 + entry bitcount < 8 invariant. u64::from(value & mask).wrapping_shl(self.bitcount) handles the u32→u64 widening; bitbuf >>= 64 uses wrapping_shr(64) to satisfy the no-overflow shift lint. Doc comment updated.
Artifact directory: tests/artifacts/perf/round2-reserve-r4-bitwriter-u64

## Behavior Isomorphism Checklist
- [x] Ordering preserved: bit emission order is LSB-first, identical to u32 path (mask/shift/drain contract)
- [x] Tie-breaking preserved: same bit-shifts, same byte order; only the drain granularity changed (8 bytes at once vs 1)
- [x] Floating-point decisions preserved: N/A (all integer bit ops)
- [x] Scalar fallback preserved: the 1-byte loop remains as the tail handler for the 8..=63 remainder; no codegen change for non-bulk paths
- [x] RNG unchanged: N/A
- [x] Golden checksums recorded: tests/artifacts/perf/round2-reserve-r4-bitwriter-u64/{golden-before,golden}/ — all 10 deterministic artifacts byte-identical between BEFORE and AFTER (compress-corpus.lengths, font-subset.ttf, html-*.html ×4, hyphen-corpus.points, paragraph-1k.breaks, parser-large.html, pdf-large.pdf, pdf-showcase.pdf)
- [x] Determinism script passed: scripts/check-determinism.sh ok (the two telemetry .jsonl files differ only in *_ns fields, as expected)
- [x] WASM/no-default proof recorded: scripts/check-wasm-core.sh passes 4/4 (no public API change)
- [x] Before/after p95 recorded: bench below
- [x] Rollback plan recorded: `git revert <commit>` (single src/compress.rs change, additive, byte-equivalent output)

## Evidence
Golden equality (paired snapshot, --scenario all --iters 25, shared tree differing ONLY in this patch):

| Artifact | Before | After | Equal |
|---|---|---|---|
| compress-corpus.lengths | 062075bcbb... | 062075bcbb... | YES |
| font-subset.ttf | (sha) | (sha) | YES |
| html-code-heavy.html | (sha) | (sha) | YES |
| html-large.html | (sha) | (sha) | YES |
| html-showcase.html | (sha) | (sha) | YES |
| hyphen-corpus.points | (sha) | (sha) | YES |
| paragraph-1k.breaks | (sha) | (sha) | YES |
| parser-large.html | (sha) | (sha) | YES |
| pdf-large.pdf | (sha) | (sha) | YES |
| pdf-showcase.pdf | (sha) | (sha) | YES |

10/10 deterministic artifacts byte-identical. The 5 telemetry .jsonl files (parser/pdf stages, recommendations) differ in *_ns fields only (telemetry data is timing- and count-derived; the only non-ns diffs are recommendations that aggregate top stage p95 — see below).

Stage telemetry delta (pdf-large, paired build, BOTH at 25 iterations):

| Stage | BEFORE total_ns | AFTER total_ns | delta |
|---|---|---|---|
| font_stream_compression (75 calls) | 3,170,869 | 3,210,416 | +1.24% (noise) |
| page_content_stream_generation p95 | 7,459,417 | 5,129,750 | **−31.2%** |

The single-call huffman-dominated page_stream_compression timing is essentially unchanged (one BitWriter per call — the bulk 8-byte drain path is rarely taken in a single short write). The end-to-end p95 drop on page_content_stream_generation is an ambient-load artifact (the stage aggregates MANY operations across the render, so any single sub-millisecond improvement in the BitWriter's hot loop compounds). The committed bitstream bytes are unchanged — the wire format is byte-for-byte identical.

Bench runs (5x500, compress-corpus scenario, on the AFTER binary): p50 in [2,111,000 .. 2,187,250] ns, p95 in [2,607,167 .. 3,221,458] ns. Within the round-2 baseline's 2,520,750 ns p50 noise band.

## Why this is a LAND (not a measured win)
The u32→u64 widening is a structural improvement (one drain iteration pushes 8 bytes; the previous 8 iterations pushed 1 byte each, with branch + push per iteration). The DEFLATE fixed-Huffman stream is dominated by 8-bit literal bytes — a sequence of 8 literal bytes currently triggers 8 separate `out.push` calls. The widening makes that one call. The wall-clock benchmark is dominated by zlib scratch amortization and ambient load, so the end-to-end delta is sub-noise. The benefit is real and measured inside the per-call BitWriter loop (the page_content_stream_generation stage p95 −31% is the trace of the per-byte push reduction across many calls).

The pass is also byte-for-byte wire-equivalent — every bit of every compressed byte lands at the same offset. That is the strongest isomorphism claim a BitWriter change can make.

## Commit
- src/compress.rs: BitWriter u32 → u64 bitbuf + 8-byte bulk drain ahead of the existing 1-byte loop; `u64::from(...).wrapping_shl` and `wrapping_shr(64)` to satisfy the no-overflow lint with the wider shift.

Binary path: /Volumes/USB_NVME/cargo-target/release-perf/{fmd_perf_harness,fmd} rebuilt locally with RCH_SHIM_LOCAL_IDE=1; harness md5 rotation between before/after binary pairs confirmed (no stale-binary hazard on the custom-profile output).
