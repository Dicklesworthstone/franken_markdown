# Scanner-share synthetic corpus

Committed seed documents for bead
`br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-p61q.1`.

The measurement harness (`examples/fmd_scanner_share.rs`) reads these files
plus `examples/showcase.md` and `README.md`, then generates a 1 MiB document
at run time by repeating `prose-heavy.md`. Regeneration flags are recorded in
each artifact's `DEFINE.md` / `inprocess.jsonl`:

```text
--outer 3 --iters <html-inner> --scanner-iters <scanner-inner>
generated-large target_bytes=1048576 seed=tests/fixtures/perf/scanner-share/prose-heavy.md
```

Do not hand-edit generated 1 MiB blobs into git. Re-run the harness.
