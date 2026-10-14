# Selected-chapter reader browser regression

From the repository root, with Python Playwright and an installed Chromium:

```sh
python wasm/book_chapter_browser.py --chromium /usr/bin/chromium \
  --output /tmp/fmd-chapter-browser
```

Omit `--chromium` to use Playwright's already-installed default browser. This
runner never downloads a browser or loads a generated WASM substitute silently.
It writes a JSON result and a screenshot of explicit fixture content, not a
rendering golden. Failures preserve the completed-check list and diagnostics.

The harness uses the real workbench HTML, production preview controller,
collection/resource stores, source-capture helper, worker protocol, retained
session facade, preview decoder and isolated iframe. Only static relative module
URLs are rewritten into an offline module graph. No production function bodies
are rewritten. A test-only native-ABI/editor fixture supplies recognizable HTML;
there is no JavaScript Markdown parser or claim of Rust-renderer fidelity.

The authoring environment's managed Chromium rejects HTTP pages and direct
module-worker startup from its available offline context. The harness therefore
uses a **test-only classic-worker bootstrap** that imports the same ESM modules
and buffers startup messages. Rendering still executes in a real separate Worker,
and cancellation is checked after the fixture logs entry into synchronous work.
This is not proof of the normal package's module-worker entrypoint or startup.

Checks cover selected rendering, source-only transfers, retained-session reuse,
chapter links and fragment focus, stale editor state, resource revocation,
suspension, source edits, and cancellation/recovery. It verifies opaque-origin
isolation and removal of active markup, plus explicit CSP violation events for
unauthorized styles, images and inline scripts. Those events distinguish CSP
behavior from ambient network blocking. Embedded PNG loading is checked too.
All observed HTTP attempts are recorded and aborted; none is expected.

The authoring run passed all 28 browser checks with zero page errors and zero
observed HTTP requests. The related Node run passed 41 reader/retained tests.
Neither result validates compiled Rust/WASM, font shaping, native performance,
PDF/EPUB export fidelity, the full package build or the normal HTTP-hosted startup.
Keep the repository's native/WASM/package gates as separate acceptance checks.
