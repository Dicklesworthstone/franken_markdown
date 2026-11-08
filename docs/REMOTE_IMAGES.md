# Remote images in portable publications

`--embed-remote-images` downloads Markdown image destinations through the
native shell and supplies the resulting bytes to the existing HTML, PDF, SVG
and EPUB renderers. It makes remote figures portable without adding network
access to the rendering library.

```bash
fmd report.md --embed-remote-images --out report.html
fmd report.md --to both --embed-remote-images --out report.html
fmd report.md --to epub --embed-remote-images --out report.epub
fmd batch notes/ --to epub --embed-remote-images --out-dir publications
fmd book chapters/ --to both --embed-remote-images --out-dir publication
fmd watch report.md --embed-remote-images --out report.html
```

## Network policy

| Surface | Default | With `--embed-remote-images` |
|---|---|---|
| Native HTML / interactive HTML | Retain remote image URLs; do not download | Embed supported image bytes as data URLs |
| Native EPUB | Do not download; unresolved images become accessible placeholders | Package supported image bytes inside the EPUB |
| Native PDF / SVG | Download remote images | Same existing download policy |
| Native combined HTML/PDF | Download for PDF; HTML retains remote URLs | Resolve once and share the same bytes in both outputs |
| Native book HTML / PDF / EPUB | Do not download | Download each distinct URL once for all chapters and outputs |
| Public Rust / WASM APIs, including the batch library API | Caller supplies bytes; no downloading | No network flag or implicit downloader in the core |

The new flag conflicts with `--no-remote-images`. To disable downloads, omit
`--embed-remote-images` and supply `--no-remote-images`. Explicit `--pdf-image`
mappings remain available in either mode.

## Resolution and reuse

Only literal `http://` and `https://` image destinations authorize downloads.
Ordinary hyperlinks, code fences, external stylesheets, and resource references
inside an SVG image do not become fetch requests. Supported raw HTML image
elements and images in includes, headings, tables, lists and footnotes use the
same source-aware image traversal as ordinary PDF exports.

Explicit `--pdf-image 'URL=PATH'` mappings take precedence, including mappings
whose bytes cannot be decoded. The downloader never replaces a supplied
binding with remote content. URL query strings and filenames containing `=`
use the existing native mapping parser. Running header/footer logos still
require explicit image mappings; a remote-looking logo key alone does not
authorize a fetch.

Each destination is attempted once per document, including failures. Combined
output shares its resolved local and remote asset bytes. Batch jobs each own
their document snapshot; there is no global cross-document cache. Book exports
deduplicate URLs across all chapters and pass one resource inventory to every
output. EPUB's existing resource packager also deduplicates identical image
payloads across chapters.

Watch fetches a fresh snapshot on each source/dependency-triggered rebuild.
It does not poll remote URLs for changes. Fixed source, options and image bytes
retain deterministic renderer output; changing a remote response can change a
later export.

## Limits and failures

Downloads reuse the native curl/wget adapter and its existing explicit
transport policy. Personal downloader config files are disabled; proxy and
certificate environment settings still apply. No additional HTTP library or
runtime dependency is introduced.

- `--max-pdf-image-bytes` caps each downloaded response, including a response
  without a `Content-Length` header. The default is 32 MiB.
- `--remote-image-timeout-secs` sets the complete per-image wall-clock deadline,
  defaulting to 20 seconds. Zero selects the one-second minimum. On book
  commands, specifying this option requires `--embed-remote-images`.
- Books retain their 4096-resource and 128 MiB aggregate image limits. Their
  per-image cap never exceeds 32 MiB.
- Batch cancellation interrupts a pending download, reaps its child process,
  and skips the affected job before replacing any of its outputs. Cancellation
  does not become an ordinary missing-image warning.

A failed request produces `remote_image_fetch_failed` and retains the format's
normal unresolved-image behavior. HTML keeps the safe source URL and alt text;
EPUB writes an accessible unavailable-image placeholder; PDF/SVG retain their
existing visible fallback. Thus the flag is best-effort embedding: inspect
warnings or provide explicit image files when every figure must be present.
Book receipts replace preliminary missing-image notices with the actual fetch
result, so successfully embedded figures are not reported as missing.

The opt-in does not inline arbitrary CSS imports, recursively fetch resources
inside SVGs, or change the allowlisted PNG/JPEG/GIF/SVG image repertoire.
