// Actual generated WASM + real Chromium, with no downloaded test dependencies
// or substituted bindings. Example:
// node wasm/tests/run_flow_generated.mjs --suite flow_emoji.browser.mjs \
//   --fixture /path/native.json --generated-package /path/pkg --chromium /path/chrome
import { createReadStream } from "node:fs";
import { open, readFile, stat } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const options = {};
const accepted = new Set(["suite", "fixture", "generated-package", "chromium", "mode"]);
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i].replace(/^--/, "");
  if (!process.argv[i].startsWith("--") || !accepted.has(key) || options[key]
      || !process.argv[i + 1]) throw Error("invalid or repeated test runner option");
  options[key] = process.argv[i + 1];
}
if (!/^[A-Za-z0-9_]+\.browser\.mjs$/.test(options.suite ?? "")
    || !options.fixture || !options["generated-package"])
  throw Error("--suite NAME.browser.mjs --fixture PATH --generated-package PATH are required");
if (options.mode && !["enabled", "lean"].includes(options.mode)) throw Error("invalid --mode");
const { chromium } = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : "playwright");
const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
const wasmPath = resolve(options["generated-package"], "franken_markdown_bg.wasm");
const wasmBytes = (await stat(wasmPath)).size;
if (wasmBytes > 128 * 1024 * 1024) throw Error("WASM exceeds the 128 MiB test ingress bound");
const digest = createHash("sha256");
for await (const chunk of createReadStream(wasmPath)) digest.update(chunk);
const wasmSha256 = digest.digest("hex");
const bindings = await readFile(resolve(options["generated-package"], "franken_markdown.js"), "utf8");
const fixtureBytes = await readFile(resolve(options.fixture));
const fixture = JSON.parse(fixtureBytes);
const names = ["flow_session.mjs", "flow_outlines.mjs", "flow-canvas.js", "flow_reading.mjs",
  "flow-reader.js", "flow.js", "flow_asset_batch.mjs", "flow_export.mjs", "franken_markdown.js",
  "pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs", "tests/" + options.suite];
const modules = Object.fromEntries(await Promise.all(names.map(async name =>
  [name, await readFile(new URL("../" + name, import.meta.url), "utf8")])));
const browser = await chromium.launch({ executablePath: options.chromium ?? process.env.FMD_CHROMIUM,
  headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage"] });
const requests = [], errors = [];
let result;
try {
  const page = await browser.newPage({
    userAgent: "franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)",
  });
  page.on("pageerror", error => errors.push(String(error)));
  page.on("request", request => {
    if (/^https?:/.test(request.url())) requests.push(request.url());
  });
  await page.route("**/*", route => route.abort());
  await page.setContent("<!doctype html><title>Generated Flow feature acceptance</title>");
  await page.evaluate(length => {
    globalThis.__fmdFeatureWasm = { bytes: new Uint8Array(length), offset: 0 };
  }, wasmBytes);
  const handle = await open(wasmPath, "r"), streamed = createHash("sha256");
  let offset = 0;
  try {
    const buffer = Buffer.alloc(1024 * 1024);
    for (;;) {
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, null);
      if (!bytesRead) break;
      if (offset + bytesRead > wasmBytes) throw Error("WASM changed length while streaming");
      const chunk = buffer.subarray(0, bytesRead);
      streamed.update(chunk);
      await page.evaluate(({ offset, chunk }) => {
        const owned = globalThis.__fmdFeatureWasm, binary = atob(chunk);
        if (owned.offset !== offset || offset + binary.length > owned.bytes.length)
          throw Error("nonsequential WASM ingress");
        for (let i = 0; i < binary.length; i++) owned.bytes[offset + i] = binary.charCodeAt(i);
        owned.offset += binary.length;
      }, { offset, chunk: chunk.toString("base64") });
      offset += bytesRead;
    }
  } finally { await handle.close(); }
  if (offset !== wasmBytes || streamed.digest("hex") !== wasmSha256)
    throw Error("WASM changed while streaming");
  result = await page.evaluate(async ({ modules, bindings, fixture, suite, mode, wasmBytes }) => {
    const urls = new Map(), pending = new Set();
    const blob = source => URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    const load = name => {
      if (urls.has(name)) return urls.get(name);
      if (pending.has(name)) throw Error("cyclic fixture module dependency");
      pending.add(name);
      let source;
      if (name === "pkg/franken_markdown.js") source = bindings;
      else {
        if (!Object.hasOwn(modules, name)) throw Error("missing actual module: " + name);
        source = modules[name].replace(/(["'])(\.{1,2}\/[^"'\n]+)\1/g, (whole, quote, specifier) => {
          const dependency = new URL(specifier, "https://flow.invalid/" + name).pathname.slice(1);
          return Object.hasOwn(modules, dependency) || dependency === "pkg/franken_markdown.js"
            ? JSON.stringify(load(dependency)) : whole;
        });
      }
      const url = blob(source); urls.set(name, url); pending.delete(name); return url;
    };
    try {
      const runtime = await import(load("flow.js"));
      const owned = globalThis.__fmdFeatureWasm;
      delete globalThis.__fmdFeatureWasm;
      if (!owned || owned.offset !== wasmBytes || owned.bytes.length !== wasmBytes)
        throw Error("incomplete generated WASM ingress");
      await runtime.init(owned.bytes);
      return await (await import(load("tests/" + suite))).run(fixture, runtime, mode);
    } finally { for (const url of urls.values()) URL.revokeObjectURL(url); }
  }, { modules, bindings, fixture, suite: options.suite, mode: options.mode ?? "enabled", wasmBytes });
  result.browser = browser.version();
} finally { await browser.close(); }
result.fixtureSha256 = sha256(fixtureBytes);
result.modules = Object.fromEntries(names.map(name => [name, sha256(modules[name])]));
result.generatedWasm = { wasmBytes, wasmSha256, bindingsSha256: sha256(bindings) };
result.networkRequests = requests;
result.pageErrors = errors;
console.log(JSON.stringify(result, null, 2));
if (result.failed || requests.length || errors.length) process.exitCode = 1;
