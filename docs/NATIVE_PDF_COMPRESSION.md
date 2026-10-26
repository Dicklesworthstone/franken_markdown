# Explicit native PDF compression

PDF compression runs synchronously on the calling thread by default. Large inputs
and machine CPU count do not change that policy. Native applications may enable
the dependency-free `native-compression` Cargo feature and explicitly choose a
per-render worker budget. The same compression primitive and page order are used
in both paths.

## Opt in from a native host

Enable the feature in the host's dependency on this checkout:

```toml
[dependencies]
franken_markdown = { path = "../franken_markdown", default-features = false, features = ["native-compression"] }
```

Then pass a policy with the existing writer options:

```rust
use franken_markdown::{
    PdfCompression, PdfEmitOptions, PdfOptions, render_pdf_emitted,
};

fn publish(markdown: &str) -> franken_markdown::Result<Vec<u8>> {
    render_pdf_emitted(
        markdown,
        &PdfOptions::default(),
        PdfEmitOptions {
            compression: PdfCompression::NativeWorkers { max_workers: 2 },
            ..PdfEmitOptions::default()
        },
    )
}
```

`PdfCompression::Sequential` is the default for every `PdfEmitOptions` constructor.
Enabling the Cargo feature alone starts no workers. Existing CLI and native batch
entry points use this sequential policy; this API adds no CLI flag or browser
worker setting. Batch parallelism therefore remains the native batch scheduler's
explicit job budget, without an automatic inner compression pool.

| Request | Behavior |
|---|---|
| `Sequential` | Compress on the calling thread on native and WASM targets. |
| `NativeWorkers { max_workers: 1..=4 }`, feature enabled, native target | Allow at most that many additional compressor workers for this render. |
| Zero or more than four workers | `RenderError::InvalidInput`, with `pdf_compression:`. |
| Native workers without the feature, or on WASM | `RenderError::InvalidInput`, with `pdf_compression_unavailable:`. |

The budget is an upper bound. One worker, small inputs, and requests with a retained
byte ceiling use the calling thread. Parallel work is admitted only after at least
256 KiB of actual uncompressed page streams are available. Chunked emission reaches
that threshold while drawing pages; monolithic emission knows the total up front.
No environment variable or host CPU-count query chooses a worker count.

## Bounds and lifetime

Each chunked pool permits at most one outstanding page per successfully started
worker, including queued, compressing and completed-but-not-yet-drained pages. It
collects results in page order. Each worker also owns compression scratch; the page
count bound is not a process RSS limit. Monolithic emission continues to retain all
uncompressed pages by design.

A host running `M` independent renders with budget `N` can request up to `M × N`
additional compressor workers. The host should include that product in its own
execution budget. The library neither discovers nor changes the host's scheduler.
The default policy adds zero compressor workers even when the feature is compiled.

All started workers are joined before their rendering scope finishes, including
errors and early cleanup. Thread creation uses fallible builders. Resource failure
can reduce the chunked pool or fall back to calling-thread compression; a failed
monolithic chunk spawn compresses that chunk inline. A worker panic that unwinds
becomes `RenderError::PdfGeneration`, and remaining workers are joined. Builds using
`panic = "abort"` retain their normal process-abort behavior; this is not recovery
from an allocator abort or operating-system process termination.

## Retained-byte ceilings

Setting `PdfEmitOptions::max_retained_bytes` forces sequential compression even when
native workers were requested. Budget and feature admission still run first. This
preserves the existing writer-owned retained-byte accounting instead of excluding
in-flight worker pages from a claimed bound. Successful output bytes and
`pdf_heap_ceiling:` failures follow the same policy as an explicit sequential
request. The existing retained-byte option is not a whole-process memory limit.

## Verification

The [integration suite](../tests/pdf_compression_policy_test.rs) follows actual PDF
page references, decodes their streams and
recovers painted text through each font's ToUnicode map. Its large code fixture
asserts that decoded streams exceed 256 KiB, then compares complete output bytes,
page order and text for worker budgets 1, 2 and 4 in both emission modes. It also
covers unavailable/invalid policies, retained-byte boundary admission and nested
independent host jobs. [Scheduler unit tests](../src/pdf/native_compression.rs)
cover reduced or denied spawns, bounded
in-flight work, a lost page during an unwinding panic, and joined cleanup.

Run the policies separately so unsupported requests remain covered:

```bash
cargo test --no-default-features --test pdf_compression_policy_test
cargo test --no-default-features --features native-compression --test pdf_compression_policy_test -- --test-threads=1
cargo test --no-default-features --features native-compression --lib native_compression -- --test-threads=1
```

These checks establish behavior when executed; they are not a throughput claim.
