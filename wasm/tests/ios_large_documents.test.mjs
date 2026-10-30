import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import test from "node:test";

// Exercise the actual bundle embedded in the Apple app, including its generated
// WASM glue. This is core/output fidelity proof, not UIKit/WebKit responsiveness
// or qualification of a newly built renderer from the current Rust sources.
const packageRoot = new URL("../../ios/Renderer/", import.meta.url);
const finalSentinel = "FINAL_LARGE_DOCUMENT_SENTINEL_café_漢字_🙂";

function largeDocument(minimumBytes) {
  const chunks = [
    "# Large Apple document\n\n",
    "[stable-reference]: https://example.com/large-document\n\n",
  ];
  let byteLength = Buffer.byteLength(chunks.join(""));
  let sectionCount = 0;
  while (byteLength < minimumBytes) {
    const section = `## Section ${sectionCount} — café 漢字\n\n`
      + "A paragraph with **bold**, *emphasis*, Unicode café 漢字 🙂, and "
      + "a [reference link][stable-reference]. All sections must survive rendering.\n\n"
      + "- A first list item\n- A second list item with `inline_code`\n\n"
      + "| Item | Value |\n|:--|--:|\n| café | 123.45 |\n\n"
      + "```rust\nlet preserved_identifier = 123;\n"
      + "// <script>not executable</script>\n```\n\n";
    chunks.push(section);
    byteLength += Buffer.byteLength(section);
    sectionCount++;
  }
  chunks.push(`${finalSentinel}\n`);
  return { markdown: chunks.join(""), sectionCount };
}

async function renderInWorker(minimumBytes) {
  // A real worker makes the deadline enforceable even if a synchronous WASM
  // export never returns. Node's test timeout alone cannot interrupt that call.
  const worker = new Worker(new URL(import.meta.url), { workerData: { minimumBytes } });
  let timeout;
  try {
    return await new Promise((resolve, reject) => {
      timeout = setTimeout(() => reject(new Error("Bundled iOS renderer exceeded the 20 second regression deadline")), 20_000);
      worker.once("message", resolve);
      worker.once("error", reject);
      worker.once("exit", code => reject(new Error(`Renderer worker exited before returning output (code ${code})`)));
    });
  } finally {
    clearTimeout(timeout);
    await worker.terminate();
  }
}

if (!isMainThread) {
  const { createRenderer } = await import(new URL("franken_markdown.js", packageRoot));
  const wasm = await readFile(new URL("pkg/franken_markdown_bg.wasm", packageRoot));
  const renderer = await createRenderer(wasm);
  const { markdown, sectionCount } = largeDocument(workerData.minimumBytes);
  const start = performance.now();
  const output = await renderer.renderHtml(markdown);
  parentPort.postMessage({
    html: output.text(), outputBytes: output.bytes.byteLength,
    sourceBytes: Buffer.byteLength(markdown), sectionCount,
    elapsedMS: performance.now() - start,
  });
  parentPort.close();
} else {
  for (const minimumBytes of [750 * 1024, 1536 * 1024]) {
    test(`bundled iOS WASM preserves the complete ${minimumBytes / 1024} KiB mixed document`, { timeout: 25_000 }, async t => {
      const result = await renderInWorker(minimumBytes);
      assert.ok(result.sourceBytes >= minimumBytes);
      assert.equal(result.outputBytes, Buffer.byteLength(result.html));
      assert.match(result.html, /^<!doctype html>/i);
      assert.ok(result.html.includes(finalSentinel), "the final Unicode sentinel must not be truncated");
      const sections = [...result.html.matchAll(/Section (\d+) — café 漢字/g)].map(match => Number(match[1]));
      assert.deepEqual(sections, Array.from({ length: result.sectionCount }, (_, index) => index),
        "every uniquely numbered section must appear exactly once and in source order");
      assert.equal([...result.html.matchAll(/<table\b/g)].length, result.sectionCount);
      assert.equal([...result.html.matchAll(/<pre\b/g)].length, result.sectionCount);
      assert.equal([...result.html.matchAll(/href="https:\/\/example\.com\/large-document"/g)].length, result.sectionCount);
      assert.equal([...result.html.matchAll(/&lt;script&gt;not executable&lt;\/script&gt;/g)].length, result.sectionCount,
        "fenced code must remain literal and safely escaped throughout the document");
      assert.ok(!result.html.includes("<script>not executable</script>"));
      t.diagnostic(`Actual bundled WASM on this Node host: ${result.sourceBytes} source bytes, ${result.outputBytes} HTML bytes, ${result.sectionCount} complete sections, ${result.elapsedMS.toFixed(1)} ms core render; not a device/UI benchmark.`);
    });
  }
}
