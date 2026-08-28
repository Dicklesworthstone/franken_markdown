# RSS vs page-count corpus

Committed generator contract for bead
`br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-u9jt.1`.

The harness (`examples/fmd_rss_profile.rs`) synthesizes N page-sized Markdown
sections from the flags recorded in each artifact:

```text
--pages N --lines 48 --width 72
```

Each section is one ATX heading plus a single paragraph of repeated
documentation vocabulary sized to fill roughly one Letter body area. Do not
check in the 1k/5k/10k generated blobs; regenerate from these flags.

`seed-three-pages.md` is a tiny checked-in sample of the same shape.
