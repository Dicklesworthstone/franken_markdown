#!/usr/bin/env node
// Source/storage browser gate, independent of generated WASM. Node >=18 and
// Chromium required. Uses an owned CDP pipe: no public debugging listener.
import { spawn } from "node:child_process";
import { mkdtemp, readFile, realpath } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = await realpath(fileURLToPath(new URL("..", import.meta.url)));
const profile = await mkdtemp(path.join(tmpdir(), "fmd-document-browser-"));
const server = createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
    const file = await realpath(path.join(root, pathname));
    if (!file.startsWith(root + path.sep) || !pathname.startsWith("/wasm/"))
      throw new Error("outside test surface");
    response.setHeader(
      "Content-Type",
      file.endsWith(".html") ? "text/html; charset=utf-8" : "text/javascript; charset=utf-8",
    );
    response.end(await readFile(file));
  } catch {
    response.writeHead(404);
    response.end("not found");
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const url = `http://127.0.0.1:${server.address().port}/wasm/tests/flow_document_browser.html`;
const args = [
  "--headless",
  "--disable-dev-shm-usage",
  "--disable-background-networking",
  "--no-first-run",
  "--no-proxy-server",
  "--remote-debugging-pipe",
  `--user-data-dir=${profile}`,
  "about:blank",
];
// Only use this opt-in in an isolated test container that lacks sandbox support.
if (process.env.FMD_BROWSER_NO_SANDBOX === "1") args.unshift("--no-sandbox");
const browser = spawn(process.env.CHROMIUM_PATH ?? "chromium", args, {
  stdio: ["ignore", "ignore", "pipe", "pipe", "pipe"],
});
let next = 0,
  buffer = "",
  diagnostics = "",
  stopped = false;
const pending = new Map(),
  events = new Map();
function stop(error) {
  if (stopped) return;
  stopped = true;
  for (const item of [...pending.values(), ...events.values()]) {
    clearTimeout(item.timer);
    item.reject(error);
  }
  pending.clear();
  events.clear();
}
browser.on("error", stop);
browser.on("exit", () => stop(new Error(`Chromium exited. ${diagnostics}`)));
browser.stderr.on("data", (data) => {
  diagnostics = (diagnostics + data.toString()).slice(-16384);
});
browser.stdio[3].on("error", stop);
browser.stdio[4].setEncoding("utf8");
browser.stdio[4].on("data", (data) => {
  buffer += data.toString();
  let end;
  while ((end = buffer.indexOf("\0")) !== -1) {
    const line = buffer.slice(0, end);
    buffer = buffer.slice(end + 1);
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      stop(new Error("Invalid browser response"));
      return;
    }
    const map = message.id ? pending : events;
    const key = message.id ?? `${message.sessionId}:${message.method}`;
    const item = map.get(key);
    if (item) {
      map.delete(key);
      clearTimeout(item.timer);
      if (message.error) item.reject(new Error(JSON.stringify(message.error)));
      else item.resolve(message.result ?? message.params);
    }
  }
});
function waiting(map, key) {
  return new Promise((resolve, reject) => {
    if (stopped) {
      reject(new Error("Browser is closed"));
      return;
    }
    const timer = setTimeout(() => {
      map.delete(key);
      reject(new Error("Browser check deadline exceeded"));
    }, 20000);
    map.set(key, { resolve, reject, timer });
  });
}
function send(method, params = {}, sessionId) {
  const id = ++next,
    result = waiting(pending, id);
  browser.stdio[3].write(
    JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }) + "\0",
  );
  return result;
}
try {
  await send("Browser.getVersion");
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Page.enable", {}, sessionId);
  const navigate = async () => {
    const loaded = waiting(events, `${sessionId}:Page.loadEventFired`);
    const result = await send("Page.navigate", { url }, sessionId);
    await loaded;
    if (result.errorText)
      throw new Error(`Browser could not reach its loopback test server: ${result.errorText}`);
  };
  const evaluate = async (expression) => {
    const result = await send(
      "Runtime.evaluate",
      { expression, awaitPromise: true, returnByValue: true },
      sessionId,
    );
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  await navigate();
  const result = await evaluate(
    "import('./flow_document_browser.mjs').then(module => module.run())",
  );
  for (const check of result.checks)
    console.log(
      `${check.ok ? "PASS" : "FAIL"}: ${check.name}${check.error ? `\n${check.error}` : ""}`,
    );
  if (result.checks.some((check) => !check.ok)) throw new Error("Browser document checks failed");
  await navigate();
  const restored = await evaluate(
    `import('./flow_document_browser.mjs').then(module => module.resume(${JSON.stringify(result.reloadName)}))`,
  );
  console.log(`PASS: ${restored.name}\n${result.checks.length + 1} real-browser checks passed.`);
} finally {
  stop(new Error("Browser check finished"));
  browser.kill("SIGTERM");
  const killer = setTimeout(() => browser.kill("SIGKILL"), 2000);
  killer.unref();
  browser.once("exit", () => clearTimeout(killer));
  server.close();
}
