# Hypothesis Ledger

- dominant_inprocess_cost: supports - `pdf-large` has highest p95 in `inprocess.jsonl`.
- process_startup_dominates_small_files: supports - prior CLI timings are sub-5ms for tiny showcase HTML/PDF, so in-process benches are required.
- SIMD_first: rejects - no scanner scenario is promoted until parser/HTML escaping appears top-5 under in-process evidence.
- page_builder_parallelism_first: rejects - page building has correctness coupling; parallelize file/paragraph/font-face work first.
