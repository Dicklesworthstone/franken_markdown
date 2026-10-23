// Real Chromium custom elements, shadow DOM, mutation delivery, AbortSignal,
// event dispatch and native data-URL fetches exercise the production component.
// The runner replaces only its imported createRenderer boundary. Controllable
// promises and opaque output fixtures force adversarial completion/failure
// order; they are not a Markdown engine or a render-quality/parity oracle.
import { registerFmdView } from "../fmd-view.js";

const reports = [];
const assert = (value, message = "assertion failed") => {
  if (!value) throw new Error(message);
};
const equal = (actual, expected) => assert(JSON.stringify(actual) === JSON.stringify(expected),
  `${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
const turn = () => new Promise(resolve => setTimeout(resolve, 0));
async function until(condition, message = "condition did not settle") {
  for (let i = 0; i < 100; i++) {
    if (condition()) return;
    await turn();
  }
  throw new Error(message);
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const unhandled = [];
addEventListener("unhandledrejection", event => { unhandled.push(String(event.reason)); });
const nativeFetch = globalThis.fetch;
const initializations = [], renders = [];
let renderFailure = null;
const renderer = {
  renderHtml(markdown, options) {
    if (renderFailure) {
      const error = renderFailure;
      renderFailure = null;
      throw error;
    }
    const call = { ...deferred(), markdown, options };
    renders.push(call);
    return call.promise;
  },
};
globalThis.fmdViewTestBoundary = {
  createRenderer(input) {
    const call = { ...deferred(), input };
    initializations.push(call);
    return call.promise;
  },
};

function output(call, label = call.markdown) {
  const p = document.createElement("p");
  p.textContent = label;
  const html = p.outerHTML;
  return {
    bytes: new TextEncoder().encode(html),
    sourceLength: new TextEncoder().encode(call.markdown).length,
    diagnostics: [{ severity: "warning", start: 0, end: 0, message: "boundary fixture" }],
    text() { return html; },
  };
}
function view(markdown = "", attributes = {}) {
  const element = document.createElement("fmd-view");
  const rendered = [], errors = [];
  element.addEventListener("fmd-rendered", event => rendered.push(event.detail));
  element.addEventListener("fmd-error", event => errors.push(event.detail));
  element.textContent = markdown;
  for (const [name, value] of Object.entries(attributes)) element.setAttribute(name, value);
  return { element, rendered, errors, connect() { document.body.append(element); } };
}
function controlledFetch() {
  const requests = [];
  globalThis.fetch = (url, options) => {
    const request = { ...deferred(), url, signal: options?.signal };
    requests.push(request);
    return request.promise;
  };
  return requests;
}
async function nextRender(action) {
  const count = renders.length;
  action();
  await until(() => renders.length > count, "renderer was not invoked");
  return renders.at(-1);
}
async function publish(f, call, label = call.markdown) {
  const count = f.rendered.length;
  const result = output(call, label);
  call.resolve(result);
  await until(() => f.rendered.length > count, "current result was not published");
  equal(f.element.shadowRoot.querySelector("p")?.textContent, label);
  equal(f.rendered.at(-1), {
    bytes: result.bytes.byteLength,
    sourceLength: result.sourceLength,
    diagnostics: result.diagnostics,
  });
}
async function test(name, run) {
  try {
    await run();
    await turn();
    equal(unhandled, []);
    reports.push({ name, pass: true });
  } catch (error) {
    reports.push({ name, pass: false, error: String(error.stack ?? error) });
  } finally {
    for (const element of document.querySelectorAll("fmd-view")) element.remove();
    globalThis.fetch = nativeFetch;
    renderFailure = null;
    await turn();
  }
}

await test("failed shared initialization retries and disconnected subscribers stay silent", async () => {
  equal(registerFmdView(), "fmd-view");
  const registered = customElements.get("fmd-view");
  registerFmdView();
  assert(customElements.get("fmd-view") === registered);
  const a = view("first"), b = view("second");
  a.element.setAttribute("font", "serif");
  await turn();
  equal(initializations.length, 0);
  a.connect(); b.connect();
  await until(() => initializations.length === 1);
  b.element.remove();
  initializations[0].reject(new Error("engine load interrupted"));
  await until(() => a.errors.length === 1);
  equal(b.errors, []);
  equal(renders.length, 0);
  a.element.setAttribute("dark-mode", "disabled");
  b.connect();
  await until(() => initializations.length === 2);
  initializations[1].resolve(renderer);
  await until(() => renders.length === 2);
  const first = renders.find(call => call.markdown === "first");
  const second = renders.find(call => call.markdown === "second");
  equal(first.options, { font: "serif", darkMode: "disabled" });
  await publish(a, first);
  await publish(b, second);
  equal(a.errors.length, 1);
  equal(b.errors.length, 0);
});

await test("detached attributes are inert and older option snapshots cannot overwrite newer output", async () => {
  const requests = controlledFetch(), count = renders.length;
  const f = view("inline", { src: "before.md", font: "serif" });
  f.element.setAttribute("src", "latest.md");
  f.element.removeAttribute("src");
  f.element.removeAttribute("font");
  await turn();
  equal(requests.length, 0);
  equal(renders.length, count);
  const old = await nextRender(() => f.connect());
  const current = await nextRender(() => {
    f.element.setAttribute("font", "serif");
    f.element.setAttribute("dark-mode", "disabled");
  });
  equal(old.options, { font: "sans", darkMode: "auto" });
  equal(current.options, { font: "serif", darkMode: "disabled" });
  await publish(f, current, "latest options");
  old.resolve(output(old, "old options"));
  await turn();
  equal(f.rendered.length, 1);
  equal(f.errors, []);
  equal(f.element.shadowRoot.querySelector("p").textContent, "latest options");
});

await test("superseding src aborts the old fetch and ignores late success and failure", async () => {
  for (const failure of [false, true]) {
    const requests = controlledFetch(), f = view("fallback", { src: "old.md" });
    f.connect();
    equal(requests.length, 1);
    f.element.setAttribute("src", "new.md");
    equal(requests.length, 2);
    assert(requests[0].signal instanceof AbortSignal && requests[0].signal.aborted);
    assert(!requests[1].signal.aborted);
    const current = await nextRender(() => requests[1].resolve(new Response("new source")));
    await publish(f, current);
    let oldBodyReads = 0;
    if (failure) requests[0].reject(new Error("late transport failure"));
    else requests[0].resolve({ ok: true, text() { oldBodyReads++; return "old source"; } });
    await turn();
    equal(oldBodyReads, 0);
    equal(f.errors, []);
    equal(f.rendered.length, 1);
    equal(f.element.shadowRoot.querySelector("p").textContent, "new source");
    f.element.remove();
  }
});

await test("superseded response bodies cannot reach rendering or surface late errors", async () => {
  for (const failure of [false, true]) {
    const requests = controlledFetch(), body = deferred();
    const f = view("", { src: "body-old.md" });
    let reading = false;
    f.connect();
    requests[0].resolve({ ok: true, text() { reading = true; return body.promise; } });
    await until(() => reading);
    f.element.setAttribute("src", "body-new.md");
    assert(requests[0].signal.aborted);
    const current = await nextRender(() => requests[1].resolve(new Response("new body")));
    await publish(f, current);
    const count = renders.length;
    if (failure) body.reject(new Error("old body failed"));
    else body.resolve("old body");
    await turn();
    equal(renders.length, count);
    equal(f.rendered.length, 1);
    equal(f.errors, []);
    f.element.remove();
  }
});

await test("disconnect aborts fetch, detached edits stay inert, and reconnect uses the current source", async () => {
  const requests = controlledFetch(), f = view("initial", { src: "detached-old.md" });
  f.connect();
  const old = requests[0];
  f.element.remove();
  assert(old.signal.aborted);
  f.element.setAttribute("src", "reconnected.md");
  f.element.textContent = "updated fallback";
  old.reject(new Error("after removal"));
  await turn();
  equal(requests.length, 1);
  equal(f.errors, []);
  equal(f.rendered, []);
  f.connect();
  equal(requests.length, 2);
  equal(requests[1].url, "reconnected.md");
  const current = await nextRender(() => requests[1].resolve(new Response("connected source")));
  await publish(f, current);
  const fallback = await nextRender(() => f.element.removeAttribute("src"));
  equal(fallback.markdown, "updated fallback");
  await publish(f, fallback);
});

await test("disconnect during rendering prevents late DOM resurrection and reconnect publishes a new generation", async () => {
  const f = view("old render");
  const old = await nextRender(() => f.connect());
  const initial = f.element.shadowRoot.innerHTML;
  f.element.remove();
  old.resolve(output(old));
  await turn();
  equal(f.element.shadowRoot.innerHTML, initial);
  equal(f.rendered, []);
  f.element.textContent = "reconnected render";
  const current = await nextRender(() => f.connect());
  await publish(f, current);
});

await test("inline child, character and script-type changes render exact current Markdown", async () => {
  const f = view("  original\n");
  await publish(f, await nextRender(() => f.connect()));
  const changed = await nextRender(() => { f.element.firstChild.data = "  next 😀\n"; });
  equal(changed.markdown, "  next 😀\n");
  await publish(f, changed);
  const script = document.createElement("script");
  script.type = "text/markdown";
  script.textContent = "# Script\n";
  const scripted = await nextRender(() => f.element.append(script));
  equal(scripted.markdown, "# Script\n");
  await publish(f, scripted);
  const count = renders.length;
  f.element.prepend(document.createTextNode("ignored outside script"));
  await turn();
  equal(renders.length, count);
  const edited = await nextRender(() => { script.firstChild.data = "# Edited\n"; });
  equal(edited.markdown, "# Edited\n");
  await publish(f, edited);
  const fallback = await nextRender(() => { script.type = "text/plain"; });
  equal(fallback.markdown, f.element.textContent);
  await publish(f, fallback);
  const removed = await nextRender(() => script.remove());
  equal(removed.markdown, f.element.textContent);
  await publish(f, removed);
});

await test("src remains authoritative over inline mutations until it is removed", async () => {
  const requests = controlledFetch(), f = view("fallback", { src: "remote.md" });
  f.connect();
  f.element.textContent = "changed while fetching";
  await turn();
  equal(requests.length, 1);
  assert(!requests[0].signal.aborted);
  await publish(f, await nextRender(() => requests[0].resolve(new Response("remote source"))));
  const count = renders.length;
  f.element.textContent = "latest inline";
  await turn();
  equal(renders.length, count);
  const inline = await nextRender(() => f.element.setAttribute("src", ""));
  equal(inline.markdown, "latest inline");
  await publish(f, inline);
});

await test("inline source changes fence an old promise before MutationObserver delivery", async () => {
  for (const failure of [false, true]) {
    const f = view("before mutation");
    const old = await nextRender(() => f.connect());
    const count = renders.length;
    if (failure) old.reject(new Error("stale before mutation delivery"));
    else old.resolve(output(old));
    // Promise continuation was queued before this mutation's notification.
    f.element.textContent = "after mutation";
    await until(() => renders.length > count);
    equal(f.rendered, []);
    equal(f.errors, []);
    await publish(f, renders.at(-1));
    f.element.remove();
  }
});

await test("current HTTP and response-body errors emit once and a later src retries", async () => {
  for (const kind of ["http", "body"]) {
    const requests = controlledFetch(), f = view("", { src: "failure.md" });
    f.connect();
    requests[0].resolve(kind === "http" ? new Response("missing", { status: 404 }) : {
      ok: true, text() { return Promise.reject(new Error("body read failed")); },
    });
    await until(() => f.errors.length === 1);
    assert(f.errors[0].error.includes(kind === "http" ? "HTTP 404" : "body read failed"));
    equal(f.rendered, []);
    f.element.setAttribute("src", "retry.md");
    await publish(f, await nextRender(() => requests[1].resolve(new Response("recovered"))));
    equal(f.errors.length, 1);
    f.element.remove();
  }
});

await test("render rejection and output decoding failure retain prior HTML and permit reentrant retry", async () => {
  const f = view("visible");
  await publish(f, await nextRender(() => f.connect()));
  const previous = f.element.shadowRoot.innerHTML;
  const failed = await nextRender(() => { f.element.textContent = "render fails"; });
  const count = renders.length;
  f.element.addEventListener("fmd-error", () => f.element.setAttribute("font", "serif"), { once: true });
  failed.reject(new Error("render failed"));
  await until(() => f.errors.length === 1 && renders.length > count);
  equal(f.element.shadowRoot.innerHTML, previous);
  equal(f.errors[0].error, "Error: render failed");
  const retry = renders.at(-1);
  equal(retry.options.font, "serif");
  await publish(f, retry, "recovered rendering");
  const decoding = await nextRender(() => { f.element.textContent = "decode fails"; });
  const beforeDecode = f.element.shadowRoot.innerHTML;
  decoding.resolve({ ...output(decoding), text() { throw new Error("text decoding failed"); } });
  await until(() => f.errors.length === 2);
  equal(f.errors[1].error, "Error: text decoding failed");
  equal(f.element.shadowRoot.innerHTML, beforeDecode);
  await publish(f, await nextRender(() => { f.element.textContent = "final retry"; }));
});

await test("synchronous rendering failures and obsolete output failures never escape as unhandled work", async () => {
  const f = view("synchronous failure");
  renderFailure = new Error("synchronous render failure");
  f.connect();
  await until(() => f.errors.length === 1);
  equal(f.errors[0].error, "Error: synchronous render failure");
  for (const rejection of [false, true]) {
    const old = await nextRender(() => { f.element.textContent = `old ${rejection}`; });
    const current = await nextRender(() => { f.element.textContent = `current ${rejection}`; });
    await publish(f, current);
    let textReads = 0;
    if (rejection) old.reject(new Error("obsolete render failed"));
    else old.resolve({ ...output(old), text() { textReads++; throw new Error("obsolete text failed"); } });
    await turn();
    equal(textReads, 0);
    equal(f.errors.length, 1);
  }
});

await test("real browser fetch honors cancellation and preserves exact fetched source", async () => {
  const requests = [];
  globalThis.fetch = (url, options) => {
    requests.push({ url, signal: options.signal });
    return nativeFetch(url, options);
  };
  const f = view("", { src: "data:text/plain,obsolete" });
  const latest = "# Native fetch\r\n\r\n中文 😀\n";
  const current = await nextRender(() => {
    f.connect();
    f.element.setAttribute("src", `data:text/plain,${encodeURIComponent(latest)}`);
  });
  equal(requests.length, 2);
  assert(requests[0].signal.aborted);
  equal(current.markdown, latest);
  // HTML parsing itself normalizes CRLF; exact source is asserted above,
  // independently of the opaque renderer fixture's displayed label.
  await publish(f, current, "native fetch completed");
  equal(f.rendered.length, 1);
  equal(f.errors, []);
  equal(initializations.length, 2);
});

document.querySelector("#result").textContent = JSON.stringify({
  passed: reports.filter(report => report.pass).length,
  failed: reports.filter(report => !report.pass).length,
  reports,
}, null, 2);
